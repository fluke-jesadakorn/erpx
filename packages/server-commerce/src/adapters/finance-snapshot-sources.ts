import "server-only";
/** Closed refund return join retains the original supplied transaction snapshot. */
export function financeSnapshotSources(){return {
  "refundReturn": "JOIN commerce_returns r ON r.id=o.return_id",
  "optionalRefundReturn": "LEFT JOIN commerce_returns r ON r.id=o.return_id AND r.company_id=o.company_id"
} as const;}
