//! Kotlin Gson predicates over shared, target-independent selection facts.

use std::borrow::Cow;

use sigil_stitch::prelude::*;

use crate::generators::type_application;
use crate::generators::union_match::{
    JsonShape, MatchScope, accepts_any, plan_named_shape, selection_shapes,
};
use crate::ir::types::{IrObject, IrProperty, IrSchemaKind, IrSpec};

/// Kotlin aliases are transparent; intersections retain their wrapper fields.
pub(super) fn wire_ir(ir: &IrSpec) -> Cow<'_, IrSpec> {
    if !ir
        .schemas
        .values()
        .any(|schema| matches!(schema.kind, IrSchemaKind::Intersection(_)))
    {
        return Cow::Borrowed(ir);
    }
    let mut wire = ir.clone();
    for schema in wire.schemas.values_mut() {
        let properties = match &schema.kind {
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

#[cfg(test)]
mod tests {
    use super::*;
    use sigil_stitch::assert_rendered;
    use sigil_stitch::lang::kotlin::Kotlin;

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
    fn finite_predicates_preserve_scalar_kinds_and_exact_values() {
        let ir = fixture();
        assert_rendered!(
            Kotlin::new(),
            width = 200,
            MatchPlan::default()
                .body(&JsonShape::Literal(9007199254740993_u64.into()), &ir)
                .unwrap(),
            "return candidate.isJsonPrimitive && candidate.asJsonPrimitive.isNumber && matchesNumber(candidate, \"9007199254740993\")\n"
        );
        assert_rendered!(
            Kotlin::new(),
            width = 200,
            MatchPlan::default()
                .body(&JsonShape::Literal("wire\"value".into()), &ir)
                .unwrap(),
            "return candidate.isJsonPrimitive && candidate.asJsonPrimitive.isString && candidate.asString == \"wire\\\"value\"\n"
        );
    }

    #[test]
    fn recursive_checks_use_node_identity_and_keep_false_results() {
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
            Kotlin::new(),
            width = 100,
            plan.body(&shape, &ir).unwrap(),
            r#"val results = memo.getOrPut(candidate) {
    mutableMapOf()
}
val cached = results[0]
if (cached != null) {
    return cached
}
val matched = matches1(candidate, memo)
results[0] = matched
return matched
"#
        );
    }

    #[test]
    fn exact_numeric_helpers_keep_exponents_out_of_bigdecimal_scale() {
        use sigil_stitch::spec::modifiers::DeclarationContext;
        let ir = fixture();
        let mut plan = MatchPlan::default();
        plan.register(&JsonShape::Literal(1.into()), &ir);
        let helper = plan.numeric_functions().unwrap().remove(0);
        assert_rendered!(
            Kotlin::new(),
            width = 200,
            helper
                .emit(&Kotlin::new(), DeclarationContext::Member)
                .unwrap(),
            r#"private fun numberSignificand(literal: kotlin.String): java.math.BigDecimal {
    val exponent = maxOf(literal.indexOf('e'), literal.indexOf('E'))
    return java.math.BigDecimal(if (exponent < 0) literal else literal.substring(0, exponent)).stripTrailingZeros()
}
"#
        );
    }

    #[test]
    fn wire_projection_preserves_transparent_aliases_and_wrapped_intersections() {
        let ir = fixture();
        let wire = wire_ir(&ir);
        assert_eq!(
            serde_json::to_value(&wire.schemas["NumberAlias"]).unwrap(),
            serde_json::to_value(&ir.schemas["NumberAlias"]).unwrap()
        );
        assert!(matches!(
            ir.schemas["WrappedPair"].kind,
            IrSchemaKind::Intersection(_)
        ));
        let IrSchemaKind::Object(pair) = &wire.schemas["WrappedPair"].kind else {
            panic!("intersection wrapper facts")
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
            .expect("registered Kotlin union shape")
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
            JsonShape::Object(properties) => properties.iter().any(|p| Self::recursive(&p.shape)),
            JsonShape::ObjectWithAdditional { properties, values } => {
                properties.iter().any(|p| Self::recursive(&p.shape)) || Self::recursive(values)
            }
            JsonShape::AnyOf(children) | JsonShape::AllOf(children) => {
                children.iter().any(Self::recursive)
            }
            _ => false,
        }
    }

    pub(super) fn is_recursive(&self) -> bool {
        self.shapes
            .iter()
            .any(|shape| matches!(shape, JsonShape::Reference { .. }))
    }

    pub(super) fn memo_type() -> TypeName {
        type_application(
            TypeName::qualified("java.util", "IdentityHashMap"),
            vec![
                TypeName::importable("com.google.gson", "JsonElement"),
                type_application(
                    TypeName::qualified("kotlin.collections", "MutableMap"),
                    vec![
                        TypeName::qualified("kotlin", "Int"),
                        TypeName::qualified("kotlin", "Boolean"),
                    ],
                ),
            ],
        )
    }

    pub(super) fn condition(
        &self,
        shape: &JsonShape,
        candidate: CodeBlock,
    ) -> Result<CodeBlock, SigilStitchError> {
        match Self::constant(shape) {
            Some(true) => sigil_quote!(Kotlin { true }),
            Some(false) => sigil_quote!(Kotlin { false }),
            None => {
                sigil_quote!(Kotlin { $N(format!("matches{}", self.index(shape)))($C(candidate)$if(Self::recursive(shape)) { , memo }) })
            }
        }
    }

    pub(super) fn functions(&self, ir: &IrSpec) -> Result<Vec<FunSpec>, SigilStitchError> {
        let mut functions = self
            .shapes
            .iter()
            .map(|shape| {
                let mut function = FunSpec::builder(&format!("matches{}", self.index(shape)))
                    .visibility(Visibility::Private)
                    .add_param(ParameterSpec::of(
                        "candidate",
                        TypeName::importable("com.google.gson", "JsonElement"),
                    ))
                    .returns(vec![TypeName::qualified("kotlin", "Boolean")]);
                if Self::recursive(shape) {
                    function = function.add_param(ParameterSpec::of("memo", Self::memo_type()));
                }
                function.body(self.body(shape, ir)?).build()
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
        let text = TypeName::qualified("kotlin", "String");
        let element = TypeName::importable("com.google.gson", "JsonElement");
        let boolean = TypeName::qualified("kotlin", "Boolean");
        // Gson's BigDecimal accessor imposes a scale cap; separate the exact
        // significand from the unbounded exponent before choosing a codec.
        let significand = FunSpec::builder("numberSignificand").visibility(Visibility::Private)
            .add_param(ParameterSpec::of("literal", text.clone())).returns(vec![decimal.clone()])
            .body(sigil_quote!(Kotlin {
                val exponent = maxOf(literal.indexOf('e'), literal.indexOf('E'))
                return $T(decimal.clone())(if (exponent < 0) literal else literal.substring(0, exponent)).stripTrailingZeros()
            })?).build()?;
        let exponent = FunSpec::builder("numberExponent").visibility(Visibility::Private)
            .add_param(ParameterSpec::of("literal", text.clone())).returns(vec![integer.clone()])
            .body(sigil_quote!(Kotlin {
                val exponent = maxOf(literal.indexOf('e'), literal.indexOf('E'))
                return if (exponent < 0) $T(integer.clone()).ZERO else $T(integer.clone())(literal.substring(exponent + 1))
            })?).build()?;
        let integral = FunSpec::builder("isIntegerNumber").visibility(Visibility::Private)
            .add_param(ParameterSpec::of("candidate", element.clone())).returns(vec![boolean.clone()])
            .body(sigil_quote!(Kotlin {
                val literal = candidate.asString
                val significand = numberSignificand(literal)
                return significand.signum() == 0 || numberExponent(literal).compareTo($T(integer.clone()).valueOf(significand.scale().toLong())) >= 0
            })?).build()?;
        let binary = FunSpec::builder("isBinaryNumber").visibility(Visibility::Private)
            .add_param(ParameterSpec::of("candidate", element.clone())).returns(vec![boolean.clone()])
            .body(sigil_quote!(Kotlin {
                if (!candidate.isJsonPrimitive || !candidate.asJsonPrimitive.isNumber || !isIntegerNumber(candidate)) { return false }
                val literal = candidate.asString
                val significand = numberSignificand(literal)
                if (significand.signum() == 0) { return true }
                val scale = numberExponent(literal).subtract($T(integer.clone()).valueOf(significand.scale().toLong()))
                if (scale.compareTo($T(integer.clone()).valueOf(2)) > 0) { return false }
                val number = significand.unscaledValue().multiply($T(integer.clone()).TEN.pow(scale.toInt()))
                return number.compareTo($T(integer.clone()).valueOf(-128)) >= 0 && number.compareTo($T(integer.clone()).valueOf(255)) <= 0
            })?).build()?;
        let equal = FunSpec::builder("matchesNumber").visibility(Visibility::Private)
            .add_param(ParameterSpec::of("candidate", element)).add_param(ParameterSpec::of("expected", text))
            .returns(vec![boolean]).body(sigil_quote!(Kotlin {
                val literal = candidate.asString
                val significand = numberSignificand(literal)
                val expectedNumber = $T(decimal.clone())(expected).stripTrailingZeros()
                if (significand.unscaledValue() != expectedNumber.unscaledValue()) { return false }
                return significand.signum() == 0 || numberExponent(literal) == $T(integer.clone()).valueOf(significand.scale().toLong() - expectedNumber.scale())
            })?).build()?;
        Ok(vec![significand, exponent, integral, binary, equal])
    }

    fn body(&self, shape: &JsonShape, ir: &IrSpec) -> Result<CodeBlock, SigilStitchError> {
        match shape {
            JsonShape::Any => sigil_quote!(Kotlin { return true }),
            JsonShape::Never => sigil_quote!(Kotlin { return false }),
            JsonShape::Null | JsonShape::Literal(serde_json::Value::Null) => {
                sigil_quote!(Kotlin { return candidate.isJsonNull })
            }
            JsonShape::Boolean => {
                sigil_quote!(Kotlin { return candidate.isJsonPrimitive && candidate.asJsonPrimitive.isBoolean })
            }
            JsonShape::String => {
                sigil_quote!(Kotlin { return candidate.isJsonPrimitive && candidate.asJsonPrimitive.isString })
            }
            JsonShape::Number { integer: true } => {
                sigil_quote!(Kotlin { return candidate.isJsonPrimitive && candidate.asJsonPrimitive.isNumber && isIntegerNumber(candidate) })
            }
            JsonShape::Number { integer: false } => {
                sigil_quote!(Kotlin { return candidate.isJsonPrimitive && candidate.asJsonPrimitive.isNumber })
            }
            JsonShape::Literal(serde_json::Value::Bool(value)) => {
                sigil_quote!(Kotlin { return candidate.isJsonPrimitive && candidate.asJsonPrimitive.isBoolean && candidate.asBoolean == $L(value.to_string()) })
            }
            JsonShape::Literal(serde_json::Value::String(value)) => {
                sigil_quote!(Kotlin { return candidate.isJsonPrimitive && candidate.asJsonPrimitive.isString && candidate.asString == $S(value) })
            }
            JsonShape::Literal(serde_json::Value::Number(value)) => {
                sigil_quote!(Kotlin { return candidate.isJsonPrimitive && candidate.asJsonPrimitive.isNumber && matchesNumber(candidate, $S(value.to_string())) })
            }
            JsonShape::Literal(_) => sigil_quote!(Kotlin { return false }),
            JsonShape::Nullable(inner) => {
                let check = self.condition(inner, sigil_quote!(Kotlin { candidate })?)?;
                sigil_quote!(Kotlin { return candidate.isJsonNull || $C(check) })
            }
            JsonShape::Binary => sigil_quote!(Kotlin {
                if (!candidate.isJsonArray) { return false }
                for (item in candidate.asJsonArray) { if (!isBinaryNumber(item)) { return false } }
                return true
            }),
            JsonShape::Array(inner) | JsonShape::Map(inner) => {
                let array = matches!(shape, JsonShape::Array(_));
                let check = self.condition(inner, sigil_quote!(Kotlin { item })?)?;
                sigil_quote!(Kotlin {
                    $if(array) {
                        if (!candidate.isJsonArray) { return false }
                        for (item in candidate.asJsonArray) { if (!$C(check)) { return false } }
                    } $else {
                        if (!candidate.isJsonObject) { return false }
                        for (item in candidate.asJsonObject.asMap().values) { if (!$C(check)) { return false } }
                    }
                    return true
                })
            }
            JsonShape::MapWithConcreteValue { values, concrete } => {
                let values = self.condition(
                    &JsonShape::Map(values.clone()),
                    sigil_quote!(Kotlin { candidate })?,
                )?;
                let concrete = self.condition(concrete, sigil_quote!(Kotlin { item })?)?;
                sigil_quote!(Kotlin {
                    if (!$C(values)) { return false }
                    for (item in candidate.asJsonObject.asMap().values) { if ($C(concrete)) { return true } }
                    return false
                })
            }
            JsonShape::Object(properties) => {
                let checks = properties.iter().map(|property| {
                    let key = &property.wire_name;
                    let check = self.condition(&property.shape, sigil_quote!(Kotlin { jsonObject.get($S(key)) })?)?;
                    if property.required { sigil_quote!(Kotlin { if (!jsonObject.has($S(key)) || !$C(check)) { return false } }) }
                    else { sigil_quote!(Kotlin { if (jsonObject.has($S(key)) && !$C(check)) { return false } }) }
                }).collect::<Result<Vec<_>, _>>()?;
                sigil_quote!(Kotlin {
                    if (!candidate.isJsonObject) { return false }
                    $if(!properties.is_empty()) { val jsonObject = candidate.asJsonObject }
                    $C_each(checks)
                    return true
                })
            }
            JsonShape::ObjectWithAdditional { properties, values } => {
                let object = self.condition(
                    &JsonShape::Object(properties.clone()),
                    sigil_quote!(Kotlin { candidate })?,
                )?;
                let check = self.condition(values, sigil_quote!(Kotlin { item.value })?)?;
                let keys: Vec<_> = properties.iter().map(|p| &p.wire_name).collect();
                let text = TypeName::qualified("kotlin", "String");
                sigil_quote!(Kotlin {
                    if (!$C(object)) { return false }
                    val declared = setOf<$T(text)>($for(key in &keys; separator = ", ") { $S(key) })
                    for (item in candidate.asJsonObject.entrySet()) { if (!declared.contains(item.key) && !$C(check)) { return false } }
                    return true
                })
            }
            JsonShape::AnyOf(children) | JsonShape::AllOf(children) => {
                let any = matches!(shape, JsonShape::AnyOf(_));
                let checks = children
                    .iter()
                    .map(|child| {
                        let check = self.condition(child, sigil_quote!(Kotlin { candidate })?)?;
                        if any {
                            sigil_quote!(Kotlin { if ($C(check)) { return true } })
                        } else {
                            sigil_quote!(Kotlin { if (!$C(check)) { return false } })
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                sigil_quote!(Kotlin { $C_each(checks) $if(any) { return false } $else { return true } })
            }
            JsonShape::Reference { schema, scope, .. } => {
                let index = self.index(shape).to_string();
                let named = plan_named_shape(schema, ir);
                let expression = if *scope == MatchScope::Any {
                    self.condition(&named, sigil_quote!(Kotlin { candidate })?)?
                } else {
                    let (concrete, fallback) = selection_shapes(named);
                    let concrete =
                        self.condition(&concrete, sigil_quote!(Kotlin { candidate })?)?;
                    if *scope == MatchScope::Concrete {
                        concrete
                    } else {
                        let fallback =
                            self.condition(&fallback, sigil_quote!(Kotlin { candidate })?)?;
                        sigil_quote!(Kotlin { (!$C(concrete) && $C(fallback)) })?
                    }
                };
                // Node identity avoids structural hashing; nullable lookup, not
                // truthiness, retains failed matches as well as successful ones.
                sigil_quote!(Kotlin {
                    val results = memo.getOrPut(candidate) { mutableMapOf() }
                    val cached = results[$L(index.clone())]
                    if (cached != null) { return cached }
                    val matched = $C(expression)
                    results[$L(index)] = matched
                    return matched
                })
            }
        }
    }
}
