// Shared owner-local EXISTS version-fence family. Both Records callers render
// their exact prior SQL byte-for-byte; only the admitted table and tail differ.
// r30: the fence delegates to the single-row EXISTS guard family with predicate
// 'id=? AND company_id=? AND version=?' + tail; the named export and binds are
// unchanged and byte identity is proven by the r30 oracle.
import { existsGuardSql } from './read-tail-exists-sql';
export const ownerVersionExistsSql = (table: string, tail: string) =>
  existsGuardSql(table, `id=? AND company_id=? AND version=?${tail}`);
