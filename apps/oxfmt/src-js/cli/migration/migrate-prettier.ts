/* oxlint-disable no-console */

import { basename, join } from "node:path";
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { hasOxfmtrcFile, createBlankOxfmtrcFile, saveOxfmtrcFile, exitWithError } from "./shared";
import type { Options } from "prettier";

/**
 * Run the `--migrate prettier` command to migrate various Prettier's config to `.oxfmtrc.json` file.
 * https://prettier.io/docs/configuration
 */
export async function runMigratePrettier() {
  const cwd = process.cwd();

  if (await hasOxfmtrcFile(cwd)) {
    return exitWithError("Oxfmt configuration file already exists.");
  }

  // XXX: If you statically import `prettier` here,
  // completely unsure why, but Prettier hangs forever when run via `napi`...
  const { resolveConfigFile, resolveConfig } = await import("prettier");

  // NOTE: Prettier's config resolving is based on each file,
  // but ours is based on the project root, typically `cwd`.
  // So we assume the config for a dummy file at the `cwd`.
  const prettierConfigPath = await resolveConfigFile(join(cwd, "dummy.js"));

  // No Prettier config found, fallback with `--init` behavior
  if (!prettierConfigPath) {
    console.log("No Prettier configuration file found.");

    const oxfmtrc = await createBlankOxfmtrcFile(cwd);
    const jsonStr = JSON.stringify(oxfmtrc, null, 2);

    // TODO: Create napi `validateConfig()` and use to ensure validity?

    try {
      await saveOxfmtrcFile(cwd, jsonStr);
      console.log("Created `.oxfmtrc.json` instead.");
    } catch {
      exitWithError("Failed to create `.oxfmtrc.json`.");
    }

    return;
  }

  let prettierConfig;
  try {
    prettierConfig = await resolveConfig(prettierConfigPath, {
      // Avoid merging `.editorconfig` values
      editorconfig: false,
    });
    console.log("Found Prettier configuration at:", prettierConfigPath);
  } catch {
    return exitWithError(`Failed to parse: ${prettierConfigPath}`);
  }

  // Start with blank, then fill in from `prettierConfig`.
  // NOTE: Some options unsupported by Oxfmt may still be valid when invoking Prettier.
  // However, to avoid inconsistency, we do not enable options that affect Oxfmt.
  const oxfmtrc = await createBlankOxfmtrcFile(cwd);
  migratePrettierOptions(prettierConfig ?? {}, oxfmtrc, { applyDefaults: true });

  const rawPrettierConfig = await resolveRawPrettierConfig(prettierConfigPath);
  const migratedOverrides = migratePrettierOverrides(rawPrettierConfig);
  if (migratedOverrides) {
    oxfmtrc.overrides = migratedOverrides;
  }

  // Migrate `ignorePatterns` from `.prettierignore`
  const ignores = await resolvePrettierIgnore(cwd);
  if (ignores.length > 0) {
    console.log("Migrated ignore patterns from `.prettierignore`");
  }
  // Keep ignorePatterns at the bottom
  delete oxfmtrc.ignorePatterns;
  oxfmtrc.ignorePatterns = ignores;

  const jsonStr = JSON.stringify(oxfmtrc, null, 2);

  // TODO: Create napi `validateConfig()` and use to ensure validity?

  try {
    await saveOxfmtrcFile(cwd, jsonStr);
    console.log("Created `.oxfmtrc.json`.");
  } catch {
    return exitWithError("Failed to create `.oxfmtrc.json`.");
  }
}

// ---

type PrettierConfigObject = Record<string, unknown>;

type MigrationScope = {
  applyDefaults: boolean;
  label?: string;
};

const JSON_LIKE_PRETTIER_CONFIG_BASENAMES = new Set([
  ".prettierrc",
  ".prettierrc.json",
  ".prettierrc.jsonc",
  "prettier.config.json",
  "prettier.config.jsonc",
  "package.json",
]);

