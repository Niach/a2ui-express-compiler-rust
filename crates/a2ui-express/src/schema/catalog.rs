//! One catalog: loading it, and what the toolchain asks about it.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use super::common::{common_types, system_function_schemas};
use super::refs::{
    ENVELOPE_MIXIN_NAMES, common_def_name, common_def_of, envelope_key, is_supported_v1_ref,
    local_def_name, local_ref,
};
use super::resolve::{Schemas, Segment, SlotKind};
use super::{CatalogError, CatalogVersion};
use crate::syntax::uax31::is_uax31_identifier;

const STANDARD_CATALOG_ID_DESCRIPTION: &str =
    "The catalog ID for this component, overriding any surface-level default catalogId.";
const STANDARD_METADATA_DESCRIPTION: &str =
    "Optional component-level metadata for vendor extensions.";

/// Options for [`Catalog::load`].
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// Read the catalog with this version instead of the `protocolVersion` it
    /// declares (or v0.9 when it declares none).
    pub protocol_version: Option<CatalogVersion>,
    /// Use this catalog ID instead of the one the document declares.
    pub catalog_id: Option<String>,
    /// Use this `common_types.json` instead of the vendored one of the version.
    pub common_types: Option<Value>,
}

/// Which roles may call a function (`allowedCallers`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AllowedCallers {
    /// Only the renderer (the default).
    #[default]
    RendererOnly,
    /// Only the agent.
    AgentOnly,
    /// Either.
    RendererOrAgent,
}

impl AllowedCallers {
    fn parse(raw: &str) -> Option<AllowedCallers> {
        match raw {
            "rendererOnly" => Some(AllowedCallers::RendererOnly),
            "agentOnly" => Some(AllowedCallers::AgentOnly),
            "rendererOrAgent" => Some(AllowedCallers::RendererOrAgent),
            _ => None,
        }
    }

    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AllowedCallers::RendererOnly => "rendererOnly",
            AllowedCallers::AgentOnly => "agentOnly",
            AllowedCallers::RendererOrAgent => "rendererOrAgent",
        }
    }

    /// Whether the agent may call the function.
    pub fn admits_agent(self) -> bool {
        self != AllowedCallers::RendererOnly
    }

    /// Whether the renderer may call the function.
    pub fn admits_renderer(self) -> bool {
        self != AllowedCallers::AgentOnly
    }
}

/// What the catalog declares for one component.
#[derive(Debug, Clone)]
pub struct Component {
    name: String,
    schema: Value,
    properties: Vec<String>,
    required: Vec<String>,
    check_rule_property: Option<String>,
    inherited_check_rule_schema: Option<Value>,
    custom_envelope: Vec<String>,
    enums: HashMap<String, Vec<Value>>,
    /// Every property any sub-schema declares, first declaration wins,
    /// dereferenced.
    property_schemas: HashMap<String, Value>,
    description: Option<String>,
}

impl Component {
    /// The component's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The component's schema as the catalog writes it.
    pub fn schema(&self) -> &Value {
        &self.schema
    }

    /// The Express arguments in positional order: the order the schema
    /// declares them (own properties first, then `allOf` entries, then
    /// mixins), without `id`, `component` and the envelope mixins, with the
    /// check-rule property last.
    pub fn properties(&self) -> &[String] {
        &self.properties
    }

    /// The required properties (including `component`), in declared order.
    pub fn required(&self) -> &[String] {
        &self.required
    }

    /// Whether `property` is required.
    pub fn is_required(&self, property: &str) -> bool {
        self.required.iter().any(|r| r == property)
    }

    /// The property that holds the component's check rules, if it is checkable.
    pub fn check_rule_property(&self) -> Option<&str> {
        self.check_rule_property.as_deref()
    }

    /// Whether the component supports client-side checks.
    pub fn is_checkable(&self) -> bool {
        self.check_rule_property.is_some()
    }

    /// The envelope mixin names (`catalogId`, `metadata`, `accessibility`,
    /// `checks`) that the component declares as its own custom properties
    /// rather than inheriting from `ComponentCommon` or `Checkable`.
    pub fn custom_envelope_properties(&self) -> &[String] {
        &self.custom_envelope
    }

    /// The allowed values of a property's enum, if it declares one.
    pub fn property_enum(&self, property: &str) -> Option<&[Value]> {
        self.enums.get(property).map(Vec::as_slice)
    }

    /// The schema of a property, with local `$ref`s dereferenced. `checks`
    /// also finds a v1.0 `@checks`.
    pub fn property_schema(&self, property: &str) -> Option<&Value> {
        self.property_schemas.get(property).or_else(|| {
            (property == "checks")
                .then(|| self.property_schemas.get("@checks"))
                .flatten()
        })
    }

