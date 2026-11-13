//! Layout of the TypeScript declarations emitted by this generator.
//!
//! Keep structural type references until sigil-stitch resolves imports. Layout
//! decisions use declaration parts, never reparsed generated source.

use sigil_stitch::code_block::{Arg, CodeBlock};
use sigil_stitch::error::SigilStitchError;
use sigil_stitch::import::ImportGroup;
use sigil_stitch::lang::capability::{FunctionContext, FunctionForm, LanguageCapabilities};
use sigil_stitch::lang::{CodeLang, RendererLang};
use sigil_stitch::prelude::sigil_quote;
use sigil_stitch::spec::modifiers::{DeclarationContext, TypeKind, Visibility};
use sigil_stitch::type_name::TypeName;

const WIDTH: usize = 100;

#[derive(Debug, Clone)]
pub struct TypeScript(sigil_stitch::lang::typescript::TypeScript);

impl TypeScript {
    fn width(&self, block: &CodeBlock) -> Result<usize, SigilStitchError> {
        Ok(block
            .render_standalone(&self.0, usize::MAX)?
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .count())
    }
}

impl RendererLang for TypeScript {
    fn lower_type_name(
        &self,
        type_name: &sigil_stitch::type_name::TypeName,
    ) -> Result<sigil_stitch::code_block::CodeBlock, sigil_stitch::error::SigilStitchError> {
        let mut block = self.0.lower_type_name(type_name)?;
        fn flatten(nodes: &mut [sigil_stitch::code_node::CodeNode]) {
            use sigil_stitch::code_node::CodeNode;
            for node in nodes {
                match node {
                    CodeNode::SoftBreak => *node = CodeNode::Literal(" ".into()),
                    CodeNode::Nested(block) => flatten(block.nodes_mut()),
                    CodeNode::Sequence(children) => flatten(children),
                    _ => {}
                }
            }
        }
        flatten(block.nodes_mut());
        Ok(block)
    }
    fn file_extension(&self) -> &str {
        self.0.file_extension()
    }
    fn reserved_words(&self) -> &[&str] {
        self.0.reserved_words()
    }
    fn render_string_literal(&self, s: &str) -> String {
        self.0.render_string_literal(s)
    }
    fn render_verbatim_string(&self, s: &str) -> String {
        self.0.render_verbatim_string(s)
    }
    fn line_comment_prefix(&self) -> &str {
        self.0.line_comment_prefix()
    }
    fn indent_unit(&self) -> &str {
        self.0.indent_unit()
    }
    fn render_statement_end(&self) -> Result<&str, sigil_stitch::error::SigilStitchError> {
        self.0.render_statement_end()
    }
    fn render_block_open(
        &self,
        _intent: sigil_stitch::code_node::BlockIntent,
        _condition: &str,
    ) -> Result<&str, sigil_stitch::error::SigilStitchError> {
        self.0.render_block_open(_intent, _condition)
    }
    fn render_block_close(
        &self,
        _intent: sigil_stitch::code_node::BlockIntent,
        _condition: &str,
    ) -> Result<&str, sigil_stitch::error::SigilStitchError> {
        self.0.render_block_close(_intent, _condition)
    }
    fn render_branch_transition(
        &self,
        _intent: sigil_stitch::code_node::BlockIntent,
        _condition: &str,
    ) -> Result<String, sigil_stitch::error::SigilStitchError> {
        self.0.render_branch_transition(_intent, _condition)
    }
}

