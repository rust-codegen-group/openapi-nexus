//! Static, shared predicates for the value plan's structural matching shapes.
//!
//! Small functions keep selection code readable and directly formatter-stable;
//! no schema objects or interpreter are shipped to the generated client.

use heck::ToLowerCamelCase;
use sigil_stitch::error::SigilStitchError;
use sigil_stitch::lang::typescript::TypeScript;
use sigil_stitch::prelude::{CodeBlock, TypeName, sigil_quote};
use sigil_stitch::spec::{
    file_spec::FileSpec, fun_spec::FunSpec, modifiers::Visibility, parameter_spec::ParameterSpec,
};

use crate::generators::type_application;
use crate::generators::union_match::{JsonShape, MatchScope, accepts_any};

use super::layout;
use super::plan::{ByDirection, Direction};

#[derive(Default)]
pub(super) struct MatchPlan {
    shapes: Vec<JsonShape>,
}

#[derive(Clone, Copy)]
enum MatchOutcome {
    Unmatched,
    Fallback,
    Concrete,
}

impl MatchOutcome {
    // Numeric literals are the private generated matcher protocol, not public API.
    fn literal(self) -> &'static str {
        match self {
            Self::Unmatched => "0",
            Self::Fallback => "1",
            Self::Concrete => "2",
        }
    }
}

impl MatchPlan {
    pub(super) fn references(&self) -> std::collections::BTreeSet<String> {
        let mut references = std::collections::BTreeSet::new();
        for shape in &self.shapes {
            crate::generators::union_match::referenced_shapes(shape, &mut references);
        }
        references
    }
    fn constant(shape: &JsonShape) -> Option<bool> {
        match shape {
            JsonShape::Any => Some(true),
            JsonShape::Never => Some(false),
            JsonShape::AnyOf(_) | JsonShape::AllOf(_) | JsonShape::Nullable(_)
                if accepts_any(shape) =>
            {
                Some(true)
            }
            _ => None,
        }
    }

    fn callback(
        &self,
        shape: &JsonShape,
        direction: Direction,
    ) -> Result<CodeBlock, SigilStitchError> {
        match Self::constant(shape) {
            Some(true) => sigil_quote!(TypeScript { () => true }),
            Some(false) => sigil_quote!(TypeScript { () => false }),
            None => sigil_quote!(TypeScript { $T(self.symbol(shape, direction, true)) }),
        }
    }
    pub(super) fn register(&mut self, shape: &JsonShape) {
        if self.shapes.contains(shape) {
            return;
        }
        self.shapes.push(shape.clone());
        match shape {
            JsonShape::Array(inner) | JsonShape::Map(inner) | JsonShape::Nullable(inner) => {
                self.register(inner)
            }
            JsonShape::MapWithConcreteValue { values, concrete } => {
                self.register(&JsonShape::Map(values.clone()));
                self.register(concrete);
            }
            JsonShape::Object(properties) => {
                for property in properties {
                    self.register(&property.shape);
                }
            }
            JsonShape::ObjectWithAdditional { properties, values } => {
                self.register(&JsonShape::Object(properties.clone()));
                self.register(values);
            }
            JsonShape::Literal(serde_json::Value::Array(items)) => {
                for value in items {
                    self.register(&JsonShape::Literal(value.clone()));
                }
            }
            JsonShape::Literal(serde_json::Value::Object(fields)) => {
                for value in fields.values() {
                    self.register(&JsonShape::Literal(value.clone()));
                }
            }
            JsonShape::AnyOf(children) | JsonShape::AllOf(children) => {
                for child in children {
                    self.register(child);
                }
            }
            _ => {}
        }
    }

    fn symbol(&self, shape: &JsonShape, direction: Direction, local: bool) -> TypeName {
        let index = self
            .shapes
            .iter()
            .position(|candidate| candidate == shape)
            .expect("matching shape was registered by the value plan");
        // Schema-derived symbols cannot contain `$`, so helper names cannot
        // collide with named recursive matchers or public model converters.
        let name = format!("matches${index}{}", direction.suffix());
        if local {
            TypeName::primitive(&name)
        } else {
            TypeName::importable("../runtime/unionMatch", &name)
        }
    }

