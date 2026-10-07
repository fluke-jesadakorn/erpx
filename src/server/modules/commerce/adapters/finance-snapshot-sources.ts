import "server-only";
/** Closed refund return join retains the original supplied transaction snapshot. */
export function financeSnapshotSources(){return {
  "refundReturn": "JOIN commerce_returns r ON r.id=o.return_id"
} as const;}