impl CodeLang for TypeScript {
    fn validate_function(
        &self,
        function: sigil_stitch::lang::FunctionIntent<'_>,
    ) -> Result<(), SigilStitchError> {
        self.0.validate_function(function)
    }
    fn validate_resolved_imports(
        &self,
        imports: &sigil_stitch::import::ImportGroup,
    ) -> Result<(), sigil_stitch::error::SigilStitchError> {
        self.0.validate_resolved_imports(imports)
    }
    fn capabilities(&self) -> LanguageCapabilities<'_> {
        self.0.capabilities()
    }
    fn validate_type(
        &self,
        type_: sigil_stitch::lang::TypeIntent<'_>,
    ) -> Result<(), SigilStitchError> {
        self.0.validate_type(type_)
    }
    fn validate_fields(
        &self,
        fields: sigil_stitch::lang::FieldSequenceIntent<'_>,
    ) -> Result<(), SigilStitchError> {
        self.0.validate_fields(fields)
    }
    fn collect_field_validation_errors(
        &self,
        fields: sigil_stitch::lang::FieldSequenceIntent<'_>,
        errors: &mut Vec<SigilStitchError>,
    ) {
        self.0.collect_field_validation_errors(fields, errors)
    }
    fn validate_property(
        &self,
        property: sigil_stitch::lang::PropertyIntent<'_>,
    ) -> Result<(), SigilStitchError> {
        self.0.validate_property(property)
    }
    fn collect_property_validation_errors(
        &self,
        property: sigil_stitch::lang::PropertyIntent<'_>,
        errors: &mut Vec<SigilStitchError>,
    ) {
        self.0.collect_property_validation_errors(property, errors)
    }
    fn lower_property(
        &self,
        property: sigil_stitch::lang::ValidatedProperty<'_>,
    ) -> Result<Vec<CodeBlock>, SigilStitchError> {
        self.0.lower_property(property)
    }
    fn validate_type_members(
        &self,
        members: sigil_stitch::lang::TypeMembersIntent<'_>,
    ) -> Result<(), SigilStitchError> {
        self.0.validate_type_members(members)
    }
    fn collect_type_members_validation_errors(
        &self,
        members: sigil_stitch::lang::TypeMembersIntent<'_>,
        errors: &mut Vec<SigilStitchError>,
    ) {
        self.0
            .collect_type_members_validation_errors(members, errors)
    }
    fn lower_variants(
        &self,
        variants: sigil_stitch::lang::ValidatedVariants<'_>,
    ) -> Result<CodeBlock, SigilStitchError> {
        self.0.lower_variants(variants)
    }
    fn validate_function_type_constraints(
        &self,
        function_name: &str,
        type_params: &[sigil_stitch::spec::where_spec::GenericParamView<'_>],
        constraints: &[sigil_stitch::spec::where_spec::WhereConstraint],
    ) -> Result<(), SigilStitchError> {
        self.0
            .validate_function_type_constraints(function_name, type_params, constraints)
    }
    fn constructor_name_matches(&self, name: &str, _declaring_type: Option<&str>) -> bool {
        self.0.constructor_name_matches(name, _declaring_type)
    }
    fn static_constructor_name_matches(&self, _name: &str, _declaring_type: Option<&str>) -> bool {
        self.0
            .static_constructor_name_matches(_name, _declaring_type)
    }
    fn function_visibility_is_valid(
        &self,
        context: FunctionContext,
        _form: FunctionForm,
        _is_static: bool,
        visibility: Visibility,
    ) -> bool {
        self.0
            .function_visibility_is_valid(context, _form, _is_static, visibility)
    }
    fn function_parameters_require_trailing_defaults(
        &self,
        _context: FunctionContext,
        _form: FunctionForm,
    ) -> bool {
        self.0
            .function_parameters_require_trailing_defaults(_context, _form)
    }
    fn constructor_name_is_valid(&self, name: &str, _declaring_type: Option<&str>) -> bool {
        self.0.constructor_name_is_valid(name, _declaring_type)
    }
    fn escape_field_name(&self, name: &str) -> String {
        self.0.escape_field_name(name)
    }
    fn render_doc_comment(&self, lines: &[&str]) -> String {
        self.0.render_doc_comment(lines)
    }
    fn render_visibility(&self, vis: Visibility, ctx: DeclarationContext) -> &str {
        self.0.render_visibility(vis, ctx)
    }

    fn render_imports(&self, imports: &ImportGroup) -> String {
        if imports
            .entries()
            .iter()
            .any(|entry| entry.is_side_effect || entry.is_wildcard)
        {
            return self.0.render_imports(imports);
        }
        let mut modules = std::collections::BTreeMap::<&str, (Vec<String>, Vec<String>)>::new();
        for entry in imports.entries() {
            let names = modules.entry(&entry.module).or_default();
            let name = entry.alias.as_ref().map_or_else(
                || entry.name.clone(),
                |alias| format!("{} as {alias}", entry.name),
            );
            if entry.is_type_only {
                names.0.push(name);
            } else {
                names.1.push(name);
            }
        }
        let mut lines = Vec::new();
        for (module, (mut types, mut values)) in modules {
            types.sort();
            values.sort();
            for (prefix, names) in [("import type", types), ("import", values)] {
                if !names.is_empty() {
                    lines.push(named_list(prefix, &names, module));
                }
            }
        }
        lines.join("\n")
    }

    fn lower_type(
        &self,
        ty: sigil_stitch::lang::ValidatedType<'_>,
    ) -> Result<Vec<CodeBlock>, SigilStitchError> {
        // These constructs are not emitted by this generator. Preserve the base
        // language's complete lowering if a caller supplies one in the future.
        if ty.generic_params().next().is_some()
            || !ty.properties().is_empty()
            || ty.variants().is_some()
            || !ty.primary_constructor_parameters().is_empty()
            || ty.modifiers().is_abstract
        {
            return self.0.lower_type(ty);
        }
        let mut cb = CodeBlock::builder();
        if !ty.doc().is_empty() {
            cb.add(
                "%L\n",
                self.render_doc_comment(&ty.doc().iter().map(String::as_str).collect::<Vec<_>>()),
            );
        }
        for annotation in ty.annotation_specs() {
            cb.add_code(annotation.emit_with_syntax("@", "")?);
            cb.add_line();
        }
        for annotation in ty.annotations() {
            cb.add_code(annotation.clone());
            cb.add_line();
        }
        let export = if ty.modifiers().visibility == Visibility::Public {
            "export "
        } else {
            ""
        };
        if ty.kind() == TypeKind::TypeAlias {
            let prefix = format!("{export}type {} =", ty.name());
            let target = ty.target_type().expect("validated alias target");
            cb.add("%L", prefix.as_str());
            cb.add_code(self.type_expression(target, prefix.chars().count() + 1, 0)?);
            cb.add(";\n", ());
            return Ok(vec![cb.build()?]);
        }
        let keyword = match ty.kind() {
            TypeKind::Interface | TypeKind::Trait => "interface",
            TypeKind::Enum => "enum",
            _ => "class",
        };
        cb.add(
            "%L",
            format!(
                "{export}{keyword}{} {}",
                if ty.modifiers().is_abstract {
                    " abstract"
                } else {
                    ""
                },
                ty.name()
            ),
        );
        for (connector, bases) in [
            (" extends ", ty.nominal_super_types()),
            (" implements ", ty.implemented_types()),
        ] {
            if !bases.is_empty() {
                cb.add(connector, ());
                for (i, base) in bases.iter().enumerate() {
                    if i > 0 {
                        cb.add(", ", ());
                    }
                    cb.add_code(self.type_part(
                        base,
                        export.len() + keyword.len() + ty.name().len() + connector.len() + 1,
                        0,
                    )?);
                }
            }
        }
        if ty.fields().is_none_or(|fields| fields.fields().is_empty())
            && ty.methods().is_empty()
            && ty.extra_members().is_empty()
        {
            cb.add(" {}\n", ());
            return Ok(vec![cb.build()?]);
        }
        cb.add(" {\n%>", ());
        if let Some(fields) = ty.fields() {
            cb.add_code(self.lower_fields(fields.clone())?);
        }
        if !ty.methods().is_empty() && ty.fields().is_some_and(|f| !f.fields().is_empty()) {
            cb.add_line();
        }
        for (i, method) in ty.methods().iter().enumerate() {
            if i > 0 {
                cb.add_line();
            }
            cb.add_code(self.lower_function(*method)?);
        }
        for member in ty.extra_members() {
            cb.add_code(member.clone());
        }
        cb.add("%<}\n", ());
        Ok(vec![cb.build()?])
    }

    fn lower_fields(
        &self,
        fields: sigil_stitch::lang::ValidatedFields<'_>,
    ) -> Result<CodeBlock, SigilStitchError> {
        if fields.fields().iter().any(|field| {
            !field.annotations().is_empty()
                || !field.annotation_specs().is_empty()
                || !matches!(
                    field.modifiers().visibility,
                    Visibility::Public | Visibility::Inherited
                )
        }) {
            return self.0.lower_fields(fields);
        }
        let mut cb = CodeBlock::builder();
        for field in fields.fields() {
            if !field.doc().is_empty() {
                cb.add(
                    "%L\n",
                    self.render_doc_comment(
                        &field.doc().iter().map(String::as_str).collect::<Vec<_>>(),
                    ),
                );
            }
            let prefix = format!(
                "{}{}{}{}:",
                if field.modifiers().is_static {
                    "static "
                } else {
                    ""
                },
                if field.modifiers().is_readonly {
                    "readonly "
                } else {
                    ""
                },
                field.name(),
                if field.is_optional() { "?" } else { "" }
            );
            cb.add("%L", prefix.as_str());
            cb.add_code(self.type_expression(
                field.field_type(),
                prefix.chars().count() + self.indent_unit().len() + 1,
                1,
            )?);
            if let Some(initializer) = field.initializer() {
                cb.add(" = %L", initializer.clone());
            }
            cb.add(";\n", ());
        }
        cb.build()
    }

    fn lower_function(
        &self,
        function: sigil_stitch::lang::ValidatedFunction<'_>,
    ) -> Result<CodeBlock, SigilStitchError> {
        if function.generic_params().next().is_some()
            || !function.annotations().is_empty()
            || !function.annotation_specs().is_empty()
            || function.delegation().is_some()
            || function.modifiers().is_abstract
            || function.modifiers().is_override
            || function.parameters().iter().any(|parameter| {
                parameter.is_property()
                    || parameter.is_mutable_property()
                    || parameter.is_variadic()
            })
        {
            return self.0.lower_function(function);
        }
        let mut cb = CodeBlock::builder();
        if !function.doc().is_empty() {
            cb.add(
                "%L\n",
                self.render_doc_comment(
                    &function
                        .doc()
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                ),
            );
        }
        let top = function.declaration_context() == DeclarationContext::TopLevel;
        let prefix = format!(
            "{}{}{}{}{}",
            self.render_visibility(
                function.modifiers().visibility,
                function.declaration_context()
            ),
            if function.modifiers().is_static {
                "static "
            } else {
                ""
            },
            if function.modifiers().is_async {
                "async "
            } else {
                ""
            },
            if top && function.form() != FunctionForm::Constructor {
                "function "
            } else {
                ""
            },
            function.name()
        );
        let mut parameters = Vec::new();
        for parameter in function.parameters() {
            let mut p = CodeBlock::builder();
            if parameter.is_variadic() {
                p.add("...", ());
            }
            p.add("%L", parameter.name());
            if !parameter.param_type().is_empty() {
                p.add(": %T", parameter.param_type().clone());
            }
            if let Some(default) = parameter.default_value() {
                p.add(" = %L", inline(default.clone()));
            }
            parameters.push(p.build()?);
        }
        let mut suffix = CodeBlock::builder();
        suffix.add(")", ());
        for text in function.suffixes() {
            suffix.add(" %L", text.as_str());
        }
        if let Some(returns) = function.returns() {
            if let Some(ty) = returns.first() {
                suffix.add(": %T", ty.clone());
            } else {
                suffix.add(": void", ());
            }
        }
        suffix.add(if function.body().is_some() { " {" } else { ";" }, ());
        let suffix = suffix.build()?;
        let width = (if top { 0 } else { self.indent_unit().len() })
            + prefix.chars().count()
            + 1
            + parameters
                .iter()
                .map(|p| self.width(p))
                .collect::<Result<Vec<_>, _>>()?
                .iter()
                .sum::<usize>()
            + parameters.len().saturating_sub(1) * 2
            + self.width(&suffix)?;
        let multiline = !parameters.is_empty() && width > WIDTH;
        cb.add("%L(", prefix);
        if multiline {
            cb.add("\n%>", ());
        }
        for (i, parameter) in parameters.into_iter().enumerate() {
            if i > 0 && !multiline {
                cb.add(", ", ());
            }
            cb.add_code(parameter);
            if multiline {
                cb.add(",\n", ());
            }
        }
        if multiline {
            cb.add("%<", ());
        }
        cb.add_code(suffix);
        cb.add_line();
        if let Some(body) = function.body() {
            cb.add("%>", ());
            cb.add_code(body.clone());
            if !body.ends_with_newline_or_block_close() {
                cb.add_line();
            }
            cb.add("%<}\n", ());
        }
        cb.build()
    }
}

