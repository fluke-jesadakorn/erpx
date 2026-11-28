/** Byte-identical PMS read-tail r34 SQL shared across owner-local sources.
 * Text only: every consumer keeps its own WHERE/ORDER/LIMIT, binds, result
 * type and transaction; nothing is cached or executed here. The join-head
 * projection comes from a closed union owned by this module, never caller SQL.
 * Locking reads, r17-r33 fragments and the native read registry are untouched. */

/** Rank 1: byte-identical receipt-recovery predicate shared by both receipt heads. */
export const PMS_NEEDS_RECEIPT_RECOVERY_SQL =
  "(p.status='refund_review' AND r.state='requested' AND r.allocations_json::jsonb='[]'::jsonb AND NOT EXISTS(SELECT 1 FROM pms_records e WHERE e.property_id=p.property_id AND e.kind='entry' AND e.data_json::jsonb->>'sourceKey'='online-payment:'||p.id)) needs_receipt_recovery";

type RefundPaymentHead =
  | 'requestId'
  | 'brief'
  | 'count'
  | 'detail'
  | 'detailReceipt'
  | 'property'
  | 'fullReceipt';

const HEADS: Record<RefundPaymentHead, { cols: string; sep?: string }> = {
  requestId: { cols: 'r.id' },
  brief: { cols: 'r.id,r.amount_minor,r.state,r.reason' },
  count: { cols: 'count(*) n', sep: '\n    ' },
  detail: { cols: 'r.id,p.property_id,r.amount_minor,p.status,r.state refund_state,r.created_at,r.last_error,r.reason' },
  detailReceipt: { cols: `r.id,p.property_id,r.amount_minor,p.status,r.state refund_state,r.created_at,r.last_error, ${PMS_NEEDS_RECEIPT_RECOVERY_SQL}` },
  property: { cols: 'r.id,p.property_id' },
  fullReceipt: { cols: `r.id,p.id payment_id,p.property_id,r.amount_minor,p.status,r.state refund_state,r.last_error,r.reason,r.created_at, ${PMS_NEEDS_RECEIPT_RECOVERY_SQL}` },
};

/** Rank 2: closed-union refund/payment join head. WHERE/ORDER/LIMIT and binds stay at call sites. */
export const pmsRefundPaymentJoinHeadSql = (head: RefundPaymentHead): string => {
  const { cols, sep = ' ' } = HEADS[head];
  return `SELECT ${cols} FROM pms_booking_refund_requests r${sep}JOIN pms_booking_payments p ON p.id=r.payment_id`;
};
