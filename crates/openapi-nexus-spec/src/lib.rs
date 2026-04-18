//! OpenAPI specification types for OpenAPI Nexus.
//!
//! This crate provides versioned modules for OpenAPI 3.0, 3.1, and 3.2.
//! OpenAPI 3.0 and 3.1 are fully implemented; 3.2 is a stub for future use.

pub mod oas30;
pub mod oas31;
pub mod oas32;

pub use oas30::OpenApiV30Spec;
pub use oas31::OpenApiV31Spec;

#[cfg(test)]
mod test_utils;

#[cfg(test)]
fn run_fixture_test(rel_path: &str) {
    test_utils::run_fixture_test(rel_path);
}

#[cfg(test)]
#[allow(non_snake_case)]
mod generated_fixture_tests {
    include!(concat!(env!("OUT_DIR"), "/generated_fixture_tests.rs"));
}