/// Import and barrel export lists share the same line-width policy.
pub(super) fn named_list(prefix: &str, names: &[String], module: &str) -> String {
    let quoted = sigil_stitch::lang::typescript::TypeScript::new().render_string_literal(module);
    let inline = format!("{prefix} {{ {} }} from {quoted};", names.join(", "));
    if names.len() <= 1 || inline.chars().count() <= WIDTH {
        inline
    } else {
        format!(
            "{prefix} {{\n{}}} from {quoted};",
            names
                .iter()
                .map(|name| format!("  {name},\n"))
                .collect::<String>()
        )
    }
}

impl TypeScript {
    fn type_part(
        &self,
        ty: &TypeName,
        column: usize,
        depth: usize,
    ) -> Result<CodeBlock, SigilStitchError> {
        let fragment = CodeBlock::of("%T", ty.clone())?;
        if column + self.width(&fragment)? + 2 <= WIDTH {
            return Ok(fragment);
        }
        if let TypeName::Application { base, arguments } = ty {
            use sigil_stitch::spec::where_spec::TypeArgument;
            let mut cb = CodeBlock::builder();
            cb.add("%T<\n%>", *base.clone());
            for (i, arg) in arguments.iter().enumerate() {
                if i > 0 {
                    cb.add(",\n", ());
                }
                let TypeArgument::Single(ty) = arg else {
                    return Ok(fragment);
                };
                cb.add_code(self.type_part(
                    ty,
                    (depth + 1) * self.indent_unit().len(),
                    depth + 1,
                )?);
            }
            cb.add("\n%<>", ());
            return cb.build();
        }
        Ok(fragment)
    }

