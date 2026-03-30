import { defineConfig } from "#oxlint";
import baseConfig from "./base-config.ts";

function getLoc(code: string) {
  const lines = code.split("\n");
  return {
    start: { line: 1, column: 0 },
    end: { line: lines.length, column: lines[lines.length - 1]?.length ?? 0 },
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

const svelteStubParser = {
  parseForESLint(code: string, options?: Record<string, unknown>) {
    const parserOptions = options ?? {};
    const svelteConfig = isRecord(parserOptions.svelteConfig) ? parserOptions.svelteConfig : null;
    const compilerOptions = isRecord(svelteConfig?.compilerOptions)
      ? svelteConfig.compilerOptions
      : null;
    const extraFileExtensions = Array.isArray(parserOptions.extraFileExtensions)
      ? parserOptions.extraFileExtensions
      : [];
    const nestedParser = isRecord(parserOptions.parser) ? parserOptions.parser : null;

    return {
      ast: {
        type: "Program",
        sourceType: "module",
        body: [],
        range: [0, code.length],
        loc: getLoc(code),
        comments: [],
        tokens: [],
      },
      services: {
        isSvelte: true,
        filePath: parserOptions.filePath ?? null,
        projectService: parserOptions.projectService ?? null,
        extraFileExtensions,
        nestedParserHasParseForESLint: typeof nestedParser?.parseForESLint === "function",
        svelteRunes: compilerOptions?.runes ?? null,
        svelteGenerate: compilerOptions?.generate ?? null,
        sveltePreprocessIsFunction: typeof svelteConfig?.preprocess === "function",
        tsFlavor: parserOptions.tsFlavor ?? null,
      },
      visitorKeys: {
        Program: ["body"],
      },
    };
  },
};

export default defineConfig({
  categories: {
    correctness: "off",
  },
  extends: [baseConfig],
  jsPlugins: ["./plugin.ts"],
  overrides: [
    {
      files: ["**/*.svelte"],
      languageOptions: {
        parser: svelteStubParser,
        parserOptions: {
          projectService: true,
          extraFileExtensions: [".svelte"],
        },
      },
      rules: {
        "whole-file-svelte-type-aware/options-visible": "error",
      },
    },
  ],
});
