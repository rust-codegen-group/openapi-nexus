//! Deterministic name generation for promoted inline schemas

use std::collections::HashSet;

use heck::ToPascalCase;

/// Generates unique, deterministic names for promoted inline schemas.
///
/// Seeds from existing `components/schemas` names to avoid collisions.
/// All generated names are PascalCase.
pub struct SchemaNameGenerator {
    used_names: HashSet<String>,
}

impl SchemaNameGenerator {
    /// Create a new generator seeded with existing schema names.
    pub fn new(existing_names: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        Self {
            used_names: existing_names
                .into_iter()
                .map(|n| n.as_ref().to_string())
                .collect(),
        }
    }

    /// Generate a unique name from a base, appending numeric suffix on collision.
    /// Registers the name so subsequent calls won't collide.
    pub fn generate_unique(&mut self, base: &str) -> String {
        if self.used_names.insert(base.to_string()) {
            return base.to_string();
        }
        let mut suffix = 2;
        loop {
            let candidate = format!("{base}{suffix}");
            if self.used_names.insert(candidate.clone()) {
                return candidate;
            }
            suffix += 1;
        }
    }

    /// Compute a name without registering it (for use as parent_name during recursion).
    pub fn peek_name(&self, parent: &str, ctx: &FieldContext) -> String {
        match ctx {
            FieldContext::Property(field) => format!("{parent}{}", field.to_pascal_case()),
            FieldContext::Variant(index) => format!("{parent}Variant{index}"),
            FieldContext::ArrayItem => format!("{parent}Item"),
            FieldContext::PrefixItem(index) => format!("{parent}PrefixItem{index}"),
            FieldContext::AdditionalProperties => format!("{parent}Value"),
            FieldContext::RequestBody {
                op_id,
                method,
                path,
            } => {
                let base = op_id
                    .as_deref()
                    .map(|id| id.to_pascal_case())
                    .unwrap_or_else(|| path_to_pascal(method, path));
                format!("{base}Request")
            }
            FieldContext::ResponseBody {
                op_id,
                method,
                path,
                status,
            } => {
                let base = op_id
                    .as_deref()
                    .map(|id| id.to_pascal_case())
                    .unwrap_or_else(|| path_to_pascal(method, path));
                format!("{base}Response{status}")
            }
            FieldContext::ParameterSchema { param_name } => {
                format!("{parent}{}", param_name.to_pascal_case())
            }
        }
    }

    /// Generate a name for a property schema: `{Parent}{Field}` in PascalCase.
    #[allow(dead_code)]
    pub fn for_property(&mut self, parent: &str, field: &str) -> String {
        let base = format!("{parent}{}", field.to_pascal_case());
        self.generate_unique(&base)
    }

    /// Generate a name for a composition variant: `{Parent}Variant{N}`.
    #[allow(dead_code)]
    pub fn for_variant(&mut self, parent: &str, index: usize) -> String {
        let base = format!("{parent}Variant{index}");
        self.generate_unique(&base)
    }

    /// Generate a name for a request body schema.
    #[allow(dead_code)]
    pub fn for_request_body(&mut self, op_id: Option<&str>, method: &str, path: &str) -> String {
        let base_name = op_id
            .map(|id| id.to_pascal_case())
            .unwrap_or_else(|| path_to_pascal(method, path));
        let base = format!("{base_name}Request");
        self.generate_unique(&base)
    }

    /// Generate a name for a response body schema.
    #[allow(dead_code)]
    pub fn for_response_body(
        &mut self,
        op_id: Option<&str>,
        method: &str,
        path: &str,
        status: &str,
    ) -> String {
        let base_name = op_id
            .map(|id| id.to_pascal_case())
            .unwrap_or_else(|| path_to_pascal(method, path));
        let base = format!("{base_name}Response{status}");
        self.generate_unique(&base)
    }

    /// Generate a name for array items: `{Parent}Item`.
    #[allow(dead_code)]
    pub fn for_array_item(&mut self, parent: &str) -> String {
        let base = format!("{parent}Item");
        self.generate_unique(&base)
    }

    /// Generate a name for additionalProperties value: `{Parent}Value`.
    #[allow(dead_code)]
    pub fn for_additional_properties(&mut self, parent: &str) -> String {
        let base = format!("{parent}Value");
        self.generate_unique(&base)
    }
}