    /// The schema of the check-rule property, own or inherited.
    pub fn check_rule_property_schema(&self) -> Option<&Value> {
        let prop = self.check_rule_property.as_deref()?;
        self.property_schema(prop)
            .or(self.inherited_check_rule_schema.as_ref())
    }

    /// The description the component schema (or its first described `allOf`
    /// entry) gives.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// The schema an Express argument reads: the check-rule schema for the
    /// check-rule property, else the property's schema.
    fn argument_schema(&self, property: &str) -> Option<&Value> {
        if self.check_rule_property.as_deref() == Some(property) {
            if let Some(s) = self.check_rule_property_schema() {
                return Some(s);
            }
        }
        self.property_schema(property)
    }
}

/// What the catalog (or the protocol, for a system function) declares for a function.
#[derive(Debug, Clone)]
pub struct Function {
    name: String,
    schema: Value,
    arguments: Vec<String>,
    required: Vec<String>,
    argument_schemas: Vec<(String, Value)>,
    return_type: Option<String>,
    allowed_callers: AllowedCallers,
    requires_user_activation: bool,
    system: bool,
}

impl Function {
    /// The function's name (`@`-prefixed for a system function).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The function's schema as written.
    pub fn schema(&self) -> &Value {
        &self.schema
    }

    /// The argument names in positional order.
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    /// The required arguments.
    pub fn required(&self) -> &[String] {
        &self.required
    }

    /// Whether `argument` is required.
    pub fn is_required(&self, argument: &str) -> bool {
        self.required.iter().any(|r| r == argument)
    }

    /// The schema of an argument, as written.
    pub fn argument_schema(&self, argument: &str) -> Option<&Value> {
        self.argument_schemas
            .iter()
            .find_map(|(k, v)| (k == argument).then_some(v))
    }

    /// The declared `returnType` (`string`, `number`, `boolean`, `array`,
    /// `object`, `validationResult`, `any`, `void`).
    pub fn return_type(&self) -> Option<&str> {
        self.return_type.as_deref()
    }

    /// Which roles may call it; `rendererOnly` when undeclared.
    pub fn allowed_callers(&self) -> AllowedCallers {
        self.allowed_callers
    }

    /// Whether calling it needs a user activation.
    pub fn requires_user_activation(&self) -> bool {
        self.requires_user_activation
    }

    /// Whether the agent may call it directly (`callRendererFunction`).
    pub fn is_agent_callable(&self) -> bool {
        !self.requires_user_activation && self.allowed_callers.admits_agent()
    }

    /// Whether the protocol, not the catalog, defines it (such as `@index`).
    pub fn is_system(&self) -> bool {
        self.system
    }

    /// The function's description.
    pub fn description(&self) -> Option<&str> {
        self.schema.get("description").and_then(Value::as_str)
    }
}

/// A loaded catalog document with its common types.
///
/// Everything the compiler, decompiler and prompt generator ask about the
/// catalog. Ported from upstream's TypeScript `CatalogSchemaHelper`.
#[derive(Debug, Clone)]
pub struct Catalog {
    id: String,
    version: CatalogVersion,
    doc: Value,
    common: Value,
    components: Vec<Component>,
    component_index: HashMap<String, usize>,
    functions: Vec<Function>,
    function_index: HashMap<String, usize>,
    system_functions: Vec<Function>,
}

impl Catalog {
    /// Parses and loads a catalog document from JSON text.
    pub fn from_json_str(text: &str) -> Result<Catalog, CatalogError> {
        let doc: Value =
            serde_json::from_str(text).map_err(|e| CatalogError::InvalidJson(e.to_string()))?;
        Catalog::load(doc, LoadOptions::default())
    }

    /// Loads a catalog document with default options.
    pub fn from_json(doc: Value) -> Result<Catalog, CatalogError> {
        Catalog::load(doc, LoadOptions::default())
    }

