//! Preserve Go member entrypoints while composing one flat JSON object.

use std::collections::{BTreeMap, BTreeSet};

use heck::ToPascalCase;
use sigil_stitch::lang::go::Go;
use sigil_stitch::prelude::*;

use crate::ir::types::{
    IrEnumValueType, IrIntersection, IrPrimitive, IrSchema, IrSchemaKind, IrSpec, IrTypeExpr,
};

use super::config::AllOfStrategy;
use super::sigil_emit::{go_type_name, package_header};

struct Member<'a> {
    field: String,
    expr: &'a IrTypeExpr,
    embedded: bool,
    keys: BTreeSet<String>,
}

#[derive(Clone, Default)]
struct ObjectView {
    keys: BTreeSet<String>,
    fields: Vec<(String, IrTypeExpr)>,
    additional: Vec<(BTreeSet<String>, IrTypeExpr)>,
}

/// Follow only object/map identities. Non-object and unproductive cycles stay raw;
/// this deliberately does not pretend that arbitrary intersections are objects.
fn object_view(
    expr: &IrTypeExpr,
    ir: &IrSpec,
    visiting: &mut BTreeSet<String>,
    cache: &mut BTreeMap<String, ObjectView>,
) -> Option<ObjectView> {
    match expr {
        IrTypeExpr::Map(values) => Some(ObjectView {
            additional: vec![(BTreeSet::new(), *values.clone())],
            ..Default::default()
        }),
        // Flat structs cannot retain root null, and repeated nullable wrappers
        // can produce illegal anonymous pointer-to-pointer fields. Keep the
        // whole intersection raw, including through aliases/nested compositions.
        IrTypeExpr::Nullable(_) => None,
        IrTypeExpr::Named(name) => {
            if let Some(view) = cache.get(name) {
                return Some(view.clone());
            }
            if !visiting.insert(name.clone()) {
                return None;
            }
            let result = match &ir.schemas.get(name)?.kind {
                IrSchemaKind::Object(object) => {
                    let keys = object.properties.keys().cloned().collect::<BTreeSet<_>>();
                    Some(ObjectView {
                        fields: object
                            .properties
                            .iter()
                            .map(|(key, property)| {
                                (
                                    key.clone(),
                                    if property.nullable {
                                        IrTypeExpr::Nullable(Box::new(property.type_expr.clone()))
                                    } else {
                                        property.type_expr.clone()
                                    },
                                )
                            })
                            .collect(),
                        additional: object
                            .additional_properties
                            .iter()
                            .map(|values| (keys.clone(), values.clone()))
                            .collect(),
                        keys,
                    })
                }
                IrSchemaKind::Alias(inner) => object_view(inner, ir, visiting, cache),
                IrSchemaKind::Intersection(intersection) => {
                    let mut result = ObjectView::default();
                    for member in &intersection.members {
                        let view = object_view(member, ir, visiting, cache)?;
                        result.keys.extend(view.keys);
                        for field in view.fields {
                            if !result.fields.contains(&field) {
                                result.fields.push(field);
                            }
                        }
                        for additional in view.additional {
                            if !result.additional.contains(&additional) {
                                result.additional.push(additional);
                            }
                        }
                    }
                    Some(result)
                }
                _ => None,
            };
            visiting.remove(name);
            if let Some(view) = &result {
                cache.insert(name.clone(), view.clone());
            }
            result
        }
        _ => None,
    }
}

