//! The parser against the vendored conformance suite and upstream's own
//! parser tests.
//!
//! The expected error texts were taken from upstream's Python SDK (ANTLR
//! 4.13 runtime) at the commit pinned in `vendor/a2ui/UPSTREAM`. Beyond the
//! cases here, the parser was checked against that runtime on ~20,000
//! randomly mutated conformance inputs: the first error matched in all of
//! them.

use std::fs;
use std::path::{Path, PathBuf};

use a2ui_express::syntax::{
    CallKind, ExprKind, NumberValue, ParseErrorKind, StatementForm, StatementKind, parse,
};
use serde_yaml_ng::Value as Yaml;

fn suite(name: &str) -> Vec<Yaml> {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/a2ui/conformance/agent/express")
        .join(name);
    let text = fs::read_to_string(&path).unwrap();
    match serde_yaml_ng::from_str(&text).unwrap() {
        Yaml::Sequence(cases) => cases,
        other => panic!("{other:?}"),
    }
}

fn str_field<'a>(case: &'a Yaml, key: &str) -> Option<&'a str> {
    case.get(key).and_then(Yaml::as_str)
}

fn error_category(case: &Yaml) -> Option<&str> {
    case.get("expect_error")
        .and_then(|e| e.get("category"))
        .and_then(Yaml::as_str)
}

/// The text inside `<a2ui>` … `</a2ui>`, or the whole input.
fn block(input: &str) -> &str {
    match (input.find("<a2ui>"), input.rfind("</a2ui>")) {
        (Some(s), Some(e)) if s < e => &input[s + "<a2ui>".len()..e],
        _ => input,
    }
}

/// The message upstream's compiler raises for each `ParseError` case.
const PARSE_ERRORS: &[(&str, &str)] = &[
    (
        "test_compile_express_syntax_error_is_a_parse_error",
        "Failed to parse expression: Syntax error at line 1:19: mismatched input '<EOF>' expecting {',', ')'}",
    ),
    (
        "test_compile_express_empty_argument_is_a_parse_error",
        "Failed to parse expression: Syntax error at line 1:17: extraneous input ',' expecting {'[', '{', ')', '_', 'null', RAW_TRIPLE_STRING, TRIPLE_STRING, RAW_STRING, STANDARD_STRING, PATH, CHECK, NUMBER, BOOLEAN, IDENTIFIER, SYSTEM_IDENTIFIER}",
    ),
    (
        "test_compile_express_unexpected_character_is_a_parse_error",
        "Syntax error at line 1:7: token recognition error at: '^'",
    ),
    (
        "test_compile_express_template_without_a_component_is_a_parse_error",
        "_template helper requires exactly 2 arguments: path and templateComponent.",
    ),
    (
        "test_compile_express_number_out_of_range_is_a_parse_error",
        "Failed to parse expression: Number literal 1e999 is out of range.",
    ),
    (
        "test_compile_express_exponent_without_digits_is_a_parse_error",
        "Failed to parse expression: Syntax error at line 1:14: extraneous input 'e' expecting {',', ')'}",
    ),
    (
        "test_compile_express_missing_comma_between_defined_variables_is_a_parse_error",
        "Failed to parse expression: Syntax error at line 5:0: extraneous input 'body' expecting {',', ']'}",
    ),
    (
        "test_compile_express_split_identifier_one_half_resolves_not_joined",
        "Failed to parse expression: Syntax error at line 3:0: extraneous input 'suffix' expecting {',', ']'}",
    ),
];

