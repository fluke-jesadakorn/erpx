import "server-only";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  billing_accounts: "SELECT \"holding_id\",\"version\",\"payment_mode\" FROM billing_accounts",
} as const;
