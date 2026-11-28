/** Byte-identical PMS read-tail r42 query heads shared across owner-local sources.
 * Text only: every consumer keeps its own suffix, binds, result type and
 * transaction; nothing is cached or executed here. Variants come from closed
 * unions owned by this module, never caller SQL. Locking reads, the r17-r41
 * fragments and the native read registry stay untouched. */

/** Rank 1: stay-existence witness head. The correlated key is a closed union;
 * trailing predicates and all binds stay at each call site. */
type StayWitnessField = 'roomId' | 'reservationId';
export const pmsStayExistsHeadSql = (field: StayWitnessField): string =>
  `EXISTS(SELECT 1 FROM pms_records st WHERE st.property_id=r.property_id AND st.kind='stay' AND st.data_json::jsonb->>'${field}'=r.id`;

/** Rank 2: pending correction fact head. The scoping tail stays at sites. */
export const PMS_CORRECTION_FACT_HEAD_SQL = "SELECT 'correction',c.id,c.version,c.status FROM pms_charge_corrections c";

/** Rank 3: selected-record ordering tail shared by the dashboard/section reads. */
export const PMS_ORDERED_RECORDS_TAIL_SQL = 'SELECT * FROM selected_records ORDER BY updated_at,id';
