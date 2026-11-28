import { billingProjectionSql } from './billing-read-tail-r35-sql';

/** Byte-identical Billing read-tail r32 legacy beta_billing subscription head.
 * Text only: both executive readers supply their own projection, indentation,
 * binds and result type; nothing is executed or cached here. The r43 tail-aware
 * fold routes the shared projection head through r35's generic renderer. */
export const billingLegacySubscriptionSql = (projection: string, pad: string) =>
  `${pad}${billingProjectionSql(projection, `\n${pad}FROM beta_billing b WHERE b.holding_id=? AND b.subscription_id IS NOT NULL AND b.status IN ('active','trialing')\n${pad}  AND (b.current_period_end IS NULL OR b.current_period_end*1000>?)`)}`;
