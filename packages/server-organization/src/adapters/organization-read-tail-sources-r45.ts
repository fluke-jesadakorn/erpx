import 'server-only';

/** r45 additions: byte-identical read-tail SQL source. The helper renders
 * exactly the literal it replaces; callers keep their own binds, locks and
 * supplied statements. No r17-r44 helper, FOR SHARE/FOR UPDATE lock, LOCK TABLE
 * statement or spaced JOIN constant is changed. Only the owner-supplied
 * predicate tail varies, never caller input. */

/** Child-node existence under a company parent. Family: the hasChildren and
 * slugConflict reads in the organization adapter. Each call renders exactly
 * the replaced literal; only the trailing predicate varies. */
export const nodeChildExists = (tail: string) =>
  `SELECT 1 FROM organization_nodes WHERE company_id=? AND parent_id=? AND ${tail} LIMIT 1`;
