//! Rust rendering of the shared authored JSON predicates used by inferred unions.

use sigil_stitch::error::SigilStitchError;
use sigil_stitch::prelude::{CodeBlock, TypeName, sigil_quote};
use sigil_stitch::spec::{fun_spec::FunSpec, parameter_spec::ParameterSpec};

use crate::generators::type_application;
use crate::ir::types::{IrJsonKind, IrSpec, IrWireShape};

pub(super) fn value_type() -> TypeName {
    TypeName::qualified("serde_json", "Value")
}

#[derive(Default)]
pub(super) struct WireMatching {
    shapes: Vec<IrWireShape>,
}

impl WireMatching {
    pub(super) fn register(&mut self, shape: &IrWireShape, ir: &IrSpec) {
        if self.shapes.contains(shape) {
            return;
        }
        self.shapes.push(shape.clone());
        match shape {
            IrWireShape::Array(items) => self.register(items, ir),
            IrWireShape::Object(object) => {
                for property in object.properties.values() {
                    self.register(property, ir);
                }
                self.register(&object.additional_properties, ir);
            }
            IrWireShape::AllOf(parts) | IrWireShape::AnyOf(parts) | IrWireShape::OneOf(parts) => {
                for part in parts {
                    self.register(part, ir);
                }
            }
            IrWireShape::Reference(name) => {
                if let Some(referenced) = ir.wire_schemas.get(name) {
                    self.register(referenced, ir);
                }
            }
            _ => {}
        }
    }

    pub(super) fn uses_memo(&self) -> bool {
        self.shapes
            .iter()
            .any(|shape| matches!(shape, IrWireShape::Reference(_)))
    }

    pub(super) fn needs_memo(shape: &IrWireShape) -> bool {
        match shape {
            IrWireShape::Reference(_) => true,
            IrWireShape::Array(items) => Self::needs_memo(items),
            IrWireShape::Object(object) => {
                object.properties.values().any(Self::needs_memo)
                    || Self::needs_memo(&object.additional_properties)
            }
            IrWireShape::AllOf(parts) | IrWireShape::AnyOf(parts) | IrWireShape::OneOf(parts) => {
                parts.iter().any(Self::needs_memo)
            }
            _ => false,
        }
    }

    fn index(&self, shape: &IrWireShape) -> usize {
        self.shapes
            .iter()
            .position(|candidate| candidate == shape)
            .expect("authored wire shape was registered")
    }

    fn symbol(&self, shape: &IrWireShape) -> String {
        format!("__nexus_wire_match_{}", self.index(shape))
    }

    /// `0` is not eligible, `1` is opaque, `2` is concrete. Root dispatch uses
    /// the same evidence ordering as the reference UnionWirePlan evaluator.
    pub(super) fn evidence(
        &self,
        shape: &IrWireShape,
        value: CodeBlock,
    ) -> Result<CodeBlock, SigilStitchError> {
        let symbol = self.symbol(shape);
        sigil_quote!(RustLang {
            $N(symbol)($C(value)$if(Self::needs_memo(shape)) { , memo })
        })
    }

    pub(super) fn functions(&self, ir: &IrSpec) -> Result<Vec<FunSpec>, SigilStitchError> {
        self.shapes
            .iter()
            .map(|shape| {
                let value = match shape {
                    IrWireShape::Any | IrWireShape::Never => "_candidate",
                    IrWireShape::Reference(name) if !ir.wire_schemas.contains_key(name) => {
                        "_candidate"
                    }
                    _ => "candidate",
                };
                let mut function = FunSpec::builder(&self.symbol(shape))
                    .add_param(ParameterSpec::of(value, TypeName::reference(value_type())))
                    .returns(vec![TypeName::primitive("u8")]);
                if Self::needs_memo(shape) {
                    let memo = type_application(
                        TypeName::qualified("std::collections", "HashMap"),
                        vec![
                            TypeName::tuple(vec![
                                TypeName::primitive("usize"),
                                TypeName::pointer(value_type()),
                            ]),
                            TypeName::primitive("u8"),
                        ],
                    );
                    function = function.add_param(ParameterSpec::of(
                        if value == "_candidate" {
                            "_memo"
                        } else {
                            "memo"
                        },
                        TypeName::reference_mut(memo),
                    ));
                }
                function.body(self.body(shape, ir)?).build()
            })
            .collect()
    }

