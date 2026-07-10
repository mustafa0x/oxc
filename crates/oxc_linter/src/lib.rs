#![expect(clippy::self_named_module_files)]
// for rules.rs
// RuleEnum contains rule configs with interior mutability (e.g. Regex),
// but Hash/Eq/Ord are based only on the rule id, so it's safe as a map key.
#![expect(clippy::mutable_key_type)]
// Rule::from_configuration returns Result but documenting errors is not useful here.
#![expect(clippy::missing_errors_doc)]

use std::{
    iter,
    path::Path,
    ptr::{self, NonNull},
    rc::Rc,
    string::ToString,
};

use oxc_allocator::{Allocator, AllocatorPool, ArenaVec, CloneIn, TakeIn};
use oxc_ast::{
    ast::{Comment, CommentContent, CommentKind, Program},
    ast_kind::AST_TYPE_MAX,
};
use oxc_ast_macros::ast;
use oxc_ast_visit::utf8_to_utf16::Utf8ToUtf16;
use oxc_data_structures::box_macros::boxed_array;
use oxc_diagnostics::OxcDiagnostic;
use oxc_estree_tokens::update_tokens_as_js;
use oxc_parser::Token;
use oxc_semantic::{AstNode, Semantic};
use oxc_span::Span;

mod ast_util;
mod config;
mod context;
mod disable_directives;
mod external_linter;
mod external_plugin_store;
mod fixer;
mod frameworks;
mod globals;
mod module_graph_visitor;
mod module_record;
mod options;
mod rule;
mod service;
mod suppression;
pub(crate) mod timing;
mod tsgolint;
mod utils;

pub mod loader;
pub mod rules;
pub mod table;

mod generated {
    #[cfg(debug_assertions)]
    mod assert_layouts;
    mod rule_runner_impls;
    pub mod rules_enum;
}

#[cfg(test)]
mod tester;

mod lint_runner;

pub use crate::config::{normalize_rule_name, plugins::normalize_plugin_name};
pub use crate::disable_directives::{
    DirectivePrefix, DisableDirectives, DisableRuleComment, RuleCommentRule, RuleCommentType,
    create_unused_directives_diagnostics,
};
pub use crate::{
    config::{
        Config, ConfigBuilderError, ConfigStore, ConfigStoreBuilder, ESLintRule, LintIgnoreMatcher,
        LintPlugins, Oxlintrc, OxlintrcExtendsEntry, ResolvedLinterState,
    },
    context::{ContextSubHost, ContextSubHostOptions, LintContext},
    external_linter::{
        ExternalComment, ExternalCommentKind, ExternalLinter, ExternalLinterCreateWorkspaceCb,
        ExternalLinterDestroyWorkspaceCb, ExternalLinterLintFileCb, ExternalLinterLoadPluginCb,
        ExternalLinterSetupRuleConfigsCb, JsFix, LintFileParseError, LintFilePayload,
        LintFileResult, LintFileSuccessPayload, LoadPluginResult, convert_and_merge_js_fixes,
    },
    external_plugin_store::{ExternalOptionsId, ExternalPluginStore, ExternalRuleId},
    fixer::{Fix, FixKind, Fixer, Message, PossibleFixes, oxc_code_short_canonical_name},
    frameworks::FrameworkFlags,
    lint_runner::{DirectivesStore, LintRunner, LintRunnerBuilder},
    loader::LINTABLE_EXTENSIONS,
    module_record::ModuleRecord,
    options::LintOptions,
    options::{AllowWarnDeny, InvalidFilterKind, LintFilter, LintFilterKind},
    rule::{RuleCategory, RuleFixMeta, RuleMeta, RuleRunFunctionsImplemented, RuleRunner},
    service::{LintService, LintServiceOptions, OsFileSystem, RuntimeFileSystem},
    suppression::{OxlintSuppressionFileAction, SuppressionManager},
    timing::{RuleTimingRecord, RuleTimingSource, RuleTimingStore},
    tsgolint::TsGoLintState,
    utils::{read_to_arena_str, read_to_string},
};
use crate::{
    config::{LintConfig, OxlintEnv, OxlintGlobals, OxlintSettings},
    context::ContextHost,
    disable_directives::RawDirectiveComment,
    external_linter::GlobalsAndEnvs,
    fixer::CompositeFix,
    loader::LINT_PARTIAL_LOADER_EXTENSIONS,
    rules::RuleEnum,
    timing::{RuleTimingRecorder, RuleTimingStat},
    utils::iter_possible_jest_call_node,
};

#[cfg(target_pointer_width = "64")]
#[test]
fn size_asserts() {
    // `RuleEnum` runs in a really tight loop, make sure it is small for CPU cache.
    // A reduction from 168 bytes to 16 results 15% performance improvement.
    // See codspeed in https://github.com/oxc-project/oxc/pull/1783
    assert_eq!(size_of::<RuleEnum>(), 16);
}

#[inline]
fn get_timing_stat<const TIMINGS: bool>(
    timing_stats: &mut Option<Vec<RuleTimingStat>>,
    rule_index: usize,
) -> Option<&mut RuleTimingStat> {
    if TIMINGS {
        Some(&mut timing_stats.as_mut().expect("missing rule timing stats")[rule_index])
    } else {
        None
    }
}

#[cfg(debug_assertions)]
fn cmp_diagnostics_for_runtime_optimization_assertion(
    left: &Message,
    right: &Message,
) -> std::cmp::Ordering {
    left.error
        .labels
        .iter()
        .map(|label| (label.offset(), label.len(), label.primary()))
        .cmp(right.error.labels.iter().map(|label| (label.offset(), label.len(), label.primary())))
        .then_with(|| left.error.message.cmp(&right.error.message))
        .then_with(|| left.error.help.cmp(&right.error.help))
        .then_with(|| left.error.note.cmp(&right.error.note))
        .then_with(|| left.error.severity.cmp(&right.error.severity))
        .then_with(|| left.error.code.cmp(&right.error.code))
        .then_with(|| left.error.url.cmp(&right.error.url))
        .then_with(|| left.span.cmp(&right.span))
        .then_with(|| left.fixes.cmp_fix_sequence(&right.fixes))
}

/// Per-thread scratch buffers for dispatching rules to AST nodes by node type.
///
/// Reused across files, so the single traversal in [`execute_rules`] — which visits each node once
/// and dispatches it only to the rules registered for its type — incurs no per-file allocation.
/// (Per-file allocation is what previously made bucketing worthwhile only for very large files.)
struct RuleBuckets {
    /// `by_type[ast_type]` = indices, into the per-file `rules` slice, of rules that run on that AST
    /// node type. A boxed fixed-size array so indexing by an `AstType` elides bounds checks.
    by_type: Box<[Vec<usize>; AST_TYPE_MAX as usize + 1]>,
    /// Indices of rules that run on every node (rules without `types_info`).
    any_type: Vec<usize>,
}

impl RuleBuckets {
    fn clear(&mut self) {
        for bucket in self.by_type.iter_mut() {
            bucket.clear();
        }
        self.any_type.clear();
    }
}

thread_local! {
    static RULE_BUCKETS: std::cell::RefCell<RuleBuckets> = std::cell::RefCell::new(RuleBuckets {
        by_type: boxed_array![Vec::new(); AST_TYPE_MAX as usize + 1],
        any_type: Vec::new(),
    });
}

