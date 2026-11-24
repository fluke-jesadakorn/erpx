// Shared owner-local EXISTS version-fence family. Both Records callers render
// their exact prior SQL byte-for-byte; only the admitted table and tail differ.
export const ownerVersionExistsSql = (table: string, tail: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?${tail})`;
