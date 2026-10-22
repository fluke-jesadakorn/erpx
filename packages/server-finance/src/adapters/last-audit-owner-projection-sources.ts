import "server-only";
/** Closed owner read projections; consumers retain original filters, aliases and binds in their supplied statement/transaction. */
export const lastAuditOwnerProjectionSources = {
  company_fx_rates: "SELECT \"id\",\"company_id\",\"currency\",\"effective_date\",\"currency_per_usd\",\"confirmed_by\",\"confirmed_at\",\"note\" FROM company_fx_rates",
  money_transaction_snapshots: "SELECT \"company_id\",\"source_type\",\"source_id\",\"amount_kind\",\"transaction_date\",\"currency\",\"currency_scale\",\"amount_units\",\"usd_units\",\"fx_rate_id\",\"used_latest_fallback\",\"captured_at\",\"original_source_type\",\"original_source_id\",\"original_amount_kind\",\"carried_source_type\",\"carried_source_id\",\"carried_amount_kind\",\"carried_allocation_offset_units\" FROM money_transaction_snapshots",
} as const;