    /// Loads a catalog document (v0.9, v0.9.1 or v1.0).
    pub fn load(doc: Value, options: LoadOptions) -> Result<Catalog, CatalogError> {
        let Some(map) = doc.as_object() else {
            return Err(CatalogError::NotAnObject);
        };
        let id = options
            .catalog_id
            .clone()
            .or_else(|| {
                ["catalogId", "$id", "id"]
                    .iter()
                    .find_map(|k| map.get(*k).and_then(Value::as_str))
                    .map(str::to_owned)
            })
            .filter(|id| !id.is_empty())
            .ok_or(CatalogError::MissingId)?;
        let version = match options.protocol_version {
            Some(v) => v,
            None => match map.get("protocolVersion") {
                None | Some(Value::Null) => CatalogVersion::DEFAULT,
                Some(Value::String(raw)) => CatalogVersion::parse(raw)
                    .ok_or_else(|| CatalogError::UnsupportedVersion(raw.clone()))?,
                Some(other) => return Err(CatalogError::UnsupportedVersion(other.to_string())),
            },
        };
        if version.at_least_v1() {
            check_v1_refs(&doc, &mut String::new())?;
        }
        let common = options
            .common_types
            .unwrap_or_else(|| common_types(version).clone());

        let mut catalog = Catalog {
            id,
            version,
            doc,
            common,
            components: Vec::new(),
            component_index: HashMap::new(),
            functions: Vec::new(),
            function_index: HashMap::new(),
            system_functions: Vec::new(),
        };
        let (components, functions, system_functions) = {
            let schemas = catalog.schemas();
            let map = catalog.doc.as_object().expect("checked object");
            let permitted_components = permitted_names(map, "anyComponent", "#/components/");
            let permitted_functions = permitted_names(map, "anyFunction", "#/functions/");

            if version.at_least_v1() {
                check_metadata(map.get("metadata"), "catalog")?;
            }
            let mut components = Vec::new();
            if let Some(Value::Object(raw)) = map.get("components") {
                for (name, schema) in raw {
                    if version.at_least_v1() && schema.is_object() {
                        check_v1_component(name, schema)?;
                    }
                    if permitted_components
                        .as_ref()
                        .is_some_and(|p| !p.contains(name))
                        || !schema.is_object()
                    {
                        continue;
                    }
                    components.push(read_component(&schemas, name, schema));
                }
            }

            let mut functions = Vec::new();
            let mut add_function = |name: &str, schema: &Value| -> Result<(), CatalogError> {
                if !schema.is_object() {
                    return Ok(());
                }
                let function = read_function(&schemas, name, schema, false)?;
                if version.at_least_v1() {
                    check_v1_function(&function)?;
                }
                if version.at_least_v1()
                    && function.requires_user_activation
                    && function.allowed_callers != AllowedCallers::RendererOnly
                {
                    return Err(CatalogError::UserActivationCallers {
                        function: name.to_owned(),
                        callers: function.allowed_callers.as_str().to_owned(),
                    });
                }
                if permitted_functions
                    .as_ref()
                    .is_none_or(|p| p.contains(name))
                {
                    functions.push(function);
                }
                Ok(())
            };
            match map.get("functions") {
                Some(Value::Object(raw)) => {
                    for (name, schema) in raw {
                        add_function(name, schema)?;
                    }
                }
                Some(Value::Array(raw)) => {
                    for schema in raw {
                        if let Some(name) = schema.get("name").and_then(Value::as_str) {
                            add_function(name, schema)?;
                        }
                    }
                }
                _ => {}
            }

            let mut system_functions = Vec::new();
            for (name, schema) in system_function_schemas(&catalog.common) {
                system_functions.push(read_function(&schemas, &name, &schema, true)?);
            }
            (components, functions, system_functions)
        };
        catalog.component_index = index_of(components.iter().map(Component::name));
        catalog.function_index = index_of(functions.iter().map(Function::name));
        catalog.components = components;
        catalog.functions = functions;
        catalog.system_functions = system_functions;
        Ok(catalog)
    }

