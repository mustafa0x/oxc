export function createRequiredParserCallOptions(
  filePath: string,
  parserOptions: Record<string, unknown> | null | undefined,
): Record<string, unknown> {
  const options = { ...(parserOptions ?? {}) };

  if (options.filePath == null) options.filePath = filePath;

  options.loc = true;
  options.range = true;
  options.raw = true;
  options.tokens = true;
  options.comment = true;
  options.eslintVisitorKeys = true;
  options.eslintScopeManager = true;

  return options;
}
