//! Reading a streamed model response chunk by chunk.
//!
//! A port of upstream's `ExpressResponseStream` (`response_stream.py`).

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::Arc;

use serde_json::{Map, Value};

use super::fences::{
    ends_in_inline_backticks, is_outer_leading_fence_prefix, possible_fence_start,
    strip_outer_leading_fence, strip_outer_trailing_fence, strip_trailing_fence_close,
};
use super::lexer::{close_tag_at, find_open_tag};
use super::scan::{
    clean_payload, heal_progressive_fragment, possible_open_tag_start, scan_block_from,
    split_complete_statements,
};
use super::{Inner, Part, ResponseError};
use crate::Error;
use crate::compile::session::ScopeSession;
use crate::state::RendererState;

/// What a stream does once it rejects a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AfterRejectedBlock {
    /// The stream ends: the call that rejected the block fails.
    #[default]
    End,
    /// The stream returns a [`Part::Error`] in place of the block, skips the
    /// rest of it, and reads on.
    Continue,
}

/// The text between blocks, emitted as it arrives without the whitespace at
/// its edges (`TextRun`).
#[derive(Debug, Default)]
struct TextRun {
    started: bool,
    held_space: String,
}

impl TextRun {
    fn emit(&mut self, parts: &mut Vec<Part>, text: &str) {
        let lead = if self.started {
            text
        } else {
            text.trim_start()
        };
        let body = lead.trim_end();
        if body.is_empty() {
            if self.started {
                self.held_space.push_str(text);
            }
            return;
        }
        let start = text.len() - lead.len();
        parts.push(Part::Text(format!("{}{body}", self.held_space)));
        self.held_space = text[start + body.len()..].to_owned();
        self.started = true;
    }

    fn reset(&mut self) {
        self.started = false;
        self.held_space.clear();
    }
}

#[derive(Clone, Copy)]
enum Step {
    Ready(usize),
    Finish(usize),
    Unclosed,
}

/// Reads one streamed response.
///
/// Text outside blocks streams as it arrives (a tail that could start a tag
/// or a fence is held back). Inside a block, each statement compiles once it
/// is complete, and an open string of a progressive key heals, so a sentence
/// types in live. Every message returned has been checked against and
/// applied to the stream's [`RendererState`].
///
/// From [`super::ExpressParser::open_stream`].
#[derive(Debug)]
pub struct ResponseStream {
    inner: Arc<Inner>,
    wrapped: bool,
    continues: bool,
    state: RendererState,
    closed: bool,
    held: String,
    text: TextRun,
    after_block: bool,
    inline_backticks: bool,
    /// Whether the text outside blocks read so far ends in whitespace (or
    /// there is none): where a chunk starts, backticks open inline code.
    after_space: bool,
    in_block: bool,
    rejected: bool,
    block: String,
    scan_pos: usize,
    last_prefix: String,
    /// Where in the block the payload `last_prefix` came from started.
    last_prefix_at: Option<usize>,
    /// Whether the block holds a `<` (a bare body may carry tags).
    saw_lt: bool,
    touched: Vec<String>,
    initial: HashSet<String>,
    session: ScopeSession,
}

impl ResponseStream {
    pub(crate) fn new(
        inner: Arc<Inner>,
        wrapped: bool,
        after: AfterRejectedBlock,
        state: RendererState,
    ) -> Self {
        let initial = state.surface_ids();
        let session = ScopeSession::new(&inner.surface_id);
        ResponseStream {
            inner,
            wrapped,
            continues: after == AfterRejectedBlock::Continue,
            state,
            closed: false,
            held: String::new(),
            text: TextRun::default(),
            after_block: false,
            inline_backticks: false,
            after_space: true,
            in_block: !wrapped,
            rejected: false,
            block: String::new(),
            scan_pos: 0,
            last_prefix: String::new(),
            last_prefix_at: None,
            saw_lt: false,
            touched: Vec::new(),
            initial,
            session,
        }
    }

    /// The state the stream's messages are applied to.
    pub fn state(&self) -> &RendererState {
        &self.state
    }

    /// The state, for the next response of a canvas that keeps its surfaces.
    pub fn into_state(self) -> RendererState {
        self.state
    }

