import 'server-only';

/** r49 additions: byte-identical read-tail SQL source. The helpers render
 * exactly the literals they replace; callers keep their own binds, locks and
 * supplied statements. No r17-r48 helper, FOR SHARE/FOR UPDATE lock, LOCK TABLE
 * statement or spaced JOIN constant is changed. Only the owner-supplied
 * predicate/order tails vary, never caller input. */

/** Branch-node option list. Family: the pms command-facts reader and the pos
 * facts reader. The runtime-selected active/id tail and the pos ORDER BY tail
 * are supplied verbatim by each caller. */
export const branchNodeOptions = (tail: string) =>
  `SELECT id,display_name name FROM organization_nodes WHERE company_id=? AND kind='branch' AND ${tail}`;

/** Full node selection projection. Family: the organization ports `nodes` and
 * `node` readers. The WHERE/ORDER BY clause is supplied verbatim by each caller;
 * bind arrays stay at the call sites. */
export const fullNodeSelection = (where: string) =>
  `SELECT id,kind,slug,display_name,branch_id,status,hierarchy_revision FROM organization_nodes WHERE ${where}`;

/** Collaboration messaging nodes EXISTS guard. Family: the managed-team and
 * room-organization projections. Only the correlated node predicate varies. */
export const messagingNodeExists = (predicate: string) =>
  `EXISTS(SELECT 1 FROM organization_nodes n WHERE ${predicate})`;
