//! The rules text: the Express output contract every snippet opens with.

/// Rule 8 from v1.0, where an event carries `userMessage`.
const EVENT_RULE_V1_0: &str = r#"8. Action events: represent server-side actions using the Event helper, `Event(name, context?, userMessage=...)`:
   Event("save_deal", {rep: $/form/rep})
   Event("reply", userMessage="Replying now")
   Event("save_deal", {rep: $/form/rep}, userMessage=$/form/summary)
   The context map is optional. Pass `userMessage=` as a keyword argument (a string, `$path` binding, or function call) to set the message shown on the user's behalf when the event fires."#;

/// Rule 8 below v1.0.
const EVENT_RULE_PRE_V1_0: &str = r#"8. Action events: represent server-side actions using the Event helper:
   Event("save_deal", {rep: $/form/rep})"#;

const RULES_PREFIX: &str = r#"# A2UI Express DSL Output Contract

You must output the user interface using A2UI Express.

IMPORTANT: You MUST always surround the entire A2UI Express block with the literal sentinel tags `<a2ui>` and `</a2ui>` (always include both the opening and closing tags; do NOT wrap the tags in backticks or replace the opening tag with a markdown code fence such as ```a2ui).

The host compiler will compile your A2UI Express output into the correct JSON envelopes automatically.

## Grammar Rules

1. Component constructors can be assigned to variables or nested inline inside parent component arguments:
   header = ComponentA(prop1="val1")
   root = ComponentB([header, ComponentC("Click", action=Event("submit"))])

   Keyword arguments (`param=value`) and positional arguments with `_` placeholders are supported.

   Variable names MUST start with a letter or underscore, and only contain letters, digits, and underscores.

2. The interface tree must have a single entry point assigned to the reserved variable 'root'.

3. Primitives:
   - Strings: Quoted with `"` or `"""`. Support for `\n`, `\t`, `\\`, and `\"` escapes.
     Raw Strings: Prefaced by `r` (e.g., `r"..."` or `r"""..."""`), with no escape processing.
   - Numbers: write as integers or decimals, e.g., 42
   - Booleans: write true or false
   - Null values: write null
   - Dates & Times: Values for date-time inputs (e.g. in DateTimeInput) must strictly use RFC 3339 format with a timezone offset (e.g. "2026-03-14T00:00:00Z").

4. Lists: represent as arrays, e.g., [child1, child2].

5. Maps: represent as key-value blocks, e.g., {title: "Overview", child: contentCol}. Map keys are always literal strings (dynamic variable resolution is not supported for keys).

6. Data bindings: prefix absolute paths in the data model with '$', e.g., $/user/firstName.
   Prefix relative list scopes with '$', e.g., $firstName.
   A lone '$' represents an empty relative path which resolves to the root of the current context (e.g. inside a template, representing the entire item itself).

7. Logic and validation: prefix client check rules with '?', e.g., ?required or ?regex("^[0-9]{5}$"). To specify a custom error message for validation failures, append it as an extra string argument, e.g. ?regex("^[0-9]{5}$", "Postal code must be 5 digits")."#;

const RULES_SUFFIX: &str = r#"9. Nested functions: call client functions directly using catalog signatures, for example myFunction("value").

10. Data model population: Assign a value directly to an absolute data path (e.g. $/path/to/key = "value") to populate or initialize values inside the shared dataModel. The value can be a primitive, array, or map.

11. Dynamic list templates: If a component expects a template child list, represent it using the _template helper:
    _template($/path/to/list, itemTemplate)
    And define the template component variable on another line, utilizing relative path references prefixed with $:
    itemTemplate = Image($url)

12. To delete a user interface surface, output the standalone `deleteSurface(surfaceId)` command (no variable assignment):
    deleteSurface("dashboard-surface-1")

13. Static properties: Arguments annotated with '(static)' in the signatures below MUST be defined as literal values or arrays inline. You CANNOT use a dynamic data binding path (prefixed by $) for these arguments.

14. Required actions: Parameters named 'action' (or annotated in component signatures) are strictly required. You must pass a valid Event (e.g. Event("click")) or function call. If no specific action is described in the user request, you must provide a dummy click event like Event("click") instead of passing null or omitting the parameter.

15. Surface targeting: Output `surface(surfaceId)` to specify or target a user interface surface:
    surface("dashboard-surface-1")
    root = ComponentA(...)"#;

/// Rule 12, as the allowlist filter removes it.
pub(super) const DELETE_RULE: &str = "\n\n12. To delete a user interface surface, output the standalone `deleteSurface(surfaceId)` command (no variable assignment):\n    deleteSurface(\"dashboard-surface-1\")";

/// Rule 10, as the allowlist filter removes it.
pub(super) const DATA_RULE: &str = "\n\n10. Data model population: Assign a value directly to an absolute data path (e.g. $/path/to/key = \"value\") to populate or initialize values inside the shared dataModel. The value can be a primitive, array, or map.";

/// Rule 15's example line; its placeholder name is replaced by a component
/// the active catalogs define.
pub(super) const ROOT_EXAMPLE_LINE: &str = "root = ComponentA(...)";

/// Rule 15 without its `root = ...` line, as the allowlist filter removes it.
pub(super) const SURFACE_RULE: &str = "\n\n15. Surface targeting: Output `surface(surfaceId)` to specify or target a user interface surface:\n    surface(\"dashboard-surface-1\")";

/// Where the renderer-call rule is inserted.
pub(super) const STATIC_RULE_HEAD: &str = "\n\n13. Static properties:";

/// The full rules text of a version.
pub(super) fn rules(at_least_v1: bool) -> String {
    let event = if at_least_v1 {
        EVENT_RULE_V1_0
    } else {
        EVENT_RULE_PRE_V1_0
    };
    format!("{RULES_PREFIX}\n\n{event}\n\n{RULES_SUFFIX}")
}

/// Numbers the rules 1, 2, 3, … again after some were removed or inserted.
///
/// A rule is a paragraph that follows a blank line and opens with `<n>. `;
/// its continuation lines are re-indented when the width of the number
/// changes. Ports upstream's
/// `/(?<=\n\n)(\d+\. )([^\n]*(?:\n(?!\n)[^\n]*)*)/g` replacement.
pub(super) fn renumber(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut index = 0usize;
    let mut pos = 0usize;
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let after_blank = i >= 2 && &bytes[i - 2..i] == b"\n\n";
        let digits = bytes[i..].iter().take_while(|b| b.is_ascii_digit()).count();
        if after_blank && digits > 0 && text[i + digits..].starts_with(". ") {
            let old_prefix_len = digits + 2;
            let body_start = i + old_prefix_len;
            let body_end = text[body_start..]
                .find("\n\n")
                .map_or(text.len(), |e| body_start + e);
            index += 1;
            let new_prefix = format!("{index}. ");
            let body = &text[body_start..body_end];
            out.push_str(&text[pos..i]);
            out.push_str(&new_prefix);
            if new_prefix.len() == old_prefix_len {
                out.push_str(body);
            } else {
                let old = format!("\n{}", " ".repeat(old_prefix_len));
                let new = format!("\n{}", " ".repeat(new_prefix.len()));
                out.push_str(&body.replace(&old, &new));
            }
            pos = body_end;
            i = body_end;
            continue;
        }
        i += 1;
    }
    out.push_str(&text[pos..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renumbering_closes_gaps_and_reindents() {
        let text = "head\n\n1. a\n\n3. b\n   cont\n\n10. c\n    more";
        assert_eq!(
            renumber(text),
            "head\n\n1. a\n\n2. b\n   cont\n\n3. c\n   more"
        );
    }

    #[test]
    fn a_number_not_after_a_blank_line_is_not_a_rule() {
        assert_eq!(renumber("x\n5. y\n\n7. z"), "x\n5. y\n\n1. z");
    }
}