    pub(super) fn condition(
        &self,
        shape: &JsonShape,
        value: CodeBlock,
        direction: Direction,
        local: bool,
    ) -> Result<CodeBlock, SigilStitchError> {
        match Self::constant(shape) {
            Some(true) => return sigil_quote!(TypeScript { true }),
            Some(false) => return sigil_quote!(TypeScript { false }),
            None => {}
        }
        let function = self.symbol(shape, direction, local);
        layout::call(sigil_quote!(TypeScript { $T(function) })?, &[value], 6)
    }

    fn body(
        &self,
        shape: &JsonShape,
        direction: Direction,
        record: &TypeName,
    ) -> Result<CodeBlock, SigilStitchError> {
        let candidate = || sigil_quote!(TypeScript { candidate });
        let record_type = type_application(
            record.clone(),
            vec![
                TypeName::primitive("string"),
                TypeName::primitive("unknown"),
            ],
        );
        let object_guard = sigil_quote!(TypeScript {
            if (candidate === null || typeof candidate !== $S("object") || Array.isArray(candidate)) {
                return false;
            }
        })?;
        match shape {
            JsonShape::Any => sigil_quote!(TypeScript { return true; }),
            JsonShape::Never => sigil_quote!(TypeScript { return false; }),
            JsonShape::Null => sigil_quote!(TypeScript { return candidate === null; }),
            JsonShape::Nullable(inner) => {
                let condition = self.condition(inner, candidate()?, direction, true)?;
                sigil_quote!(TypeScript { return candidate === null || $C(condition); })
            }
            JsonShape::Boolean => {
                sigil_quote!(TypeScript { return typeof candidate === $S("boolean"); })
            }
            JsonShape::String => {
                sigil_quote!(TypeScript { return typeof candidate === $S("string"); })
            }
            JsonShape::Number { integer } => {
                let predicate = if *integer { "isInteger" } else { "isFinite" };
                sigil_quote!(TypeScript { return typeof candidate === $S("number") && Number.$N(predicate)(candidate); })
            }
            JsonShape::Binary => sigil_quote!(TypeScript {
                if (typeof Blob !== $S("undefined") && candidate instanceof Blob) { return true; }
                return typeof File !== $S("undefined") && candidate instanceof File;
            }),
            JsonShape::Literal(value) => match value {
                serde_json::Value::Array(items) => {
                    let count = items.len().to_string();
                    let shape = TypeName::readonly_array(TypeName::primitive("unknown"));
                    let checks = items
                        .iter()
                        .enumerate()
                        .map(|(index, item)| {
                            self.literal_slot(
                                item,
                                sigil_quote!(TypeScript { $L(index.to_string()) })?,
                                direction,
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    sigil_quote!(TypeScript {
                        if (!Array.isArray(candidate) || candidate.length !== $L(count)) { return false; }
                        $if(!checks.is_empty()) { const literal: $T(shape) = candidate; }
                        $C_each(checks)
                        return true;
                    })
                }
                serde_json::Value::Object(fields) => {
                    let count = fields.len().to_string();
                    // Native model converters can materialize omitted optional fields
                    // as undefined, which never become JSON object keys on write.
                    let keys = if direction == Direction::ToWire {
                        sigil_quote!(TypeScript { Object.values(candidate).filter((item) => item !== undefined) })?
                    } else {
                        sigil_quote!(TypeScript { Object.keys(candidate) })?
                    };
                    let checks = fields
                        .iter()
                        .map(|(key, item)| {
                            self.literal_slot(
                                item,
                                sigil_quote!(TypeScript { $S(key) })?,
                                direction,
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    sigil_quote!(TypeScript {
                        $C(object_guard)
                        if ($C(keys).length !== $L(count)) { return false; }
                        $if(!checks.is_empty()) { const literal = candidate as $T(record_type); }
                        $C_each(checks)
                        return true;
                    })
                }
                serde_json::Value::Null => sigil_quote!(TypeScript { return candidate === null; }),
                serde_json::Value::Bool(true) => {
                    sigil_quote!(TypeScript { return candidate === true; })
                }
                serde_json::Value::Bool(false) => {
                    sigil_quote!(TypeScript { return candidate === false; })
                }
                serde_json::Value::String(value) => {
                    sigil_quote!(TypeScript { return candidate === $S(value); })
                }
                serde_json::Value::Number(value) => {
                    // Exact JSON numeric tokens are not identifiers or strings.
                    sigil_quote!(TypeScript { return candidate === $L(value.to_string()); })
                }
            },
            JsonShape::Reference { schema, scope, .. } => {
                let function =
                    TypeName::primitive(&super::wire_matching::matcher_name(schema, direction));
                match scope {
                    MatchScope::Concrete => {
                        sigil_quote!(TypeScript { return $T(function)(candidate) === $L(MatchOutcome::Concrete.literal()); })
                    }
                    MatchScope::Fallback => {
                        sigil_quote!(TypeScript { return $T(function)(candidate) === $L(MatchOutcome::Fallback.literal()); })
                    }
                    MatchScope::Any => {
                        sigil_quote!(TypeScript { return $T(function)(candidate) > $L(MatchOutcome::Unmatched.literal()); })
                    }
                }
            }
            JsonShape::Array(inner) => {
                if accepts_any(inner) {
                    return sigil_quote!(TypeScript { return Array.isArray(candidate); });
                }
                let item = self.callback(inner, direction)?;
                sigil_quote!(TypeScript {
                    if (!Array.isArray(candidate)) { return false; }
                    return candidate.every($C(item));
                })
            }
            JsonShape::Map(inner) => {
                let item = self.callback(inner, direction)?;
                sigil_quote!(TypeScript {
                    $C(object_guard)
                    $if(accepts_any(inner)) { return true; }
                    $else { return Object.values(candidate).every($C(item)); }
                })
            }
            JsonShape::MapWithConcreteValue { values, concrete } => {
                let values = self.condition(
                    &JsonShape::Map(values.clone()),
                    candidate()?,
                    direction,
                    true,
                )?;
                let concrete = self.callback(concrete, direction)?;
                sigil_quote!(TypeScript {
                    if (!$C(values)) { return false; }
                    return Object.values(candidate as $T(record_type)).some($C(concrete));
                })
            }
            JsonShape::Object(properties) => {
                let mut checks = Vec::new();
                for property in properties {
                    let key = if direction == Direction::ToWire && property.rename {
                        property.wire_name.to_lower_camel_case()
                    } else {
                        property.wire_name.clone()
                    };
                    let owns = layout::call(
                        sigil_quote!(TypeScript { Object.prototype.hasOwnProperty.call })?,
                        &[candidate()?, sigil_quote!(TypeScript { $S(key.clone()) })?],
                        6,
                    )?;
                    let any = accepts_any(&property.shape);
                    let check = self.condition(
                        &property.shape,
                        sigil_quote!(TypeScript { object[$S(key)] })?,
                        direction,
                        true,
                    )?;
                    checks.push(if property.required {
                        sigil_quote!(TypeScript {
                            if (!$C(owns)) { return false; }
                            $if(!any) { if (!$C(check)) { return false; } }
                        })?
                    } else if any {
                        continue;
                    } else {
                        let item_check = self.condition(
                            &property.shape,
                            sigil_quote!(TypeScript { item })?,
                            direction,
                            true,
                        )?;
                        sigil_quote!(TypeScript {
                            if ($C(owns)) {
                                const item = object[$S(key)];
                                if (item !== undefined && !$C(item_check)) { return false; }
                            }
                        })?
                    });
                }
                let typed = properties
                    .iter()
                    .any(|property| !accepts_any(&property.shape));
                sigil_quote!(TypeScript {
                    $C(object_guard)
                    $if(typed) { const object = candidate as $T(record_type); }
                    $C_each(checks)
                    return true;
                })
            }
            JsonShape::ObjectWithAdditional { properties, values } => {
                if properties.is_empty() {
                    // The authored object remains distinct while planning the
                    // evidence tier; membership itself is a dictionary check.
                    return self.body(&JsonShape::Map(values.clone()), direction, &record_type);
                }
                let object = self.condition(
                    &JsonShape::Object(properties.clone()),
                    candidate()?,
                    direction,
                    true,
                )?;
                if accepts_any(values) {
                    return sigil_quote!(TypeScript { return $C(object); });
                }
                let item = self.callback(values, direction)?;
                let item_check =
                    self.condition(values, sigil_quote!(TypeScript { item })?, direction, true)?;
                let keys: Vec<_> = properties
                    .iter()
                    .map(|property| {
                        if direction == Direction::ToWire && property.rename {
                            property.wire_name.to_lower_camel_case()
                        } else {
                            property.wire_name.clone()
                        }
                    })
                    .collect();
                let retained = direction == Direction::ToWire
                    && properties.iter().any(|property| {
                        property.rename
                            && property.wire_name != property.wire_name.to_lower_camel_case()
                    });
                let retained_type =
                    TypeName::union(vec![record_type.clone(), TypeName::primitive("undefined")]);
                sigil_quote!(TypeScript {
                    if (!$C(object)) { return false; }
                    const object = candidate as $T(record_type);
                    const declared = [$for(key in &keys; separator = ", ") { $S(key) }];
                    for (const [key, item] of Object.entries(object)) {
                        if (!declared.includes(key) && !$C(item_check)) { return false; }
                    }
                    $if(retained) {
                        const metadata = Symbol.for($S(super::sigil_emit::RETAINED_EXTRAS));
                        const descriptor = Object.getOwnPropertyDescriptor(candidate, metadata);
                        const retained = descriptor?.value as $T(retained_type);
                        if (retained !== undefined) { return Object.values(retained).every($C(item)); }
                    }
                    return true;
                })
            }
            JsonShape::AnyOf(children) | JsonShape::AllOf(children) => {
                if accepts_any(shape) {
                    return sigil_quote!(TypeScript { return true; });
                }
                let disjunction = matches!(shape, JsonShape::AnyOf(_));
                let checks = children
                    .iter()
                    .filter(|child| disjunction || !accepts_any(child))
                    .map(|child| {
                        let condition = self.condition(child, candidate()?, direction, true)?;
                        if disjunction {
                            sigil_quote!(TypeScript { if ($C(condition)) { return true; } })
                        } else {
                            sigil_quote!(TypeScript { if (!$C(condition)) { return false; } })
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                sigil_quote!(TypeScript {
                    $C_each(checks)
                    $if(disjunction) { return false; } $else { return true; }
                })
            }
        }
    }

    fn literal_slot(
        &self,
        expected: &serde_json::Value,
        key: CodeBlock,
        direction: Direction,
    ) -> Result<CodeBlock, SigilStitchError> {
        let owns = layout::call(
            sigil_quote!(TypeScript { Object.prototype.hasOwnProperty.call })?,
            &[sigil_quote!(TypeScript { literal })?, key.clone()],
            6,
        )?;
        let condition = self.condition(
            &JsonShape::Literal(expected.clone()),
            sigil_quote!(TypeScript { literal[$C(key)] })?,
            direction,
            true,
        )?;
        sigil_quote!(TypeScript {
            if (!$C(owns)) { return false; }
            if (!$C(condition)) { return false; }
        })
    }

    pub(super) fn file(
        &self,
        recursive: &std::collections::BTreeMap<String, ByDirection<(JsonShape, JsonShape)>>,
        record: &TypeName,
        ts: &TypeScript,
    ) -> Result<Option<FileSpec>, SigilStitchError> {
        if self
            .shapes
            .iter()
            .all(|shape| Self::constant(shape).is_some())
            && recursive.is_empty()
        {
            return Ok(None);
        }
        let mut file = FileSpec::builder_with("unionMatch.ts", ts.clone());
        for shape in &self.shapes {
            if Self::constant(shape).is_some() {
                continue;
            }
            for direction in [Direction::FromWire, Direction::ToWire] {
                let function = FunSpec::builder(&format!(
                    "matches${}{}",
                    self.shapes.iter().position(|item| item == shape).unwrap(),
                    direction.suffix()
                ))
                .visibility(Visibility::Public)
                .add_param(ParameterSpec::of(
                    "candidate",
                    TypeName::primitive("unknown"),
                ))
                .returns(vec![TypeName::primitive("boolean")])
                .body(self.body(shape, direction, record)?)
                .build()?;
                file = file.add_function(function);
            }
        }
        for (name, tiers) in recursive {
            for direction in [Direction::FromWire, Direction::ToWire] {
                let (concrete, fallback) = tiers.get(direction);
                let candidate = sigil_quote!(TypeScript { candidate })?;
                let concrete_condition =
                    self.condition(concrete, candidate.clone(), direction, true)?;
                let fallback_condition = self.condition(fallback, candidate, direction, true)?;
                let body = sigil_quote!(TypeScript {
                    $if(*concrete != JsonShape::Never) { if ($C(concrete_condition)) { return $L(MatchOutcome::Concrete.literal()); } }
                    $if(accepts_any(fallback)) { return $L(MatchOutcome::Fallback.literal()); }
                    $else {
                        $if(*fallback != JsonShape::Never) { if ($C(fallback_condition)) { return $L(MatchOutcome::Fallback.literal()); } }
                        return $L(MatchOutcome::Unmatched.literal());
                    }
                })?;
                file = file.add_function(
                    FunSpec::builder(&super::wire_matching::matcher_name(name, direction))
                        .visibility(Visibility::Public)
                        .add_param(ParameterSpec::of(
                            "candidate",
                            TypeName::primitive("unknown"),
                        ))
                        .returns(vec![TypeName::primitive("number")])
                        .body(body)
                        .build()?,
                );
            }
        }
        file.build().map(Some)
    }
}

// Use the same planned wire predicates as a native untagged converter when
// deciding how one static authored constant would be represented by its model.
// A productive runtime reference is not a form of proof here.
pub(super) fn match_wire_literal(shape: &JsonShape, value: &serde_json::Value) -> Option<bool> {
    if let Some(constant) = MatchPlan::constant(shape) {
        return Some(constant);
    }
    let properties = |properties: &[crate::generators::union_match::PropertyMatch]| {
        let Some(object) = value.as_object() else {
            return Some(false);
        };
        all_literal_checks(properties.iter().map(|property| {
            object
                .get(&property.wire_name)
                .map_or(Some(!property.required), |item| {
                    match_wire_literal(&property.shape, item)
                })
        }))
    };
    match shape {
        JsonShape::Any | JsonShape::Never => unreachable!("constant predicate already handled"),
        JsonShape::Null => Some(value.is_null()),
        JsonShape::Nullable(_) if value.is_null() => Some(true),
        JsonShape::Nullable(inner) => match_wire_literal(inner, value),
        JsonShape::Boolean => Some(value.is_boolean()),
        JsonShape::String => Some(value.is_string()),
        JsonShape::Number { integer } => Some(
            value
                .as_f64()
                .is_some_and(|number| number.is_finite() && (!integer || number.fract() == 0.0)),
        ),
        JsonShape::Binary => Some(false),
        JsonShape::Literal(expected) => Some(literal_eq(value, expected)),
        JsonShape::Array(inner) => value.as_array().map_or(Some(false), |items| {
            all_literal_checks(items.iter().map(|item| match_wire_literal(inner, item)))
        }),
        JsonShape::Map(inner) => value.as_object().map_or(Some(false), |items| {
            all_literal_checks(items.values().map(|item| match_wire_literal(inner, item)))
        }),
        JsonShape::MapWithConcreteValue { values, concrete } => {
            value.as_object().map_or(Some(false), |items| {
                all_literal_checks([
                    all_literal_checks(items.values().map(|item| match_wire_literal(values, item))),
                    any_literal_check(
                        items
                            .values()
                            .map(|item| match_wire_literal(concrete, item)),
                    ),
                ])
            })
        }
        JsonShape::Object(fields) => properties(fields),
        JsonShape::ObjectWithAdditional {
            properties: fields,
            values,
        } => {
            let Some(object) = value.as_object() else {
                return Some(false);
            };
            let extra = object
                .iter()
                .filter(|(key, _)| !fields.iter().any(|prop| &prop.wire_name == *key))
                .map(|(_, value)| match_wire_literal(values, value));
            all_literal_checks(std::iter::once(properties(fields)).chain(extra))
        }
        JsonShape::AnyOf(parts) => {
            any_literal_check(parts.iter().map(|part| match_wire_literal(part, value)))
        }
        JsonShape::AllOf(parts) => {
            all_literal_checks(parts.iter().map(|part| match_wire_literal(part, value)))
        }
        JsonShape::Reference { .. } => None,
    }
}

fn all_literal_checks(checks: impl IntoIterator<Item = Option<bool>>) -> Option<bool> {
    let mut unresolved = false;
    for check in checks {
        match check {
            Some(false) => return Some(false),
            Some(true) => (),
            None => unresolved = true,
        }
    }
    (!unresolved).then_some(true)
}

fn any_literal_check(checks: impl IntoIterator<Item = Option<bool>>) -> Option<bool> {
    all_literal_checks(checks.into_iter().map(|check| check.map(|result| !result)))
        .map(|result| !result)
}

fn literal_eq(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    use serde_json::Value;
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.as_f64() == right.as_f64(),
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| literal_eq(left, right))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    right.get(key).is_some_and(|other| literal_eq(value, other))
                })
        }
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sigil_stitch::assert_rendered;

    #[test]
    fn union_literal_and_numeric_guards_render_exactly() {
        let plan = MatchPlan::default();
        let integer = plan
            .body(
                &JsonShape::Number { integer: true },
                Direction::FromWire,
                &TypeName::primitive("Record"),
            )
            .unwrap();
        assert_rendered!(
            TypeScript::new(),
            integer,
            "return typeof candidate === 'number' && Number.isInteger(candidate);\n"
        );
        let boolean = plan
            .body(
                &JsonShape::Literal(false.into()),
                Direction::FromWire,
                &TypeName::primitive("Record"),
            )
            .unwrap();
        assert_rendered!(TypeScript::new(), boolean, "return candidate === false;\n");
    }

    #[test]
    fn exact_composite_guard_checks_length_own_keys_and_child_literals() {
        let shape = JsonShape::Literal(serde_json::json!({"__proto__": [1, true]}));
        let mut plan = MatchPlan::default();
        plan.register(&shape);
        let body = plan
            .body(&shape, Direction::FromWire, &TypeName::primitive("Record"))
            .unwrap();
        assert_rendered!(
            TypeScript::new(),
            body,
            "if (candidate === null || typeof candidate !== 'object' || Array.isArray(candidate)) {\n  return false;\n}\nif (Object.keys(candidate).length !== 1) {\n  return false;\n}\nconst literal = candidate as Record<string, unknown>;\nif (!Object.prototype.hasOwnProperty.call(literal, '__proto__')) {\n  return false;\n}\nif (!matches$1FromJSON(literal['__proto__'])) {\n  return false;\n}\nreturn true;\n"
        );
    }

    #[test]
    fn binary_union_guard_renders_blob_and_file_checks_exactly() {
        let plan = MatchPlan::default();
        for direction in [Direction::FromWire, Direction::ToWire] {
            let body = plan
                .body(
                    &JsonShape::Binary,
                    direction,
                    &TypeName::primitive("Record"),
                )
                .unwrap();
            assert_rendered!(
                TypeScript::new(),
                body,
                "if (typeof Blob !== 'undefined' && candidate instanceof Blob) {\n  return true;\n}\nreturn typeof File !== 'undefined' && candidate instanceof File;\n"
            );
        }
    }

    #[test]
    fn open_dictionary_guard_requires_at_least_one_concrete_value() {
        let shape = JsonShape::MapWithConcreteValue {
            values: Box::new(JsonShape::Any),
            concrete: Box::new(JsonShape::String),
        };
        let mut plan = MatchPlan::default();
        plan.register(&shape);
        let body = plan
            .body(
                &shape,
                Direction::FromWire,
                &TypeName::qualified("globalThis", "Record"),
            )
            .unwrap();
        assert_rendered!(
            layout::TypeScript::from(TypeScript::new()),
            body,
            "if (!matches$1FromJSON(candidate)) {\n  return false;\n}\nreturn Object.values(candidate as globalThis.Record<string, unknown>).some(matches$3FromJSON);\n"
        );
    }
}