fn execute_rules<'a, const TIMINGS: bool>(
    rules: &[(&RuleEnum, LintContext<'a>)],
    semantic: &Semantic<'a>,
    should_run_on_jest_node: bool,
    with_runtime_optimization: bool,
    mut timing_recorder: Option<&mut RuleTimingRecorder>,
) {
    let mut timing_stats = TIMINGS.then(|| vec![RuleTimingStat::default(); rules.len()]);

    if with_runtime_optimization {
        // Bucket rules by the AST node types they care about into a reused per-thread buffer, then
        // make a single pass over the AST, dispatching each node only to the rules registered for
        // its type. This replaces "every rule tests every node" (which dominated dispatch cost in
        // profiles), and because the buffer is reused there is no per-file allocation — so this is
        // a win for files of all sizes, not just large ones.
        RULE_BUCKETS.with_borrow_mut(|buckets| {
            buckets.clear();

            for (rule_index, (rule, ctx)) in rules.iter().enumerate() {
                let run_info = rule.run_info();
                if let Some(ast_types) = rule.types_info()
                    && run_info.is_run_implemented()
                {
                    for ty in ast_types {
                        buckets.by_type[ty as usize].push(rule_index);
                    }
                } else if run_info.is_run_implemented() {
                    buckets.any_type.push(rule_index);
                }

                if run_info.is_run_once_implemented() {
                    let timing_stat = get_timing_stat::<TIMINGS>(&mut timing_stats, rule_index);
                    rule.run_once::<TIMINGS>(ctx, timing_stat);
                }
            }

            for node in semantic.nodes() {
                for &rule_index in &buckets.by_type[node.kind().ty() as usize] {
                    let (rule, ctx) = &rules[rule_index];
                    let timing_stat = get_timing_stat::<TIMINGS>(&mut timing_stats, rule_index);
                    rule.run::<TIMINGS>(node, ctx, timing_stat);
                }
                for &rule_index in &buckets.any_type {
                    let (rule, ctx) = &rules[rule_index];
                    let timing_stat = get_timing_stat::<TIMINGS>(&mut timing_stats, rule_index);
                    rule.run::<TIMINGS>(node, ctx, timing_stat);
                }
            }

            if should_run_on_jest_node {
                for jest_node in iter_possible_jest_call_node(semantic) {
                    for (rule_index, (rule, ctx)) in rules.iter().enumerate() {
                        if rule.run_info().is_run_on_jest_node_implemented() {
                            let timing_stat =
                                get_timing_stat::<TIMINGS>(&mut timing_stats, rule_index);
                            rule.run_on_jest_node::<TIMINGS>(&jest_node, ctx, timing_stat);
                        }
                    }
                }
            }
        });
    } else {
        // Unoptimized reference path: every rule runs on every node, with no type filtering. Used
        // only in debug builds, to assert the optimized path produces identical diagnostics.
        for (rule_index, (rule, ctx)) in rules.iter().enumerate() {
            let timing_stat = get_timing_stat::<TIMINGS>(&mut timing_stats, rule_index);
            rule.run_once::<TIMINGS>(ctx, timing_stat);

            for node in semantic.nodes() {
                let timing_stat = get_timing_stat::<TIMINGS>(&mut timing_stats, rule_index);
                rule.run::<TIMINGS>(node, ctx, timing_stat);
            }

            if should_run_on_jest_node {
                for jest_node in iter_possible_jest_call_node(semantic) {
                    let timing_stat = get_timing_stat::<TIMINGS>(&mut timing_stats, rule_index);
                    rule.run_on_jest_node::<TIMINGS>(&jest_node, ctx, timing_stat);
                }
            }
        }
    }

    if TIMINGS {
        let timing_recorder = timing_recorder.as_mut().expect("missing rule timing recorder");
        for ((rule, _), stat) in rules.iter().zip(timing_stats.expect("missing rule timing stats"))
        {
            timing_recorder.record_native(rule.plugin_name(), rule.name(), stat);
        }
    }
}

/// Base URL for the documentation, used to generate rule documentation URLs when a diagnostic is reported.
const WEBSITE_BASE_RULES_URL: &str = "https://oxc.rs/docs/guide/usage/linter/rules";

#[inline]
fn uses_native_svelte_backend(path: &Path) -> bool {
    #[cfg(feature = "svelte-rsvelte-backend")]
    {
        path.extension().and_then(std::ffi::OsStr::to_str) == Some("svelte")
    }

    #[cfg(not(feature = "svelte-rsvelte-backend"))]
    {
        let _ = path;
        false
    }
}

#[derive(Debug)]
#[expect(clippy::struct_field_names)]
pub struct Linter {
    options: LintOptions,
    config: ConfigStore,
    external_linter: Option<ExternalLinter>,
    workspace_uri: Option<Box<str>>,
}

impl Linter {
    pub fn new(
        options: LintOptions,
        config: ConfigStore,
        external_linter: Option<ExternalLinter>,
    ) -> Self {
        Self { options, config, external_linter, workspace_uri: None }
    }

    #[must_use]
    pub fn with_workspace_uri(mut self, workspace_uri: Option<&str>) -> Self {
        self.workspace_uri = workspace_uri.map(Box::from);
        self
    }

    /// Set the kind of auto fixes to apply.
    #[must_use]
    pub fn with_fix(mut self, kind: FixKind) -> Self {
        self.options.fix = kind;
        self
    }

    #[must_use]
    pub fn with_report_unused_directives(mut self, report_config: Option<AllowWarnDeny>) -> Self {
        self.options.report_unused_directive = report_config;
        self
    }

    pub(crate) fn options(&self) -> &LintOptions {
        &self.options
    }

    pub(crate) fn respect_eslint_disable_directives(&self) -> bool {
        self.config.respect_eslint_disable_directives()
    }

    /// Returns the number of rules that will are being used, unless there
    /// nested configurations in use, in which case it returns `None` since the
    /// number of rules depends on which file is being linted.
    pub fn number_of_rules(&self, type_aware: bool) -> Option<usize> {
        self.config.number_of_rules(type_aware)
    }

    /// Return `true` if `Linter` has an external linter (JS plugins).
    pub fn has_external_linter(&self) -> bool {
        self.external_linter.is_some()
    }

    /// # Panics
    /// Panics if running in debug mode and the number of diagnostics does not match when running with/without optimizations
    pub fn run<'a>(
        &self,
        path: &Path,
        context_sub_hosts: Vec<ContextSubHost<'a>>,
        allocator: &'a Allocator,
    ) -> Vec<Message> {
        self.run_with_disable_directives::<false>(
            path,
            context_sub_hosts,
            allocator,
            None,
            None,
            None,
        )
        .0
    }

    /// Run only external JS-plugin rules against whole-file source text.
    ///
    /// This is used for files that select a custom parser through JS `languageOptions`, but do not
    /// produce any native script sections for the Rust linter to analyze. Native `.svelte` files
    /// are excluded because rsvelte owns their complete source.
    pub fn run_external_only_on_source_text<'a>(
        &self,
        path: &Path,
        source_text: &'a str,
        allocator: &'a Allocator,
    ) -> Vec<Message> {
        if uses_native_svelte_backend(path) {
            return Vec::new();
        }

        let ResolvedLinterState { config, external_rules, .. } = self.config.resolve(path);
        if external_rules.is_empty()
            || !config.js_has_custom_parser
            || self.external_linter.is_none()
        {
            return Vec::new();
        }

        let ctx_host = ContextHost::new_external_only(path, allocator, self.options, config);
        let external_disable_directives = self.run_external_rules_on_source_text(
            &external_rules,
            path,
            &ctx_host,
            source_text,
            allocator,
        );

        if let Some(severity) = self.options.report_unused_directive
            && severity.is_warn_deny()
            && let Some(directives) = external_disable_directives.as_ref()
        {
            Self::report_unused_full_file_disable_directives(
                &ctx_host,
                directives,
                source_text,
                severity.into(),
            );
        }

        ctx_host.take_diagnostics()
    }

    /// Same as `run` but also returns the disable directives for the file
    ///
    /// # Parameters
    /// - `js_allocator_pool`: Optional pool of fixed-size allocators for copying AST before JS transfer.
    ///   When `Some`, the AST will be copied into a fixed-size allocator before passing to JS plugins,
    ///   allowing the main allocator to be a standard (non-fixed-size) allocator.
    ///
    /// # Panics
    /// Panics in debug mode if running with and without optimizations produces different diagnostic counts.
    pub fn run_with_disable_directives<'a, const TIMINGS: bool>(
        &self,
        path: &Path,
        context_sub_hosts: Vec<ContextSubHost<'a>>,
        allocator: &'a Allocator,
        js_allocator_pool: Option<&AllocatorPool>,
        rule_timing_store: Option<&RuleTimingStore>,
        full_source_text: Option<&'a str>,
    ) -> (Vec<Message>, Option<DisableDirectives>) {
        let ResolvedLinterState { rules, config, external_rules: resolved_external_rules } =
            self.config.resolve(path);
        let mut timing_recorder = TIMINGS.then(|| RuleTimingRecorder::with_capacity(rules.len()));

        let mut ctx_host =
            Rc::new(ContextHost::new(path, context_sub_hosts, allocator, self.options, config));

        // The native backend owns the complete `.svelte` file. External JS rules would either
        // invoke the configured whole-file parser or receive an incomplete embedded-script AST.
        let external_rules =
            if uses_native_svelte_backend(path) { &[] } else { resolved_external_rules.as_ref() };

        let use_external_source_parser = full_source_text.is_some()
            && ctx_host.js_has_custom_parser()
            && !external_rules.is_empty();

        #[cfg(debug_assertions)]
        let mut current_diagnostic_index = 0;

        let is_partial_loader_file = ctx_host
            .file_extension()
            .is_some_and(|ext| LINT_PARTIAL_LOADER_EXTENSIONS.iter().any(|e| e == &ext));

        // Linting is done in 3 passes over the sub hosts.
        //
        // 1. Native rules
        // 2. JS plugin rules
        // 3. Unused directives
        //
        // Each pass must be complete before the next can start:
        //
        // `run_external_rules` (JS plugins) destroys the AST of the sub host it runs on - it clears the AST references
        // from that sub host's `Semantic`, and converts that AST's spans to UTF-16 in place.
        // Some native rules read the ASTs of *other* sub hosts via `ContextHost::other_file_hosts`
        // (e.g. `vue/valid-define-emits` checks the `<script>` block for `export default { emits }`
        // while linting the `<script setup>` block), so no AST can be destroyed or mutated until pass 1 is complete.
        //
        // A directive counts as used if it suppressed a diagnostic from either a native rule or a JS plugin rule,
        // so this has to come after both passes above.

        // Pass 1: Run native rules on every sub host in turn
        loop {
            let semantic = ctx_host.semantic();
            let rules = rules
                .iter()
                .filter(|(rule, _)| {
                    if rule.is_tsgolint_rule() {
                        return false;
                    }

                    // If only the `run` function is implemented, we can skip running the file entirely if the current
                    // file does not contain any of the relevant AST node types.
                    if rule.run_info() == RuleRunFunctionsImplemented::Run
                        && let Some(ast_types) = rule.types_info()
                        && !semantic.nodes().contains_any(ast_types)
                    {
                        return false;
                    }

                    rule.should_run(&ctx_host)
                })
                .map(|(rule, severity)| (rule, Rc::clone(&ctx_host).spawn(rule, *severity)))
                .collect::<Vec<_>>();

            let should_run_on_jest_node =
                ctx_host.plugins().has_test() && ctx_host.frameworks().is_test();

            execute_rules::<TIMINGS>(
                &rules,
                semantic,
                should_run_on_jest_node,
                true,
                timing_recorder.as_mut(),
            );

            #[cfg(debug_assertions)]
            {
                let diagnostics_after_optimized = ctx_host.diagnostic_count();
                execute_rules::<false>(&rules, semantic, should_run_on_jest_node, false, None);
                let diagnostics_after_unoptimized = ctx_host.diagnostic_count();
                ctx_host.get_diagnostics(|diagnostics| {
                    let optimized_diagnostics = &diagnostics[current_diagnostic_index..diagnostics_after_optimized];
                    let unoptimized_diagnostics = &diagnostics[diagnostics_after_optimized..diagnostics_after_unoptimized];

                    // Check that we have the same number of diagnostics
                    assert_eq!(
                        optimized_diagnostics.len(),
                        unoptimized_diagnostics.len(),
                        "Running with and without optimizations produced different diagnostic counts: {} vs {}.\nThis can be caused by a mismatch between the rule definition and generated RuleRunner impl. Try `cargo run -p oxc_linter_codegen` to regenerate.",
                        optimized_diagnostics.len(),
                        unoptimized_diagnostics.len()
                    );

                    let mut sorted_optimized = optimized_diagnostics.iter().collect::<Vec<_>>();
                    let mut sorted_unoptimized = unoptimized_diagnostics.iter().collect::<Vec<_>>();

                    sorted_optimized
                        .sort_unstable_by(|left, right| {
                            cmp_diagnostics_for_runtime_optimization_assertion(left, right)
                        });
                    sorted_unoptimized
                        .sort_unstable_by(|left, right| {
                            cmp_diagnostics_for_runtime_optimization_assertion(left, right)
                        });

                    for (opt_diag, unopt_diag) in
                        sorted_optimized.iter().zip(sorted_unoptimized.iter())
                    {
                        assert_eq!(
                            *opt_diag,
                            *unopt_diag,
                            "Diagnostic differs between optimized and unoptimized runs",
                        );
                    }

                    diagnostics.truncate(current_diagnostic_index + optimized_diagnostics.len());
                });
            }

            // If no next `<script>` block found, the complete file is finished linting
            if !ctx_host.next_sub_host() {
                break;
            }

            #[cfg(debug_assertions)]
            {
                current_diagnostic_index = ctx_host.diagnostic_count();
            }
        }

        // Pass 2: Run JS plugin rules on every sub host in turn
        ctx_host.rewind_sub_hosts();
        loop {
            self.run_external_rules(
                &external_rules,
                path,
                &mut ctx_host,
                allocator,
                js_allocator_pool,
            );

            if !ctx_host.next_sub_host() {
                break;
            }
        }

        // Pass 3: Report unused enable/disable directives for every sub host.
        // Reporting unused directives is handled differently with type-aware linting,
        // so this only applies to partial loader files (Vue/Astro/Svelte).
        if let Some(severity) = self.options.report_unused_directive
            && severity.is_warn_deny()
            && is_partial_loader_file
        {
            ctx_host.rewind_sub_hosts();
            loop {
                ctx_host.report_unused_directives(severity.into());

                if !ctx_host.next_sub_host() {
                    break;
                }
            }
        }

        let external_disable_directives = if use_external_source_parser {
            full_source_text.and_then(|source_text| {
                let directives = self.run_external_rules_on_source_text(
                    external_rules,
                    path,
                    &ctx_host,
                    source_text,
                    allocator,
                );

                if let Some(severity) = self.options.report_unused_directive
                    && severity.is_warn_deny()
                    && is_partial_loader_file
                    && let Some(directives_ref) = directives.as_ref()
                {
                    Self::report_unused_full_file_disable_directives(
                        &ctx_host,
                        directives_ref,
                        source_text,
                        severity.into(),
                    );
                }

                directives
            })
        } else {
            None
        };

        let diagnostics = ctx_host.take_diagnostics();
        let disable_directives = if is_partial_loader_file {
            None
        } else {
            let ctx_disable_directives =
                Rc::try_unwrap(ctx_host).unwrap().into_disable_directives();
            external_disable_directives.or(ctx_disable_directives)
        };

        let result = (diagnostics, disable_directives);
        if TIMINGS {
            let timing_recorder = timing_recorder.expect("missing rule timing recorder");
            rule_timing_store.expect("missing rule timing store").merge(
                timing_recorder.into_timings().into_iter().map(|(key, stat)| RuleTimingRecord {
                    source: key.source,
                    plugin_name: key.plugin_name.into_owned(),
                    rule_name: key.rule_name.into_owned(),
                    duration: stat.duration,
                    calls: stat.calls,
                }),
            );
        }
        result
    }

    #[cfg(all(target_pointer_width = "64", target_endian = "little"))]
    fn run_external_rules<'a>(
        &self,
        external_rules: &[(ExternalRuleId, ExternalOptionsId, AllowWarnDeny)],
        path: &Path,
        ctx_host: &mut Rc<ContextHost<'a>>,
        allocator: &'a Allocator,
        js_allocator_pool: Option<&AllocatorPool>,
    ) {
        if external_rules.is_empty() {
            return;
        }

        let ctx_host = Rc::get_mut(ctx_host).unwrap();

        // We need a `&mut Program` here, but `Semantic` only contains a `&Program`, along with `&` references
        // to all other nodes in the AST.
        //
        // Use a very dodgy hack to achieve this.
        //
        // 1. Get an immutable reference to `&Program` from the `AstNodes` contained in `Semantic`.
        // 2. Call `Semantic::clear_ast_references` to discard all references it holds to AST nodes and comments.
        // 3. Make a bitwise copy of the `Program`.
        // 4. Allocate it back into arena, yielding a `&mut Program`.
        //
        // Within this function, this is sound.
        // The dangerous part is copying the `ArenaVec`s (`program.body` etc), but `ArenaVec` only contains a pointer
        // which has no ownership semantics, and they cannot contain `Drop` types. So as long as we don't hold
        // references to any of the *contents* of the original `ArenaVec`, and don't use it after its been copied,
        // we can't violate aliasing rules.
        //
        // Here we create the `&Program` in a block from which it doesn't escape, ensuring the reference doesn't live
        // while the copy lives, and we empty all AST node/comment references from `Semantic`, ensuring there's no way
        // to access references to AST nodes after this point, which could alias the `&mut Program` we've created.
        //
        // What we CAN'T protect against is if some other references to AST nodes have been already stashed somewhere,
        // and that would be UB. At present, we don't do that, but there is nothing statically preventing that,
        // so it'd be easy for someone add code which inadvertently causes UB, without any idea they were doing that.
        //
        // TODO: We need to fix this. We should avoid the need for a mutable AST here by converting AST node spans
        // to UTF-16 during deserialization on JS side - but that is not easy to achieve without a heavy perf penalty.
        // This current hack *is* unsound, but is unlikely to bite in practice, so we can live with it for now.
        let program = {
            let semantic = ctx_host.semantic_mut();
            let program = semantic.nodes().program();
            semantic.clear_ast_references();
            // SAFETY: Only somewhat safe! See above.
            let program = unsafe { NonNull::from(program).read() };
            allocator.alloc(program)
        };

        // If `js_allocator_pool` is provided, use clone-into-fixed-allocator approach
        if let Some(js_allocator_pool) = js_allocator_pool {
            self.clone_into_fixed_size_allocator_and_run_external_rules(
                external_rules,
                path,
                ctx_host,
                program,
                js_allocator_pool,
            );
            return;
        }

        // `allocator` is a fixed-size allocator, so no need to clone AST into a new one
        let tokens = ctx_host.parser_tokens_mut().take_in(&allocator).into_arena_slice_mut();

        // If file has a hashbang, add it to comments.
        // It will be converted to a `Shebang` comment on JS side.
        if let Some(hashbang) = &program.hashbang {
            program
                .comments
                .insert(0, Comment::new(hashbang.span.start, hashbang.span.end, CommentKind::Line));
        }

        self.convert_and_call_external_linter(
            external_rules,
            path,
            ctx_host,
            program,
            tokens,
            allocator,
        );
    }

    #[cfg(not(all(target_pointer_width = "64", target_endian = "little")))]
    fn run_external_rules<'a>(
        &self,
        _external_rules: &[(ExternalRuleId, ExternalOptionsId, AllowWarnDeny)],
        _path: &Path,
        _ctx_host: &mut Rc<ContextHost<'a>>,
        _allocator: &'a Allocator,
        _js_allocator_pool: Option<&AllocatorPool>,
    ) {
        // External rules (JS plugins) are not supported on non-64-bit or big-endian platforms
    }

    #[cfg(all(target_pointer_width = "64", target_endian = "little"))]
    fn run_external_rules_on_source_text(
        &self,
        external_rules: &[(ExternalRuleId, ExternalOptionsId, AllowWarnDeny)],
        path: &Path,
        ctx_host: &ContextHost<'_>,
        source_text: &str,
        allocator: &Allocator,
    ) -> Option<DisableDirectives> {
        if external_rules.is_empty() {
            return None;
        }

        let (original_source_text, _source_text_without_bom, has_bom, span_converter) =
            Self::create_span_converter(source_text);

        let path = path.to_string_lossy();
        let path = path.as_ref();

        let settings_json = match &ctx_host.settings().json {
            Some(json) => serde_json::to_string(&json).unwrap_or_else(|e| {
                let message = format!("Error serializing settings.\nFile path: {path}\n{e}");
                ctx_host.push_diagnostic_without_offset(Message::new(
                    OxcDiagnostic::error(message),
                    PossibleFixes::None,
                ));
                "{}".to_string()
            }),
            None => "{}".to_string(),
        };

        let globals_and_envs = GlobalsAndEnvs::new(ctx_host);
        let globals_json = serde_json::to_string(&globals_and_envs).unwrap_or_else(|e| {
            let message = format!("Error serializing globals.\nFile path: {path}\n{e}");
            ctx_host.push_diagnostic_without_offset(Message::new(
                OxcDiagnostic::error(message),
                PossibleFixes::None,
            ));
            "{}".to_string()
        });

        let external_linter = self.external_linter.as_ref().unwrap();
        let result = (external_linter.lint_file)(
            path.to_owned(),
            external_rules.iter().map(|(rule_id, _, _)| rule_id.raw()).collect(),
            external_rules.iter().map(|(_, options_id, _)| options_id.raw()).collect(),
            settings_json,
            globals_json,
            ctx_host.js_language_options_ids().to_vec(),
            self.workspace_uri.as_ref().map(ToString::to_string),
            Some(original_source_text.to_string()),
            allocator,
        );

        let (result, external_disable_directives) = match result {
            Ok(payload) => {
                if let Some(parse_error) = payload.parse_error.as_ref() {
                    Self::handle_external_source_parse_error(
                        path,
                        ctx_host,
                        original_source_text,
                        &span_converter,
                        parse_error,
                    );
                    return None;
                }

                let external_disable_directives =
                    Self::build_disable_directives_from_external_comments(
                        original_source_text,
                        &payload.comments,
                        &span_converter,
                    );
                (Ok(payload.diagnostics), external_disable_directives)
            }
            Err(err) => (Err(err), None),
        };

        self.handle_external_linter_result(
            external_rules,
            path,
            ctx_host,
            original_source_text,
            &span_converter,
            has_bom,
            true,
            external_disable_directives.as_ref(),
            result,
        );

        external_disable_directives
    }

    #[cfg(not(all(target_pointer_width = "64", target_endian = "little")))]
    fn run_external_rules_on_source_text(
        &self,
        _external_rules: &[(ExternalRuleId, ExternalOptionsId, AllowWarnDeny)],
        _path: &Path,
        _ctx_host: &ContextHost<'_>,
        _source_text: &str,
        _allocator: &Allocator,
    ) -> Option<DisableDirectives> {
        // External rules (JS plugins) are not supported on non-64-bit or big-endian platforms
        None
    }

    /// Clone AST into a fixed-size allocator and run external rules.
    ///
    /// This copies the AST and source text from the standard allocator into a fixed-size
    /// allocator before passing to JS plugins. This allows using standard allocators for
    /// parsing/linting while still supporting JS plugin raw transfer.
    #[cfg(all(target_pointer_width = "64", target_endian = "little"))]
    fn clone_into_fixed_size_allocator_and_run_external_rules(
        &self,
        external_rules: &[(ExternalRuleId, ExternalOptionsId, AllowWarnDeny)],
        path: &Path,
        ctx_host: &ContextHost<'_>,
        original_program: &mut Program<'_>,
        js_allocator_pool: &AllocatorPool,
    ) {
        let js_allocator_guard = js_allocator_pool.get();
        let js_allocator = &*js_allocator_guard;

        // Get the original source text from the `Program`, and replace it with an empty string.
        // This avoids cloning the original source text, which can be large.
        let original_source_text = original_program.source_text;
        original_program.source_text = "";

        // Copy source text to the fixed-size allocator.
        // We have to allocate source text first, because the JS deserializer expects source text
        // to be later in the buffer than all other strings in the AST, and the allocator bumps downwards.
        let new_source_text = js_allocator.alloc_str(original_source_text);

        // If file has a hashbang, add it to comments.
        // It will be converted to a `Shebang` comment on JS side.
        // Clear the original `Vec<Comment>` to avoid cloning it again below.
        let comments = if let Some(hashbang) = &original_program.hashbang {
            let mut comments_with_hashbang =
                ArenaVec::with_capacity_in(original_program.comments.len() + 1, &js_allocator);
            comments_with_hashbang.push(Comment::new(
                hashbang.span.start,
                hashbang.span.end,
                CommentKind::Line,
            ));
            comments_with_hashbang.extend_from_slice_copy(&original_program.comments);

            original_program.comments.clear();

            Some(comments_with_hashbang)
        } else {
            None
        };

        // Clone `Program` into fixed-size allocator.
        // We need to allocate the `Program` struct ITSELF in the allocator, not just its contents.
        // `clone_in` returns a value on the stack, but we need it in the allocator for raw transfer.
        let program = {
            let mut program = original_program.clone_in(js_allocator);
            program.source_text = new_source_text;
            js_allocator.alloc(program)
        };

        // If added hashbang comment, set comments to the new `Vec<Comment>` including hashbang comment
        if let Some(comments) = comments {
            program.comments = comments;
        }

        // Clone tokens into fixed-size allocator
        let tokens = js_allocator.alloc_slice_copy(ctx_host.parser_tokens());

        self.convert_and_call_external_linter(
            external_rules,
            path,
            ctx_host,
            program,
            tokens,
            js_allocator,
        );

        // The `AllocatorGuard` (`js_allocator_guard`) is dropped here, returning the allocator to the pool.
        // This ensures that we never have too many allocators in play at once, avoiding OOM.
    }

    /// Convert spans to UTF-16, write metadata, call external linter, and process diagnostics.
    ///
    /// This is the common code path shared by both `run_external_rules` and
    /// `clone_into_fixed_size_allocator_and_run_external_rules`.
    #[cfg(all(target_pointer_width = "64", target_endian = "little"))]
    fn convert_and_call_external_linter(
        &self,
        external_rules: &[(ExternalRuleId, ExternalOptionsId, AllowWarnDeny)],
        path: &Path,
        ctx_host: &ContextHost<'_>,
        program: &mut Program<'_>,
        tokens: &mut [Token],
        allocator: &Allocator,
    ) {
        let (original_source_text, mut source_text, has_bom, _span_converter) =
            Self::create_span_converter(program.source_text);
        if has_bom {
            program.source_text = source_text;
        }

        // If partial source (Vue/Astro/Svelte), trim the leading newline after `<script>`
        let is_partial = !has_bom && ctx_host.current_sub_host().source_text_offset() > 0;
        let trim_leading = match source_text.as_bytes() {
            [b'\r', b'\n', ..] if is_partial => 2u32,
            [b'\n', ..] if is_partial => 1,
            _ => 0,
        };
        if trim_leading > 0 {
            source_text = &source_text[trim_leading as usize..];
            program.source_text = source_text;
        }

        #[expect(clippy::cast_possible_truncation)]
        let source_text_utf8_end =
            source_text.len() as u32 + if has_bom { BOM_LEN as u32 } else { trim_leading };

        // Create span converter.
        // If source starts with BOM, create converter which ignores the BOM.
        let span_converter = if has_bom {
            Utf8ToUtf16::new_with_offset(source_text, 3)
        } else if trim_leading > 0 {
            Utf8ToUtf16::new_with_offset(source_text, trim_leading)
        } else {
            Utf8ToUtf16::new(source_text)
        };

        // Convert the end of the (trimmed) source text to the UTF-16 offsets used by JS plugins.
        let mut source_text_utf16_len = source_text_utf8_end;
        if let Some(mut converter) = span_converter.converter() {
            converter.convert_offset(&mut source_text_utf16_len);
        }

        // Convert token spans to UTF-16 and update token kinds
        #[expect(clippy::if_not_else, clippy::cast_possible_truncation)]
        let (tokens_offset, tokens_len) = if !tokens.is_empty() {
            update_tokens_as_js(tokens, program, &span_converter);
            (tokens.as_ptr() as u32, tokens.len() as u32)
        } else {
            (0, 0)
        };

        // Convert AST spans to UTF-16
        span_converter.convert_program(program);

        // Convert comment spans to UTF-16.
        // Also set the `content` field (byte 15) of each comment to `None` (0).
        // JS side uses this byte as a "deserialized" flag for tracking lazy deserialization.
        if let Some(mut converter) = span_converter.converter() {
            for comment in &mut program.comments {
                converter.convert_span(&mut comment.span);
                comment.content = CommentContent::None;
            }
        } else {
            for comment in &mut program.comments {
                comment.content = CommentContent::None;
            }
        }

        // Get offset of `Program` within buffer (bottom 32 bits of pointer)
        let program_offset = ptr::from_ref(program) as u32;

        // Write offset of `Program` in metadata at end of buffer
        let is_ts = program.source_type.is_typescript();
        let is_jsx = program.source_type.is_jsx();
        let metadata = RawTransferMetadata::new(
            program_offset,
            is_ts,
            is_jsx,
            has_bom,
            tokens_offset,
            tokens_len,
        );
        // `RawTransferMetadata` sits immediately before `FixedSizeAllocatorMetadata` in the chunk.
        // SAFETY: `Allocator` was created by `FixedSizeAllocator`, so the chunk has a valid
        // `FixedSizeAllocatorMetadata` and a `RawTransferMetadata`-sized region right before it.
        // The position is aligned for `RawTransferMetadata`.
        unsafe {
            let metadata_ptr = allocator
                .fixed_size_metadata_ptr()
                .cast::<u8>()
                .sub(size_of::<RawTransferMetadata>())
                .cast::<RawTransferMetadata>();
            debug_assert!(
                metadata_ptr.addr().get().is_multiple_of(align_of::<RawTransferMetadata>())
            );
            metadata_ptr.write(metadata);
        }

        let path_string = path.to_string_lossy();
        let path_string = path_string.as_ref();

        let settings_json = match &ctx_host.settings().json {
            Some(json) => serde_json::to_string(&json).unwrap_or_else(|e| {
                let message = format!("Error serializing settings.\nFile path: {path_string}\n{e}");
                ctx_host.push_diagnostic(Message::new(
                    OxcDiagnostic::error(message),
                    PossibleFixes::None,
                ));
                "{}".to_string()
            }),
            None => "{}".to_string(),
        };

        let globals_and_envs = GlobalsAndEnvs::new(ctx_host);
        let globals_json = serde_json::to_string(&globals_and_envs).unwrap_or_else(|e| {
            let message = format!("Error serializing globals.\nFile path: {path_string}\n{e}");
            ctx_host
                .push_diagnostic(Message::new(OxcDiagnostic::error(message), PossibleFixes::None));
            "{}".to_string()
        });

        // `external_linter` always exists when `external_rules` is not empty
        let external_linter = self.external_linter.as_ref().unwrap();

        // Pass AST and rule IDs + options IDs to JS
        let result = (external_linter.lint_file)(
            path_string.to_owned(),
            external_rules.iter().map(|(rule_id, _, _)| rule_id.raw()).collect(),
            external_rules.iter().map(|(_, options_id, _)| options_id.raw()).collect(),
            settings_json,
            globals_json,
            ctx_host.js_language_options_ids().to_vec(),
            self.workspace_uri.as_ref().map(ToString::to_string),
            None,
            allocator,
        );

        self.handle_external_linter_result(
            external_rules,
            path_string,
            ctx_host,
            original_source_text,
            &span_converter,
            has_bom,
            false,
            None,
            result.map(|payload| payload.diagnostics),
        );
    }

    fn report_unused_full_file_disable_directives(
        ctx_host: &ContextHost<'_>,
        directives: &DisableDirectives,
        source_text: &str,
        rule_severity: Severity,
    ) {
        let message_for_disable = "Unused eslint-disable directive (no problems were reported).";
        let fix_message = "remove unused disable directive";

        for unused_disable_comment in directives.collect_unused_disable_comments() {
            let span = unused_disable_comment.span;
            match &unused_disable_comment.r#type {
                RuleCommentType::All => {
                    ctx_host.push_diagnostic_without_offset(Message::new(
                        OxcDiagnostic::error(message_for_disable)
                            .with_label(span)
                            .with_severity(rule_severity),
                        PossibleFixes::Single(
                            Fix::delete(span)
                                .with_kind(FixKind::Suggestion)
                                .with_message(fix_message),
                        ),
                    ));
                }
                RuleCommentType::Single(rules_vec) => {
                    for rule in rules_vec {
                        let rule_message = format!(
                            "Unused eslint-disable directive (no problems were reported from {}).",
                            rule.rule_name
                        );
                        let fix = rule.create_fix(source_text, span).with_message(fix_message);

                        ctx_host.push_diagnostic_without_offset(Message::new(
                            OxcDiagnostic::error(rule_message)
                                .with_label(rule.name_span)
                                .with_severity(rule_severity),
                            PossibleFixes::Single(fix),
                        ));
                    }
                }
            }
        }

        for (_, rule_name, enable_comment_span) in directives.unused_enable_comments() {
            let message = rule_name.as_ref().map_or_else(
                || {
                    "Unused eslint-enable directive (no matching eslint-disable directives were found)."
                        .to_string()
                },
                |name| {
                    format!(
                        "Unused eslint-enable directive (no matching eslint-disable directives were found for {name})."
                    )
                },
            );
            ctx_host.push_diagnostic_without_offset(Message::new(
                OxcDiagnostic::error(message)
                    .with_label(*enable_comment_span)
                    .with_severity(rule_severity),
                PossibleFixes::None,
            ));
        }
    }

    fn create_span_converter(source_text: &str) -> (&str, &str, bool, Utf8ToUtf16) {
        const BOM: &str = "\u{feff}";
        const BOM_LEN: usize = BOM.len();

        let original_source_text = source_text;
        let mut source_text_without_bom = source_text;
        let has_bom = source_text_without_bom.starts_with(BOM);
        let span_converter = if has_bom {
            source_text_without_bom = &source_text_without_bom[BOM_LEN..];
            #[expect(clippy::cast_possible_truncation)]
            Utf8ToUtf16::new_with_offset(source_text_without_bom, BOM_LEN as u32)
        } else {
            Utf8ToUtf16::new(source_text_without_bom)
        };

        (original_source_text, source_text_without_bom, has_bom, span_converter)
    }

    fn build_disable_directives_from_external_comments<'a>(
        source_text: &'a str,
        comments: &'a [ExternalComment],
        span_converter: &Utf8ToUtf16,
    ) -> Option<DisableDirectives> {
        if comments.is_empty() {
            return None;
        }

        let raw_comments = comments
            .iter()
            .filter_map(|comment| {
                let mut span = Span::new(comment.start, comment.end);
                span_converter.convert_span_back(&mut span);
                let raw_text = source_text.get(span.start as usize..span.end as usize)?;
                let content_span =
                    Self::external_comment_content_span(raw_text, span, &comment.kind);
                let content_text =
                    source_text.get(content_span.start as usize..content_span.end as usize)?;
                Some(RawDirectiveComment {
                    span,
                    content_span: Some(content_span),
                    text: content_text,
                })
            })
            .collect::<Vec<_>>();

        if raw_comments.is_empty() {
            return None;
        }

        Some(
            crate::disable_directives::DisableDirectivesBuilder::new()
                .build_raw_comments(source_text, &raw_comments),
        )
    }

    fn external_comment_content_span(
        raw_text: &str,
        span: Span,
        kind: &ExternalCommentKind,
    ) -> Span {
        let (start_offset, end_offset) =
            if raw_text.starts_with("<!--") && raw_text.ends_with("-->") {
                (4, 3)
            } else if (matches!(kind, ExternalCommentKind::Shebang) && raw_text.starts_with("#!"))
                || (matches!(kind, ExternalCommentKind::Line | ExternalCommentKind::Shebang)
                    && raw_text.starts_with("//"))
            {
                (2, 0)
            } else if raw_text.starts_with("/*") && raw_text.ends_with("*/") {
                (2, 2)
            } else {
                (0, 0)
            };

        let start = span.start.saturating_add(start_offset);
        let end = span.end.saturating_sub(end_offset).max(start);
        Span::new(start, end)
    }

    fn push_external_linter_diagnostic(
        ctx_host: &ContextHost<'_>,
        use_full_file_spans: bool,
        diagnostic: Message,
    ) {
        if use_full_file_spans {
            ctx_host.push_diagnostic_without_offset(diagnostic);
        } else {
            ctx_host.push_diagnostic(diagnostic);
        }
    }

    fn validate_external_diagnostic_span(span: Span, source_text: &str) -> Result<(), String> {
        if span.start > span.end {
            return Err(format!(
                "Diagnostic range start {} is after end {}.",
                span.start, span.end
            ));
        }

        if source_text.get(span.start as usize..span.end as usize).is_none() {
            return Err(format!(
                "Diagnostic range {}..{} is out of bounds for source text of length {}.",
                span.start,
                span.end,
                source_text.len()
            ));
        }

        Ok(())
    }

    fn handle_external_source_parse_error(
        path: &str,
        ctx_host: &ContextHost<'_>,
        original_source_text: &str,
        span_converter: &Utf8ToUtf16,
        parse_error: &LintFileParseError,
    ) {
        let mut span = Span::new(parse_error.start, parse_error.end);
        span_converter.convert_span_back(&mut span);

        if let Err(err) = Self::validate_external_diagnostic_span(span, original_source_text) {
            let message = format!(
                "Whole-file custom parser returned invalid parse error range.
File path: {path}
{err}"
            );
            Self::push_external_linter_diagnostic(
                ctx_host,
                true,
                Message::new(OxcDiagnostic::error(message), PossibleFixes::None),
            );
            return;
        }

        Self::push_external_linter_diagnostic(
            ctx_host,
            true,
            Message::new(
                OxcDiagnostic::error(format!("Parsing error: {}", parse_error.message))
                    .with_label(span),
                PossibleFixes::None,
            ),
        );
    }

    #[expect(clippy::too_many_arguments)]
    fn handle_external_linter_result(
        &self,
        external_rules: &[(ExternalRuleId, ExternalOptionsId, AllowWarnDeny)],
        path: &str,
        ctx_host: &ContextHost<'_>,
        original_source_text: &str,
        span_converter: &Utf8ToUtf16,
        has_bom: bool,
        use_full_file_spans: bool,
        external_disable_directives: Option<&DisableDirectives>,
        result: Result<Vec<LintFileResult>, String>,
    ) {
        match result {
            Ok(diagnostics) => {
                for diagnostic in diagnostics {
                    // Convert UTF-16 offsets back to UTF-8.
                    // External plugins may report locations outside the source text, or which end
                    // before they start. Clamp them to the end of the source, and represent
                    // reversed locations as empty spans at their stated start.
                    //
                    // Also make sure offsets do not fall in middle of a multi-byte UTF-8 character.
                    // That's possible if UTF-16 offset points to middle of a surrogate pair.
                    let start = diagnostic.start.min(source_text_utf16_len);
                    let end = diagnostic.end.min(source_text_utf16_len).max(start);
                    let mut span = Span::new(start, end);
                    span_converter.convert_span_back(&mut span);

                    if let Err(err) =
                        Self::validate_external_diagnostic_span(span, original_source_text)
                    {
                        let message = format!(
                            "Plugin `{plugin_name}/{rule_name}` returned invalid diagnostic range.
File path: {path}
{err}"
                        );
                        Self::push_external_linter_diagnostic(
                            ctx_host,
                            use_full_file_spans,
                            Message::new(OxcDiagnostic::error(message), PossibleFixes::None),
                        );
                        continue;
                    }

                    let is_disabled = if use_full_file_spans {
                        external_disable_directives
                            .is_some_and(|directives| directives.contains(&full_rule_name, span))
                            || ctx_host.contains_disable_directive_for_full_file_span(
                                &full_rule_name,
                                span,
                            )
                    } else {
                        ctx_host.disable_directives().contains(&full_rule_name, span)
                    };
                    if is_disabled {
                        continue;
                    }

                    let create_fix = |fixes, fix_kind| match convert_and_merge_js_fixes(
                        fixes,
                        original_source_text,
                        span_converter,
                        has_bom,
                    ) {
                        Ok(fix) => Some(fix.with_kind(fix_kind)),
                        Err(err) => {
                            let fixes_type = if fix_kind.contains(FixKind::Suggestion) {
                                "suggestions"
                            } else {
                                "fixes"
                            };
                            let message = format!(
                                "Plugin `{plugin_name}/{rule_name}` returned invalid {fixes_type}.
File path: {path}
{err}"
                            );
                            Self::push_external_linter_diagnostic(
                                ctx_host,
                                use_full_file_spans,
                                Message::new(OxcDiagnostic::error(message), PossibleFixes::None),
                            );
                            None
                        }
                    };

                    let fix = diagnostic.fixes.and_then(|fixes| create_fix(fixes, FixKind::Fix));

                    let possible_fixes = if let Some(suggestions) = diagnostic.suggestions
                        && ctx_host.fix.can_apply(FixKind::Suggestion)
                    {
                        debug_assert!(
                            !suggestions.is_empty(),
                            "`diagnostic.suggestions` should be `None` if there are no suggestions"
                        );

                        let suggestions = suggestions.into_iter().filter_map(|suggestion| {
                            create_fix(suggestion.fixes, FixKind::Suggestion)
                                .map(|fix| fix.with_message(suggestion.message))
                        });

                        PossibleFixes::from_iter(iter::chain(fix, suggestions))
                    } else {
                        PossibleFixes::from(fix)
                    };

                    ctx_host.push_diagnostic(Message::new(
                        OxcDiagnostic::error(diagnostic.message)
                            .with_label(span)
                            .with_error_code(plugin_name.to_string(), rule_name.to_string())
                            .with_severity(severity.into()),
                        possible_fixes,
                    ));
                }
            }
            Err(err) => {
                let message = format!(
                    "Error running JS plugin.
File path: {path}
{err}"
                );
                Self::push_external_linter_diagnostic(
                    ctx_host,
                    use_full_file_spans,
                    Message::new(OxcDiagnostic::error(message), PossibleFixes::None),
                );
            }
        }
    }
}

