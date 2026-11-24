/** Shared revocation probe heads for the r36 identity_access read-tail family.
 * The helper owns the single statement token, table projection and predicate
 * text. Callers pass only server-owned alias/placeholder expressions drawn from
 * a closed set; a closed-union discriminator selects the exact pre-dedupe head
 * (alias qualification, predicate order and original whitespace). No caller SQL,
 * lock, write, session authority, native kind or earlier-round helper changes. */

export type RevokedSessionProbeShape =
  | 'unaliased-session-first'
  | 'unaliased-company-first'
  | 'aliased-company-first'
  | 'aliased-session-first'
  | 'aliased-session-first-multiline';

/** Render a revoked-session existence probe byte-identically to its pre-dedupe
 * literal. `sessionExpr`, `companyExpr` and `userExpr` are fixed aliases or
 * placeholders authored by the owner call site, never caller-supplied SQL. */
export const revokedSessionHeadSql = (
  sessionExpr: string,
  companyExpr: string,
  userExpr: string,
  shape: RevokedSessionProbeShape,
): string => {
  const aliased = shape.startsWith('aliased-');
  const from = aliased ? 'iam_revoked_sessions r' : 'iam_revoked_sessions';
  const col = aliased ? 'r.' : '';
  const gap = shape === 'aliased-session-first-multiline' ? '\n      ' : ' ';
  const where = shape.endsWith('company-first')
    ? `${col}company_id=${companyExpr} AND ${col}user_id=${userExpr} AND ${col}session_id=${sessionExpr}`
    : `${col}session_id=${sessionExpr}${gap}AND ${col}company_id=${companyExpr} AND ${col}user_id=${userExpr}`;
  return `SELECT 1 FROM ${from} WHERE ${where}`;
};