#[test]
fn every_parse_error_case_fails_to_parse_with_upstreams_message() {
    let mut seen = 0;
    for case in suite("compiler.yaml") {
        if error_category(&case) != Some("ParseError") {
            continue;
        }
        let name = str_field(&case, "name").unwrap();
        let input = str_field(&case, "input").unwrap();
        let out = parse(input);
        seen += 1;
        if name == "test_compile_express_empty_block_is_a_parse_error" {
            // Nothing to read: the compiler reports the missing root.
            assert!(out.errors.is_empty() && out.program.statements.is_empty());
            continue;
        }
        let want = PARSE_ERRORS
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("{name}: no expected message"))
            .1;
        let got = out
            .first_error()
            .unwrap_or_else(|| panic!("{name}: parsed without error"));
        assert_eq!(got.upstream_message(), want, "{name}");
    }
    assert_eq!(seen, PARSE_ERRORS.len() + 1);
}

#[test]
fn every_other_compile_input_parses_cleanly() {
    let mut seen = 0;
    for case in suite("compiler.yaml") {
        if error_category(&case) == Some("ParseError") {
            continue;
        }
        let name = str_field(&case, "name").unwrap();
        let out = parse(block(str_field(&case, "input").unwrap()));
        assert!(out.errors.is_empty(), "{name}: {:?}", out.errors);
        assert!(!out.program.statements.is_empty(), "{name}");
        seen += 1;
    }
    assert!(seen > 100);
}

#[test]
fn response_parser_blocks_parse_cleanly() {
    for case in suite("response_parser.yaml") {
        let input = str_field(&case, "input").unwrap_or_default();
        if str_field(&case, "action") != Some("parse_response")
            || input.matches("<a2ui>").count() != 1
        {
            continue;
        }
        let name = str_field(&case, "name").unwrap();
        let out = parse(block(str_field(&case, "input").unwrap()));
        assert!(out.errors.is_empty(), "{name}: {:?}", out.errors);
    }
}

/// Upstream's `visitor.test.ts` error cases: the first error, and the
/// statements before its line.
#[test]
fn upstream_parser_error_cases() {
    type Case = (
        &'static str,
        (u32, u32, &'static str),
        &'static [&'static str],
    );
    let cases: &[Case] = &[
        (
            "a = 1\nb = 2\nc = [broken\nd = 4\ne = 5",
            (4, 0, "mismatched input 'd' expecting {',', ']'}"),
            &["a", "b"],
        ),
        (
            "a = 1\nb = 2\n= broken\nd = 4\ne = 5",
            (
                3,
                0,
                "extraneous input '=' expecting {<EOF>, '[', '{', '_', 'null', RAW_TRIPLE_STRING, TRIPLE_STRING, RAW_STRING, STANDARD_STRING, PATH, CHECK, NUMBER, BOOLEAN, IDENTIFIER, SYSTEM_IDENTIFIER}",
            ),
            &["a", "b"],
        ),
        (
            "valid = 1\n^invalid = 2",
            (2, 0, "token recognition error at: '^'"),
            &["valid"],
        ),
        (
            "ok = 1\n~bad_lexer\n broken_parser [",
            (2, 0, "token recognition error at: '~'"),
            &["ok"],
        ),
    ];
    for (src, (line, col, msg), before) in cases {
        let out = parse(src);
        let e = out.first_error().unwrap();
        assert_eq!(
            (e.line(), e.column(), e.message.as_str()),
            (*line, *col, *msg)
        );
        let kept: Vec<String> = out
            .program
            .statements
            .iter()
            .filter(|s| s.span.start.line < *line)
            .map(|s| match &s.kind {
                StatementKind::Assign { target, .. } => target.text(),
                StatementKind::Expr(_) => "expr".into(),
            })
            .collect();
        assert_eq!(kept, *before, "{src}");
    }
    // The lexer and parser errors of the last case come in that order.
    let out = parse("ok = 1\n~bad_lexer\n broken_parser [");
    let all: Vec<_> = out
        .errors
        .iter()
        .map(|e| (e.kind, e.line(), e.column()))
        .collect();
    assert_eq!(
        all,
        [
            (ParseErrorKind::Lexer, 2, 0),
            (ParseErrorKind::Syntax, 3, 16)
        ]
    );
    assert_eq!(
        out.errors[1].message,
        "mismatched input '<EOF>' expecting {'[', ']', '{', '_', 'null', RAW_TRIPLE_STRING, TRIPLE_STRING, RAW_STRING, STANDARD_STRING, PATH, CHECK, NUMBER, BOOLEAN, IDENTIFIER, SYSTEM_IDENTIFIER}"
    );
}

