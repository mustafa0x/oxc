import { describe, expect, it } from "vitest";
import { createRequiredParserCallOptions } from "../src-js/plugins/parser_call_options.ts";

describe("createRequiredParserCallOptions", () => {
  it("merges top-level languageOptions.sourceType when parserOptions omit it", () => {
    expect(createRequiredParserCallOptions("/test/App.svelte", null, "script")).toMatchObject({
      sourceType: "script",
      filePath: "/test/App.svelte",
      loc: true,
      range: true,
      raw: true,
      tokens: true,
      comment: true,
      eslintVisitorKeys: true,
      eslintScopeManager: true,
    });
  });

  it("keeps explicit parserOptions.sourceType over the top-level sourceType", () => {
    expect(
      createRequiredParserCallOptions(
        "/test/App.svelte",
        { sourceType: "commonjs" },
        "module",
      ),
    ).toMatchObject({
      sourceType: "commonjs",
    });
  });

  it("merges top-level languageOptions.ecmaVersion when parserOptions omit it", () => {
    expect(
      createRequiredParserCallOptions("/test/App.svelte", null, undefined, 2022),
    ).toMatchObject({
      ecmaVersion: 2022,
      filePath: "/test/App.svelte",
    });
  });

  it("keeps explicit parserOptions.ecmaVersion over the top-level ecmaVersion", () => {
    expect(
      createRequiredParserCallOptions(
        "/test/App.svelte",
        { ecmaVersion: 2020 },
        undefined,
        2024,
      ),
    ).toMatchObject({
      ecmaVersion: 2020,
    });
  });
});
