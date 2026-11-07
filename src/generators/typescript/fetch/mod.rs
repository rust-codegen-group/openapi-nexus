pub mod codegen;
pub mod config;
mod layout;
mod matching;
mod plan;
pub mod project_files;
pub mod sigil_emit;
pub mod sigil_emit_api;
mod tagged;

pub use codegen::TypeScriptFetchCodeGenerator;
pub use config::{TypeScriptFetchConfig, TypeScriptModule};
