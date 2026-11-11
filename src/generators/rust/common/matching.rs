//! Rust predicates for the shared structural union-selection plan.
//!
//! Private, deduplicated functions keep recursive graphs finite without shipping
//! an interpreter or weakening recursive edges to an unrestricted JSON match.
//! Recursive references memoize results during one immutable JSON selection so
//! overlapping members do not repeatedly walk the same subtree.

use sigil_stitch::error::SigilStitchError;
use sigil_stitch::prelude::{CodeBlock, TypeName, sigil_quote};
use sigil_stitch::spec::{fun_spec::FunSpec, parameter_spec::ParameterSpec};

use crate::generators::union_match::{
    JsonShape, MatchScope, accepts_any, plan_named_shape, selection_shapes,
};
use crate::ir::types::IrSpec;

#[derive(Default)]
pub(super) struct MatchPlan {
    shapes: Vec<JsonShape>,
}

impl MatchPlan {
    fn constant(shape: &JsonShape) -> Option<bool> {
        match shape {
            JsonShape::Any => Some(true),
            JsonShape::Never => Some(false),
            JsonShape::AnyOf(_) | JsonShape::AllOf(_) | JsonShape::Nullable(_)
                if accepts_any(shape) =>
            {
                Some(true)
            }
            // Retained finite facts are scalar; composite enum values are not
            // evidence for choosing a typed converter.
            JsonShape::Literal(serde_json::Value::Array(_) | serde_json::Value::Object(_)) => {
                Some(false)
            }
            _ => None,
        }
    }

    pub(super) fn register(&mut self, shape: &JsonShape, ir: &IrSpec) {
        if Self::constant(shape).is_some() || self.shapes.contains(shape) {
            return;
        }
        self.shapes.push(shape.clone());
        match shape {
            JsonShape::Array(inner) | JsonShape::Map(inner) | JsonShape::Nullable(inner) => {
                self.register(inner, ir);
            }
            JsonShape::MapWithConcreteValue { values, concrete } => {
                self.register(&JsonShape::Map(values.clone()), ir);
                self.register(concrete, ir);
            }
            JsonShape::Object(properties) => {
                for property in properties {
                    self.register(&property.shape, ir);
                }
            }
            JsonShape::ObjectWithAdditional { properties, values } => {
                self.register(&JsonShape::Object(properties.clone()), ir);
                self.register(values, ir);
            }
            JsonShape::AnyOf(children) | JsonShape::AllOf(children) => {
                for child in children {
                    self.register(child, ir);
                }
            }
            JsonShape::Reference { schema, scope, .. } => {
                let shape = plan_named_shape(schema, ir);
                if *scope == MatchScope::Any {
                    self.register(&shape, ir);
                } else {
                    let (concrete, fallback) = selection_shapes(shape);
                    self.register(&concrete, ir);
                    if *scope == MatchScope::Fallback {
                        self.register(&fallback, ir);
                    }
                }
            }
            _ => {}
        }
    }

    fn index(&self, shape: &JsonShape) -> usize {
        self.shapes
            .iter()
            .position(|candidate| candidate == shape)
            .expect("union matcher shape was registered")
    }

    fn symbol(&self, shape: &JsonShape) -> String {
        format!("__nexus_matches_{}", self.index(shape))
    }

    pub(super) fn is_recursive(&self) -> bool {
        self.shapes
            .iter()
            .any(|shape| matches!(shape, JsonShape::Reference { .. }))
    }

    fn recursive(shape: &JsonShape) -> bool {
        if Self::constant(shape).is_some() {
            return false;
        }
        match shape {
            JsonShape::Reference { .. } => true,
            JsonShape::Array(inner) | JsonShape::Map(inner) | JsonShape::Nullable(inner) => {
                Self::recursive(inner)
            }
            JsonShape::MapWithConcreteValue { values, concrete } => {
                Self::recursive(values) || Self::recursive(concrete)
            }
            JsonShape::Object(properties) => properties
                .iter()
                .any(|property| Self::recursive(&property.shape)),
            JsonShape::ObjectWithAdditional { properties, values } => {
                properties
                    .iter()
                    .any(|property| Self::recursive(&property.shape))
                    || Self::recursive(values)
            }
            JsonShape::AnyOf(children) | JsonShape::AllOf(children) => {
                children.iter().any(Self::recursive)
            }
            _ => false,
        }
    }

