//! Emit Python evidence predicates over the shared authored wire shapes.

use sigil_stitch::error::SigilStitchError;
use sigil_stitch::prelude::*;

use crate::ir::types::{IrJsonKind, IrSpec, IrWireShape};

use super::value::python_literal;

#[derive(Default)]
pub(super) struct WireMatching {
    shapes: Vec<IrWireShape>,
}

impl WireMatching {
    pub(super) fn predicate(&mut self, shape: &IrWireShape) -> String {
        let index = self
            .shapes
            .iter()
            .position(|item| item == shape)
            .unwrap_or_else(|| {
                let index = self.shapes.len();
                self.shapes.push(shape.clone());
                index
            });
        name(index)
    }

    pub(super) fn has_predicates(&self) -> bool {
        !self.shapes.is_empty()
    }

    pub(super) fn functions(&mut self, ir: &IrSpec) -> Result<Vec<FunSpec>, SigilStitchError> {
        let mut functions = Vec::new();
        let mut index = 0;
        while let Some(shape) = self.shapes.get(index).cloned() {
            let body = self.body(&shape, ir, index)?;
            functions.push(
                FunSpec::builder(&name(index))
                    .add_param(ParameterSpec::of("value", TypeName::primitive("object")))
                    .add_param(ParameterSpec::of(
                        "memo",
                        TypeName::qualified("_tagged_support", "EvidenceMemo"),
                    ))
                    .returns(vec![TypeName::primitive("int")])
                    .body(body)
                    .build()?,
            );
            index += 1;
        }
        Ok(functions)
    }

    fn body(
        &mut self,
        shape: &IrWireShape,
        ir: &IrSpec,
        index: usize,
    ) -> Result<CodeBlock, SigilStitchError> {
        match shape {
            IrWireShape::Any => sigil_quote!(Python { return 1 }),
            IrWireShape::Never => sigil_quote!(Python { return 0 }),
            IrWireShape::Kind(IrJsonKind::Object) => sigil_quote!(Python {
                return 1 if _union_support.is_dict(value) else 0
            }),
            IrWireShape::Kind(kind) => {
                let condition = match kind {
                    IrJsonKind::Null => sigil_quote!(Python { value is None }),
                    IrJsonKind::Boolean => sigil_quote!(Python { isinstance(value, bool) }),
                    IrJsonKind::Integer => {
                        sigil_quote!(Python { _union_support.number(value, True) })
                    }
                    IrJsonKind::Number => {
                        sigil_quote!(Python { _union_support.number(value, False) })
                    }
                    IrJsonKind::String => sigil_quote!(Python { isinstance(value, str) }),
                    IrJsonKind::Array => sigil_quote!(Python { _union_support.is_list(value) }),
                    IrJsonKind::Object => unreachable!(),
                }?;
                sigil_quote!(Python { return 2 if $C(condition) else 0 })
            }
            IrWireShape::Finite(values) => {
                if values.is_empty() {
                    return sigil_quote!(Python { return 0 });
                }
                let values = values
                    .iter()
                    .map(python_literal)
                    .collect::<Result<Vec<_>, _>>()?;
                sigil_quote!(Python {
                    candidates: list[object] = [$for(literal in &values; separator = ", ") { $C(literal.clone()) }]
                    return 2 if any(_union_support.equal(value, candidate) for candidate in candidates) else 0
                })
            }
            IrWireShape::Array(item) => {
                let predicate = self.predicate(item);
                sigil_quote!(Python {
                    if not _union_support.is_list(value) { return 0 }
                    return 2 if all($N(predicate)(item, memo) for item in value) else 0
                })
            }
            IrWireShape::Object(object) => {
                let opaque = object.properties.is_empty()
                    && object.required.is_empty()
                    && !matches!(*object.additional_properties, IrWireShape::Never);
                let required = object
                    .required
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                let properties = object
                    .properties
                    .iter()
                    .map(|(key, value)| (key.as_str(), self.predicate(value)))
                    .collect::<Vec<_>>();
                let additional = self.predicate(&object.additional_properties);
                sigil_quote!(Python {
                    if not _union_support.is_dict(value) { return 0 }
                    $if(!required.is_empty()) {
                        required = [$for(key in &required; separator = ", ") { $S(*key) }]
                        if any(key not in value for key in required) { return 0 }
                    }
                    evidence = $if(opaque) { 1 } $else { 2 }
                    $if(!properties.is_empty()) {
                        properties: dict[str, _tagged_support.Matcher] = {}
                        $for((key, predicate) in &properties) { properties[$S(*key)] = $N(predicate.as_str()) }
                        for key, item in value.items() {
                            matched = properties.get(key, $N(additional.as_str()))(item, memo)
                            if not matched { return 0 }
                            evidence = max(evidence, matched)
                        }
                    } $else {
                        for item in value.values() {
                            matched = $N(additional.as_str())(item, memo)
                            if not matched { return 0 }
                            evidence = max(evidence, matched)
                        }
                    }
                    return evidence
                })
            }
            IrWireShape::AllOf(parts) | IrWireShape::OneOf(parts) | IrWireShape::AnyOf(parts) => {
                let all = matches!(shape, IrWireShape::AllOf(_));
                let checks = parts
                    .iter()
                    .map(|part| {
                        let predicate = self.predicate(part);
                        sigil_quote!(Python {
                            matched = $N(predicate)(value, memo)
                            $if(all) { if not matched { return 0 } }
                            $else { if matched == 2 { return 2 } }
                            evidence = max(evidence, matched)
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                sigil_quote!(Python {
                    evidence = $if(all) { 1 } $else { 0 }
                    $C_each(checks)
                    return evidence
                })
            }
            IrWireShape::Reference(schema) => {
                let Some(shape) = ir.wire_schemas.get(schema) else {
                    return sigil_quote!(Python { return 0 });
                };
                let predicate = self.predicate(shape);
                let index = index.to_string();
                // The value graph is immutable while matching. A repeat at
                // the same value is a non-progressing reference cycle.
                sigil_quote!(Python {
                    key = ($L(index), id(value))
                    if key not in memo {
                        memo[key] = 0
                        memo[key] = $N(predicate)(value, memo)
                    }
                    return memo[key]
                })
            }
        }
    }
}

fn name(index: usize) -> String {
    format!("_matches_wire_{index}")
}