    fn type_expression(
        &self,
        ty: &TypeName,
        column: usize,
        depth: usize,
    ) -> Result<CodeBlock, SigilStitchError> {
        let inline = CodeBlock::of("%T", ty.clone())?;
        if let TypeName::Intersection(parts) = ty
            && column + self.width(&inline)? > WIDTH
            && let Some((first, rest)) = parts.split_first()
        {
            use sigil_stitch::code_node::CodeNode;
            let mut result =
                sigil_quote!(TypeScript { $C(self.type_part(first, column, depth)?) })?;
            result = super::layout::inline(result);
            for part in rest {
                // The type-name renderer has no width-aware intersection layout.
                // Keep each member semantic and add only connector/layout nodes.
                result.nodes_mut().extend([
                    CodeNode::Literal(" &".into()),
                    CodeNode::Newline,
                    CodeNode::Indent,
                    CodeNode::Nested(self.type_part(part, (depth + 1) * 2, depth + 1)?),
                    CodeNode::Dedent,
                ]);
            }
            let mut spaced = sigil_quote!(TypeScript {})?;
            spaced
                .nodes_mut()
                .extend([CodeNode::Literal(" ".into()), CodeNode::Nested(result)]);
            return Ok(spaced);
        }
        if let TypeName::Callable {
            parameters,
            returns,
        } = ty
            && column + self.width(&inline)? > WIDTH
        {
            use sigil_stitch::spec::where_spec::{CallableParam, CallableParamPresence};
            let mut cb = CodeBlock::builder();
            cb.add(" (\n%>", ());
            for parameter in parameters {
                let CallableParam::Single {
                    name,
                    type_name,
                    presence,
                } = parameter
                else {
                    return CodeBlock::of(" %T", ty.clone());
                };
                cb.add(
                    "%L%L: %T,\n",
                    vec![
                        Arg::Literal(name.clone().unwrap_or_else(|| "arg".into())),
                        Arg::Literal(if *presence == CallableParamPresence::Optional {
                            "?".into()
                        } else {
                            String::new()
                        }),
                        Arg::TypeName(type_name.clone()),
                    ],
                );
            }
            cb.add(
                "%<) => %T",
                returns
                    .first()
                    .cloned()
                    .unwrap_or_else(|| TypeName::primitive("void")),
            );
            return cb.build();
        }
        if column + self.width(&inline)? <= WIDTH || !matches!(ty, TypeName::Union(_)) {
            let mut cb = CodeBlock::builder();
            cb.add(" ", ());
            cb.add_code(self.type_part(ty, column, depth)?);
            return cb.build();
        }
        let TypeName::Union(members) = ty else {
            unreachable!()
        };
        let mut cb = CodeBlock::builder();
        cb.add("\n%>", ());
        for (i, member) in members.iter().enumerate() {
            if i > 0 {
                cb.add_line();
            }
            if let TypeName::Intersection(parts) = member {
                cb.add("| (", ());
                let multiline = 5 + self.width(&CodeBlock::of("%T", member.clone())?)? > WIDTH;
                for (i, part) in parts.iter().enumerate() {
                    if i > 0 {
                        cb.add(if multiline { " &\n%>%>" } else { " & " }, ());
                    }
                    cb.add_code(self.type_part(
                        part,
                        if multiline && i > 0 {
                            (depth + 3) * 2
                        } else {
                            5
                        },
                        if multiline && i > 0 {
                            depth + 3
                        } else {
                            depth + 1
                        },
                    )?);
                    if multiline && i > 0 {
                        cb.add("%<%<", ());
                    }
                }
                cb.add(")", ());
            } else {
                cb.add("| %T", member.clone());
            }
        }
        cb.add("%<", ());
        cb.build()
    }
}

