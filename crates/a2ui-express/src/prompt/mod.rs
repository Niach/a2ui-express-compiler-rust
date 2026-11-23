//! Prompt generator: the system-prompt section that teaches a model Express.
//!
//! For the active catalogs it writes the Express output contract (the rules
//! text, trimmed to the allowed messages), the multi-catalog rules when
//! several catalogs are active, the protocol's system functions, one
//! positional signature per component and function of each catalog, the
//! catalog's own instructions, and the examples, rewritten from A2UI JSON
//! into Express through the [decompiler](crate::decompile).
//!
//! Ported from upstream's TypeScript `prompt-generator.ts` (pinned in
//! `vendor/a2ui/UPSTREAM`, after a2ui-project/a2ui#4184); the behaviour is
//! fixed by `vendor/a2ui/conformance/agent/express/prompt_generator.yaml`.
//!
//! ```
//! use a2ui_express::prompt::{generate_prompt, PromptOptions};
//! use a2ui_express::schema::{Catalog, CatalogSet};
//! use serde_json::json;
//!
//! let catalog = Catalog::from_json(json!({
//!     "catalogId": "demo",
//!     "protocolVersion": "1.0",
//!     "components": {
//!         "Badge": {"properties": {
//!             "component": {"const": "Badge"},
//!             "text": {"$ref": "common_types.json#/$defs/DynamicString"},
//!             "variant": {"type": "string", "enum": ["info", "warn"]}
//!         }, "required": ["component", "text"]}
//!     }
//! }))
//! .unwrap();
//! let catalogs = CatalogSet::new(vec![catalog]).unwrap();
//! let prompt = generate_prompt(&catalogs, &PromptOptions::default()).unwrap();
//! assert!(prompt.contains("• Badge(text, variant? (static))"));
//! assert!(prompt.contains("  - variant: Must be one of: 'info', 'warn'"));
//! ```

mod examples;
mod rules;
mod signatures;

use serde_json::Value;

use crate::Result;
use crate::schema::{Catalog, CatalogSet, SlotKind, SymbolKind};

/// The heading that introduces the examples.
const EXAMPLES_HEADING: &str = "### Examples:\n\n";

/// What a prompt is generated from, besides the catalogs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PromptOptions {
    /// The surface the agent writes to. Upstream's generator does not read
    /// it; it is kept for parity with the parser's options.
    pub surface_id: String,
    /// Examples, one list of A2UI messages per turn, in the order given.
    /// They belong to the first catalog: a message or component that names
    /// no catalog is read with it.
    pub examples: Vec<Vec<Value>>,
    /// The message types the agent may write (`createSurface`,
    /// `updateComponents`, `updateDataModel`, `deleteSurface`,
    /// `callRendererFunction`, or their `…Message` class names). The rules
    /// for the others are left out. `None` allows all of them.
    pub allowed_messages: Option<Vec<String>>,
}

/// Generates the Express prompt section for the active catalogs.
///
/// Fails only when an example cannot be decompiled (it names a component,
/// function or catalog the catalogs do not define).
pub fn generate_prompt(catalogs: &CatalogSet, options: &PromptOptions) -> Result<String> {
    Generator { catalogs, options }.generate()
}

/// The functions the agent may call standalone (`callRendererFunction`),
/// across the catalogs, each once. Empty below v1.0.
pub fn agent_callable_functions(catalogs: &CatalogSet) -> Vec<String> {
    catalogs
        .agent_callable_functions()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

struct Generator<'a> {
    catalogs: &'a CatalogSet,
    options: &'a PromptOptions,
}

