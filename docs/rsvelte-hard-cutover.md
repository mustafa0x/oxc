# rsvelte backend hard cutover plan

Date: 2026-07-10

Status: implementation and local verification complete on the `svelte` branch.
The native Cargo backend, oxlint cutover, oxfmt runtime cutover, and npm package
artifacts are verified for every publish target. The branch remains local and
must not be pushed until explicitly requested.

Repos:

- Oxc fork: `/Users/mustafaj/dev/tmp/oxc`
- rsvelte: `/Users/mustafaj/dev/tmp/rsvelte`

## Goal

Use `rsvelte` itself as this Oxc fork's Svelte backend for parsing, formatting,
and linting.

The end state is:

- `oxfmt` formats `.svelte` through `rsvelte_formatter`, not
  `prettier-plugin-svelte`.
- `oxlint` parses `.svelte` through `rsvelte`, not `svelte-eslint-parser`.
- Svelte-specific lint diagnostics come from either rsvelte compiler/analyzer
  diagnostics or native Oxc Rust rules over rsvelte AST data, not from a required
  ESLint parser backend.
- CLI, API, stdin, file walking, LSP, and npm packages all use the same backend.
- No Svelte-specific runtime dependency on Prettier, `prettier-plugin-svelte`,
  or `svelte-eslint-parser`.

This is not a plan to migrate the `rsvelte` repo's own playground from upstream
`oxlint` / `oxfmt` packages to forked npm packages. That may be useful later,
but it is a different task.

## Non-goals

- Do not replace Oxc's JavaScript or TypeScript parser/formatter.
- Do not rely on ESLint or Prettier as the final `.svelte` fallback.
- Do not commit unrelated changes in `../rsvelte`.
- Do not remove existing Svelte smoke tests until the rsvelte-backed path has
  equivalent or better coverage.

## Current state

The local implementation now uses a single native backend:

- `crates/oxc_svelte_backend` owns parsing, formatting, source ranges,
  comments, scripts, and rsvelte diagnostics.
- `oxlint` parses `.svelte` with rsvelte before embedded JS/TS linting. Native
  Svelte files never invoke whole-file JS custom parsers or external JS rules.
- `oxfmt` classifies `.svelte` as `RsvelteFormatter` in native builds. CLI,
  API, stdin, walk, and LSP share that strategy.
- `prettier-plugin-svelte`, `svelte/compiler`, and `svelte-eslint-parser` are
  not loaded by the core Svelte runtime paths. Legacy formatter plugin specs
  are accepted only as ignored migration/config input.
- Bridge-only real-package helpers, fixtures, canaries, and beta reporting
  infrastructure have been removed. Native fixtures cover the retained
  contracts.

The vendored `rsvelte` source provides the pieces used by the adapter:

- `svelte-compiler-rust` exposes `parse`, `parse_parallel`, `compile`,
  `compile_module`, `print`, and `svelte2tsx`.
- `rsvelte_formatter` formats Svelte source in-process using rsvelte parsing and
  `oxc_formatter` for JS/TS expressions and script bodies.
- `rsvelte` has NAPI and WASM surfaces, but the Oxc CLI/runtime integration
  should prefer direct Rust library integration.

Current formatter boundaries:

- JS/TS module and instance scripts are formatted with Oxc, including imports,
  TypeScript, and Svelte runes.
- `svelteIndentScriptAndStyle` controls script-body indentation; Rukn's existing
  `false` configuration is covered by API and real-project smoke tests.
- Template expressions, block expressions, typed snippet parameters,
  destructuring patterns, and whitespace-only indentation are formatted.
- Element open tags and attributes are preserved verbatim. The vendored
  open-tag rewrite is intentionally disabled because real-project validation
  found destructive range handling for self-closing components and attributes.
- `<style>` bodies, `{@const}` bodies, non-whitespace text, and
  whitespace-sensitive elements are preserved verbatim when no dedicated
  formatter exists.
- Formatted real-project copies must reparse and be idempotent; preserving an
  unsupported span is preferred over speculative rewriting.

## Architecture

