import type { Node, Plugin } from "#oxlint/plugins";

const SPAN: Node = {
  start: 0,
  end: 0,
  range: [0, 0],
  loc: {
    start: { line: 0, column: 0 },
    end: { line: 0, column: 0 },
  },
};

const plugin: Plugin = {
  meta: {
    name: "whole-file-svelte-type-aware",
  },
  rules: {
    "options-visible": {
      create(context) {
        return {
          Program() {
            const parserServices = context.sourceCode.parserServices as {
              isSvelte?: unknown;
              filePath?: unknown;
              projectService?: unknown;
              extraFileExtensions?: unknown;
              nestedParserHasParseForESLint?: unknown;
              svelteRunes?: unknown;
              svelteGenerate?: unknown;
              sveltePreprocessIsFunction?: unknown;
              tsFlavor?: unknown;
            };
            const parserOptions = context.languageOptions.parserOptions as {
              parser?: { parseForESLint?: unknown };
              svelteConfig?: {
                compilerOptions?: { runes?: unknown; generate?: unknown };
                preprocess?: unknown;
              };
              projectService?: unknown;
              extraFileExtensions?: unknown;
              tsFlavor?: unknown;
            };
            const extraFileExtensions = Array.isArray(parserOptions.extraFileExtensions)
              ? parserOptions.extraFileExtensions
              : [];

            context.report({
              message: [
                `whole-file: ${context.sourceCode.text.includes("<h1>Hello {name}</h1>")}`,
                `services: ${parserServices.isSvelte === true}`,
                `filePath: ${parserServices.filePath === context.filename}`,
                `projectService: ${parserServices.projectService === true && parserOptions.projectService === true}`,
                `extraFileExtensions: ${extraFileExtensions.includes(".svelte") && Array.isArray(parserServices.extraFileExtensions) && parserServices.extraFileExtensions.includes(".svelte")}`,
                `nestedParserFn: ${typeof parserOptions.parser?.parseForESLint === "function" && parserServices.nestedParserHasParseForESLint === true}`,
                `svelteRunes: ${parserOptions.svelteConfig?.compilerOptions?.runes === true && parserServices.svelteRunes === true}`,
                `svelteGenerate: ${parserOptions.svelteConfig?.compilerOptions?.generate === "dom" && parserServices.svelteGenerate === "dom"}`,
                `preprocessFn: ${typeof parserOptions.svelteConfig?.preprocess === "function" && parserServices.sveltePreprocessIsFunction === true}`,
                `mergedBaseOption: ${parserOptions.tsFlavor === "base-ts-parser" && parserServices.tsFlavor === "base-ts-parser"}`,
              ].join("; "),
              node: SPAN,
            });
          },
        };
      },
    },
  },
};

export default plugin;
