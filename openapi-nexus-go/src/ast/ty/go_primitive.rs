//! Go primitive types

use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use openapi_nexus_core::traits::ToRcDoc;

/// Go primitive types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GoPrimitive {
    String,
    Int,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Float32,
    Float64,
    Bool,
    Byte,
    Rune,
    Time,  // time.Time
    Error, // error
}

impl ToRcDoc for GoPrimitive {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        let type_name = match self {
            GoPrimitive::String => "string",
            GoPrimitive::Int => "int",
            GoPrimitive::Int8 => "int8",
            GoPrimitive::Int16 => "int16",
            GoPrimitive::Int32 => "int32",
            GoPrimitive::Int64 => "int64",
            GoPrimitive::Uint => "uint",
            GoPrimitive::Uint8 => "uint8",
            GoPrimitive::Uint16 => "uint16",
            GoPrimitive::Uint32 => "uint32",
            GoPrimitive::Uint64 => "uint64",
            GoPrimitive::Float32 => "float32",
            GoPrimitive::Float64 => "float64",
            GoPrimitive::Bool => "bool",
            GoPrimitive::Byte => "byte",
            GoPrimitive::Rune => "rune",
            GoPrimitive::Time => "time.Time",
            GoPrimitive::Error => "error",
        };
        RcDoc::text(type_name.to_string())
    }
}
