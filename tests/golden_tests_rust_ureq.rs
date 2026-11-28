//! Golden file tests for Rust ureq generator
//!
//! These tests compare generated Rust code against known-good golden files.
//! To update golden files after intentional changes, run:
//!   UPDATE_GOLDEN=1 cargo test --test golden_tests_rust_ureq

use openapi_nexus::generators::rust::ureq::RustUreqCodeGenerator;

mod support;
type GoldenGenerator = RustUreqCodeGenerator;
include!(concat!(env!("OUT_DIR"), "/golden_cases_rust_ureq.rs"));