/// Metadata written to end of buffer.
///
/// Duplicate of `RawTransferMetadata` in `napi/parser/src/raw_transfer_types.rs`.
/// Any changes made here also need to be made there.
/// `oxc_ast_tools` checks that the 2 copies are identical.
#[ast]
pub struct RawTransferMetadata2 {
    /// Offset of `Program` within buffer.
    /// Note: In `RawTransferMetadata` (in `napi/parser`), this field is offset of `RawTransferData`,
    /// but here it's offset of `Program`.
    pub data_offset: u32,
    /// `true` if AST is TypeScript.
    pub is_ts: bool,
    /// `true` if AST is JSX.
    pub is_jsx: bool,
    /// `true` if source text has a BOM.
    pub has_bom: bool,
    /// Offset of lexer `Token`s within buffer.
    pub tokens_offset: u32,
    /// Number of lexer `Token`s.
    pub tokens_len: u32,
}

use RawTransferMetadata2 as RawTransferMetadata;

impl RawTransferMetadata {
    pub fn new(
        data_offset: u32,
        is_ts: bool,
        is_jsx: bool,
        has_bom: bool,
        tokens_offset: u32,
        tokens_len: u32,
    ) -> Self {
        #[expect(clippy::inconsistent_struct_constructor)] // `#[ast]` macro reorders fields
        Self { data_offset, is_ts, is_jsx, has_bom, tokens_offset, tokens_len }
    }
}

