const tsParser = {
  parseForESLint(code: string) {
    return {
      ast: {
        type: "Program",
        sourceType: "module",
        body: [],
        range: [0, code.length],
        loc: {
          start: { line: 1, column: 0 },
          end: { line: 1, column: code.length },
        },
        comments: [],
        tokens: [],
      },
      visitorKeys: {
        Program: ["body"],
      },
    };
  },
};

export default tsParser;