fn representation(expr: &IrTypeExpr, ir: &IrSpec, visiting: &mut BTreeSet<String>) -> IrTypeExpr {
    match expr.representation() {
        IrTypeExpr::Named(name) if visiting.insert(name.clone()) => {
            let result = match ir.schemas.get(name).map(|schema| &schema.kind) {
                Some(IrSchemaKind::Alias(inner)) => representation(inner, ir, visiting),
                Some(IrSchemaKind::Enum(value)) => match value.value_type {
                    IrEnumValueType::String => IrTypeExpr::Primitive(IrPrimitive::String),
                    IrEnumValueType::Integer => IrTypeExpr::Primitive(IrPrimitive::Integer),
                    IrEnumValueType::Number => IrTypeExpr::Primitive(IrPrimitive::Number),
                    IrEnumValueType::Mixed => IrTypeExpr::Any,
                },
                _ => expr.clone(),
            };
            visiting.remove(name);
            result
        }
        IrTypeExpr::Nullable(inner) => {
            IrTypeExpr::Nullable(Box::new(representation(inner, ir, visiting)))
        }
        IrTypeExpr::Array(inner) => {
            IrTypeExpr::Array(Box::new(representation(inner, ir, visiting)))
        }
        IrTypeExpr::Map(inner) => IrTypeExpr::Map(Box::new(representation(inner, ir, visiting))),
        IrTypeExpr::StringEnum(_) | IrTypeExpr::StringLiteral(_) => {
            IrTypeExpr::Primitive(IrPrimitive::String)
        }
        other => other.clone(),
    }
}

/// A conservative representation check, not a full JSON Schema validator.
fn accepts(values: &IrTypeExpr, field: &IrTypeExpr) -> bool {
    if values == field || matches!(values, IrTypeExpr::Any) {
        return true;
    }
    match (values, field) {
        (IrTypeExpr::Nullable(values), IrTypeExpr::Nullable(field)) => accepts(values, field),
        (IrTypeExpr::Nullable(values), field) => accepts(values, field),
        (
            IrTypeExpr::Primitive(IrPrimitive::Number | IrPrimitive::NumberWithFormat(_)),
            IrTypeExpr::Primitive(IrPrimitive::Integer | IrPrimitive::IntegerWithFormat(_)),
        ) => true,
        _ => false,
    }
}

fn embedded_name(expr: &IrTypeExpr) -> Option<String> {
    match expr {
        IrTypeExpr::Named(name) => Some(name.to_pascal_case()),
        _ => None,
    }
}

fn plan<'a>(intersection: &'a IrIntersection, ir: &IrSpec) -> Option<Vec<Member<'a>>> {
    // Repeated references in a composition DAG must not expand metadata as a tree.
    let mut cache = BTreeMap::new();
    let views = intersection
        .members
        .iter()
        .map(|expr| object_view(expr, ir, &mut BTreeSet::new(), &mut cache))
        .collect::<Option<Vec<_>>>()?;
    let fields = views
        .iter()
        .flat_map(|view| &view.fields)
        .map(|(key, expr)| (key, representation(expr, ir, &mut BTreeSet::new())))
        .collect::<Vec<_>>();
    for (index, (key, expr)) in fields.iter().enumerate() {
        if fields[..index].iter().any(|(other_key, other)| {
            key == other_key && !accepts(expr, other) && !accepts(other, expr)
        }) {
            return None;
        }
    }
    for (known, values) in views.iter().flat_map(|view| &view.additional) {
        let values = representation(values, ir, &mut BTreeSet::new());
        if fields
            .iter()
            .any(|(key, expr)| !known.contains(key.as_str()) && !accepts(&values, expr))
        {
            return None;
        }
    }
    let mut used = intersection
        .members
        .iter()
        .filter_map(embedded_name)
        .collect::<BTreeSet<_>>();
    let mut embedded = BTreeSet::new();
    intersection
        .members
        .iter()
        .zip(views)
        .map(|(expr, view)| {
            let keys = view.keys;
            let name = embedded_name(expr);
            let keep_embedded = name
                .as_ref()
                .is_some_and(|name| embedded.insert(name.clone()));
            let base = name.unwrap_or_else(|| "AdditionalProperties".into());
            let field = if keep_embedded {
                base
            } else {
                let mut candidate = base.clone();
                let mut suffix = 2;
                while !used.insert(candidate.clone()) {
                    candidate = format!("{base}{suffix}");
                    suffix += 1;
                }
                candidate
            };
            Some(Member {
                field,
                expr,
                embedded: keep_embedded,
                keys,
            })
        })
        .collect()
}

