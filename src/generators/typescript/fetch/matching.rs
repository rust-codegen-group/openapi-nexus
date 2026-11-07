//! Static, shared predicates for the value plan's structural matching shapes.
//!
//! Small functions keep selection code readable and directly formatter-stable;
//! no schema objects or interpreter are shipped to the generated client.

use heck::{ToLowerCamelCase, ToPascalCase};
use sigil_stitch::error::SigilStitchError;
use sigil_stitch::lang::typescript::TypeScript;
use sigil_stitch::prelude::{CodeBlock, TypeName, sigil_quote};
use sigil_stitch::spec::{
    file_spec::FileSpec, fun_spec::FunSpec, modifiers::Visibility, parameter_spec::ParameterSpec,
};

use crate::generators::type_application;
use crate::generators::union_match::{JsonShape, MatchScope, accepts_any, selection_shapes};

use super::layout;
use super::plan::Direction;

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
            JsonShape::Literal(serde_json::Value::Array(_) | serde_json::Value::Object(_)) => {
                Some(false)
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
                _ => sigil_quote!(TypeScript { return false; }),
            },
            JsonShape::Reference { schema, scope, .. } => {
                let function = TypeName::primitive(&format!(
                    "match{}{}",
                    schema.to_pascal_case(),
                    direction.suffix()
                ));
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
                        const metadata = Symbol.for($S("openapi-nexus.additionalProperties"));
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

    pub(super) fn file(
        &self,
        recursive: &std::collections::BTreeMap<String, JsonShape>,
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
        for (name, shape) in recursive {
            let (concrete, fallback) = selection_shapes(shape.clone());
            for direction in [Direction::FromWire, Direction::ToWire] {
                let candidate = sigil_quote!(TypeScript { candidate })?;
                let concrete_condition =
                    self.condition(&concrete, candidate.clone(), direction, true)?;
                let fallback_condition = self.condition(&fallback, candidate, direction, true)?;
                let body = sigil_quote!(TypeScript {
                    $if(concrete != JsonShape::Never) { if ($C(concrete_condition)) { return $L(MatchOutcome::Concrete.literal()); } }
                    $if(accepts_any(&fallback)) { return $L(MatchOutcome::Fallback.literal()); }
                    $else {
                        $if(fallback != JsonShape::Never) { if ($C(fallback_condition)) { return $L(MatchOutcome::Fallback.literal()); } }
                        return $L(MatchOutcome::Unmatched.literal());
                    }
                })?;
                file = file.add_function(
                    FunSpec::builder(&format!(
                        "match{}{}",
                        name.to_pascal_case(),
                        direction.suffix()
                    ))
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