    pub(crate) fn schemas(&self) -> Schemas<'_> {
        Schemas {
            doc: &self.doc,
            common: &self.common,
            path_key: self.path_key(),
        }
    }

    /// The catalog ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The protocol version the catalog is read with.
    pub fn version(&self) -> CatalogVersion {
        self.version
    }

    /// The catalog document as written.
    pub fn document(&self) -> &Value {
        &self.doc
    }

    /// The `common_types.json` the catalog is read with.
    pub fn common_types(&self) -> &Value {
        &self.common
    }

    /// The wire key of a data binding: `@path` on v1.0, `path` below.
    pub fn path_key(&self) -> &'static str {
        if self.version.at_least_v1() {
            "@path"
        } else {
            "path"
        }
    }

    /// The wire key of a function call: `@call` on v1.0, `call` below.
    pub fn call_key(&self) -> &'static str {
        if self.version.at_least_v1() {
            "@call"
        } else {
            "call"
        }
    }

    // ---- components ------------------------------------------------------

    /// The components, in catalog order.
    pub fn components(&self) -> &[Component] {
        &self.components
    }

    /// A component by name.
    pub fn component(&self, name: &str) -> Option<&Component> {
        self.component_index.get(name).map(|&i| &self.components[i])
    }

    /// Whether the catalog defines component `name`.
    pub fn has_component(&self, name: &str) -> bool {
        self.component_index.contains_key(name)
    }

    /// A component's Express arguments in positional order (empty if unknown).
    pub fn component_properties(&self, name: &str) -> &[String] {
        self.component(name).map_or(&[], Component::properties)
    }

    /// A component's required properties (empty if unknown).
    pub fn component_required(&self, name: &str) -> &[String] {
        self.component(name).map_or(&[], Component::required)
    }

    /// The check-rule property of a component, if it is checkable.
    pub fn check_rule_property(&self, name: &str) -> Option<&str> {
        self.component(name)?.check_rule_property()
    }

    /// Whether a component is checkable.
    pub fn is_checkable(&self, name: &str) -> bool {
        self.check_rule_property(name).is_some()
    }

    /// The wire key a component's check rules are written under: the
    /// envelope key (`@checks` on v1.0, `checks` below) for the standard
    /// `checks`, else the custom check-rule property's own name.
    pub fn check_rule_wire_key(&self, name: &str) -> Option<String> {
        let prop = self.check_rule_property(name)?;
        Some(if prop == "checks" {
            envelope_key("checks", self.version.at_least_v1())
        } else {
            prop.to_owned()
        })
    }

    /// A property's schema, local `$ref`s dereferenced.
    pub fn property_schema(&self, component: &str, property: &str) -> Option<&Value> {
        self.component(component)?.property_schema(property)
    }

    /// The allowed enum values of a property.
    pub fn property_enum(&self, component: &str, property: &str) -> Option<&[Value]> {
        self.component(component)?.property_enum(property)
    }

    /// Whether a property is required.
    pub fn is_required(&self, component: &str, property: &str) -> bool {
        self.component(component)
            .is_some_and(|c| c.is_required(property))
    }

    /// Whether a property is static: its schema is known and admits no data
    /// binding. `None` when the component or property is unknown.
    pub fn is_static(&self, component: &str, property: &str) -> Option<bool> {
        let schema = self.component(component)?.argument_schema(property)?;
        Some(!self.admits_path(schema))
    }

    /// What a property is for: a child id, a child list or an action.
    pub fn property_slot_kind(&self, component: &str, property: &str) -> Option<SlotKind> {
        self.slot_kind(self.property_schema(component, property)?)
    }

    /// Whether a property is an `Action` slot.
    pub fn is_action_property(&self, component: &str, property: &str) -> bool {
        self.property_schema(component, property)
            .is_some_and(|s| self.is_action_slot(s))
    }

    /// Whether a property expects a list of option objects (`label` + `value`).
    pub fn expects_option_objects_property(&self, component: &str, property: &str) -> bool {
        self.property_schema(component, property)
            .is_some_and(|s| self.expects_option_objects(s))
    }

    /// The schema at a position inside a property's value: `path` walks into
    /// array `items` and object `properties`. `None` when the schema declares
    /// nothing there (or the component or property is unknown).
    pub fn schema_at(
        &self,
        component: &str,
        property: &str,
        path: &[Segment<'_>],
    ) -> Option<&Value> {
        let schema = self.component(component)?.argument_schema(property)?;
        self.schemas().schema_at(schema, path)
    }

    /// Whether a `$path` is allowed at a position inside a property's value.
    ///
    /// `Some(true)` / `Some(false)` when the schema declares the position;
    /// `None` when it does not (callers treat that as allowed). For eui's
    /// `Menu`, `items[].checked` is bindable and `items[].label` is not.
    pub fn admits_path_at(
        &self,
        component: &str,
        property: &str,
        path: &[Segment<'_>],
    ) -> Option<bool> {
        self.schema_at(component, property, path)
            .map(|s| self.admits_path(s))
    }

    /// [`Catalog::admits_path_at`], with an undeclared position allowed.
    pub fn is_bindable_at(&self, component: &str, property: &str, path: &[Segment<'_>]) -> bool {
        self.admits_path_at(component, property, path)
            .unwrap_or(true)
    }

    // ---- functions -------------------------------------------------------

    /// The catalog's own functions, in catalog order.
    pub fn functions(&self) -> &[Function] {
        &self.functions
    }

    /// The protocol's system functions (such as v1.0's `@index`).
    pub fn system_functions(&self) -> &[Function] {
        &self.system_functions
    }

    /// Whether the catalog itself defines function `name`.
    pub fn has_function(&self, name: &str) -> bool {
        self.function_index.contains_key(name)
    }

    /// A catalog or system function by name.
    pub fn function(&self, name: &str) -> Option<&Function> {
        self.function_index
            .get(name)
            .map(|&i| &self.functions[i])
            .or_else(|| self.system_functions.iter().find(|f| f.name == name))
    }

    /// A function's argument names in positional order (empty if unknown).
    pub fn function_arguments(&self, name: &str) -> &[String] {
        self.function(name).map_or(&[], Function::arguments)
    }

    /// A function's required arguments (empty if unknown).
    pub fn function_required(&self, name: &str) -> &[String] {
        self.function(name).map_or(&[], Function::required)
    }

    /// The names of the catalog functions the agent may call directly, in
    /// catalog order. Always empty below v1.0.
    pub fn agent_callable_functions(&self) -> Vec<&str> {
        if !self.version.at_least_v1() {
            return Vec::new();
        }
        self.functions
            .iter()
            .filter(|f| f.is_agent_callable())
            .map(Function::name)
            .collect()
    }

    // ---- schema queries --------------------------------------------------

    /// Whether a schema admits a data binding (`$path`). Never descends into
    /// `items` or property values; see [`Catalog::admits_path_at`].
    pub fn admits_path(&self, schema: &Value) -> bool {
        self.schemas().admits_path(schema)
    }

    /// Whether a schema accepts a number.
    pub fn accepts_number(&self, schema: &Value) -> bool {
        self.schemas().accepts_number(schema)
    }

    /// The schema of an array's items.
    pub fn resolve_item_schema<'x>(&'x self, schema: &'x Value) -> Option<&'x Value> {
        self.schemas().resolve_at(schema, Segment::Item)
    }

    /// The schema of an object's property (else its `additionalProperties`).
    pub fn resolve_property_schema<'x>(
        &'x self,
        schema: &'x Value,
        name: &str,
    ) -> Option<&'x Value> {
        self.schemas().resolve_at(schema, Segment::Property(name))
    }

    /// The schema at a position inside a value.
    pub fn resolve_schema_at<'x>(
        &'x self,
        schema: &'x Value,
        path: &[Segment<'_>],
    ) -> Option<&'x Value> {
        self.schemas().schema_at(schema, path)
    }

    /// Resolves a schema's chain of local `#/...` `$ref`s, laying its own keys
    /// (such as `description`) over the target. A followed reference to a
    /// standard common type name is kept as `$ref`.
    pub fn dereference_local_schema(&self, schema: &Value) -> Option<Value> {
        self.schemas().dereference(schema)
    }

    /// Whether a schema is an `Action` slot.
    pub fn is_action_slot(&self, schema: &Value) -> bool {
        self.schemas().is_action_slot(schema)
    }

    /// Whether a schema expects a list of option objects.
    pub fn expects_option_objects(&self, schema: &Value) -> bool {
        self.schemas().expects_option_objects(schema)
    }

    /// What a schema is for: a child id, a child list or an action.
    pub fn slot_kind(&self, schema: &Value) -> Option<SlotKind> {
        self.schemas().slot_kind(schema)
    }

    /// Whether a schema holds component ids (`ComponentId`, `Child`, `ChildList`).
    pub fn is_component_reference(&self, schema: &Value) -> bool {
        self.schemas().is_component_reference(schema)
    }
}