    fn body(&self, shape: &IrWireShape, ir: &IrSpec) -> Result<CodeBlock, SigilStitchError> {
        match shape {
            IrWireShape::Any => sigil_quote!(RustLang { 1 }),
            IrWireShape::Never => sigil_quote!(RustLang { 0 }),
            IrWireShape::Kind(kind) => kind_body(*kind),
            IrWireShape::Finite(values) => {
                let checks = values
                    .iter()
                    .map(|value| {
                        let condition = match value {
                            serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                                let wire = value.to_string();
                                sigil_quote!(RustLang {
                                    serde_json::from_str::<serde_json::Value>($S(wire))
                                        .is_ok_and(|value| &value == candidate)
                                })?
                            }
                            _ => super::matching::literal_body(value)?,
                        };
                        // Number literals can use an early return inside the helper.
                        sigil_quote!(RustLang { if (|| { $C(condition) })() { return 2; } })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                sigil_quote!(RustLang { $C_each(checks) 0 })
            }
            IrWireShape::Array(items) => {
                let item = self.evidence(items, sigil_quote!(RustLang { item })?)?;
                sigil_quote!(RustLang {
                    if candidate.as_array().is_some_and(|items| items.iter().all(|item| $C(item) != 0)) { 2 } else { 0 }
                })
            }
            IrWireShape::Object(object) => {
                let required = &object.required;
                let baselines_are_concrete = !object.properties.is_empty()
                    || !required.is_empty()
                    || *object.additional_properties == IrWireShape::Never;
                let branches = object
                    .properties
                    .iter()
                    .map(|(key, shape)| {
                        let check = self.evidence(shape, sigil_quote!(RustLang { value })?)?;
                        sigil_quote!(RustLang { $S(key) => $C(check), })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let extra = self.evidence(
                    &object.additional_properties,
                    sigil_quote!(RustLang { value })?,
                )?;
                sigil_quote!(RustLang {
                    let object = match candidate.as_object() {
                        Some(object) => object,
                        None => return 0,
                    };
                    $for(key in required) {
                        if object.get($S(key)).is_none() { return 0; }
                    }
                    let mut evidence = $if(baselines_are_concrete) { 2 } $else { 1 };
                    for ($if(branches.is_empty()) { _key } $else { key }, value) in object {
                        let matched = $if(branches.is_empty()) { $C(extra) } $else {
                            match key.as_str() {
                                $C_each(branches)
                                _ => $C(extra),
                            }
                        };
                        if matched == 0 { return 0; }
                        evidence = evidence.max(matched);
                    }
                    evidence
                })
            }
            IrWireShape::AllOf(parts) | IrWireShape::AnyOf(parts) | IrWireShape::OneOf(parts) => {
                let conjunction = matches!(shape, IrWireShape::AllOf(_));
                let checks = parts
                    .iter()
                    .map(|part| {
                        let evidence =
                            self.evidence(part, sigil_quote!(RustLang { candidate })?)?;
                        sigil_quote!(RustLang {
                            let matched = $C(evidence);
                            $if(conjunction) { if matched == 0 { return 0; } }
                            $else { if matched == 2 { return 2; } }
                            evidence = evidence.max(matched);
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                sigil_quote!(RustLang {
                    let mut evidence = $if(conjunction) { 1 } $else { 0 };
                    $C_each(checks)
                    evidence
                })
            }
            IrWireShape::Reference(name) => {
                let Some(referenced) = ir.wire_schemas.get(name) else {
                    return sigil_quote!(RustLang { 0 });
                };
                let index = self.index(shape).to_string();
                let check = self.evidence(referenced, sigil_quote!(RustLang { candidate })?)?;
                sigil_quote!(RustLang {
                    let key = ($L(index), std::ptr::from_ref(candidate));
                    if let Some(result) = memo.get(&key) { return *result; }
                    memo.insert(key, 0);
                    let evidence = $C(check);
                    memo.insert(key, evidence);
                    evidence
                })
            }
        }
    }
}

fn kind_body(kind: IrJsonKind) -> Result<CodeBlock, SigilStitchError> {
    let (method, evidence) = match kind {
        IrJsonKind::Null => ("is_null", "2"),
        IrJsonKind::Boolean => ("is_boolean", "2"),
        IrJsonKind::Number => ("is_number", "2"),
        IrJsonKind::String => ("is_string", "2"),
        IrJsonKind::Array => ("is_array", "2"),
        IrJsonKind::Object => ("is_object", "1"),
        IrJsonKind::Integer => {
            return sigil_quote!(RustLang {
                if candidate.is_i64() || candidate.is_u64() || candidate.as_f64().is_some_and(|number| number.fract() == 0.0) {
                    2
                } else { 0 }
            });
        }
    };
    sigil_quote!(RustLang { if candidate.$N(method)() { $L(evidence) } else { 0 } })
}
