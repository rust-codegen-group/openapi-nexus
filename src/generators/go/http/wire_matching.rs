//! Static Go evidence predicates over the shared authored wire shape graph.

use std::collections::HashMap;

use sigil_stitch::lang::go::Go;
use sigil_stitch::prelude::*;

use crate::ir::types::{IrJsonKind, IrSpec, IrWireShape};

use super::sigil_emit::package_header;

pub(super) const SUPPORT: &str = include_str!("runtime/tagged_wire.go.txt");

#[derive(Default)]
pub(super) struct WireMatching<'ir> {
    shapes: Vec<&'ir IrWireShape>,
    registered: HashMap<*const IrWireShape, usize>,
}

impl<'ir> WireMatching<'ir> {
    pub(super) fn register(&mut self, shape: &'ir IrWireShape, ir: &'ir IrSpec) -> usize {
        let key = std::ptr::from_ref(shape);
        if let Some(&index) = self.registered.get(&key) {
            return index;
        }
        let index = self.shapes.len();
        self.shapes.push(shape);
        self.registered.insert(key, index);
        match shape {
            IrWireShape::Object(object) => {
                for shape in object.properties.values() {
                    self.register(shape, ir);
                }
                self.register(&object.additional_properties, ir);
            }
            IrWireShape::Array(item) => {
                self.register(item, ir);
            }
            IrWireShape::AllOf(parts) | IrWireShape::AnyOf(parts) | IrWireShape::OneOf(parts) => {
                for part in parts {
                    self.register(part, ir);
                }
            }
            IrWireShape::Reference(name) => {
                if let Some(shape) = ir.wire_schemas.get(name) {
                    self.register(shape, ir);
                }
            }
            _ => {}
        }
        index
    }

    fn name(&self, shape: &IrWireShape) -> String {
        shape_name(self.registered[&std::ptr::from_ref(shape)])
    }

    pub(super) fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    pub(super) fn render(&self, ir: &IrSpec) -> Result<String, SigilStitchError> {
        let mut file =
            FileSpec::builder_with("nexus_tagged_evidence.go", Go::new()).header(package_header()?);
        for (index, shape) in self.shapes.iter().enumerate() {
            let function = FunSpec::builder(&shape_name(index))
                .add_param(ParameterSpec::of("value", TypeName::primitive("any")))
                .add_param(ParameterSpec::of("path", path_type()))
                .add_param(ParameterSpec::of("depth", TypeName::primitive("int")))
                .returns(vec![TypeName::primitive("int")])
                .body(self.body(shape, ir)?)
                .build()?;
            file = file.add_function(function);
        }
        file.build()?.render(100)
    }

