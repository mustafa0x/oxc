# rsvelte dependency audit

Date: 2026-07-10

Source pin: `mustafa0x/rsvelte@93eac0b77c5fb3bef56a91bfde0b6fc9e939d623`

Command:

```bash
mise x -- cargo tree -p oxlint --features svelte-rsvelte-backend -d
```

## Vendored source

The source under `vendor/rsvelte` contains the three crates required by this
fork:

- `rsvelte_core` 0.7.16
- `rsvelte_formatter` 0.1.0
- `rsvelte_esrap` 0.7.11

The vendored tree omits rsvelte's apps, tests, benches, and unrelated nested
repositories. Builds do not depend on a sibling checkout, Git submodule, or
moving branch.

## Oxc graph

The vendored manifests point directly at this workspace's Oxc crates. The
rsvelte-enabled graph therefore uses one version of each Oxc package name:

- core parser, AST, semantic, codegen, span, syntax, and allocator crates use
  the workspace 0.139 packages;
- formatter, formatter-core, formatter-css, and formatter-json use the
  workspace 0.58 packages;
- `oxc_sourcemap` resolves to 8.1.0.

`cargo tree -d` still reports normal third-party version duplication and some
host/target instances of the same workspace package. It does not report two
versions of any `oxc_*` or `oxlint` package name.

The old `string_wizard` dependency remains absent, so the historical
`oxc_sourcemap` 6.x duplication does not return.

## Compatibility adaptations

The rsvelte pin was written against the immediately preceding Oxc AST builder
API. This fork carries two narrow adaptations:

- a copyable allocator-backed builder provider implements Oxc's current
  `GetAstBuilder`, `AstBuild`, and `GetAllocator` traits for rsvelte transform
  helpers;
- the formatter removes Oxc's synthetic leading ASI guard semicolon after
  formatting wrapped Svelte attribute expressions.

These adaptations are covered by the backend build, component reparse test,
and formatter idempotence test. They must be re-audited whenever either Oxc or
the rsvelte pin changes.

## Validation

```bash
mise x -- cargo check -p oxc_svelte_backend --features rsvelte
mise x -- cargo test -p oxc_svelte_backend --features rsvelte
mise x -- cargo check -p oxlint --features svelte-rsvelte-backend
mise x -- cargo tree -p oxlint --features svelte-rsvelte-backend -d
```

All commands resolve rsvelte from `vendor/rsvelte` and Oxc crates from this
checkout.
