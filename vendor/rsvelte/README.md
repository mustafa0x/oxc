# Vendored rsvelte backend

This directory contains the runtime source needed by Oxc's native Svelte
backend, copied from:

- Repository: <https://github.com/mustafa0x/rsvelte>
- Revision: `672bb074b0faed092b4093d500fb3f02e94205ed`
- Crates: `svelte-compiler-rust` and `rsvelte_formatter`

The upstream apps, tests, benches, and nested Git submodules are intentionally
omitted. The root Oxc `Cargo.toml` patches rsvelte's Oxc dependencies to this
workspace so both projects compile against one set of Oxc crate types.

When updating the vendor, copy both crate manifests and runtime source trees,
retain the upstream `LICENSE`, update the revision above, and rerun the checks in
`docs/rsvelte-dependency-audit.md`.