    fn body(&self, shape: &IrWireShape, ir: &IrSpec) -> Result<CodeBlock, SigilStitchError> {
        match shape {
            IrWireShape::Any => sigil_quote!(Go { return nexusTaggedOpaque }),
            IrWireShape::Never => sigil_quote!(Go { return nexusTaggedNoMatch }),
            IrWireShape::Kind(kind) => kind_body(*kind),
            IrWireShape::Finite(values) => sigil_quote!(Go {
                $for(literal in values) {
                    if nexusTaggedLiteral(value, $S(literal.to_string())) {
                        return nexusTaggedConcrete
                    }
                }
                return nexusTaggedNoMatch
            }),
            IrWireShape::Object(object) => {
                let starting = if !object.properties.is_empty()
                    || !object.required.is_empty()
                    || *object.additional_properties == IrWireShape::Never
                {
                    "nexusTaggedConcrete"
                } else {
                    "nexusTaggedOpaque"
                };
                let extra = self.name(&object.additional_properties);
                let map = TypeName::map(TypeName::primitive("string"), TypeName::primitive("any"));
                sigil_quote!(Go {
                    object, ok := value.($T(map))
                    if !ok { return nexusTaggedNoMatch }
                    $for(key in &object.required) {
                        if _, present := object[$S(key)]; !present { return nexusTaggedNoMatch }
                    }
                    evidence := $N(starting)
                    $if(object.properties.is_empty()) {
                        for _, item := range object {
                            next := $N(extra)(item, path, depth+1)
                            if next == nexusTaggedNoMatch { return nexusTaggedNoMatch }
                            evidence = max(evidence, next)
                        }
                    } $else {
                        for key, item := range object {
                            var next int
                            switch key {
                                $for((key, shape) in &object.properties; separator = "\n") {
                                    case $S(key):
                                        next = $N(self.name(shape))(item, path, depth+1)
                                }
                                default:
                                    next = $N(extra)(item, path, depth+1)
                            }
                            if next == nexusTaggedNoMatch { return nexusTaggedNoMatch }
                            evidence = max(evidence, next)
                        }
                    }
                    return evidence
                })
            }
            IrWireShape::Array(item) => {
                let list = TypeName::slice(TypeName::primitive("any"));
                sigil_quote!(Go {
                    items, ok := value.($T(list))
                    if !ok { return nexusTaggedNoMatch }
                    for _, item := range items {
                        if $N(self.name(item))(item, path, depth+1) == nexusTaggedNoMatch {
                            return nexusTaggedNoMatch
                        }
                    }
                    return nexusTaggedConcrete
                })
            }
            IrWireShape::AllOf(parts) | IrWireShape::AnyOf(parts) | IrWireShape::OneOf(parts) => {
                let all = matches!(shape, IrWireShape::AllOf(_));
                let start = if all {
                    "nexusTaggedOpaque"
                } else {
                    "nexusTaggedNoMatch"
                };
                sigil_quote!(Go {
                    evidence := $N(start)
                    $for((i, part) in parts.iter().enumerate()) {
                        $N(format!("next{i}")) := $N(self.name(part))(value, path, depth)
                        $if(all) {
                            if $N(format!("next{i}")) == nexusTaggedNoMatch { return nexusTaggedNoMatch }
                        } $else {
                            if $N(format!("next{i}")) == nexusTaggedConcrete { return nexusTaggedConcrete }
                        }
                        evidence = max(evidence, $N(format!("next{i}")))
                    }
                    return evidence
                })
            }
            IrWireShape::Reference(name) => {
                if let Some(shape) = ir.wire_schemas.get(name) {
                    let index = self.registered[&std::ptr::from_ref(shape)];
                    sigil_quote!(Go {
                        visit := nexusTaggedVisit{
                            shape: $L(index.to_string()),
                            depth: depth,
                        }
                        if path[visit] { return nexusTaggedNoMatch }
                        path[visit] = true
                        defer delete(path, visit)
                        return $N(self.name(shape))(value, path, depth)
                    })
                } else {
                    sigil_quote!(Go { return nexusTaggedNoMatch })
                }
            }
        }
    }
}

pub(super) fn shape_name(index: usize) -> String {
    format!("nexusTaggedShape{index}")
}

pub(super) fn path_type() -> TypeName {
    TypeName::map(
        TypeName::primitive("nexusTaggedVisit"),
        TypeName::primitive("bool"),
    )
}

fn kind_body(kind: IrJsonKind) -> Result<CodeBlock, SigilStitchError> {
    if kind == IrJsonKind::Null {
        return sigil_quote!(Go {
            if value == nil { return nexusTaggedConcrete }
            return nexusTaggedNoMatch
        });
    }
    let ty = match kind {
        IrJsonKind::Null => unreachable!(),
        IrJsonKind::Boolean => TypeName::primitive("bool"),
        IrJsonKind::Integer | IrJsonKind::Number => {
            TypeName::importable_type("encoding/json", "Number")
        }
        IrJsonKind::String => TypeName::primitive("string"),
        IrJsonKind::Array => TypeName::slice(TypeName::primitive("any")),
        IrJsonKind::Object => {
            TypeName::map(TypeName::primitive("string"), TypeName::primitive("any"))
        }
    };
    let integer = kind == IrJsonKind::Integer;
    let target = if integer { "number" } else { "_" };
    let evidence = if kind == IrJsonKind::Object {
        "nexusTaggedOpaque"
    } else {
        "nexusTaggedConcrete"
    };
    sigil_quote!(Go {
        $N(target), ok := value.($T(ty))
        if !ok { return nexusTaggedNoMatch }
        $if(integer) {
            if !nexusTaggedIsInteger(number) { return nexusTaggedNoMatch }
        }
        return $N(evidence)
    })
}