const JS_LIKE_PRETTIER_CONFIG_BASENAMES = new Set([
  ".prettierrc.js",
  ".prettierrc.cjs",
  ".prettierrc.mjs",
  "prettier.config.js",
  "prettier.config.cjs",
  "prettier.config.mjs",
]);

function migratePrettierOptions(
  prettierConfig: PrettierConfigObject,
  oxfmtrc: Record<string, unknown>,
  scope: MigrationScope,
): void {
  let hasSortPackageJsonPlugin = false;
  let migratedPlugins: string[] | undefined;

  for (const [key, value] of Object.entries(prettierConfig)) {
    // Handle plugins specially:
    // - `prettier-plugin-tailwindcss` becomes `sortTailwindcss`
    // - `prettier-plugin-packagejson` becomes `sortPackageJson`
    // - other string plugin specs are preserved because Oxfmt can load them directly
    if (key === "plugins" && Array.isArray(value)) {
      const { plugins, usesSortPackageJsonPlugin } = migratePlugins(
        (value as Options["plugins"])!,
        prettierConfig,
        oxfmtrc,
        scope,
      );
      migratedPlugins = plugins.length > 0 ? plugins : undefined;
      hasSortPackageJsonPlugin = usesSortPackageJsonPlugin;
      continue;
    }

    if (key === "overrides") {
      continue;
    }

    // Oxfmt does not support this, fallback to default
    if (key === "endOfLine" && value === "auto") {
      warnMigration(scope, '"endOfLine: auto" is not supported, skipping...');
      continue;
    }
    // Oxfmt does not support these experimental options yet
    if (key === "experimentalTernaries" || key === "experimentalOperatorPosition") {
      warnMigration(scope, `"${key}" is not supported in JS/TS files yet`);
      continue;
    }

    // Skip Tailwind options - handled separately by migrateTailwindOptions
    if (key.startsWith("tailwind")) {
      continue;
    }

    // Otherwise, copy the value.
    // This may include options that do not affect Oxfmt, like `vueIndentScriptAndStyle`.
    oxfmtrc[key] = value;
  }

  if (migratedPlugins) {
    oxfmtrc.plugins = migratedPlugins;
  }

  if (hasSortPackageJsonPlugin) {
    oxfmtrc.sortPackageJson = {};
    warnMigration(scope, 'Migrated "prettier-plugin-packagejson" to "sortPackageJson"');
  } else if (scope.applyDefaults) {
    // `sortPackageJson` is enabled by default in Oxfmt, but Prettier does not have this.
    // Only enable if `prettier-plugin-packagejson` is used.
    oxfmtrc.sortPackageJson = false;
  }

  if (scope.applyDefaults) {
    // `printWidth` has different default between Prettier and Oxfmt.
    // Oxfmt default is 100, Prettier default is 80.
    if (typeof oxfmtrc.printWidth !== "number") {
      warnMigration(
        scope,
        '"printWidth" is not set in Prettier config, defaulting to 80 (Oxfmt default: 100)',
      );
      oxfmtrc.printWidth = 80;
    }

    // `embeddedLanguageFormatting` is not fully supported for JS-in-XXX yet.
    if (oxfmtrc.embeddedLanguageFormatting !== "off") {
      warnMigration(scope, '"embeddedLanguageFormatting" in JS/TS files is not fully supported yet');
    }
  } else if (oxfmtrc.embeddedLanguageFormatting !== undefined && oxfmtrc.embeddedLanguageFormatting !== "off") {
    warnMigration(scope, '"embeddedLanguageFormatting" in JS/TS files is not fully supported yet');
  }
}

