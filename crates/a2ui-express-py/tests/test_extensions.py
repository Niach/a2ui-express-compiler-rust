"""Replays the at_exponential_express fixtures (conformance/extensions/*.json)
through the Python binding, as crates/a2ui-express/tests/extensions.rs and the
npm package's test do: compile (also streamed one character at a time, and
decompiled back to a fixed point), decompile, expand_theme, compile_patch
(also as a property: applying the patch equals compiling the new screen)."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

import a2ui_express as ax

ROOT = Path(__file__).resolve().parents[3]
FIXTURES = ROOT / "conformance/extensions"
NAMES = ["theme", "hex", "style", "motion", "patch"]


def catalog(path: str) -> dict:
    doc = json.loads((FIXTURES / path).read_text())
    doc.setdefault("metadata", {}).setdefault("extensions", {}).setdefault("at_exponential_express", True)
    return doc


def cases():
    for name in NAMES:
        doc = json.loads((FIXTURES / f"{name}.json").read_text())
        for case in doc["cases"]:
            yield pytest.param(doc.get("catalog", "catalog.json"), case, id=f"{name}/{case['name']}")


def set_pointer(root: dict, path: str, value):
    if path in ("", "/"):
        return value
    tokens = [t.replace("~1", "/").replace("~0", "~") for t in path.lstrip("/").split("/")]
    cur = root
    for t in tokens[:-1]:
        if not isinstance(cur.get(t), dict):
            cur[t] = {}
        cur = cur[t]
    if value is None:
        cur.pop(tokens[-1], None)
    else:
        cur[tokens[-1]] = value
    return root


def model(messages: list[dict], state: dict | None = None) -> dict:
    surfaces = state if state is not None else {}
    for m in messages:
        if "createSurface" in m:
            b = m["createSurface"]
            surfaces[b["surfaceId"]] = {
                "components": {c["id"]: c for c in b.get("components", [])},
                "data": b.get("dataModel", {}),
                "theme": b.get("metadata", {}).get("extensions", {}).get("at_exponential_theme"),
            }
        elif "updateComponents" in m:
            s = surfaces.setdefault(m["updateComponents"]["surfaceId"], {"components": {}, "data": {}, "theme": None})
            s["components"].update({c["id"]: c for c in m["updateComponents"]["components"]})
        elif "updateDataModel" in m:
            b = m["updateDataModel"]
            s = surfaces.setdefault(b["surfaceId"], {"components": {}, "data": {}, "theme": None})
            s["data"] = set_pointer(s["data"], b.get("path", "/"), b.get("value"))
        elif "callRendererFunction" in m:
            b = m["callRendererFunction"]
            if b["callFunction"].get("@call") == "setTheme":
                s = surfaces.setdefault(b.get("surfaceId", "default_surface"), {"components": {}, "data": {}, "theme": None})
                s["theme"] = b["callFunction"]["args"]["theme"]
    return surfaces


def strings(v, out):
    if isinstance(v, str):
        out.append(v)
    elif isinstance(v, list):
        for x in v:
            strings(x, out)
    elif isinstance(v, dict):
        for k, x in v.items():
            if k != "id":
                strings(x, out)


def drawn(surfaces: dict) -> dict:
    out = {}
    for sid, s in surfaces.items():
        seen, todo = set(), ["root"]
        while todo:
            c = todo.pop()
            if c in seen or c not in s["components"]:
                continue
            seen.add(c)
            found = []
            strings(s["components"][c], found)
            todo.extend(found)
        out[sid] = ({k: v for k, v in s["components"].items() if k in seen}, s["data"], s["theme"])
    return out


def stream(cat: dict, source: str) -> list[dict]:
    s = ax.StreamCompiler(cat, surface_id="main")
    out = []
    for ch in f"<a2ui>\n{source}\n</a2ui>":
        for part in s.push(ch):
            out.extend(part.get("a2ui", []))
    for part in s.finish():
        out.extend(part.get("a2ui", []))
    return out


@pytest.mark.parametrize("catalog_path,case", list(cases()))
def test_fixture(catalog_path, case):
    cat = catalog(catalog_path)
    action = case["action"]
    if action == "compile":
        if "expect_error" in case:
            with pytest.raises(ax.ExpressError) as e:
                ax.compile(cat, case["input"], surface_id="main")
            want = case["expect_error"]
            if "message" in want:
                assert want["message"] in str(e.value)
            return
        got = ax.compile(cat, case["input"], surface_id="main")
        assert got == case["expect"]
        text = ax.decompile(cat, got, order="top-down")
        assert text == case["decompiled"]
        assert ax.compile(cat, text, surface_id="main") == got
        if case.get("stream", True):
            assert drawn(model(stream(cat, case["input"]))) == drawn(model(got))
    elif action == "decompile":
        assert ax.decompile(cat, case["input"], order="top-down") == case["expect"]
    elif action == "expand_theme":
        if "expect_error" in case:
            with pytest.raises(ax.ExpressError):
                ax.expand_theme(case["input"])
            return
        assert ax.expand_theme(case["input"]) == case["expect"]
    elif action == "patch":
        if "expect_error" in case:
            with pytest.raises(ax.ExpressError) as e:
                ax.compile_patch(cat, case["screen"], case["patch"], surface_id="main")
            if "message" in case["expect_error"]:
                assert case["expect_error"]["message"] in str(e.value)
            return
        p = ax.compile_patch(cat, case["screen"], case["patch"], surface_id="main")
        assert p["messages"] == case["expect"]
        assert p["screen"] == case["screen_after"]
        before = model(ax.compile(cat, case["screen"], surface_id="main"))
        assert drawn(model(p["messages"], before)) == drawn(model(ax.compile(cat, p["screen"], surface_id="main")))
    else:
        raise AssertionError(f"unknown action {action}")


def test_catalogs_compile_patch_and_style_table():
    cat = catalog("catalog.json")
    loaded = ax.Catalogs(cat, surface_id="main")
    p = loaded.compile_patch('root = Column([a])\na = Text("x")', 'a = Text("y")', surface_id="main")
    assert p["messages"][0]["updateComponents"]["components"] == [{"id": "a", "component": "Text", "text": "y"}]
    table = ax.style_table()
    assert any(k["key"] == "padding" and k["prefix"] == "p" for k in table["keys"])
