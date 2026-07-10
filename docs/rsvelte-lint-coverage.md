# rsvelte lint coverage matrix

Date: 2026-07-10

This matrix tracks the lint coverage that must be accepted before the Svelte
path can hard-cut from `svelte-eslint-parser` to the native `rsvelte` backend.
It is scoped to `oxlint`.

The removed JS-backed baseline used `svelte@5.55.1`,
`svelte-eslint-parser@1.5.1`, and `eslint-plugin-svelte@3.16.0`. Its fixtures
were removed when the bridge was cut; the behavior differences below preserve
the inventory needed to decide which signals should later become native rules.

## Decision Summary

Native `.svelte` linting currently covers:

- rsvelte parser and analyzer errors surfaced as `svelte(<code>)`;
- rsvelte compiler/analyzer warnings, including implemented accessibility
  warnings, surfaced with stable source ranges;
- embedded module and instance `<script>` blocks through existing Oxc JS/TS
  rules;
- Svelte/HTML `oxlint-disable` and `eslint-disable` directives for full-file
  spans and native Svelte parse diagnostics;
- JS comment disable directives inside extracted script blocks;
- explicit type-aware degradation for `.svelte` files in CLI and LSP.

The hard cut does not retain the JS Svelte plugin bridge. When the native
backend is enabled, `.svelte` files never invoke a configured whole-file JS
parser or external JS rules. Whole-file custom parsers remain supported for
non-Svelte extensions.

Native `.svelte` linting intentionally does not cover:

- `eslint-plugin-svelte` recommended template rules;
- Svelte-specific fixes or suggestions over template AST nodes;
- parser-service compatibility for JS plugin rules without
  `svelte-eslint-parser`;
- template-to-script symbol usage mapping for rules that currently skip
  `.svelte`.

These are accepted hard-cut behavior differences. New Svelte template lint
coverage must be implemented as rsvelte compiler/analyzer diagnostics or native
Oxc Rust rules over rsvelte data; it must not restore `svelte-eslint-parser` as
the backend.

## Matrix

| Signal | Current JS-backed source | Native rsvelte/Oxc source | Severity | Fixes | Status |
| --- | --- | --- | --- | --- | --- |
| Svelte parser and analyzer errors | `svelte-eslint-parser` parse errors and `svelte/valid-compile` style behavior | `oxc_svelte_backend::parse_svelte` runs rsvelte parse and analysis, then converts failures to `svelte(<code>)` diagnostics | error | no | covered for diagnostics implemented by the pinned rsvelte revision |
| `svelte/no-useless-mustaches` | Pinned real recommended fixture reports `svelte/no-useless-mustaches` for `{"hello"}` | none yet | error | upstream has fixes for some cases | accepted drop for hard cut; native Rust rule is a follow-up |
| `svelte/valid-compile` and compiler diagnostics | Historical/recommended Svelte plugin signal, depending on plugin version/config | rsvelte analysis and no-codegen transform pass; warnings retain rsvelte codes and ranges | warning/error from compiler | no | covered for diagnostics implemented by the pinned rsvelte revision; parity gaps remain rsvelte gaps |
| Svelte/HTML disable directives | External parser comments from `svelte-eslint-parser` | HTML comments collected from rsvelte parse payload, with source scan fallback for parse errors | n/a | unused-directive suggestions not wired for file-level HTML comments | covered for suppressing/reporting; richer fixes pending |
| JS disable directives in scripts | JS comments from embedded script AST | existing partial script lint path | n/a | existing JS directive fixes | covered |
| Module and instance script lint | `svelte-eslint-parser` script AST/services | existing Oxc partial script extraction after rsvelte parse gate | configured rule severity | existing JS fixes | covered for JS/TS script rules |
| Comments and tokens for JS plugin source APIs | `svelte-eslint-parser` `SourceCode` comments/tokens | rsvelte adapter exposes comments for native directives; no ESTree source-code bridge | n/a | n/a | accepted drop; JS Svelte plugin bridge removed |
| Parser services | `parserServices.isSvelte`, Svelte context, style context, parser options | none in native path | n/a | n/a | accepted drop; JS Svelte plugin bridge removed |
| Fixes/suggestions over template nodes | `eslint-plugin-svelte` rules over Svelte ESTree nodes | none | rule-specific | no | accepted drop until native rules provide their own fixes |
| Type-aware Svelte | `svelte-eslint-parser` plus TypeScript parser/project service | explicitly unsupported; `.svelte` is excluded from type-aware checks | info/degraded | no | accepted temporary gap; requires `svelte2tsx` remapping to support |
| CLI file walking | normal CLI path | rsvelte parse gate plus Oxc script linting | configured | existing JS fixes | covered |
| CLI stdin | no current `oxlint` stdin CLI surface found | n/a | n/a | n/a | not applicable unless a stdin lint surface is added |
| LSP diagnostics | LSP run source path | same runtime path; has rsvelte parse diagnostics and type-aware degradation notice | configured/info | code actions exist | covered for current native diagnostics |

## Existing Native Rule Skips

These built-in Oxc rules intentionally skip `.svelte` today because template
usage is not visible from the extracted JS/TS AST alone:

| Rule | Current behavior | Cutover requirement |
| --- | --- | --- |
| `eslint/no-unused-vars` | skips `.svelte`, `.vue`, `.astro`, and `.d.ts` | keep skipped until rsvelte maps template identifier usage to script symbols |
| `typescript/consistent-type-imports` | skips `.svelte`, `.vue`, and `.astro` | keep skipped until type-only usage can be proven across template and script |
| `react/rules-of-hooks` | skips `.svelte` and `.vue` | keep skipped; Svelte `use*` calls are not React hooks |
| `eslint/no-unused-labels` | skips `.svelte` | re-evaluate once rsvelte script/template ranges are the only Svelte parse source |
| `unicorn/no-empty-file` | does not run for partial-loader extensions, including `.svelte` | define native empty-template behavior before enabling |

## Required Follow-ups

1. Expand native fixtures as additional rsvelte compiler/analyzer diagnostics
   are accepted as replacements for `svelte/valid-compile`.
2. Port `svelte/no-useless-mustaches` as a native rule if that signal is still
   desired; its JS-plugin implementation is intentionally not part of the hard
   cut.
3. Convert or remove bridge-only fixtures that assert
   `svelte-eslint-parser`, parser services, external template traversal, or JS
   template fixes.
4. Keep the built-in rule skips until there is tested template-to-script usage
   mapping from rsvelte.

## Removed Bridge Fixture Inventory

The hard cut removed the branch-added `js_config_svelte_*_whole_file` fixtures
that exercised these JS-only contracts:

- parser flags, parser metadata, parser services, scope methods, comments, and
  tokens;
- no-script template traversal and template parse errors from a custom parser;
- external plugin fixes, suggestions, disable directives, and unused-directive
  accounting;
- real `svelte-eslint-parser`, `eslint-plugin-svelte` recommended rules, and
  type-aware nested TypeScript parser services.

Native replacements retained in the suite are
`svelte_native_rsvelte_backend`, `svelte_native_type_aware`, the Rust CLI/LSP
fixtures under `apps/oxlint/fixtures`, and generic custom-parser tests using
non-Svelte extensions.
