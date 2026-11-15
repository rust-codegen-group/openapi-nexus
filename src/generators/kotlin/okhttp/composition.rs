//! Flat Kotlin Gson composition retains source-facing member properties.

use std::collections::{BTreeMap, BTreeSet};

use heck::ToPascalCase;
use sigil_stitch::lang::kotlin::Kotlin;
use sigil_stitch::prelude::*;

use crate::generators::type_application;
use crate::ir::types::{IrIntersection, IrObject, IrSchema, IrSchemaKind, IrSpec, IrTypeExpr};

use super::config::AllOfStrategy;
use super::sigil_emit::{kotlin_type_name_in, package_header};
use super::util::intersection_fields;

#[derive(Clone, Default)]
struct ObjectView {
    keys: BTreeSet<String>,
    fields: Vec<(String, IrTypeExpr)>,
    additional: Vec<(BTreeSet<String>, IrTypeExpr)>,
    sources: BTreeSet<String>,
}

fn object_view(object: &IrObject, name: &str) -> ObjectView {
    let keys = object.properties.keys().cloned().collect::<BTreeSet<_>>();
    ObjectView {
        fields: object
            .properties
            .iter()
            .map(|(key, property)| {
                let ty = if property.nullable {
                    IrTypeExpr::Nullable(Box::new(property.type_expr.clone()))
                } else {
                    property.type_expr.clone()
                };
                (key.clone(), ty)
            })
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

fn resolve_object_view(
    expr: &IrTypeExpr,
    ir: &IrSpec,
    visiting: &mut BTreeSet<String>,
    cache: &mut BTreeMap<String, ObjectView>,
) -> Option<ObjectView> {
    match expr.representation() {
        IrTypeExpr::Map(values) => Some(ObjectView {
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
                // Kotlin aliases are transparent, unlike Java's ordinary wrappers.
                IrSchemaKind::Alias(inner) => resolve_object_view(inner, ir, visiting, cache),
                IrSchemaKind::Intersection(intersection) if !intersection.members.is_empty() => {
                    let mut result = ObjectView::default();
                    for member in &intersection.members {
                        let next = resolve_object_view(member, ir, visiting, cache)?;
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
                _ => None,
            };
            visiting.remove(name);
            if let Some(view) = &result {
                cache.insert(name.clone(), view.clone());
            }
            result
        }
        // Nullable and non-object intersections retain their complete JSON value.
        _ => None,
    }
}

fn plan(intersection: &IrIntersection, ir: &IrSpec) -> Option<Vec<ObjectView>> {
    if intersection.members.is_empty() {
        return None;
    }
    let mut cache = BTreeMap::new();
    let views = intersection
        .members
        .iter()
        .map(|expr| resolve_object_view(expr, ir, &mut BTreeSet::new(), &mut cache))
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
    strategy: AllOfStrategy,
) -> bool {
    strategy == AllOfStrategy::Composed && plan(intersection, ir).is_some()
}

// Preserve the historical source types for inline map members. Opaque leaves use
// the JsonElement delegate at this boundary without changing the Kotlin Any API.
fn model_type(expr: &IrTypeExpr, package: &str) -> TypeName {
    match expr.representation() {
        IrTypeExpr::Any | IrTypeExpr::Union(_) => TypeName::qualified("kotlin", "Any"),
        IrTypeExpr::Array(inner) => type_application(
            TypeName::qualified("kotlin.collections", "List"),
            vec![model_type(inner, package)],
        ),
        IrTypeExpr::Map(inner) => type_application(
            TypeName::qualified("kotlin.collections", "Map"),
            vec![
                TypeName::qualified("kotlin", "String"),
                model_type(inner, package),
            ],
        ),
        IrTypeExpr::Nullable(inner) => optional(model_type(inner, package)),
        _ => kotlin_type_name_in(expr, Some(package)),
    }
}

fn optional(ty: TypeName) -> TypeName {
    if matches!(ty, TypeName::Optional(_)) {
        ty
    } else {
        TypeName::optional(ty)
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

fn quote_context<T>(
    context: impl Into<String>,
    result: Result<T, SigilStitchError>,
) -> Result<T, SigilStitchError> {
    result.map_err(|error| SigilStitchError::Render {
        context: context.into(),
        message: error.to_string(),
    })
}

fn constructor_quote(
    root: &TypeName,
    models: &str,
    views: &[ObjectView],
) -> Result<CodeBlock, SigilStitchError> {
    let members = views.iter().map(|view| {
        quote_context("Kotlin allOf member metadata", sigil_quote!(Kotlin {
            member(arrayOf<$T(TypeName::qualified("kotlin", "String"))>($for(key in &view.keys; separator = ", ") { $S(key) })
                $for(source in &view.sources) { , $T(TypeName::qualified(models, &source.to_pascal_case()))::class.java })
        }))
    }).collect::<Result<Vec<_>, _>>()?;
    quote_context(
        "Kotlin allOf adapter constructor",
        sigil_quote!(Kotlin {
            super($T(root.clone())::class.java$for(member in &members) { , $C(member.clone()) })
        }),
    )
}

pub(super) fn emit(
    schema: &IrSchema,
    intersection: &IrIntersection,
    ir: &IrSpec,
    package: &str,
    strategy: AllOfStrategy,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();
    let models = format!("{package}.models");
    let root = TypeName::qualified(&models, &name);
    let views = if strategy == AllOfStrategy::Raw {
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
                model_type(expr, &models)
            }
        })
        .collect::<Vec<_>>();
    let used = ir
        .schemas
        .values()
        .map(|schema| schema.name.to_pascal_case())
        .collect::<BTreeSet<_>>();
    let base_factory = format!("{name}AllOfAdapterFactory");
    let mut factory = base_factory.clone();
    let mut suffix = 2;
    while used.contains(&factory) {
        factory = format!("{base_factory}{suffix}");
        suffix += 1;
    }
    // AnnotationSpec currently accepts textual class-literal arguments only.
    let mut model = TypeSpec::builder(&name, TypeKind::Struct)
        .visibility(Visibility::Public)
        .annotate(
            AnnotationSpec::importable(TypeName::qualified(
                "com.google.gson.annotations",
                "JsonAdapter",
            ))
            .args([format!("{factory}::class"), "nullSafe = false".into()]),
        );
    if let Some(description) = &schema.description {
        model = model.doc(description);
    }
    for ((field, _), ty) in fields.iter().zip(&types) {
        model = model.add_primary_constructor_param(
            ParameterSpec::builder(field, ty.clone())
                .is_property()
                .build()?,
        );
    }
    let constructor = FunSpec::builder("constructor")
        .is_constructor()
        .delegation(
            constructor_quote(&root, &models, views.as_deref().unwrap_or_default())?
                .__sigil_trim_trailing_newline(),
        )
        .body(quote_context(
            format!("Kotlin {name} allOf constructor body"),
            sigil_quote!(Kotlin {}),
        )?)
        .build()?;
    let mut codecs = ValueCodecs::new(&models);
    let mut readers = Vec::new();
    let mut writers = Vec::new();
    if !raw {
        for (index, (_, expr)) in fields.iter().enumerate() {
            let function = codecs.decode(expr)?;
            readers.push(quote_context(
                format!("Kotlin {name} allOf decode member {index}"),
                sigil_quote!(Kotlin {
                    $N(function)(gson, readers[$L(index.to_string())])
                }),
            )?);
            writers.push(codecs.encode(expr)?);
        }
    }
    let reader_list = type_application(
        TypeName::qualified("kotlin.collections", "List"),
        vec![TypeName::qualified("com.google.gson.stream", "JsonReader")],
    );
    let decode = FunSpec::builder("decode").is_override()
        .add_param(ParameterSpec::of("gson", TypeName::qualified("com.google.gson", "Gson")))
        .add_param(ParameterSpec::of("readers", reader_list))
        .returns(vec![root.clone()])
        .body(quote_context(format!("Kotlin {name} allOf adapter decode"), sigil_quote!(Kotlin {
            $if(raw) { return $T(root.clone())(gson.getAdapter($T(tree_type())::class.java).read(readers[0])) }
            $else { return $T(root.clone())($for(reader in &readers; separator = ", ") { $C(reader.clone()) }) }
        }))?).build()?;
    let encode = FunSpec::builder("encode").is_override()
        .add_param(ParameterSpec::of("gson", TypeName::qualified("com.google.gson", "Gson")))
        .add_param(ParameterSpec::of("value", root.clone()))
        .add_param(ParameterSpec::of("context", TypeName::qualified("com.google.gson.stream", "JsonWriter")))
        .returns(vec![type_application(TypeName::qualified("kotlin.collections", "List"), vec![tree_type()])])
        .body(quote_context(format!("Kotlin {name} allOf adapter encode"), sigil_quote!(Kotlin {
            return listOf($if(raw) { value.value } $else {
                $for((function, (field, _)) in writers.iter().zip(&fields); separator = ", ") { $N(function)(gson, value.$N(field), context) }
            })
        }))?).build()?;
    let parent = type_application(
        TypeName::qualified(&format!("{package}.runtime"), "AllOfAdapterFactory"),
        vec![root],
    );
    let mut adapter = TypeSpec::builder(&factory, TypeKind::Class)
        .visibility(Visibility::Private)
        .extends(parent)
        .add_method(constructor)
        .add_method(decode)
        .add_method(encode);
    for helper in codecs.functions {
        adapter = adapter.add_method(helper);
    }
    FileSpec::builder_with("composition.kt", Kotlin::new())
        .header(package_header(package)?)
        .add_type(model.build()?)
        .add_type(adapter.build()?)
        .build()
}

/// Paired codecs share the model package and helper namespace. The two traversals
/// keep read coercion and write null policy separate while sharing semantic types.
struct ValueCodecs<'a> {
    package: &'a str,
    functions: Vec<FunSpec>,
}

impl<'a> ValueCodecs<'a> {
    fn new(package: &'a str) -> Self {
        Self {
            package,
            functions: Vec::new(),
        }
    }

    fn decode(&mut self, expr: &IrTypeExpr) -> Result<String, SigilStitchError> {
        let original = expr.representation();
        let (expr, nullable) = match original {
            IrTypeExpr::Nullable(inner) => (inner.representation(), true),
            _ => (original, false),
        };
        let body = match expr {
            IrTypeExpr::Any | IrTypeExpr::Union(_) => quote_context(
                "Kotlin allOf opaque decode helper",
                sigil_quote!(Kotlin {
                    return gson.getAdapter($T(tree_type())::class.java).read(reader)
                }),
            )?,
            IrTypeExpr::Map(inner) => {
                let child = self.decode(inner)?;
                let map = type_application(
                    TypeName::qualified("kotlin.collections", "LinkedHashMap"),
                    vec![
                        TypeName::qualified("kotlin", "String"),
                        model_type(inner, self.package),
                    ],
                );
                quote_context(
                    "Kotlin allOf map decode helper",
                    sigil_quote!(Kotlin {
                        val result = $T(map)()
                        reader.beginObject()
                        while (reader.hasNext()) {
                            val key = reader.nextName()
                            val previous = result.put(key, $N(child)(gson, reader))
                            if (previous != null) { throw $T(TypeName::qualified("com.google.gson", "JsonSyntaxException"))("duplicate key: " + key) }
                        }
                        reader.endObject()
                        return result
                    }),
                )?
            }
            IrTypeExpr::Array(inner) => {
                let child = self.decode(inner)?;
                let list = type_application(
                    TypeName::qualified("kotlin.collections", "ArrayList"),
                    vec![model_type(inner, self.package)],
                );
                quote_context(
                    "Kotlin allOf array decode helper",
                    sigil_quote!(Kotlin {
                        val result = $T(list)()
                        reader.beginArray()
                        while (reader.hasNext()) { result.add($N(child)(gson, reader)) }
                        reader.endArray()
                        return result
                    }),
                )?
            }
            _ => quote_context(
                "Kotlin allOf typed decode helper",
                sigil_quote!(Kotlin {
                    return gson.getAdapter(object : $T(token_type(expr, self.package))() {}).read(reader)
                }),
            )?,
        };
        let function = format!("readMember{}", self.functions.len());
        let helper_body = quote_context(
            format!("Kotlin allOf {function} null handling"),
            sigil_quote!(Kotlin {
                $if(nullable) { if (reader.peek() == $T(TypeName::qualified("com.google.gson.stream", "JsonToken")).NULL) { reader.nextNull(); return null } }
                $C(body)
            }),
        )?;
        self.functions.push(
            FunSpec::builder(&function)
                .visibility(Visibility::Private)
                .add_param(ParameterSpec::of(
                    "gson",
                    TypeName::qualified("com.google.gson", "Gson"),
                ))
                .add_param(ParameterSpec::of(
                    "reader",
                    TypeName::qualified("com.google.gson.stream", "JsonReader"),
                ))
                .returns(vec![model_type(original, self.package)])
                .body(helper_body)
                .build()?,
        );
        Ok(function)
    }

    fn encode(&mut self, expr: &IrTypeExpr) -> Result<String, SigilStitchError> {
        let expr = expr.representation();
        if let IrTypeExpr::Nullable(inner) = expr {
            return self.encode(inner);
        }
        let body = match expr {
            IrTypeExpr::Any | IrTypeExpr::Union(_) => quote_context(
                "Kotlin allOf opaque encode helper",
                sigil_quote!(Kotlin {
                    if (value is $T(tree_type())) { return value }
                    return gson.toJsonTree(value)
                }),
            )?,
            IrTypeExpr::Map(inner) => {
                let child = self.encode(inner)?;
                quote_context(
                    "Kotlin allOf map encode helper",
                    sigil_quote!(Kotlin {
                        val result = $T(TypeName::qualified("com.google.gson", "JsonObject"))()
                        for (entry in value.entries) { result.add(entry.key, $N(child)(gson, entry.value, context)) }
                        return result
                    }),
                )?
            }
            IrTypeExpr::Array(inner) => {
                let child = self.encode(inner)?;
                quote_context(
                    "Kotlin allOf array encode helper",
                    sigil_quote!(Kotlin {
                        val result = $T(TypeName::qualified("com.google.gson", "JsonArray"))()
                        for (item in value) { result.add($N(child)(gson, item, context)) }
                        return result
                    }),
                )?
            }
            _ => quote_context(
                "Kotlin allOf typed encode helper",
                sigil_quote!(Kotlin {
                    return tree(gson, value, object : $T(token_type(expr, self.package))() {}, context)
                }),
            )?,
        };
        let function = format!("writeMember{}", self.functions.len());
        let helper_body = quote_context(
            format!("Kotlin allOf {function} null handling"),
            sigil_quote!(Kotlin {
                if (value == null) { return $T(TypeName::qualified("com.google.gson", "JsonNull")).INSTANCE }
                $C(body)
            }),
        )?;
        self.functions.push(
            FunSpec::builder(&function)
                .visibility(Visibility::Private)
                .add_param(ParameterSpec::of(
                    "gson",
                    TypeName::qualified("com.google.gson", "Gson"),
                ))
                .add_param(ParameterSpec::of(
                    "value",
                    optional(model_type(expr, self.package)),
                ))
                .add_param(ParameterSpec::of(
                    "context",
                    TypeName::qualified("com.google.gson.stream", "JsonWriter"),
                ))
                .returns(vec![tree_type()])
                .body(helper_body)
                .build()?,
        );
        Ok(function)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sigil_stitch::assert_rendered;
    use sigil_stitch::lang::kotlin::Kotlin;
    use sigil_stitch::spec::modifiers::DeclarationContext;

    fn view(keys: &[&str], sources: &[&str]) -> ObjectView {
        ObjectView {
            keys: keys.iter().map(|key| (*key).to_string()).collect(),
            sources: sources.iter().map(|source| (*source).to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn constructor_quote_exactly_renders_composed_member_types() {
        let root = TypeName::qualified("com.example.sdk.models", "Pair");
        let block = constructor_quote(
            &root,
            "com.example.sdk.models",
            &[
                view(&["known_field"], &["BaseObject"]),
                view(&["other_field"], &["Peer"]),
            ],
        )
        .unwrap();
        assert_rendered!(
            Kotlin::new(),
            width = 200,
            block,
            "super(com.example.sdk.models.Pair::class.java , member(arrayOf<kotlin.String>(\"known_field\")\n, com.example.sdk.models.BaseObject::class.java), member(arrayOf<kotlin.String>(\"other_field\")\n, com.example.sdk.models.Peer::class.java))\n"
        );
    }

    #[test]
    fn map_codecs_exactly_render_json_object_helpers() {
        let mut codecs = ValueCodecs::new("com.example.sdk.models");
        assert_eq!(
            codecs
                .decode(&IrTypeExpr::Map(Box::new(IrTypeExpr::Any)))
                .unwrap(),
            "readMember1"
        );
        let decode = codecs.functions.pop().unwrap();
        assert_rendered!(
            Kotlin::new(),
            width = 200,
            decode
                .emit(&Kotlin::new(), DeclarationContext::Member)
                .unwrap(),
            "private fun readMember1(gson: com.google.gson.Gson, reader: com.google.gson.stream.JsonReader): kotlin.collections.Map<kotlin.String, kotlin.Any> {\n    val result = kotlin.collections.LinkedHashMap<kotlin.String, kotlin.Any>()\n    reader.beginObject()\n    while (reader.hasNext()) {\n        val key = reader.nextName()\n        val previous = result.put(key, readMember0(gson, reader))\n        if (previous != null) {\n            throw com.google.gson.JsonSyntaxException(\"duplicate key: \" + key)\n        }\n    }\n    reader.endObject()\n    return result\n}\n"
        );

        assert_eq!(
            codecs
                .encode(&IrTypeExpr::Map(Box::new(IrTypeExpr::Any)))
                .unwrap(),
            "writeMember2"
        );
        let encode = codecs.functions.pop().unwrap();
        assert_rendered!(
            Kotlin::new(),
            width = 200,
            encode
                .emit(&Kotlin::new(), DeclarationContext::Member)
                .unwrap(),
            "private fun writeMember2(gson: com.google.gson.Gson, value: kotlin.collections.Map<kotlin.String, kotlin.Any>?, context: com.google.gson.stream.JsonWriter): com.google.gson.JsonElement {\n    if (value == null) {\n        return com.google.gson.JsonNull.INSTANCE\n    }\n    val result = com.google.gson.JsonObject()\n    for (entry in value.entries) {\n        result.add(entry.key, writeMember1(gson, entry.value, context))\n    }\n    return result\n}\n"
        );
    }
}
