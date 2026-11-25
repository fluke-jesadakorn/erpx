// Shared owner-local all-columns by-key family. Both Records callers render
// their exact prior SQL byte-for-byte; the spaced read head now lives only in
// the shared r32 renderer.
import { projectionColumnHeadSql } from './read-tail-r32-sql';
export const selectAllWhereSql = (table: string, predicate: string) =>
  projectionColumnHeadSql('*', table, ` WHERE ${predicate}`);