impl Generator<'_> {
    fn generate(&self) -> Result<String> {
        let instructions = self
            .catalogs
            .catalogs()
            .iter()
            .map(|c| self.catalog_instructions(c))
            .collect::<Vec<_>>()
            .join("\n\n");
        let examples = self.examples()?;
        let examples = if examples.is_empty() {
            String::new()
        } else {
            format!("{EXAMPLES_HEADING}{examples}")
        };
        Ok([self.base_rules(), instructions, examples]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"))
    }

    fn allows(&self, names: &[&str]) -> bool {
        self.options
            .allowed_messages
            .as_ref()
            .is_none_or(|allowed| allowed.iter().any(|a| names.contains(&a.as_str())))
    }

    /// The output contract and grammar rules.
    fn base_rules(&self) -> String {
        let at_least_v1 = self.catalogs.version().at_least_v1();
        let mut rules = rules::rules(at_least_v1);
        let mut wants_components_or_data = true;
        let mut wants_components = true;
        let callable = if at_least_v1 {
            agent_callable_functions(self.catalogs)
        } else {
            Vec::new()
        };
        let call_renderer_allowed =
            self.allows(&["callRendererFunction", "CallRendererFunctionMessage"]);

        if self.options.allowed_messages.is_some() {
            if !self.allows(&["deleteSurface", "DeleteSurfaceMessage"]) {
                rules = rules.replacen(rules::DELETE_RULE, "", 1);
            }
            let writes_data = self.allows(&["updateDataModel", "UpdateDataModelMessage"]);
            if !writes_data {
                rules = rules.replacen(rules::DATA_RULE, "", 1);
            }
            // Rule 15 teaches `surface(...)`, which component and data
            // statements both need. Its `root = ...` line applies only to
            // components, and the rule goes when neither is allowed.
            let writes_components = self.allows(&[
                "createSurface",
                "CreateSurfaceMessage",
                "updateComponents",
                "UpdateComponentsMessage",
            ]);
            wants_components_or_data = writes_components || writes_data;
            wants_components = writes_components;
            if !writes_components {
                let root_line = format!("\n    {}", rules::ROOT_EXAMPLE_LINE);
                if let Some(stripped) = rules.strip_suffix(&root_line) {
                    rules = stripped.to_owned();
                }
                if !writes_data {
                    if let Some(stripped) = rules.strip_suffix(rules::SURFACE_RULE) {
                        rules = stripped.to_owned();
                    }
                }
            }
        }

        let has_callable_rule = call_renderer_allowed && !callable.is_empty();
        if has_callable_rule {
            let listed = callable
                .iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", ");
            let call_rule = format!(
                "To have the renderer run a function right away, output the call as a standalone command (no variable assignment):\n    {}(...)\n    Only these functions can be called this way: {listed}.",
                callable[0]
            );
            rules = rules.replacen(
                rules::STATIC_RULE_HEAD,
                &format!("\n\n13. {call_rule}\n\n14. Static properties:"),
                1,
            );
        }

        if let Some(root) = self.example_root_component() {
            rules = rules.replacen(rules::ROOT_EXAMPLE_LINE, &format!("root = {root}(...)"), 1);
        }

        let has_multi_catalog_rules =
            self.catalogs.catalogs().len() > 1 && wants_components_or_data;
        let mut combined = rules;
        if has_multi_catalog_rules {
            combined.push('\n');
            combined.push_str(&self.multi_catalog_rules());
        }
        if self.options.allowed_messages.is_some() || has_callable_rule || has_multi_catalog_rules {
            combined = rules::renumber(&combined);
        }
        if wants_components {
            let system = self.system_function_signatures();
            if !system.is_empty() {
                combined.push_str("\n\n");
                combined.push_str(&system);
            }
        }
        combined
    }

    /// The component rule 15's `root = ...(...)` example names: the first
    /// that holds a single child reference, as a card does, else the first.
    fn example_root_component(&self) -> Option<&str> {
        let catalogs = self.catalogs.catalogs();
        let holds_child = |catalog: &Catalog, component: &crate::schema::Component| {
            component.properties().iter().any(|p| {
                let schema = component.property_schema(p);
                schema.is_some_and(|s| catalog.slot_kind(s) == Some(SlotKind::Child))
                    || (p == "child"
                        && schema.is_some_and(|s| s.get("type").is_some_and(|t| t == "string")))
            })
        };
        catalogs
            .iter()
            .flat_map(|c| c.components().iter().map(move |x| (c, x)))
            .find(|(c, x)| holds_child(c, x))
            .map(|(_, x)| x)
            .or_else(|| catalogs.iter().flat_map(Catalog::components).next())
            .map(|c| c.name())
    }

    fn multi_catalog_rules(&self) -> String {
        let ids: Vec<&str> = self.catalogs.catalogs().iter().map(Catalog::id).collect();
        let example_id = ids.last().copied().unwrap_or_default();
        let listing = ids
            .iter()
            .map(|id| format!("- `{id}`"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut lines = vec![
            String::new(),
            "## Multiple Catalogs".into(),
            String::new(),
            "Several component catalogs are active. Each is identified by its catalog ID, and the signatures below are grouped by catalog:".into(),
            listing,
            String::new(),
            "16. A surface has no catalog of its own: do not name a catalog on a `surface` line. Each component and function call, including checks, actions, nested function calls and inline child components, is found by its name in whichever catalog defines it.".into(),
            String::new(),
            "17. When several catalogs define the same component or function name, you MUST say which one you mean: add a `catalogId` keyword argument to the component or function call, or a `{catalogId: ...}` argument to a check. Do not add `catalogId` to any other name:".into(),
            format!("    widget = ComponentName(..., catalogId=\"{example_id}\")"),
            format!("    action = functionName(..., catalogId=\"{example_id}\")"),
            format!("    ?checkName(..., {{catalogId: \"{example_id}\"}})"),
        ];

        let mut shared = Vec::new();
        for (kind, label) in [
            (SymbolKind::Component, "Component"),
            (SymbolKind::Function, "Function"),
        ] {
            let mut names: Vec<&str> = self
                .catalogs
                .catalogs()
                .iter()
                .flat_map(|c| -> Vec<&str> {
                    match kind {
                        SymbolKind::Component => c.components().iter().map(|x| x.name()).collect(),
                        SymbolKind::Function => c.functions().iter().map(|x| x.name()).collect(),
                    }
                })
                .collect();
            names.sort_unstable();
            names.dedup();
            for name in names {
                let defining = self.catalogs.catalogs_defining(kind, name);
                if defining.len() > 1 {
                    let ids = defining
                        .iter()
                        .map(|id| format!("`{id}`"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    shared.push(format!("- {label} `{name}`: {ids}"));
                }
            }
        }
        if !shared.is_empty() {
            lines.push(String::new());
            lines
                .push("These names are defined in several catalogs and need a `catalogId`:".into());
            lines.extend(shared);
        }
        lines.join("\n")
    }

    /// The protocol's system functions (such as `@index`), listed once.
    fn system_function_signatures(&self) -> String {
        let Some(first) = self.catalogs.catalogs().first() else {
            return String::new();
        };
        let mut names: Vec<&str> = first.system_functions().iter().map(|f| f.name()).collect();
        if names.is_empty() {
            return String::new();
        }
        names.sort_unstable();
        format!(
            "## System Functions\n\nThese functions are defined by the A2UI protocol, not by a catalog. Call them by their `@` name, like any function, and never add a `catalogId` to them:\n{}",
            signatures::functions(first, names)
        )
    }

    /// One catalog's signatures and instructions.
    fn catalog_instructions(&self, catalog: &Catalog) -> String {
        let instructions = catalog
            .document()
            .get("instructions")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|text| examples::rewrite_json_blocks(text, self.catalogs, catalog))
            .filter(|s| !s.is_empty())
            .map(|text| format!("\n\n## Catalog Instructions\n\n{text}"))
            .unwrap_or_default();
        let header = if self.catalogs.catalogs().len() > 1 {
            format!("# Catalog: {}\n\n", catalog.id())
        } else {
            String::new()
        };
        format!(
            "{header}## Positional Component Signatures\n\nUse these exact positional signatures to instantiate components. Do not output property keys:\n{}\n\n## Positional Function Signatures\n\nUse these exact positional signatures to instantiate check rules or logic functions:\n{}{instructions}",
            signatures::components(catalog),
            signatures::catalog_functions(catalog),
        )
    }

    /// The examples, one sentinel-wrapped block per turn.
    fn examples(&self) -> Result<String> {
        let Some(catalog) = self.catalogs.catalogs().first() else {
            return Ok(String::new());
        };
        let mut blocks = Vec::new();
        for turn in &self.options.examples {
            let dsl = examples::decompile_turn(self.catalogs, catalog, turn)?;
            if !dsl.is_empty() {
                blocks.push(examples::wrap(&dsl));
            }
        }
        Ok(blocks.join("\n\n"))
    }
}
