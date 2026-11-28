//! Golden file tests for Java OkHttp generator
//!
//! These tests compare generated Java code against known-good golden files.
//! To update golden files after intentional changes, run:
//!   UPDATE_GOLDEN=1 cargo test --test golden_tests_java_okhttp

use openapi_nexus::generators::java::okhttp::JavaOkhttpCodeGenerator;

mod support;
type GoldenGenerator = JavaOkhttpCodeGenerator;
include!(concat!(env!("OUT_DIR"), "/golden_cases_java_okhttp.rs"));