function migratePrettierOverrides(
  rawPrettierConfig: PrettierConfigObject | null,
): Array<Record<string, unknown>> | undefined {
  const overrides = rawPrettierConfig?.overrides;
  if (!Array.isArray(overrides)) {
    return undefined;
  }

  const migratedOverrides: Array<Record<string, unknown>> = [];

  for (const [index, overrideEntry] of overrides.entries()) {
    if (!isRecord(overrideEntry)) {
      warnMigration({ label: `overrides[${index}]`, applyDefaults: false }, "invalid override entry, skipping...");
      continue;
    }

    const files = normalizeOverridePatterns(overrideEntry.files);
    if (!files || files.length === 0) {
      warnMigration(
        { label: `overrides[${index}]`, applyDefaults: false },
        'missing valid "files" patterns, skipping override...',
      );
      continue;
    }

    const migratedOverride: Record<string, unknown> = { files };
    const excludeFiles = normalizeOverridePatterns(overrideEntry.excludeFiles);
    if (excludeFiles && excludeFiles.length > 0) {
      migratedOverride.excludeFiles = excludeFiles;
    }

    const options = isRecord(overrideEntry.options) ? overrideEntry.options : {};
    const migratedOptions: Record<string, unknown> = {};
    migratePrettierOptions(options, migratedOptions, {
      applyDefaults: false,
      label: `overrides[${index}].options`,
    });

    migratedOverride.options = migratedOptions;
    migratedOverrides.push(migratedOverride);
  }

  return migratedOverrides.length > 0 ? migratedOverrides : undefined;
}

function migratePlugins(
  plugins: Options["plugins"],
  prettierConfig: Record<string, unknown>,
  oxfmtrc: Record<string, unknown>,
  scope: MigrationScope,
): { plugins: string[]; usesSortPackageJsonPlugin: boolean } {
  const preservedPlugins: string[] = [];
  let usesSortPackageJsonPlugin = false;

  for (const plugin of plugins ?? []) {
    if (plugin === "prettier-plugin-tailwindcss") {
      migrateTailwindOptions(prettierConfig, oxfmtrc, scope);
      continue;
    }

    if (plugin === "prettier-plugin-packagejson") {
      usesSortPackageJsonPlugin = true;
      continue;
    }

    if (typeof plugin === "string") {
      if (!preservedPlugins.includes(plugin)) {
        preservedPlugins.push(plugin);
      }
      continue;
    }

    warnMigration(scope, "plugins: custom plugin module is not supported, skipping...");
  }

  return { plugins: preservedPlugins, usesSortPackageJsonPlugin };
}

// ---

async function resolvePrettierIgnore(cwd: string) {
  const ignores = [];

  try {
    const content = await readFile(join(cwd, ".prettierignore"), "utf8");

    const lines = content.split("\n");
    for (let line of lines) {
      line = line.trim();
      if (line === "" || line.startsWith("#")) {
        continue;
      }
      ignores.push(line);
    }
  } catch {}

  return ignores;
}

async function resolveRawPrettierConfig(
  prettierConfigPath: string,
): Promise<PrettierConfigObject | null> {
  const configBasename = basename(prettierConfigPath);

  if (JSON_LIKE_PRETTIER_CONFIG_BASENAMES.has(configBasename)) {
    return resolveRawJSONPrettierConfig(prettierConfigPath, configBasename);
  }

  if (JS_LIKE_PRETTIER_CONFIG_BASENAMES.has(configBasename)) {
    return resolveRawJSImportPrettierConfig(prettierConfigPath);
  }

  return null;
}

async function resolveRawJSONPrettierConfig(
  prettierConfigPath: string,
  configBasename: string,
): Promise<PrettierConfigObject | null> {

  try {
    const rawText = await readFile(prettierConfigPath, "utf8");
    const parsed = parseJSONC(rawText);
    if (!isRecord(parsed)) {
      return null;
    }

    if (configBasename === "package.json") {
      return isRecord(parsed.prettier) ? parsed.prettier : null;
    }

    return parsed;
  } catch {
    return null;
  }
}

async function resolveRawJSImportPrettierConfig(
  prettierConfigPath: string,
): Promise<PrettierConfigObject | null> {
  try {
    const moduleUrl = pathToFileURL(prettierConfigPath);
    moduleUrl.searchParams.set("oxfmt-prettier-config", `${Date.now()}`);

    const importedModule = await import(moduleUrl.href);
    const importedConfig = await unwrapImportedPrettierConfig(importedModule);
    return isRecord(importedConfig) ? importedConfig : null;
  } catch {
    return null;
  }
}

