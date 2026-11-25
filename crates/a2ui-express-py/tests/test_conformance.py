"""Runs the upstream Express conformance suite through the Python binding,
so it cannot drift from the crate.

Every compile, decompile, parse_response, parse_chunk (also fed one
character at a time) and generate_prompt_snippet case runs as written (v1.0),
except those `conformance/bindings_skip.txt` and `conformance/out_of_scope.txt`
list; the npm package's test (`packages/npm/test`) runs the same cases.
"""

from __future__ import annotations

import json
import re
from pathlib import Path

import pytest
import yaml

import a2ui_express as ax

ROOT = Path(__file__).resolve().parents[3]
CONFORMANCE = ROOT / "vendor/a2ui/conformance"
SUITES = ["compiler", "decompiler", "response_parser", "response_streaming", "prompt_generator"]
DEFAULT_CATALOG = "test_data/catalogs/simplified_catalog_v1_0.json"
ACTIONS = {"compile", "decompile", "parse_response", "parse_chunk", "generate_prompt_snippet"}


def load_cases() -> dict[str, dict]:
    cases = {}
    for suite in SUITES:
        for case in yaml.safe_load((CONFORMANCE / f"agent/express/{suite}.yaml").read_text()):
            cases[case["name"]] = case
    return cases


CASES = load_cases()


def skipped() -> list[str]:
    lines = [
        line
        for name in ("bindings_skip", "out_of_scope")
        for line in (ROOT / f"conformance/{name}.txt").read_text().splitlines()
    ]
    return [n for n in (line.split("#")[0].strip() for line in lines) if n]


SKIPPED = skipped()


def case_names() -> list[str]:
    return [n for n, c in CASES.items() if c["action"] in ACTIONS and n not in SKIPPED]


def resolve_catalog(entry) -> dict:
    """A catalog entry as upstream's harness reads it: a path, an inline
    document, or `{catalog, transformers}`; `protocolVersion` defaults to 1.0
    and `catalogId` to the file's basename (or `inline`)."""
    transformers = []
    if isinstance(entry, dict) and "catalog" in entry:
        transformers = entry.get("transformers", [])
        entry = entry["catalog"]
    if isinstance(entry, str):
        doc = json.loads((CONFORMANCE / entry).read_text())
        default_id = Path(entry).stem
    else:
        doc, default_id = dict(entry), "inline"
    for spec in transformers:
        for key, kind in (("component_pruning", "components"), ("function_pruning", "functions")):
            if key in spec and isinstance(doc.get(kind), dict):
                doc[kind] = {k: v for k, v in doc[kind].items() if k in spec[key]}
            elif key in spec and isinstance(doc.get(kind), list):
                doc[kind] = [f for f in doc[kind] if f.get("name") in spec[key]]
    doc.setdefault("protocolVersion", "1.0")
    doc.setdefault("catalogId", default_id)
    return doc


def catalogs_of(args: dict, *, prompt: bool = False) -> list[dict]:
    if "catalogs" in args:
        return [resolve_catalog(e) for e in args["catalogs"]]
    if "catalog" in args:
        return [resolve_catalog(args["catalog"])]
    return [] if prompt else [resolve_catalog(DEFAULT_CATALOG)]


def json_eq(a, b) -> bool:
    """Deep equality, numbers by value (`3 == 3.0`), key order ignored."""
    if isinstance(a, bool) or isinstance(b, bool):
        return a is b if isinstance(a, bool) and isinstance(b, bool) else False
    if isinstance(a, (int, float)) and isinstance(b, (int, float)):
        return a == b
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(json_eq(x, y) for x, y in zip(a, b))
    if isinstance(a, dict) and isinstance(b, dict):
        return a.keys() == b.keys() and all(json_eq(a[k], b[k]) for k in a)
    return a == b


def assert_json(what: str, actual, expected) -> None:
    assert json_eq(actual, expected), (
        f"{what}\n--- actual\n{json.dumps(actual, indent=2)}\n--- expected\n{json.dumps(expected, indent=2)}"
    )


def check_error(error: Exception, expected: dict) -> None:
    assert isinstance(error, ax.ExpressError), f"not an ExpressError: {error!r}"
    category = expected.get("category")
    if category:
        assert error.category == category, f"expected a {category}, got {error.category}: {error}"
        assert type(error).__name__ == category
    if expected.get("message"):
        assert re.search(expected["message"], str(error)), f"{error} does not match {expected['message']!r}"


def comparable(parts: list) -> list:
    """Error parts carry a message the suites do not state: drop it."""
    return [
        {"error": {"category": p["error"]["category"]}} if "error" in p else p for p in parts
    ]


def merge_text(parts: list) -> list:
    merged: list = []
    for p in parts:
        if "text" in p and merged and "text" in merged[-1]:
            merged[-1] = {"text": merged[-1]["text"] + p["text"]}
        else:
            merged.append(dict(p))
    return merged


def fragments(text: str, case: dict) -> None:
    for f in case.get("expect_contains", []):
        assert f in text, f"missing {f!r}\n{text}"
    for f in case.get("expect_absent", []):
        assert f not in text, f"unexpected {f!r}\n{text}"


