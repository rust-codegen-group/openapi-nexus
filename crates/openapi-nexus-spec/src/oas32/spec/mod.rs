//! Structures used in parsing and navigating OpenAPI 3.2 specifications.
//!
//! High-level structures include [`OpenApiV32Spec`], [`Components`] & [`Schema`].

mod callback;
mod components;
mod contact;
mod discriminator;
mod encoding;
mod error;
mod example;
mod external_doc;
mod flows;
mod header;
mod info;
mod license;
mod link;
mod media_type;
mod media_type_examples;
mod openapi_spec;
mod operation;
mod parameter;
mod path_item;
mod reference;
mod request_body;
mod response;
mod schema;
mod security_requirement;
mod security_scheme;
mod server;
mod spec_extensions;
mod tag;

pub use self::{
    callback::Callback,
    components::Components,
    contact::{Contact, ErrorInvalidEmail},
    discriminator::Discriminator,
    encoding::Encoding,
    error::ErrorSpec,
    example::Example,
    external_doc::ExternalDoc,
    flows::{
        AuthorizationCodeFlow, ClientCredentialsFlow, DeviceAuthorizationFlow, Flows, ImplicitFlow,
        PasswordFlow,
    },
    header::Header,
    info::Info,
    license::License,
    link::Link,
    media_type::MediaType,
    media_type_examples::MediaTypeExamples,
    openapi_spec::OpenApiV32Spec,
    operation::Operation,
    parameter::{Parameter, ParameterIn, ParameterStyle},
    path_item::PathItem,
    reference::{ErrorRef, FromRef, ObjectOrReference, Ref, RefType},
    request_body::RequestBody,
    response::Response,
    schema::{
        BooleanSchema, ErrorSchema, ObjectSchema, Schema, Type as SchemaType,
        TypeSet as SchemaTypeSet,
    },
    security_requirement::SecurityRequirement,
    security_scheme::SecurityScheme,
    server::{Server, ServerVariable},
    tag::Tag,
};
