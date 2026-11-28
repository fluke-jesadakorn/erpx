/** Byte-identical Billing read-tail r33 enrollment subscription head.
 * Text only: both executive readers supply their own projection, FROM clause,
 * holding predicate and indentation; the r18 plan-name lateral is reused, not
 * copied. Nothing is executed or cached here. */
import { BILLING_PLAN_NAME_LATERAL_SQL } from './billing-read-tail-r18-sql';
import { billingProjectionSql } from './billing-read-tail-r35-sql';
export const billingEnrollmentSubscriptionSql = (projection: string, from: string, holding: string, pad: string) =>
  `${pad}${billingProjectionSql(projection, `\n${from}\n${pad}${BILLING_PLAN_NAME_LATERAL_SQL}\n${pad}WHERE ${holding}`)}`;
