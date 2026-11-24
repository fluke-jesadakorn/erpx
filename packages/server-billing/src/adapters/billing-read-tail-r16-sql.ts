/** Byte-identical Billing read-tail text shared across owner-local repositories.
 * Text only: every caller still prepares and executes its own statement with its
 * own binds and result-type alias; nothing is cached or executed here. */
import {billingPointReadSql} from './billing-read-tail-r29-sql';

/** Sandbox designation or effective approval permits rollout. The two guards are
 * identical in the command and target repositories; only the row alias differs. */
export const BILLING_ROLLOUT_ELIGIBILITY_SQL = `SELECT 1 FROM billing_target_sandbox_holdings WHERE holding_id=$1
     UNION ALL SELECT 1 FROM billing_target_rollout_approvals
     WHERE holding_id=$1 AND effective_month<=$2 LIMIT 1`;

/** Legacy platform-usage enrollment overlapping the measured month. */
export const BILLING_LEGACY_ENROLLMENT_OVERLAP_SQL = billingPointReadSql('1', 'billing_enrollments', `holding_id=$1 AND plan_id<>'platform_usage'
     AND started_at<$3 AND COALESCE(ended_at,effective_at,9223372036854775807)>$2`);

/** Non-void invoice exists in the billing month. */
export const BILLING_MONTH_INVOICE_SQL = billingPointReadSql('1', `billing_invoices i JOIN billing_periods p ON p.id=i.period_id
    `, `i.holding_id=$1 AND p.month=$2 AND i.status<>'void'`);