pub(super) fn emit(
    schema: &IrSchema,
    intersection: &IrIntersection,
    ir: &IrSpec,
    strategy: AllOfStrategy,
) -> Result<String, SigilStitchError> {
    let name = schema.name.to_pascal_case();
    let root = TypeName::primitive(&name);
    let mut file = FileSpec::builder_with("composition.go", Go::new()).header(package_header()?);
    let members = if strategy == AllOfStrategy::Raw {
        None
    } else {
        plan(intersection, ir)
    };
    let Some(members) = members.filter(|members| !members.is_empty()) else {
        let mut alias = TypeSpec::builder(&name, TypeKind::TypeAlias)
            .extends(TypeName::importable("encoding/json", "RawMessage"));
        if let Some(doc) = &schema.description {
            alias = alias.doc(doc);
        }
        return file.add_type(alias.build()?).build()?.render(100);
    };
    let mut declaration = TypeSpec::builder(&name, TypeKind::Struct);
    if let Some(doc) = &schema.description {
        declaration = declaration.doc(doc);
    }
    for member in &members {
        declaration = if member.embedded {
            declaration.add_embedded(go_type_name(member.expr))
        } else {
            declaration.add_field(
                FieldSpec::builder(&member.field, go_type_name(member.expr))
                    .tag("json:\"-\"")
                    .build()?,
            )
        };
    }
    file = file
        .add_type(declaration.build()?)
        .add_import(ImportSpec::named("encoding/json", "json"))
        .add_import(ImportSpec::named("bytes", "bytes"))
        .add_import(ImportSpec::named("fmt", "fmt"));
    let marshal = FunSpec::builder("MarshalJSON")
        .receiver(ParameterSpec::of("o", root.clone()))
        .returns(vec![TypeName::slice(TypeName::primitive("byte")), TypeName::primitive("error")])
        .body(sigil_quote!(Go {
            merged := make(map[string]json.RawMessage)
            $for((index, member) in members.iter().enumerate()) {
                $N(format!("memberData{index}")), err := json.Marshal(o.$N(&member.field))
                if err != nil { return nil, err }
                var $N(format!("memberFields{index}")) map[string]json.RawMessage
                if err := json.Unmarshal($N(format!("memberData{index}")), &$N(format!("memberFields{index}"))); err != nil { return nil, err }
                for key, value := range $N(format!("memberFields{index}")) {
                    if previous, exists := merged[key]; exists && !bytes.Equal(previous, value) {
                        return nil, fmt.Errorf("conflicting allOf property %q", key)
                    }
                    merged[key] = value
                }
            }
            return json.Marshal(merged)
        })?).build()?;
    let unmarshal = FunSpec::builder("UnmarshalJSON")
        .receiver(ParameterSpec::of("o", TypeName::pointer(root)))
        .add_param(ParameterSpec::of(
            "data",
            TypeName::slice(TypeName::primitive("byte")),
        ))
        .returns(vec![TypeName::primitive("error")])
        .body(unmarshal_body(&members)?)
        .build()?;
    file.add_function(marshal)
        .add_function(unmarshal)
        .build()?
        .render(100)
}

