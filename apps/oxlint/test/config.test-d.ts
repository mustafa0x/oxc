import { defineConfig } from "../src-js/index.ts";
import type { OxlintConfig, RuleCategoryConfig } from "../src-js/index.ts";

const category: RuleCategoryConfig = "recommended";
void category;

const base = defineConfig({
  rules: {
    "no-console": "warn",
  },
});

const extendsEntries: OxlintConfig["extends"] = ["oxlint-config-svelte", base];
void extendsEntries;

const config: OxlintConfig = defineConfig({
  extends: ["oxlint-config-svelte", base],
  categories: {
    suspicious: "recommended",
  },
});

void config;
