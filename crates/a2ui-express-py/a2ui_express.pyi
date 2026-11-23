"""Type stubs for the A2UI Express compiler binding (`a2ui_express`)."""

from typing import Any, Literal, Mapping, Optional, Sequence, Union

__version__: str
DEFAULT_SURFACE_ID: str

Json = Any
CatalogDocument = Mapping[str, Json]
CatalogInput = Union[CatalogDocument, Sequence[CatalogDocument]]
Message = dict[str, Json]
Part = dict[str, Json]
"""`{"text": str}` | `{"a2ui": list[Message]}` | `{"error": {"category", "message"}}`"""
RawPart = dict[str, Json]
"""`{"text": str}` | `{"a2ui_raw": str, "is_final"?: bool}`"""

class ExpressError(Exception):
    category: str
    partial: list[Part]

class ParseError(ExpressError): ...
class ValidationError(ExpressError): ...
class CatalogError(ExpressError): ...

def compile(
    catalogs: CatalogInput,
    source: str,
    *,
    surface_id: Optional[str] = None,
    version: Optional[str] = None,
) -> list[Message]: ...
def decompile(
    catalogs: CatalogInput,
    messages: Sequence[Mapping[str, Json]],
    *,
    keyword_args: bool = False,
    order: Literal["upstream", "top-down"] = "upstream",
) -> str: ...
def parse_response(
    catalogs: CatalogInput,
    text: str,
    *,
    surface_id: Optional[str] = None,
    wrapped: bool = True,
    progressive_keys: Optional[Sequence[str]] = None,
    version: Optional[str] = None,
) -> list[Part]: ...
def unwrap(text: str) -> list[RawPart]: ...
def wrap(parts: Sequence[Mapping[str, Json]]) -> str: ...
def has_format_content(text: str, complete: bool = False) -> bool: ...
def generate_prompt(
    catalogs: CatalogInput,
    *,
    surface_id: Optional[str] = None,
    examples: Optional[Sequence[Sequence[Mapping[str, Json]]]] = None,
    allowed_messages: Optional[Sequence[str]] = None,
) -> str: ...
def agent_callable_functions(catalogs: CatalogInput) -> list[str]: ...

class StreamCompiler:
    def __init__(
        self,
        catalogs: CatalogInput,
        *,
        surface_id: Optional[str] = None,
        wrapped: bool = True,
        progressive_keys: Optional[Sequence[str]] = None,
        after_rejected_block: Literal["end", "continue"] = "end",
        version: Optional[str] = None,
    ) -> None: ...
    def push(self, chunk: str) -> list[Part]: ...
    def finish(self) -> list[Part]: ...
    @property
    def in_block(self) -> bool: ...

class Catalogs:
    """Loaded catalogs: read once, compiled against any number of times."""

    def __init__(
        self, catalogs: CatalogInput, *, version: Optional[str] = None, surface_id: Optional[str] = None
    ) -> None: ...
    @property
    def version(self) -> str: ...
    @property
    def ids(self) -> list[str]: ...
    def compile(self, source: str, *, surface_id: Optional[str] = None) -> list[Message]: ...
    def decompile(
        self,
        messages: Sequence[Mapping[str, Json]],
        *,
        keyword_args: bool = False,
        order: Literal["upstream", "top-down"] = "upstream",
    ) -> str: ...
    def parse_response(
        self,
        text: str,
        *,
        surface_id: Optional[str] = None,
        wrapped: bool = True,
        progressive_keys: Optional[Sequence[str]] = None,
    ) -> list[Part]: ...
    def stream(
        self,
        *,
        surface_id: Optional[str] = None,
        wrapped: bool = True,
        progressive_keys: Optional[Sequence[str]] = None,
        after_rejected_block: Literal["end", "continue"] = "end",
    ) -> StreamCompiler: ...
    def generate_prompt(
        self,
        *,
        surface_id: Optional[str] = None,
        examples: Optional[Sequence[Sequence[Mapping[str, Json]]]] = None,
        allowed_messages: Optional[Sequence[str]] = None,
    ) -> str: ...
    def agent_callable_functions(self) -> list[str]: ...
