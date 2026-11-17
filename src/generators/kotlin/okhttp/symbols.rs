//! Kotlin-only symbol planning for union declarations emitted beside model
//! classes.
//!
//! The shared IR owns model identities, while this module owns the union
//! declarations Kotlin needs to implement those models. In particular, Gson
//! union adapters live in the same package as model classes, so their names must
//! be allocated against the complete model namespace before emission starts.

use std::collections::{HashMap, HashSet};

use heck::ToPascalCase as _;

use crate::ir::types::{IrSchemaKind, IrSpec};

#[derive(Debug)]
pub(super) struct KotlinSymbols {
    adapter_names: HashMap<String, String>,
}

impl KotlinSymbols {
    pub(super) fn from_ir<'a>(
        ir: &IrSpec,
        extra_model_names: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let model_names = ir.schemas.values().map(|schema| schema.name.as_str());
        let adapter_bases = ir.schemas.values().filter_map(|schema| {
            matches!(
                schema.kind,
                IrSchemaKind::Union(_) | IrSchemaKind::TaggedUnion(_)
            )
            .then_some(schema.name.as_str())
        });
        let mut used = model_names
            .map(|name| name.to_pascal_case())
            .collect::<HashSet<_>>();
        for name in extra_model_names {
            used.insert(name.to_pascal_case());
        }
        Self::from_reserved_names(used, adapter_bases)
    }

    pub(super) fn adapter_name(&self, model_name: &str) -> &str {
        self.adapter_names
            .get(model_name)
            .unwrap_or_else(|| panic!("missing Kotlin adapter symbol for {model_name}"))
    }

    #[cfg(test)]
    fn from_names<'a>(
        model_names: impl IntoIterator<Item = &'a str>,
        adapter_bases: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let used = model_names
            .into_iter()
            .map(|name| name.to_pascal_case())
            .collect::<HashSet<_>>();
        Self::from_reserved_names(used, adapter_bases)
    }

    fn from_reserved_names<'a>(
        mut used: HashSet<String>,
        adapter_bases: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let mut adapter_names = HashMap::new();
        for model_name in adapter_bases {
            let base = format!("{}JsonAdapter", model_name.to_pascal_case());
            let name = allocate(&mut used, &base);
            adapter_names.insert(model_name.to_string(), name);
        }
        Self { adapter_names }
    }
}

fn allocate(used: &mut HashSet<String>, base: &str) -> String {
    if used.insert(base.to_pascal_case()) {
        return base.to_string();
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{base}{suffix}");
        if used.insert(candidate.to_pascal_case()) {
            return candidate;
        }
        suffix += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::KotlinSymbols;

    #[test]
    fn adapter_avoids_authored_model_after_pascal_normalization() {
        let symbols = KotlinSymbols::from_names(
            ["Values", "valuesAdditionalPropertyJsonAdapter"],
            ["ValuesAdditionalProperty"],
        );
        assert_eq!(
            symbols.adapter_name("ValuesAdditionalProperty"),
            "ValuesAdditionalPropertyJsonAdapter2"
        );
    }

    #[test]
    fn adapter_names_are_unique_across_union_models() {
        let symbols = KotlinSymbols::from_names(["ChoiceJsonAdapter2"], ["Choice", "choice"]);
        assert_eq!(symbols.adapter_name("Choice"), "ChoiceJsonAdapter");
        assert_eq!(symbols.adapter_name("choice"), "ChoiceJsonAdapter3");
    }

    #[test]
    fn non_conflicting_adapter_keeps_existing_name() {
        let symbols = KotlinSymbols::from_names(["Choice"], ["Choice"]);
        assert_eq!(symbols.adapter_name("Choice"), "ChoiceJsonAdapter");
    }
}
