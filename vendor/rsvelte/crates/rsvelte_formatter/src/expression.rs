use oxc_allocator::Allocator;
use oxc_span::SourceType;
use svelte_compiler_rust::ast::js::Expression;
use svelte_compiler_rust::ast::template::{ExpressionTag, Fragment, TemplateNode};

use crate::error::FormatError;
use crate::options::FormatOptions;

/// Walk a `Fragment` recursively, appending `(start, end, replacement)`
/// edits for every JS expression we can safely format.
pub(crate) fn collect_template_edits(
    source: &str,
    fragment: &Fragment,
    options: &FormatOptions,
    edits: &mut Vec<(u32, u32, String)>,
) -> Result<(), FormatError> {
    for node in &fragment.nodes {
        collect_node_edits(source, node, options, edits)?;
    }
    Ok(())
}

fn collect_node_edits(
    source: &str,
    node: &TemplateNode,
    options: &FormatOptions,
    edits: &mut Vec<(u32, u32, String)>,
) -> Result<(), FormatError> {
    match node {
        TemplateNode::ExpressionTag(tag) => {
            push_expression_tag(source, tag, options, edits)?;
        }
        TemplateNode::HtmlTag(tag) => {
            push_tag_form(
                source,
                tag.start,
                tag.end,
                "@html",
                &tag.expression,
                options,
                edits,
            )?;
        }
        TemplateNode::RenderTag(tag) => {
            push_tag_form(
                source,
                tag.start,
                tag.end,
                "@render",
                &tag.expression,
                options,
                edits,
            )?;
        }
        TemplateNode::AttachTag(tag) => {
            push_tag_form(
                source,
                tag.start,
                tag.end,
                "@attach",
                &tag.expression,
                options,
                edits,
            )?;
        }
        TemplateNode::DebugTag(tag) => {
            push_debug_tag(source, tag.start, tag.end, &tag.identifiers, options, edits)?;
        }
        TemplateNode::ConstTag(_) | TemplateNode::DeclarationTag(_) => {
            // Statement-shaped tags (`{@const x = e}`, `{let x = e}`,
            // `{const x = e}`) — defer until statement formatting lands.
        }
        // For every element type, attribute lists (and `this={X}` on
        // `<svelte:component>` / `<svelte:element>`) are owned by the
        // open-tag rewrite in `crate::markup`. Here we only recurse into
        // the children.
        TemplateNode::RegularElement(elem) => {
            collect_template_edits(source, &elem.fragment, options, edits)?;
        }
        TemplateNode::Component(c) => {
            collect_template_edits(source, &c.fragment, options, edits)?;
        }
        TemplateNode::TitleElement(t) => {
            collect_template_edits(source, &t.fragment, options, edits)?;
        }
        TemplateNode::SlotElement(s) => {
            collect_template_edits(source, &s.fragment, options, edits)?;
        }
        TemplateNode::SvelteHead(s)
        | TemplateNode::SvelteBody(s)
        | TemplateNode::SvelteDocument(s)
        | TemplateNode::SvelteFragment(s)
        | TemplateNode::SvelteBoundary(s)
        | TemplateNode::SvelteOptions(s)
        | TemplateNode::SvelteSelf(s)
        | TemplateNode::SvelteWindow(s) => {
            collect_template_edits(source, &s.fragment, options, edits)?;
        }
        TemplateNode::SvelteComponent(c) => {
            collect_template_edits(source, &c.fragment, options, edits)?;
        }
        TemplateNode::SvelteElement(e) => {
            collect_template_edits(source, &e.fragment, options, edits)?;
        }
        TemplateNode::IfBlock(blk) => {
            push_bare_expression(source, &blk.test, options, edits)?;
            collect_template_edits(source, &blk.consequent, options, edits)?;
            if let Some(alt) = &blk.alternate {
                collect_template_edits(source, alt, options, edits)?;
            }
        }
        TemplateNode::EachBlock(blk) => {
            push_bare_expression(source, &blk.expression, options, edits)?;
            if let Some(ctx) = &blk.context {
                push_pattern_at_span(source, ctx, options, edits)?;
            }
            if let Some(key) = &blk.key {
                push_brace_wrapped_expression(source, key, options, edits)?;
            }
            collect_template_edits(source, &blk.body, options, edits)?;
            if let Some(fb) = &blk.fallback {
                collect_template_edits(source, fb, options, edits)?;
            }
        }
        TemplateNode::AwaitBlock(blk) => {
            push_bare_expression(source, &blk.expression, options, edits)?;
            if let Some(v) = &blk.value {
                push_pattern_at_span(source, v, options, edits)?;
            }
            if let Some(e) = &blk.error {
                push_pattern_at_span(source, e, options, edits)?;
            }
            if let Some(frag) = &blk.pending {
                collect_template_edits(source, frag, options, edits)?;
            }
            if let Some(frag) = &blk.then {
                collect_template_edits(source, frag, options, edits)?;
            }
            if let Some(frag) = &blk.catch {
                collect_template_edits(source, frag, options, edits)?;
            }
        }
        TemplateNode::KeyBlock(blk) => {
            push_bare_expression(source, &blk.expression, options, edits)?;
            collect_template_edits(source, &blk.fragment, options, edits)?;
        }
        TemplateNode::SnippetBlock(blk) => {
            push_bare_expression(source, &blk.expression, options, edits)?;
            for p in &blk.parameters {
                push_pattern_at_span(source, p, options, edits)?;
            }
            collect_template_edits(source, &blk.body, options, edits)?;
        }
        TemplateNode::Text(_) | TemplateNode::Comment(_) => {}
    }
    Ok(())
}

