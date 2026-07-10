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
  meta: { name: "svelte-parser-sentinel" },
  rules: {
    "should-not-run": {
      create(context) {
        return {
          Program() {
            context.report({
              message: "native .svelte files must not invoke external JS rules",
              node: SPAN,
            });
          },
        };
      },
    },
  },
};

export default plugin;
