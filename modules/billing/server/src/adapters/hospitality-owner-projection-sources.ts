import "server-only";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
import {billingAccountProjectionSql} from "./billing-read-tail-r30-sql";
export const hospitalityOwnerProjectionSources = {
  billing_accounts: billingAccountProjectionSql('"holding_id","version","payment_mode"'),
} as const;
