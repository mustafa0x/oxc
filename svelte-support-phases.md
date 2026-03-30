# Svelte support phases

This file tracks the work completed so far to add proper Svelte support to Oxlint/Oxfmt.

## Phase 0 — Native Svelte loader hardening

Status: ready to PR

Changes:
- tightened `partial_loader/svelte.rs` so `<script>` handling uses real attribute parsing instead of a broad `"ts"` substring check
- detect both `module` and `context="module"`
- infer TypeScript from `lang="ts"`
- added unit tests for module scripts, TS scripts, combined module+TS scripts, and false positives like `data-language="ts"`
- added a CLI integration fixture for `.svelte` files with module + instance TypeScript scripts

PR:
- **Title:** `fix(linter): detect Svelte TS and module scripts correctly`

## Phase 1 — Config blockers for Svelte migration

Status: ready to PR

Changes:
- added string/package entries to `extends` for `oxlint.config.ts`
- added category-level `"recommended"` support so categories can enable Oxlint's built-in recommended subset instead of every rule in the category
- preserved explicit category settings when later plugin/override processing runs
- updated JS config parsing tests, config typings, and builder coverage

PR:
- **Title:** `feat(config): support recommended categories and string extends in oxlint.config.ts`

## Phase 2 — External parser foundation for JS plugins

Status: ready to PR

Changes:
- added the generic external/custom parser foundation for JS plugins
- taught `RuleTester` to use `parseForESLint()` with `parse()` fallback
- plumbed parser-owned metadata for external ASTs (`parserServices`, `visitorKeys`, `scopeManager`)
- added normalization needed for external AST traversal
- added RuleTester coverage for direct external node visitors and selectors involving external-only ancestors

PR:
- **Title:** `feat(js-plugins): add custom parser foundation for RuleTester and external ASTs`

## Phase 3A — Preserve JS-config `languageOptions` through config resolution

Status: ready to PR in the current working tree

Changes:
- added a JS-side `languageOptions` registry to preserve non-serializable parser objects and parser options loaded from `oxlint.config.ts`
- normalized JS config output so root configs, nested `extends`, and overrides send internal `_languageOptionsId` markers to Rust instead of attempting to JSON-serialize parser objects
- added internal `_languageOptionsHasParser` tracking so Rust can tell whether the resolved JS-side `languageOptions` select a custom parser
- extended Rust config parsing to accept the internal language-options IDs and parser-presence flag on root configs and overrides
- carried resolved language-options IDs and parser-presence state through `LintConfig`, override application, and the external linter callback boundary
- exposed the resolved parser/parserOptions back to JS rules through `context.languageOptions`
- added tests for JS-config parsing of `_languageOptionsId` / `_languageOptionsHasParser`, merge behavior for parser-presence state, and ordered accumulation of override language-options IDs
- added a lightweight shared parser type for config typing so `defineConfig(...)` no longer depends on the full plugin runtime type graph

Notes:
- this does **not** yet select `svelte-eslint-parser` for whole `.svelte` files
- this is the plumbing needed before the whole-file Svelte parser lane can be added

PR:
- **Title:** `feat(js-config): preserve languageOptions across config resolution and JS linting`

## Phase 3B — Whole-file `.svelte` parser lane

Status: in progress; narrow plumbing slice is close to PRable

Changes:
- added Rust-to-JS transport for whole-file source text so JS plugins can lint files without relying on the raw-transfer AST buffer path
- shared external-rule diagnostic/fix handling between the raw-transfer path and the new whole-file source-text path
- taught the JS plugin runtime to run the configured custom parser against whole-file source text and populate `sourceCode.text`, `parserServices`, `visitorKeys`, and `scopeManager` from parser output
- added `setupExternalSourceForFile(...)` and external-parser source setup in the JS runtime so rules can operate on a custom-parser AST without a raw-transfer buffer
- added a canary fixture `apps/oxlint/test/fixtures/js_config_svelte_whole_file` proving a `.svelte` file can expose original markup and parser services to a JS plugin rule
- tightened the surrounding config tests while landing this work, including regression coverage for the custom-parser flag propagation