/// Describes what kind of child location this schema occupies, for naming purposes.
#[derive(Clone)]
pub enum FieldContext {
    /// A property of a parent object schema.
    Property(String),
    /// An element in allOf/anyOf/oneOf at the given index.
    Variant(usize),
    /// The `items` schema of an array.
    ArrayItem,
    /// A `prefixItems` element at the given index.
    PrefixItem(usize),
    /// The `additionalProperties` schema.
    AdditionalProperties,
    /// A request body schema.
    RequestBody {
        op_id: Option<String>,
        method: String,
        path: String,
    },
    /// A response body schema.
    ResponseBody {
        op_id: Option<String>,
        method: String,
        path: String,
        status: String,
    },
    /// A parameter's schema.
    ParameterSchema { param_name: String },
}

/// Convert an HTTP method + path to a PascalCase name.
/// e.g., "POST", "/api/v1/users/{id}" -> "PostApiV1UsersId"
fn path_to_pascal(method: &str, path: &str) -> String {
    let segments: Vec<&str> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_start_matches('{').trim_end_matches('}'))
        .collect();
    let combined = format!("{method} {}", segments.join(" "));
    combined.to_pascal_case()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_for_property() {
        let mut namer = SchemaNameGenerator::new(Vec::<String>::new());
        assert_eq!(namer.for_property("Pet", "address"), "PetAddress");
    }

    #[test]
    fn test_for_property_collision() {
        let mut namer = SchemaNameGenerator::new(["PetAddress".to_string()]);
        assert_eq!(namer.for_property("Pet", "address"), "PetAddress2");
    }

    #[test]
    fn test_for_property_double_collision() {
        let mut namer = SchemaNameGenerator::new(Vec::<String>::new());
        assert_eq!(namer.for_property("Pet", "address"), "PetAddress");
        assert_eq!(namer.for_property("Pet", "address"), "PetAddress2");
    }

    #[test]
    fn test_for_variant() {
        let mut namer = SchemaNameGenerator::new(Vec::<String>::new());
        assert_eq!(namer.for_variant("Foo", 0), "FooVariant0");
        assert_eq!(namer.for_variant("Foo", 1), "FooVariant1");
    }

    #[test]
    fn test_for_request_body_with_operation_id() {
        let mut namer = SchemaNameGenerator::new(Vec::<String>::new());
        assert_eq!(
            namer.for_request_body(Some("createUser"), "POST", "/users"),
            "CreateUserRequest"
        );
    }

    #[test]
    fn test_for_request_body_without_operation_id() {
        let mut namer = SchemaNameGenerator::new(Vec::<String>::new());
        assert_eq!(
            namer.for_request_body(None, "POST", "/users"),
            "PostUsersRequest"
        );
    }

    #[test]
    fn test_for_request_body_complex_path() {
        let mut namer = SchemaNameGenerator::new(Vec::<String>::new());
        assert_eq!(
            namer.for_request_body(None, "PUT", "/api/v1/users/{id}"),
            "PutApiV1UsersIdRequest"
        );
    }

    #[test]
    fn test_for_response_body() {
        let mut namer = SchemaNameGenerator::new(Vec::<String>::new());
        assert_eq!(
            namer.for_response_body(Some("getUser"), "GET", "/users/{id}", "200"),
            "GetUserResponse200"
        );
    }

    #[test]
    fn test_for_response_body_without_operation_id() {
        let mut namer = SchemaNameGenerator::new(Vec::<String>::new());
        assert_eq!(
            namer.for_response_body(None, "GET", "/users", "200"),
            "GetUsersResponse200"
        );
    }

    #[test]
    fn test_for_array_item() {
        let mut namer = SchemaNameGenerator::new(Vec::<String>::new());
        assert_eq!(namer.for_array_item("Tags"), "TagsItem");
    }

    #[test]
    fn test_for_additional_properties() {
        let mut namer = SchemaNameGenerator::new(Vec::<String>::new());
        assert_eq!(namer.for_additional_properties("Config"), "ConfigValue");
    }

    #[test]
    fn test_peek_name_does_not_register() {
        let mut namer = SchemaNameGenerator::new(Vec::<String>::new());
        let peeked = namer.peek_name("Pet", &FieldContext::Property("address".to_string()));
        assert_eq!(peeked, "PetAddress");
        // Subsequent generate_unique should still get the same name (not suffixed)
        assert_eq!(namer.generate_unique("PetAddress"), "PetAddress");
    }

    #[test]
    fn test_path_to_pascal() {
        assert_eq!(path_to_pascal("POST", "/users"), "PostUsers");
        assert_eq!(
            path_to_pascal("GET", "/api/v1/users/{id}"),
            "GetApiV1UsersId"
        );
        assert_eq!(
            path_to_pascal("DELETE", "/items/{item_id}/tags/{tag_id}"),
            "DeleteItemsItemIdTagsTagId"
        );
    }
}