    /// Whether the response so far ends inside a block. False once the
    /// stream has ended.
    pub fn is_in_block(&self) -> bool {
        !self.closed && self.in_block
    }

    /// Reads the next chunk and returns the parts it made ready.
    ///
    /// On failure the stream has ended; the error's `partial` holds the
    /// parts the chunk made ready before it failed.
    pub fn push(&mut self, chunk: &str) -> Result<Vec<Part>, ResponseError> {
        if self.closed {
            return Err(ResponseError::new(
                Error::Parse("The response stream is closed.".into()),
                Vec::new(),
            ));
        }
        let mut parts = Vec::new();
        match self.feed(chunk, &mut parts) {
            Ok(()) => Ok(parts),
            Err(e) => Err(self.fail(e, parts)),
        }
    }

    /// Ends the response and returns the parts still held back. Closing a
    /// stream that has ended returns nothing.
    pub fn finish(&mut self) -> Result<Vec<Part>, ResponseError> {
        if self.closed {
            return Ok(Vec::new());
        }
        self.closed = true;
        let mut parts = Vec::new();
        if !self.wrapped || self.in_block {
            let step = if self.wrapped {
                Step::Unclosed
            } else {
                Step::Finish(self.block.len())
            };
            let r = if self.rejected {
                Ok(())
            } else {
                self.read_block(&mut parts, step)
            };
            if self.wrapped {
                self.mark_touched_checked();
            }
            return match r {
                Ok(()) => Ok(parts),
                Err(e) => Err(self.fail(e, parts)),
            };
        }
        if !self.held.is_empty() {
            let held = std::mem::take(&mut self.held);
            let mut rest = held.as_str();
            if self.after_block {
                rest = strip_outer_leading_fence(rest, self.inline_backticks);
                self.inline_backticks = false;
            }
            self.text.emit(&mut parts, strip_trailing_fence_close(rest));
        }
        Ok(parts)
    }

    fn fail(&mut self, error: Error, parts: Vec<Part>) -> ResponseError {
        self.closed = true;
        self.mark_touched_checked();
        let partial = parts
            .into_iter()
            .filter(|p| !matches!(p, Part::Error(_)))
            .collect();
        ResponseError::new(error, partial)
    }

    fn mark_touched_checked(&mut self) {
        let touched = std::mem::take(&mut self.touched);
        self.state.mark_checked(touched.iter().map(String::as_str));
        self.touched = touched;
    }