/// From v1.0 a component's name and the properties it declares are UAX #31
/// identifiers, and its static `metadata` holds only `extensions`.
fn check_v1_component(name: &str, schema: &Value) -> Result<(), CatalogError> {
    if !is_uax31_identifier(name) {
        return Err(CatalogError::InvalidIdentifier {
            kind: "component",
            name: name.to_owned(),
            owner: None,
        });
    }
    if let Some(Value::Object(properties)) = schema.get("properties") {
        // The standard mixins are spelled with a leading `@` from v1.0.
        let invalid = properties.keys().find(|p| {
            !is_uax31_identifier(p)
                && !p
                    .strip_prefix('@')
                    .is_some_and(|bare| ENVELOPE_MIXIN_NAMES.contains(&bare))
        });
        if let Some(property) = invalid {
            return Err(CatalogError::InvalidIdentifier {
                kind: "property",
                name: property.clone(),
                owner: Some(format!("component '{name}'")),
            });
        }
    }
    check_metadata(schema.get("metadata"), &format!("component '{name}'"))
}

/// From v1.0 a function's name and its arguments are UAX #31 identifiers.
fn check_v1_function(function: &Function) -> Result<(), CatalogError> {
    let name = function.name();
    if !is_uax31_identifier(name) {
        return Err(CatalogError::InvalidIdentifier {
            kind: "function",
            name: name.to_owned(),
            owner: None,
        });
    }
    if let Some(argument) = function.arguments().iter().find(|a| !is_uax31_identifier(a)) {
        return Err(CatalogError::InvalidIdentifier {
            kind: "argument",
            name: argument.clone(),
            owner: Some(format!("function '{name}'")),
        });
    }
    check_metadata(
        function.schema().get("metadata"),
        &format!("function '{name}'"),
    )
}