#[cfg(all(test, target_pointer_width = "64", target_endian = "little"))]
mod tests {
    use std::{
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use rustc_hash::FxHashMap;

    use super::*;
    use crate::config::{categories::OxlintCategories, config_store::ResolvedOxlintOverrides};

    fn noop_load_plugin() -> ExternalLinterLoadPluginCb {
        Arc::new(Box::new(|_, _, _, _| {
            panic!("load_plugin should not be called in this test");
        }))
    }

    fn noop_setup_rule_configs() -> ExternalLinterSetupRuleConfigsCb {
        Arc::new(Box::new(|_| Ok(())))
    }

    fn noop_create_workspace() -> ExternalLinterCreateWorkspaceCb {
        Arc::new(Box::new(|_| Ok(())))
    }

    fn noop_destroy_workspace() -> ExternalLinterDestroyWorkspaceCb {
        Arc::new(Box::new(|_| Ok(())))
    }

    fn create_external_only_test_linter(lint_file: ExternalLinterLintFileCb) -> Linter {
        let mut external_plugin_store = ExternalPluginStore::new(true);
        external_plugin_store.register_plugin(
            PathBuf::from("/tmp/eslint-plugin-svelte/index.js"),
            "svelte".to_string(),
            0,
            vec!["empty-file".to_string()],
        );
        let external_rule_id =
            external_plugin_store.lookup_rule_id("svelte", "empty-file").unwrap();

        let config = Config::new(
            vec![],
            vec![(external_rule_id, ExternalOptionsId::NONE, AllowWarnDeny::Warn)],
            OxlintCategories::default(),
            LintConfig { js_has_custom_parser: true, ..LintConfig::default() },
            ResolvedOxlintOverrides::default(),
        );
        let config_store = ConfigStore::new(config, FxHashMap::default(), external_plugin_store);

        let external_linter = ExternalLinter::new(
            noop_load_plugin(),
            noop_setup_rule_configs(),
            lint_file,
            noop_create_workspace(),
            noop_destroy_workspace(),
        );

        Linter::new(LintOptions::default(), config_store, Some(external_linter))
    }

    #[test]
    fn test_external_only_custom_parser_runs_on_empty_source_text() {
        let mut external_plugin_store = ExternalPluginStore::new(true);
        external_plugin_store.register_plugin(
            PathBuf::from("/tmp/eslint-plugin-svelte/index.js"),
            "svelte".to_string(),
            0,
            vec!["empty-file".to_string()],
        );
        let external_rule_id =
            external_plugin_store.lookup_rule_id("svelte", "empty-file").unwrap();

        let config = Config::new(
            vec![],
            vec![(external_rule_id, ExternalOptionsId::NONE, AllowWarnDeny::Warn)],
            OxlintCategories::default(),
            LintConfig { js_has_custom_parser: true, ..LintConfig::default() },
            ResolvedOxlintOverrides::default(),
        );
        let config_store = ConfigStore::new(config, FxHashMap::default(), external_plugin_store);

        let call_count = Arc::new(AtomicUsize::new(0));
        let lint_call_count = Arc::clone(&call_count);
        let external_linter = ExternalLinter::new(
            noop_load_plugin(),
            noop_setup_rule_configs(),
            Arc::new(Box::new(move |_, _, _, _, _, _, _, source_text, _| {
                lint_call_count.fetch_add(1, Ordering::Relaxed);
                assert_eq!(source_text.as_deref(), Some(""));
                Ok(LintFilePayload {
                    diagnostics: vec![LintFileResult {
                        rule_index: 0,
                        message: "empty whole-file custom parser ran".to_string(),
                        start: 0,
                        end: 0,
                        fixes: None,
                        suggestions: None,
                    }],
                    comments: vec![],
                    parse_error: None,
                })
            })),
            noop_create_workspace(),
            noop_destroy_workspace(),
        );

        let linter = Linter::new(LintOptions::default(), config_store, Some(external_linter));
        let allocator = Allocator::default();
        let messages =
            linter.run_external_only_on_source_text(Path::new("App.custom"), "", &allocator);

        assert_eq!(call_count.load(Ordering::Relaxed), 1);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].span, Span::new(0, 0));
    }

    #[test]
    fn test_external_only_custom_parser_invalid_rule_index_reports_error() {
        let linter =
            create_external_only_test_linter(Arc::new(Box::new(|_, _, _, _, _, _, _, _, _| {
                Ok(LintFilePayload {
                    diagnostics: vec![LintFileResult {
                        rule_index: 1,
                        message: "invalid rule index".to_string(),
                        start: 0,
                        end: 0,
                        fixes: None,
                        suggestions: None,
                    }],
                    comments: vec![],
                    parse_error: None,
                })
            })));

        let allocator = Allocator::default();
        let messages = linter.run_external_only_on_source_text(
            Path::new("App.custom"),
            "<h1>Hello</h1>",
            &allocator,
        );

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].span, Span::new(0, 0));
        assert!(messages[0].error.to_string().contains("invalid rule index 1"));
    }

    #[test]
    fn test_external_only_custom_parser_invalid_diagnostic_range_reports_error() {
        let linter =
            create_external_only_test_linter(Arc::new(Box::new(|_, _, _, _, _, _, _, _, _| {
                Ok(LintFilePayload {
                    diagnostics: vec![LintFileResult {
                        rule_index: 0,
                        message: "invalid diagnostic range".to_string(),
                        start: 0,
                        end: 100,
                        fixes: None,
                        suggestions: None,
                    }],
                    comments: vec![],
                    parse_error: None,
                })
            })));

        let allocator = Allocator::default();
        let messages = linter.run_external_only_on_source_text(
            Path::new("App.custom"),
            "<h1>Hello</h1>",
            &allocator,
        );

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].span, Span::new(0, 0));
        let error_message = messages[0].error.to_string();
        assert!(error_message.contains("returned invalid diagnostic range"));
        assert!(error_message.contains("0..100"));
    }

    #[test]
    fn test_external_only_custom_parser_parse_error_reports_diagnostic() {
        let linter =
            create_external_only_test_linter(Arc::new(Box::new(|_, _, _, _, _, _, _, _, _| {
                Ok(LintFilePayload {
                    diagnostics: vec![],
                    comments: vec![],
                    parse_error: Some(LintFileParseError {
                        message: "Expected an identifier".to_string(),
                        start: 23,
                        end: 24,
                    }),
                })
            })));

        let allocator = Allocator::default();
        let source_text = "{#if page.data.user && }\n{/if}";
        let messages = linter.run_external_only_on_source_text(
            Path::new("App.custom"),
            source_text,
            &allocator,
        );

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].span, Span::new(23, 24));
        assert!(messages[0].error.to_string().contains("Parsing error: Expected an identifier"));
    }

    #[test]
    fn test_external_only_custom_parser_invalid_parse_error_range_reports_error() {
        let linter =
            create_external_only_test_linter(Arc::new(Box::new(|_, _, _, _, _, _, _, _, _| {
                Ok(LintFilePayload {
                    diagnostics: vec![],
                    comments: vec![],
                    parse_error: Some(LintFileParseError {
                        message: "invalid parse error range".to_string(),
                        start: 100,
                        end: 101,
                    }),
                })
            })));

        let allocator = Allocator::default();
        let messages = linter.run_external_only_on_source_text(
            Path::new("App.custom"),
            "<h1>Hello</h1>",
            &allocator,
        );

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].span, Span::new(0, 0));
        let error_message = messages[0].error.to_string();
        assert!(error_message.contains("invalid parse error range"));
        assert!(error_message.contains("100..101"));
    }

    #[cfg(feature = "svelte-rsvelte-backend")]
    #[test]
    fn test_external_only_native_svelte_skips_custom_parser() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let lint_call_count = Arc::clone(&call_count);
        let linter = create_external_only_test_linter(Arc::new(Box::new(
            move |_, _, _, _, _, _, _, _, _| {
                lint_call_count.fetch_add(1, Ordering::Relaxed);
                Ok(LintFilePayload { diagnostics: vec![], comments: vec![], parse_error: None })
            },
        )));

        let allocator = Allocator::default();
        let messages = linter.run_external_only_on_source_text(
            Path::new("App.svelte"),
            "<h1>Hello</h1>",
            &allocator,
        );

        assert_eq!(call_count.load(Ordering::Relaxed), 0);
        assert!(messages.is_empty());
    }
}