Add one Oxc-facing adapter layer instead of wiring rsvelte directly into every
caller.

Recommended shape:

- Add a crate such as `crates/oxc_svelte` or `crates/oxc_svelte_backend`.
- That crate owns all rsvelte dependency glue and exposes stable Oxc-facing
  APIs:
  - `parse_svelte(source, path, options) -> SvelteParseResult`
  - `format_svelte(source, path, options) -> FormatResult`
  - `lint_svelte_payload(source, path, options) -> LintParsePayload`
- `apps/oxfmt`, `apps/oxlint`, and `oxc_language_server` call this adapter.
- The adapter hides whether rsvelte is a path dependency, git dependency,
  vendored crate, or published crate.

Do not call rsvelte's NAPI package from Oxc binaries. That would make the Rust
CLI depend on Node at runtime and would split behavior between native and npm
paths. Compile the Rust rsvelte library into the same binaries/packages.

## Dependency strategy

Do not start by adding this directly to Oxc:

```toml
svelte-compiler-rust = { path = "../rsvelte/crates/rsvelte_core" }
```

That would likely pull in a second copy of core Oxc crates through rsvelte's
`0.133` dependencies and its git-pinned `oxc_formatter`. Duplicate Oxc crate
versions will create incompatible AST/span/formatter types and difficult
linkage bugs.

First make rsvelte build against this local Oxc workspace:

1. Create a branch in `../rsvelte` for Oxc backend integration.
2. Replace every rsvelte Oxc crate dependency with path dependencies to
   `../oxc/crates/*`, or use a `[patch.crates-io]` strategy that forces every
   Oxc crate to resolve to the local workspace.
3. Remove every git-pinned Oxc dependency from rsvelte crates used by this
   backend, including `oxc_formatter` and `oxc_formatter_core` in
   `rsvelte_core`, `rsvelte_formatter`, and `rsvelte_fmt`.
4. Confirm all rsvelte crates pulled into Oxc build and test against the local
   Oxc APIs.

Then add optional Oxc dependencies behind a feature gate:

```toml
[features]
svelte-rsvelte-backend = [
  "dep:svelte-compiler-rust",
  "dep:rsvelte_formatter",
]

[dependencies]
svelte-compiler-rust = { path = "../rsvelte/crates/rsvelte_core", default-features = false, features = ["native"], optional = true }
rsvelte_formatter = { path = "../rsvelte/crates/rsvelte_formatter", optional = true }
```

Choose the distribution model during Phase 0, before product integration:

- vendored/submodule rsvelte source inside the fork,
- a git dependency pinned to an rsvelte commit,
- or published rsvelte crates.

For a hard cutover that users can install reproducibly, avoid an unpublished
local path dependency as the final state.

Current Phase 0 implementation decision:

- Vendor the two required crates from immutable `mustafa0x/rsvelte` commit
  `672bb074` under `vendor/rsvelte` and consume them by path. Local, CI, and
  release builds use the same source without a sibling checkout, Git submodule,
  or rsvelte's unrelated apps, tests, and nested repositories.
- Add Cargo patches in this Oxc workspace so rsvelte's `oxc_*` dependencies and
  git-pinned formatter dependencies resolve to this checkout's local crates.
- Keep the adapter behind an explicit Cargo feature, enabled by default in the
  product crates that ship native Svelte support.
- Update the pinned rsvelte revision deliberately and validate the unified Cargo
  graph before publishing a new fork package version.
- The current dependency audit lives in `docs/rsvelte-dependency-audit.md`.
  It records that the earlier `oxc_sourcemap` duplicate was resolved locally by
  removing rsvelte's unused `string_wizard` dependency.

## Phase 0: compatibility spike

Goal: prove Oxc and rsvelte can compile as one Rust dependency graph.

Tasks:

- Choose the repo/distribution model for rsvelte source in this fork. This
  decision affects workspace layout, lockfiles, CI, npm build inputs, and release
  reproducibility, so do not postpone it to packaging cleanup.