/// A `metadata` envelope is an object whose only key is `extensions`, an
/// object keyed by UAX #31 identifiers.
fn check_metadata(metadata: Option<&Value>, owner: &str) -> Result<(), CatalogError> {
    let invalid = |reason: String| CatalogError::InvalidMetadata {
        owner: owner.to_owned(),
        reason,
    };
    let metadata = match metadata {
        None | Some(Value::Null) => return Ok(()),
        Some(Value::Object(metadata)) => metadata,
        Some(other) => {
            return Err(invalid(format!(
                "Expected {owner} metadata to be an object, got {}.",
                json_type(other)
            )));
        }
    };
    if let Some(key) = metadata.keys().find(|k| *k != "extensions") {
        return Err(invalid(format!(
            "Unexpected property '{key}' in {owner} metadata; only 'extensions' is allowed."
        )));
    }
    match metadata.get("extensions") {
        None => Ok(()),
        Some(Value::Object(extensions)) => match extensions.keys().find(|k| !is_uax31_identifier(k)) {
            Some(key) => Err(invalid(format!(
                "Invalid UAX #31 extension identifier '{key}' in {owner} metadata.extensions."
            ))),
            None => Ok(()),
        },
        Some(other) => Err(invalid(format!(
            "Expected {owner} metadata.extensions to be an object, got {}.",
            json_type(other)
        ))),
    }
}

fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn index_of<'n>(names: impl Iterator<Item = &'n str>) -> HashMap<String, usize> {
    names.enumerate().map(|(i, n)| (n.to_owned(), i)).collect()
}

/// The names `$defs.<def>.oneOf` permits, if it lists any.
fn permitted_names(doc: &Map<String, Value>, def: &str, prefix: &str) -> Option<HashSet<String>> {
    let one_of = doc.get("$defs")?.get(def)?.get("oneOf")?.as_array()?;
    Some(
        one_of
            .iter()
            .filter_map(|item| item.get("$ref")?.as_str()?.strip_prefix(prefix))
            .map(|raw| raw.replace("~1", "/").replace("~0", "~"))
            .collect(),
    )
}

fn escape_pointer_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// Rejects a v1.0 `$ref` into any document other than the catalog itself or
/// `common_types.json`.
fn check_v1_refs(node: &Value, pointer: &mut String) -> Result<(), CatalogError> {
    let len = pointer.len();
    match node {
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                pointer.push_str(&format!("/{i}"));
                check_v1_refs(item, pointer)?;
                pointer.truncate(len);
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                pointer.push('/');
                pointer.push_str(&escape_pointer_token(k));
                if let (true, Value::String(r)) = (k == "$ref", v) {
                    if !is_supported_v1_ref(r) {
                        return Err(CatalogError::ExternalRef {
                            reference: r.clone(),
                            pointer: pointer.clone(),
                        });
                    }
                }
                check_v1_refs(v, pointer)?;
                pointer.truncate(len);
            }
        }
        _ => {}
    }
    Ok(())
}

/// The sub-schemas of a component in the order its properties are read.
///
/// The component's own schema first, so its own definition of a property
/// overrides an inherited one. Then the `allOf` entries that declare
/// `component` (the specific schemas), then the other entries (the mixins),
/// each expanded depth first: a local `#/...` reference is resolved against
/// the catalog and the entries of its own `allOf` follow it. References into
/// `common_types.json` are kept as they are.
fn ordered_component_sub_schemas<'a>(schemas: &Schemas<'a>, schema: &'a Value) -> Vec<&'a Value> {
    let mut specific = Vec::new();
    let mut inherited = Vec::new();
    for sub in schema
        .get("allOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let sub = match schemas.resolve_local_ref(sub) {
            Some(resolved) if resolved.is_object() => resolved,
            _ => sub,
        };
        if !sub.is_object() {
            continue;
        }
        if sub
            .get("properties")
            .and_then(Value::as_object)
            .is_some_and(|p| p.contains_key("component"))
        {
            specific.push(sub);
        } else {
            inherited.push(sub);
        }
    }

    let mut expanded = vec![schema];
    let mut visited: HashSet<*const Value> = HashSet::from([schema as *const Value]);
    fn expand<'a>(
        schemas: &Schemas<'a>,
        node: &'a Value,
        expanded: &mut Vec<&'a Value>,
        visited: &mut HashSet<*const Value>,
    ) {
        let target = match schemas.resolve_local_ref(node) {
            Some(resolved) if resolved.is_object() => resolved,
            _ => node,
        };
        if !visited.insert(target as *const Value) {
            return;
        }
        expanded.push(target);
        for child in target
            .get("allOf")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if child.is_object() {
                expand(schemas, child, expanded, visited);
            }
        }
    }
    for s in specific.into_iter().chain(inherited) {
        expand(schemas, s, &mut expanded, &mut visited);
    }
    expanded
}