Notes:
- this slice still does **not** yet route the real CLI/runtime to `svelte-eslint-parser`; the fixture uses an inline parser stub to prove the whole-file lane works
- CFG listeners are still unsupported on the whole-file custom-parser path

## Phase 3C — Whole-file custom-parser routing for `.svelte` files without scripts

Status: in progress; narrow runtime slice is close to PRable

Changes:
- added an external-only runtime path so files with a configured custom parser can still run JS plugin rules even when the native partial loader produces no script sections
- added `ContextHost::new_external_only(...)` and `Linter::run_external_only_on_source_text(...)` so resolved settings, globals, diagnostics, and fixes still flow through the existing external-rule machinery
- wired `Runtime::run`, `run_source`, and `run_test_source` to use that path instead of returning early when a file has no native script sections
- added a `.svelte` no-script canary fixture proving the whole-file parser lane works for markup-only Svelte components

Notes:
- disable-directive matching for this no-script whole-file path is still incomplete because Rust does not yet receive parser comments from the external framework parser
- CFG listeners are still unsupported on the whole-file custom-parser path

## Phase 4 — Parser metadata/runtime integration

Status: partially done in Phase 2 and Phase 3B

Done already:
- external parser metadata plumbing exists in the JS runtime foundation work
- whole-file custom-parser runs now carry parser-owned metadata through the JS plugin runtime

Still needed for Svelte:
- full CLI/runtime integration for `.svelte`
- remaining traversal/runtime gaps once real Svelte parser fixtures are wired in

## Phase 4A — External parser comment/token `SourceCode` APIs

Status: ready to PR in the current working tree

Changes:
- added external-parser setup helpers in the JS runtime so whole-file custom-parser files populate `sourceCode` comment/token state without relying on the Rust raw-transfer buffer
- whole-file custom-parser runs now normalize and retain external `ast.comments` and `ast.tokens` (with fallback to parser-result-level `comments` / `tokens` metadata when present)
- existing `SourceCode` comment/token methods now work on externally parsed files through the same cached-object and merged-order machinery used by native files
- added a markup-only `.svelte` canary fixture proving `getAllComments`, `getCommentsBefore`, `getTokens`, `tokensAndComments`, and `ast.comments` / `ast.tokens` identity all work through the whole-file custom-parser lane

Notes:
- this improves JS-plugin compatibility for framework parsers that provide ESTree comments/tokens metadata
- disable-directive matching on the Rust side is still incomplete for whole-file framework parsers because Rust does not yet receive parser comments back from JS

PR:
- **Title:** `feat(js-plugins): support SourceCode comment and token APIs for whole-file custom parsers`

## Phase 5A — Type-aware parserOptions / `svelteConfig` passthrough canary

Status: ready to PR in the current working tree

Changes:
- added a new whole-file Svelte canary fixture `apps/oxlint/test/fixtures/js_config_svelte_type_aware_whole_file`
- the canary routes a `.svelte` file through the whole-file custom-parser lane with:
  - top-level Svelte parser on the override
  - nested TypeScript parser in `parserOptions.parser`
  - imported `svelteConfig` object in `parserOptions.svelteConfig`
  - `projectService` and `extraFileExtensions` on the matching override
- the fixture proves all of those values survive config loading and merge correctly across an object-style `extends` chain before reaching the parser and rule runtime
- the canary also proves non-serializable nested values are preserved, including parser functions and `svelteConfig.preprocess`
- added a direct JS unit test for `resolveLanguageOptionsIds(...)` covering deep parser-options merges and preservation of non-serializable nested values
- expanded `apps/oxlint/test/config.test-d.ts` with a more realistic type-aware Svelte config shape using object-style `extends`

Notes:
- this is still a canary/stub-parser slice; it does not yet run the real `svelte-eslint-parser` package end to end
- the runtime path now has explicit coverage for the data shape Svelte expects, which lowers the risk of the later real-parser integration work

PR:
- **Title:** `test(js-plugins): add type-aware Svelte parserOptions passthrough canaries`

## Phase 5B — Parser-provided scope methods for whole-file custom parsers

Status: ready to PR in the current working tree

