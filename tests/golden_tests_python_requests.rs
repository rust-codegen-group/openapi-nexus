//! Golden file tests for Python requests generator
//!
//! These tests compare generated Python code against known-good golden files.
//! To update golden files after intentional changes, run:
//!   UPDATE_GOLDEN=1 cargo test --test golden_tests_python_requests

use openapi_nexus::generators::python::requests::PythonRequestsCodeGenerator;

mod support;
type GoldenGenerator = PythonRequestsCodeGenerator;
include!(concat!(env!("OUT_DIR"), "/golden_cases_python_requests.rs"));
