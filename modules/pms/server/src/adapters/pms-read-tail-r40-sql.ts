/** Byte-identical PMS read-tail r40 query heads shared across owner-local sources.
 * Text only: every consumer keeps its own suffix, binds, result type and
 * transaction; nothing is cached or executed here. Variants come from closed
 * unions owned by this module, never caller SQL. Locking reads, the r17-r39
 * fragments and the native read registry stay untouched. */
import { hospitalityOwnerProjectionSources as organizationProjectionSources } from '@erpx/organization-server/public/hospitality-owner-projection-sources';

/** Rank 3 (booking receipt / change point): the two purchase replay existence
 * witnesses share one head; the relation name comes from a closed union. */
type PmsBookingReplayKind = 'receipt' | 'change';
const PMS_BOOKING_REPLAY_TABLE: Record<PmsBookingReplayKind, 'receipts' | 'changes'> = {
  receipt: 'receipts',
  change: 'changes',
};
export const pmsBookingReplaySql = (kind: PmsBookingReplayKind): string =>
  `SELECT 1 FROM pms_booking_${PMS_BOOKING_REPLAY_TABLE[kind]} WHERE id=? AND property_id=?`;

/** Rank 4 (branch label alias): the occupancy/guest-balance and ADR/RevPAR
 * branch labels differ only by the display-name subquery alias; both aliases
 * come from a closed union owned here. */
type PmsBranchLabelAlias = 'n' | 'n2';
export const pmsBranchLabelSql = (alias: PmsBranchLabelAlias): string =>
  `COALESCE((SELECT ${alias}.display_name FROM (${organizationProjectionSources.organization_nodes}) ${alias} WHERE ${alias}.id=p.branch_id AND ${alias}.company_id=p.company_id),'Unassigned branch')`;

/** Rank 6 (refund non-lock): two read-only witnesses share this head; the
 * row-lock read sites keep their own inline statements. */
export const PMS_REFUND_NON_LOCK_HEAD_SQL = 'SELECT * FROM pms_booking_refund_requests WHERE ';
