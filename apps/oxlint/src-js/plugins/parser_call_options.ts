export function createRequiredParserCallOptions(
  filePath: string,
  parserOptions: Record<string, unknown> | null | undefined,
  sourceType?: unknown,
  ecmaVersion?: unknown,
): Record<string, unknown> {
  const options = { ...(parserOptions ?? {}) };

  if (sourceType != null && options.sourceType == null) {
    options.sourceType = sourceType;
  }

  if (ecmaVersion != null && options.ecmaVersion == null) {
    options.ecmaVersion = ecmaVersion;
  }

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