    pub(super) fn condition(
        &self,
        shape: &JsonShape,
        candidate: CodeBlock,
    ) -> Result<CodeBlock, SigilStitchError> {
        match Self::constant(shape) {
            Some(true) => sigil_quote!(RustLang { true }),
            Some(false) => sigil_quote!(RustLang { false }),
            None => {
                let symbol = self.symbol(shape);
                let recursive = Self::recursive(shape);
                sigil_quote!(RustLang {
                    $N(symbol)($C(candidate)$if(recursive) { , memo })
                })
            }
        }
    }

    pub(super) fn functions(&self, ir: &IrSpec) -> Result<Vec<FunSpec>, SigilStitchError> {
        self.shapes
            .iter()
            .map(|shape| {
                let mut function =
                    FunSpec::builder(&self.symbol(shape)).add_param(ParameterSpec::of(
                        "candidate",
                        TypeName::reference(TypeName::qualified("serde_json", "Value")),
                    ));
                if Self::recursive(shape) {
                    let memo = crate::generators::type_application(
                        TypeName::qualified("std::collections", "HashMap"),
                        vec![
                            TypeName::tuple(vec![
                                TypeName::primitive("usize"),
                                TypeName::pointer(TypeName::qualified("serde_json", "Value")),
                            ]),
                            TypeName::primitive("bool"),
                        ],
                    );
                    function = function
                        .add_param(ParameterSpec::of("memo", TypeName::reference_mut(memo)));
                }
                function
                    .returns(vec![TypeName::primitive("bool")])
                    .body(self.function_body(shape, ir)?)
                    .build()
            })
            .collect()
    }

    fn function_body(&self, shape: &JsonShape, ir: &IrSpec) -> Result<CodeBlock, SigilStitchError> {
        let body = self.body(shape, ir)?;
        if !matches!(shape, JsonShape::Reference { .. }) {
            return Ok(body);
        }
        let index = self.index(shape).to_string();
        // The JSON tree is immutable and alive for this selection. Pointer
        // identity is only a local cache key: it is never dereferenced or shared
        // across selections. The shape index keeps reference scopes distinct.
        // A closure retains early returns while caching both true and false.
        sigil_quote!(RustLang {
            let key = ($L(index), std::ptr::from_ref(candidate));
            if let Some(result) = memo.get(&key) { return *result; }
            let result = (|| { $C(body) })();
            memo.insert(key, result);
            result
        })
    }

