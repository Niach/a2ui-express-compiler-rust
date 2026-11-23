//! Python bindings for the A2UI Express compiler (`import a2ui_express`).
//!
//! Every call goes through `a2ui-express-ffi`, which the wasm binding shares,
//! so both accept and return the same JSON: here as `dict`/`list`/`str`/…
//! (dicts keep the emission order of their keys).

use a2ui_express_ffi::{self as ffi, Category, FfiError, ResponseOptions};
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use serde_json::{Map, Number, Value};

create_exception!(
    a2ui_express,
    ExpressError,
    PyException,
    "Base class of the errors the Express toolchain raises. Carries `category` (the suite's name for it) and `partial` (the parts a response read made ready before the failure)."
);
create_exception!(
    a2ui_express,
    ParseError,
    ExpressError,
    "The payload text is not valid Express."
);
create_exception!(
    a2ui_express,
    ValidationError,
    ExpressError,
    "The payload reads but does not match the catalog."
);
create_exception!(
    a2ui_express,
    CatalogError,
    ExpressError,
    "A catalog document cannot be loaded, or the catalogs cannot be used together."
);

fn to_py_err(py: Python<'_>, error: FfiError) -> PyErr {
    let err = match error.category {
        Category::Parse => ParseError::new_err(error.message),
        Category::Validation => ValidationError::new_err(error.message),
        Category::Catalog => CatalogError::new_err(error.message),
        Category::Argument => return PyValueError::new_err(error.message),
    };
    let value = err.value(py);
    let partial = to_py(py, &Value::Array(error.partial));
    let set = value
        .setattr("category", error.category.as_str())
        .and_then(|()| value.setattr("partial", partial?));
    match set {
        Ok(()) => err,
        Err(e) => e,
    }
}

fn ok<T>(py: Python<'_>, result: ffi::Result<T>) -> PyResult<T> {
    result.map_err(|e| to_py_err(py, e))
}

/// A Python object (dict, list, str, int, float, bool, None) as JSON.
fn from_py(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    if obj.is_none() {
        return Ok(Value::Null);
    }
    if let Ok(b) = obj.cast::<PyBool>() {
        return Ok(Value::Bool(b.is_true()));
    }
    if let Ok(i) = obj.cast::<PyInt>() {
        if let Ok(v) = i.extract::<i64>() {
            return Ok(Value::from(v));
        }
        if let Ok(v) = i.extract::<u64>() {
            return Ok(Value::from(v));
        }
        let v: f64 = i.extract()?;
        return Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| PyValueError::new_err("integer out of range"));
    }
    if let Ok(f) = obj.cast::<PyFloat>() {
        return Number::from_f64(f.value())
            .map(Value::Number)
            .ok_or_else(|| PyValueError::new_err("NaN and infinity are not JSON"));
    }
    if let Ok(s) = obj.cast::<PyString>() {
        return Ok(Value::String(s.extract::<String>()?));
    }
    if let Ok(d) = obj.cast::<PyDict>() {
        let mut map = Map::with_capacity(d.len());
        for (k, v) in d.iter() {
            let key = k
                .cast::<PyString>()
                .map_err(|_| PyTypeError::new_err("JSON object keys must be str"))?;
            map.insert(key.extract::<String>()?, from_py(&v)?);
        }
        return Ok(Value::Object(map));
    }
    if let Ok(l) = obj.cast::<PyList>() {
        return l
            .iter()
            .map(|v| from_py(&v))
            .collect::<PyResult<_>>()
            .map(Value::Array);
    }
    if let Ok(t) = obj.cast::<PyTuple>() {
        return t
            .iter()
            .map(|v| from_py(&v))
            .collect::<PyResult<_>>()
            .map(Value::Array);
    }
    Err(PyTypeError::new_err(format!(
        "cannot convert {} to JSON",
        obj.get_type().name()?
    )))
}