/// Compose an expression without the statement newline added by a quoted block.
pub(super) fn inline(mut block: CodeBlock) -> CodeBlock {
    fn trim(nodes: &mut Vec<sigil_stitch::code_node::CodeNode>) {
        use sigil_stitch::code_node::CodeNode;
        match nodes.last_mut() {
            Some(CodeNode::Newline) => {
                nodes.pop();
            }
            Some(CodeNode::Sequence(children)) => trim(children),
            Some(CodeNode::Nested(block)) => trim(block.nodes_mut()),
            _ => {}
        }
    }
    trim(block.nodes_mut());
    block
}

pub(super) fn fragment_width(block: &CodeBlock) -> Result<usize, SigilStitchError> {
    Ok(block
        .render_standalone(
            &sigil_stitch::lang::typescript::TypeScript::new(),
            usize::MAX,
        )?
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .count())
}

fn fits(block: &CodeBlock, column: usize) -> Result<bool, SigilStitchError> {
    let rendered = block.render_standalone(
        &TypeScript::from(sigil_stitch::lang::typescript::TypeScript::new()),
        usize::MAX,
    )?;
    Ok(
        !rendered.trim_end().contains('\n')
            && column + rendered.trim_end().chars().count() <= WIDTH,
    )
}

fn delimited(
    mut prefix: CodeBlock,
    open: &'static str,
    close: &'static str,
    items: &[CodeBlock],
    multiline: bool,
) -> CodeBlock {
    use sigil_stitch::code_node::CodeNode;
    // sigil-stitch has no expression-list layout API; quoted object braces and
    // parenthesized argument lists discard their boundary newlines. Only their
    // static delimiters/whitespace live here. Items remain semantic quote trees.
    let nodes = prefix.nodes_mut();
    nodes.push(CodeNode::Literal(open.into()));
    if multiline {
        nodes.extend([CodeNode::Newline, CodeNode::Indent]);
    }
    for (index, item) in items.iter().enumerate() {
        if index > 0 && !multiline {
            nodes.push(CodeNode::Literal(", ".into()));
        }
        nodes.push(CodeNode::Nested(inline(item.clone())));
        if multiline {
            nodes.extend([CodeNode::Literal(",".into()), CodeNode::Newline]);
        }
    }
    if multiline {
        nodes.push(CodeNode::Dedent);
    }
    nodes.push(CodeNode::Literal(close.into()));
    prefix
}

