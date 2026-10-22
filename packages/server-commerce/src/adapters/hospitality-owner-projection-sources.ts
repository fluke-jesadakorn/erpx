import "server-only";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  commerce_fulfillment_evidence: "SELECT \"fulfillment_id\",\"attachment_id\",\"linked_by\",\"linked_at\" FROM commerce_fulfillment_evidence",
  commerce_orders: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders",
  commerce_products: "SELECT \"id\",\"company_id\",\"sku\",\"name\",\"unit\",\"price_minor\",\"tax_bps\",\"status\",\"version\",\"tracking_mode\" FROM commerce_products",
  commerce_settings: "SELECT \"company_id\",\"chart_id\",\"currency\",\"receivable_account_id\",\"payable_account_id\",\"revenue_account_id\",\"inventory_account_id\",\"clearing_account_id\",\"cogs_account_id\",\"input_tax_account_id\",\"output_tax_account_id\",\"cash_account_id\" FROM commerce_settings",
  commerce_warehouses: "SELECT \"id\",\"company_id\",\"branch_id\",\"code\",\"name\",\"status\",\"version\" FROM commerce_warehouses",
} as const;

/** Query-free source lease for actual hospitality Stock ownership joins. */
export function hospitalitySettlementReferenceReadLocks(database:import("@erpx/server-runtime/public/runtime/types").SqlDatabase){return [database.prepare("LOCK TABLE commerce_warehouses,commerce_products IN SHARE MODE")];}
