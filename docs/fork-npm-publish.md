# Fork npm Publish Runbook

This branch publishes forked `oxlint` and `oxfmt` packages under `@mustafaj`
from `.github/workflows/npm-publish.yml`.

The fork publish matrix intentionally targets only Apple ARM64 and Linux x64.
Windows packages are disabled because their build consistently dominates the
release loop; add Windows back to both the matrix and `FORK_NAPI_TARGETS` if it
becomes a supported consumer platform.

## Required Tool Pins

The workflow depends on `mise.toml` being present in the repository:

```toml
[tools]
rust = "1.97.0"
node = "24.14.0"
pnpm = "11.9.0"
```

If publishing suddenly fails with missing `pnpm`, an older Node, or
`node: bad option: --experimental-strip-types`, first verify that `mise.toml`
is tracked and present on the branch.

## Rebase / Clean-Port Check

When rebasing or clean-porting the Svelte branch, check for branch-only files
that were added on the old branch but are absent from the new branch:

```bash
base=$(git merge-base OLD_HEAD origin/main)
comm -23 \
  <(git diff --name-only --diff-filter=A "$base"..OLD_HEAD | sort) \
  <(git ls-tree -r --name-only HEAD | sort)
```

In the April 2026 port, this caught `mise.toml`. It had been dropped during a
clean port, which made the publish workflow fail even though the workflow logic
itself was mostly fine. `mise.toml` is no longer listed in `.git/info/exclude`,
so it should now appear normally in `git status` if missing or modified.

## Pick a Version Suffix

Do not reuse a suffix after any package has partially published. npm versions
are immutable.

Known partial publish:

- `0.46.0-svelte.0` exists for some `@mustafaj/oxfmt-binding-*` packages, but
  the root `@mustafaj/oxfmt@0.46.0-svelte.0` was not published.

Use a fresh suffix for each attempt, for example `-svelte.1`, `-svelte.2`, etc.

Before dispatching, spot-check that the target suffix is free:

```bash
npm view @mustafaj/oxlint@1.61.0-svelte.N version
npm view @mustafaj/oxfmt@0.46.0-svelte.N version
npm view @mustafaj/oxlint-binding-darwin-arm64@1.61.0-svelte.N version
npm view @mustafaj/oxfmt-binding-darwin-arm64@0.46.0-svelte.N version
```

`E404` means that specific package version is still free.

## Publish

Dispatch the workflow from the `svelte` branch:

```bash
gh workflow run npm-publish.yml \
  --repo mustafa0x/oxc \
  --ref svelte \
  -f packages=oxlint,oxfmt \
  -f scope=@mustafaj \
  -f version_suffix=-svelte.N \
  -f npm_tag=svelte
```

The native backend's rsvelte source is pinned under `vendor/rsvelte`.
Publishing does not require a sibling `../rsvelte` checkout, Git submodules, or
a separate workflow input. Update the vendored source and its recorded revision
deliberately when adopting a new rsvelte version, then run the Rust and npm
validation before publishing.

Monitor it:

```bash
gh run list --repo mustafa0x/oxc --workflow npm-publish.yml --branch svelte --limit 3
gh run view RUN_ID --repo mustafa0x/oxc --json status,conclusion,url,jobs
```

`gh run watch` is convenient, but it can fail on transient GitHub API/socket
errors. If that happens, the workflow may still be fine; switch to periodic
`gh run view` checks.

## Verify npm

After a successful run, check that the `svelte` dist-tags moved:

```bash
npm view @mustafaj/oxlint version dist-tags
npm view @mustafaj/oxfmt version dist-tags
```

Expected shape:

```text
dist-tags = { svelte: "1.61.0-svelte.N", latest: "..." }
dist-tags = { svelte: "0.46.0-svelte.N", latest: "..." }
```

Also verify at least one native binding package for each app:

```bash
npm view @mustafaj/oxlint-binding-darwin-arm64@1.61.0-svelte.N version
npm view @mustafaj/oxfmt-binding-darwin-arm64@0.46.0-svelte.N version
```

## Consumer Smoke Test

To test in a consumer repo without changing `package.json` or lockfiles, use
`pnpm dlx --package`:

```bash
pnpm dlx --package @mustafaj/oxlint@svelte oxlint --version
pnpm dlx --package @mustafaj/oxfmt@svelte oxfmt --version
pnpm dlx --package @mustafaj/oxlint@svelte oxlint -c oxlint.config.js client/App.svelte
pnpm dlx --package @mustafaj/oxfmt@svelte oxfmt --check -c .oxfmtrc.json client/App.svelte
pnpm dlx --package @mustafaj/oxlint@svelte oxlint -c oxlint.config.js .
```

For `/Users/mustafaj/dev/rukn`, the April 2026 `-svelte.1` publish verified:

- `@mustafaj/oxlint@svelte` installed and reported `Version: 1.61.0`.
- `@mustafaj/oxfmt@svelte` installed and reported `Version: 0.46.0`.
- `client/App.svelte` lint passed with `0 warnings and 0 errors`.
- `client/App.svelte` format check passed.
- Full `oxlint -c oxlint.config.js .` passed on 375 files.