fn unmarshal_body(members: &[Member<'_>]) -> Result<CodeBlock, SigilStitchError> {
    let all_keys = members
        .iter()
        .flat_map(|member| &member.keys)
        .collect::<BTreeSet<_>>();
    // Validate the complete object first, retaining native root/syntax errors.
    // Projection then streams the original entries rather than marshaling that
    // map: input order and duplicate keys affect ordinary Go member decoding.
    sigil_quote!(Go {
            var fields map[string]json.RawMessage
            if err := json.Unmarshal(data, &fields); err != nil { return err }
            if fields == nil { return nil }
            $for(index in 0..members.len()) {
                $N(format!("projected{index}")) := bytes.NewBufferString("{")
            }
            decoder := json.NewDecoder(bytes.NewReader(data))
            if _, err := decoder.Token(); err != nil { return err }
            for decoder.More() {
                token, err := decoder.Token()
                if err != nil { return err }
                key := token.(string)
                var value json.RawMessage
                if err := decoder.Decode(&value); err != nil { return err }
                encodedKey, err := json.Marshal(key)
                if err != nil { return err }
                $for((index, member) in members.iter().enumerate()) {
                    switch key {
                    $for(key in all_keys.iter().filter(|key| !member.keys.contains(key.as_str())); separator = "\n") {
                        case $S(key):
                    }
                    default:
                        if $N(format!("projected{index}")).Len() > 1 { $N(format!("projected{index}")).WriteString(",") }
                        $N(format!("projected{index}")).Write(encodedKey)
                        $N(format!("projected{index}")).WriteString(":")
                        $N(format!("projected{index}")).Write(value)
                    }
                }
            }
            $for((index, member) in members.iter().enumerate()) {
                $N(format!("projected{index}")).WriteString("}")
                if err := json.Unmarshal($N(format!("projected{index}")).Bytes(), &o.$N(&member.field)); err != nil { return err }
            }
            return nil
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sigil_stitch::assert_rendered;

    #[test]
    fn ordered_projection_renders_token_assertions_and_buffer_writes() {
        let expr = IrTypeExpr::Named("Base".into());
        let body = unmarshal_body(&[Member {
            field: "Base".into(),
            expr: &expr,
            embedded: true,
            keys: BTreeSet::from(["ID".into()]),
        }])
        .unwrap();
        assert_rendered!(
            Go::new(),
            width = 100,
            body,
            r#"var fields map[string]json.RawMessage
if err := json.Unmarshal(data, &fields); err != nil {
	return err
}
if fields == nil {
	return nil
}
projected0 := bytes.NewBufferString("{")
decoder := json.NewDecoder(bytes.NewReader(data))
if _, err := decoder.Token(); err != nil {
	return err
}
for decoder.More() {
	token, err := decoder.Token()
	if err != nil {
		return err
	}
	key := token.(string)
	var value json.RawMessage
	if err := decoder.Decode(&value); err != nil {
		return err
	}
	encodedKey, err := json.Marshal(key)
	if err != nil {
		return err
	}
	switch key {
		default:
		if projected0.Len() > 1 {
			projected0.WriteString(",")
		}
		projected0.Write(encodedKey)
		projected0.WriteString(":")
		projected0.Write(value)
	}
}
projected0.WriteString("}")
if err := json.Unmarshal(projected0.Bytes(), &o.Base); err != nil {
	return err
}
return nil
"#
        );
    }

    #[test]
    fn go_map_storage_and_json_method_receiver_render_semantically() {
        let declaration = TypeSpec::builder("Composed", TypeKind::Struct)
            .add_embedded(TypeName::primitive("Base"))
            .add_field(
                FieldSpec::builder(
                    "AdditionalProperties",
                    TypeName::map(TypeName::primitive("string"), TypeName::primitive("any")),
                )
                .tag("json:\"-\"")
                .build()
                .unwrap(),
            )
            .build()
            .unwrap()
            .emit(&Go::new())
            .unwrap();
        let method = FunSpec::builder("MarshalJSON")
            .receiver(ParameterSpec::of("o", TypeName::primitive("Composed")))
            .returns(vec![
                TypeName::slice(TypeName::primitive("byte")),
                TypeName::primitive("error"),
            ])
            .body(sigil_quote!(Go { return json.Marshal(o.AdditionalProperties) }).unwrap())
            .build()
            .unwrap()
            .emit(&Go::new(), DeclarationContext::TopLevel)
            .unwrap();
        let block = sigil_quote!(Go { $C_each(declaration) $C(method) }).unwrap();
        assert_rendered!(
            Go::new(),
            width = 100,
            block,
            "type Composed struct {\n\tBase\n\tAdditionalProperties map[string]any `json:\"-\"`\n}\nfunc (o Composed) MarshalJSON() ([]byte, error) {\n\treturn json.Marshal(o.AdditionalProperties)\n}\n"
        );
    }
}
