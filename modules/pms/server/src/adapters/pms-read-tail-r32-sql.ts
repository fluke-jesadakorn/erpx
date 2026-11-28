/** Byte-identical and parameterized PMS read-tail r32 SQL shared across owner-local sources.
 * Text only: every consumer keeps its own binds, result type and transaction;
 * nothing is cached or executed here. The trailing ORDER BY is a closed boolean
 * variant, never caller SQL.
 * Locking reads, r17-r31 fragments and the native day-report registry are untouched. */

/** Closed booking-allocation refund projection. The optional ordering is a
 * closed flag; the projection, predicates and bind placeholder are owned text. */
export const pmsBookingAllocationRefundSql = (ordered: boolean): string =>
  `SELECT a.*,COALESCE((SELECT SUM((part->>'amount')::bigint) FROM pms_booking_refund_requests r CROSS JOIN LATERAL jsonb_array_elements(r.allocations_json::jsonb) part WHERE r.payment_id=a.payment_id AND part->>'entryId'=a.entry_id),0) refunded FROM pms_booking_allocations a WHERE a.stay_id=?${ordered ? ' ORDER BY a.created_at,a.id' : ''}`;