Changes:
- taught `SourceCode` scope helper methods to prefer the parser-provided `scopeManager` when whole-file custom-parser files supply one
- `sourceCode.getScope(...)`, `getDeclaredVariables(...)`, `markVariableAsUsed(...)`, and `isGlobalReference(...)` now use the active parser scope manager instead of always rebuilding fallback scope data from the Oxc AST
- preserved the existing fallback TS-ESLint scope analysis path for native files and files without an external parser scope manager
- added a whole-file `.svelte` canary fixture `apps/oxlint/test/fixtures/js_config_svelte_scope_methods_whole_file`
- the canary proves whole-file custom-parser runs can use parser-provided scope data through:
  - `sourceCode.scopeManager`
  - `sourceCode.getScope(...)`
  - `sourceCode.getDeclaredVariables(...)`
  - `sourceCode.isGlobalReference(...)`
  - `sourceCode.markVariableAsUsed(...)`

Notes:
- this closes an important compatibility gap for framework parsers such as `svelte-eslint-parser`, which return a custom `scopeManager` for virtual/template scopes
- this is still a stub-parser canary slice; it does not yet run the real `svelte-eslint-parser` package end to end

PR:
- **Title:** `feat(js-plugins): use parser-provided scope managers for SourceCode scope APIs`

## Phase 5C — Whole-file disable directives from external parser comments

Status: ready to PR in the current working tree

Changes:
- extended the JS external-linter payload so whole-file custom-parser runs can round-trip parser comments back to Rust alongside diagnostics
- added Rust-side reconstruction of directive comment spans from external parser comments, including HTML comments like `<!-- eslint-disable-next-line ... -->`
- taught `handle_external_linter_result(...)` to consult those reconstructed whole-file directives before reporting JS-plugin diagnostics
- generalized `DisableDirectivesBuilder` with `build_raw_comments(...)` so external parser comments can reuse the existing directive parser without needing native Oxc `Comment` structs
- added a unit test proving `eslint-disable-next-line` works from an HTML-style comment
- added a markup-only `.svelte` fixture `apps/oxlint/test/fixtures/js_config_svelte_disable_directives_whole_file` proving a whole-file custom-parser diagnostic is suppressed by an HTML disable directive

Notes:
- this closes the main runtime gap where `.svelte` whole-file parser diagnostics ignored disable directives in template comments
- it still does not add Rust-side unused-disable reporting/fixes for whole-file framework-parser comments

PR:
- **Title:** `feat(js-plugins): respect whole-file disable directives from custom parser comments`

## Phase 5D — Package-shaped Svelte ecosystem whole-file canary

Status: ready to PR in the current working tree

Changes:
- added a new whole-file Svelte canary fixture `apps/oxlint/test/fixtures/js_config_svelte_package_ecosystem_whole_file`
- the fixture uses package-shaped local `node_modules` entries for both `svelte-eslint-parser` and `eslint-plugin-svelte`, instead of inline stubs
- the config imports the parser from the package and extends a recommended config object exported by the plugin package
- the plugin rule proves package-loaded framework integrations can see the Svelte-specific parser surface Oxlint needs to preserve, including:
  - `parserServices.isSvelte`
  - `parserServices.svelteParseContext`
  - `parserServices.getStyleContext()`
  - `context.settings.svelte.compileOptions`
  - `context.settings.svelte.kit`
  - nested `parserOptions.parser`
  - `projectService`
  - `extraFileExtensions`
  - imported `svelteConfig.preprocess`
- the fixture also proves package-name normalization still yields the expected `svelte/valid-compile` rule ID on the whole-file custom-parser path

Notes:
- this is still a package-shaped canary, not the real upstream `svelte-eslint-parser` / `eslint-plugin-svelte` packages
- it is the first end-to-end fixture in this stack that exercises the same import and package-resolution shape real Svelte projects use

PR:
- **Title:** `test(js-plugins): add package-shaped Svelte ecosystem whole-file canary`

## Phase 5E — Report unused whole-file disable directives from external parser comments

Status: ready to PR in the current working tree

Changes:
- extended raw directive comment handling so whole-file custom-parser comments can keep both the full outer comment span and the inner content span used for rule-name parsing
- whole-file external parser comments now build disable directives with the original HTML comment span, so unused-directive diagnostics and suggested removals target the full `<!-- ... -->` comment
- `run_external_rules_on_source_text(...)` now returns reconstructed whole-file disable directives after it uses them to suppress JS-plugin diagnostics
- whole-file custom-parser linting now reports unused disable/enable directives in both paths:
  - files with native script sections plus whole-file JS-plugin parsing
  - external-only files with no native script sections