/// Messages for the other ANTLR error shapes, from upstream's runtime.
#[test]
fn antlr_error_shapes() {
    let cases = [
        ("x = [1,,2]", "1:7: no viable alternative at input ',,'"),
        ("x = [1,", "1:7: no viable alternative at input ','"),
        ("x = {a b}", "1:7: missing ':' at 'b'"),
        (
            "x = {a: 1 b: 2}",
            "1:10: mismatched input 'b' expecting {',', '}'}",
        ),
        ("x = @index 5", "1:11: missing '(' at '5'"),
        (
            "x = Text(\"a\", \"c\" b)",
            "1:18: extraneous input 'b' expecting {',', ')'}",
        ),
        ("x = @ y", "1:4: token recognition error at: '@ '"),
        (
            "x = \"open\nend",
            "1:4: token recognition error at: '\"open\\nend'",
        ),
    ];
    for (src, want) in cases {
        let e = parse(src).first_error().cloned().unwrap();
        assert_eq!(
            format!("{}:{}: {}", e.line(), e.column(), e.message),
            want,
            "{src}"
        );
    }
}

#[test]
fn a_bad_statement_is_dropped_and_the_rest_still_parses() {
    let out = parse("a = Text(\"a\")\nb = Text(,)\nc = Column([\n  x,,\n  y\n])\nd = Text(\"d\")");
    let names: Vec<String> = out
        .program
        .statements
        .iter()
        .map(|s| match &s.kind {
            StatementKind::Assign { target, .. } => target.text(),
            StatementKind::Expr(_) => "expr".into(),
        })
        .collect();
    assert_eq!(names, ["a", "d"]);
    assert_eq!(
        out.errors.iter().map(|e| e.line()).collect::<Vec<_>>(),
        [2, 4]
    );
    assert_eq!(out.dropped.len(), 2);
}

#[test]
fn statements_and_values_have_the_expected_shape() {
    let src = r#"surface("main", catalogId="c")
root = Column([title, Text("Hi", variant="h1"), _template($/items, row)],
  gap = 4)
row = Button(Text(formatString("${/name}")), Event("open", {id: $id}), [?required, ?max(3, "Too long")])
n = Text(@index())
$/form/name = "Ada"
deleteSurface("old")
notify(_, "x")"#;
    let out = parse(src);
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    let st = &out.program.statements;
    assert_eq!(st.len(), 7);
    assert!(matches!(st[0].form(), StatementForm::Surface(c) if c.arg(0, "surfaceId").is_some()));
    assert!(matches!(st[5].form(), StatementForm::DeleteSurface(_)));
    assert!(matches!(st[6].form(), StatementForm::Call(c) if c.name == "notify"));
    match st[4].form() {
        StatementForm::Assign(t, v) => {
            assert_eq!(t.text(), "$/form/name");
            assert_eq!(v.kind, ExprKind::String("Ada".into()));
        }
        other => panic!("{other:?}"),
    }

    // root spans two lines, with positional and keyword arguments mixed.
    assert_eq!((st[1].span.start.line, st[1].span.end.line), (2, 3));
    let StatementForm::Assign(_, root) = st[1].form() else {
        panic!()
    };
    let ExprKind::Call(column) = &root.kind else {
        panic!()
    };
    assert_eq!(column.positional().count(), 1);
    assert_eq!(
        column.keywords().map(|(k, _)| k).collect::<Vec<_>>(),
        ["gap"]
    );
    let ExprKind::List(items) = &column.args[0].value.kind else {
        panic!()
    };
    assert_eq!(items[0].kind, ExprKind::Variable("title".into()));
    let ExprKind::Call(template) = &items[2].kind else {
        panic!()
    };
    assert_eq!(template.kind(), CallKind::Template);
    assert_eq!(items[2].span.start.column, 48);

    let StatementForm::Assign(_, row) = st[2].form() else {
        panic!()
    };
    let ExprKind::Call(button) = &row.kind else {
        panic!()
    };
    let kinds: Vec<_> = button
        .positional()
        .map(|a| match &a.kind {
            ExprKind::Call(c) => format!("{:?}", c.kind()),
            ExprKind::List(l) => format!("checks:{}", l.len()),
            _ => "?".into(),
        })
        .collect();
    assert_eq!(kinds, ["Catalog", "Event", "checks:2"]);
    let StatementForm::Assign(_, n) = st[3].form() else {
        panic!()
    };
    let ExprKind::Call(text) = &n.kind else {
        panic!()
    };
    let ExprKind::Call(index) = &text.args[0].value.kind else {
        panic!()
    };
    assert_eq!(index.kind(), CallKind::System("index"));
    let StatementForm::Call(notify) = st[6].form() else {
        panic!()
    };
    assert_eq!(notify.args[0].value.kind, ExprKind::Skipped);
}