def run_compile(case: dict) -> None:
    catalogs = catalogs_of(case.get("args", {}))
    if "expect_error" in case:
        with pytest.raises(ax.ExpressError) as info:
            ax.compile(catalogs, case["input"])
        check_error(info.value, case["expect_error"])
        return
    assert_json("compiled messages", ax.compile(catalogs, case["input"]), case["expect"])


def run_decompile(case: dict) -> None:
    catalogs = catalogs_of(case.get("args", {}))
    if "expect_error" in case:
        with pytest.raises(ax.ExpressError) as info:
            ax.decompile(catalogs, case["messages"])
        check_error(info.value, case["expect_error"])
        return
    notation = ax.decompile(catalogs, case["messages"])
    fragments(notation, case)
    if case.get("expect_round_trip"):
        assert_json("round trip", ax.compile(catalogs, notation), case["messages"])
    if "expect_recompiled" in case:
        assert_json("recompiled", ax.compile(catalogs, notation), case["expect_recompiled"])


def run_parse_response(case: dict) -> None:
    args = case.get("args", {})
    catalogs = catalogs_of(args)
    wrapped = args.get("wrapped", True)
    if "expect_error" in case:
        with pytest.raises(ax.ExpressError) as info:
            ax.parse_response(catalogs, case["input"], wrapped=wrapped)
        check_error(info.value, case["expect_error"])
        return
    parts = ax.parse_response(catalogs, case["input"], wrapped=wrapped)
    assert_json("parts", comparable(parts), case["expect"])


def open_stream(args: dict) -> ax.StreamCompiler:
    return ax.StreamCompiler(
        catalogs_of(args),
        wrapped=args.get("wrapped", True),
        progressive_keys=args.get("progressive_keys"),
        after_rejected_block=args.get("after_rejected_block", "end"),
    )


def stream_error(case: dict):
    """The error a case expects, at a step or on close."""
    for step in case["steps"]:
        if "expect_error" in step:
            return step["expect_error"]
    return case.get("expect_error")


def run_parse_chunk(case: dict) -> None:
    args = case.get("args", {})
    stream = open_stream(args)
    for index, step in enumerate(case["steps"]):
        if "expect_error" in step:
            with pytest.raises(ax.ExpressError) as info:
                stream.push(step["input"])
            check_error(info.value, step["expect_error"])
            if "expect_partial_results" in step:
                assert_json("partial", comparable(info.value.partial), step["expect_partial_results"])
            break
        parts = stream.push(step["input"])
        assert_json(f"step {index} ({step['input']!r})", comparable(parts), step.get("expect", []))
        if "expect_in_block" in step:
            assert stream.in_block == step["expect_in_block"], f"step {index}: in_block"
    else:
        if "expect_error" in case:
            with pytest.raises(ax.ExpressError) as info:
                stream.finish()
            check_error(info.value, case["expect_error"])
        else:
            closed = stream.finish()
            if "expect_close" in case:
                assert_json("close", comparable(closed), case["expect_close"])

    # The same response fed one character at a time: the same text, and the
    # same failure (or none).
    text = "".join(step["input"] for step in case["steps"])
    stream = open_stream(args)
    parts: list = []
    expected_error = stream_error(case)
    try:
        for ch in text:
            parts.extend(stream.push(ch))
        parts.extend(stream.finish())
    except ax.ExpressError as error:
        assert expected_error is not None, f"char by char: unexpected {error!r}"
        check_error(error, {"category": expected_error.get("category")})
        return
    assert expected_error is None, "char by char: no error raised"
    texts = [p for p in merge_text(parts) if "text" in p]
    if case.get("expect_matches_single_shot"):
        single = ax.parse_response(catalogs_of(args), text, wrapped=args.get("wrapped", True))
        assert_json("char-by-char text", texts, [p for p in merge_text(single) if "text" in p])


def run_generate_prompt(case: dict) -> None:
    args = case.get("args", {})
    catalogs = catalogs_of(args, prompt=True)
    examples = []
    for path in args.get("examples", []):
        doc = json.loads((CONFORMANCE / path).read_text())
        examples.append(doc if isinstance(doc, list) else [doc])
    call = lambda: ax.generate_prompt(  # noqa: E731
        catalogs, examples=examples, allowed_messages=args.get("allowed_messages")
    )
    if "expect_error" in case:
        with pytest.raises(ax.ExpressError) as info:
            call()
        check_error(info.value, case["expect_error"])
        return
    snippet = call()
    if "expect_file" in case:
        assert snippet == (CONFORMANCE / case["expect_file"]).read_text()
    fragments(snippet, case)
    if "expect_callable_functions" in case:
        assert ax.agent_callable_functions(catalogs) == case["expect_callable_functions"]
    if case.get("expect_deterministic"):
        assert call() == snippet


RUNNERS = {
    "compile": run_compile,
    "decompile": run_decompile,
    "parse_response": run_parse_response,
    "parse_chunk": run_parse_chunk,
    "generate_prompt_snippet": run_generate_prompt,
}


def test_skip_list_names_existing_cases() -> None:
    missing = [n for n in SKIPPED if n not in CASES]
    assert not missing, f"the conformance skip lists name unknown cases: {missing}"


def test_runs_enough_cases() -> None:
    # A guard against a suite that silently stops loading.
    assert len(case_names()) > 200


@pytest.mark.parametrize("name", case_names())
def test_conformance_case(name: str) -> None:
    RUNNERS[CASES[name]["action"]](CASES[name])