- added disable-directive tests proving HTML comment directives preserve the outer comment span for unused-directive reporting
- added a new `.svelte` CLI canary fixture `apps/oxlint/test/fixtures/js_config_svelte_unused_disable_directives_whole_file`
  with `--report-unused-disable-directives` coverage for a markup-only whole-file parser run

Notes:
- this closes the remaining practical gap where whole-file `.svelte` parser comments could suppress diagnostics but could not themselves be reported as unused
- the fixture currently snapshots warning output; it does not yet add an end-to-end `--fix-suggestions` canary for removing the unused comment

PR:
- **Title:** `feat(js-plugins): report unused whole-file disable directives from external parser comments`


## Phase 5F — Whole-file `.svelte` fixes and fix-suggestions canary

Status: ready to PR in the current working tree

Changes:
- added a new package-shaped whole-file Svelte fixture `apps/oxlint/test/fixtures/js_config_svelte_fixes_suggestions_whole_file`
- the fixture uses local `svelte-eslint-parser` and `eslint-plugin-svelte` packages, mirroring the package import shape real Svelte projects use
- the parser returns external-only Svelte template nodes (`SvelteClassName` and `SvelteText`) through the whole-file custom-parser lane
- the plugin exposes a single rule that has both safe fixes and suggestions, so the fixture now snapshots all three modes on `.svelte` input:
  - normal lint output
  - `--fix`
  - `--fix-suggestions`
- the fix canary proves a whole-file custom-parser rule can safely rewrite original Svelte markup ranges
- the suggestion canary proves `--fix-suggestions` applies the first suggested edit on the original whole-file `.svelte` source text

Notes:
- this is still a package-shaped canary, not the real upstream `svelte-eslint-parser` / `eslint-plugin-svelte` packages
- the value of this slice is locking down edit application on the whole-file framework-parser lane, which had runtime coverage but no dedicated Svelte fix snapshots yet

PR:
- **Title:** `test(js-plugins): add whole-file Svelte fix and fix-suggestions canary`

## Phase 5G — Pass ESLint parser feature-detection flags on whole-file Svelte parser runs

Status: ready to PR in the current working tree

Changes:
- taught whole-file custom-parser calls to always pass `eslintVisitorKeys: true` and `eslintScopeManager: true` alongside `filePath`
- mirrored the same behavior in `RuleTester`, so custom-parser tests see the same parser-call contract as the runtime
- added RuleTester coverage proving those flags are present even if user-supplied `parserOptions` tried to set them to `false`
- added a new package-shaped whole-file Svelte canary fixture `apps/oxlint/test/fixtures/js_config_svelte_parser_feature_flags_whole_file`
- the canary proves a package-loaded Svelte parser sees:
  - `eslintVisitorKeys: true`
  - `eslintScopeManager: true`
  - the expected `filePath`
- the canary also proves the file still reaches the whole-file parser lane and reports through a package-loaded Svelte rule

Notes:
- this is a small but important compatibility slice for ESLint custom parsers that use those parserOptions flags for feature detection
- the goal is to make the generic whole-file parser lane look more like normal ESLint, not to add another Svelte-specific special case

PR:
- **Title:** `feat(js-plugins): pass ESLint parser feature-detection flags to whole-file custom parsers`

## Phase 5H — Pass core AST metadata flags to whole-file custom parsers

Status: ready to PR in the current working tree

Changes:
- added a shared `createRequiredParserCallOptions(...)` helper so the runtime and `RuleTester` no longer drift on custom-parser call options
- whole-file custom-parser calls now always force the core AST metadata flags ESLint-style parsers commonly rely on:
  - `loc: true`
  - `range: true`
  - `raw: true`
  - `comment: true`
  - `tokens: true`
