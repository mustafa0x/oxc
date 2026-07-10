# `oxfmt --migrate prettier` compatibility notes

This file documents the current migration contract for Prettier configs, with special attention to Svelte projects.

## Lossless or near-lossless cases

String plugin specs and imported plugin objects are preserved when Oxfmt still
uses that external plugin. `prettier-plugin-svelte` is the exception: migration
drops it because `.svelte` formatting is owned by the native rsvelte backend.

## Converted or defaulted cases

Some Prettier concepts are intentionally migrated into the closest Oxfmt representation instead of being copied literally.

- `prettier-plugin-packagejson` becomes `sortPackageJson`
- if `printWidth` is omitted, migration writes `printWidth: 80` so the generated Oxfmt config keeps Prettier's default behavior
- Tailwind plugin options move into `sortTailwindcss`
- `svelteIndentScriptAndStyle` moves into the native Svelte config surface;
  `prettier-plugin-svelte` itself is omitted
- `svelteSortOrder` and `svelteAllowShorthand` are retained as compatibility
  options, but the native formatter currently preserves section order and open
  tags verbatim

These are not byte-for-byte copies, but they preserve the practical formatting behavior Oxfmt can express.

## Warning-only / partial-support cases

Some settings are copied, but migration emits a warning because Oxfmt support is still partial.

- `embeddedLanguageFormatting` values other than `"off"`
- `experimentalTernaries`
- `experimentalOperatorPosition`

Those warnings are there to make it clear the generated config may still need manual review.

## Skipped cases

Some inputs cannot be represented safely in `.oxfmtrc.json`, so migration leaves them out and prints a warning.

- custom plugin objects that do not expose a stable package spec Oxfmt can preserve
- `endOfLine: "auto"`
- Svelte formatting inside Markdown/MDX fences; the hard cut does not load a
  Prettier Svelte plugin for embedded snippets

The most common skipped case is a JS config that constructs or mutates a plugin object inline instead of referencing it via a stable package spec.

## Practical guidance for Svelte projects

If you want the most reliable migration result today:

- keep `svelteIndentScriptAndStyle` when you need unindented script bodies;
  style bodies are currently preserved verbatim
- remove `prettier-plugin-svelte` and `svelte` package dependencies after migration
- expect a short manual review whenever your Prettier config uses experimental options or inline custom plugin objects