fn check_rule_def_name(reference: &str) -> Option<&str> {
    common_def_name(reference).or_else(|| local_def_name(reference))
}

fn is_standard_catalog_id_schema(schema: &Value) -> bool {
    schema.get("description").and_then(Value::as_str) == Some(STANDARD_CATALOG_ID_DESCRIPTION)
}

fn is_standard_metadata_schema(schema: &Value) -> bool {
    schema.get("description").and_then(Value::as_str) == Some(STANDARD_METADATA_DESCRIPTION)
        || schema
            .get("properties")
            .and_then(|p| p.get("extensions"))
            .and_then(common_def_of)
            == Some("Extensions")
}

fn is_standard_accessibility_schema(schema: &Value) -> bool {
    common_def_of(schema) == Some("AccessibilityAttributes")
}

fn read_component(schemas: &Schemas<'_>, name: &str, schema: &Value) -> Component {
    let subs = ordered_component_sub_schemas(schemas, schema);

    // (a) An own property `{type: array, items: {$ref: <common CheckRule>}}`.
    let mut own_check: Option<String> = None;
    let mut inherited_check_schema: Option<Value> = None;
    'own: for sub in &subs {
        if sub.get("$ref").is_some() {
            continue;
        }
        if let Some(Value::Object(props)) = sub.get("properties") {
            for (pk, pv) in props {
                if Schemas::is_check_rule_array(pv) {
                    if pk == "@checks" {
                        own_check = Some("checks".into());
                        inherited_check_schema = Some(pv.clone());
                    } else {
                        own_check = Some(pk.clone());
                    }
                    break 'own;
                }
            }
        }
    }

    // (b) A check-rule list inherited from an `allOf` reference into the common types.
    let mut inherited_check: Option<String> = None;
    if own_check.is_none() {
        'inherited: for sub in &subs {
            let Some(def) = sub
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(common_def_name)
            else {
                continue;
            };
            let Some(Value::Object(props)) =
                schemas.common_def(def).and_then(|d| d.get("properties"))
            else {
                continue;
            };
            for (pk, pv) in props {
                let is_list = pv.get("type").is_some_and(|t| t == "array")
                    && pv
                        .get("items")
                        .filter(|i| i.is_object())
                        .and_then(|i| i.get("$ref"))
                        .and_then(Value::as_str)
                        .and_then(check_rule_def_name)
                        == Some("CheckRule");
                if is_list {
                    inherited_check = Some(if pk == "@checks" {
                        "checks".into()
                    } else {
                        pk.clone()
                    });
                    inherited_check_schema = Some(pv.clone());
                    break 'inherited;
                }
            }
        }
    }

    // Properties: the first definition wins and keeps its position.
    let mut props: Map<String, Value> = Map::new();
    let mut required: Vec<String> = Vec::new();
    let mut enums = HashMap::new();
    let mut property_schemas = HashMap::new();
    for sub in &subs {
        if let Some(Value::Object(sub_props)) = sub.get("properties") {
            for (pk, pv) in sub_props {
                if props.contains_key(pk) {
                    continue;
                }
                props.insert(pk.clone(), pv.clone());
                if let Some(values) = schemas.find_enum(pv) {
                    enums.insert(pk.clone(), values);
                }
                if let Some(deref) = schemas.dereference(pv) {
                    property_schemas.insert(pk.clone(), deref);
                }
            }
        }
        for r in sub
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !required.iter().any(|x| x == r) {
                required.push(r.to_owned());
            }
        }
    }

    let standard_checkable =
        own_check.as_deref() == Some("checks") || inherited_check.as_deref() == Some("checks");
    let custom_envelope: Vec<String> = ENVELOPE_MIXIN_NAMES
        .iter()
        .filter(|&&mixin| {
            let Some(s) = props.get(mixin) else {
                return false;
            };
            match mixin {
                "checks" => !Schemas::is_check_rule_array(s),
                "catalogId" => !is_standard_catalog_id_schema(s),
                "metadata" => !is_standard_metadata_schema(s),
                "accessibility" => !is_standard_accessibility_schema(s),
                _ => true,
            }
        })
        .map(|m| (*m).to_owned())
        .collect();
    let is_custom = |n: &str| custom_envelope.iter().any(|c| c == n);

    let mut excluded: HashSet<String> = ["component", "id"]
        .into_iter()
        .map(String::from)
        .chain(ENVELOPE_MIXIN_NAMES.iter().map(|m| format!("@{m}")))
        .collect();
    for mixin in ["catalogId", "metadata", "accessibility"] {
        if !is_custom(mixin) {
            excluded.insert(mixin.into());
        }
    }
    if standard_checkable && !is_custom("checks") {
        excluded.insert("checks".into());
    }
    let mut properties: Vec<String> = props
        .keys()
        .filter(|k| !excluded.contains(*k))
        .cloned()
        .collect();
    if standard_checkable && !is_custom("checks") {
        properties.push("checks".into());
    } else if let Some(prop) = &inherited_check {
        if !is_custom(prop) && !properties.contains(prop) {
            properties.push(prop.clone());
        }
    }

    let description = schema
        .get("description")
        .and_then(Value::as_str)
        .or_else(|| {
            schema
                .get("allOf")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find_map(|sub| {
                    let sub = match local_ref(sub) {
                        Some(_) => schemas.resolve_local_ref(sub).unwrap_or(sub),
                        None => sub,
                    };
                    sub.get("description").and_then(Value::as_str)
                })
        })
        .map(str::to_owned);

    Component {
        name: name.to_owned(),
        schema: schema.clone(),
        properties,
        required,
        check_rule_property: own_check.or(inherited_check),
        inherited_check_rule_schema: inherited_check_schema,
        custom_envelope,
        enums,
        property_schemas,
        description,
    }
}