- Build `../rsvelte` against this Oxc checkout with unified Oxc path deps.
- Add a small Oxc adapter crate or temporary spike module.
- Implement minimal calls:
  - parse a `.svelte` string with rsvelte,
  - format it with `rsvelte_formatter`,
  - return source locations, comments, and errors in an Oxc-friendly shape.
- Run focused checks:

```bash
mise x -- cargo check -p oxfmt -p oxlint -p oxc_linter
```

Exit criteria:

- The rsvelte source/dependency model is chosen and documented.
- No duplicate Oxc crate versions in `cargo tree`.
- `rsvelte_formatter` builds against the local Oxc formatter crates.
- A tiny `.svelte` fixture can parse and format in-process without Node.

## Phase 1: parser adapter

Goal: define the canonical parse payload Oxc will use for `.svelte`.

The hard part is not calling `rsvelte::parse`; it is matching what Oxc's lint
and tooling layers need after parsing.

The adapter must provide:

- parse diagnostics with correct byte ranges and line/column mapping,
- comments, including Svelte/HTML comments,
- tokens if JS plugin compatibility still needs them,
- visitor keys for Svelte nodes,
- parser metadata such as "this is Svelte",
- parser services needed by existing Svelte lint rules,
- embedded script/module ASTs in a form Oxc can lint and fix,
- stable source ranges for fixes, suggestions, and disable directives.

There are three possible compatibility levels:

1. Native rsvelte AST only.
   Best long-term Rust design, but it requires native Svelte lint rules.
2. ESTree-compatible Svelte AST.
   Better compatibility with JS plugins, but requires a careful converter.
3. `svelte-eslint-parser` compatibility.
   Highest compatibility with current `eslint-plugin-svelte` behavior, but the
   most exacting because plugin rules expect specific node shapes and
   `parserServices`.

Recommended hard-cut path: make level 1 the core backend contract, and add level
2 only where Oxc infrastructure needs an ESTree-like interchange shape. Treat
level 3 as an optional compatibility bridge for existing JS plugin rules, not as
the backend definition. If level 3 is implemented, it must be allowed to disappear
later without changing the native rsvelte parser/formatter/linter path.

Tests to add:

- AST shape parity for representative Svelte files.
- Comments and token ranges.
- `<!-- oxlint-disable -->` and inline disable directive behavior.
- Parse errors with correct locations.
- Module script plus instance script.
- TypeScript script blocks.
- Svelte 5 runes syntax and snippets.

## Phase 2: oxlint backend

Goal: replace the `.svelte` JS custom parser backend with rsvelte and define how
Svelte lint diagnostics are produced.

Lint backend decision:

- Primary hard-cut path: native Svelte diagnostics. Feed rsvelte
  parser/analyzer/compiler diagnostics into `oxlint`, then port high-value
  Svelte rules to native Rust over rsvelte AST data.
- Optional bridge: keep JS `eslint-plugin-svelte` compatibility only if a
  project needs those rules before native parity exists. In that mode,
  `eslint-plugin-svelte` is a plugin layer fed by rsvelte-compatible data; it is
  not the parser backend.
- Rejected final state: requiring `svelte-eslint-parser` to lint `.svelte`.

Initial implementation:

- Detect `.svelte` files before the JS custom-parser path.
- Call the Oxc Svelte adapter.
- Convert rsvelte diagnostics into Oxc diagnostics with stable ranges and rule
  metadata.
- Feed embedded JS/TS to existing Oxc lint flows where appropriate.
- Feed Svelte AST/services to JS plugin rules if `eslint-plugin-svelte` remains
  supported during the transition.
- Preserve existing whole-file custom parser behavior only for non-Svelte custom
  parsers.

Existing partial loader decision:

Oxc already has `crates/oxc_linter/src/loader/partial_loader/svelte.rs`, which
extracts `<script>` blocks with a lightweight Rust scanner. The rsvelte cutover
must not leave that as a competing Svelte parser.

Pick one path during implementation:

- Replace it with rsvelte script extraction and delete the scanner once all
  callers are moved.
- Keep it only as a documented fast path for embedded JS/TS script linting, with
  tests proving it matches rsvelte script ranges.