- mirrored the same behavior in `RuleTester`, including overriding user-supplied `false` values for those flags
- added `RuleTester` coverage proving those AST metadata flags are always passed to custom parsers
- added a new package-shaped whole-file Svelte canary fixture `apps/oxlint/test/fixtures/js_config_svelte_parser_ast_metadata_flags_whole_file`
- the canary proves a package-loaded Svelte parser sees all five flags and can return parser-generated comments/tokens that flow through `SourceCode` APIs on the whole-file lane

Notes:
- this is a generic custom-parser compatibility improvement, not a Svelte-only special case
- it makes the whole-file parser lane look more like normal ESLint for parsers that gate comment/token/raw/loc/range output on parser-call options

PR:
- **Title:** `feat(js-plugins): pass core AST metadata flags to whole-file custom parsers`

## Phase 5 — Type-aware Svelte support

Status: in progress

Remaining scope:
- wire the real `svelte-eslint-parser` package into end-to-end canaries
- carry any remaining parser-specific settings required by real Svelte ecosystem rules
- add stronger coverage once actual type-aware rules are exercised instead of stub-parser canaries
- finish any remaining whole-file runtime gaps that only surface with real Svelte parser output

## Phase 6 — Formatter support

Status: in progress

Planned scope:
- route `.svelte` formatting through formatter plugin language resolution
- integrate `prettier-plugin-svelte` style plugin discovery on the formatter side

## Phase 6A — Oxfmt plugin language discovery and plugin-option passthrough

Status: ready to PR in the current working tree

Changes:
- added `ExternalPluginSupport` in Oxfmt so external formatter plugin languages can advertise parser names by extension and exact filename
- added `FormatFileStrategy::from_path_with_external_support(...)` and taught CLI/stdin/LSP/API entry points to use plugin language support when choosing how to format a file
- changed external formatter initialization so Rust asks JS to resolve a specific list of configured plugin specs and receive their serialized `languages` metadata back
- preserved top-level external formatter config fields when building Prettier options, instead of dropping unknown plugin-owned fields during `FormatConfig` serialization
- resolved relative plugin paths against the config directory (or API cwd) and preserved them in external formatter options so workers can load the actual plugins during formatting
- taught the JS formatter runtime to load configured plugins from `options.plugins` before formatting, while still separately resolving plugin language metadata for the Rust-side file walker
- added Rust tests for:
  - plugin-spec path resolution
  - preserving `plugins` and plugin-owned top-level options in external formatter options
  - routing `.svelte` files through plugin language metadata
- added new CLI and API canaries using a local fake `prettier-plugin-svelte` package/file:
  - the CLI canary proves `.svelte` files can be discovered and formatted from config-loaded plugin languages
  - the API canary proves direct `format("App.svelte", ...)` works with `plugins: [pluginPath]`
  - both canaries prove a plugin-owned option (`svelteSortOrder`) survives into the formatter call and affects output

Notes:
- this slice is formatter-side plumbing; it does not add a real upstream `prettier-plugin-svelte` dependency
- the fake Svelte plugin is intentionally minimal and exists only to lock down file-type discovery and plugin-option passthrough behavior

PR:
- **Title:** `feat(oxfmt): detect plugin-defined file types and preserve plugin options`


## Phase 6B — Package-name formatter plugin resolution from project `node_modules`

Status: ready to PR in the current working tree