pub(super) fn call(
    function: CodeBlock,
    arguments: &[CodeBlock],
    column: usize,
) -> Result<CodeBlock, SigilStitchError> {
    let flat = delimited(inline(function.clone()), "(", ")", arguments, false);
    if fits(&flat, column)? {
        return Ok(flat);
    }
    Ok(delimited(inline(function), "(", ")", arguments, true))
}

pub(super) fn block_arrow(mut parameters: CodeBlock, body: CodeBlock) -> CodeBlock {
    use sigil_stitch::code_node::CodeNode;
    // As with expression lists above, quotes do not retain the opening newline
    // of a block-bodied arrow nested inside call parentheses.
    parameters = inline(parameters);
    parameters.nodes_mut().extend([
        CodeNode::Literal(" => {".into()),
        CodeNode::Newline,
        CodeNode::Indent,
        CodeNode::Nested(body),
        CodeNode::Dedent,
        CodeNode::Literal("}".into()),
    ]);
    parameters
}

pub(super) fn array(items: &[CodeBlock], column: usize) -> Result<CodeBlock, SigilStitchError> {
    let flat = delimited(sigil_quote!(TypeScript {})?, "[", "]", items, false);
    if fits(&flat, column)? {
        return Ok(flat);
    }
    Ok(delimited(
        sigil_quote!(TypeScript {})?,
        "[",
        "]",
        items,
        true,
    ))
}

pub(super) fn const_value(
    name: &str,
    value: CodeBlock,
    column: usize,
) -> Result<CodeBlock, SigilStitchError> {
    use sigil_stitch::code_node::CodeNode;
    let flat = sigil_quote!(TypeScript { const $N(name) = $C(inline(value.clone())); })?;
    if fits(&flat, column)? {
        return Ok(flat);
    }
    // Local bindings have no declaration spec. Preserve the quoted identifier
    // and expression, adding only the unsupported continuation indentation.
    let mut prefix = inline(sigil_quote!(TypeScript { const $N(name) = })?);
    prefix.nodes_mut().extend([
        CodeNode::Newline,
        CodeNode::Indent,
        CodeNode::Nested(inline(value)),
        CodeNode::Literal(";".into()),
        CodeNode::Newline,
        CodeNode::Dedent,
    ]);
    Ok(prefix)
}