// ─── Splice strategies ──────────────────────────────────────────────────

/// Replace `{...}` (template-position or attribute-value `ExpressionTag`)
/// with the formatted expression body wrapped in braces. Collapses any
/// whitespace inside the braces.
fn push_expression_tag(
    source: &str,
    tag: &ExpressionTag,
    options: &FormatOptions,
    edits: &mut Vec<(u32, u32, String)>,
) -> Result<(), FormatError> {
    let outer = source
        .get(tag.start as usize..tag.end as usize)
        .ok_or_else(|| FormatError::Parse("expression tag span out of bounds".into()))?;
    let inner = outer
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or_else(|| FormatError::Parse("expression tag missing braces".into()))?
        .trim();

    if inner.is_empty() {
        return Ok(());
    }

    let formatted = format_expression_source(inner, options)?;
    edits.push((tag.start, tag.end, format!("{{{formatted}}}")));
    Ok(())
}

/// Replace `{@<keyword> EXPR}` (full tag span) with the formatted expression
/// body and a single space after the keyword.
fn push_tag_form(
    source: &str,
    tag_start: u32,
    tag_end: u32,
    keyword: &str,
    expr: &Expression,
    options: &FormatOptions,
    edits: &mut Vec<(u32, u32, String)>,
) -> Result<(), FormatError> {
    let (Some(start), Some(end)) = (expr.start(), expr.end()) else {
        return Ok(());
    };
    let slice = source
        .get(start as usize..end as usize)
        .ok_or_else(|| FormatError::Parse("tag expression span out of bounds".into()))?
        .trim();
    if slice.is_empty() {
        return Ok(());
    }
    let formatted = format_expression_source(slice, options)?;
    edits.push((tag_start, tag_end, format!("{{{keyword} {formatted}}}")));
    Ok(())
}

/// Replace `{@debug a, b, c}` with each identifier formatted, joined by
/// a comma + single space.
fn push_debug_tag(
    source: &str,
    tag_start: u32,
    tag_end: u32,
    identifiers: &[Expression],
    options: &FormatOptions,
    edits: &mut Vec<(u32, u32, String)>,
) -> Result<(), FormatError> {
    let mut parts = Vec::with_capacity(identifiers.len());
    for id in identifiers {
        let (Some(start), Some(end)) = (id.start(), id.end()) else {
            continue;
        };
        let slice = source
            .get(start as usize..end as usize)
            .ok_or_else(|| FormatError::Parse("debug identifier span out of bounds".into()))?
            .trim();
        if slice.is_empty() {
            continue;
        }
        parts.push(format_expression_source(slice, options)?);
    }
    if parts.is_empty() {
        return Ok(());
    }
    let joined = parts.join(", ");
    edits.push((tag_start, tag_end, format!("{{@debug {joined}}}")));
    Ok(())
}

