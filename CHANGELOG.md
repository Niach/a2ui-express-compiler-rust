# Changelog

All crates and bindings of this workspace share one version.

## Unreleased

### Fixed

- v1.0 identifiers (component ids, catalog names, extension keys) follow
  Unicode 17.0.0, as eui does, instead of the Express lexer's older tables:
  an id with ZWJ or ZWNJ (`XID_Continue` since Unicode 15.1) is accepted, and
  the decompiler writes it under a derived variable name with `id="…"`.
  The lexer is unchanged (#AXC-28).
- A v1.0 catalog may name a component, property, function or argument `true`,
  `false`, `null` or `_`, as upstream's loaders allow.

### Added

- `syntax::uax31`: `is_uax31_identifier`, `is_xid_start`, `is_xid_continue`
  and `UNICODE_VERSION`.

## 0.2.0

### Breaking

- `schema::CatalogError` gains `InvalidIdentifier` and `InvalidMetadata`, and
  is now `#[non_exhaustive]`: a `match` on it needs a wildcard arm.
- `compile`, the streaming compiler and response parsing validate their output
  by default. They return `ValidationError` for a renderer-only or
  user-activation function called standalone, an undeclared `@call`, duplicate
  ids, and a tree or call chain past the depth limits.
- A v1.0 catalog whose names are not UAX #31 identifiers, or whose metadata is
  malformed, is rejected at load.

### Added

- `compile::validate`: the same check, for messages you built yourself.
- `compile::MAX_DEPTH` and `compile::MAX_CALL_DEPTH`: the depth limits.

### Fixed

- The decompiler writes a check whose condition is not a call in the long form.

## 0.1.0

The first version: parse, compile, stream and decompile the Express inference
format, with the FFI, Python and WebAssembly bindings.
