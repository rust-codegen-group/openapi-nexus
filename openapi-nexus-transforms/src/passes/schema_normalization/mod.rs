//! Schema normalization transformation pass
//!
//! Promotes inline schemas to named entries in `components/schemas`,
//! replacing the originals with `$ref` pointers. After this pass,
//! generators only need to handle ref-based schemas.

mod naming;
mod predicate;
pub(crate) mod walker;

use openapi_nexus_ir::OpenApi;

use super::{OpenApiTransformPass, TransformError, TransformPass};

/// Schema normalization transformation pass.
///
/// Promotes inline schemas (objects with properties, composition schemas with
/// inline members, arrays with inline item schemas) to named entries in
/// `components/schemas`, replacing the originals with `$ref` pointers.
pub struct SchemaNormalizationPass {
    pub normalize_arrays: bool,
    pub normalize_objects: bool,
}

impl Default for SchemaNormalizationPass {
    fn default() -> Self {
        Self {
            normalize_arrays: true,
            normalize_objects: true,
        }
    }
}

impl SchemaNormalizationPass {
    pub fn new() -> Self {
        Self::default()
    }
}

impl OpenApiTransformPass for SchemaNormalizationPass {
    fn name(&self) -> &str {
        "schema-normalization"
    }

    fn transform(&self, openapi: &mut OpenApi) -> Result<(), TransformError> {
        tracing::debug!("Normalizing schema structures");

        walker::normalize_spec(
            openapi,
            walker::NormalizationConfig {
                normalize_objects: self.normalize_objects,
                normalize_arrays: self.normalize_arrays,
            },
        )
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["reference-resolution"]
    }
}

impl TransformPass for SchemaNormalizationPass {
    fn transform(&self, openapi: &mut OpenApi) -> Result<(), TransformError> {
        <Self as OpenApiTransformPass>::transform(self, openapi)
    }
}

#[cfg(test)]
mod tests {
    use openapi_nexus_spec::oas31::spec::ObjectOrReference;

    use super::{OpenApiTransformPass, SchemaNormalizationPass};

    #[test]
    fn test_schema_normalization_pass_name() {
        let pass = SchemaNormalizationPass::new();
        assert_eq!(pass.name(), "schema-normalization");
    }

    #[test]
    fn test_schema_normalization_pass_dependencies() {
        let pass = SchemaNormalizationPass::new();
        let deps = pass.dependencies();
        assert_eq!(deps, vec!["reference-resolution"]);
    }

    #[test]
    fn test_inline_property_promotion() {
        let yaml = r#"
openapi: 3.1.0
info:
  title: Test
  version: "1.0"
components:
  schemas:
    Pet:
      type: object
      properties:
        name:
          type: string
        address:
          type: object
          properties:
            street:
              type: string
            city:
              type: string
"#;
        let mut spec = openapi_nexus_parser::parse_content_yaml_v31(yaml).unwrap();
        let pass = SchemaNormalizationPass::new();
        OpenApiTransformPass::transform(&pass, &mut spec).unwrap();

        let schemas = &spec.components.as_ref().unwrap().schemas;
        // PetAddress should exist as a promoted schema
        assert!(
            schemas.contains_key("PetAddress"),
            "Expected PetAddress in schemas, found: {:?}",
            schemas.keys().collect::<Vec<_>>()
        );

        // Pet.properties.address should now be a $ref
        let pet = schemas.get("Pet").unwrap();
        if let ObjectOrReference::Object(obj) = pet {
            let addr = obj.properties.get("address").unwrap();
            match addr {
                ObjectOrReference::Ref { ref_path, .. } => {
                    assert_eq!(ref_path, "#/components/schemas/PetAddress");
                }
                ObjectOrReference::Object(_) => panic!("Expected Ref, got inline Object"),
            }
        } else {
            panic!("Expected Pet to be an Object");
        }

        // PetAddress should have the promoted properties
        let pet_address = schemas.get("PetAddress").unwrap();
        if let ObjectOrReference::Object(obj) = pet_address {
            assert!(obj.properties.contains_key("street"));
            assert!(obj.properties.contains_key("city"));
        } else {
            panic!("Expected PetAddress to be an Object");
        }
    }

