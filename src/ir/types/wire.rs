//! Authored JSON shapes used for member selection, not model declarations.
//!
//! The graph keeps local object constraints separate at every `allOf` node.
//! Named edges refer to `IrSpec::wire_schemas`, including aliases and productive
//! recursion. No model is promoted or rewritten just to describe a wire shape.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum IrWireShape {
    Any,
    Never,
    Kind(IrJsonKind),
    Finite(Vec<Value>),
    Object(IrWireObject),
    Array(Box<Self>),
    AllOf(Vec<Self>),
    AnyOf(Vec<Self>),
    OneOf(Vec<Self>),
    Reference(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum IrJsonKind {
    Null,
    Boolean,
    Integer,
    Number,
    String,
    Array,
    Object,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IrWireObject {
    pub properties: BTreeMap<String, IrWireShape>,
    pub required: BTreeSet<String>,
    /// Local to this object: declarations from a sibling are not in scope.
    pub additional_properties: Box<IrWireShape>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IrInferredUnionWire {
    /// Constraints on the containing schema, outside its selected `oneOf`.
    pub common: IrWireShape,
    /// Full, unprojected `oneOf` branches, in authored order.
    pub branches: Vec<IrWireShape>,
}
