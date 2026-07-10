import { execFileSync } from 'node:child_process'
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join, relative, resolve } from 'node:path'

const repo_root = resolve(import.meta.dirname, '..')
const output_root = join(repo_root, '.fork-publish')

const package_configs = {
  oxfmt: {
    app_dir: 'apps/oxfmt',
    app_package_name: 'oxfmt-app',
    binding_prefix: '@oxfmt/binding',
    binary_name: 'oxfmt',
    npm_dir: 'npm/oxfmt',
    package_name: 'oxfmt',
    readme_command: 'npx --yes {name}@{version}',
  },
  oxlint: {
    app_dir: 'apps/oxlint',
    app_package_name: 'oxlint-app',
    binding_prefix: '@oxlint/binding',
    binary_name: 'oxlint',
    npm_dir: 'npm/oxlint',
    package_name: 'oxlint',
    readme_command: 'npx --yes {name}@{version}',
  },
}

const args = parse_args(process.argv.slice(2))
const scope = normalize_scope(args.scope ?? detect_default_scope())
const version_suffix = args['version-suffix'] ?? ''
const target_list = parse_csv_arg(args.targets)
const selected_packages = (args.packages ?? 'oxlint,oxfmt')
  .split(',')
  .map((value) => value.trim())
  .filter(Boolean)

for (const package_id of selected_packages) {
  if (!(package_id in package_configs)) {
    throw new Error(
      `Unsupported package "${package_id}". Expected one of: ${Object.keys(package_configs).join(', ')}`
    )
  }
}

for (const package_id of selected_packages) {
  prepare_package(package_id, package_configs[package_id], {
    scope,
    version_suffix,
    skip_build: !!args['skip-build'],
    skip_native_bundle: !!args['skip-native-bundle'],
    targets: target_list,
  })
}

function prepare_package(package_id, config, options) {
  const scoped_name = `${options.scope}/${config.package_name}`
  const binding_prefix = `${options.scope}/${config.package_name}-binding`
  const release_root = join(output_root, package_id)
  const package_root = join(release_root, 'package')
  const manifest_path = join(repo_root, config.npm_dir, 'package.json')
  const manifest = read_json(manifest_path)
  const version = options.version_suffix === '' ? manifest.version : `${manifest.version}${options.version_suffix}`
  const repo_url = normalize_repo_url(
    safe_exec('git', ['remote', 'get-url', 'mustafa0x']) ??
      safe_exec('git', ['remote', 'get-url', 'origin']) ??
      ''
  )

  if (!options.skip_build) {
    run('pnpm', ['-C', config.app_dir, 'run', 'build-napi-release'])
    run('pnpm', ['-C', config.app_dir, 'run', 'build-js'])
  }

  const dist_dir = join(repo_root, config.app_dir, 'dist')
  const src_js_dir = join(repo_root, config.app_dir, 'src-js')
  if (!existsSync(dist_dir)) throw new Error(`Missing build output: ${relative(repo_root, dist_dir)}`)
  const native_binary_names = options.skip_native_bundle
    ? []
    : readdirSync(src_js_dir)
        .filter((name) => name.startsWith(`${config.binary_name}.`) && name.endsWith('.node'))
        .sort()
  if (!options.skip_native_bundle && native_binary_names.length === 0) {
    throw new Error(`Missing native binding output in ${relative(repo_root, src_js_dir)}. Run a NAPI build first.`)
  }

  rmSync(release_root, { force: true, recursive: true })
  mkdirSync(release_root, { recursive: true })
  cpSync(join(repo_root, config.npm_dir), package_root, { recursive: true })
  rmSync(join(package_root, 'dist'), { force: true, recursive: true })
  cpSync(dist_dir, join(package_root, 'dist'), { recursive: true })
  if (!options.skip_native_bundle) {
    for (const native_binary_name of native_binary_names) {
      cpSync(join(src_js_dir, native_binary_name), join(package_root, 'dist', native_binary_name))
    }
  }

  const package_json_path = join(package_root, 'package.json')
  const package_json = read_json(package_json_path)
  package_json.name = scoped_name
  package_json.version = version
  package_json.publishConfig = { ...(package_json.publishConfig ?? {}), access: 'public' }
  package_json.napi = { ...(package_json.napi ?? {}), packageName: binding_prefix }
  if (options.targets.length > 0) {
    package_json.napi = { ...(package_json.napi ?? {}), targets: options.targets }
  }
  if (repo_url !== '') {
    package_json.repository = {
      ...(typeof package_json.repository === 'object' && package_json.repository !== null ? package_json.repository : {}),
      type: 'git',
      url: `git+${repo_url}.git`,
    }
    package_json.bugs = `${repo_url}/issues`
  }
  if (!options.skip_native_bundle && native_binary_names.length === 1) {
    const target = parse_native_target(native_binary_names[0], config.binary_name)
    if (target !== null) {
      package_json.os = [target.os]
      package_json.cpu = [target.cpu]
    }
  } else {
    delete package_json.os
    delete package_json.cpu
  }
  write_json(package_json_path, package_json)

  const readme_path = join(package_root, 'README.md')
  if (existsSync(readme_path)) {
    const target_notice = options.skip_native_bundle
      ? options.targets.length > 0
        ? `> Published native targets: ${options.targets.join(', ')}.\n`
        : `> Native targets are published as separate packages.\n`
      : `> Bundled native targets: ${native_binary_names.map((name) => name.slice(`${config.binary_name}.`.length, -'.node'.length)).join(', ')}.\n`
    const readme_notice =
      `> Fork publish for immediate use from this branch.\n` +
      target_notice +
      `> Install with \`${config.readme_command.replace('{name}', scoped_name).replace('{version}', version)}\`.\n\n`
    writeFileSync(readme_path, readme_notice + readFileSync(readme_path, 'utf8'))
  }

  rewrite_text_files(join(package_root, 'dist'), [
    [config.binding_prefix, binding_prefix],
    [config.app_package_name, scoped_name],
  ])
  console.log(`Prepared ${scoped_name} in ${relative(repo_root, package_root)}`)
}