/// JSON as Python objects.
fn to_py(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    Ok(match value {
        Value::Null => py.None(),
        Value::Bool(b) => PyBool::new(py, *b).to_owned().into_any().unbind(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_pyobject(py)?.into_any().unbind()
            } else if let Some(u) = n.as_u64() {
                u.into_pyobject(py)?.into_any().unbind()
            } else {
                n.as_f64()
                    .unwrap_or(f64::NAN)
                    .into_pyobject(py)?
                    .into_any()
                    .unbind()
            }
        }
        Value::String(s) => PyString::new(py, s).into_any().unbind(),
        Value::Array(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(to_py(py, item)?)?;
            }
            list.into_any().unbind()
        }
        Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(k, to_py(py, v)?)?;
            }
            dict.into_any().unbind()
        }
    })
}

fn to_py_list(py: Python<'_>, values: Vec<Value>) -> PyResult<Py<PyAny>> {
    to_py(py, &Value::Array(values))
}

/// Compiles one Express block (sentinel tags optional) to A2UI messages.
#[pyfunction]
#[pyo3(signature = (catalogs, source, *, surface_id=None, version=None))]
fn compile(
    py: Python<'_>,
    catalogs: &Bound<'_, PyAny>,
    source: &str,
    surface_id: Option<&str>,
    version: Option<&str>,
) -> PyResult<Py<PyAny>> {
    let catalogs = from_py(catalogs)?;
    let messages = ok(py, ffi::compile(catalogs, source, surface_id, version))?;
    to_py_list(py, messages)
}

/// Writes A2UI messages back as one Express block (without tags).
///
/// `order` is `"upstream"` (message order) or `"top-down"` (`root` first,
/// then breadth-first: what a streaming renderer wants).
#[pyfunction]
#[pyo3(signature = (catalogs, messages, *, keyword_args=false, order="upstream"))]
fn decompile(
    py: Python<'_>,
    catalogs: &Bound<'_, PyAny>,
    messages: &Bound<'_, PyAny>,
    keyword_args: bool,
    order: &str,
) -> PyResult<String> {
    let (catalogs, messages) = (from_py(catalogs)?, from_py(messages)?);
    ok(
        py,
        ffi::decompile(catalogs, messages, keyword_args, Some(order)),
    )
}

fn response_options(
    surface_id: Option<&str>,
    wrapped: bool,
    progressive_keys: Option<Vec<String>>,
    after_rejected_block: &str,
    version: Option<&str>,
) -> ffi::Result<ResponseOptions> {
    let defaults = ResponseOptions::default();
    Ok(ResponseOptions {
        surface_id: surface_id.map_or(defaults.surface_id, str::to_owned),
        wrapped,
        progressive_keys,
        after_rejected_block: ffi::parse_after_rejected_block(Some(after_rejected_block))?,
        version: version.map(str::to_owned),
    })
}

/// Reads a whole response (prose with `<a2ui>` blocks) and compiles each
/// block; returns its parts: `{"text"}`, `{"a2ui": [messages]}`.
#[pyfunction]
#[pyo3(signature = (catalogs, text, *, surface_id=None, wrapped=true, progressive_keys=None, version=None))]
fn parse_response(
    py: Python<'_>,
    catalogs: &Bound<'_, PyAny>,
    text: &str,
    surface_id: Option<&str>,
    wrapped: bool,
    progressive_keys: Option<Vec<String>>,
    version: Option<&str>,
) -> PyResult<Py<PyAny>> {
    let options = ok(
        py,
        response_options(surface_id, wrapped, progressive_keys, "end", version),
    )?;
    let catalogs = from_py(catalogs)?;
    let parts = ok(py, ffi::parse_response(catalogs, text, &options))?;
    to_py_list(py, parts)
}

/// Splits a response into raw parts: `{"text"}` and
/// `{"a2ui_raw", "is_final"?}`.
#[pyfunction]
fn unwrap(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
    to_py_list(py, ffi::unwrap(text))
}

/// Writes raw parts back out as a model would have emitted them.
#[pyfunction]
fn wrap(py: Python<'_>, parts: &Bound<'_, PyAny>) -> PyResult<String> {
    ok(py, ffi::wrap(&from_py(parts)?))
}

/// Whether a response holds an `<a2ui>` block (`complete`: a closed one).
#[pyfunction]
#[pyo3(signature = (text, complete=false))]
fn has_format_content(text: &str, complete: bool) -> bool {
    ffi::has_format_content(text, complete)
}