    fn read_block(&mut self, parts: &mut Vec<Part>, step: Step) -> Result<(), Error> {
        let r = match step {
            Step::Ready(end) => self.emit_ready(parts, end),
            Step::Finish(end) => self.finish_block(parts, end),
            Step::Unclosed => Err(Error::Parse(
                "The response ended inside an <a2ui> block, before its </a2ui> tag.".into(),
            )),
        };
        match r {
            Ok(()) => Ok(()),
            Err(e) if self.continues => {
                parts.push(Part::Error(e));
                self.mark_touched_checked();
                self.rejected = true;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    fn feed(&mut self, chunk: &str, parts: &mut Vec<Part>) -> Result<(), Error> {
        if !self.wrapped {
            self.saw_lt |= chunk.contains('<');
            self.block.push_str(chunk);
            if !self.rejected {
                self.read_block(parts, Step::Ready(self.block.len()))?;
            }
            return Ok(());
        }
        let mut input = std::mem::take(&mut self.held);
        input.push_str(chunk);
        while !input.is_empty() {
            if self.in_block {
                input = self.consume_block_text(parts, input)?;
                continue;
            }
            if self.after_block {
                if is_outer_leading_fence_prefix(&input, self.inline_backticks) {
                    self.held = input;
                    return Ok(());
                }
                self.after_block = false;
                input = strip_outer_leading_fence(&input, self.inline_backticks).to_owned();
                self.inline_backticks = false;
                continue;
            }
            input = self.consume_outside_text(parts, &input);
        }
        Ok(())
    }

    fn consume_block_text(
        &mut self,
        parts: &mut Vec<Part>,
        input: String,
    ) -> Result<String, Error> {
        self.block.push_str(&input);
        let (close, readable, resume) = scan_block_from(&self.block, self.scan_pos);
        self.scan_pos = resume;
        let Some(close) = close else {
            if !self.rejected {
                self.read_block(parts, Step::Ready(readable))?;
            }
            return Ok(String::new());
        };
        let close_end =
            close_tag_at(self.block.as_bytes(), close, self.block.len()).expect("scanned");
        let remaining = self.block[close_end..].to_owned();
        if !self.rejected {
            self.read_block(parts, Step::Finish(close))?;
        }
        self.in_block = false;
        self.after_block = true;
        self.after_space = true;
        self.reset_block();
        Ok(remaining)
    }

    fn consume_outside_text(&mut self, parts: &mut Vec<Part>, input: &str) -> String {
        if let Some((start, end)) = find_open_tag(input.as_bytes(), 0) {
            let before = &input[..start];
            self.inline_backticks = ends_in_inline_backticks(before, self.after_space);
            self.text
                .emit(parts, strip_outer_trailing_fence(before, self.after_space));
            self.after_space = true;
            self.text.reset();
            self.in_block = true;
            self.reset_block();
            return input[end..].to_owned();
        }
        let tag = possible_open_tag_start(input);
        let hold = possible_fence_start(&input[..tag]);
        self.text.emit(parts, &input[..hold]);
        if let Some(c) = input[..hold].chars().next_back() {
            self.after_space = c.is_whitespace();
        }
        self.held = input[hold..].to_owned();
        String::new()
    }

    fn reset_block(&mut self) {
        self.block.clear();
        self.scan_pos = 0;
        self.rejected = false;
        self.last_prefix.clear();
        self.last_prefix_at = None;
        self.saw_lt = false;
        self.touched.clear();
        self.initial = self.state.surface_ids();
        self.session = ScopeSession::new(&self.inner.surface_id);
    }

    /// The block's Express up to `end`, without fences; `None` while a
    /// leading fence is still arriving. A bare body that carries its tags
    /// anyway loses them, as the batch compiler strips them.
    fn payload(&self, end: usize) -> Option<Cow<'_, str>> {
        let payload = clean_payload(&self.block, end)?;
        if !self.wrapped && self.saw_lt {
            return Some(Cow::Owned(crate::compile::strip_sentinel_tags(payload)));
        }
        Some(Cow::Borrowed(payload))
    }

    /// Compiles the statements that completed, and a healed open string.
    fn emit_ready(&mut self, parts: &mut Vec<Part>, end: usize) -> Result<(), Error> {
        let Some(payload) = self.payload(end) else {
            return Ok(());
        };
        if payload.trim().is_empty() {
            return Ok(());
        }
        // Only the text after the statements compiled last is scanned again.
        // A payload that starts where the last one did, in the block that
        // only grows, starts with the same bytes: no need to compare them.
        let offset = match &payload {
            Cow::Borrowed(p) => Some(p.as_ptr() as usize - self.block.as_ptr() as usize),
            Cow::Owned(_) => None,
        };
        let same_start = offset.is_some() && offset == self.last_prefix_at;
        let resume = if (self.last_prefix.ends_with('\n') || self.last_prefix.ends_with(';'))
            && payload.len() >= self.last_prefix.len()
            && (same_start || payload.starts_with(self.last_prefix.as_str()))
        {
            self.last_prefix.len()
        } else {
            0
        };
        let (tail, _) = split_complete_statements(
            &payload[resume..],
            self.inner.compiler.extension().is_some(),
        );
        let complete_len = resume + tail.len();
        let (complete, fragment) = payload.split_at(complete_len);
        let mut messages = Vec::new();
        let compile_prefix = !complete.trim().is_empty()
            && if resume > 0 {
                complete.len() > resume
            } else {
                complete != self.last_prefix.as_str()
            };
        let healed = heal_progressive_fragment(
            fragment,
            self.inner.compiler.catalogs(),
            &self.inner.progressive_keys,
        )
        .map(|healed| {
            if complete.trim().is_empty() {
                healed
            } else {
                format!("{complete}\n{healed}")
            }
        });
        let complete = compile_prefix.then(|| complete.to_owned());
        drop(payload);
        if let Some(complete) = complete {
            messages = self.step(&complete, false, true)?;
            self.last_prefix = complete;
            self.last_prefix_at = offset;
        }
        if let Some(combined) = healed {
            // A healed statement that does not compile yet (a required
            // argument still to come) waits for more text.
            if let Ok(healed) = self.step(&combined, false, false) {
                messages.extend(healed);
            }
        }
        let messages = merge_same_surface_updates(messages);
        if !messages.is_empty() {
            parts.push(Part::A2ui(messages));
        }
        Ok(())
    }

    /// One compile step, applied to the state; both or neither.
    fn step(&mut self, text: &str, is_final: bool, cache: bool) -> Result<Vec<Value>, Error> {
        let undo = self
            .session
            .compile_step(&self.inner.compiler, text, is_final, &self.initial, cache)
            .map_err(Error::from)?;
        let messages = undo.messages.clone();
        let mark = self.state.begin();
        match self.apply(&messages) {
            Ok(()) => {
                self.state.commit(mark);
                Ok(messages)
            }
            Err(e) => {
                self.state.rollback(mark);
                self.session.undo(undo);
                Err(e)
            }
        }
    }

    fn apply(&mut self, messages: &[Value]) -> Result<(), Error> {
        for (surface, id) in std::mem::take(&mut self.session.superseded) {
            self.state.drop_provisional_component(&surface, &id);
        }
        for s in self.state.apply(messages)? {
            if !self.touched.contains(&s) {
                self.touched.push(s);
            }
        }
        Ok(())
    }

    fn finish_block(&mut self, parts: &mut Vec<Part>, end: usize) -> Result<(), Error> {
        let payload = self.payload(end).unwrap_or_default().into_owned();
        let mark = self.state.begin();
        let touched_before = self.touched.len();
        let result = (|| -> Result<Vec<Value>, Error> {
            if !self.session.has_emitted && self.initial.is_empty() {
                let messages = self.inner.compile(&payload, true)?;
                self.apply(&messages)?;
                Ok(messages)
            } else {
                let step = self
                    .session
                    .compile_step(&self.inner.compiler, &payload, true, &self.initial, true)
                    .map_err(Error::from)?;
                self.apply(&step.messages)?;
                Ok(step.messages)
            }
        })()
        .and_then(|messages| {
            let touched = self.touched.clone();
            self.state
                .check_topology(touched.iter().map(String::as_str))?;
            Ok(messages)
        });
        match result {
            Ok(messages) => {
                self.state.commit(mark);
                if !messages.is_empty() {
                    parts.push(Part::A2ui(messages));
                }
                Ok(())
            }
            Err(e) => {
                self.state.rollback(mark);
                self.touched.truncate(touched_before);
                Err(e)
            }
        }
    }
}

/// Folds consecutive component updates of one surface into one message
/// (`_merge_same_surface_updates`).
fn merge_same_surface_updates(messages: Vec<Value>) -> Vec<Value> {
    let mut merged: Vec<Value> = Vec::new();
    for message in messages {
        if let Some(prev) = merged.last_mut() {
            if try_merge(prev, &message) {
                continue;
            }
        }
        merged.push(message);
    }
    merged
}

fn try_merge(prev: &mut Value, message: &Value) -> bool {
    let Some(next) = message.get("updateComponents") else {
        return false;
    };
    let target = &next["surfaceId"];
    let body = if prev
        .get("updateComponents")
        .is_some_and(|b| b["surfaceId"] == *target)
    {
        prev.get_mut("updateComponents")
    } else if prev
        .get("createSurface")
        .is_some_and(|b| b["surfaceId"] == *target && b.get("components").is_some())
    {
        prev.get_mut("createSurface")
    } else {
        return false;
    };
    let Some(Value::Array(list)) = body.and_then(|b| b.get_mut("components")) else {
        return false;
    };
    let mut by_id: Map<String, Value> = Map::new();
    for c in list
        .drain(..)
        .chain(next["components"].as_array().into_iter().flatten().cloned())
    {
        let id = c["id"].as_str().unwrap_or_default().to_owned();
        by_id.insert(id, c);
    }
    list.extend(by_id.into_iter().map(|(_, c)| c));
    true
}