async function unwrapImportedPrettierConfig(importedModule: Record<string, unknown>): Promise<unknown> {
  let importedConfig: unknown = importedModule.default ?? importedModule;
  if (typeof importedConfig === "function") {
    importedConfig = importedConfig();
  }
  return await importedConfig;
}

function normalizeOverridePatterns(value: unknown): string[] | undefined {
  if (typeof value === "string") {
    return [value];
  }

  if (!Array.isArray(value)) {
    return undefined;
  }

  const patterns = value.filter((item): item is string => typeof item === "string");
  return patterns.length > 0 ? patterns : undefined;
}

function warnMigration(scope: MigrationScope, message: string): void {
  const label = scope.label ? `${scope.label}: ` : "";
  console.error(`  - ${label}${message}`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value);
}

// https://github.com/fabiospampinato/tiny-jsonc/blob/bb722089210174ec9cb53afcce15245e7ee21b9a/src/index.ts
const stringOrCommentRe = /("(?:\\?[^])*?")|(\/\/.*)|(\/\*[^]*?\*\/)/g;
const stringOrTrailingCommaRe = /("(?:\\?[^])*?")|(,\s*)(?=]|})/g;
function parseJSONC(text: string): unknown {
  text = String(text); // To be extra safe
  try {
    // Fast path for valid JSON
    return JSON.parse(text);
  } catch {
    // Slow path for JSONC and invalid inputs
    return JSON.parse(text.replace(stringOrCommentRe, "$1").replace(stringOrTrailingCommaRe, "$1"));
  }
}

// ---

const TAILWIND_OPTION_MAPPING: Record<string, string> = {
  config: "tailwindConfig",
  stylesheet: "tailwindStylesheet",
  functions: "tailwindFunctions",
  attributes: "tailwindAttributes",
  preserveWhitespace: "tailwindPreserveWhitespace",
  preserveDuplicates: "tailwindPreserveDuplicates",
};

/**
 * Migrate prettier-plugin-tailwindcss options to Oxfmt's sortTailwindcss format.
 *
 * Prettier format:
 * ```json
 * {
 *   "plugins": ["prettier-plugin-tailwindcss"],
 *   "tailwindConfig": "./tailwind.config.js",
 *   "tailwindFunctions": ["clsx", "cn"]
 * }
 * ```
 *
 * Oxfmt format:
 * ```json
 * {
 *   "sortTailwindcss": {
 *     "config": "./tailwind.config.js",
 *     "functions": ["clsx", "cn"]
 *   }
 * }
 * ```
 */
function migrateTailwindOptions(
  prettierConfig: Record<string, unknown>,
  oxfmtrc: Record<string, unknown>,
  scope: MigrationScope,
): void {
  // Collect Tailwind options from Prettier config
  const tailwindOptions: Record<string, unknown> = {};
  for (const [oxfmtKey, prettierKey] of Object.entries(TAILWIND_OPTION_MAPPING)) {
    const value = prettierConfig[prettierKey];
    if (value !== undefined) {
      if (
        (prettierKey == "tailwindFunctions" || prettierKey == "tailwindAttributes") &&
        Array.isArray(value)
      ) {
        for (const item of value as string[]) {
          if (typeof item === "string" && item.startsWith("/") && item.endsWith("/")) {
            console.warn(
              `  - ${scope.label ? `${scope.label}: ` : ""}Do not support regex in "${prettierKey}" option yet, skipping: ${item}`,
            );
            continue;
          }
        }
      }
      tailwindOptions[oxfmtKey] = value;
    }
  }

  // Only add sortTailwindcss if plugin is used or options are present
  oxfmtrc.sortTailwindcss = tailwindOptions;
  console.log(
    `${scope.label ? `Migrated ${scope.label} ` : "Migrated "}prettier-plugin-tailwindcss options to sortTailwindcss`,
  );
}