Changes:
- stopped treating every plugin spec containing `/` or `\` as a filesystem path during formatter config normalization
- relative and absolute filesystem plugin paths are still normalized, but package names, scoped packages, and package subpaths are now preserved as package specs
- encoded package-style plugin specs with an internal `resolveFrom` base directory so the JS formatter runtime can resolve them from the project/config directory instead of Oxfmt’s own module location
- taught the JS formatter runtime to decode those internal plugin specs and resolve them with `createRequire(...)` before importing the plugin module
- added Rust tests covering:
  - relative path plugin resolution
  - bare package name encoding
  - scoped package encoding
  - package-subpath encoding
  - preserving encoded package specs in external formatter options
- added a new CLI canary fixture `apps/oxfmt/test/cli/plugin_languages_package`
  using a package-shaped local `node_modules/prettier-plugin-svelte`
- added a matching API canary proving `format("App.svelte", ..., { plugins: ["prettier-plugin-svelte"] })` works when the package is installed in the current project

Notes:
- this closes a real ecosystem gap: package-name plugins were previously imported relative to Oxfmt’s own code, which could miss the target project’s local `node_modules`
- this is still a local package-shaped canary, not the real upstream `prettier-plugin-svelte` package

PR:
- **Title:** `feat(oxfmt): resolve package-style formatter plugins from project node_modules`


## Phase 6C — Formatter override-scoped plugin discovery and config-dir package-subpath canary

Status: ready to PR in the current working tree

Changes:
- extended formatter plugin-spec extraction so Oxfmt now discovers plugin specs declared inside `.oxfmtrc` `overrides[].options.plugins`, not just top-level `plugins`
- deduplicated extracted plugin specs while preserving first-seen order, so the formatter-side language resolver can initialize every plugin language once even when the same package appears in multiple overrides
- preserved raw override-only external formatter options during per-file resolution by storing each override’s original `options` object alongside the typed `FormatConfig`
- when a file matches formatter overrides, Oxfmt now merges the matching raw override `options` into the external Prettier option object before applying typed merged options, so plugin-owned settings such as `plugins` and `svelteSortOrder` survive override resolution
- added Rust tests covering:
  - collecting package-style plugin specs from override options
  - merging raw override plugin options into resolved external formatter options with the correct `resolveFrom` base directory
- added a new CLI canary fixture `apps/oxfmt/test/cli/plugin_languages_override_package_subpath`
  that proves all of the following at once:
  - `.svelte` discovery works when the plugin is declared only inside an override
  - package subpath specs like `prettier-plugin-svelte/subpath` are preserved as package specs rather than being mistaken for file paths
  - the plugin is resolved from the nested config directory via local `node_modules`
  - override-only plugin options survive into the formatter call and affect output

Notes:
- this closes a real compatibility gap for Prettier-style configs that scope framework plugins and plugin-owned options to `*.svelte` overrides instead of placing them at the config root
- the canary still uses a local package-shaped fake Svelte plugin; it does not yet depend on the real upstream `prettier-plugin-svelte` package

PR:
- **Title:** `feat(oxfmt): honor override-scoped formatter plugins and package subpaths`


## Phase 6D — Direct JS API support for imported formatter plugin objects

Status: ready to PR in the current working tree

Changes:
- added a JS-side formatter plugin registry (`apps/oxfmt/src-js/plugin_registry.ts`) that can replace direct Prettier plugin objects with internal marker strings before options cross the Rust NAPI boundary
- taught the JS formatter runtime (`apps/oxfmt/src-js/libs/apis.ts`) to recognize those registered plugin markers and rehydrate them back into real plugin objects when resolving plugin languages and when preparing `options.plugins` for Prettier
- taught the public JS API entry point (`apps/oxfmt/src-js/index.ts`) to normalize direct `format(..., { plugins: [pluginObject] })` calls through that registry
- preserved registered plugin markers on the Rust side instead of accidentally rewriting them as package specs during external-plugin extraction / option normalization
- added Rust tests covering:
  - preserving registered plugin markers during plugin-spec extraction
  - preserving registered plugin markers in resolved external formatter options
- added JS coverage for:
  - direct API formatting of `.svelte` with an imported plugin object (`apps/oxfmt/test/api/plugin_object.test.ts`)
  - registry normalization / lookup behavior (`apps/oxfmt/test/plugin_registry.test.ts`)

Notes:
- this slice currently targets the direct JS API path (`format()`), where the same JS process owns both the plugin registry and the formatter runtime
- it does **not** extend `oxfmt.config.ts` / CLI worker-process formatting to imported plugin objects yet, because child-process formatter workers cannot safely receive arbitrary plugin objects over IPC
- package/path string plugin specs continue to be the supported route for CLI/LSP/stdin formatting

PR:
- **Title:** `feat(oxfmt): support direct formatter plugin objects in the JS API`


## Phase 6E — Preserve Svelte formatter plugins when migrating Prettier configs

Status: ready to PR in the current working tree

Changes:
- updated `apps/oxfmt/src-js/cli/migration/migrate-prettier.ts` so `--migrate prettier` no longer drops all non-internal plugin strings
- `prettier-plugin-tailwindcss` is still migrated into `sortTailwindcss`
- `prettier-plugin-packagejson` is still migrated into `sortPackageJson`
- all other string plugin specs are now preserved in the generated `.oxfmtrc.json`, including:
  - package names like `prettier-plugin-svelte`
  - relative plugin paths
  - package subpaths / scoped package specs
- duplicate preserved plugin specs are deduplicated while keeping first-seen order
- switched the top-level `Options` import in the migration file to `import type` so the module can be loaded for syntax-only validation without requiring Prettier at module-evaluation time
- added CLI migration coverage proving a Svelte-style Prettier config with:
  - `prettier-plugin-svelte`
  - `prettier-plugin-tailwindcss`
  - extra preserved plugin specs
  migrates to an `.oxfmtrc.json` that keeps the Svelte/plugin entries while still translating Tailwind and package-json behavior

Notes:
- this closes a real migration gap for Svelte projects: Oxfmt now supports package/path formatter plugins, so dropping `prettier-plugin-svelte` during `--migrate prettier` would produce a broken config
- Tailwind is still handled via Oxfmt's internal `sortTailwindcss` path, which matches the existing migration behavior

PR:
- **Title:** `feat(oxfmt): preserve supported formatter plugins in --migrate prettier`

## Phase 6F — Migrate JSON-based Prettier overrides for Svelte/plugin configs

Status: ready to PR in the current working tree

Changes:
- taught `oxfmt --migrate prettier` to read raw JSON-like Prettier configs (`.prettierrc`, `.prettierrc.json*`, `prettier.config.json*`, and `package.json#prettier`) in addition to the already-resolved top-level config
- added override migration for JSON-based Prettier configs, so `overrides[].files`, `excludeFiles`, and `options` now survive into `.oxfmtrc.json`
- normalized single-string `files` / `excludeFiles` patterns into the array form Oxfmt expects
- reused the existing plugin migration logic inside override `options`, so override-scoped plugin behavior now migrates correctly:
  - `prettier-plugin-svelte` and other supported string plugin specs are preserved
  - `prettier-plugin-tailwindcss` is migrated into `sortTailwindcss`
  - `prettier-plugin-packagejson` is migrated into `sortPackageJson`
