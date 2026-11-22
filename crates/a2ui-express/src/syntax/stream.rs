//! Line-at-a-time parsing for the streaming compiler.

use super::ast::{Pos, Statement, StatementKind, Target};
use super::error::ParseError;
use super::parser::parse_at;
use std::collections::HashSet;

/// What [`LineParser`] hands back.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamItem {
    /// A statement that is complete.
    Statement(Statement),
    /// A statement that failed; it is dropped and parsing goes on.
    Error(ParseError),
}

/// Parses a block fed one complete line at a time.
///
/// A statement may span lines, so a statement is handed out only once the
/// next one has begun (or at [`LineParser::finish`]): until then a line
/// break could still continue it, and upstream's repairs could still join
/// it with the next line (`greeting_` + `label = …`). An error at the end of
/// the buffered text (an unclosed bracket or string) means the statement is
/// unfinished, so it waits for more lines; any other error is reported at
/// once and the bad statement dropped.
///
/// Lines and columns in statements and errors are positions in the whole
/// block. Byte offsets are into the repaired text of each parse and are not
/// comparable across pushes.
#[derive(Debug, Clone, Default)]
pub struct LineParser {
    pending: String,
    origin: Pos,
    known: HashSet<String>,
    started: bool,
    /// Where the last statement handed out starts.
    emitted: Option<(u32, u32)>,
}

impl LineParser {
    /// A parser at the start of a block.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one line, without its line break.
    pub fn push_line(&mut self, line: &str) -> Vec<StreamItem> {
        if !self.started {
            self.started = true;
            self.origin = Pos::START;
        }
        self.pending.push_str(line);
        self.pending.push('\n');
        self.drain(false)
    }

    /// Ends the block: everything still buffered is parsed as final.
    pub fn finish(mut self) -> Vec<StreamItem> {
        self.drain(true)
    }

    fn drain(&mut self, last: bool) -> Vec<StreamItem> {
        let out = parse_at(&self.pending, self.origin, &self.known);
        let mut statements = out.program.statements;
        let mut errors = out.errors;
        // Where buffering resumes: the line of the statement held back.
        let keep_from: Option<u32> = if !last && errors.iter().any(|e| e.at_eof) {
            // Something is unfinished. Statements before the first one that
            // failed are complete, since another has begun after them; the
            // rest waits for more lines.
            let limit = out
                .dropped
                .iter()
                .map(|s| s.start)
                .chain(errors.iter().map(|e| line_start(e.span.start)))
                .min_by_key(|p| (p.line, p.column))
                .expect("an error was reported");
            statements
                .retain(|s| (s.span.start.line, s.span.start.column) < (limit.line, limit.column));
            errors.clear();
            Some(limit.line)
        } else if !last && errors.is_empty() {
            statements.pop().map(|held| held.span.start.line)
        } else {
            None
        };

        let mut items: Vec<StreamItem> = Vec::new();
        let mut errors = errors.into_iter().peekable();
        for s in statements {
            let at = (s.span.start.line, s.span.start.column);
            if self.emitted.is_some_and(|e| at <= e) {
                continue;
            }
            self.emitted = Some(at);
            while let Some(e) = errors.next_if(|e| e.span.start.offset < s.span.start.offset) {
                items.push(StreamItem::Error(e));
            }
            if let StatementKind::Assign {
                target: Target::Variable { name, .. },
                ..
            } = &s.kind
            {
                self.known.insert(name.clone());
            }
            items.push(StreamItem::Statement(s));
        }
        items.extend(errors.map(StreamItem::Error));

        // Keep the text from the start of that line; the repairs keep line
        // breaks, so lines of the buffer and of the repaired text
        // correspond. Statements before it on that line were handed out
        // already and are skipped when the line is parsed again.
        let cut = match keep_from {
            Some(line) => {
                let skip_lines = (line - self.origin.line) as usize;
                if skip_lines == 0 {
                    0
                } else {
                    self.pending
                        .match_indices('\n')
                        .nth(skip_lines - 1)
                        .map_or(self.pending.len(), |(i, _)| i + 1)
                }
            }
            None => self.pending.len(),
        };
        self.origin = end_of(&self.pending[..cut], self.origin);
        self.pending.drain(..cut);
        items
    }
}

/// The start of the line `pos` is on.
fn line_start(pos: Pos) -> Pos {
    Pos {
        line: pos.line,
        column: 0,
        offset: pos.offset,
    }
}

/// The position just past `text`, which starts at `origin`.
fn end_of(text: &str, origin: Pos) -> Pos {
    let mut pos = origin;
    for c in text.chars() {
        if c == '\n' {
            pos.line += 1;
            pos.column = 0;
        } else {
            pos.column += 1;
        }
    }
    pos.offset += text.len();
    pos
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::ast::ExprKind;

    fn names(items: &[StreamItem]) -> Vec<String> {
        items
            .iter()
            .map(|i| match i {
                StreamItem::Statement(s) => match &s.kind {
                    StatementKind::Assign { target, .. } => target.text(),
                    StatementKind::Expr(e) => match &e.kind {
                        ExprKind::Call(c) => format!("{}()", c.name),
                        other => format!("{other:?}"),
                    },
                },
                StreamItem::Error(e) => format!("error@{}", e.line()),
            })
            .collect()
    }

    #[test]
    fn statements_come_out_once_the_next_one_starts() {
        let mut p = LineParser::new();
        assert!(p.push_line("surface(\"main\")").is_empty());
        assert_eq!(names(&p.push_line("root = Column([")), ["surface()"]);
        assert!(p.push_line("  title,").is_empty());
        assert!(p.push_line("])").is_empty());
        assert_eq!(names(&p.push_line("title = Text(\"Hi\")")), ["root"]);
        let rest = p.finish();
        assert_eq!(names(&rest), ["title"]);
        let StreamItem::Statement(s) = &rest[0] else {
            panic!()
        };
        assert_eq!(s.span.start.line, 5);
    }

    #[test]
    fn a_bad_line_is_reported_and_the_rest_still_parses() {
        let mut p = LineParser::new();
        let mut items = p.push_line("a = Text(\"a\")");
        items.extend(p.push_line("b = Text(,)"));
        items.extend(p.push_line("c = Text(\"c\")"));
        items.extend(p.finish());
        assert_eq!(names(&items), ["a", "error@2", "c"]);
    }

    #[test]
    fn a_split_assignment_target_is_joined_across_pushes() {
        let mut p = LineParser::new();
        let mut items = p.push_line("greeting_");
        items.extend(p.push_line("label = Text(\"Hi\")"));
        items.extend(p.push_line("root = Card(greeting_label)"));
        items.extend(p.finish());
        assert_eq!(names(&items), ["greeting_label", "root"]);
    }
}
