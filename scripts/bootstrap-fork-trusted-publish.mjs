import { execFileSync } from 'node:child_process'
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'

const repo_root = resolve(import.meta.dirname, '..')
const output_root = join(repo_root, '.fork-trusted-publish')

const supported_packages = {
  oxfmt: {
    version: read_package_version('npm/oxfmt/package.json'),
    bindings: [
      {
        name_suffix: 'binding-darwin-arm64',
        target: 'aarch64-apple-darwin',
        os: 'darwin',
        cpu: 'arm64',
      },
      {
        name_suffix: 'binding-linux-x64-gnu',
        target: 'x86_64-unknown-linux-gnu',
        os: 'linux',
        cpu: 'x64',
      },
      {
        name_suffix: 'binding-win32-x64-msvc',
        target: 'x86_64-pc-windows-msvc',
        os: 'win32',
        cpu: 'x64',
      },
    ],
  },
  oxlint: {
    version: read_package_version('npm/oxlint/package.json'),
    bindings: [
      {
        name_suffix: 'binding-darwin-arm64',
        target: 'aarch64-apple-darwin',
        os: 'darwin',
        cpu: 'arm64',
      },
      {
        name_suffix: 'binding-linux-x64-gnu',
        target: 'x86_64-unknown-linux-gnu',
        os: 'linux',
        cpu: 'x64',
      },
      {
        name_suffix: 'binding-win32-x64-msvc',
        target: 'x86_64-pc-windows-msvc',
        os: 'win32',
        cpu: 'x64',
      },
    ],
  },
}

const args = parse_args(process.argv.slice(2))
const selected_package_ids = parse_csv_arg(args.packages ?? 'oxlint,oxfmt')
const scope = normalize_scope(args.scope ?? detect_default_scope())
const bootstrap_version = String(args.version ?? '0.0.1')
const publish = !!args.publish

for (const package_id of selected_package_ids) {
  if (!(package_id in supported_packages)) {
    throw new Error(
      `Unsupported package "${package_id}". Expected one of: ${Object.keys(supported_packages).join(', ')}`
    )
  }
}

rmSync(output_root, { recursive: true, force: true })
mkdirSync(output_root, { recursive: true })

const generated_packages = []
for (const package_id of selected_package_ids) {
  for (const binding of supported_packages[package_id].bindings) {
    generated_packages.push(
      create_bootstrap_package({
        package_id,
        scope,
        bootstrap_version,
        current_version: supported_packages[package_id].version,
        ...binding,
      })
    )
  }
}

if (publish) {
  for (const pkg of generated_packages) {
    if (npm_package_exists(pkg.name)) {
      console.log(`Skipping ${pkg.name}; it already exists on npm.`)
      continue
    }

    run('npm', ['publish', '--access', 'public'], pkg.dir)
  }
} else {
  console.log(`Generated ${generated_packages.length} bootstrap packages in ${relative_to_repo(output_root)}.`)
  console.log('Review them, then publish with:')
  console.log(`pnpm bootstrap:fork-trusted-publish --scope ${scope} --publish`)
}

for (const pkg of generated_packages) {
  console.log(`- ${pkg.name}@${pkg.version} -> ${relative_to_repo(pkg.dir)}`)
}

function create_bootstrap_package({
  package_id,
  scope,
  name_suffix,
  target,
  os,
  cpu,
  bootstrap_version,
  current_version,
}) {
  const name = `${scope}/${package_id}-${name_suffix}`
  const dir = join(output_root, name.slice(1).replaceAll('/', '__'))
  mkdirSync(dir, { recursive: true })

  write_json(join(dir, 'package.json'), {
    name,
    version: bootstrap_version,
    description: `Bootstrap package for npm trusted publishing of ${package_id} native bindings`,
    license: 'MIT',
    os: [os],
    cpu: [cpu],
    files: ['README.md', 'index.js'],
    main: './index.js',
  })

  writeFileSync(
    join(dir, 'README.md'),
    [
      `# ${name}`,
      '',
      'Bootstrap package for npm trusted publishing.',
      '',
      `Target: \`${target}\``,
      `Real package version intended after bootstrap: \`${current_version}\``,
      '',
      'This placeholder exists only so `npm trust github ...` can be configured.',
      '',
    ].join('\n')
  )

  writeFileSync(
    join(dir, 'index.js'),
    `throw new Error(${JSON.stringify(`${name} is a bootstrap placeholder package. Install a real published version instead.`)})\n`
  )

  return { dir, name, package_id, version: bootstrap_version }
}

function npm_package_exists(name) {
  try {
    execFileSync('npm', ['view', name, 'version'], {
      cwd: repo_root,
      stdio: 'ignore',
    })
    return true
  } catch {
    return false
  }
}

function run(cmd, args, cwd = repo_root) {
  console.log(`$ ${cmd} ${args.join(' ')}`)
  execFileSync(cmd, args, {
    cwd,
    env: { ...process.env, CI: process.env.CI ?? 'true' },
    stdio: 'inherit',
  })
}

function read_package_version(relative_path) {
  const package_json = JSON.parse(readFileSync(join(repo_root, relative_path), 'utf8'))
  return package_json.version
}

function write_json(file_path, value) {
  writeFileSync(file_path, `${JSON.stringify(value, null, 2)}\n`)
}

function parse_args(argv) {
  const out = {}
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i]
    if (!arg.startsWith('--')) continue
    const [raw_key, inline_value] = arg.slice(2).split('=', 2)
    if (inline_value !== undefined) {
      out[raw_key] = inline_value
      continue
    }
    const next = argv[i + 1]
    if (next === undefined || next.startsWith('--')) {
      out[raw_key] = true
      continue
    }
    out[raw_key] = next
    i++
  }
  return out
}

function parse_csv_arg(value) {
  if (value === undefined || value === true || value === '') return []
  return String(value)
    .split(',')
    .map((entry) => entry.trim())
    .filter(Boolean)
}

function normalize_scope(scope) {
  return scope.startsWith('@') ? scope : `@${scope}`
}

function detect_default_scope() {
  const npm_user = safe_exec('npm', ['whoami'])
  if (npm_user !== null && npm_user !== '') {
    return `@${npm_user}`
  }

  return '@mustafaj'
}

function safe_exec(cmd, args) {
  try {
    return execFileSync(cmd, args, { cwd: repo_root, encoding: 'utf8' }).trim()
  } catch {
    return null
  }
}

function relative_to_repo(file_path) {
  return file_path.startsWith(repo_root) ? file_path.slice(repo_root.length + 1) : file_path
}
