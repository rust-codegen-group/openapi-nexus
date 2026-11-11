//! Private Gson predicates for the shared structural union-selection facts.

use std::borrow::Cow;

use sigil_stitch::error::SigilStitchError;
use sigil_stitch::prelude::{CodeBlock, FunSpec, ParameterSpec, TypeName, Visibility, sigil_quote};

use crate::generators::type_application;
use crate::generators::union_match::{
    JsonShape, MatchScope, accepts_any, plan_named_shape, selection_shapes,
};
use crate::ir::types::{IrObject, IrProperty, IrSchemaKind, IrSpec};

/// Java aliases and intersections have ordinary Gson wrapper fields. Project
/// only matcher facts; their declarations and codecs remain intact.
pub(super) fn wire_ir(ir: &IrSpec) -> Cow<'_, IrSpec> {
    if !ir.schemas.values().any(|schema| {
        matches!(
            schema.kind,
            IrSchemaKind::Alias(_) | IrSchemaKind::Intersection(_)
        )
    }) {
        return Cow::Borrowed(ir);
    }
    let mut wire = ir.clone();
    for schema in wire.schemas.values_mut() {
        let properties = match &schema.kind {
            IrSchemaKind::Alias(expr) => vec![("value".into(), expr)],
            IrSchemaKind::Intersection(intersection) => {
                super::util::intersection_fields(intersection)
            }
            _ => continue,
        }
        .into_iter()
        .map(|(name, expr)| {
            (
                name.clone(),
                IrProperty {
                    name,
                    type_expr: expr.clone(),
                    required: true,
                    nullable: false,
                    description: None,
                    default_value: None,
                    format: None,
                    validation: None,
                },
            )
        })
        .collect();
        schema.kind = IrSchemaKind::Object(IrObject {
            properties,
            additional_properties: None,
        });
    }
    Cow::Owned(wire)
}

#[derive(Default)]
pub(super) struct MatchPlan {
    shapes: Vec<JsonShape>,
}

