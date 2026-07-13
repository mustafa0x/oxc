//! Feature-gated adapter for using rsvelte as Oxc's Svelte backend.
//!
//! This crate is intentionally small while the Phase 0 dependency spike is
//! underway. Product crates should depend on this adapter rather than reaching
//! into rsvelte directly.

#[cfg(feature = "rsvelte")]
mod rsvelte_backend {
    use std::fmt;

    use oxc_span::Span;
    use svelte_compiler_rust::{
        CompileOptions, GenerateMode, ParseOptions,
        ast::{
            Fragment, Root, Script, ScriptContext, TemplateNode, arena::SerializeArenaGuard,
            typed_expr::JsNode,
        },
        compiler::phases::phase2_analyze::{
            AnalysisError, Binding, BindingKind, ComponentAnalysis, analyze_component,
        },
        compiler::phases::phase3_transform::{TransformError, transform_component},
        error::ParseError,
        parse,
    };

    /// Zero-based byte span plus one-based line / zero-based column positions.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SvelteSourceRange {
        pub span: Span,
        pub start: SvelteSourcePosition,
        pub end: SvelteSourcePosition,
    }

    impl SvelteSourceRange {
        fn new(source: &str, start: u32, end: u32) -> Self {
            Self {
                span: Span::new(start, end),
                start: source_position(source, start),
                end: source_position(source, end),
            }
        }
    }

    /// Source position using rsvelte/Svelte line-column conventions.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SvelteSourcePosition {
        pub line: u32,
        pub column: u32,
    }

    /// Svelte comment kind normalized for Oxc callers.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SvelteCommentKind {
        Html,
        JsLine,
        JsBlock,
    }

    /// Comment captured from either Svelte markup or embedded JavaScript.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SvelteComment {
        pub kind: SvelteCommentKind,
        pub range: SvelteSourceRange,
        pub text: String,
    }

    /// Svelte script block kind.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SvelteScriptKind {
        Instance,
        Module,
    }

    /// Script tag metadata needed by linting and formatting entrypoints.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SvelteScript {
        pub kind: SvelteScriptKind,
        pub tag_range: SvelteSourceRange,
        pub body_range: SvelteSourceRange,
        pub is_typescript: bool,
    }

    /// Parser warning emitted by rsvelte.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SvelteParseWarning {
        pub code: String,
        pub message: String,
        pub range: SvelteSourceRange,
    }

    /// Oxc-facing parse payload for `.svelte` files.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SvelteParseResult {
        pub scripts: Vec<SvelteScript>,
        pub comments: Vec<SvelteComment>,
        pub warnings: Vec<SvelteParseWarning>,
        pub top_level_node_count: usize,
    }

    /// Component-wide semantic facts needed by partial-file lint rules.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SvelteSemanticSummary {
        /// Absolute source spans that rsvelte resolved to a component binding.
        pub resolved_references: Vec<Span>,
        /// Absolute declaration starts with uses outside Oxc's isolated script scope.
        pub used_bindings: Vec<u32>,
        /// Framework-provided globals that are valid in embedded scripts.
        pub implicit_globals: Vec<String>,
    }

    /// Oxc-facing parse error for `.svelte` files.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SvelteParseError {
        pub code: String,
        pub message: String,
        pub range: SvelteSourceRange,
    }

    /// Svelte-specific formatting controls layered on top of Oxc's JS options.
    #[derive(Debug, Clone)]
    pub struct SvelteFormatOptions {
        pub indent_script_and_style: bool,
        pub sort_order: Option<String>,
        pub allow_shorthand: bool,
        pub single_attribute_per_line: bool,
        pub bracket_same_line: bool,
        pub style_options: Option<rsvelte_formatter::CssFormatOptions>,
    }

    impl Default for SvelteFormatOptions {
        fn default() -> Self {
            Self {
                indent_script_and_style: true,
                sort_order: None,
                allow_shorthand: true,
                single_attribute_per_line: false,
                bracket_same_line: false,
                style_options: None,
            }
        }
    }

    impl fmt::Display for SvelteParseError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}: {}", self.code, self.message)
        }
    }

    impl std::error::Error for SvelteParseError {}

    /// Small, stable parse summary used by Phase 0 smoke tests.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SvelteParseSummary {
        pub has_instance_script: bool,
        pub has_module_script: bool,
        pub top_level_node_count: usize,
        pub comment_count: usize,
        pub warning_count: usize,
    }

    /// Parse Svelte source with rsvelte and return the Oxc-facing payload.
    ///
    /// # Errors
    ///
    /// Returns the normalized rsvelte parse error when the source is invalid.
    pub fn parse_svelte(source: &str) -> Result<SvelteParseResult, SvelteParseError> {
        let mut root = parse(source, ParseOptions::default())
            .map_err(|error| convert_parse_error(source, &error))?;
        // SAFETY: `root.arena` lives until the guard is dropped at the end of this function.
        let _arena_guard = unsafe { SerializeArenaGuard::new(&raw const root.arena) };

        let mut result = build_parse_result(source, &root);

        let compile_options = CompileOptions {
            generate: GenerateMode::None,
            enable_sourcemap: false,
            ..CompileOptions::default()
        };
        let analysis = analyze_component(&mut root, source, &compile_options)
            .map_err(|error| convert_analysis_error(source, &error))?;
        let transform = transform_component(&analysis, &root, source, &compile_options)
            .map_err(|error| convert_transform_error(source, &error))?;

        result.warnings = transform
            .warnings
            .into_iter()
            .map(|warning| {
                let start = warning.start.unwrap_or(0);
                let end = warning.end.unwrap_or(start).max(start);
                SvelteParseWarning {
                    code: warning.code,
                    message: warning.message,
                    range: SvelteSourceRange::new(source, start, end),
                }
            })
            .collect();

        Ok(result)
    }

    /// Parse Svelte syntax without running rsvelte's analysis and transform phases.
    ///
    /// This path extracts scripts and comments for linting while leaving Svelte compiler
    /// diagnostics to configured lint rules.
    ///
    /// # Errors
    ///
    /// Returns the normalized rsvelte parse error when the source is invalid.
    pub fn parse_svelte_syntax(source: &str) -> Result<SvelteParseResult, SvelteParseError> {
        let root = parse(source, ParseOptions::default())
            .map_err(|error| convert_parse_error(source, &error))?;
        // SAFETY: `root.arena` lives until the guard is dropped at the end of this function.
        let _arena_guard = unsafe { SerializeArenaGuard::new(&raw const root.arena) };

        Ok(build_parse_result(source, &root))
    }

    /// Parse a component and retain compact cross-section semantic facts for linting.
    ///
    /// Analysis errors intentionally produce no semantic summary. The lint runtime can still
    /// report syntax diagnostics and lint rules that do not require component-wide scopes.
    ///
    /// # Errors
    ///
    /// Returns the normalized rsvelte parse error when the component has invalid syntax.
    pub fn parse_svelte_for_lint(
        source: &str,
    ) -> Result<(SvelteParseResult, Option<SvelteSemanticSummary>), SvelteParseError> {
        let mut root = parse(source, ParseOptions::default())
            .map_err(|error| convert_parse_error(source, &error))?;
        // SAFETY: `root.arena` lives until the guard is dropped at the end of this function.
        let _arena_guard = unsafe { SerializeArenaGuard::new(&raw const root.arena) };

        let parsed = build_parse_result(source, &root);
        let compile_options = CompileOptions {
            generate: GenerateMode::None,
            enable_sourcemap: false,
            ..CompileOptions::default()
        };
        let semantic = analyze_component(&mut root, source, &compile_options)
            .ok()
            .map(|analysis| build_semantic_summary(source, &analysis, &root, &parsed.scripts));

        Ok((parsed, semantic))
    }

    fn build_semantic_summary(
        source: &str,
        analysis: &ComponentAnalysis,
        root: &Root,
        scripts: &[SvelteScript],
    ) -> SvelteSemanticSummary {
        let mut resolved_references = Vec::new();
        let mut used_bindings = Vec::new();

        for binding in &analysis.root.bindings {
            resolved_references.extend(
                binding
                    .references
                    .iter()
                    .map(|reference| Span::new(reference.start, reference.end)),
            );

            let Some(declaration_start) = binding_declaration_start(source, binding, root, scripts)
            else {
                continue;
            };
            let declaration_script = scripts.iter().find(|script| {
                let span = script.body_range.span;
                declaration_start >= span.start && declaration_start < span.end
            });
            let used_outside_script = binding.references.iter().any(|reference| {
                reference.is_template_reference
                    || declaration_script.is_some_and(|script| {
                        let span = script.body_range.span;
                        !(span.start..span.end).contains(&reference.start)
                    })
            });
            let externally_observed_bindable =
                binding.kind == BindingKind::BindableProp && binding.is_updated();
            if used_outside_script || externally_observed_bindable {
                used_bindings.push(declaration_start);
            }
        }

        for store_subscription in analysis.root.bindings.iter().filter(|binding| {
            binding.kind == BindingKind::StoreSub && !binding.references.is_empty()
        }) {
            let Some(store_name) = store_subscription.name.strip_prefix('$') else {
                continue;
            };
            if let Some(binding_index) =
                analysis.root.get_binding(store_name, analysis.root.instance_scope_index)
                && let Some(declaration_start) = binding_declaration_start(
                    source,
                    &analysis.root.bindings[binding_index],
                    root,
                    scripts,
                )
            {
                used_bindings.push(declaration_start);
            }
        }

        let mut component_bindings = Vec::new();
        collect_component_bindings(&root.fragment, &mut component_bindings);
        for binding_index in component_bindings {
            if let Some(binding) = analysis.root.bindings.get(binding_index)
                && let Some(declaration_start) =
                    binding_declaration_start(source, binding, root, scripts)
            {
                used_bindings.push(declaration_start);
            }
        }

        let mut implicit_globals = Vec::new();
        if analysis.runes {
            implicit_globals.extend(
                ["$state", "$derived", "$props", "$bindable", "$effect", "$inspect", "$host"]
                    .map(str::to_string),
            );
        } else {
            if analysis.uses_props {
                implicit_globals.push("$$props".to_string());
            }
            if analysis.uses_rest_props {
                implicit_globals.push("$$restProps".to_string());
            }
            if analysis.uses_slots {
                implicit_globals.push("$$slots".to_string());
            }
        }

        resolved_references.sort_unstable_by_key(|span| (span.start, span.end));
        resolved_references.dedup();
        used_bindings.sort_unstable();
        used_bindings.dedup();
        implicit_globals.sort_unstable();

        SvelteSemanticSummary { resolved_references, used_bindings, implicit_globals }
    }

    fn binding_declaration_start(
        source: &str,
        binding: &Binding,
        root: &Root,
        scripts: &[SvelteScript],
    ) -> Option<u32> {
        let matches_name = |start: u32| {
            usize::try_from(start).ok().is_some_and(|start| {
                start
                    .checked_add(binding.name.len())
                    .is_some_and(|end| source.get(start..end) == Some(binding.name.as_str()))
            })
        };

        if let Some(start) = binding
            .references
            .iter()
            .find(|reference| reference.is_self_declaration)
            .map(|reference| reference.start)
            .filter(|start| matches_name(*start))
        {
            return Some(start);
        }

        if let Some(start) = binding.declaration_start {
            if matches_name(start) {
                return Some(start);
            }
            if let Some(start) = scripts.iter().find_map(|script| {
                start
                    .checked_sub(script.body_range.span.start)
                    .filter(|candidate| matches_name(*candidate))
            }) {
                return Some(start);
            }
        }

        let script = if binding.scope_index == 0 {
            root.module.as_deref()
        } else {
            root.instance.as_deref()
        }?;
        let program = script.content.as_node();
        root.arena
            .get_js_children(program.body_stmts())
            .iter()
            .filter(|statement| matches!(statement, JsNode::ImportDeclaration { .. }))
            .flat_map(|statement| root.arena.get_js_children(statement.specifiers()))
            .filter_map(JsNode::local)
            .map(|local| root.arena.get_js_node(local))
            .find_map(|local| match local {
                JsNode::Identifier { name, start, .. }
                    if name.as_str() == binding.name && matches_name(*start) =>
                {
                    Some(*start)
                }
                _ => None,
            })
    }

    fn collect_component_bindings(fragment: &Fragment, bindings: &mut Vec<usize>) {
        for node in &fragment.nodes {
            match node {
                TemplateNode::Component(component) => {
                    bindings.extend(component.metadata.expression.references.iter().copied());
                    collect_component_bindings(&component.fragment, bindings);
                }
                TemplateNode::IfBlock(block) => {
                    collect_component_bindings(&block.consequent, bindings);
                    if let Some(alternate) = &block.alternate {
                        collect_component_bindings(alternate, bindings);
                    }
                }
                TemplateNode::EachBlock(block) => {
                    collect_component_bindings(&block.body, bindings);
                    if let Some(fallback) = &block.fallback {
                        collect_component_bindings(fallback, bindings);
                    }
                }
                TemplateNode::AwaitBlock(block) => {
                    for fragment in
                        [&block.pending, &block.then, &block.catch].into_iter().flatten()
                    {
                        collect_component_bindings(fragment, bindings);
                    }
                }
                TemplateNode::KeyBlock(block) => {
                    collect_component_bindings(&block.fragment, bindings);
                }
                TemplateNode::SnippetBlock(block) => {
                    collect_component_bindings(&block.body, bindings);
                }
                TemplateNode::RegularElement(element) => {
                    collect_component_bindings(&element.fragment, bindings);
                }
                TemplateNode::TitleElement(element) => {
                    collect_component_bindings(&element.fragment, bindings);
                }
                TemplateNode::SlotElement(element) => {
                    collect_component_bindings(&element.fragment, bindings);
                }
                TemplateNode::SvelteBody(element)
                | TemplateNode::SvelteDocument(element)
                | TemplateNode::SvelteFragment(element)
                | TemplateNode::SvelteBoundary(element)
                | TemplateNode::SvelteHead(element)
                | TemplateNode::SvelteOptions(element)
                | TemplateNode::SvelteSelf(element)
                | TemplateNode::SvelteWindow(element) => {
                    collect_component_bindings(&element.fragment, bindings);
                }
                TemplateNode::SvelteComponent(element) => {
                    collect_component_bindings(&element.fragment, bindings);
                }
                TemplateNode::SvelteElement(element) => {
                    collect_component_bindings(&element.fragment, bindings);
                }
                TemplateNode::Text(_)
                | TemplateNode::Comment(_)
                | TemplateNode::ExpressionTag(_)
                | TemplateNode::HtmlTag(_)
                | TemplateNode::ConstTag(_)
                | TemplateNode::DeclarationTag(_)
                | TemplateNode::DebugTag(_)
                | TemplateNode::RenderTag(_)
                | TemplateNode::AttachTag(_) => {}
            }
        }
    }

    fn build_parse_result(source: &str, root: &Root) -> SvelteParseResult {
        let mut scripts = Vec::with_capacity(2);
        if let Some(script) = root.module.as_deref() {
            scripts.push(convert_script(source, SvelteScriptKind::Module, script));
        }
        if let Some(script) = root.instance.as_deref() {
            scripts.push(convert_script(source, SvelteScriptKind::Instance, script));
        }
        scripts.sort_by_key(|script| script.tag_range.span.start);

        let mut comments = Vec::new();
        for comment in &root.comments {
            let kind = match comment.kind {
                svelte_compiler_rust::ast::template::JsCommentKind::Line => {
                    SvelteCommentKind::JsLine
                }
                svelte_compiler_rust::ast::template::JsCommentKind::Block => {
                    SvelteCommentKind::JsBlock
                }
            };
            comments.push(SvelteComment {
                kind,
                range: SvelteSourceRange::new(source, comment.start, comment.end),
                text: comment.value.to_string(),
            });
        }
        collect_html_comments(source, &root.fragment, &mut comments);
        comments.sort_by_key(|comment| comment.range.span.start);

        SvelteParseResult {
            scripts,
            comments,
            warnings: Vec::new(),
            top_level_node_count: root.fragment.nodes.len(),
        }
    }

    /// Parse Svelte source with rsvelte and return a minimal Oxc-facing summary.
    ///
    /// # Errors
    ///
    /// Returns the normalized rsvelte parse error message when the source is invalid.
    pub fn parse_svelte_summary(source: &str) -> Result<SvelteParseSummary, String> {
        let parsed = parse_svelte(source).map_err(|error| error.to_string())?;

        Ok(SvelteParseSummary {
            has_instance_script: parsed
                .scripts
                .iter()
                .any(|script| script.kind == SvelteScriptKind::Instance),
            has_module_script: parsed
                .scripts
                .iter()
                .any(|script| script.kind == SvelteScriptKind::Module),
            top_level_node_count: parsed.top_level_node_count,
            comment_count: parsed.comments.len(),
            warning_count: parsed.warnings.len(),
        })
    }

    /// Format Svelte source with rsvelte's formatter.
    ///
    /// # Errors
    ///
    /// Returns the rsvelte formatter error message when formatting fails.
    pub fn format_svelte(source: &str) -> Result<String, String> {
        format_svelte_with_options(source, rsvelte_formatter::JsFormatOptions::default())
    }

    /// Format Svelte source with rsvelte's formatter and Oxc JS options.
    ///
    /// # Errors
    ///
    /// Returns the rsvelte formatter error message when formatting fails.
    pub fn format_svelte_with_options(
        source: &str,
        js_options: rsvelte_formatter::JsFormatOptions,
    ) -> Result<String, String> {
        format_svelte_with_options_and_indent(source, js_options, true)
    }

    /// Format Svelte source with Oxc JS options and script/style indentation control.
    ///
    /// # Errors
    ///
    /// Returns the rsvelte formatter error message when formatting fails.
    pub fn format_svelte_with_options_and_indent(
        source: &str,
        js_options: rsvelte_formatter::JsFormatOptions,
        indent_script_and_style: bool,
    ) -> Result<String, String> {
        let svelte_options =
            SvelteFormatOptions { indent_script_and_style, ..SvelteFormatOptions::default() };
        format_svelte_with_config(source, js_options, &svelte_options)
    }

    /// Format Svelte source with the full native rsvelte option set.
    ///
    /// # Errors
    ///
    /// Returns the rsvelte formatter error message when formatting fails.
    pub fn format_svelte_with_config(
        source: &str,
        js_options: rsvelte_formatter::JsFormatOptions,
        svelte_options: &SvelteFormatOptions,
    ) -> Result<String, String> {
        let sort_order = svelte_options
            .sort_order
            .as_deref()
            .and_then(rsvelte_formatter::SortOrderSpec::parse)
            .unwrap_or_default();
        let options = rsvelte_formatter::FormatOptions {
            js: js_options,
            style_formatter: svelte_options
                .style_options
                .map(rsvelte_formatter::native_style_formatter),
            single_attribute_per_line: svelte_options.single_attribute_per_line,
            allow_shorthand: svelte_options.allow_shorthand,
            indent_script_and_style: svelte_options.indent_script_and_style,
            sort_order,
            bracket_same_line: svelte_options.bracket_same_line,
            ..rsvelte_formatter::FormatOptions::default()
        };
        rsvelte_formatter::format(source, &options).map_err(|error| error.to_string())
    }

    fn convert_script(source: &str, kind: SvelteScriptKind, script: &Script) -> SvelteScript {
        let body_start = script.content_offset;
        let body_end = find_script_body_end(source, body_start).unwrap_or(script.end);

        debug_assert!(matches!(
            (kind, script.context),
            (SvelteScriptKind::Instance, ScriptContext::Default)
                | (SvelteScriptKind::Module, ScriptContext::Module)
        ));

        SvelteScript {
            kind,
            tag_range: SvelteSourceRange::new(source, script.start, script.end),
            body_range: SvelteSourceRange::new(source, body_start, body_end),
            is_typescript: script.is_typescript,
        }
    }

    fn find_script_body_end(source: &str, body_start: u32) -> Option<u32> {
        let body_start = usize::try_from(body_start).ok()?;
        let body = source.get(body_start..)?;
        let close = body.find("</script")?;
        u32::try_from(body_start + close).ok()
    }

    fn convert_parse_error(source: &str, error: &ParseError) -> SvelteParseError {
        let (start, end) = error.span();
        SvelteParseError {
            code: parse_error_code(error).to_string(),
            message: error.to_string(),
            range: SvelteSourceRange::new(
                source,
                u32::try_from(start).unwrap_or(u32::MAX),
                u32::try_from(end).unwrap_or(u32::MAX),
            ),
        }
    }

    fn convert_analysis_error(source: &str, error: &AnalysisError) -> SvelteParseError {
        let (code, message) = match error {
            AnalysisError::Scope(message) => ("scope_error", message.as_str()),
            AnalysisError::Validation(message) => ("validation_error", message.as_str()),
            AnalysisError::Css(message) => ("css_error", message.as_str()),
            AnalysisError::ValidationWithCode { code, message } => {
                (code.as_str(), message.as_str())
            }
        };

        SvelteParseError {
            code: code.to_string(),
            message: message.to_string(),
            range: SvelteSourceRange::new(
                source,
                0,
                u32::try_from(source.len()).unwrap_or(u32::MAX),
            ),
        }
    }

    fn convert_transform_error(source: &str, error: &TransformError) -> SvelteParseError {
        let code = match error {
            TransformError::CodeGen(_) => "codegen_error",
            TransformError::Css(_) => "css_transform_error",
        };

        SvelteParseError {
            code: code.to_string(),
            message: error.to_string(),
            range: SvelteSourceRange::new(
                source,
                0,
                u32::try_from(source.len()).unwrap_or(u32::MAX),
            ),
        }
    }

    fn parse_error_code(error: &ParseError) -> &str {
        match error {
            ParseError::UnexpectedEof { .. } => "unexpected_eof",
            ParseError::UnexpectedToken { .. } => "unexpected_token",
            ParseError::UnclosedElement { .. } => "unclosed_element",
            ParseError::UnclosedBlock { .. } => "unclosed_block",
            ParseError::InvalidAttribute { .. } => "invalid_attribute",
            ParseError::InvalidExpression { .. } => "invalid_expression",
            ParseError::Generic { .. } => "generic",
            ParseError::SvelteError { code, .. } => code.as_str(),
        }
    }

    fn collect_html_comments(source: &str, fragment: &Fragment, comments: &mut Vec<SvelteComment>) {
        for node in &fragment.nodes {
            collect_html_comments_from_node(source, node, comments);
        }
    }

    fn collect_html_comments_from_node(
        source: &str,
        node: &TemplateNode,
        comments: &mut Vec<SvelteComment>,
    ) {
        match node {
            TemplateNode::Comment(comment) => comments.push(SvelteComment {
                kind: SvelteCommentKind::Html,
                range: SvelteSourceRange::new(source, comment.start, comment.end),
                text: comment.data.to_string(),
            }),
            TemplateNode::IfBlock(block) => {
                collect_html_comments(source, &block.consequent, comments);
                if let Some(alternate) = &block.alternate {
                    collect_html_comments(source, alternate, comments);
                }
            }
            TemplateNode::EachBlock(block) => {
                collect_html_comments(source, &block.body, comments);
                if let Some(fallback) = &block.fallback {
                    collect_html_comments(source, fallback, comments);
                }
            }
            TemplateNode::AwaitBlock(block) => {
                for fragment in [&block.pending, &block.then, &block.catch].into_iter().flatten() {
                    collect_html_comments(source, fragment, comments);
                }
            }
            TemplateNode::KeyBlock(block) => {
                collect_html_comments(source, &block.fragment, comments);
            }
            TemplateNode::SnippetBlock(block) => {
                collect_html_comments(source, &block.body, comments);
            }
            TemplateNode::RegularElement(element) => {
                collect_html_comments(source, &element.fragment, comments);
            }
            TemplateNode::Component(element) => {
                collect_html_comments(source, &element.fragment, comments);
            }
            TemplateNode::TitleElement(element) => {
                collect_html_comments(source, &element.fragment, comments);
            }
            TemplateNode::SlotElement(element) => {
                collect_html_comments(source, &element.fragment, comments);
            }
            TemplateNode::SvelteBody(element)
            | TemplateNode::SvelteDocument(element)
            | TemplateNode::SvelteFragment(element)
            | TemplateNode::SvelteBoundary(element)
            | TemplateNode::SvelteHead(element)
            | TemplateNode::SvelteOptions(element)
            | TemplateNode::SvelteSelf(element)
            | TemplateNode::SvelteWindow(element) => {
                collect_html_comments(source, &element.fragment, comments);
            }
            TemplateNode::SvelteComponent(element) => {
                collect_html_comments(source, &element.fragment, comments);
            }
            TemplateNode::SvelteElement(element) => {
                collect_html_comments(source, &element.fragment, comments);
            }
            TemplateNode::Text(_)
            | TemplateNode::ExpressionTag(_)
            | TemplateNode::HtmlTag(_)
            | TemplateNode::ConstTag(_)
            | TemplateNode::DeclarationTag(_)
            | TemplateNode::DebugTag(_)
            | TemplateNode::RenderTag(_)
            | TemplateNode::AttachTag(_) => {}
        }
    }

    fn source_position(source: &str, offset: u32) -> SvelteSourcePosition {
        let mut line = 1;
        let mut line_start = 0;
        let offset = usize::try_from(offset).unwrap_or(usize::MAX).min(source.len());

        for (index, byte) in source.bytes().enumerate() {
            if index >= offset {
                break;
            }
            if byte == b'\n' {
                line += 1;
                line_start = index + 1;
            }
        }

        SvelteSourcePosition {
            line,
            column: u32::try_from(offset.saturating_sub(line_start)).unwrap_or(u32::MAX),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{
            SvelteCommentKind, SvelteScriptKind, format_svelte, format_svelte_with_options,
            format_svelte_with_options_and_indent, parse_svelte, parse_svelte_for_lint,
            parse_svelte_summary, parse_svelte_syntax,
        };

        #[test]
        fn parses_svelte_component() {
            let summary = parse_svelte_summary(
                r"<script>let count=1;</script>
<button>{count}</button>",
            )
            .expect("Svelte source should parse");

            assert!(summary.has_instance_script);
            assert!(!summary.has_module_script);
            assert!(summary.top_level_node_count > 0);
        }

        #[test]
        fn formats_svelte_script_body() {
            let formatted =
                format_svelte("<script>let count=1+2</script>\n<button>{count}</button>")
                    .expect("Svelte source should format");

            assert!(formatted.contains("let count = 1 + 2;"));
            assert!(formatted.contains("<button>{count}</button>"));
        }

        #[test]
        fn formats_svelte_script_body_with_js_options() {
            let options = rsvelte_formatter::JsFormatOptions {
                indent_width: rsvelte_formatter::IndentWidth::try_from(4).unwrap(),
                ..rsvelte_formatter::JsFormatOptions::default()
            };

            let formatted = format_svelte_with_options(
                "<script>let count=1+2;</script>\n<button>{count}</button>",
                options,
            )
            .expect("Svelte source should format");

            assert!(formatted.contains("    let count = 1 + 2;"));
        }

        #[test]
        fn unindented_svelte_script_retains_full_line_width() {
            let options = rsvelte_formatter::JsFormatOptions {
                indent_width: rsvelte_formatter::IndentWidth::try_from(4).unwrap(),
                line_width: rsvelte_formatter::LineWidth::try_from(100).unwrap(),
                ..rsvelte_formatter::JsFormatOptions::default()
            };
            let source = r"<script>
const metrics = $derived.by(() => {
    return {
        recurring_amount:
            overview.recurring_snapshot?.currencies?.[0]?.committed_monthly_equivalent_display ||
            'No recurring base yet',
    }
})
</script>";

            let formatted = format_svelte_with_options_and_indent(source, options, false)
                .expect("Svelte source should format");

            assert!(formatted.contains(
                "overview.recurring_snapshot?.currencies?.[0]?.committed_monthly_equivalent_display ||"
            ));
        }

        #[test]
        fn formats_svelte_module_syntax() {
            let formatted = format_svelte(
                r#"<script lang="ts">
import { tick } from "svelte";
interface Props { value: number }
let { value }: Props = $props();
</script>
{#snippet render(node: unknown)}
<button onclick={() => tick()}>{node ?? value}</button>
{/snippet}
{@render render(value)}"#,
            )
            .expect("Svelte module syntax should format");

            assert!(formatted.contains("import { tick } from \"svelte\";"));
            assert!(formatted.contains("interface Props"));
            assert!(formatted.contains("{#snippet render(node: unknown)}"));
        }

        #[test]
        fn formatted_svelte_with_components_reparses() {
            #[expect(
                clippy::literal_string_with_formatting_args,
                reason = "the literal is Svelte source, not a format string"
            )]
            let source = r#"<script>let items=[{id:1}]</script>
<svelte:head><title>Example</title><meta name="description" content="test" /></svelte:head>
{#each items as item (item.id)}
<button onclick={()=>item.id++}><Icon value={item.id} /></button>
{/each}"#;

            let formatted = format_svelte_with_options(
                source,
                rsvelte_formatter::JsFormatOptions {
                    semicolons: rsvelte_formatter::Semicolons::AsNeeded,
                    ..rsvelte_formatter::JsFormatOptions::default()
                },
            )
            .expect("Svelte source should format");

            assert!(formatted.contains("<Icon value={item.id} />"));
            assert!(!formatted.contains("__rsvelte_fmt_rhs__"));
            parse_svelte(&formatted).unwrap_or_else(|error| {
                panic!("formatted Svelte source should reparse: {error:?}\n{formatted}")
            });
        }

        #[test]
        fn parse_payload_includes_comments_and_script_ranges() {
            let source = r#"<script context="module">
// module comment
export const answer=42;
</script>
<!-- template comment -->
<script lang="ts">
/* instance comment */
let count:number=1;
</script>
<button>{count}</button>"#;

            let parsed = parse_svelte(source).expect("Svelte source should parse");

            assert_eq!(parsed.scripts.len(), 2);
            assert_eq!(parsed.scripts[0].kind, SvelteScriptKind::Module);
            assert_eq!(parsed.scripts[1].kind, SvelteScriptKind::Instance);
            assert!(parsed.scripts[1].is_typescript);
            assert_eq!(
                &source[parsed.scripts[1].body_range.span],
                "\n/* instance comment */\nlet count:number=1;\n"
            );
            assert!(parsed.comments.iter().any(|comment| {
                comment.kind == SvelteCommentKind::Html && comment.text.trim() == "template comment"
            }));
            assert!(parsed.comments.iter().any(|comment| {
                comment.kind == SvelteCommentKind::JsLine && comment.text.trim() == "module comment"
            }));
            assert!(parsed.comments.iter().any(|comment| {
                comment.kind == SvelteCommentKind::JsBlock
                    && comment.text.trim() == "instance comment"
            }));
        }

        #[test]
        fn lint_payload_includes_cross_section_semantics() {
            let source = r"<script module>
const from_module = 1;
</script>
<script>
import Widget from './Widget.svelte';
import { writable } from 'svelte/store';
const count = writable(0);
const from_instance = from_module;
let state = $state(0);
</script>
<Widget>{from_instance} {$count} {state}</Widget>";

            let (_, semantic) = parse_svelte_for_lint(source).expect("Svelte source should parse");
            let semantic = semantic.expect("Svelte source should analyze");

            for declaration in
                ["from_module =", "Widget from", "count =", "from_instance =", "state ="]
            {
                let start = u32::try_from(source.find(declaration).unwrap()).unwrap();
                assert!(
                    semantic.used_bindings.contains(&start),
                    "expected {declaration} to be used outside its isolated script scope"
                );
            }
            let module_reference = u32::try_from(source.rfind("from_module").unwrap()).unwrap();
            assert!(semantic.resolved_references.iter().any(|span| span.start == module_reference));
            assert!(semantic.implicit_globals.iter().any(|name| name == "$state"));
        }

        #[test]
        fn lint_payload_marks_event_handler_bindings_used() {
            let source = r"<button onclick={handle_click}>Click</button>
<script>
function handle_click() {}
</script>";

            let (_, semantic) = parse_svelte_for_lint(source).expect("Svelte source should parse");
            let semantic = semantic.expect("Svelte source should analyze");
            let declaration = u32::try_from(source.rfind("handle_click").unwrap()).unwrap();

            assert!(semantic.used_bindings.contains(&declaration));
        }

        #[test]
        fn lint_payload_marks_directives_spreads_and_bindables_used() {
            let source = r"<div use:action transition:slide {...rest}></div>
<script>
import { slide } from 'svelte/transition';
function action() {}
let { value = $bindable(), ...rest } = $props();
value = 1;
</script>";

            let (_, semantic) = parse_svelte_for_lint(source).expect("Svelte source should parse");
            let semantic = semantic.expect("Svelte source should analyze");

            for declaration in ["slide }", "action()", "value = $bindable", "rest } = $props"] {
                let start = u32::try_from(source.rfind(declaration).unwrap()).unwrap();
                assert!(
                    semantic.used_bindings.contains(&start),
                    "expected {declaration} to be semantically used"
                );
            }
            for reference in ["action transition", "slide {...rest", "rest}></div>"] {
                let start = u32::try_from(source.find(reference).unwrap()).unwrap();
                assert!(
                    semantic.resolved_references.iter().any(|span| span.start == start),
                    "expected {reference} to have its source reference span"
                );
            }
        }

        #[test]
        fn parse_payload_includes_analysis_warnings() {
            let source = r#"<a href="javascript:void(0)">unsafe</a>"#;
            let parsed = parse_svelte(source).expect("Svelte source should analyze");
            let warning = parsed
                .warnings
                .iter()
                .find(|warning| warning.code == "a11y_invalid_attribute")
                .expect("rsvelte should report the unsafe href");

            assert!(warning.range.span.end <= u32::try_from(source.len()).unwrap());
        }

        #[test]
        fn analysis_errors_include_svelte_code_and_stable_range() {
            let source = r#"<svelte:component foo="bar"/>"#;
            let error = parse_svelte(source).expect_err("Svelte analysis should fail");

            assert_eq!(error.code, "svelte_component_missing_this");
            assert_eq!(
                error.range.span,
                oxc_span::Span::new(0, u32::try_from(source.len()).unwrap())
            );
        }

        #[test]
        fn syntax_parse_does_not_emit_analysis_diagnostics() {
            let source = r#"<svelte:component foo="bar"/>"#;
            let parsed = parse_svelte_syntax(source).expect("Svelte syntax should parse");

            assert!(parsed.warnings.is_empty());
        }

        #[test]
        fn formats_snippet_parameter_with_default() {
            let source = "{#snippet child(label = '')}{label}{/snippet}";
            let formatted = format_svelte(source).expect("Svelte snippet should format");

            parse_svelte_syntax(&formatted).expect("formatted Svelte snippet should reparse");
        }

        #[test]
        fn formats_else_if_blocks_idempotently() {
            let source = r"{#if first}
<p>first</p>
{:else if second}
<p>second</p>
{:else}
<p>other</p>
{/if}";
            let expected = r"{#if first}
  <p>first</p>
{:else if second}
  <p>second</p>
{:else}
  <p>other</p>
{/if}
";

            let formatted = format_svelte(source).expect("Svelte if block should format");

            assert_eq!(formatted, expected);
            assert_eq!(format_svelte(&formatted).unwrap(), formatted);
            parse_svelte_syntax(&formatted).expect("formatted Svelte if block should reparse");
        }

        #[test]
        fn formats_inline_else_block_idempotently() {
            let source = r#"{#if found}<div>Found</div>{:else}<section class="mx-auto max-w-3xl px-4 py-20 text-center"><h1 class="text-3xl font-bold">Stage not found</h1></section>{/if}"#;

            let formatted = format_svelte(source).expect("Svelte if block should format");

            parse_svelte_syntax(&formatted).expect("formatted Svelte if block should reparse");
            assert_eq!(format_svelte(&formatted).unwrap(), formatted);
        }

        #[test]
        fn parse_error_includes_code_and_range() {
            let source = r#"<script context="not-module"></script>"#;
            let error = parse_svelte(source).expect_err("Svelte source should fail to parse");

            assert_eq!(error.code, "script_invalid_context");
            assert!(error.range.span.start <= error.range.span.end);
            assert_eq!(error.range.start.line, 1);
        }
    }
}

#[cfg(feature = "rsvelte")]
pub use rsvelte_backend::{
    SvelteComment, SvelteCommentKind, SvelteFormatOptions, SvelteParseError, SvelteParseResult,
    SvelteParseSummary, SvelteParseWarning, SvelteScript, SvelteScriptKind, SvelteSemanticSummary,
    SvelteSourcePosition, SvelteSourceRange, format_svelte, format_svelte_with_config,
    format_svelte_with_options, format_svelte_with_options_and_indent, parse_svelte,
    parse_svelte_for_lint, parse_svelte_summary, parse_svelte_syntax,
};
