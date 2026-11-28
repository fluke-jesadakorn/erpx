/** Byte-identical PMS read-tail r22 SQL fragments shared across owner-local readers.
 * Text only: every call site still prepares/executes its own statement with its own
 * binds, filters and result type; nothing is cached or executed here.
 * Locking reads (FOR UPDATE / FOR SHARE / advisory) stay inline at call sites. */

import { pmsSourcesSql } from "@erpx/pms-server/adapters/pms/source-query";

/** Stripe refund projection columns shared by overview, detail and compat readers. */
export const PMS_STRIPE_REFUND_COLUMNS_SQL = "SELECT id,property_id,guest_account_id,amount_minor,refund_amount_minor,refund_state,last_error,refund_reason,payer_email FROM pms_stripe_payments";

/** Parameterized recipient FROM fragment used by source-scoped point and set reads. */
export const PMS_RECIPIENT_FROM_SQL = `FROM (SELECT ?::text user_id) recipient CROSS JOIN (${pmsSourcesSql}) pr`;

/** Scheduled feature-rollout membership read; module list stays call-site owned. */
export const PMS_ROLLOUT_ENABLED_SQL = "SELECT module_id FROM company_feature_rollouts f WHERE company_id=?";

/** Active feature-rollout read; action predicate stays call-site owned. */
export const PMS_ROLLOUT_ACTIVE_SQL = "SELECT 1 FROM company_feature_rollouts f WHERE company_id=? AND module_id=? AND";