/// Generates the system-prompt section that teaches a model Express for the
/// catalogs. `examples` is a list of turns, each a list of A2UI messages.
#[pyfunction]
#[pyo3(signature = (catalogs, *, surface_id=None, examples=None, allowed_messages=None))]
fn generate_prompt(
    py: Python<'_>,
    catalogs: &Bound<'_, PyAny>,
    surface_id: Option<String>,
    examples: Option<&Bound<'_, PyAny>>,
    allowed_messages: Option<Vec<String>>,
) -> PyResult<String> {
    let options = prompt_options(py, surface_id, examples, allowed_messages)?;
    ok(py, ffi::generate_prompt(from_py(catalogs)?, &options))
}

fn prompt_options(
    py: Python<'_>,
    surface_id: Option<String>,
    examples: Option<&Bound<'_, PyAny>>,
    allowed_messages: Option<Vec<String>>,
) -> PyResult<ffi::PromptOptions> {
    let examples = match examples {
        Some(e) => ok(py, ffi::prompt_examples(from_py(e)?))?,
        None => Vec::new(),
    };
    Ok(ffi::PromptOptions {
        surface_id,
        examples,
        allowed_messages,
    })
}

/// The functions an agent may call standalone (`callRendererFunction`),
/// across the catalogs.
#[pyfunction]
fn agent_callable_functions(py: Python<'_>, catalogs: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    ok(py, ffi::agent_callable_functions(from_py(catalogs)?))
}

/// Loaded catalogs: a catalog is read once, then compiled against any number
/// of times. Prefer it to the free functions in a loop (loading a large
/// catalog costs far more than compiling a block against it).
#[pyclass(frozen, module = "a2ui_express")]
struct Catalogs {
    inner: ffi::Catalogs,
}

#[pymethods]
impl Catalogs {
    /// `surface_id`: the surface id streams and responses mostly use; its
    /// parser is set up with the catalogs (another one's on first use).
    #[new]
    #[pyo3(signature = (catalogs, *, version=None, surface_id=None))]
    fn new(
        py: Python<'_>,
        catalogs: &Bound<'_, PyAny>,
        version: Option<&str>,
        surface_id: Option<&str>,
    ) -> PyResult<Self> {
        let catalogs = from_py(catalogs)?;
        Ok(Catalogs {
            inner: ok(
                py,
                ffi::Catalogs::with_surface(catalogs, version, surface_id),
            )?,
        })
    }

    /// The protocol version messages are compiled for.
    #[getter]
    fn version(&self) -> &'static str {
        self.inner.version()
    }

    /// The catalog ids, in order.
    #[getter]
    fn ids(&self) -> Vec<String> {
        self.inner.ids()
    }

    #[pyo3(signature = (source, *, surface_id=None))]
    fn compile(
        &self,
        py: Python<'_>,
        source: &str,
        surface_id: Option<&str>,
    ) -> PyResult<Py<PyAny>> {
        let messages = ok(py, self.inner.compile(source, surface_id))?;
        to_py_list(py, messages)
    }

    #[pyo3(signature = (messages, *, keyword_args=false, order="upstream"))]
    fn decompile(
        &self,
        py: Python<'_>,
        messages: &Bound<'_, PyAny>,
        keyword_args: bool,
        order: &str,
    ) -> PyResult<String> {
        let messages = from_py(messages)?;
        ok(
            py,
            self.inner.decompile(messages, keyword_args, Some(order)),
        )
    }

    #[pyo3(signature = (text, *, surface_id=None, wrapped=true, progressive_keys=None))]
    fn parse_response(
        &self,
        py: Python<'_>,
        text: &str,
        surface_id: Option<&str>,
        wrapped: bool,
        progressive_keys: Option<Vec<String>>,
    ) -> PyResult<Py<PyAny>> {
        let options = ok(
            py,
            response_options(surface_id, wrapped, progressive_keys, "end", None),
        )?;
        let parts = ok(py, self.inner.parse_response(text, &options))?;
        to_py_list(py, parts)
    }

    /// Opens a `StreamCompiler` on these catalogs.
    #[pyo3(signature = (*, surface_id=None, wrapped=true, progressive_keys=None, after_rejected_block="end"))]
    fn stream(
        &self,
        py: Python<'_>,
        surface_id: Option<&str>,
        wrapped: bool,
        progressive_keys: Option<Vec<String>>,
        after_rejected_block: &str,
    ) -> PyResult<StreamCompiler> {
        let options = ok(
            py,
            response_options(
                surface_id,
                wrapped,
                progressive_keys,
                after_rejected_block,
                None,
            ),
        )?;
        Ok(StreamCompiler {
            inner: ok(py, self.inner.stream(&options))?,
        })
    }

    #[pyo3(signature = (*, surface_id=None, examples=None, allowed_messages=None))]
    fn generate_prompt(
        &self,
        py: Python<'_>,
        surface_id: Option<String>,
        examples: Option<&Bound<'_, PyAny>>,
        allowed_messages: Option<Vec<String>>,
    ) -> PyResult<String> {
        let options = prompt_options(py, surface_id, examples, allowed_messages)?;
        ok(py, self.inner.generate_prompt(&options))
    }

    fn agent_callable_functions(&self) -> Vec<String> {
        self.inner.agent_callable_functions()
    }
}