- Move its behavior behind the new Svelte adapter so no caller can accidentally
  bypass rsvelte for `.svelte`.

Do not keep both independently routed.

Keep these existing behavior contracts:

- disable directives work in JS comments and Svelte/HTML comments,
- fixes and suggestions use correct source ranges,
- unused disable directives are reported,
- LSP diagnostics match CLI diagnostics,
- type-aware paths do not treat raw `.svelte` markup as JS syntax errors.

Lint coverage contract:

Before removing `svelte-eslint-parser` support for the core Svelte path, create
a matrix with one row per current Svelte lint signal:

- current `eslint-plugin-svelte` recommended rule or parser diagnostic,
- equivalent rsvelte compiler/analyzer diagnostic, if one exists,
- native Oxc Rust rule candidate, if one is needed,
- existing Oxc built-in rule special case for `.svelte`, if one exists,
- severity mapping,
- fix/suggestion support,
- accepted gap or intentional difference.

Hard cutover requires accepting that matrix explicitly. "Baseline diagnostics"
is not enough if it silently drops recommended Svelte coverage.

The current working matrix lives in `docs/rsvelte-lint-coverage.md`.

Include Oxc's current `.svelte`-specific built-in rule behavior in that matrix.
Several existing native rules skip or alter behavior for Svelte because template
usage is not visible to the JS/TS AST today. Once rsvelte can map template usage
back to script symbols, these skips may need to change. Audit at least:

- `no_unused_vars`,
- `consistent_type_imports`,
- `rules_of_hooks`,
- `no_unused_labels`,
- `no_empty_file`.

Likely Oxc areas:

- `apps/oxlint`
- `crates/oxc_linter`
- external plugin raw-transfer/deserializer code if JS plugin rules still run
- `crates/oxc_language_server`

Exit criteria:

- `.svelte` linting no longer requires `svelte-eslint-parser`.
- `oxlint` emits at least a baseline set of native rsvelte-backed Svelte
  diagnostics.
- The Svelte lint coverage matrix has been reviewed, and all dropped
  `eslint-plugin-svelte` coverage is documented as accepted.
- Existing Svelte oxlint fixtures pass through the rsvelte backend.
- Real Svelte package tests are converted from "real parser package" tests into
  "rsvelte backend" tests.

Current implementation decision:

- The optional JS Svelte plugin bridge is not retained. Under the native
  backend, `.svelte` files do not invoke whole-file custom parsers or external
  JS rules. Generic whole-file custom parser support remains available for
  non-Svelte extensions.
- Parser services, ESTree template traversal, `eslint-plugin-svelte` template
  fixes, and its recommended `no-useless-mustaches` signal are accepted
  compatibility drops for the hard cut. Future equivalents must be native
  rsvelte/Oxc behavior.

## Phase 3: type-aware Svelte decision

Goal: decide whether type-aware `.svelte` linting is in scope for the hard
cutover, and if it is, route it through rsvelte instead of JS parser services.

This is a blocking cutover decision, not an optional follow-up. The hard cutover
may proceed with type-aware Svelte explicitly out of scope, but it must not
proceed with an ambiguous support level.

rsvelte exposes `svelte2tsx`, which is the right primitive for type-aware Svelte
work. A complete type-aware path needs:

- virtual TSX generation from `.svelte`,
- source maps from virtual TSX diagnostics back to original `.svelte` spans,
- TypeScript project-service integration for virtual files,
- parser-service equivalents such as ESTree-to-TS node maps if the optional JS
  plugin bridge remains,
- deterministic cache invalidation when a Svelte file or imported TS file
  changes,
- CLI and LSP diagnostics using the same remapping.

Decision options:

- In scope for hard cutover: implement and test the full `svelte2tsx` type-aware
  path before removing the old Svelte parser path.
- Out of scope for hard cutover: explicitly disable or degrade type-aware Svelte
  diagnostics with a clear message, while still supporting syntax/parser
  diagnostics and non-type-aware linting.

