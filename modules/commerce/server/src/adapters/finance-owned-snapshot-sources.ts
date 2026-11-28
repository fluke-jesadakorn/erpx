import "server-only";
/** Closed native projections with explicit owned columns for named Finance reads.
 * The consumer retains its supplied transaction and one-statement snapshot;
 * this contract accepts no table name, predicate, SQL, or query from a caller. */
export function financeOwnedSnapshotSources(){const ordersFromO="FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o";
const fulfillmentLinesJoinFl="JOIN (SELECT \"id\",\"fulfillment_id\",\"order_line_id\",\"quantity_milli\",\"value_minor\",\"subtotal_minor\",\"tax_minor\",\"stock_currency\" FROM commerce_fulfillment_lines) fl";
const fulfillmentsFromF="FROM (SELECT \"id\",\"order_id\",\"number\",\"fulfillment_date\",\"created_by\",\"created_at\",\"journal_id\",\"invoice_id\",\"version\",\"evidence_note\",\"completion_kind\",\"completion_note\" FROM commerce_fulfillments) f";
const ordersJoinO="JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o";
const returnsFromR="FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"order_id\",\"fulfillment_id\",\"invoice_id\",\"kind\",\"number\",\"return_date\",\"reason\",\"status\",\"subtotal_minor\",\"tax_minor\",\"total_minor\",\"stock_journal_id\",\"credit_journal_id\",\"version\",\"requested_by\",\"approved_by\",\"created_at\",\"updated_at\",\"credited_minor\",\"refund_minor\",\"base_credit_minor\",\"base_credited_minor\",\"base_refund_minor\",\"fx_journal_id\",\"goods_fx_journal_id\" FROM commerce_returns) r";
const returnsJoinR="JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"order_id\",\"fulfillment_id\",\"invoice_id\",\"kind\",\"number\",\"return_date\",\"reason\",\"status\",\"subtotal_minor\",\"tax_minor\",\"total_minor\",\"stock_journal_id\",\"credit_journal_id\",\"version\",\"requested_by\",\"approved_by\",\"created_at\",\"updated_at\",\"credited_minor\",\"refund_minor\",\"base_credit_minor\",\"base_credited_minor\",\"base_refund_minor\",\"fx_journal_id\",\"goods_fx_journal_id\" FROM commerce_returns) r";
const returnsFromNamed="FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"order_id\",\"fulfillment_id\",\"invoice_id\",\"kind\",\"number\",\"return_date\",\"reason\",\"status\",\"subtotal_minor\",\"tax_minor\",\"total_minor\",\"stock_journal_id\",\"credit_journal_id\",\"version\",\"requested_by\",\"approved_by\",\"created_at\",\"updated_at\",\"credited_minor\",\"refund_minor\",\"base_credit_minor\",\"base_credited_minor\",\"base_refund_minor\",\"fx_journal_id\",\"goods_fx_journal_id\" FROM commerce_returns) commerce_returns";
return {
  "commerce_facts_commerce_orders_o_from": ordersFromO,
  "commerce_facts_commerce_fulfillment_lines_fl_join": fulfillmentLinesJoinFl,
  "commerce_facts_commerce_fulfillments_f_from": fulfillmentsFromF,
  "commerce_goods_accounting_facts_commerce_fulfillment_lines_fl_join": fulfillmentLinesJoinFl,
  "commerce_goods_accounting_facts_commerce_orders_o_join": ordersJoinO,
  "commerce_goods_accounting_facts_commerce_fulfillments_f_from": fulfillmentsFromF,
  "commerce_joined_facts_commerce_settings_s_from": "FROM (SELECT \"company_id\",\"chart_id\",\"currency\",\"receivable_account_id\",\"payable_account_id\",\"revenue_account_id\",\"inventory_account_id\",\"clearing_account_id\",\"cogs_account_id\",\"input_tax_account_id\",\"output_tax_account_id\",\"cash_account_id\" FROM commerce_settings) s",
  "commerce_joined_facts_commerce_warehouses_w_join": "JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"code\",\"name\",\"status\",\"version\" FROM commerce_warehouses) w",
  "commerce_joined_facts_commerce_orders_o_from": ordersFromO,
  "commerce_joined_facts_commerce_fulfillments_f_from": fulfillmentsFromF,
  "commerce_joined_facts_commerce_returns_r_from": returnsFromR,
  "commerce_joined_facts_commerce_orders_o_join": ordersJoinO,
  "commerce_return_accounting_facts_commerce_returns_r_from": returnsFromR,
  "finance_history_commerce_returns_r_from": returnsFromR,
  "finance_tax_register_commerce_returns_r_join": returnsJoinR,
  "finance_tax_register_commerce_returns_r_from": returnsFromR,
  "finance_tax_register_commerce_returns_from": returnsFromNamed,
  "finance_commerce_returns_from": returnsFromNamed,
  "refunds_commerce_returns_r_join": returnsJoinR,
  "supplier_bill_draft_void_commerce_fulfillments_f_join": "JOIN (SELECT \"id\",\"order_id\",\"number\",\"fulfillment_date\",\"created_by\",\"created_at\",\"journal_id\",\"invoice_id\",\"version\",\"evidence_note\",\"completion_kind\",\"completion_note\" FROM commerce_fulfillments) f",
} as const;}
