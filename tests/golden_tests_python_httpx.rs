//! Golden file tests for Python httpx generator
//!
//! These tests compare generated Python code against known-good golden files.
//! To update golden files after intentional changes, run:
//!   UPDATE_GOLDEN=1 cargo test --test golden_tests_python_httpx

use openapi_nexus::generators::python::httpx::PythonHttpxCodeGenerator;

mod support;
type GoldenGenerator = PythonHttpxCodeGenerator;
include!(concat!(env!("OUT_DIR"), "/golden_cases_python_httpx.rs"));
