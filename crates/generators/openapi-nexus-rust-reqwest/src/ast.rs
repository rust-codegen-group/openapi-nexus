//! Rust AST definitions

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Rust AST node types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RustNode {
    Struct(Struct),
    Enum(Enum),
    TypeAlias(TypeAlias),
    Function(Function),
    Trait(Trait),
    Module(Module),
    Import(Import),
    Use(Use),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Struct {
    pub name: String,
    pub fields: Vec<Field>,
    pub derives: Vec<String>,
    pub generics: Vec<Generic>,
    pub documentation: Option<String>,
    pub visibility: Visibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enum {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub derives: Vec<String>,
    pub generics: Vec<Generic>,
    pub documentation: Option<String>,
    pub visibility: Visibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeAlias {
    pub name: String,
    pub type_expr: TypeExpression,
    pub generics: Vec<Generic>,
    pub documentation: Option<String>,
    pub visibility: Visibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeExpression>,
    pub visibility: Visibility,
    pub is_async: bool,
    pub is_unsafe: bool,
    pub generics: Vec<Generic>,
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trait {
    pub name: String,
    pub methods: Vec<Method>,
    pub generics: Vec<Generic>,
    pub documentation: Option<String>,
    pub visibility: Visibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub type_expr: TypeExpression,
    pub optional: bool,
    pub visibility: Visibility,
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub type_expr: TypeExpression,
    pub reference: bool,
    pub mutable: bool,
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Method {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeExpression>,
    pub visibility: Visibility,
    pub is_async: bool,
    pub is_unsafe: bool,
    pub generics: Vec<Generic>,
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<Field>,
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    pub items: Vec<RustNode>,
    pub visibility: Visibility,
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub path: String,
    pub items: Vec<ImportItem>,
    pub visibility: Visibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Use {
    pub path: String,
    pub items: Vec<UseItem>,
    pub visibility: Visibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportItem {
    pub name: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseItem {
    pub name: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Generic {
    pub name: String,
    pub bounds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TypeExpression {
    Primitive(PrimitiveType),
    Option(Box<TypeExpression>),
    Vec(Box<TypeExpression>),
    HashMap(Box<TypeExpression>, Box<TypeExpression>),
    Reference(String),
    Tuple(Vec<TypeExpression>),
    Array(Box<TypeExpression>, usize),
    Slice(Box<TypeExpression>),
    Function(FunctionSignature),
    Generic(String),
    Union(Vec<TypeExpression>),
    Intersection(Vec<TypeExpression>),
    Object(HashMap<String, TypeExpression>),
    Literal(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PrimitiveType {
    Bool,
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    F32,
    F64,
    Char,
    String,
    Str,
    Unit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSignature {
    pub parameters: Vec<Parameter>,
    pub return_type: Option<Box<TypeExpression>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Private,
    Crate,
    Super,
    In(String),
}