    #[test]
    fn test_nested_inline_promotion() {
        let yaml = r#"
openapi: 3.1.0
info:
  title: Test
  version: "1.0"
components:
  schemas:
    Pet:
      type: object
      properties:
        address:
          type: object
          properties:
            street:
              type: string
            zipInfo:
              type: object
              properties:
                code:
                  type: string
                plus4:
                  type: string
"#;
        let mut spec = openapi_nexus_parser::parse_content_yaml_v31(yaml).unwrap();
        let pass = SchemaNormalizationPass::new();
        OpenApiTransformPass::transform(&pass, &mut spec).unwrap();

        let schemas = &spec.components.as_ref().unwrap().schemas;
        // Bottom-up: PetAddressZipInfo promoted first, then PetAddress
        assert!(schemas.contains_key("PetAddressZipInfo"), "keys: {:?}", schemas.keys().collect::<Vec<_>>());
        assert!(schemas.contains_key("PetAddress"), "keys: {:?}", schemas.keys().collect::<Vec<_>>());

        // PetAddress should reference PetAddressZipInfo
        if let ObjectOrReference::Object(addr) = schemas.get("PetAddress").unwrap() {
            match addr.properties.get("zipInfo").unwrap() {
                ObjectOrReference::Ref { ref_path, .. } => {
                    assert_eq!(ref_path, "#/components/schemas/PetAddressZipInfo");
                }
                _ => panic!("Expected zipInfo to be a ref"),
            }
        }
    }

    #[test]
    fn test_one_of_with_inline_members() {
        let yaml = r#"
openapi: 3.1.0
info:
  title: Test
  version: "1.0"
components:
  schemas:
    Animal:
      oneOf:
        - type: object
          properties:
            breed:
              type: string
        - $ref: '#/components/schemas/Cat'
    Cat:
      type: object
      properties:
        color:
          type: string
"#;
        let mut spec = openapi_nexus_parser::parse_content_yaml_v31(yaml).unwrap();
        let pass = SchemaNormalizationPass::new();
        OpenApiTransformPass::transform(&pass, &mut spec).unwrap();

        let schemas = &spec.components.as_ref().unwrap().schemas;
        assert!(schemas.contains_key("AnimalVariant0"), "keys: {:?}", schemas.keys().collect::<Vec<_>>());

        // The oneOf member should now be a ref
        if let ObjectOrReference::Object(animal) = schemas.get("Animal").unwrap() {
            match &animal.one_of[0] {
                ObjectOrReference::Ref { ref_path, .. } => {
                    assert_eq!(ref_path, "#/components/schemas/AnimalVariant0");
                }
                _ => panic!("Expected first oneOf member to be a ref"),
            }
            // Second member should still be the original ref
            match &animal.one_of[1] {
                ObjectOrReference::Ref { ref_path, .. } => {
                    assert_eq!(ref_path, "#/components/schemas/Cat");
                }
                _ => panic!("Expected second oneOf member to remain a ref"),
            }
        }
    }