/// Splice over an expression's enclosing `{ ... }` if the source has
/// `{ <ws> EXPR <ws> }` around the AST expression span (directive values,
/// `this={X}` etc.). If no such enclosure is detectable, splice just the
/// bare expression span (block headers like `{#if EXPR}` fall into this
/// branch — the `{#if ` keyword stops the back-scan).
fn push_brace_wrapped_expression(
    source: &str,
    expr: &Expression,
    options: &FormatOptions,
    edits: &mut Vec<(u32, u32, String)>,
) -> Result<(), FormatError> {
    let (Some(start), Some(end)) = (expr.start(), expr.end()) else {
        return Ok(());
    };
    let slice = source
        .get(start as usize..end as usize)
        .ok_or_else(|| FormatError::Parse("expression span out of bounds".into()))?
        .trim();
    if slice.is_empty() {
        return Ok(());
    }
    let formatted = format_expression_source(slice, options)?;

    if let Some((brace_start, brace_end)) = enclosing_braces_span(source, start, end) {
        edits.push((brace_start, brace_end, format!("{{{formatted}}}")));
    } else {
        edits.push((start, end, formatted));
    }
    Ok(())
}

/// Splice just the bare expression span — preserves whatever surrounds it
/// in the source. Used for block-header expressions (`{#if EXPR}`,
/// `{#each EXPR as ...}`, etc.) where the `{` is followed by a Svelte
/// keyword (`#if` / `#each` / ...) rather than the expression itself.
fn push_bare_expression(
    source: &str,
    expr: &Expression,
    options: &FormatOptions,
    edits: &mut Vec<(u32, u32, String)>,
) -> Result<(), FormatError> {
    let (Some(start), Some(end)) = (expr.start(), expr.end()) else {
        return Ok(());
    };
    let slice = source
        .get(start as usize..end as usize)
        .ok_or_else(|| FormatError::Parse("expression span out of bounds".into()))?
        .trim();
    if slice.is_empty() {
        return Ok(());
    }
    let formatted = format_expression_source(slice, options)?;
    edits.push((start, end, formatted));
    Ok(())
}

/// If the AST expression at `[expr_start, expr_end)` is enclosed by `{`
/// and `}` (only whitespace between brace and expression), return the
/// span covering the braces inclusive. Otherwise return `None`.
fn enclosing_braces_span(source: &str, expr_start: u32, expr_end: u32) -> Option<(u32, u32)> {
    let bytes = source.as_bytes();

    let mut lbrace = None;
    let mut i = expr_start as usize;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' => continue,
            b'{' => {
                lbrace = Some(i);
                break;
            }
            _ => return None,
        }
    }
    let lbrace = lbrace?;

    let mut rbrace = None;
    let mut j = expr_end as usize;
    while j < bytes.len() {
        match bytes[j] {
            b' ' | b'\t' | b'\n' | b'\r' => j += 1,
            b'}' => {
                rbrace = Some(j);
                break;
            }
            _ => return None,
        }
    }
    let rbrace = rbrace?;

    Some((lbrace as u32, (rbrace + 1) as u32))
}

// ─── Expression formatter ───────────────────────────────────────────────

/// Format a single JS expression source. Wraps in parens to force
/// expression context (so object literals like `{a:1}` aren't parsed as
/// block statements) and strips the wrapper from the output.
pub(crate) fn format_expression_source(
    expr_source: &str,
    options: &FormatOptions,
) -> Result<String, FormatError> {
    let allocator = Allocator::default();
    let source_type = SourceType::ts().with_module(true);

    let wrapped = format!("({expr_source});");
    let formatted = oxc_formatter::format(
        &allocator,
        &wrapped,
        source_type,
        options.js.clone(),
        None,
    )
    .map_err(|err| FormatError::ScriptParse(format!("{err:?}")))?
    .print()
    .map_err(|err| FormatError::ScriptParse(format!("{err:?}")))?
    .into_code();

    let s = formatted.trim_end().trim_end_matches(';').trim_end();
    let s = s.strip_prefix(';').unwrap_or(s).trim_start();
    Ok(strip_outer_parens(s).trim().to_string())
}

/// Splice a destructuring pattern's source span with its formatted
/// version. Mirrors `push_bare_expression` but routes through
/// `format_pattern_source` so default values and rest elements survive.
fn push_pattern_at_span(
    source: &str,
    expr: &Expression,
    options: &FormatOptions,
    edits: &mut Vec<(u32, u32, String)>,
) -> Result<(), FormatError> {
    let (Some(start), Some(end)) = (expr.start(), expr.end()) else {
        return Ok(());
    };
    let slice = source
        .get(start as usize..end as usize)
        .ok_or_else(|| FormatError::Parse("pattern span out of bounds".into()))?
        .trim();
    if slice.is_empty() {
        return Ok(());
    }
    let formatted = format_pattern_source(slice, options)?;
    edits.push((start, end, formatted));
    Ok(())
}

