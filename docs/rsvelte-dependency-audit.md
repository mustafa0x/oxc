# rsvelte dependency audit

Date: 2026-07-10

Command:

```bash
mise x -- cargo tree -p oxlint --features svelte-rsvelte-backend -d
```

Summary:

- The rsvelte-enabled `oxlint` graph resolves Oxc workspace crates from this
  checkout for the core parser, AST, semantic, formatter, linter, diagnostics,
  and syntax crates.
- No `oxc_*` or `oxlint` package names resolve to multiple versions.

Resolved duplicate:

| Crate | Versions | Cause | Cutover action |
| --- | --- | --- | --- |
| `oxc_sourcemap` | `6.1.1`, `7.0.0` | `string_wizard@1.0.3` pulled `oxc_sourcemap@^6`; Oxc workspace crates use `oxc_sourcemap@7.0.0` | removed unused `string_wizard` in pinned rsvelte commit `672bb074` |

Original trace:

```text
oxc_sourcemap@6.1.1
  declared by string_wizard@1.0.3 req ^6

oxc_sourcemap@7.0.0
  declared by oxc_codegen@0.133.0 req ^7.0.0
  declared by oxc_minifier@0.133.0 req ^7.0.0
  declared by oxc_minify_napi@0.133.0 req ^7.0.0
  declared by oxc_transform_napi@0.133.0 req ^7.0.0
  declared by oxc_transformer_plugins@0.133.0 req ^7.0.0
```

Resolved source model:

- The source under `vendor/rsvelte` vendors `svelte-compiler-rust` and
  `rsvelte_formatter` from `mustafa0x/rsvelte` commit
  `672bb074b0faed092b4093d500fb3f02e94205ed`.
- That commit removes the unused `string_wizard` dependency, so
  `oxc_sourcemap@6.1.1` is absent from this fork's `Cargo.lock`.
- Builds no longer depend on a sibling `../rsvelte` checkout, Git submodule, or
  moving branch. The vendored tree omits rsvelte's apps, tests, benches, and
  unrelated nested repositories.
- A metadata audit after the removal reports no duplicate `oxc_*` or `oxlint`
  package names.

Phase 0 status:

- The hard-cutover "no duplicate Oxc crate versions" gate is satisfied for the
  rsvelte-enabled `oxlint` graph.

Validation:

```bash
mise x -- cargo check -p oxlint --features svelte-rsvelte-backend
mise x -- cargo check -p oxc_svelte_backend --features rsvelte
```

Both commands run from this Oxc checkout and resolve rsvelte from the pinned
source under `vendor/rsvelte`.
