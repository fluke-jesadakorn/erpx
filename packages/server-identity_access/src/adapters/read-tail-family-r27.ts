/** Shared query heads for the r27 identity_access read-tail families. Each
 * helper owns a single statement token and pins the projection, table and fixed
 * predicates; callers inject only the varying fragment or a literal table /
 * limit chosen at the call site. No lock, write, session authority, native
 * kind or earlier-round helper is changed. */
export const iamConnectionByIdSql = (predicate: string): string =>
  `SELECT * FROM iam_connections WHERE ${predicate}`;

export const auditFeedSql = (predicate: string): string =>
  `SELECT * FROM iam_audit WHERE company_id=?${predicate}`;

export const activeWorkContextAssignmentSql = (predicate: string): string =>
  `SELECT assignment_id FROM active_work_contexts WHERE company_id=? AND ${predicate}`;

export const newestPageSql = (table: string, limit: number): string =>
  `SELECT * FROM ${table} WHERE company_id=? ORDER BY created_at DESC LIMIT ${limit}`;

export const principalDisplaySql = (predicate: string): string =>
  `SELECT user_id,display_name FROM principals WHERE status='active' AND ${predicate}`;

export const memberByCompanyUserSql = (columns: string, style: '$' | '?' = '?'): string =>
  `SELECT ${columns} FROM iam_members WHERE company_id=${style === '$' ? '$1' : '?'} AND user_id=${style === '$' ? '$2' : '?'}`;

export const assignmentValidUntilSql = (table: string = 'assignments'): string =>
  `SELECT valid_until FROM ${table} WHERE id=$1`;