/// A function's sub-schemas: its own schema, its inline `allOf` entries,
/// then its local `#/...` `allOf` references, resolved.
fn function_sub_schemas<'a>(schemas: &Schemas<'a>, schema: &'a Value) -> Vec<&'a Value> {
    let mut subs = vec![schema];
    let mut locals = Vec::new();
    for sub in schema
        .get("allOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if local_ref(sub).is_some() {
            locals.push(
                schemas
                    .resolve_local_ref(sub)
                    .filter(|r| r.is_object())
                    .unwrap_or(sub),
            );
        } else {
            subs.push(sub);
        }
    }
    subs.extend(locals);
    subs.retain(|s| s.is_object());
    subs
}

fn read_function(
    schemas: &Schemas<'_>,
    name: &str,
    schema: &Value,
    system: bool,
) -> Result<Function, CatalogError> {
    let mut arguments: Vec<String> = Vec::new();
    let mut argument_schemas: Vec<(String, Value)> = Vec::new();
    let mut required = Vec::new();
    for sub in function_sub_schemas(schemas, schema) {
        let Some(args) = sub.get("properties").and_then(|p| p.get("args")) else {
            continue;
        };
        if !args.is_object() {
            continue;
        }
        if let Some(Value::Object(arg_props)) = args.get("properties") {
            for (k, v) in arg_props {
                if !arguments.contains(k) {
                    arguments.push(k.clone());
                    argument_schemas.push((k.clone(), v.clone()));
                }
            }
        }
        required.extend(
            args.get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        );
    }
    let allowed_callers = match schema.get("allowedCallers") {
        None | Some(Value::Null) => AllowedCallers::default(),
        Some(Value::String(raw)) => {
            AllowedCallers::parse(raw).ok_or_else(|| CatalogError::InvalidFunction {
                function: name.to_owned(),
                reason: format!("unknown allowedCallers '{raw}'"),
            })?
        }
        Some(other) => {
            return Err(CatalogError::InvalidFunction {
                function: name.to_owned(),
                reason: format!("allowedCallers must be a string, got {other}"),
            });
        }
    };
    Ok(Function {
        name: name.to_owned(),
        schema: schema.clone(),
        arguments,
        required,
        argument_schemas,
        return_type: schema
            .get("returnType")
            .and_then(Value::as_str)
            .map(str::to_owned),
        allowed_callers,
        requires_user_activation: schema
            .get("requiresUserActivation")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        system,
    })
}
