//! Explicit extension storage and flat object conversion for Python models.

use std::collections::BTreeSet;

use sigil_stitch::prelude::*;

use super::emit_models::python_field_name;
use super::value::{Direction, ValuePlan};
use crate::generators::type_application;
use crate::ir::types::{IrObject, IrTypeExpr};

pub(super) struct AdditionalProperties<'a> {
    name: String,
    values: &'a IrTypeExpr,
    known: Vec<&'a str>,
}

impl<'a> AdditionalProperties<'a> {
    pub fn new(object: &'a IrObject) -> Option<Self> {
        let values = object.additional_properties.as_ref()?;
        let used = object
            .properties
            .values()
            .map(|property| python_field_name(&property.name))
            .collect::<BTreeSet<_>>();
        let mut name = "additional_properties".to_string();
        while used.contains(&name) {
            name.push('_');
        }
        Some(Self {
            name,
            values,
            known: object.properties.keys().map(String::as_str).collect(),
        })
    }

    pub fn field(&self, schema: &str, plan: &ValuePlan<'_>) -> FieldSpec {
        let factory = TypeName::importable("dataclasses", "field").with_alias("_dataclass_field");
        let dictionary = TypeName::importable("builtins", "dict").with_alias("_dict");
        let key = TypeName::importable("builtins", "str").with_alias("_str");
        let container =
            type_application(dictionary, vec![key, plan.model_type(self.values, schema)]);
        // Specialize the factory for strict typing, but defer recursive model
        // references until construction, after their modules finish importing.
        FieldSpec::builder(&self.name, container.clone())
        .initializer(
            sigil_quote!(Python {
                $T(factory)(default_factory=lambda: $T(container)(), kw_only=True, metadata={"openapi_additional_properties": True, "openapi_dictionary": $if(self.known.is_empty()) { True } $else { False }})
            })
            .expect("Python extension storage initializer builds")
            .__sigil_trim_trailing_newline(),
        )
        .build()
        .expect("Python extension storage field builds")
    }

    pub fn write_entries(&self, plan: &ValuePlan<'_>) -> CodeBlock {
        let value = plan.call(
            self.values,
            sigil_quote!(Python { item }).expect("Python extension value access builds"),
            Direction::To,
        );
        sigil_quote!(Python {
            for key, item in self.$N(self.name.as_str()).items() {
                $if(!self.known.is_empty()) {
                    if key in [$for(key in &self.known; separator = ", ") { $S(*key) }] {
                        raise ValueError("Additional property collides with declared wire field: " + key)
                    }
                }
                result[key] = $C(value)
            }
        })
        .expect("Python flat extension serialization builds")
    }

    pub fn read_entries(&self, schema: &str, plan: &ValuePlan<'_>) -> CodeBlock {
        let value = plan.call(
            self.values,
            sigil_quote!(Python { item }).expect("Python extension wire value builds"),
            Direction::From,
        );
        let value = plan.coerce(self.values, plan.model_type(self.values, schema), value);
        sigil_quote!(Python {
            $N(self.name.as_str()) = {key: $C(value) for key, item in data.items() $if(!self.known.is_empty()) { if key not in [$for(key in &self.known; separator = ", ") { $S(*key) }] }},
        })
        .expect("Python typed extension deserialization builds")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::types::IrSchemaKind;
    use sigil_stitch::{assert_rendered, lang::python::Python};

    #[test]
    fn storage_factory_aliases_and_collision_name_render_exactly() {
        let source = "openapi: 3.1.0\ninfo: {title: Storage, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    Model:\n      type: object\n      properties:\n        additionalProperties: {type: string}\n      additionalProperties: {type: string}\n";
        let ir =
            crate::ir::lower::lower(crate::parser::parse_content_yaml(source).unwrap()).unwrap();
        let IrSchemaKind::Object(object) = &ir.schemas["Model"].kind else {
            panic!("extension model retains object facts");
        };
        let plan = ValuePlan::new(&ir, super::super::value::UntaggedUnionStrategy::FirstMatch);
        let storage = AdditionalProperties::new(object).unwrap();
        let declaration = TypeSpec::builder("Model", TypeKind::Class)
            .annotate(AnnotationSpec::importable(TypeName::importable(
                "dataclasses",
                "dataclass",
            )))
            .add_field(storage.field("Model", &plan))
            .build()
            .unwrap()
            .emit(&Python::new())
            .unwrap();
        let body = sigil_quote!(Python { $C_each(declaration) }).unwrap();
        assert_rendered!(
            Python::new(),
            width = 300,
            body,
            "from builtins import dict as _dict, str as _str\nfrom dataclasses import dataclass, field as _dataclass_field\n\n@dataclass\nclass Model:\n    additional_properties_: _dict[_str, str] = _dataclass_field(default_factory = lambda: _dict[_str, str](), kw_only = True, metadata = {\"openapi_additional_properties\": True, \"openapi_dictionary\": False})\n\n",
        );
    }
}