- added CLI migration coverage for:
  - a `.prettierrc` with Svelte override plugins and Tailwind/packagejson migration inside overrides
  - a `package.json#prettier` config with Svelte overrides

Notes:
- this targets JSON-like Prettier config formats only; JS/YAML Prettier configs still do not have automatic override migration
- this closes an important Svelte migration gap because many real projects scope `prettier-plugin-svelte` and related plugin options to `*.svelte` overrides instead of the config root

PR:
- **Title:** `feat(oxfmt): migrate JSON-based Prettier overrides for Svelte and plugin configs`


## Phase 6G — Migrate JS-based Prettier overrides for Svelte/plugin configs

Status: ready to PR in the current working tree

Changes:
- taught `oxfmt --migrate prettier` to load raw JS-based Prettier config files when preserving overrides, covering:
  - `.prettierrc.js`
  - `.prettierrc.cjs`
  - `.prettierrc.mjs`
  - `prettier.config.js`
  - `prettier.config.cjs`
  - `prettier.config.mjs`
- added raw JS-config loading via dynamic module import so override arrays survive instead of being flattened away by the already-resolved top-level config
- supported both CommonJS and ESM object exports for raw override migration
- kept the existing override migration behavior once the raw config object is loaded, including:
  - preserving `prettier-plugin-svelte` and other supported string plugin specs
  - translating `prettier-plugin-tailwindcss` into `sortTailwindcss`
  - translating `prettier-plugin-packagejson` into `sortPackageJson`
- added CLI migration coverage for:
  - a CommonJS `.prettierrc.cjs` with Svelte override plugins and Tailwind option migration
  - an ESM `prettier.config.mjs` with Svelte override plugins, package-subpath preservation, and package-json migration inside the override

Notes:
- this closes the JS-config half of the remaining override migration gap; YAML-based Prettier overrides are still not migrated automatically
- the new raw JS loader currently targets config files that export object-shaped configs (the common Prettier config pattern)

PR:
- **Title:** `feat(oxfmt): migrate JS-based Prettier overrides for Svelte and plugin configs`
