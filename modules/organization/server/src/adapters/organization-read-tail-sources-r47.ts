import 'server-only';

/** r47 additions: byte-identical read-tail SQL source. The helper renders
 * exactly the literal it replaces; callers keep their own binds, locks and
 * supplied statements. No r17-r46 helper, FOR SHARE/FOR UPDATE lock, LOCK TABLE
 * statement or spaced JOIN constant is changed. Only the owner-supplied
 * predicate tail varies, never caller input. */

/** Worker-company scheduled-rollout guard. Family: the collaboration reply and
 * executive generation workspace company guards. Each call renders exactly the
 * replaced literal; only the scheduled-feature predicate tail varies. */
export const workerCompanyRolloutGuard = (predicate: string) =>
  `EXISTS(SELECT 1 FROM organization_nodes worker_company WHERE worker_company.id=? AND ${predicate})`;
