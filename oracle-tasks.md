# Oracle Tasks

## Intro Prompt
You are reviewing Oxc Svelte-support work from a code snapshot (no local command execution available).  
Your job is to fix issues directly in code and keep progress tracked in `svelte-support-phases.md`.

Constraints:
- You are cloud-only: do not assume local toolchains, shell commands, or test execution.
- Focus on code-level fixes and correctness first.
- Update `svelte-support-phases.md` in every response with:
  - current status
  - what you changed
  - expected impact
  - remaining risks/gaps
- Keep changes minimal and targeted.
- Do not split commits or plan PRs yet.

Goal:
- Make full Svelte support publish-ready (lint + formatter plugin path), then leave a clear remaining-gap list.

## Issues To Fix

1. Hard crash in `languageOptions_parser_parse` fixture.
- Symptom: Rust panic in allocator (`oxc_allocator/src/bump.rs:1432`, `ptr <= footer`) during JS-plugin execution.
- Priority: P0 release blocker.
- Likely files:
  - `apps/oxlint/src-js/plugins/context.ts`
  - `apps/oxlint/src-js/plugins/lint.ts`
  - `crates/oxc_linter/src/lib.rs`
  - `crates/oxc_linter/src/external_linter.rs`

2. Whole-file custom-parser AST traversal is incomplete.
- Symptom: listeners on external nodes (`SvelteElement`, `SvelteClassName`, `SvelteText`) do not run in several fixtures.
- Impact: key Svelte plugin behavior missing.
- Likely cause: parser `visitorKeys` are captured but not actually used for traversal in external-parser mode.
- File:
  - `apps/oxlint/src-js/plugins/lint.ts`

3. Override `settings` rejected in package-shaped Svelte config.
- Symptom: config parse error: unknown field `settings` inside override.
- Impact: realistic Svelte config shape fails.
- Likely files:
  - `crates/oxc_linter/src/config/overrides.rs`
  - related config merge/plumbing in builder/store modules.

4. Incorrect rule-count reporting for external-parser runs.
- Symptom: output shows `with 0 rules` while diagnostics still come from JS plugin rules.
- Impact: wrong runtime/reporting metadata.
- Likely files:
  - `apps/oxlint/src/run.rs`
  - `crates/oxc_linter/src/service/runtime.rs`

5. Internal JS-config fields leak into user-facing unknown-field hints.
- Symptom: `_languageOptionsId` and `_languageOptionsHasParser` appear in expected schema field lists.
- Impact: UX/debuggability regression.
- Likely files:
  - `apps/oxlint/src-js/js_config.ts`
  - `crates/oxc_linter/src/config/oxlintrc.rs`

6. Oxfmt migration regression: relative plugin path becomes absolute.
- Symptom: migrate-prettier test expects `./plugins/...` but output becomes absolute temp path.
- Impact: portability regression in migrated config.
- File:
  - `apps/oxfmt/src-js/cli/migration/migrate-prettier.ts`

7. Snapshot/rendering drift in diagnostics output for some Svelte fixtures.
- Symptom: changed span rendering (`: ^` vs expanded spans / alternate multiline markers).
- Impact: may be intended, but currently creates mismatch noise and uncertainty.
- Areas:
  - diagnostic span mapping/formatting in oxlint runtime+CLI paths.

8. Fixture stderr noise from Node ESM deprecation warnings.
- Symptom: `[DEP0151]` warnings in package-shaped fixture stderr.
- Impact: brittle snapshots/noisy output.
- Fixture package metadata likely needs explicit `main`/`exports` handling.

9. Environment-sensitive stylish snapshot behavior (`NO_COLOR`).
- Symptom: style snapshots fail when `NO_COLOR=1`; pass when unset.
- Impact: environment fragility.
- Area:
  - oxlint snapshot test harness/output formatter assumptions.

10. Minor cleanup warning in Rust.
- Symptom: unused variable `source_text`.
- File:
  - `crates/oxc_linter/src/lib.rs`

