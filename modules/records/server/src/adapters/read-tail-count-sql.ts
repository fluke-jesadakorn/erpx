// Shared owner-local count(*) family over request_ai_usage. Both Records quota
// subqueries render their exact prior SQL byte-for-byte; the spaced read head
// now lives only in the shared r32 renderer.
import { projectionColumnHeadSql } from './read-tail-r32-sql';
export const requestAiUsageCountSql = (predicate: string) =>
  `(${projectionColumnHeadSql('count(*)', 'request_ai_usage', ` WHERE ${predicate}`)})`;
