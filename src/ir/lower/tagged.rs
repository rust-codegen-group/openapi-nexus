//! Shared facts for inferred tagged-union discriminator constraints.
//!
//! Version adapters provide access to their raw schema shapes. The policy for
//! following referenced enum values and retaining optional tags lives here so
//! OAS 3.0, 3.1 and 3.2 do not drift.

use std::collections::HashSet;

pub(super) trait TaggedSchema: Clone {
    type Member: Clone;

    fn object(member: &Self::Member) -> Option<&Self>;
    fn reference(member: &Self::Member) -> Option<&str>;
    fn required(&self) -> &[String];
    fn property(&self, name: &str) -> Option<&Self::Member>;
    fn all_of(&self) -> &[Self::Member];
    fn enum_values(&self) -> &[serde_json::Value];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Facts {
    pub values: Vec<String>,
    pub optional: bool,
}

macro_rules! adapt_schema {
    ($version:ident) => {
        impl TaggedSchema for crate::spec::$version::spec::ObjectSchema {
            type Member = crate::spec::$version::spec::ObjectOrReference<Self>;

            fn object(member: &Self::Member) -> Option<&Self> {
                match member {
                    Self::Member::Object(object) => Some(object),
                    _ => None,
                }
            }

            fn reference(member: &Self::Member) -> Option<&str> {
                match member {
                    Self::Member::Ref { ref_path, .. } => Some(ref_path.as_str()),
                    _ => None,
                }
            }

            fn required(&self) -> &[String] {
                &self.required
            }

            fn property(&self, name: &str) -> Option<&Self::Member> {
                self.properties.get(name)
            }

            fn all_of(&self) -> &[Self::Member] {
                &self.all_of
            }

            fn enum_values(&self) -> &[serde_json::Value] {
                &self.enum_values
            }
        }
    };
}

adapt_schema!(oas30);
adapt_schema!(oas31);
adapt_schema!(oas32);

/// Resolve the legal string values and requiredness of an inferred tag.
pub(super) fn facts<S: TaggedSchema>(
    member: &S::Member,
    tag_field: &str,
    fallback: &str,
    resolve: impl Fn(&str) -> Option<S::Member>,
) -> Facts {
    let mut values = Vec::new();
    let mut required = false;
    let mut visited = HashSet::new();
    collect::<S>(
        member,
        tag_field,
        &resolve,
        &mut visited,
        &mut values,
        &mut required,
    );
    values.dedup();
    if values.is_empty() {
        values.push(fallback.to_string());
    }
    Facts {
        values,
        optional: !required,
    }
}

fn collect<S: TaggedSchema>(
    member: &S::Member,
    tag_field: &str,
    resolve: &impl Fn(&str) -> Option<S::Member>,
    visited: &mut HashSet<String>,
    values: &mut Vec<String>,
    required: &mut bool,
) {
    if let Some(reference) = S::reference(member) {
        if visited.insert(reference.to_string())
            && let Some(target) = resolve(reference)
        {
            collect::<S>(&target, tag_field, resolve, visited, values, required);
        }
        return;
    }
    let Some(object) = S::object(member) else {
        return;
    };
    *required |= object.required().iter().any(|field| field == tag_field);
    if let Some(property) = object.property(tag_field) {
        collect_enum::<S>(property, resolve, visited, values);
    }
    for member in object.all_of() {
        collect::<S>(member, tag_field, resolve, visited, values, required);
    }
}

fn collect_enum<S: TaggedSchema>(
    member: &S::Member,
    resolve: &impl Fn(&str) -> Option<S::Member>,
    visited: &mut HashSet<String>,
    values: &mut Vec<String>,
) {
    if let Some(reference) = S::reference(member) {
        if visited.insert(reference.to_string())
            && let Some(target) = resolve(reference)
        {
            collect_enum::<S>(&target, resolve, visited, values);
        }
        return;
    }
    let Some(object) = S::object(member) else {
        return;
    };
    values.extend(
        object
            .enum_values()
            .iter()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned)),
    );
}