    fn body(&self, shape: &JsonShape, ir: &IrSpec) -> Result<CodeBlock, SigilStitchError> {
        match shape {
            JsonShape::Any => sigil_quote!(RustLang { true }),
            JsonShape::Never => sigil_quote!(RustLang { false }),
            JsonShape::Null => sigil_quote!(RustLang { candidate.is_null() }),
            JsonShape::Boolean => sigil_quote!(RustLang { candidate.is_boolean() }),
            JsonShape::String => sigil_quote!(RustLang { candidate.is_string() }),
            JsonShape::Number { integer: true } => {
                sigil_quote!(RustLang { candidate.is_i64() || candidate.is_u64() })
            }
            JsonShape::Number { integer: false } => {
                sigil_quote!(RustLang { candidate.is_number() })
            }
            JsonShape::Binary => sigil_quote!(RustLang {
                candidate.as_array().is_some_and(|items| {
                    items.iter().all(|item| item.as_u64().is_some_and(|number| number <= 255))
                })
            }),
            JsonShape::Nullable(inner) => {
                let inner = self.condition(inner, sigil_quote!(RustLang { candidate })?)?;
                sigil_quote!(RustLang { candidate.is_null() || $C(inner) })
            }
            JsonShape::Literal(value) => literal_body(value),
            JsonShape::Array(inner) => {
                if Self::constant(inner) == Some(true) {
                    return sigil_quote!(RustLang { candidate.is_array() });
                }
                if Self::constant(inner) == Some(false) {
                    return sigil_quote!(RustLang {
                        candidate.as_array().is_some_and(|items| items.is_empty())
                    });
                }
                let check = self.condition(inner, sigil_quote!(RustLang { item })?)?;
                sigil_quote!(RustLang {
                    candidate.as_array().is_some_and(|items| items.iter().all(|item| $C(check)))
                })
            }
            JsonShape::Map(inner) => {
                if Self::constant(inner) == Some(true) {
                    return sigil_quote!(RustLang { candidate.is_object() });
                }
                if Self::constant(inner) == Some(false) {
                    return sigil_quote!(RustLang {
                        candidate.as_object().is_some_and(|items| items.is_empty())
                    });
                }
                let check = self.condition(inner, sigil_quote!(RustLang { item })?)?;
                sigil_quote!(RustLang {
                    candidate.as_object().is_some_and(|items| items.values().all(|item| $C(check)))
                })
            }
            JsonShape::MapWithConcreteValue { values, concrete } => {
                let values = self.condition(
                    &JsonShape::Map(values.clone()),
                    sigil_quote!(RustLang { candidate })?,
                )?;
                let concrete = self.condition(concrete, sigil_quote!(RustLang { item })?)?;
                sigil_quote!(RustLang {
                    if !$C(values) { return false; }
                    candidate.as_object().is_some_and(|items| items.values().any(|item| $C(concrete)))
                })
            }
            JsonShape::Object(properties) => {
                if properties.is_empty() {
                    return sigil_quote!(RustLang { candidate.is_object() });
                }
                let checks = properties
                    .iter()
                    .filter(|property| {
                        property.required || Self::constant(&property.shape) != Some(true)
                    })
                    .map(|property| {
                        let key = &property.wire_name;
                        if Self::constant(&property.shape) == Some(true) {
                            return sigil_quote!(RustLang {
                                if !object.contains_key($S(key)) { return false; }
                            });
                        }
                        let item = if Self::constant(&property.shape).is_some() {
                            "_item"
                        } else {
                            "item"
                        };
                        let check =
                            self.condition(&property.shape, sigil_quote!(RustLang { $N(item) })?)?;
                        if property.required {
                            sigil_quote!(RustLang {
                                if !object.get($S(key)).is_some_and(|$N(item)| $C(check)) {
                                    return false;
                                }
                            })
                        } else {
                            sigil_quote!(RustLang {
                                if object.get($S(key)).is_some_and(|$N(item)| !$C(check)) {
                                    return false;
                                }
                            })
                        }
                    })
                    .collect::<Result<Vec<_>, SigilStitchError>>()?;
                if checks.is_empty() {
                    return sigil_quote!(RustLang { candidate.is_object() });
                }
                sigil_quote!(RustLang {
                    let object = match candidate.as_object() {
                        Some(object) => object,
                        None => return false,
                    };
                    $C_each(checks)
                    true
                })
            }
            JsonShape::ObjectWithAdditional { properties, values } => {
                let object = self.condition(
                    &JsonShape::Object(properties.clone()),
                    sigil_quote!(RustLang { candidate })?,
                )?;
                if Self::constant(values) == Some(true) {
                    return sigil_quote!(RustLang { $C(object) });
                }
                let item = if Self::constant(values).is_some() {
                    "_item"
                } else {
                    "item"
                };
                let check = self.condition(values, sigil_quote!(RustLang { $N(item) })?)?;
                let keys: Vec<_> = properties
                    .iter()
                    .map(|property| &property.wire_name)
                    .collect();
                sigil_quote!(RustLang {
                    if !$C(object) { return false; }
                    let declared = [$for(key in &keys; separator = ", ") { $S(key) }];
                    candidate.as_object().is_some_and(|object| {
                        object.iter().all(|(key, $N(item))| declared.contains(&key.as_str()) || $C(check))
                    })
                })
            }
            JsonShape::AnyOf(children) | JsonShape::AllOf(children) => {
                let disjunction = matches!(shape, JsonShape::AnyOf(_));
                let checks = children
                    .iter()
                    .map(|child| {
                        let check = self.condition(child, sigil_quote!(RustLang { candidate })?)?;
                        if disjunction {
                            sigil_quote!(RustLang { if $C(check) { return true; } })
                        } else {
                            sigil_quote!(RustLang { if !$C(check) { return false; } })
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                sigil_quote!(RustLang {
                    $C_each(checks)
                    $if(disjunction) { false } $else { true }
                })
            }
            JsonShape::Reference { schema, scope, .. } => {
                let shape = plan_named_shape(schema, ir);
                if *scope == MatchScope::Any {
                    return self.condition(&shape, sigil_quote!(RustLang { candidate })?);
                }
                let (concrete, fallback) = selection_shapes(shape);
                let concrete = self.condition(&concrete, sigil_quote!(RustLang { candidate })?)?;
                if *scope == MatchScope::Concrete {
                    return Ok(concrete);
                }
                let fallback = self.condition(&fallback, sigil_quote!(RustLang { candidate })?)?;
                sigil_quote!(RustLang {
                    if $C(concrete) { return false; }
                    $C(fallback)
                })
            }
        }
    }
}

fn literal_body(value: &serde_json::Value) -> Result<CodeBlock, SigilStitchError> {
    match value {
        serde_json::Value::Null => sigil_quote!(RustLang { candidate.is_null() }),
        serde_json::Value::Bool(value) => {
            sigil_quote!(RustLang { candidate.as_bool() == Some($L(value.to_string())) })
        }
        serde_json::Value::String(value) => {
            sigil_quote!(RustLang { candidate.as_str() == Some($S(value)) })
        }
        serde_json::Value::Number(number) => number_body(number),
        _ => sigil_quote!(RustLang { false }),
    }
}

fn number_body(number: &serde_json::Number) -> Result<CodeBlock, SigilStitchError> {
    // Compare integral JSON numbers exactly, including values above 2^53. Only
    // cross the float/integer boundary when a finite float fits that integer;
    // converting both integers to f64 would conflate adjacent enum values.
    if number.is_i64() || number.is_u64() {
        let signed = number.is_i64();
        let integer = format!("{number}_{}", if signed { "i64" } else { "u64" });
        let accessor = if signed { "as_i64" } else { "as_u64" };
        let integer_type = TypeName::primitive(if signed { "i64" } else { "u64" });
        return sigil_quote!(RustLang {
            if !candidate.is_f64() {
                return candidate.$N(accessor)() == Some($L(integer.clone()));
            }
            candidate.as_f64().is_some_and(|number| {
                number.fract() == 0.0
                    && $if(signed) { number >= i64::MIN as f64 && number < -(i64::MIN as f64) }
                       $else { number >= 0.0 && number < u64::MAX as f64 }
                    && number as $T(integer_type) == $L(integer)
            })
        });
    }
    let number = format!("{number}_f64");
    sigil_quote!(RustLang {
        let expected = $L(number);
        if candidate.is_f64() { return candidate.as_f64() == Some(expected); }
        if expected.fract() != 0.0 { return false; }
        if expected >= i64::MIN as f64 && expected < -(i64::MIN as f64) {
            return candidate.as_i64() == Some(expected as i64);
        }
        if expected >= 0.0 && expected < u64::MAX as f64 {
            return candidate.as_u64() == Some(expected as u64);
        }
        false
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generators::union_match::PropertyMatch;
    use sigil_stitch::assert_rendered;
    use sigil_stitch::lang::rust::Rust;
    use sigil_stitch::spec::modifiers::DeclarationContext;

    #[test]
    fn recursive_reference_predicates_cache_results_by_shape_and_json_node() {
        let ir = crate::ir::lower::lower(
            crate::parser::parse_content_yaml(include_str!(
                "../../../../tests/fixtures/valid/rust-union-policies.yaml"
            ))
            .unwrap(),
        )
        .unwrap();
        let shape = JsonShape::Reference {
            schema: "Tree".into(),
            scope: MatchScope::Any,
            unrestricted: false,
            accepts_null: false,
        };
        let mut plan = MatchPlan::default();
        plan.register(&shape, &ir);
        assert_rendered!(
            Rust::new(),
            width = 100,
            plan.functions(&ir).unwrap()[0]
                .emit(&Rust::new(), DeclarationContext::TopLevel)
                .unwrap(),
            r#"fn __nexus_matches_0(candidate: &serde_json::Value,
    memo: &mut std::collections::HashMap<(usize, *const serde_json::Value), bool>) -> bool {
    let key = (0, std::ptr::from_ref(candidate));
    if let Some(result) = memo.get(&key) {
        return *result;
    }
    let result = (|| {__nexus_matches_1(candidate, memo)})();
    memo.insert(key, result);
    result
}
"#,
        );
    }

    #[test]
    fn object_matchers_keep_match_statement_terminators_and_wire_key_escaping() {
        let ir = crate::ir::lower::lower(
            crate::parser::parse_content_yaml(include_str!(
                "../../../../tests/fixtures/valid/union-selection.yaml"
            ))
            .unwrap(),
        )
        .unwrap();
        let shape = JsonShape::Object(vec![PropertyMatch {
            wire_name: "wire\"key".into(),
            rename: false,
            required: true,
            shape: JsonShape::String,
        }]);
        let mut plan = MatchPlan::default();
        plan.register(&shape, &ir);
        let body = plan.body(&shape, &ir).unwrap();
        assert_rendered!(
            Rust::new(),
            width = 100,
            body,
            r#"let object = match candidate.as_object() {
    Some(object) => object,
    None => return false,
};
if!object.get("wire\"key").is_some_and(|item | __nexus_matches_1(item)) {
    return false;
}
true
"#,
        );
    }

    #[test]
    fn scalar_guard_literals_preserve_boolean_and_numeric_json_kinds() {
        assert_rendered!(
            Rust::new(),
            literal_body(&true.into()).unwrap(),
            "candidate.as_bool() == Some(true)\n"
        );
        assert_rendered!(
            Rust::new(),
            literal_body(&"wire\"value".into()).unwrap(),
            "candidate.as_str() == Some(\"wire\\\"value\")\n"
        );
    }
}