    #[test]
    fn test_request_body_inline_schema() {
        let yaml = r#"
openapi: 3.1.0
info:
  title: Test
  version: "1.0"
paths:
  /users:
    post:
      operationId: createUser
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                name:
                  type: string
                email:
                  type: string
      responses:
        '200':
          description: OK
"#;
        let mut spec = openapi_nexus_parser::parse_content_yaml_v31(yaml).unwrap();
        let pass = SchemaNormalizationPass::new();
        OpenApiTransformPass::transform(&pass, &mut spec).unwrap();

        let schemas = &spec.components.as_ref().unwrap().schemas;
        assert!(
            schemas.contains_key("CreateUserRequest"),
            "keys: {:?}",
            schemas.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_response_body_inline_schema() {
        let yaml = r#"
openapi: 3.1.0
info:
  title: Test
  version: "1.0"
paths:
  /users/{id}:
    get:
      operationId: getUser
      responses:
        '200':
          description: OK
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:
                    type: string
                  name:
                    type: string
"#;
        let mut spec = openapi_nexus_parser::parse_content_yaml_v31(yaml).unwrap();
        let pass = SchemaNormalizationPass::new();
        OpenApiTransformPass::transform(&pass, &mut spec).unwrap();

        let schemas = &spec.components.as_ref().unwrap().schemas;
        assert!(
            schemas.contains_key("GetUserResponse200"),
            "keys: {:?}",
            schemas.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_array_with_inline_items() {
        let yaml = r#"
openapi: 3.1.0
info:
  title: Test
  version: "1.0"
components:
  schemas:
    TagList:
      type: array
      items:
        type: object
        properties:
          name:
            type: string
          value:
            type: string
"#;
        let mut spec = openapi_nexus_parser::parse_content_yaml_v31(yaml).unwrap();
        let pass = SchemaNormalizationPass::new();
        OpenApiTransformPass::transform(&pass, &mut spec).unwrap();

        let schemas = &spec.components.as_ref().unwrap().schemas;
        assert!(
            schemas.contains_key("TagListItem"),
            "keys: {:?}",
            schemas.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_additional_properties_inline() {
        let yaml = r#"
openapi: 3.1.0
info:
  title: Test
  version: "1.0"
components:
  schemas:
    Config:
      type: object
      additionalProperties:
        type: object
        properties:
          key:
            type: string
          value:
            type: string
"#;
        let mut spec = openapi_nexus_parser::parse_content_yaml_v31(yaml).unwrap();
        let pass = SchemaNormalizationPass::new();
        OpenApiTransformPass::transform(&pass, &mut spec).unwrap();

        let schemas = &spec.components.as_ref().unwrap().schemas;
        assert!(
            schemas.contains_key("ConfigValue"),
            "keys: {:?}",
            schemas.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_collision_handling() {
        let yaml = r#"
openapi: 3.1.0
info:
  title: Test
  version: "1.0"
components:
  schemas:
    PetAddress:
      type: object
      properties:
        existing:
          type: string
    Pet:
      type: object
      properties:
        address:
          type: object
          properties:
            street:
              type: string
"#;
        let mut spec = openapi_nexus_parser::parse_content_yaml_v31(yaml).unwrap();
        let pass = SchemaNormalizationPass::new();
        OpenApiTransformPass::transform(&pass, &mut spec).unwrap();

        let schemas = &spec.components.as_ref().unwrap().schemas;
        // Original PetAddress still exists
        assert!(schemas.contains_key("PetAddress"));
        // Promoted schema should get a suffixed name
        assert!(
            schemas.contains_key("PetAddress2"),
            "keys: {:?}",
            schemas.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_idempotency() {
        let yaml = r#"
openapi: 3.1.0
info:
  title: Test
  version: "1.0"
components:
  schemas:
    Pet:
      type: object
      properties:
        address:
          type: object
          properties:
            street:
              type: string
"#;
        let mut spec = openapi_nexus_parser::parse_content_yaml_v31(yaml).unwrap();
        let pass = SchemaNormalizationPass::new();

        // First run
        OpenApiTransformPass::transform(&pass, &mut spec).unwrap();
        let first_run = spec.clone();

        // Second run
        OpenApiTransformPass::transform(&pass, &mut spec).unwrap();

        // Should be identical
        assert_eq!(
            first_run.components.as_ref().unwrap().schemas.keys().collect::<Vec<_>>(),
            spec.components.as_ref().unwrap().schemas.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_primitives_not_promoted() {
        let yaml = r#"
openapi: 3.1.0
info:
  title: Test
  version: "1.0"
components:
  schemas:
    Pet:
      type: object
      properties:
        name:
          type: string
        age:
          type: integer
        active:
          type: boolean
"#;
        let mut spec = openapi_nexus_parser::parse_content_yaml_v31(yaml).unwrap();
        let pass = SchemaNormalizationPass::new();
        OpenApiTransformPass::transform(&pass, &mut spec).unwrap();

        let schemas = &spec.components.as_ref().unwrap().schemas;
        // Only Pet should exist -- no promoted schemas for primitive properties
        assert_eq!(schemas.len(), 1);
        assert!(schemas.contains_key("Pet"));
    }

    #[test]
    fn test_ref_schemas_untouched() {
        let yaml = r#"
openapi: 3.1.0
info:
  title: Test
  version: "1.0"
components:
  schemas:
    Pet:
      type: object
      properties:
        owner:
          $ref: '#/components/schemas/Owner'
    Owner:
      type: object
      properties:
        name:
          type: string
"#;
        let mut spec = openapi_nexus_parser::parse_content_yaml_v31(yaml).unwrap();
        let pass = SchemaNormalizationPass::new();
        OpenApiTransformPass::transform(&pass, &mut spec).unwrap();

        let schemas = &spec.components.as_ref().unwrap().schemas;
        // Only Pet and Owner -- no new promoted schemas
        assert_eq!(schemas.len(), 2);

        // Pet's owner property should still be a ref
        if let ObjectOrReference::Object(pet) = schemas.get("Pet").unwrap() {
            assert!(matches!(
                pet.properties.get("owner").unwrap(),
                ObjectOrReference::Ref { .. }
            ));
        }
    }
}