Do not claim type-aware Svelte support merely because raw `.svelte` files no
longer syntax-error.

Exit criteria:

- The support level for type-aware Svelte is documented as either in scope or out
  of scope for the hard cutover.
- If in scope, `svelte2tsx` remapping and project-service behavior have tests.
- If out of scope, CLI/API/LSP behavior is explicit and tested.

Current decision: type-aware Svelte is out of scope for this hard cut. CLI and
LSP explicitly degrade `.svelte` type-aware requests instead of feeding raw
markup to TypeScript or claiming parser-service compatibility. Supporting it
later requires the complete `svelte2tsx` mapping described above.

## Phase 4: oxfmt backend

Goal: replace `prettier-plugin-svelte` with `rsvelte_formatter`.

Initial implementation should be a gated spike, not the default user-visible
path:

- Route `.svelte` files to the Oxc Svelte adapter from `oxfmt` behind a feature
  flag or clearly isolated branch path.
- Map existing Oxfmt options to `rsvelte_formatter::FormatOptions`.
- Measure representative diffs against the current `prettier-plugin-svelte`
  path.
- Fix or explicitly accept rsvelte formatter gaps before removing the current
  plugin-backed path.
- Only after those gates pass, make the rsvelte formatter path the default.
- Keep the public `svelte` config surface if possible, but make it configure the
  native backend instead of loading a plugin.
- Remove the requirement that users configure `plugins: ["prettier-plugin-svelte"]`
  for `.svelte`.

Formatter gaps must be resolved before calling this a hard cutover. Either:

- implement missing rsvelte formatter surfaces, or
- explicitly preserve unsupported spans without damaging code and document the
  expected differences.

The hard-cut bar should include:

- `<script>` and module script formatting,
- markup indentation and block formatting,
- Svelte expressions and directives,
- `{@const}`,
- destructuring patterns in Svelte syntax,
- `<style>` body handling,
- stable behavior for whitespace-sensitive text.

Likely Oxc areas:

- `apps/oxfmt/src/core/support.rs`
- `apps/oxfmt/src/core/format.rs`
- `apps/oxfmt/src/core/options`
- `apps/oxfmt/src-js/libs/apis.ts`
- CLI/API/LSP/stdin tests that currently mention `prettier-plugin-svelte`

Exit criteria:

- `.svelte` formatting no longer requires `prettier-plugin-svelte` or
  `svelte/compiler`.
- CLI, API, stdin, walk, and LSP all use `rsvelte_formatter`.
- Existing real-package formatter tests are replaced or rewritten to validate
  native rsvelte behavior.

Current implementation decision:

- The native strategy is authoritative and enabled by default in shipping
  oxfmt builds.
- The external Svelte Prettier loader, language discovery, payload injection,
  runtime dependencies, bridge fixtures, managed-package scripts, and canary
  workflow are removed.
- Native CLI, API, stdin, walk, and LSP tests remain. Migration tests verify
  that `prettier-plugin-svelte` is dropped with a warning.
- Unsupported open-tag/attribute, style, const-tag, and whitespace-sensitive
  text spans are preserved. Real-project regression tests prove formatted
  output reparses and no internal sentinel text leaks into output.
- Section sort order and shorthand settings remain accepted compatibility
  inputs but preserve their corresponding source spans.

## Phase 5: API, LSP, stdin, and walk integration

Goal: no secondary `.svelte` path remains.

Verify every entrypoint:

- `oxlint App.svelte`
- `oxlint --stdin --stdin-filename App.svelte`
- `oxfmt App.svelte`
- `oxfmt --stdin-filepath App.svelte`
- Node API format/lint calls,
- recursive file walking,
- editor/LSP diagnostics,
- editor/LSP formatting.

All entrypoints should route through the same adapter crate and should not load
Svelte JS packages at runtime.

Current status:

- Formatter API, CLI stdin, recursive walk, and LSP tests use the native
  strategy and pass without a resolvable Svelte plugin.
- Linter CLI walking and LSP diagnostics use the native parser gate. Oxlint has
  no stdin lint CLI surface; this item is not applicable unless one is added.
