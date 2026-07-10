import { describe, expect, it } from "vitest";
import { format } from "../../dist/index.js";

describe("Svelte support", () => {
  describe("Native backend", () => {
    it("formats `.svelte` without requiring a `svelte` option", async () => {
      const input = `<script>let   count=$state(0);</script>
<button onclick={()=>count++}>clicks: {count}</button>
<style>button{color:red;}</style>
`;

      const result = await format("App.svelte", input);

      expect(result.errors).toStrictEqual([]);
      expect(result.code).toContain("let count = $state(0);");
      expect(result.code).toContain("onclick={() => count++}");
      expect(result.code).toContain("button {\n    color: red;\n  }");
    });

    it("keeps `svelte: true` and `svelte: {}` as native-formatting config aliases", async () => {
      const input = `<script>let   count=$state(0);</script>
<p>{count}</p>
`;

      const defaultResult = await format("App.svelte", input);
      const trueResult = await format("App.svelte", input, { svelte: true });
      const objectResult = await format("App.svelte", input, { svelte: {} });

      expect(defaultResult.errors).toStrictEqual([]);
      expect(trueResult.errors).toStrictEqual([]);
      expect(objectResult.errors).toStrictEqual([]);
      expect(trueResult.code).toBe(defaultResult.code);
      expect(objectResult.code).toBe(defaultResult.code);
    });

    it("does not treat `svelte: false` as a missing plugin", async () => {
      const input = `<script>let count=$state(0);</script>
<p>{count}</p>
`;

      const result = await format("App.svelte", input, { svelte: false });

      expect(result.errors).toStrictEqual([]);
      expect(result.code).toContain("let count = $state(0);");
    });

    it("does not load configured Prettier Svelte plugin specs for native `.svelte` files", async () => {
      const input = `<script>let count=$state(0);</script>
<p>{count}</p>
`;

      const result = await format("App.svelte", input, {
        plugins: ["definitely-missing-prettier-plugin-svelte"],
      } as any);

      expect(result.errors).toStrictEqual([]);
      expect(result.code).toContain("let count = $state(0);");
    });

    it("respects `svelteIndentScriptAndStyle: false`", async () => {
      const input = `<script>let count=$state(0);</script>
<p>{count}</p>
`;

      const result = await format("App.svelte", input, {
        svelteIndentScriptAndStyle: false,
      });

      expect(result.errors).toStrictEqual([]);
      expect(result.code).toContain("<script>\nlet count = $state(0);\n</script>");
    });
  });

  describe("Script section", () => {
    it("respects normal JS formatting options inside `<script>`", async () => {
      const input = `<script>
const x={a:1,b:2};
</script>
<p>{x.a}</p>
`;

      const result = await format("App.svelte", input, { semi: false });

      expect(result.errors).toStrictEqual([]);
      expect(result.code).toContain("const x = { a: 1, b: 2 }");
      expect(result.code).not.toContain("const x = { a: 1, b: 2 };");
    });

    it("respects `oxfmt-ignore` inside `<script>`", async () => {
      const input = `<script>
// oxfmt-ignore
const x   =   { a:1,b:2 };
const y={c:3,d:4};
</script>
<p>{x.a + y.c}</p>
`;

      const result = await format("App.svelte", input);

      expect(result.errors).toStrictEqual([]);
      expect(result.code).toContain("const x   =   { a:1,b:2 };");
      expect(result.code).toContain("const y = { c: 3, d: 4 };");
    });

    it("formats TypeScript script blocks through the native backend", async () => {
      const input = `<script lang="ts">
let n: number = $state(0);
</script>
<p>{n}</p>
`;

      const result = await format("App.svelte", input);

      expect(result.errors).toStrictEqual([]);
      expect(result.code).toContain("let n: number = $state(0);");
    });

    it("formats scripts, markup expressions, text, and CSS", async () => {
      const input = `<script context="module">export const answer=40+2</script>
<script>let items=[{foo:1,bar:2}]</script>
{#each items as {foo,bar = 2}}
{@const value={foo:1,bar:2}}
<p>Hello   world {foo}</p>
{/each}
<style>
.button {  color: red; }
</style>
<pre>  keep
  spacing </pre>
`;

      const result = await format("App.svelte", input);

      expect(result.errors).toStrictEqual([]);
      expect(result.code).toBe(`<script context="module">
  export const answer = 40 + 2;
</script>

<script>
  let items = [{ foo: 1, bar: 2 }];
</script>

{#each items as { foo, bar = 2 }}
  {@const value = { foo: 1, bar: 2 }}
  <p>Hello world {foo}</p>
{/each}
<pre>  keep
  spacing </pre>

<style>
  .button {
    color: red;
  }
</style>
`);
    });
  });

  it("should format Svelte 5 declaration tags", async () => {
    const input = `{#each boxes as box}
{const area = box.width * box.height}
{const label = \`\${box.width} x \${box.height} = \${area}\`}
<p>{label}</p>
{/each}
`;
    const result = await format("App.svelte", input, { svelte: {} });
    expect(result.errors).toStrictEqual([]);
    expect(result.code).toMatchSnapshot();
  });
});