impl MatchPlan {
    fn constant(shape: &JsonShape) -> Option<bool> {
        match shape {
            JsonShape::Any => Some(true),
            JsonShape::Never => Some(false),
            JsonShape::Reference {
                scope: MatchScope::Any,
                unrestricted: true,
                ..
            } => Some(true),
            JsonShape::AnyOf(_) | JsonShape::AllOf(_) | JsonShape::Nullable(_)
                if accepts_any(shape) =>
            {
                Some(true)
            }
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
            JsonShape::Nullable(inner) | JsonShape::Array(inner) | JsonShape::Map(inner) => {
                self.register(inner, ir)
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
            .expect("registered Java union shape")
    }

    fn symbol(&self, shape: &JsonShape) -> String {
        format!("matches{}", self.index(shape))
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
            JsonShape::Nullable(inner) | JsonShape::Array(inner) | JsonShape::Map(inner) => {
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

    pub(super) fn memo_type() -> TypeName {
        type_application(
            TypeName::qualified("java.util", "IdentityHashMap"),
            vec![
                TypeName::importable("com.google.gson", "JsonElement"),
                Self::results_type(),
            ],
        )
    }

    fn results_type() -> TypeName {
        type_application(
            TypeName::qualified("java.util", "Map"),
            vec![
                TypeName::qualified("java.lang", "Integer"),
                TypeName::qualified("java.lang", "Boolean"),
            ],
        )
    }

    pub(super) fn condition(
        &self,
        shape: &JsonShape,
        candidate: CodeBlock,
    ) -> Result<CodeBlock, SigilStitchError> {
        match Self::constant(shape) {
            Some(true) => sigil_quote!(Java { true }),
            Some(false) => sigil_quote!(Java { false }),
            None => {
                sigil_quote!(Java { $N(self.symbol(shape))($C(candidate)$if(Self::recursive(shape)) { , memo }) })
            }
        }
    }

    pub(super) fn functions(&self, ir: &IrSpec) -> Result<Vec<FunSpec>, SigilStitchError> {
        let mut functions = self
            .shapes
            .iter()
            .map(|shape| {
                let mut function = FunSpec::builder(&self.symbol(shape))
                    .visibility(Visibility::Private)
                    .is_static()
                    .add_param(ParameterSpec::of(
                        "candidate",
                        TypeName::importable("com.google.gson", "JsonElement"),
                    ));
                if Self::recursive(shape) {
                    function = function.add_param(ParameterSpec::of("memo", Self::memo_type()));
                }
                function
                    .returns(vec![TypeName::primitive("boolean")])
                    .body(self.body(shape, ir)?)
                    .build()
            })
            .collect::<Result<Vec<_>, _>>()?;
        functions.extend(self.numeric_functions()?);
        Ok(functions)
    }

    fn numeric_functions(&self) -> Result<Vec<FunSpec>, SigilStitchError> {
        if !self.shapes.iter().any(|shape| {
            matches!(
                shape,
                JsonShape::Number { integer: true }
                    | JsonShape::Binary
                    | JsonShape::Literal(serde_json::Value::Number(_))
            )
        }) {
            return Ok(Vec::new());
        }
        let decimal = TypeName::qualified("java.math", "BigDecimal");
        let integer = TypeName::qualified("java.math", "BigInteger");
        let text = TypeName::qualified("java.lang", "String");
        let math = TypeName::qualified("java.lang", "Math");
        // Gson's decimal accessor limits absolute scale to 9999. Read only the
        // significand as BigDecimal and keep the exponent as BigInteger, so
        // selection stays exact without adding a numeric limit to the codec.
        let significand = FunSpec::builder("numberSignificand")
            .visibility(Visibility::Private)
            .is_static()
            .add_param(ParameterSpec::of("literal", text.clone()))
            .returns(vec![decimal.clone()])
            .body(sigil_quote!(Java {
                int exponent = $T(math.clone()).max(literal.indexOf('e'), literal.indexOf('E'));
                return new $T(decimal.clone())(exponent < 0 ? literal : literal.substring(0, exponent)).stripTrailingZeros();
            })?)
            .build()?;
        let exponent = FunSpec::builder("numberExponent")
            .visibility(Visibility::Private)
            .is_static()
            .add_param(ParameterSpec::of("literal", text.clone()))
            .returns(vec![integer.clone()])
            .body(sigil_quote!(Java {
                int exponent = $T(math).max(literal.indexOf('e'), literal.indexOf('E'));
                return exponent < 0 ? $T(integer.clone()).ZERO : new $T(integer.clone())(literal.substring(exponent + 1));
            })?)
            .build()?;
        let mut functions = vec![significand, exponent];
        if self
            .shapes
            .iter()
            .any(|shape| matches!(shape, JsonShape::Literal(serde_json::Value::Number(_))))
        {
            functions.push(FunSpec::builder("matchesNumber")
                .visibility(Visibility::Private)
                .is_static()
                .add_param(ParameterSpec::of("candidate", TypeName::importable("com.google.gson", "JsonElement")))
                .add_param(ParameterSpec::of("expected", text.clone()))
                .returns(vec![TypeName::primitive("boolean")])
                .body(sigil_quote!(Java {
                    $T(text) literal = candidate.getAsString();
                    $T(decimal.clone()) significand = numberSignificand(literal);
                    $T(decimal.clone()) expectedNumber = new $T(decimal)(expected).stripTrailingZeros();
                    if (!significand.unscaledValue().equals(expectedNumber.unscaledValue())) { return false; }
                    return significand.signum() == 0 $+
                        || numberExponent(literal).equals($T(integer).valueOf((long) significand.scale() - expectedNumber.scale()));
                })?)
                .build()?);
        }
        Ok(functions)
    }

    fn body(&self, shape: &JsonShape, ir: &IrSpec) -> Result<CodeBlock, SigilStitchError> {
        let element = TypeName::importable("com.google.gson", "JsonElement");
        let decimal = TypeName::qualified("java.math", "BigDecimal");
        let big_integer = TypeName::qualified("java.math", "BigInteger");
        let text = TypeName::qualified("java.lang", "String");
        match shape {
            JsonShape::Any => sigil_quote!(Java { return true; }),
            JsonShape::Never => sigil_quote!(Java { return false; }),
            JsonShape::Null => sigil_quote!(Java { return candidate.isJsonNull(); }),
            JsonShape::Boolean => {
                sigil_quote!(Java { return candidate.isJsonPrimitive() && candidate.getAsJsonPrimitive().isBoolean(); })
            }
            JsonShape::String => {
                sigil_quote!(Java { return candidate.isJsonPrimitive() && candidate.getAsJsonPrimitive().isString(); })
            }
            JsonShape::Number { integer } => sigil_quote!(Java {
                $if(*integer) {
                    if (!candidate.isJsonPrimitive() || !candidate.getAsJsonPrimitive().isNumber()) { return false; }
                    $T(text) literal = candidate.getAsString();
                    $T(decimal) significand = numberSignificand(literal);
                    return significand.signum() == 0 $+
                        || numberExponent(literal).compareTo($T(big_integer).valueOf(significand.scale())) >= 0;
                } $else {
                    return candidate.isJsonPrimitive() && candidate.getAsJsonPrimitive().isNumber();
                }
            }),
            JsonShape::Binary => sigil_quote!(Java {
                if (!candidate.isJsonArray()) { return false; }
                for ($T(element) item : candidate.getAsJsonArray()) {
                    if (!item.isJsonPrimitive() || !item.getAsJsonPrimitive().isNumber()) { return false; }
                    $T(text) literal = item.getAsString();
                    $T(decimal.clone()) significand = numberSignificand(literal);
                    if (significand.signum() == 0) { continue; }
                    $T(big_integer.clone()) scale = $T(big_integer.clone()).valueOf(significand.scale()).subtract(numberExponent(literal));
                    // A nonzero integral byte has at most two trailing zeros.
                    if (scale.signum() > 0 || scale.compareTo($T(big_integer).valueOf(-2)) < 0) { return false; }
                    $T(decimal.clone()) number = new $T(decimal.clone())(significand.unscaledValue(), scale.intValueExact());
                    if (number.compareTo(new $T(decimal.clone())("-128")) < 0
                        || number.compareTo(new $T(decimal.clone())("255")) > 0) { return false; }
                }
                return true;
            }),
            JsonShape::Literal(value) => match value {
                serde_json::Value::Null => sigil_quote!(Java { return candidate.isJsonNull(); }),
                serde_json::Value::Bool(value) => sigil_quote!(Java {
                    return candidate.isJsonPrimitive() && candidate.getAsJsonPrimitive().isBoolean() $+
                        && candidate.getAsBoolean() == $L(value.to_string());
                }),
                serde_json::Value::String(value) => sigil_quote!(Java {
                    return candidate.isJsonPrimitive() && candidate.getAsJsonPrimitive().isString() $+
                        && candidate.getAsString().equals($S(value));
                }),
                serde_json::Value::Number(value) => sigil_quote!(Java {
                    return candidate.isJsonPrimitive() && candidate.getAsJsonPrimitive().isNumber() $+
                        && matchesNumber(candidate, $S(value.to_string()));
                }),
                _ => sigil_quote!(Java { return false; }),
            },
            JsonShape::Nullable(inner) => {
                let check = self.condition(inner, sigil_quote!(Java { candidate })?)?;
                sigil_quote!(Java { return candidate.isJsonNull() || $C(check); })
            }
            JsonShape::Array(inner) | JsonShape::Map(inner) => {
                let array = matches!(shape, JsonShape::Array(_));
                let check = self.condition(inner, sigil_quote!(Java { item })?)?;
                sigil_quote!(Java {
                    $if(array) {
                        if (!candidate.isJsonArray()) { return false; }
                        for ($T(element) item : candidate.getAsJsonArray()) {
                            if (!$C(check)) { return false; }
                        }
                    } $else {
                        if (!candidate.isJsonObject()) { return false; }
                        for ($T(element) item : candidate.getAsJsonObject().asMap().values()) {
                            if (!$C(check)) { return false; }
                        }
                    }
                    return true;
                })
            }
            JsonShape::MapWithConcreteValue { values, concrete } => {
                let values = self.condition(
                    &JsonShape::Map(values.clone()),
                    sigil_quote!(Java { candidate })?,
                )?;
                let concrete = self.condition(concrete, sigil_quote!(Java { item })?)?;
                sigil_quote!(Java {
                    if (!$C(values)) { return false; }
                    for ($T(element) item : candidate.getAsJsonObject().asMap().values()) {
                        if ($C(concrete)) { return true; }
                    }
                    return false;
                })
            }
            JsonShape::Object(properties) => {
                let checks = properties.iter().map(|property| {
                    let key = &property.wire_name;
                    let check = self.condition(&property.shape, sigil_quote!(Java { object.get($S(key)) })?)?;
                    if property.required {
                        sigil_quote!(Java { if (!object.has($S(key)) || !$C(check)) { return false; } })
                    } else {
                        sigil_quote!(Java { if (object.has($S(key)) && !$C(check)) { return false; } })
                    }
                }).collect::<Result<Vec<_>, _>>()?;
                let object = TypeName::importable("com.google.gson", "JsonObject");
                sigil_quote!(Java {
                    if (!candidate.isJsonObject()) { return false; }
                    $if(!properties.is_empty()) {
                        $T(object) object = candidate.getAsJsonObject();
                        $C_each(checks)
                    }
                    return true;
                })
            }
            JsonShape::ObjectWithAdditional { properties, values } => {
                let object = self.condition(
                    &JsonShape::Object(properties.clone()),
                    sigil_quote!(Java { candidate })?,
                )?;
                let check = self.condition(values, sigil_quote!(Java { item.getValue() })?)?;
                let keys: Vec<_> = properties
                    .iter()
                    .map(|property| &property.wire_name)
                    .collect();
                let entry = type_application(
                    TypeName::qualified("java.util.Map", "Entry"),
                    vec![TypeName::primitive("String"), element],
                );
                let strings = type_application(
                    TypeName::qualified("java.util", "Set"),
                    vec![TypeName::primitive("String")],
                );
                sigil_quote!(Java {
                    if (!$C(object)) { return false; }
                    $T(strings) declared = java.util.Set.of($for(key in &keys; separator = ", ") { $S(key) });
                    for ($T(entry) item : candidate.getAsJsonObject().entrySet()) {
                        if (!declared.contains(item.getKey()) && !$C(check)) { return false; }
                    }
                    return true;
                })
            }
            JsonShape::AnyOf(children) | JsonShape::AllOf(children) => {
                let disjunction = matches!(shape, JsonShape::AnyOf(_));
                let checks = children
                    .iter()
                    .map(|child| {
                        let check = self.condition(child, sigil_quote!(Java { candidate })?)?;
                        if disjunction {
                            sigil_quote!(Java { if ($C(check)) { return true; } })
                        } else {
                            sigil_quote!(Java { if (!$C(check)) { return false; } })
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                sigil_quote!(Java {
                    $C_each(checks)
                    $if(disjunction) { return false; } $else { return true; }
                })
            }
            JsonShape::Reference { schema, scope, .. } => {
                let index = self.index(shape).to_string();
                let named = plan_named_shape(schema, ir);
                let expression = if *scope == MatchScope::Any {
                    self.condition(&named, sigil_quote!(Java { candidate })?)?
                } else {
                    let (concrete, fallback) = selection_shapes(named);
                    let concrete = self.condition(&concrete, sigil_quote!(Java { candidate })?)?;
                    if *scope == MatchScope::Concrete {
                        concrete
                    } else {
                        let fallback =
                            self.condition(&fallback, sigil_quote!(Java { candidate })?)?;
                        sigil_quote!(Java { (!$C(concrete) && $C(fallback)) })?
                    }
                };
                // Gson trees are immutable during selection. Identity keys avoid
                // JsonElement's recursive structural hashing; predicate indices
                // keep scopes distinct, including cached false results.
                let results = Self::results_type();
                let boolean = TypeName::qualified("java.lang", "Boolean");
                sigil_quote!(Java {
                    $T(results) results = memo.get(candidate);
                    if (results == null) {
                        results = new java.util.HashMap<>();
                        memo.put(candidate, results);
                    }
                    $T(boolean) cached = results.get($L(index.clone()));
                    if (cached != null) { return cached; }
                    boolean matched = $C(expression);
                    results.put($L(index), matched);
                    return matched;
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sigil_stitch::assert_rendered;
    use sigil_stitch::lang::java::Java;

    fn fixture() -> IrSpec {
        crate::ir::lower::lower(
            crate::parser::parse_content_yaml(include_str!(
                "../../../../tests/fixtures/valid/java-union-policies.yaml"
            ))
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn finite_numeric_predicates_keep_json_kinds_and_exact_decimal_values() {
        let shape = JsonShape::Literal(9007199254740993_u64.into());
        assert_rendered!(
            Java::new(),
            width = 200,
            MatchPlan::default().body(&shape, &fixture()).unwrap(),
            "return candidate.isJsonPrimitive() && candidate.getAsJsonPrimitive().isNumber() && matchesNumber(candidate, \"9007199254740993\");\n"
        );
        assert_rendered!(
            Java::new(),
            width = 200,
            MatchPlan::default()
                .body(&JsonShape::Literal("wire\"value".into()), &fixture())
                .unwrap(),
            "return candidate.isJsonPrimitive() && candidate.getAsJsonPrimitive().isString() && candidate.getAsString().equals(\"wire\\\"value\");\n"
        );
    }

    #[test]
    fn numeric_helpers_compare_significands_with_unbounded_exponents() {
        use sigil_stitch::spec::modifiers::DeclarationContext;

        let mut plan = MatchPlan::default();
        plan.register(&JsonShape::Literal(1.into()), &fixture());
        let helper = plan.numeric_functions().unwrap().pop().unwrap();
        assert_rendered!(
            Java::new(),
            width = 200,
            helper
                .emit(&Java::new(), DeclarationContext::Member)
                .unwrap(),
            r#"import com.google.gson.JsonElement;

private static boolean matchesNumber(JsonElement candidate, java.lang.String expected) {
    java.lang.String literal = candidate.getAsString();
    java.math.BigDecimal significand = numberSignificand(literal);
    java.math.BigDecimal expectedNumber = new java.math.BigDecimal(expected).stripTrailingZeros();
    if (!significand.unscaledValue().equals(expectedNumber.unscaledValue())) {
        return false;
    }
    return significand.signum() == 0 || numberExponent(literal).equals(java.math.BigInteger.valueOf((long) significand.scale() - expectedNumber.scale()));
}
"#
        );
    }

    #[test]
    fn recursive_reference_predicates_cache_by_node_identity_and_shape_index() {
        let ir = fixture();
        let shape = JsonShape::Reference {
            schema: "Tree".into(),
            scope: MatchScope::Any,
            unrestricted: false,
            accepts_null: false,
        };
        let mut plan = MatchPlan::default();
        plan.register(&shape, &ir);
        assert_rendered!(
            Java::new(),
            width = 100,
            plan.body(&shape, &ir).unwrap(),
            r#"java.util.Map<java.lang.Integer, java.lang.Boolean> results = memo.get(candidate);
if (results == null) {
    results = new java.util.HashMap<>();
    memo.put(candidate, results);
}
java.lang.Boolean cached = results.get(0);
if (cached != null) {
    return cached;
}
boolean matched = matches1(candidate, memo);
results.put(0, matched);
return matched;
"#
        );
    }

    #[test]
    fn matcher_wire_projection_does_not_change_alias_or_intersection_declarations() {
        let ir = fixture();
        let wire = wire_ir(&ir);
        assert!(matches!(
            ir.schemas["NumberAlias"].kind,
            IrSchemaKind::Alias(_)
        ));
        assert!(matches!(
            ir.schemas["WrappedPair"].kind,
            IrSchemaKind::Intersection(_)
        ));
        let IrSchemaKind::Object(alias) = &wire.schemas["NumberAlias"].kind else {
            panic!("alias wrapper facts")
        };
        assert!(alias.properties["value"].required);
        let IrSchemaKind::Object(pair) = &wire.schemas["WrappedPair"].kind else {
            panic!("composition wrapper facts")
        };
        assert_eq!(
            pair.properties
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["leftPart", "rightPart"]
        );
    }
}
