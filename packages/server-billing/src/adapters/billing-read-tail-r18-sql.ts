/** Byte-identical Billing read-tail r18 text shared across owner-local readers.
 * Text only: every caller still prepares and executes its own statement with its
 * own binds and result-type alias; nothing is cached or executed here. */

/** Latest plan name for an enrollment, resolved by descending plan revision.
 * The enrollment alias `e` and projection alias `p` are fixed by both consumers'
 * subscription CTEs; the JOIN text itself is declared once here. Its inner
 * projection is routed through the r35 head without changing a rendered byte. */
import { billingProjectionSql } from './billing-read-tail-r35-sql';
export const BILLING_PLAN_NAME_LATERAL_SQL =
  `LEFT JOIN LATERAL (${billingProjectionSql('name', 'FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1')}) p ON TRUE`;
