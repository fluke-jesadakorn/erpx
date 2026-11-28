import 'server-only';

/** r44 additions: byte-identical read-tail statement source. Each helper renders
 * exactly the literal it replaces; callers keep their own binds, locks and
 * supplied statements. No r17-r43 helper, FOR SHARE/FOR UPDATE lock, LOCK TABLE
 * statement or spaced JOIN constant is changed. Only owner-supplied literals
 * (node kind and optional branch scope) vary, never caller input. */

/** Front-company slug lookup for a branch or a top-level department. Each call
 * renders exactly the replaced literal; only the owner-supplied kind and branch
 * scope fragment vary. */
export const nodeSlugByKind = (kind: string, branchScope: string) =>
  `SELECT slug FROM organization_nodes WHERE company_id=? AND kind='${kind}' AND slug=?${branchScope} AND status='active'`;
