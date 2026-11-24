import 'server-only';

/** r43 additions: byte-identical read-tail SQL sources. Each helper renders
 * exactly the literal it replaces; callers keep their own binds, locks and
 * supplied statements. No r17-r42 helper, FOR SHARE/FOR UPDATE lock, LOCK TABLE
 * statement or spaced JOIN constant is changed. Only owner-supplied literals
 * (kind lists and predicate/ORDER BY tails) vary, never caller input. */

/** Active organization-node option list by kind. Family: the project-worker
 * startup node list, the records request-page option list and the people
 * directory option list. Each call renders exactly the replaced literal; only
 * the IN-kind list and the trailing predicate/ORDER BY fragment vary. */
export const activeNodeKindList = (kinds: string, tail: string) =>
  `SELECT id,kind,display_name,branch_id FROM organization_nodes WHERE company_id=? AND status='active' AND kind IN (${kinds})${tail}`;
