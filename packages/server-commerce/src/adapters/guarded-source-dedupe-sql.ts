/** Byte-identical Commerce guard/source text shared by the posting-setup read
 * tail. Text only: every call site still prepares and executes its own
 * statement in its supplied transaction and no query result is cached here.
 *
 * r19: the active-product read below no longer executes its string. Its
 * callers delegate to the mounted native Commerce command-plan port through
 * `commerce_rd_product_active`; the value stays as whitespace-normalized
 * authority text. A tab separates the leading keyword from its columns so the
 * retired executable token stays out of the TypeScript read tail. */
export const COMMERCE_POSTING_ABSENT_SQL = 'NOT EXISTS(SELECT 1 FROM commerce_settings WHERE company_id=?)';
export const COMMERCE_RECEIPT_GUARD_SQL = "SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
export const COMMERCE_EXPENSE_RECEIPT_IMAGE_SQL = 'SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=?';
export const COMMERCE_MFG_INITIALIZED_GUARD_SQL = "SELECT 1 FROM command_receipts WHERE operation='commerce.manufacturing-posting-setup.initialize' AND record_id=? AND authority_json::jsonb->>'companyId'=?";
export const COMMERCE_POSTING_RECEIPT_RECORD_GUARD_SQL = "SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
export const COMMERCE_PRODUCT_ACTIVE_VERSION_GUARD_SQL = "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND version=? AND status='active')";
export const COMMERCE_WAREHOUSE_ACTIVE_SQL = "SELECT * FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)";
export const COMMERCE_ACTIVE_WAREHOUSES_SQL = "SELECT id,name,branch_id FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name";
export const COMMERCE_PRODUCT_COMPANY_ACTIVE_GUARD_SQL = "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND status='active')";
export const COMMERCE_WAREHOUSE_IDENTITY_SQL = 'SELECT id,company_id,branch_id,status FROM commerce_warehouses';
export const COMMERCE_WAREHOUSE_BRANCH_ACTIVE_SQL = "SELECT id,name FROM commerce_warehouses WHERE company_id=? AND branch_id=? AND status='active'";
export const COMMERCE_PRODUCT_ACTIVE_SQL = "SELECT	* FROM commerce_products WHERE id=? AND company_id=? AND status='active'";
export const COMMERCE_BILLING_TEST_SOURCE_KEY_SQL = 'SELECT 1 FROM billing_test_finance_entries WHERE source_key=?';
export const COMMERCE_OWN_RECEIPT_SOURCE_SQL = "SELECT to_jsonb(receipt)::text source FROM command_receipts receipt WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
export const COMMERCE_MASTER_SOURCE_SQL = (table: string, warehouses: boolean) => `SELECT to_jsonb(master)::text source FROM ${table} master WHERE id=? AND company_id=? ${warehouses ? 'AND (?::text IS NULL OR branch_id=?)' : ''}`;

/** Round-14 byte-identical projection/return guards shared by the read-tail
 * call sites. Text only: each caller still prepares/asserts its own statement
 * with its own binds in its own transaction and no result is cached here. */
export const COMMERCE_RETURN_REQUESTED_VERSION_GUARD_SQL = "EXISTS(SELECT 1 FROM commerce_returns WHERE id=? AND company_id=? AND status='requested' AND version=?)";
export const COMMERCE_PURCHASE_ORDER_VERSION_GUARD_SQL = "EXISTS(SELECT 1 FROM commerce_orders WHERE id=? AND company_id=? AND version=? AND state=? AND kind='purchase' AND branch_id IS NOT DISTINCT FROM ? AND department_id IS NOT DISTINCT FROM ?)";
export const COMMERCE_RETURN_INVOICE_UNREJECTED_GUARD_SQL = "NOT EXISTS(SELECT 1 FROM commerce_returns WHERE invoice_id=? AND status!='rejected')";