function rewrite_text_files(root_dir, replacements) {
  visit_dir(root_dir, (dir_path) => {
    for (const entry of readdirSync(dir_path, { withFileTypes: true })) {
      if (!entry.isFile()) continue
      if (!entry.name.endsWith('.js') && !entry.name.endsWith('.mjs') && !entry.name.endsWith('.cjs')) {
        continue
      }
      const file_path = join(dir_path, entry.name)
      let text = readFileSync(file_path, 'utf8')
      for (const [from, to] of replacements) {
        if (from === to) continue
        text = text.split(from).join(to)
      }
      writeFileSync(file_path, text)
    }
  })
}

function visit_dir(root_dir, cb) {
  if (!existsSync(root_dir)) return
  const pending = [root_dir]
  while (pending.length > 0) {
    const dir_path = pending.pop()
    cb(dir_path)
    for (const entry of readdirSync(dir_path, { withFileTypes: true })) {
      if (entry.isDirectory()) pending.push(join(dir_path, entry.name))
    }
  }
}

function parse_native_target(native_binary_name, binary_name) {
  const prefix = `${binary_name}.`
  if (!native_binary_name.startsWith(prefix) || !native_binary_name.endsWith('.node')) return null
  const target = native_binary_name.slice(prefix.length, -'.node'.length)
  const parts = target.split('-')
  if (parts.length < 2) return null
  const [os, ...rest] = parts
  const cpu = rest[0]
  if (os === '' || cpu === undefined || cpu === '') return null
  return { os, cpu, target }
}

function run(cmd, args) {
  console.log(`$ ${cmd} ${args.join(' ')}`)
  execFileSync(cmd, args, {
    cwd: repo_root,
    env: { ...process.env, CI: process.env.CI ?? 'true' },
    stdio: 'inherit',
  })
}

function safe_exec(cmd, args) {
  try {
    return execFileSync(cmd, args, { cwd: repo_root, encoding: 'utf8' }).trim()
  } catch {
    return null
  }
}

function read_json(file_path) {
  return JSON.parse(readFileSync(file_path, 'utf8'))
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

  const tracking = safe_exec('git', ['rev-parse', '--abbrev-ref', '--symbolic-full-name', '@{upstream}'])
  if (tracking !== null) {
    const remote_name = tracking.split('/')[0]
    const remote_url = safe_exec('git', ['remote', 'get-url', remote_name])
    const owner = parse_github_owner(remote_url)
    if (owner !== null) return `@${owner}`
  }

  for (const remote_name of ['mustafa0x', 'origin']) {
    const remote_url = safe_exec('git', ['remote', 'get-url', remote_name])
    const owner = parse_github_owner(remote_url)
    if (owner !== null) return `@${owner}`
  }

  throw new Error('Unable to detect a default npm scope from git remotes. Pass --scope explicitly.')
}

function parse_github_owner(remote_url) {
  if (typeof remote_url !== 'string' || remote_url === '') return null
  const ssh_match = /^git@github\.com:([^/]+)\/[^/]+(?:\.git)?$/.exec(remote_url)
  if (ssh_match) return ssh_match[1]
  const https_match = /^https:\/\/github\.com\/([^/]+)\/[^/]+(?:\.git)?$/.exec(remote_url)
  if (https_match) return https_match[1]
  return null
}

function normalize_repo_url(remote_url) {
  if (typeof remote_url !== 'string' || remote_url === '') return ''
  const ssh_match = /^git@github\.com:([^/]+)\/([^/]+?)(?:\.git)?$/.exec(remote_url)
  if (ssh_match) return `https://github.com/${ssh_match[1]}/${ssh_match[2]}`
  const https_match = /^https:\/\/github\.com\/([^/]+)\/([^/]+?)(?:\.git)?$/.exec(remote_url)
  if (https_match) return `https://github.com/${https_match[1]}/${https_match[2]}`
  return remote_url.replace(/\.git$/, '')
}
