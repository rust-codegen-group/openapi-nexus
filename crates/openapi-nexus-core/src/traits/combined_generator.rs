//! Combined trait for generators that can both generate code and write files

use super::code_generator::CodeGenerator;
use super::file_writer::FileWriter;

/// Combined trait for generators that can both generate code and write files
pub trait CombinedGenerator: CodeGenerator + FileWriter {}

impl<T: CodeGenerator + FileWriter> CombinedGenerator for T {}
