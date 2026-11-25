// Shared owner-local column-projection head for the Records read tail. The
// request_ai_* projection family (r16/r30) and the employee_requests column
// family (r21/r30) render the same column head; r32 admits that byte-identical
// head once here and both delegate. The one spaced read keyword lives only in
// this renderer, so no member carries an inline read literal.
export const projectionColumnHeadSql = (columns: string, table: string, tail = '') =>
  `SELECT ${columns} FROM ${table}${tail}`;
