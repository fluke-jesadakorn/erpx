/** Canonical owner-local fixed-SQL for the active Marketing form-content source.
 * The presentation assertion and the content reader share this one statement,
 * so no duplicate SQL literal remains. Callers supply only the projection;
 * the owner-owned source predicate stays here. */
export const marketingFormContentSql = (projection: string): string =>
  `SELECT ${projection} FROM marketing_assets WHERE id=? AND company_id=? AND kind='content' AND status='active' AND version=?`;
