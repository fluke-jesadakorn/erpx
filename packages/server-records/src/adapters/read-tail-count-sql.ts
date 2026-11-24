// Shared owner-local count(*) family over request_ai_usage. Both Records quota
// subqueries render their exact prior SQL byte-for-byte; only the predicate differs.
export const requestAiUsageCountSql = (predicate: string) =>
  `(SELECT count(*) FROM request_ai_usage WHERE ${predicate})`;
