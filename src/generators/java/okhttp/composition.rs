//! Flat Gson composition without changing ordinary model or alias codecs.

use std::collections::{BTreeMap, BTreeSet};

use heck::ToPascalCase;
use sigil_stitch::lang::java::Java;
use sigil_stitch::prelude::*;

use crate::generators::type_application;
use crate::ir::types::{
    IrIntersection, IrObject, IrPrimitive, IrSchema, IrSchemaKind, IrSpec, IrTypeExpr,
};

use super::config::AllOfStrategy;
use super::sigil_emit::package_header;
use super::util::{intersection_fields, java_getter_name, java_primitive_boxed};

#[derive(Clone, Default)]
struct View {
    keys: BTreeSet<String>,
    fields: Vec<(String, IrTypeExpr)>,
    additional: Vec<(BTreeSet<String>, IrTypeExpr)>,
    sources: BTreeSet<String>,
}

fn object_view(object: &IrObject, name: &str) -> View {
    let keys = object.properties.keys().cloned().collect::<BTreeSet<_>>();
    View {
        fields: object
            .properties
            .iter()
            .map(|(key, property)| (key.clone(), property.type_expr.clone()))
            .collect(),
        additional: object
            .additional_properties
            .iter()
            .map(|values| (keys.clone(), values.clone()))
            .collect(),
        keys,
        sources: BTreeSet::from([name.into()]),
    }
}

