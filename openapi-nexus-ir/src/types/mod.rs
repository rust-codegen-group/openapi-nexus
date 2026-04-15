//! IR type definitions for OpenAPI code generation.
//!
//! These types form the version-agnostic intermediate representation that all
//! OpenAPI spec versions (3.0, 3.1, 3.2) lower into. Generators consume these
//! types exclusively — they never see raw spec types.

mod operation;
mod schema;
mod spec;
mod type_expr;

pub use operation::{
    IrHeader, IrOperation, IrParameter, IrRequestBody, IrResponse, IrSecurityRequirement,
    ParameterLocation,
};
pub use schema::{
    IrEnum, IrEnumValue, IrEnumValueType, IrIntersection, IrObject, IrProperty, IrSchema,
    IrSchemaKind, IrTaggedUnion, IrTaggedVariant, IrUnion, TaggingStyle,
};
pub use spec::{
    ApiKeyLocation, IrContact, IrInfo, IrLicense, IrOAuth2Flow, IrOAuth2Flows, IrSecurityScheme,
    IrServer, IrSpec,
};
pub use type_expr::{IrPrimitive, IrTypeExpr, IrValidation};