/// Reads a model response as it streams in: each `push` returns the parts
/// that chunk made ready.
///
/// A stream belongs to the thread that opened it.
#[pyclass(unsendable, module = "a2ui_express")]
struct StreamCompiler {
    inner: ffi::Stream,
}

#[pymethods]
impl StreamCompiler {
    #[new]
    #[pyo3(signature = (catalogs, *, surface_id=None, wrapped=true, progressive_keys=None, after_rejected_block="end", version=None))]
    fn new(
        py: Python<'_>,
        catalogs: &Bound<'_, PyAny>,
        surface_id: Option<&str>,
        wrapped: bool,
        progressive_keys: Option<Vec<String>>,
        after_rejected_block: &str,
        version: Option<&str>,
    ) -> PyResult<Self> {
        let options = ok(
            py,
            response_options(
                surface_id,
                wrapped,
                progressive_keys,
                after_rejected_block,
                version,
            ),
        )?;
        let catalogs = from_py(catalogs)?;
        Ok(StreamCompiler {
            inner: ok(py, ffi::Stream::new(catalogs, &options))?,
        })
    }

    /// Feeds a chunk; returns the parts it made ready.
    fn push(&mut self, py: Python<'_>, chunk: &str) -> PyResult<Py<PyAny>> {
        let parts = ok(py, self.inner.push(chunk))?;
        to_py_list(py, parts)
    }

    /// Ends the response; returns what was still held back.
    fn finish(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let parts = ok(py, self.inner.finish())?;
        to_py_list(py, parts)
    }

    /// Whether the stream is inside an `<a2ui>` block.
    #[getter]
    fn in_block(&self) -> bool {
        self.inner.in_block()
    }
}

#[pymodule]
fn a2ui_express(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("DEFAULT_SURFACE_ID", ffi::DEFAULT_SURFACE_ID)?;
    m.add("ExpressError", py.get_type::<ExpressError>())?;
    m.add("ParseError", py.get_type::<ParseError>())?;
    m.add("ValidationError", py.get_type::<ValidationError>())?;
    m.add("CatalogError", py.get_type::<CatalogError>())?;
    m.add_function(wrap_pyfunction!(compile, m)?)?;
    m.add_function(wrap_pyfunction!(decompile, m)?)?;
    m.add_function(wrap_pyfunction!(parse_response, m)?)?;
    m.add_function(wrap_pyfunction!(unwrap, m)?)?;
    m.add_function(wrap_pyfunction!(wrap, m)?)?;
    m.add_function(wrap_pyfunction!(has_format_content, m)?)?;
    m.add_function(wrap_pyfunction!(generate_prompt, m)?)?;
    m.add_function(wrap_pyfunction!(agent_callable_functions, m)?)?;
    m.add_class::<StreamCompiler>()?;
    m.add_class::<Catalogs>()?;
    Ok(())
}
