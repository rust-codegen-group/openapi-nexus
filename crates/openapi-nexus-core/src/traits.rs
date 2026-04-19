//! Core traits for language generators

pub mod code_generator;
pub mod combined_generator;
pub mod file_writer;

pub use code_generator::CodeGenerator;
pub use combined_generator::CombinedGenerator;
pub use file_writer::{FileCategory, FileInfo, FileWriter};