- Real-project CLI checks cover both Rukn and rsvelte's playground. Node API and
  LSP behavior are covered by repository tests rather than by modifying those
  sibling projects.

## Phase 6: packaging cleanup

Goal: npm packages ship native Svelte support without external Svelte parser or
formatter package requirements.

Tasks:

- Remove Svelte parser/formatter package requirements from `oxfmt` runtime docs.
- Regenerate or update public config/types/docs that currently describe the old
  plugin-backed behavior, including `apps/oxfmt/src-js/config.generated.ts`,
  `apps/oxfmt/src/core/oxfmtrc.rs`, and migration docs such as
  `apps/oxfmt/MIGRATE_PRETTIER.md`.
- Remove `prettier-plugin-svelte`, `svelte-eslint-parser`, and
  `eslint-plugin-svelte` from runtime dependency expectations unless retained
  only for optional JS plugin rules.
- Ensure npm build workflows compile the rsvelte backend into all target
  packages.
- Verify macOS, Linux, and Windows package builds.
- Document whether `eslint-plugin-svelte` remains supported as a JS plugin layer
  or whether Svelte linting is native-only.

If JS plugin rules remain supported, `eslint-plugin-svelte` may still be a test
or optional user dependency, but it must not be the parser backend.

Current status:

- Runtime manifests no longer declare Svelte parser/formatter packages.
- Formatter schema JSON, generated TypeScript config, migration docs, and
  website schema snapshots are regenerated.
- Native macOS arm64 and Linux x64 release bindings build and execute locally.
  The Linux root and binding tarballs pass isolated formatter/linter smoke tests
  without any resolvable Svelte parser or formatter JS package.
- Windows x64 MSVC release bindings cross-build with `cargo-xwin`; PE inspection
  confirms the NAPI export, system imports, and compiled rsvelte paths.
- All six binding packages and both root packages pass
  `.github/scripts/check-npm-packages.js` with the same target set and wildcard
  layout as the publish workflow.
- Svelte linting is native-only. External JS rules may still be used for
  non-Svelte files, but they are never the native `.svelte` backend.

## Validation plan

Focused checks:

```bash
mise x -- cargo check -p oxfmt --all-features
mise x -- cargo check -p oxfmt --no-default-features
mise x -- cargo check -p oxfmt --features detect_code_removal
mise x -- cargo test -p oxc_svelte_backend --features rsvelte
mise x -- cargo test -p oxfmt
mise x -- cargo test -p oxc_linter svelte
```

Formatter checks:

```bash
mise x -- pnpm --filter oxfmt-app build-test
mise x -- pnpm --filter oxfmt-app test
mise x -- cargo test -p website_formatter
mise x -- cargo clippy -p oxfmt --all-targets --all-features --no-deps -- --deny warnings
mise x -- cargo clippy -p oxfmt --no-default-features --no-deps -- --deny warnings
```

Native formatter coverage must:

- require no `prettier-plugin-svelte` installation,
- cover supported syntax and explicit preserved-span boundaries,
- include snapshot or fixture output for representative Svelte files,
- verify CLI, API, stdin, walk, and LSP formatting all use the native path,
- format real-project copies, reparse them with the native backend, and pass a
  second `--check` run unchanged.

Removed formatter bridge inventory:

- `plugin_languages*`, `real_svelte*`, and `svelte_real_package*` tests only
  proved external plugin/package loading and were deleted.
- Native `api/svelte`, `cli/svelte`, `cli/stdin_svelte`, walk, and LSP format
  fixtures replace the retained product contracts.
- Generic external formatter plugin registry/spec tests remain, using a
  non-Svelte custom plugin.

Linter checks:

```bash
mise x -- pnpm --filter oxlint-app build-test
mise x -- pnpm --filter oxlint-app test
mise x -- cargo test -p oxc_linter svelte
```

Native linter coverage must:

- require no `svelte-eslint-parser` installation,
- assert rsvelte diagnostics are surfaced through `oxlint`,
- cover disable directives, comments, source ranges, embedded JS/TS, and
  explicit type-aware degradation,