pub(super) fn indexed_assertion(
    value: CodeBlock,
    ty: TypeName,
    key: &str,
    column: usize,
) -> Result<CodeBlock, SigilStitchError> {
    use sigil_stitch::code_node::CodeNode;
    let flat = inline(sigil_quote!(TypeScript { ($C(value.clone()) as $T(ty.clone()))[$S(key)] })?);
    if fits(&flat, column)? {
        return Ok(flat);
    }
    let mut head = inline(sigil_quote!(TypeScript { ($C(value.clone()) as $T(ty.clone())) })?);
    if fits(&head, column)? {
        // A wide computed index, unlike an argument list, has no trailing comma.
        // Its string key stays quoted semantically inside the layout group.
        head.nodes_mut().extend([
            CodeNode::Literal("[".into()),
            CodeNode::Newline,
            CodeNode::Indent,
            CodeNode::Nested(inline(sigil_quote!(TypeScript { $S(key) })?)),
            CodeNode::Newline,
            CodeNode::Dedent,
            CodeNode::Literal("]".into()),
        ]);
        return Ok(head);
    }
    let lang = TypeScript::from(sigil_stitch::lang::typescript::TypeScript::new());
    let mut assertion = inline(sigil_quote!(TypeScript { $C(value) as })?);
    let ty = lang.type_expression(
        &ty,
        column + 2 + lang.width(&assertion)? + 1,
        column / 2 + 1,
    )?;
    assertion.nodes_mut().push(CodeNode::Nested(ty));
    // Quotes have no parenthesized-expression layout node. Keep the assertion's
    // name/type and the indexed string semantic; only group punctuation is raw.
    let mut group = sigil_quote!(TypeScript {})?;
    group.nodes_mut().extend([
        CodeNode::Literal("(".into()),
        CodeNode::Newline,
        CodeNode::Indent,
        CodeNode::Nested(assertion),
        CodeNode::Newline,
        CodeNode::Dedent,
        CodeNode::Literal(")".into()),
    ]);
    Ok(inline(sigil_quote!(TypeScript { $C(group)[$S(key)] })?))
}

pub(super) fn type_guard(
    name: &str,
    parent: TypeName,
    narrowed: TypeName,
    body: CodeBlock,
) -> Result<CodeBlock, SigilStitchError> {
    use sigil_stitch::code_node::CodeNode;
    let lang = TypeScript::from(sigil_stitch::lang::typescript::TypeScript::new());
    let prefix = inline(sigil_quote!(TypeScript { export function $N(name) })?);
    let parameter = inline(sigil_quote!(TypeScript { value: $T(parent) })?);
    let suffix = inline(sigil_quote!(TypeScript { : value is $T(narrowed.clone()) })?);
    let header = delimited(
        prefix.clone(),
        "(",
        ")",
        std::slice::from_ref(&parameter),
        false,
    );
    let multiline = lang.width(&header)? + lang.width(&suffix)? + 2 > WIDTH;
    let mut header = delimited(prefix, "(", ")", &[parameter], multiline);
    let column = if multiline { 1 } else { lang.width(&header)? } + ": value is".len() + 1;
    // FunSpec has no semantic type-predicate return slot. The small `value is`
    // delimiter is the only raw syntax; both types and the body stay structured.
    header.nodes_mut().extend([
        CodeNode::Literal(": value is".into()),
        CodeNode::Nested(lang.type_expression(&narrowed, column, 0)?),
        CodeNode::Literal(" {".into()),
        CodeNode::Newline,
        CodeNode::Indent,
        CodeNode::Nested(body),
        CodeNode::Dedent,
        CodeNode::Literal("}".into()),
        CodeNode::Newline,
    ]);
    Ok(header)
}

pub(super) fn return_object(
    fields: &[CodeBlock],
    cast: Option<TypeName>,
    column: usize,
) -> Result<CodeBlock, SigilStitchError> {
    let object = delimited(sigil_quote!(TypeScript {})?, "{ ", " }", fields, false);
    let flat = if let Some(cast) = &cast {
        sigil_quote!(TypeScript { return $C(object) as $T(cast.clone()); })?
    } else {
        sigil_quote!(TypeScript { return $C(object); })?
    };
    if fits(&flat, column)? {
        return Ok(flat);
    }
    let object = delimited(sigil_quote!(TypeScript {})?, "{", "}", fields, true);
    if let Some(cast) = cast {
        sigil_quote!(TypeScript { return $C(object) as $T(cast); })
    } else {
        sigil_quote!(TypeScript { return $C(object); })
    }
}

/// Keep both guards visible when an optional nullable conversion exceeds a line.
pub(super) fn optional_nullable_property(
    key: CodeBlock,
    access: CodeBlock,
    value: CodeBlock,
) -> Result<CodeBlock, SigilStitchError> {
    use sigil_stitch::code_node::CodeNode;
    // sigil_quote preserves these line boundaries but does not infer indentation
    // for brace-free nested ternaries. Only that whitespace is supplied here;
    // keys, guards, branch expressions and their imports remain quote trees.
    let mut field = sigil_quote!(TypeScript { $C(key): })?;
    field.nodes_mut().extend([
        CodeNode::Indent,
        CodeNode::Nested(sigil_quote!(TypeScript { $C(access.clone()) !== undefined })?),
        CodeNode::Indent,
        CodeNode::Nested(sigil_quote!(TypeScript { ? $C(access) === null })?),
        CodeNode::Indent,
        CodeNode::Nested(sigil_quote!(TypeScript { ? null })?),
        CodeNode::Nested(sigil_quote!(TypeScript { : $C(value) })?),
        CodeNode::Dedent,
        CodeNode::Nested(inline(sigil_quote!(TypeScript { : undefined, })?)),
        CodeNode::Dedent,
        CodeNode::Dedent,
        CodeNode::Newline,
    ]);
    Ok(field)
}

