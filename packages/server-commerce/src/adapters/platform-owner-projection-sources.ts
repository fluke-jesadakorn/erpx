import "server-only";
/** Closed owner projections for Platform, Organization and Procurement joins. Original filters, bindings and supplied transactions remain with the caller. */
export const platformOwnerProjectionSources = Object.freeze({
  commerce_fulfillment_lines: "SELECT \"id\",\"fulfillment_id\",\"order_line_id\",\"quantity_milli\",\"value_minor\",\"subtotal_minor\",\"tax_minor\",\"stock_currency\" FROM commerce_fulfillment_lines",
  commerce_fulfillments: "SELECT \"id\",\"order_id\",\"number\",\"fulfillment_date\",\"created_by\",\"created_at\",\"journal_id\",\"invoice_id\",\"version\",\"evidence_note\" FROM commerce_fulfillments",
  commerce_order_lines: "SELECT \"id\",\"order_id\",\"product_id\",\"description\",\"quantity_milli\",\"fulfilled_milli\",\"price_minor\",\"tax_bps\",\"subtotal_minor\",\"tax_minor\",\"ordinal\",\"list_price_minor\",\"discount_bps\" FROM commerce_order_lines",
  commerce_orders: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders",
  commerce_products: "SELECT \"id\",\"company_id\",\"sku\",\"name\",\"unit\",\"price_minor\",\"tax_bps\",\"status\",\"version\",\"tracking_mode\" FROM commerce_products",
  commerce_settings: "SELECT \"company_id\",\"chart_id\",\"currency\",\"receivable_account_id\",\"payable_account_id\",\"revenue_account_id\",\"inventory_account_id\",\"clearing_account_id\",\"cogs_account_id\",\"input_tax_account_id\",\"output_tax_account_id\",\"cash_account_id\" FROM commerce_settings",
  commerce_warehouses: "SELECT \"id\",\"company_id\",\"branch_id\",\"code\",\"name\",\"status\",\"version\" FROM commerce_warehouses",
} as const);