fn view(
    expr: &IrTypeExpr,
    ir: &IrSpec,
    visiting: &mut BTreeSet<String>,
    cache: &mut BTreeMap<String, View>,
) -> Option<View> {
    match expr {
        IrTypeExpr::Map(values) => Some(View {
            additional: vec![(BTreeSet::new(), *values.clone())],
            ..Default::default()
        }),
        IrTypeExpr::Named(name) => {
            if let Some(view) = cache.get(name) {
                return Some(view.clone());
            }
            if !visiting.insert(name.clone()) {
                return None;
            }
            let result = match &ir.schemas.get(name)?.kind {
                IrSchemaKind::Object(object) => Some(object_view(object, name)),
                IrSchemaKind::Intersection(intersection) => {
                    let mut result = View::default();
                    for member in &intersection.members {
                        let next = view(member, ir, visiting, cache)?;
                        result.keys.extend(next.keys);
                        result.sources.extend(next.sources);
                        for field in next.fields {
                            if !result.fields.contains(&field) {
                                result.fields.push(field);
                            }
                        }
                        for additional in next.additional {
                            if !result.additional.contains(&additional) {
                                result.additional.push(additional);
                            }
                        }
                    }
                    Some(result)
                }
                // Ordinary Java aliases still have their own value wrapper.
                // Do not reinterpret that codec, or strip root-null state.
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

fn plan(intersection: &IrIntersection, ir: &IrSpec) -> Option<Vec<View>> {
    if intersection.members.is_empty() {
        return None;
    }
    let mut cache = BTreeMap::new();
    let views = intersection
        .members
        .iter()
        .map(|expr| view(expr, ir, &mut BTreeSet::new(), &mut cache))
        .collect::<Option<Vec<_>>>()?;
    let fields = views
        .iter()
        .flat_map(|view| &view.fields)
        .collect::<Vec<_>>();
    for (index, (key, expr)) in fields.iter().enumerate() {
        if fields[..index]
            .iter()
            .any(|(other_key, other)| key == other_key && expr != other)
        {
            return None;
        }
    }
    let additional = views
        .iter()
        .flat_map(|view| &view.additional)
        .collect::<Vec<_>>();
    for (known, values) in &additional {
        if fields.iter().any(|(key, expr)| {
            !known.contains(key) && !matches!(values, IrTypeExpr::Any) && values != expr
        }) {
            return None;
        }
        if additional.iter().any(|(_, other)| values != other) {
            return None;
        }
    }
    Some(views)
}

pub(super) fn is_composed(
    intersection: &IrIntersection,
    ir: &IrSpec,
    all_of_strategy: AllOfStrategy,
) -> bool {
    all_of_strategy == AllOfStrategy::Composed && plan(intersection, ir).is_some()
}

fn model_type(expr: &IrTypeExpr, package: &str) -> TypeName {
    match expr.representation() {
        IrTypeExpr::Named(name) => TypeName::qualified(package, &name.to_pascal_case()),
        // sigil-stitch 0.7.5 maps array() to Java List and has no native-array
        // constructor. Only the static primitive byte-array syntax stays raw.
        IrTypeExpr::Primitive(IrPrimitive::Binary) => TypeName::raw("byte[]"),
        IrTypeExpr::Primitive(primitive) => {
            TypeName::qualified("java.lang", java_primitive_boxed(primitive))
        }
        IrTypeExpr::Array(inner) => type_application(
            TypeName::qualified("java.util", "List"),
            vec![model_type(inner, package)],
        ),
        IrTypeExpr::Map(inner) => type_application(
            TypeName::qualified("java.util", "Map"),
            vec![
                TypeName::qualified("java.lang", "String"),
                model_type(inner, package),
            ],
        ),
        IrTypeExpr::Nullable(inner) => model_type(inner, package),
        IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_) => {
            TypeName::qualified("java.lang", "String")
        }
        IrTypeExpr::Any | IrTypeExpr::Union(_) => TypeName::qualified("java.lang", "Object"),
        IrTypeExpr::ScalarEnum { .. } => unreachable!("representation unwrapped"),
    }
}

fn tree_type() -> TypeName {
    TypeName::qualified("com.google.gson", "JsonElement")
}
fn token_type(expr: &IrTypeExpr, package: &str) -> TypeName {
    type_application(
        TypeName::qualified("com.google.gson.reflect", "TypeToken"),
        vec![model_type(expr, package)],
    )
}

pub(super) fn emit(
    schema: &IrSchema,
    intersection: &IrIntersection,
    ir: &IrSpec,
    package: &str,
    all_of_strategy: AllOfStrategy,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();
    let package_models = format!("{package}.models");
    let root = TypeName::qualified(&package_models, &name);
    let views = if all_of_strategy == AllOfStrategy::Raw {
        None
    } else {
        plan(intersection, ir)
    };
    let raw = views.is_none();
    let opaque = IrTypeExpr::Any;
    let fields = if raw {
        vec![("value".to_string(), &opaque)]
    } else {
        intersection_fields(intersection)
    };
    let types = fields
        .iter()
        .map(|(_, expr)| {
            if raw {
                tree_type()
            } else {
                model_type(expr, &package_models)
            }
        })
        .collect::<Vec<_>>();
    let mut factory = format!("{name}AllOfAdapterFactory");
    let used = ir
        .schemas
        .values()
        .map(|schema| schema.name.to_pascal_case())
        .collect::<BTreeSet<_>>();
    let mut suffix = 2;
    while used.contains(&factory) {
        factory = format!("{name}AllOfAdapterFactory{suffix}");
        suffix += 1;
    }
    let mut model = TypeSpec::builder(&name, TypeKind::Struct)
        .visibility(Visibility::Public)
        .annotate(
            AnnotationSpec::importable(TypeName::qualified(
                "com.google.gson.annotations",
                "JsonAdapter",
            ))
            .args([
                format!("value = {factory}.class"),
                "nullSafe = false".into(),
            ]),
        );
    if let Some(description) = &schema.description {
        model = model.doc(description);
    }
    let mut constructor = FunSpec::builder(&name).visibility(Visibility::Public);
    for ((field, _), ty) in fields.iter().zip(&types) {
        model = model.add_field(
            FieldSpec::builder(field, ty.clone())
                .visibility(Visibility::Private)
                .build()?,
        );
        constructor = constructor.add_param(ParameterSpec::of(field, ty.clone()));
        model = model.add_method(
            FunSpec::builder(&java_getter_name(field))
                .visibility(Visibility::Public)
                .returns(vec![ty.clone()])
                .body(sigil_quote!(Java { return this.$N(field); })?)
                .build()?,
        );
    }
    constructor = constructor.body(sigil_quote!(Java {
        $for((field, _) in &fields) { this.$N(field) = $N(field); }
    })?);
    model = model.add_method(constructor.build()?);
    let constructor = FunSpec::builder(&factory).visibility(Visibility::Public).body(sigil_quote!(Java {
        super($T(root.clone()).class $for(view in views.iter().flatten()) {
            , member(new $T(TypeName::qualified("java.lang", "String"))[] { $for(key in &view.keys; separator = ", ") { $S(key) } }
                $for(source in &view.sources) { , $T(TypeName::qualified(&package_models, &source.to_pascal_case())).class })
        });
    })?).build()?;
    let mut helpers = Vec::new();
    let mut readers = Vec::new();
    let mut writers = Vec::new();
    if !raw {
        for (_, expr) in &fields {
            readers.push(decode_value(expr, &package_models, &mut helpers)?);
            writers.push(encode_value(expr, &package_models, &mut helpers)?);
        }
    }
    let decode = FunSpec::builder("decode").visibility(Visibility::Protected).annotate(AnnotationSpec::new("Override"))
        .add_param(ParameterSpec::of("gson", TypeName::qualified("com.google.gson", "Gson")))
        .add_param(ParameterSpec::of("readers", type_application(TypeName::qualified("java.util", "List"), vec![TypeName::qualified("com.google.gson.stream", "JsonReader")])))
        .returns(vec![root.clone()]).throws(vec![TypeName::qualified("java.io", "IOException")])
        .body(sigil_quote!(Java {
            $if(raw) { return new $T(root.clone())(gson.getAdapter($T(tree_type()).class).read(readers.get(0))); }
            $else { return new $T(root.clone())($for((index, function) in readers.iter().enumerate(); separator = ", ") { $N(function)(gson, readers.get($L(index.to_string()))) }); }
        })?).build()?;
    let encode = FunSpec::builder("encode").visibility(Visibility::Protected).annotate(AnnotationSpec::new("Override"))
        .add_param(ParameterSpec::of("gson", TypeName::qualified("com.google.gson", "Gson")))
        .add_param(ParameterSpec::of("value", root.clone()))
        .add_param(ParameterSpec::of("context", TypeName::qualified("com.google.gson.stream", "JsonWriter")))
        .returns(vec![type_application(TypeName::qualified("java.util", "List"), vec![tree_type()])]).throws(vec![TypeName::qualified("java.io", "IOException")])
        .body(sigil_quote!(Java {
            return $T(TypeName::qualified("java.util", "Arrays")).asList(
                $if(raw) { value.getValue() }
                $else { $for((function, (field, _)) in writers.iter().zip(&fields); separator = ", ") { $N(function)(gson, value.$N(java_getter_name(field))(), context) } }
            );
        })?).build()?;
    let mut adapter = TypeSpec::builder(&factory, TypeKind::Class)
        .extends(type_application(
            TypeName::qualified(&format!("{package}.runtime"), "AllOfAdapterFactory"),
            vec![root],
        ))
        .add_method(constructor)
        .add_method(decode)
        .add_method(encode);
    for helper in helpers {
        adapter = adapter.add_method(helper);
    }
    FileSpec::builder_with("composition.java", Java::new())
        .header(package_header(package)?)
        .add_type(model.build()?)
        .add_type(adapter.build()?)
        .build()
}

fn decode_value(
    expr: &IrTypeExpr,
    package: &str,
    helpers: &mut Vec<FunSpec>,
) -> Result<String, SigilStitchError> {
    let expr = expr.representation();
    if let IrTypeExpr::Nullable(inner) = expr {
        return decode_value(inner, package, helpers);
    }
    let container = matches!(
        expr,
        IrTypeExpr::Map(_) | IrTypeExpr::Array(_) | IrTypeExpr::Any | IrTypeExpr::Union(_)
    );
    let body = match expr {
        IrTypeExpr::Any | IrTypeExpr::Union(_) => {
            sigil_quote!(Java { return gson.getAdapter($T(tree_type()).class).read(reader); })?
        }
        IrTypeExpr::Map(inner) => {
            let child = decode_value(inner, package, helpers)?;
            let map = type_application(
                TypeName::qualified("java.util", "LinkedHashMap"),
                vec![
                    TypeName::qualified("java.lang", "String"),
                    model_type(inner, package),
                ],
            );
            sigil_quote!(Java {
                var result = new $T(map)();
                reader.beginObject();
                while (reader.hasNext()) {
                    var key = reader.nextName();
                    var previous = result.put(key, $N(child)(gson, reader));
                    if (previous != null) { throw new $T(TypeName::qualified("com.google.gson", "JsonSyntaxException"))("duplicate key: " + key); }
                }
                reader.endObject();
                return result;
            })?
        }
        IrTypeExpr::Array(inner) => {
            let child = decode_value(inner, package, helpers)?;
            let list = type_application(
                TypeName::qualified("java.util", "ArrayList"),
                vec![model_type(inner, package)],
            );
            sigil_quote!(Java {
                var result = new $T(list)();
                reader.beginArray();
                while (reader.hasNext()) { result.add($N(child)(gson, reader)); }
                reader.endArray();
                return result;
            })?
        }
        _ => {
            sigil_quote!(Java { return gson.getAdapter(new $T(token_type(expr, package))() {}).read(reader); })?
        }
    };
    let function = format!("readMember{}", helpers.len());
    helpers.push(FunSpec::builder(&function).visibility(Visibility::Private)
        .add_param(ParameterSpec::of("gson", TypeName::qualified("com.google.gson", "Gson")))
        .add_param(ParameterSpec::of("reader", TypeName::qualified("com.google.gson.stream", "JsonReader")))
        .returns(vec![model_type(expr, package)]).throws(vec![TypeName::qualified("java.io", "IOException")])
        .body(sigil_quote!(Java {
            $if(container) { if (reader.peek() == $T(TypeName::qualified("com.google.gson.stream", "JsonToken")).NULL) { reader.nextNull(); return null; } }
            $C(body)
        })?).build()?);
    Ok(function)
}

fn encode_value(
    expr: &IrTypeExpr,
    package: &str,
    helpers: &mut Vec<FunSpec>,
) -> Result<String, SigilStitchError> {
    let expr = expr.representation();
    if let IrTypeExpr::Nullable(inner) = expr {
        return encode_value(inner, package, helpers);
    }
    let container = matches!(
        expr,
        IrTypeExpr::Map(_) | IrTypeExpr::Array(_) | IrTypeExpr::Any | IrTypeExpr::Union(_)
    );
    let body = match expr {
        IrTypeExpr::Any | IrTypeExpr::Union(_) => sigil_quote!(Java {
            if (value instanceof $T(tree_type())) { return ($T(tree_type())) value; }
            return gson.toJsonTree(value);
        })?,
        IrTypeExpr::Map(inner) => {
            let child = encode_value(inner, package, helpers)?;
            sigil_quote!(Java {
                var result = new $T(TypeName::qualified("com.google.gson", "JsonObject"))();
                for (var entry : value.entrySet()) { result.add(entry.getKey(), $N(child)(gson, entry.getValue(), context)); }
                return result;
            })?
        }
        IrTypeExpr::Array(inner) => {
            let child = encode_value(inner, package, helpers)?;
            sigil_quote!(Java {
                var result = new $T(TypeName::qualified("com.google.gson", "JsonArray"))();
                for (var item : value) { result.add($N(child)(gson, item, context)); }
                return result;
            })?
        }
        _ => {
            sigil_quote!(Java { return tree(gson, value, new $T(token_type(expr, package))() {}, context); })?
        }
    };
    let function = format!("writeMember{}", helpers.len());
    helpers.push(FunSpec::builder(&function).visibility(Visibility::Private)
        .add_param(ParameterSpec::of("gson", TypeName::qualified("com.google.gson", "Gson")))
        .add_param(ParameterSpec::of("value", model_type(expr, package)))
        .add_param(ParameterSpec::of("context", TypeName::qualified("com.google.gson.stream", "JsonWriter")))
        .returns(vec![tree_type()]).throws(vec![TypeName::qualified("java.io", "IOException")])
        .body(sigil_quote!(Java {
            $if(container) { if (value == null) { return $T(TypeName::qualified("com.google.gson", "JsonNull")).INSTANCE; } }
            $C(body)
        })?).build()?);
    Ok(function)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sigil_stitch::assert_rendered;

    #[test]
    fn native_binary_type_and_semantic_codec_list_render_exactly() {
        let field = FieldSpec::builder(
            "wireBytes",
            model_type(
                &IrTypeExpr::Primitive(IrPrimitive::Binary),
                "com.example.sdk.models",
            ),
        )
        .build()
        .unwrap()
        .emit(&Java::new(), DeclarationContext::Member)
        .unwrap();
        let method = FunSpec::builder("encode")
            .add_param(ParameterSpec::of("value", tree_type()))
            .returns(vec![type_application(TypeName::qualified("java.util", "List"), vec![tree_type()])])
            .body(sigil_quote!(Java { return $T(TypeName::qualified("java.util", "Arrays")).asList(value); }).unwrap())
            .build().unwrap().emit(&Java::new(), DeclarationContext::Member).unwrap();
        let block = sigil_quote!(Java {
            $C(field)
            $C(method)
        })
        .unwrap();
        assert_rendered!(
            Java::new(),
            width = 300,
            block,
            "byte[] wireBytes;\njava.util.List<com.google.gson.JsonElement> encode(com.google.gson.JsonElement value) {\n    return java.util.Arrays.asList(value);\n}\n"
        );
    }
}