- prove configured whole-file parsers and external JS rules are not invoked for
  native `.svelte` files.

The linter bridge inventory and accepted rule-coverage differences are recorded
in `docs/rsvelte-lint-coverage.md`.

Real project smoke tests:

- `../rukn`
- `../rsvelte/apps/playground`
- at least one larger Svelte app if available

For each project:

- run `oxlint` on `.svelte`, `.ts`, and `.js`,
- run `oxfmt --check`,
- format a copy and inspect representative diffs,
- verify no JS Svelte parser/formatter packages are required for the core path.

Real-project pass criteria and current evidence:

- Formatter: representative copies from Rukn and rsvelte's playground format,
  reparse with zero errors, contain no formatter sentinel text, and pass an
  idempotent `--check`. Original sibling worktrees are never modified.
- Linter: compare native rsvelte diagnostics against the previous JS-backed
  path. Every missing diagnostic must map to the lint coverage matrix as native
  equivalent, accepted gap, or intentional difference.
- Runtime dependencies: standalone JSON smoke configs import no Svelte JS
  packages. A deliberately missing formatter plugin spec is ignored by the
  native `.svelte` path.
- LSP/stdin/API: test at least one representative `.svelte` file through each
  entrypoint, not only through recursive CLI file walking.

## Risk register

Dependency skew:

The required rsvelte crates are vendored at immutable revision `672bb074` and
Cargo patches unify every used Oxc dependency with this workspace. Future pin
updates must repeat the dependency audit and native regression suite.

AST compatibility:

`eslint-plugin-svelte` rules expect `svelte-eslint-parser` node shapes and
services. The hard cut intentionally drops that bridge. Restoring external
template rules would violate the native-only decision; equivalent coverage must
be implemented through rsvelte diagnostics or native Rust rules.

Formatter completeness:

`rsvelte_formatter` is not behavior-compatible with `prettier-plugin-svelte`.
Unsupported spans are intentionally preserved, and the destructive open-tag
rewrite is disabled. Every expansion of native formatting scope must include
reparse and idempotence tests against real components.

Source locations:

Oxc, ESLint-compatible APIs, LSP, and Svelte syntax may disagree on byte offsets,
UTF-16 columns, and generated vs original spans. Make location conversion a
first-class adapter responsibility.

Parser services:

Parser services, style context helpers, and ESTree template traversal are not
provided. This is an accepted compatibility drop, not an unimplemented bridge.

Packaging:

The vendored path dependencies are reproducible from this repository and are
compiled into native artifacts. Darwin arm64, Linux x64 GNU, and Windows x64
MSVC artifacts have been built and inspected locally. Linux tarballs also pass
isolated runtime smoke tests; Windows artifacts are structurally inspected
because the local host cannot execute PE binaries.

Performance:

The rsvelte path should be faster than JS package fallbacks, but adapter
conversion can erase those gains if it serializes large ASTs through JSON or
duplicates source text.

## Release follow-up

1. Keep the completed commit stack local until a push is explicitly requested.
2. On the first requested publish run, confirm the native Windows runner loads
   the binding in Node; local verification can build and inspect the PE artifact
   but cannot execute it.
3. Use the normal fork publish workflow and inspect the resulting package set
   before assigning the requested npm dist-tag.

## Hard cutover exit criteria

- [x] `oxfmt` formats `.svelte` without `prettier-plugin-svelte`.
- [x] `oxlint` parses `.svelte` without `svelte-eslint-parser`.
- [x] `.svelte` lint and format use the native backend through applicable CLI,
  API, stdin, walk, and LSP surfaces.
- [x] Supported disable directives, comments, and diagnostics retain source
  locations. Template-node fixes/suggestions are an accepted dropped surface.
- [x] npm packages build for every publish target with the rsvelte backend
  included and pass artifact inspection.
- [x] Representative real Svelte project smoke tests pass on formatted copies.
- [x] Remaining behavior differences from Prettier or ESLint are documented as
  intentional rsvelte/Oxc behavior, not accidental fallback gaps.
