//! Reserve authored schema identities before allocating promoted schema names.

use std::collections::HashSet;

use heck::ToPascalCase;

/// Name allocation at the shared model-word boundary.
///
/// All registered generators normalize model declarations with heck's PascalCase
/// word splitting. Their module and file naming uses the same words, before any
/// backend-owned filename adjustments. Reserve that common identity without
/// making target-specific escaping or auxiliary-artifact naming decisions here.
/// Authored names are never renamed; only synthetic candidates are allocated.
#[derive(Debug, Default)]
pub(super) struct SchemaNames {
    reserved_models: HashSet<String>,
}

impl SchemaNames {
    pub(super) fn from_authored<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            reserved_models: names
                .into_iter()
                .map(ToPascalCase::to_pascal_case)
                .collect(),
        }
    }

    /// Preserve the candidate's original spelling and the existing `base2`,
    /// `base3`, ... suffix policy, avoiding both authored and promoted identities.
    pub(super) fn allocate(&mut self, base: &str) -> String {
        if self.reserve(base) {
            return base.to_string();
        }
        let mut suffix = 2;
        loop {
            let candidate = format!("{base}{suffix}");
            if self.reserve(&candidate) {
                return candidate;
            }
            suffix += 1;
        }
    }

    fn reserve(&mut self, name: &str) -> bool {
        self.reserved_models.insert(name.to_pascal_case())
    }
}

#[cfg(test)]
mod tests {
    use super::SchemaNames;

    #[test]
    fn non_conflicting_candidates_keep_their_original_spelling() {
        let mut names = SchemaNames::from_authored(["AuthoredModel"]);
        assert_eq!(
            names.allocate("OwnerAdditionalProperty"),
            "OwnerAdditionalProperty"
        );
        assert_eq!(names.allocate("nested_value"), "nested_value");
    }

    #[test]
    fn authored_case_and_separator_variants_reserve_the_same_model() {
        for authored in [
            "FooAdditionalProperty",
            "fooAdditionalProperty",
            "foo_additional_property",
            "foo-additional-property",
            "FOO_ADDITIONAL_PROPERTY",
        ] {
            let mut names = SchemaNames::from_authored([authored]);
            assert_eq!(
                names.allocate("FooAdditionalProperty"),
                "FooAdditionalProperty2",
                "{authored}"
            );
            assert_eq!(
                names.allocate("FooAdditionalProperty"),
                "FooAdditionalProperty3",
                "{authored}"
            );
        }
    }

    #[test]
    fn promoted_names_reserve_their_normalized_identity() {
        let mut names = SchemaNames::default();
        assert_eq!(names.allocate("owner_value"), "owner_value");
        assert_eq!(names.allocate("OwnerValue"), "OwnerValue2");
        assert_eq!(names.allocate("owner-value"), "owner-value3");
    }

    #[test]
    fn authored_suffixes_are_reserved_before_any_allocation() {
        let authored = [
            "foo_additional_property",
            "foo-additional-property2",
            "FooAdditionalProperty3",
        ];
        for names_in_order in [authored.to_vec(), authored.into_iter().rev().collect()] {
            let mut names = SchemaNames::from_authored(names_in_order);
            assert_eq!(
                names.allocate("FooAdditionalProperty"),
                "FooAdditionalProperty4"
            );
            assert_eq!(
                names.allocate("foo_additional_property"),
                "foo_additional_property5"
            );
        }
    }

    #[test]
    fn authored_normalized_collisions_do_not_rename_or_reject_authored_models() {
        let mut names = SchemaNames::from_authored(["Foo", "foo", "FOO"]);
        assert_eq!(names.allocate("Foo"), "Foo2");
    }

    #[test]
    fn suffixes_are_appended_without_reinterpreting_existing_digits() {
        let mut names = SchemaNames::from_authored(["Choice2"]);
        assert_eq!(names.allocate("Choice2"), "Choice22");
        assert_eq!(names.allocate("Choice2"), "Choice23");
    }
}
