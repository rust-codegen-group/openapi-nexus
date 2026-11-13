//! Preserve object-local facts when lowering allOf, including tagged payloads.
//!
//! Version adapters expose structural fields only. Normalization and required
//! projection are shared; named references are never modified in place.

use std::collections::{BTreeMap, HashSet};

pub(super) struct Parts<'a, M> {
    members: &'a [M],
    properties: &'a BTreeMap<String, M>,
    required: &'a [String],
}

pub(super) struct PartsMut<'a, M> {
    members: &'a mut Vec<M>,
    properties: &'a mut BTreeMap<String, M>,
    required: &'a mut Vec<String>,
}

pub(super) trait CompositionSchema: Clone + Default {
    type Member: Clone + PartialEq;

    fn parts(&self) -> Parts<'_, Self::Member>;
    fn parts_mut(&mut self) -> PartsMut<'_, Self::Member>;
    fn object(member: &Self::Member) -> Option<&Self>;
    fn object_mut(member: &mut Self::Member) -> Option<&mut Self>;
    fn reference(member: &Self::Member) -> Option<&str>;
    fn into_member(self) -> Self::Member;
    fn has_payload(&self) -> bool;
}

macro_rules! adapt_schema {
    ($version:ident) => {
        impl CompositionSchema for crate::spec::$version::spec::ObjectSchema {
            type Member = crate::spec::$version::spec::ObjectOrReference<Self>;

            fn parts(&self) -> Parts<'_, Self::Member> {
                Parts {
                    members: &self.all_of,
                    properties: &self.properties,
                    required: &self.required,
                }
            }

            fn parts_mut(&mut self) -> PartsMut<'_, Self::Member> {
                PartsMut {
                    members: &mut self.all_of,
                    properties: &mut self.properties,
                    required: &mut self.required,
                }
            }

            fn object(member: &Self::Member) -> Option<&Self> {
                match member {
                    Self::Member::Object(object) => Some(object),
                    _ => None,
                }
            }

            fn object_mut(member: &mut Self::Member) -> Option<&mut Self> {
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

            fn into_member(self) -> Self::Member {
                Self::Member::Object(self)
            }

            fn has_payload(&self) -> bool {
                !self.properties.is_empty()
                    || !self.required.is_empty()
                    || self.additional_properties.is_some()
                    || !self.all_of.is_empty()
                    || !self.one_of.is_empty()
                    || !self.any_of.is_empty()
                    || self.items.is_some()
                    || !self.enum_values.is_empty()
            }
        }
    };
}

adapt_schema!(oas30);
adapt_schema!(oas31);
adapt_schema!(oas32);

/// Ordinary allOf keeps its authored members and appends object-local facts.
pub(super) fn members<S: CompositionSchema>(
    object: &S,
    resolve: impl Fn(&str) -> Option<S::Member>,
) -> Vec<S::Member> {
    normalize(object, None, &resolve)
}

/// Tagged payloads use the same normalization, excluding the union-owned tag.
pub(super) fn tagged_payload<S: CompositionSchema>(
    object: &S,
    tag: &str,
    resolve: impl Fn(&str) -> Option<S::Member>,
) -> S::Member {
    let mut members = normalize(object, Some(tag), &resolve);
    if members.len() == 1 {
        return members.pop().expect("one payload member");
    }
    let mut payload = S::default();
    *payload.parts_mut().members = members;
    payload.into_member()
}

fn strip_tag<S: CompositionSchema>(object: &mut S, tag: Option<&str>) -> bool {
    let Some(tag) = tag else { return false };
    let parts = object.parts_mut();
    let removed = parts.properties.remove(tag).is_some();
    parts.required.retain(|field| field != tag);
    removed
}

fn normalize<S: CompositionSchema>(
    object: &S,
    tag: Option<&str>,
    resolve: &impl Fn(&str) -> Option<S::Member>,
) -> Vec<S::Member> {
    let mut own = object.clone();
    strip_tag(&mut own, tag);
    let original_members = std::mem::take(own.parts_mut().members);
    let mut members = Vec::new();
    for mut member in original_members {
        if let Some(payload) = S::object_mut(&mut member) {
            let removed = strip_tag(payload, tag);
            if removed && !payload.has_payload() {
                continue;
            }
        }
        members.push(member);
    }

    // A required list can strengthen a referenced property without redeclaring
    // it. Project its constraints into this composition's own object, not into
    // the shared component. Already-required references retain their identity.
    for name in own.parts().required.to_vec() {
        if own.parts().properties.contains_key(&name) {
            continue;
        }
        let mut facts = PropertyFacts::default();
        let mut visited = HashSet::new();
        for member in &members {
            collect_property::<S>(member, &name, resolve, &mut visited, &mut facts);
        }
        if facts.required {
            continue;
        }
        let property = match facts.schemas.as_slice() {
            [single] => single.clone(),
            _ => {
                let mut property = S::default();
                *property.parts_mut().members = facts.schemas;
                property.into_member()
            }
        };
        own.parts_mut().properties.insert(name, property);
    }
    let own_fields: HashSet<_> = own.parts().properties.keys().cloned().collect();
    own.parts_mut()
        .required
        .retain(|name| own_fields.contains(name));
    if own.has_payload() {
        members.push(own.into_member());
    }
    members
}

struct PropertyFacts<M> {
    schemas: Vec<M>,
    required: bool,
}

impl<M> Default for PropertyFacts<M> {
    fn default() -> Self {
        Self {
            schemas: Vec::new(),
            required: false,
        }
    }
}

fn collect_property<S: CompositionSchema>(
    member: &S::Member,
    name: &str,
    resolve: &impl Fn(&str) -> Option<S::Member>,
    visited: &mut HashSet<String>,
    facts: &mut PropertyFacts<S::Member>,
) {
    if let Some(reference) = S::reference(member) {
        if visited.insert(reference.to_string())
            && let Some(target) = resolve(reference)
        {
            collect_property::<S>(&target, name, resolve, visited, facts);
        }
    } else if let Some(object) = S::object(member) {
        let parts = object.parts();
        // A required-only member has no property to lower by itself. It is not
        // evidence that another member already retained this required field.
        facts.required |=
            parts.properties.contains_key(name) && parts.required.iter().any(|field| field == name);
        if let Some(property) = parts.properties.get(name)
            && !facts.schemas.contains(property)
        {
            facts.schemas.push(property.clone());
        }
        for member in parts.members {
            collect_property::<S>(member, name, resolve, visited, facts);
        }
    }
}
