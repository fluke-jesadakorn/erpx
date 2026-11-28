import 'server-only';

/** r50 additions: byte-identical read-tail SQL source. The helpers render
 * exactly the literals they replace; callers keep their own binds, locks and
 * supplied statements. No r17-r49 helper, FOR SHARE/FOR UPDATE lock, LOCK TABLE
 * statement or spaced JOIN constant is changed. Only owner-supplied tails and
 * projections vary, never caller input. */

/** Node-existence branch guard. Family: the organization node-version stage,
 * the planning branch guard and the hire-placement guard. The predicate tail is
 * supplied verbatim by each caller; bind arrays stay at the call sites. */
export const nodeExistsGuard = (tail: string) =>
  `EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND ${tail})`;

/** Active-position lookup. Family: the staff-defaults position probe, the
 * people-evaluation save-definition probe and the gpo single-position lookup.
 * The projection and the predicate tail are supplied verbatim by each caller;
 * bind arrays stay at the call sites. */
export const activePositionLookup = (projection: string, tail: string) =>
  `SELECT ${projection} FROM organization_positions WHERE id=? AND company_id=? AND ${tail}`;