impl From<sigil_stitch::lang::typescript::TypeScript> for TypeScript {
    fn from(lang: sigil_stitch::lang::typescript::TypeScript) -> Self {
        Self(lang)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sigil_stitch::assert_rendered;

    #[test]
    fn optional_nullable_property_breaks_both_guards_without_losing_imports() {
        let converter = TypeName::importable("./Node", "nodeFromJSON");
        let field = optional_nullable_property(
            sigil_quote!(TypeScript { nextNode }).unwrap(),
            sigil_quote!(TypeScript { json.next_node }).unwrap(),
            sigil_quote!(TypeScript { $T(converter)(json.next_node) }).unwrap(),
        )
        .unwrap();
        let fields = vec![field.clone()];
        let body = sigil_quote!(TypeScript {
            return {
                $C_each(fields)
            };
        })
        .unwrap();
        assert_rendered!(
            TypeScript::from(sigil_stitch::lang::typescript::TypeScript::new()),
            width = 100,
            field,
            "import { nodeFromJSON } from './Node';\n\nnextNode:\n  json.next_node !== undefined\n    ? json.next_node === null\n      ? null\n      : nodeFromJSON(json.next_node)\n    : undefined,\n"
        );
        assert_rendered!(
            TypeScript::from(sigil_stitch::lang::typescript::TypeScript::new()),
            width = 100,
            body,
            "import { nodeFromJSON } from './Node';\n\nreturn {\n  nextNode:\n    json.next_node !== undefined\n      ? json.next_node === null\n        ? null\n        : nodeFromJSON(json.next_node)\n      : undefined,\n};\n"
        );
    }

    #[test]
    fn expression_layout_keeps_semantic_imports_and_object_punctuation() {
        let converter = TypeName::importable("./Detail", "detailFromJSON");
        let argument = sigil_quote!(TypeScript { json as unknown as $T(TypeName::importable_type("./Detail", "Detail$Wire")) }).unwrap();
        let call = call(
            sigil_quote!(TypeScript { $T(converter) }).unwrap(),
            &[argument],
            80,
        )
        .unwrap();
        let field = sigil_quote!(TypeScript { [$S("payload")]: $C(call) }).unwrap();
        let block = return_object(&[field], Some(TypeName::primitive("Event")), 4).unwrap();
        assert_rendered!(
            TypeScript::from(sigil_stitch::lang::typescript::TypeScript::new()),
            width = 100,
            block,
            "import type { Detail$Wire } from './Detail';\nimport { detailFromJSON } from './Detail';\n\nreturn {\n  ['payload']: detailFromJSON(\n    json as unknown as Detail$Wire,\n  ),\n} as Event;\n"
        );
    }

    #[test]
    fn intersection_return_spreads_use_formatted_object_layout() {
        let fields = [
            sigil_quote!(TypeScript { ...optionsToJSON(value as Options) }).unwrap(),
            sigil_quote!(TypeScript { ...toolFunctionAllOf2ToJSON(value as ToolFunctionAllOf2) })
                .unwrap(),
        ];
        let block =
            return_object(&fields, Some(TypeName::primitive("ToolFunction$Wire")), 2).unwrap();
        assert_rendered!(
            TypeScript::from(sigil_stitch::lang::typescript::TypeScript::new()),
            width = 100,
            block,
            "return {\n  ...optionsToJSON(value as Options),\n  ...toolFunctionAllOf2ToJSON(value as ToolFunctionAllOf2),\n} as ToolFunction$Wire;\n"
        );
    }

    #[test]
    fn indexed_assertion_breaks_the_key_when_the_cast_head_fits() {
        let payload = TypeName::importable_type(
            "./Payload",
            "FooAdjacentPayloadFooInlineObjectPayloadValue$Wire",
        );
        let ty = crate::generators::type_application(
            TypeName::primitive("Record"),
            vec![TypeName::string_literal("payload_value"), payload],
        );
        let block = indexed_assertion(
            sigil_quote!(TypeScript { json }).unwrap(),
            ty,
            "payload_value",
            8,
        )
        .unwrap();
        assert_rendered!(
            TypeScript::from(sigil_stitch::lang::typescript::TypeScript::new()),
            width = 100,
            block,
            "import type { FooAdjacentPayloadFooInlineObjectPayloadValue$Wire } from './Payload';\n\n(json as Record<'payload_value', FooAdjacentPayloadFooInlineObjectPayloadValue$Wire>)[\n  'payload_value'\n]\n"
        );
    }
}
