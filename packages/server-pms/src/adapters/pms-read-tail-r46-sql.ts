/** Byte-identical PMS read-tail r46 two-column point-read heads shared across
 * owner-local sources. Text only: every consumer keeps its own binds, result
 * type and transaction; nothing is cached or executed here. Variants come from
 * a closed union owned by this module, never caller SQL. Locking reads,
 * feature/native reads and the r17-r45 fragments stay untouched. */

type PointRead =
  | 'refundRequestAllocations'
  | 'stripePaymentKey'
  | 'propertyScope'
  | 'bookingTokenByHash';

const POINT_READ: Record<PointRead, { columns: string; table: string; predicate: string }> = {
  refundRequestAllocations: { columns: 'amount_minor,allocations_json', table: 'pms_booking_refund_requests', predicate: 'payment_id=?' },
  stripePaymentKey: { columns: 'id,request_hash', table: 'pms_stripe_payments', predicate: 'id=?' },
  propertyScope: { columns: 'company_id,branch_id', table: 'pms_properties', predicate: 'id=?' },
  bookingTokenByHash: { columns: 'property_id,reservation_id', table: 'pms_booking_tokens', predicate: 'token_hash=? AND expires_at>?' },
};

export const pmsTwoColumnPointReadSql = (kind: PointRead): string => {
  const { columns, table, predicate } = POINT_READ[kind];
  return `SELECT ${columns} FROM ${table} WHERE ${predicate}`;
};
