/** Byte-identical Billing read-tail r33 enrollment subscription head.
 * Text only: both executive readers supply their own projection, FROM clause,
 * holding predicate and indentation; the r18 plan-name lateral is reused, not
 * copied. Nothing is executed or cached here. */
import { BILLING_PLAN_NAME_LATERAL_SQL } from './billing-read-tail-r18-sql';
export const billingEnrollmentSubscriptionSql = (projection: string, from: string, holding: string, pad: string) =>
  `${pad}SELECT ${projection}
${from}
${pad}${BILLING_PLAN_NAME_LATERAL_SQL}
${pad}WHERE ${holding}`;
