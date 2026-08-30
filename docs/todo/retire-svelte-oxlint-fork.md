# Retire the Svelte Oxlint fork

Date: 2026-08-30

Status: pending consumer validation

## Context

Rsvelte issue [#1732](https://github.com/baseballyama/rsvelte/issues/1732) is complete. `@rsvelte/lint` now provides Svelte-aware `svelte/no-undef` and `svelte/no-unused-vars`; the former shipped in `0.10.12`, and the latest verified release is `0.10.20`.

This removes the known scope-sensitive blocker to using upstream Oxlint for JavaScript and TypeScript while standalone `rsvelte-lint` handles Svelte files. It does not by itself prove parity for every Oxlint core rule currently run inside this fork's extracted `<script>` blocks. The formatter fork is also a separate decision.

## Validation plan

1. In a representative consumer such as Rawy or Quizzer, replace `@mustafaj/oxlint` with upstream `oxlint` and add `@rsvelte/lint@0.10.20`.
2. Run upstream Oxlint on JavaScript and TypeScript and standalone `rsvelte-lint` on `.svelte` files.
3. Explicitly enable the two Svelte-aware rules in `rsvelte-lint.json`:

   ```json
   {
     "extends": ["recommended"],
     "rules": {
       "svelte/no-undef": "error",
       "svelte/no-unused-vars": "error"
     },
     "env": {
       "browser": true
     }
   }
   ```

   Carry over each consumer's required environments and configured globals.
4. Compare the upstream split setup with the current fork using valid stores, runes, template references, component imports, module-to-instance references, configured globals, and genuinely undefined or unused identifiers.
5. Compare both setups against the consumer's existing lint baseline and inventory any other Oxlint core rules expected to run inside Svelte `<script>` blocks.
6. If parity is acceptable, remove `@mustafaj/oxlint`, update lint scripts and CI, and mark `docs/todo/rsvelte-lint-stock-oxlint-semantic-gap.md` as superseded.
7. Keep the generic Oxc partial-file semantic-overlay proposal only if a single Oxlint process or broader component-aware core-rule coverage remains a requirement.

## Boundaries

- Prefer the standalone `rsvelte-lint` CLI for the validation.
- Do not retire `@mustafaj/oxfmt` based on this result; validate formatter replacement separately.
- Do not retire the lint fork until the representative consumer fixture and diagnostic comparison passes.

## AI assistance disclosure

This plan was prepared with AI assistance. A human contributor must review and understand the validation results before using them to retire or publish packages.