#[test]
fn literals_read_as_upstream_reads_them() {
    let value = |src: &str| {
        let out = parse(&format!("x = {src}"));
        assert!(out.errors.is_empty(), "{src}: {:?}", out.errors);
        match out.program.statements.into_iter().next().unwrap().kind {
            StatementKind::Assign { value, .. } => value.kind,
            _ => unreachable!(),
        }
    };
    let num = |src: &str| match value(src) {
        ExprKind::Number(n) => n.value,
        other => panic!("{other:?}"),
    };
    assert_eq!(num(".5"), NumberValue::Float(0.5));
    assert_eq!(num("-.25"), NumberValue::Float(-0.25));
    assert_eq!(num("1."), NumberValue::Float(1.0));
    assert_eq!(num("+3"), NumberValue::Int(3));
    assert_eq!(num("1E-2"), NumberValue::Float(0.01));
    assert_eq!(num("9007199254740991"), NumberValue::Int(9007199254740991));
    assert_eq!(value("null"), ExprKind::Null);
    assert_eq!(value("$"), ExprKind::Path(String::new()));
    assert_eq!(value("$name"), ExprKind::Path("name".into()));
    assert_eq!(value(r#""a\nb\q""#), ExprKind::String("a\nb\\q".into()));
    assert_eq!(value("\"\nHello\n\""), ExprKind::String("Hello".into()));
    assert_eq!(value(r#"r"C:\temp""#), ExprKind::String(r"C:\temp".into()));
    assert_eq!(
        value("\"\"\"two\nlines \\t\"\"\""),
        ExprKind::String("two\nlines \t".into())
    );
    assert_eq!(
        value(r#"R"""raw \n""""#),
        ExprKind::String(r"raw \n".into())
    );
    assert_eq!(
        value("?required").clone(),
        ExprKind::Check {
            name: "required".into(),
            args: vec![]
        }
    );
    match value(r#"{"content-type": 1, variable: "v"}"#) {
        ExprKind::Map(entries) => {
            let keys: Vec<_> = entries.iter().map(|e| e.key.as_str()).collect();
            assert_eq!(keys, ["content-type", "variable"]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn comments_separators_and_repairs() {
    let out = parse(
        "# hash\n// slash\n-- sql\n/* block\n */ a = 1; b = 2 -- trailing\ngreeting_\nlabel = Text($/user/\nname)",
    );
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    let names: Vec<_> = out
        .program
        .statements
        .iter()
        .map(|s| match &s.kind {
            StatementKind::Assign { target, .. } => target.text(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(names, ["a", "b", "greeting_label"]);
}
