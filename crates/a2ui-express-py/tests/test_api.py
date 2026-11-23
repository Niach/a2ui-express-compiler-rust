"""The Python surface of the binding: shapes, options and exceptions."""

from __future__ import annotations

import pytest

import a2ui_express as ax

CATALOG = {
    "catalogId": "demo",
    "protocolVersion": "1.0",
    "components": {
        "Text": {
            "properties": {
                "component": {"const": "Text"},
                "text": {"$ref": "common_types.json#/$defs/DynamicString"},
            },
            "required": ["component", "text"],
        },
        "Card": {
            "properties": {
                "component": {"const": "Card"},
                "child": {"$ref": "common_types.json#/$defs/ComponentId"},
            },
            "required": ["component", "child"],
        },
    },
}


def test_compile_and_decompile_round_trip() -> None:
    messages = ax.compile(CATALOG, 'root = Card(body)\nbody = Text("Hi")', surface_id="main")
    surface = messages[0]["createSurface"]
    assert surface["surfaceId"] == "main"
    assert list(surface["components"][0]) == ["id", "component", "child"]  # emission order
    text = ax.decompile([CATALOG], messages, order="top-down")
    assert text.index("root") < text.index("body")
    assert ax.compile(CATALOG, text, surface_id="main") == messages


def test_errors_are_typed_and_carry_partial_parts() -> None:
    with pytest.raises(ax.ValidationError) as info:
        ax.parse_response(CATALOG, "Before.\n<a2ui>root = Nope()</a2ui>")
    assert isinstance(info.value, ax.ExpressError)
    assert info.value.category == "ValidationError"
    assert info.value.partial == [{"text": "Before."}]
    with pytest.raises(ax.ParseError):
        ax.compile(CATALOG, "root = Text(")
    with pytest.raises(ax.CatalogError):
        ax.compile({}, "root = Text('x')")
    with pytest.raises(ValueError):
        ax.decompile(CATALOG, [], order="sideways")
    with pytest.raises(TypeError):
        ax.compile(object(), "")


def test_stream_compiler() -> None:
    stream = ax.StreamCompiler(CATALOG, surface_id="main")
    parts = stream.push('Here.\n<a2ui>\nroot = Card(body)\nbody = Text("Hi')
    assert stream.in_block
    parts += stream.push('")\n</a2ui>')
    parts += stream.finish()
    assert parts[0] == {"text": "Here."}
    assert "createSurface" in parts[1]["a2ui"][0]
    assert not stream.in_block


def test_stream_continue_after_a_rejected_block() -> None:
    stream = ax.StreamCompiler(CATALOG, after_rejected_block="continue")
    parts = stream.push("<a2ui>root = Nope()</a2ui> then text") + stream.finish()
    assert parts[0]["error"]["category"] == "ValidationError"
    assert parts[-1] == {"text": "then text"}


def test_unwrap_wrap_and_prompt() -> None:
    raw = ax.unwrap('Hi\n<a2ui>\nroot = Text("x")\n</a2ui>')
    assert raw == [{"text": "Hi"}, {"a2ui_raw": 'root = Text("x")'}]
    assert ax.unwrap(ax.wrap(raw)) == raw
    assert ax.has_format_content(ax.wrap(raw), True)
    prompt = ax.generate_prompt(CATALOG, examples=[ax.compile(CATALOG, 'root = Text("x")')])
    assert "Card(" in prompt
    assert ax.agent_callable_functions(CATALOG) == []


def test_loaded_catalogs() -> None:
    catalogs = ax.Catalogs([CATALOG], surface_id="main")
    assert catalogs.version == "1.0"
    assert catalogs.ids == ["demo"]
    messages = catalogs.compile('root = Text("x")', surface_id="main")
    assert messages == ax.compile(CATALOG, 'root = Text("x")', surface_id="main")
    assert catalogs.compile(catalogs.decompile(messages), surface_id="main") == messages
    for _ in range(2):
        stream = catalogs.stream(surface_id="main")
        parts = stream.push('<a2ui>root = Text("x")</a2ui>') + stream.finish()
        assert parts == [{"a2ui": messages}]
    assert catalogs.parse_response('<a2ui>root = Text("x")</a2ui>', surface_id="main") == [{"a2ui": messages}]
    assert "Text(" in catalogs.generate_prompt()
    assert catalogs.agent_callable_functions() == []
    with pytest.raises(ax.CatalogError):
        ax.Catalogs([])
