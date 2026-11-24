/** Shared SQL text for byte-identical Commerce settings reads and source locks.
 * Text only: every call site still prepares and executes its own statement and
 * no query result is ever cached here. */
export const MFG_POS_LOCK_COMMAND_RECEIPTS = 'LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE';
export const MFG_POS_LOCK_PRODUCTS_WAREHOUSES = 'LOCK TABLE commerce_products,commerce_warehouses IN SHARE ROW EXCLUSIVE MODE';
export const MFG_POS_LOCK_SETTINGS_SHARE = 'LOCK TABLE commerce_settings IN SHARE MODE';
export const MFG_POS_LOCK_POSTING_SOURCES_SHARE = 'LOCK TABLE commerce_settings,commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_goods_valuations,commerce_stock_moves,commerce_stock,commerce_warehouses,commerce_products IN SHARE MODE';

/** 11-column Commerce→posting mapping state, one company_id bind. */
export const COMMERCE_POSTING_MAPPING_STATE_SQL = 'SELECT chart_id "chartId",currency,receivable_account_id "receivableAccountId",payable_account_id "payableAccountId",revenue_account_id "revenueAccountId",inventory_account_id "inventoryAccountId",clearing_account_id "clearingAccountId",cogs_account_id "cogsAccountId",input_tax_account_id "inputTaxAccountId",output_tax_account_id "outputTaxAccountId",cash_account_id "cashAccountId" FROM commerce_settings WHERE company_id=?';

export const COMMERCE_SETTINGS_BY_COMPANY_SQL = 'SELECT * FROM commerce_settings WHERE company_id=?';
export const COMMERCE_CURRENCY_BY_COMPANY_SQL = 'SELECT currency FROM commerce_settings WHERE company_id=?';
