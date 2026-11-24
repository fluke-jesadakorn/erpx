/** Byte-identical Billing read-tail r23 SQL text shared by the two pending
 * stripe_inbox drain readers. Text only: each caller prepares and executes its
 * own statement with its own binds and result type; the customer-correlation
 * tail differs between the holding-matched and legacy-owner queues. */
export function billingPendingInboxSql(tail: string) {
  return `SELECT * FROM stripe_inbox WHERE state='pending' AND next_at<=$1 AND ${tail}`;
}