/// Format a destructuring pattern. Patterns like `{a, b = 1}`,
/// `[a, ...rest]`, or `{ a: { b } }` aren't valid as bare expressions,
/// so we wrap them as a function parameter and parse the whole program.
/// Function parameters also support defaults used by Svelte snippets.
///
/// We force `line_width` to its maximum so nested patterns stay on one
/// line — multi-line patterns inside `{#each as ...}` would land
/// across the block header, which Svelte's parser then can't re-read.
pub(crate) fn format_pattern_source(
    pattern_source: &str,
    options: &FormatOptions,
) -> Result<String, FormatError> {
    const FUNCTION_NAME: &str = "__rsvelte_fmt_pattern__";
    let allocator = Allocator::default();
    let source_type = SourceType::ts().with_module(true);

    let wrapped = format!("function {FUNCTION_NAME}({pattern_source}) {{}}");

    // Build a single-line copy of JsFormatOptions for this pattern only.
    // Setting `expand = Never` plus a very wide `line_width` keeps even
    // deeply nested destructuring on one line — the multi-line form
    // would land across a Svelte block header (`{#each as ...}`) and
    // re-parse incorrectly.
    let mut single_line = options.js.clone();
    single_line.line_width =
        oxc_formatter_core::LineWidth::try_from(u16::MAX).unwrap_or(single_line.line_width);
    single_line.expand = oxc_formatter::Expand::Never;

    let formatted =
        oxc_formatter::format(&allocator, &wrapped, source_type, single_line, None)
            .map_err(|err| FormatError::ScriptParse(format!("{err:?}")))?
            .print()
            .map_err(|err| FormatError::ScriptParse(format!("{err:?}")))?
            .into_code();

    // Output shape: `function __rsvelte_fmt_pattern__(<pattern>) {}`.
    let s = formatted.trim_end();
    let prefix = format!("function {FUNCTION_NAME}(");
    let pattern = s
        .strip_prefix(&prefix)
        .and_then(|body| body.strip_suffix(") {}"))
        .unwrap_or(pattern_source);
    let candidate = pattern.trim().to_string();

    // Some patterns (deeply nested destructuring) get hard-wrapped by
    // oxc_formatter regardless of `line_width` / `expand`. A multi-line
    // pattern can't safely sit inside a Svelte block header
    // (`{#each as ...}` re-reads the header as a single line), so
    // fall back to a light source-normalization for those.
    if candidate.contains('\n') {
        return Ok(light_normalize_pattern(pattern_source));
    }
    Ok(candidate)
}

/// Conservative whitespace-only normalization used when the JS formatter
/// produces a multi-line pattern.
///
/// Mirrors Prettier's destructuring spacing rules: braces (`{` / `}`)
/// always carry one inner space when non-empty; brackets (`[` / `]`)
/// and parens carry none; commas and colons are followed by exactly
/// one space.
fn light_normalize_pattern(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b' ' | b'\t' | b'\n' | b'\r' => {
                // Drop existing whitespace; the rules below re-insert it.
            }
            b'{' => {
                out.push('{');
                // Peek past whitespace to see whether the brace is empty.
                let mut j = i + 1;
                while j < bytes.len() && matches!(bytes[j], b' ' | b'\t' | b'\n' | b'\r') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] != b'}' {
                    out.push(' ');
                }
            }
            b'}' => {
                if !out.ends_with('{') && !out.ends_with(' ') {
                    out.push(' ');
                }
                out.push('}');
            }
            b',' | b':' => {
                if out.ends_with(' ') {
                    out.pop();
                }
                out.push(b as char);
                // Lookahead for next non-whitespace.
                let mut j = i + 1;
                while j < bytes.len() && matches!(bytes[j], b' ' | b'\t' | b'\n' | b'\r') {
                    j += 1;
                }
                if j < bytes.len() && !matches!(bytes[j], b'}' | b']' | b')') {
                    out.push(' ');
                }
            }
            other => out.push(other as char),
        }
        i += 1;
    }
    out
}

fn strip_outer_parens(s: &str) -> &str {
    let trimmed = s.trim();
    let Some(inner) = trimmed.strip_prefix('(').and_then(|s| s.strip_suffix(')')) else {
        return s;
    };
    if outer_parens_match(inner) { inner } else { s }
}

fn outer_parens_match(inner: &str) -> bool {
    let mut depth: i32 = 0;
    for c in inner.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}
