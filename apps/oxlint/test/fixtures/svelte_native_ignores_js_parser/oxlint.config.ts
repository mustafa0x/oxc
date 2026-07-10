import { defineConfig } from "#oxlint";

const parser = {
  parseForESLint() {
    throw new Error("native .svelte files must not invoke the configured JS parser");
  },
};

export default defineConfig({
  jsPlugins: ["./plugin.ts"],
  overrides: [
    {
      files: ["**/*.svelte"],
      languageOptions: { parser },
      rules: {
        "svelte-parser-sentinel/should-not-run": "error",
      },
    },
  ],
});
