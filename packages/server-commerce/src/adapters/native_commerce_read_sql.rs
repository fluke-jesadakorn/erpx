//! Owner-owned caller-unit read plans for Commerce. Generated from the retired
//! TypeScript SELECT literals with the exact positional payloads; the mounted
//! `commerce/command-plan` route resolves the allow-listed kind here and returns
//! the statement for the caller to prepare and execute on its own open unit.
//!
//! This family covers small owner-local Commerce master/setup and source-image
//! reads on `commerce_warehouses`, `commerce_products`, `commerce_settings` and
//! `commerce_orders`. Reads carry no receipt/audit/
//! outbox ledger: the caller executes them on the same unit as the surrounding
//! command. No SQL is ever accepted from the wire and no TypeScript SQL remains
//! for these kinds.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound and `bind_count` the number
/// of positional arguments the caller must send.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeCommerceReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
}

/// Native successor of the TypeScript `pmsRolloutCommerceFacts.inventory`
/// active-warehouse existence read. The caller binds `[company_id]`.
pub const RD_PMS_WAREHOUSE_ACTIVE: &str = "commerce_rd_pms_warehouse_active";
const SQL_PMS_WAREHOUSE_ACTIVE: &str =
    "SELECT 1 FROM commerce_warehouses WHERE company_id=? AND status='active' LIMIT 1";
const READ_PMS_WAREHOUSE_ACTIVE: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_PMS_WAREHOUSE_ACTIVE,
    sql: SQL_PMS_WAREHOUSE_ACTIVE,
    bind_count: 1,
};

/// Native successor of the TypeScript `pmsRolloutCommerceFacts.commerce`
/// settings existence read. The caller binds `[company_id]`.
pub const RD_PMS_SETTINGS_EXISTS: &str = "commerce_rd_pms_settings_exists";
const SQL_PMS_SETTINGS_EXISTS: &str = "SELECT 1 FROM commerce_settings WHERE company_id=?";
const READ_PMS_SETTINGS_EXISTS: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_PMS_SETTINGS_EXISTS,
    sql: SQL_PMS_SETTINGS_EXISTS,
    bind_count: 1,
};

/// Native successor of the TypeScript `salesEditorOptions` active-product
/// options read. The caller binds `[company_id]`.
pub const RD_SALES_EDITOR_PRODUCTS: &str = "commerce_rd_sales_editor_products";
const SQL_SALES_EDITOR_PRODUCTS: &str =
    "SELECT id,sku,name,status,item_type,price_minor,tax_bps FROM commerce_products WHERE company_id=? AND status='active' ORDER BY sku";
const READ_SALES_EDITOR_PRODUCTS: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_SALES_EDITOR_PRODUCTS,
    sql: SQL_SALES_EDITOR_PRODUCTS,
    bind_count: 1,
};

/// Native successor of the TypeScript `salesEditorOptions` active-warehouse
/// options read. The caller binds `[company_id, branch_id, branch_id]`.
pub const RD_SALES_EDITOR_WAREHOUSES: &str = "commerce_rd_sales_editor_warehouses";
const SQL_SALES_EDITOR_WAREHOUSES: &str =
    "SELECT id,name,status FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name";
const READ_SALES_EDITOR_WAREHOUSES: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_SALES_EDITOR_WAREHOUSES,
    sql: SQL_SALES_EDITOR_WAREHOUSES,
    bind_count: 3,
};

/// Native successor of the TypeScript `posStartupMasterChoices` warehouse
/// options read. The caller binds `[company_id, branch_id, branch_id]`.
pub const RD_POS_STARTUP_WAREHOUSES: &str = "commerce_rd_pos_startup_warehouses";
const SQL_POS_STARTUP_WAREHOUSES: &str =
    "SELECT id,branch_id,code,name,status,version FROM commerce_warehouses WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY id";
const READ_POS_STARTUP_WAREHOUSES: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_POS_STARTUP_WAREHOUSES,
    sql: SQL_POS_STARTUP_WAREHOUSES,
    bind_count: 3,
};

/// Native successor of the TypeScript `posStartupMasterChoices` product
/// options read. The caller binds `[company_id]`.
pub const RD_POS_STARTUP_PRODUCTS: &str = "commerce_rd_pos_startup_products";
const SQL_POS_STARTUP_PRODUCTS: &str =
    "SELECT id,sku,name,item_type,tracking_mode,unit,price_minor,tax_bps,status,version FROM commerce_products WHERE company_id=? ORDER BY id";
const READ_POS_STARTUP_PRODUCTS: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_POS_STARTUP_PRODUCTS,
    sql: SQL_POS_STARTUP_PRODUCTS,
    bind_count: 1,
};

/// Native successor of the TypeScript `COMMERCE_SETTINGS_BY_COMPANY_SQL`
/// settings read. The caller binds `[company_id]`.
pub const RD_SETTINGS_BY_COMPANY: &str = "commerce_rd_settings_by_company";
const SQL_SETTINGS_BY_COMPANY: &str = "SELECT * FROM commerce_settings WHERE company_id=?";
const READ_SETTINGS_BY_COMPANY: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_SETTINGS_BY_COMPANY,
    sql: SQL_SETTINGS_BY_COMPANY,
    bind_count: 1,
};

/// Native successor of the TypeScript `COMMERCE_CURRENCY_BY_COMPANY_SQL`
/// currency read. The caller binds `[company_id]`.
pub const RD_CURRENCY_BY_COMPANY: &str = "commerce_rd_currency_by_company";
const SQL_CURRENCY_BY_COMPANY: &str = "SELECT currency FROM commerce_settings WHERE company_id=?";
const READ_CURRENCY_BY_COMPANY: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_CURRENCY_BY_COMPANY,
    sql: SQL_CURRENCY_BY_COMPANY,
    bind_count: 1,
};

/// Native successor of the TypeScript `COMMERCE_POSTING_MAPPING_STATE_SQL`
/// 11-column Commerce→posting mapping state read. The caller binds `[company_id]`.
pub const RD_POSTING_MAPPING_STATE: &str = "commerce_rd_posting_mapping_state";
const SQL_POSTING_MAPPING_STATE: &str =
    "SELECT chart_id \"chartId\",currency,receivable_account_id \"receivableAccountId\",payable_account_id \"payableAccountId\",revenue_account_id \"revenueAccountId\",inventory_account_id \"inventoryAccountId\",clearing_account_id \"clearingAccountId\",cogs_account_id \"cogsAccountId\",input_tax_account_id \"inputTaxAccountId\",output_tax_account_id \"outputTaxAccountId\",cash_account_id \"cashAccountId\" FROM commerce_settings WHERE company_id=?";
const READ_POSTING_MAPPING_STATE: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_POSTING_MAPPING_STATE,
    sql: SQL_POSTING_MAPPING_STATE,
    bind_count: 1,
};

/// Native successor of the TypeScript product page-row read. The caller binds
/// `[company_id]`.
pub const RD_PRODUCT_PAGE_ROWS: &str = "commerce_rd_product_page_rows";
const SQL_PRODUCT_PAGE_ROWS: &str =
    "SELECT *,sku AS page_key FROM commerce_products WHERE company_id=?";
const READ_PRODUCT_PAGE_ROWS: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_PRODUCT_PAGE_ROWS,
    sql: SQL_PRODUCT_PAGE_ROWS,
    bind_count: 1,
};

/// Native successor of the TypeScript product master-row read. The caller
/// binds `[company_id]`.
pub const RD_PRODUCT_MASTER_ROWS: &str = "commerce_rd_product_master_rows";
const SQL_PRODUCT_MASTER_ROWS: &str =
    "SELECT * FROM commerce_products WHERE company_id=? ORDER BY sku";
const READ_PRODUCT_MASTER_ROWS: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_PRODUCT_MASTER_ROWS,
    sql: SQL_PRODUCT_MASTER_ROWS,
    bind_count: 1,
};

/// Native successor of the TypeScript active-product read. The caller binds
/// `[record_id, company_id]`.
pub const RD_PRODUCT_ACTIVE: &str = "commerce_rd_product_active";
const SQL_PRODUCT_ACTIVE: &str =
    "SELECT * FROM commerce_products WHERE id=? AND company_id=? AND status='active'";
const READ_PRODUCT_ACTIVE: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_PRODUCT_ACTIVE,
    sql: SQL_PRODUCT_ACTIVE,
    bind_count: 2,
};

/// Native successor of the TypeScript active-product options read. The caller
/// binds `[company_id]`.
pub const RD_PRODUCT_ACTIVE_OPTIONS: &str = "commerce_rd_product_active_options";
const SQL_PRODUCT_ACTIVE_OPTIONS: &str =
    "SELECT id,name FROM commerce_products WHERE company_id=? AND status='active' ORDER BY name";
const READ_PRODUCT_ACTIVE_OPTIONS: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_PRODUCT_ACTIVE_OPTIONS,
    sql: SQL_PRODUCT_ACTIVE_OPTIONS,
    bind_count: 1,
};

/// Native successor of the TypeScript product master-detail read. The caller
/// binds `[record_id, company_id]`.
pub const RD_PRODUCT_MASTER_DETAIL: &str = "commerce_rd_product_master_detail";
const SQL_PRODUCT_MASTER_DETAIL: &str =
    "SELECT * FROM commerce_products WHERE id=? AND company_id=?";
const READ_PRODUCT_MASTER_DETAIL: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_PRODUCT_MASTER_DETAIL,
    sql: SQL_PRODUCT_MASTER_DETAIL,
    bind_count: 2,
};

/// Native successor of the TypeScript product point read. The caller binds
/// `[record_id, company_id]`.
pub const RD_PRODUCT_POINT: &str = "commerce_rd_product_point";
const SQL_PRODUCT_POINT: &str =
    "SELECT id FROM commerce_products WHERE id=? AND company_id=? AND status='active'";
const READ_PRODUCT_POINT: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_PRODUCT_POINT,
    sql: SQL_PRODUCT_POINT,
    bind_count: 2,
};

/// Native successor of the r28 stock-products selector shared by the Inventory
/// initial-count master choices. The caller binds `[company_id]`.
pub const RD_COUNT_ACTIVE_STOCK_PRODUCTS: &str = "commerce_rd_count_active_stock_products";
const SQL_COUNT_ACTIVE_STOCK_PRODUCTS: &str =
    "SELECT id,sku,name,unit,tracking_mode FROM commerce_products WHERE company_id=? AND status='active' AND item_type='stock' ORDER BY sku,id";
const READ_COUNT_ACTIVE_STOCK_PRODUCTS: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_COUNT_ACTIVE_STOCK_PRODUCTS,
    sql: SQL_COUNT_ACTIVE_STOCK_PRODUCTS,
    bind_count: 1,
};

/// Native successor of the r28 active-products selector used by the Inventory
/// count-source `activeProducts` reader. The caller binds `[company_id]`.
pub const RD_COUNT_ACTIVE_PRODUCTS: &str = "commerce_rd_count_active_products";
const SQL_COUNT_ACTIVE_PRODUCTS: &str =
    "SELECT id,sku,name,unit,tracking_mode FROM commerce_products WHERE company_id=? AND status='active' ORDER BY sku";
const READ_COUNT_ACTIVE_PRODUCTS: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_COUNT_ACTIVE_PRODUCTS,
    sql: SQL_COUNT_ACTIVE_PRODUCTS,
    bind_count: 1,
};

/// Native successor of the TypeScript `commercePages.warehouses` page-row read.
/// The caller binds `[company_id, branch_id, branch_id]`; the two branch
/// positions stay distinct and carry the caller's values verbatim.
pub const RD_WAREHOUSE_PAGE_ROWS: &str = "commerce_rd_warehouse_page_rows";
const SQL_WAREHOUSE_PAGE_ROWS: &str =
    "SELECT *,name AS page_key FROM commerce_warehouses WHERE company_id=? AND (?::text IS NULL OR branch_id=?)";
const READ_WAREHOUSE_PAGE_ROWS: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_WAREHOUSE_PAGE_ROWS,
    sql: SQL_WAREHOUSE_PAGE_ROWS,
    bind_count: 3,
};

/// Native successor of the TypeScript `unit.readCommerceWarehouses1` read. The
/// caller binds `[company_id, branch_id, branch_id]` with both branch positions
/// preserved.
pub const RD_WAREHOUSE_MASTER_ROWS: &str = "commerce_rd_warehouse_master_rows";
const SQL_WAREHOUSE_MASTER_ROWS: &str =
    "SELECT * FROM commerce_warehouses WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name";
const READ_WAREHOUSE_MASTER_ROWS: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_WAREHOUSE_MASTER_ROWS,
    sql: SQL_WAREHOUSE_MASTER_ROWS,
    bind_count: 3,
};

/// Native successor of the TypeScript `COMMERCE_WAREHOUSE_ACTIVE_SQL` read. The
/// caller binds `[record_id, company_id, branch_id, branch_id]`.
pub const RD_WAREHOUSE_ACTIVE: &str = "commerce_rd_warehouse_active";
const SQL_WAREHOUSE_ACTIVE: &str =
    "SELECT * FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)";
const READ_WAREHOUSE_ACTIVE: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_WAREHOUSE_ACTIVE,
    sql: SQL_WAREHOUSE_ACTIVE,
    bind_count: 4,
};

/// Native successor of the TypeScript `COMMERCE_ACTIVE_WAREHOUSES_SQL` read.
/// The caller binds `[company_id, branch_id, branch_id]`.
pub const RD_WAREHOUSES_ACTIVE: &str = "commerce_rd_warehouses_active";
const SQL_WAREHOUSES_ACTIVE: &str =
    "SELECT id,name,branch_id FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name";
const READ_WAREHOUSES_ACTIVE: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_WAREHOUSES_ACTIVE,
    sql: SQL_WAREHOUSES_ACTIVE,
    bind_count: 3,
};

/// Native successor of the TypeScript `COMMERCE_WAREHOUSE_BRANCH_ACTIVE_SQL`
/// read. The caller binds `[company_id, branch_id]`.
pub const RD_WAREHOUSE_BRANCH_ACTIVE: &str = "commerce_rd_warehouse_branch_active";
const SQL_WAREHOUSE_BRANCH_ACTIVE: &str =
    "SELECT id,name FROM commerce_warehouses WHERE company_id=? AND branch_id=? AND status='active'";
const READ_WAREHOUSE_BRANCH_ACTIVE: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_WAREHOUSE_BRANCH_ACTIVE,
    sql: SQL_WAREHOUSE_BRANCH_ACTIVE,
    bind_count: 2,
};

/// Native successor of the r27 expense posting-setup settings-image read. The
/// caller binds `[company_id]`; the `s` alias is part of the retired literal.
pub const RD_EXPENSE_SETTINGS_IMAGE: &str = "commerce_rd_expense_settings_image";
const SQL_EXPENSE_SETTINGS_IMAGE: &str =
    "SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE s.company_id=?";
const READ_EXPENSE_SETTINGS_IMAGE: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_EXPENSE_SETTINGS_IMAGE,
    sql: SQL_EXPENSE_SETTINGS_IMAGE,
    bind_count: 1,
};

/// Native successor of the r27 existing Manufacturing posting-setup settings
/// image read. The caller binds `[company_id]`; the unaliased predicate stays
/// byte-for-byte.
pub const RD_MFG_EXISTING_SETTINGS_IMAGE: &str = "commerce_rd_mfg_existing_settings_image";
const SQL_MFG_EXISTING_SETTINGS_IMAGE: &str =
    "SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?";
const READ_MFG_EXISTING_SETTINGS_IMAGE: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_MFG_EXISTING_SETTINGS_IMAGE,
    sql: SQL_MFG_EXISTING_SETTINGS_IMAGE,
    bind_count: 1,
};

/// Native successor of the r26 Support attachment source-image read. The
/// caller binds `[id, company_id]`; the two predicates stay in wire order.
pub const RD_SUPPORT_ORDER_SOURCE_IMAGE: &str = "commerce_rd_support_order_source_image";
const SQL_SUPPORT_ORDER_SOURCE_IMAGE: &str =
    "SELECT to_jsonb(source)::text image FROM commerce_orders source WHERE source.id=? AND source.company_id=?";
const READ_SUPPORT_ORDER_SOURCE_IMAGE: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_SUPPORT_ORDER_SOURCE_IMAGE,
    sql: SQL_SUPPORT_ORDER_SOURCE_IMAGE,
    bind_count: 2,
};

/// Native successor of the r26 CRM attachment source-image read. The caller
/// binds `[id, company_id]`; the quote/sale kind predicate stays byte-exact.
pub const RD_CRM_ORDER_SOURCE_IMAGE: &str = "commerce_rd_crm_order_source_image";
const SQL_CRM_ORDER_SOURCE_IMAGE: &str =
    "SELECT to_jsonb(source)::text image FROM commerce_orders source WHERE source.id=? AND source.company_id=? AND source.kind IN ('quote','sales')";
const READ_CRM_ORDER_SOURCE_IMAGE: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_CRM_ORDER_SOURCE_IMAGE,
    sql: SQL_CRM_ORDER_SOURCE_IMAGE,
    bind_count: 2,
};

/// Native authority for the r25 Sales settlement-line source fragment. This is
/// the exact text the Sales package witness embeds in its composite statement;
/// it carries no standalone bind or company scope of its own.
pub const RD_SALES_SETTLEMENT_LINES: &str = "commerce_rd_sales_settlement_lines";
const SQL_SALES_SETTLEMENT_LINES: &str =
    "SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE parent.kind IN ('quote','sales')";
const READ_SALES_SETTLEMENT_LINES: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_SALES_SETTLEMENT_LINES,
    sql: SQL_SALES_SETTLEMENT_LINES,
    bind_count: 0,
};

/// Native authority for the r25 Supply settlement-line source fragment. The
/// unaliased purchase predicate is kept byte-for-byte; no standalone binds.
pub const RD_SUPPLY_SETTLEMENT_LINES: &str = "commerce_rd_supply_settlement_lines";
const SQL_SUPPLY_SETTLEMENT_LINES: &str =
    "SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE parent.kind='purchase'";
const READ_SUPPLY_SETTLEMENT_LINES: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_SUPPLY_SETTLEMENT_LINES,
    sql: SQL_SUPPLY_SETTLEMENT_LINES,
    bind_count: 0,
};

/// r25 composite-query source fragments are owner-authorised SQL text, not
/// standalone scoped reads: cross-owner package witnesses embed them in larger
/// statements. They live outside `STATEMENTS` until those consumers cut over.
pub const SOURCE_FRAGMENTS: &[NativeCommerceReadStatement] = &[
    READ_SALES_SETTLEMENT_LINES,
    READ_SUPPLY_SETTLEMENT_LINES,
];

/// Resolve an owner source fragment by kind. Unregistered kinds fail closed.
pub fn source_fragment(kind: &str) -> Option<&'static NativeCommerceReadStatement> {
    SOURCE_FRAGMENTS.iter().find(|statement| statement.kind == kind)
}

/// Native successor of the r15 Billing test-finance schema-ready probe. The
/// owner table literal is fixed; the caller sends no binds.
pub const RD_BILLING_TEST_FINANCE_SCHEMA: &str = "commerce_rd_billing_test_finance_schema";
const SQL_BILLING_TEST_FINANCE_SCHEMA: &str =
    "SELECT to_regclass('billing_test_finance_documents') IS NOT NULL ready";
const READ_BILLING_TEST_FINANCE_SCHEMA: NativeCommerceReadStatement =
    NativeCommerceReadStatement {
        kind: RD_BILLING_TEST_FINANCE_SCHEMA,
        sql: SQL_BILLING_TEST_FINANCE_SCHEMA,
        bind_count: 0,
    };

/// Native successor of the r15 existing Manufacturing posting schema-ready
/// probe. The owner table literal is fixed; the caller sends no binds.
pub const RD_MFG_EXISTING_POSTING_SCHEMA: &str = "commerce_rd_mfg_existing_posting_schema";
const SQL_MFG_EXISTING_POSTING_SCHEMA: &str =
    "SELECT to_regclass('commerce_mfg_existing_posting_confirmations') IS NOT NULL ready";
const READ_MFG_EXISTING_POSTING_SCHEMA: NativeCommerceReadStatement =
    NativeCommerceReadStatement {
        kind: RD_MFG_EXISTING_POSTING_SCHEMA,
        sql: SQL_MFG_EXISTING_POSTING_SCHEMA,
        bind_count: 0,
    };

/// Native successor of the legacy attachment owner scope read. The only
/// closed descriptor is `commerce_order_delivery` on `commerce_orders`; the
/// caller binds `[id]` and an unknown kind fails closed before planning.
pub const RD_LEGACY_ATTACHMENT_SCOPE: &str = "commerce_rd_legacy_attachment_scope";
const SQL_LEGACY_ATTACHMENT_SCOPE: &str =
    "SELECT company_id,branch_id branch_id,department_id department_id,version FROM commerce_orders WHERE id=?";
const READ_LEGACY_ATTACHMENT_SCOPE: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_LEGACY_ATTACHMENT_SCOPE,
    sql: SQL_LEGACY_ATTACHMENT_SCOPE,
    bind_count: 1,
};

/// Native successor of the expense-posting prerequisite settings-source read.
/// The caller binds `[company_id]`; the `s` alias stays in the retired literal.
pub const RD_EXPENSE_PREREQUISITE_SETTINGS_SOURCE: &str =
    "commerce_rd_expense_prerequisite_settings_source";
const SQL_EXPENSE_PREREQUISITE_SETTINGS_SOURCE: &str =
    "SELECT to_jsonb(s)::text source FROM commerce_settings s WHERE s.company_id=?";
const READ_EXPENSE_PREREQUISITE_SETTINGS_SOURCE: NativeCommerceReadStatement =
    NativeCommerceReadStatement {
        kind: RD_EXPENSE_PREREQUISITE_SETTINGS_SOURCE,
        sql: SQL_EXPENSE_PREREQUISITE_SETTINGS_SOURCE,
        bind_count: 1,
    };

/// Native successor of the supplier-bill attachment receipt-id read. The
/// caller binds `[company_id, invoice_id]`; the Purchase predicate and id order
/// stay in the retired statement.
pub const RD_SUPPLIER_BILL_RECEIPT_IDS: &str = "commerce_rd_supplier_bill_receipt_ids";
const SQL_SUPPLIER_BILL_RECEIPT_IDS: &str =
    "SELECT f.id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id AND o.company_id=? AND o.kind='purchase' WHERE f.invoice_id=? ORDER BY f.id";
const READ_SUPPLIER_BILL_RECEIPT_IDS: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_SUPPLIER_BILL_RECEIPT_IDS,
    sql: SQL_SUPPLIER_BILL_RECEIPT_IDS,
    bind_count: 2,
};

/// Native successor of the POS private-image product version probe. The caller
/// binds `[product_id, company_id]` in that order.
pub const RD_POS_PRODUCT_VERSION: &str = "commerce_rd_pos_product_version";
const SQL_POS_PRODUCT_VERSION: &str =
    "SELECT version FROM commerce_products WHERE id=? AND company_id=?";
const READ_POS_PRODUCT_VERSION: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_POS_PRODUCT_VERSION,
    sql: SQL_POS_PRODUCT_VERSION,
    bind_count: 2,
};

/// Native successor of the POS private-image product projection. The caller
/// binds `[product_id, company_id]`; the `product` alias stays in the literal.
pub const RD_POS_PRODUCT_IMAGE: &str = "commerce_rd_pos_product_image";
const SQL_POS_PRODUCT_IMAGE: &str =
    "SELECT to_jsonb(product)::text image FROM commerce_products product WHERE product.id=? AND product.company_id=?";
const READ_POS_PRODUCT_IMAGE: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_POS_PRODUCT_IMAGE,
    sql: SQL_POS_PRODUCT_IMAGE,
    bind_count: 2,
};

/// Native successor of the landed-cost receipt-line read. The caller binds
/// `[fulfillment_id]`; the three-table join and ordinal order stay verbatim.
pub const RD_LANDED_COST_RECEIPT_LINES: &str = "commerce_rd_landed_cost_receipt_lines";
const SQL_LANDED_COST_RECEIPT_LINES: &str =
    "SELECT fl.*,ol.product_id,p.tracking_mode FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id JOIN commerce_products p ON p.id=ol.product_id WHERE fl.fulfillment_id=? ORDER BY ol.ordinal";
const READ_LANDED_COST_RECEIPT_LINES: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_LANDED_COST_RECEIPT_LINES,
    sql: SQL_LANDED_COST_RECEIPT_LINES,
    bind_count: 1,
};

/// Native successor of the landed-cost posting-config read. The caller binds
/// `[company_id]`.
pub const RD_LANDED_COST_POSTING_CONFIG: &str = "commerce_rd_landed_cost_posting_config";
const SQL_LANDED_COST_POSTING_CONFIG: &str =
    "SELECT chart_id,inventory_account_id FROM commerce_settings WHERE company_id=?";
const READ_LANDED_COST_POSTING_CONFIG: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_LANDED_COST_POSTING_CONFIG,
    sql: SQL_LANDED_COST_POSTING_CONFIG,
    bind_count: 1,
};

/// Native authority for the transit active-location guard predicate. The
/// caller binds `[warehouse_id, company_id, branch_id]`; the workflow assertion
/// runs this exact text on its own command unit.
pub const RD_TRANSIT_ACTIVE_LOCATION: &str = "commerce_rd_transit_active_location";
const SQL_TRANSIT_ACTIVE_LOCATION: &str =
    "EXISTS(SELECT 1 FROM commerce_warehouses WHERE id=? AND company_id=? AND branch_id=? AND status='active')";
const READ_TRANSIT_ACTIVE_LOCATION: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_TRANSIT_ACTIVE_LOCATION,
    sql: SQL_TRANSIT_ACTIVE_LOCATION,
    bind_count: 3,
};

/// Native authority for the transit product-version guard predicate. The
/// caller binds `[product_id, company_id, version]`.
pub const RD_TRANSIT_PRODUCT_VERSION: &str = "commerce_rd_transit_product_version";
const SQL_TRANSIT_PRODUCT_VERSION: &str =
    "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND status='active' AND version=?)";
const READ_TRANSIT_PRODUCT_VERSION: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_TRANSIT_PRODUCT_VERSION,
    sql: SQL_TRANSIT_PRODUCT_VERSION,
    bind_count: 3,
};

/// Native successor of the POS full active-warehouses read. The caller binds
/// `[company_id, branch_id, branch_id]`; both branch positions stay distinct.
pub const RD_POS_WAREHOUSES_ALL: &str = "commerce_rd_pos_warehouses_all";
const SQL_POS_WAREHOUSES_ALL: &str =
    "SELECT * FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name";
const READ_POS_WAREHOUSES_ALL: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_POS_WAREHOUSES_ALL,
    sql: SQL_POS_WAREHOUSES_ALL,
    bind_count: 3,
};

/// Native successor of the existing Manufacturing posting confirmation-image
/// read. The caller binds `[company_id]`; the owner table and `s` alias stay
/// byte-exact.
pub const RD_MFG_EXISTING_POSTING_CONFIRMATION_IMAGE: &str =
    "commerce_rd_mfg_existing_posting_confirmation_image";
const SQL_MFG_EXISTING_POSTING_CONFIRMATION_IMAGE: &str =
    "SELECT to_jsonb(s)::text image FROM commerce_mfg_existing_posting_confirmations s WHERE company_id=?";
const READ_MFG_EXISTING_POSTING_CONFIRMATION_IMAGE: NativeCommerceReadStatement =
    NativeCommerceReadStatement {
        kind: RD_MFG_EXISTING_POSTING_CONFIRMATION_IMAGE,
        sql: SQL_MFG_EXISTING_POSTING_CONFIRMATION_IMAGE,
        bind_count: 1,
    };

/// Native successor of the sales delivery-notification line read. The caller
/// binds `[fulfillment_id]`; the three-table owner-local join and ordinal order
/// stay byte-exact.
pub const RD_SALES_NOTIFICATION_LINES: &str = "commerce_rd_sales_notification_lines";
const SQL_SALES_NOTIFICATION_LINES: &str =
    "SELECT p.sku,fl.quantity_milli,p.unit FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id JOIN commerce_products p ON p.id=ol.product_id WHERE fl.fulfillment_id=? ORDER BY ol.ordinal";
const READ_SALES_NOTIFICATION_LINES: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_SALES_NOTIFICATION_LINES,
    sql: SQL_SALES_NOTIFICATION_LINES,
    bind_count: 1,
};

/// Native successor of the purchase-control order-target source read. The
/// caller binds `[order_id]`.
pub const RD_PURCHASE_CONTROL_ORDER: &str = "commerce_rd_purchase_control_order";
const SQL_PURCHASE_CONTROL_ORDER: &str =
    "SELECT DISTINCT o.id,o.kind,o.company_id,o.branch_id,o.department_id FROM commerce_orders o WHERE o.id=?";
const READ_PURCHASE_CONTROL_ORDER: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_PURCHASE_CONTROL_ORDER,
    sql: SQL_PURCHASE_CONTROL_ORDER,
    bind_count: 1,
};

/// Native successor of the purchase-control invoice-target source read. The
/// caller binds `[invoice_id]`; the fulfillment join stays in the owner plan.
pub const RD_PURCHASE_CONTROL_INVOICE: &str = "commerce_rd_purchase_control_invoice";
const SQL_PURCHASE_CONTROL_INVOICE: &str =
    "SELECT DISTINCT o.id,o.kind,o.company_id,o.branch_id,o.department_id FROM commerce_orders o JOIN commerce_fulfillments f ON f.order_id=o.id WHERE f.invoice_id=?";
const READ_PURCHASE_CONTROL_INVOICE: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_PURCHASE_CONTROL_INVOICE,
    sql: SQL_PURCHASE_CONTROL_INVOICE,
    bind_count: 1,
};

/// Native successor of the return-line stock witness on an order page. The
/// caller binds `[order_id, company_id, branch_id, branch_id, department_id,
/// department_id]` in exactly that order.
pub const RD_PURCHASE_CONTROL_RETURN_LINES: &str = "commerce_rd_purchase_control_return_lines";
const SQL_PURCHASE_CONTROL_RETURN_LINES: &str =
    "SELECT r.id,EXISTS(SELECT 1 FROM commerce_return_lines rl JOIN commerce_fulfillment_lines fl ON fl.id=rl.fulfillment_line_id JOIN commerce_order_lines ol ON ol.id=fl.order_line_id WHERE rl.return_id=r.id AND ol.item_type!='service') has_stock FROM commerce_returns r JOIN commerce_orders o ON o.id=r.order_id WHERE o.id=? AND o.company_id=? AND (? IS NULL OR o.branch_id=?) AND (? IS NULL OR o.department_id=?)";
const READ_PURCHASE_CONTROL_RETURN_LINES: NativeCommerceReadStatement =
    NativeCommerceReadStatement {
        kind: RD_PURCHASE_CONTROL_RETURN_LINES,
        sql: SQL_PURCHASE_CONTROL_RETURN_LINES,
        bind_count: 6,
    };

/// Native successor of the initial-count active-warehouse choice read. The
/// caller binds `[company_id, branch_id, branch_id]` and keeps the `name,id`
/// order.
pub const RD_INITIAL_COUNT_WAREHOUSES: &str = "commerce_rd_initial_count_warehouses";
const SQL_INITIAL_COUNT_WAREHOUSES: &str =
    "SELECT id,name,branch_id FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name,id";
const READ_INITIAL_COUNT_WAREHOUSES: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_INITIAL_COUNT_WAREHOUSES,
    sql: SQL_INITIAL_COUNT_WAREHOUSES,
    bind_count: 3,
};

/// Native successor of the initial-count stock-product detail read. The caller
/// binds `[product_id, company_id]`.
pub const RD_INITIAL_COUNT_STOCK_PRODUCT: &str = "commerce_rd_initial_count_stock_product";
const SQL_INITIAL_COUNT_STOCK_PRODUCT: &str =
    "SELECT * FROM commerce_products WHERE id=? AND company_id=? AND status='active' AND item_type='stock'";
const READ_INITIAL_COUNT_STOCK_PRODUCT: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_INITIAL_COUNT_STOCK_PRODUCT,
    sql: SQL_INITIAL_COUNT_STOCK_PRODUCT,
    bind_count: 2,
};

/// Native successor of the PMS active-product options read. The caller binds
/// `[company_id]`.
pub const RD_PMS_PRODUCT_OPTIONS: &str = "commerce_rd_pms_product_options";
const SQL_PMS_PRODUCT_OPTIONS: &str =
    "SELECT id,name FROM commerce_products WHERE company_id=? AND status='active'";
const READ_PMS_PRODUCT_OPTIONS: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_PMS_PRODUCT_OPTIONS,
    sql: SQL_PMS_PRODUCT_OPTIONS,
    bind_count: 1,
};

/// Native successor of the PMS branch-scoped warehouse id probe. The caller
/// binds `[warehouse_id, company_id, branch_id]`.
pub const RD_PMS_WAREHOUSE_ID: &str = "commerce_rd_pms_warehouse_id";
const SQL_PMS_WAREHOUSE_ID: &str =
    "SELECT id FROM commerce_warehouses WHERE id=? AND company_id=? AND branch_id=? AND status='active'";
const READ_PMS_WAREHOUSE_ID: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_PMS_WAREHOUSE_ID,
    sql: SQL_PMS_WAREHOUSE_ID,
    bind_count: 3,
};

/// Native successor of the PMS product/warehouse scope witness. The caller
/// binds `[warehouse_id, product_id, company_id, branch_id]`.
pub const RD_PMS_PRODUCT_WAREHOUSE_SCOPE: &str = "commerce_rd_pms_product_warehouse_scope";
const SQL_PMS_PRODUCT_WAREHOUSE_SCOPE: &str =
    "SELECT 1 FROM commerce_products p JOIN commerce_warehouses w ON w.id=? WHERE p.id=? AND p.company_id=? AND w.company_id=p.company_id AND w.branch_id=? AND p.status='active' AND w.status='active'";
const READ_PMS_PRODUCT_WAREHOUSE_SCOPE: NativeCommerceReadStatement =
    NativeCommerceReadStatement {
        kind: RD_PMS_PRODUCT_WAREHOUSE_SCOPE,
        sql: SQL_PMS_PRODUCT_WAREHOUSE_SCOPE,
        bind_count: 4,
    };

/// Native successor of the attachment product source image. The caller binds
/// `[product_id, company_id]`; the `p` alias stays in the retired literal.
pub const RD_ATTACHMENT_PRODUCT_IMAGE: &str = "commerce_rd_attachment_product_image";
const SQL_ATTACHMENT_PRODUCT_IMAGE: &str =
    "SELECT to_jsonb(p)::text image FROM commerce_products p WHERE p.id=? AND p.company_id=?";
const READ_ATTACHMENT_PRODUCT_IMAGE: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_ATTACHMENT_PRODUCT_IMAGE,
    sql: SQL_ATTACHMENT_PRODUCT_IMAGE,
    bind_count: 2,
};

const STATEMENTS: &[NativeCommerceReadStatement] = &[
    READ_PMS_WAREHOUSE_ACTIVE,
    READ_PMS_SETTINGS_EXISTS,
    READ_SALES_EDITOR_PRODUCTS,
    READ_SALES_EDITOR_WAREHOUSES,
    READ_POS_STARTUP_WAREHOUSES,
    READ_POS_STARTUP_PRODUCTS,
    READ_SETTINGS_BY_COMPANY,
    READ_CURRENCY_BY_COMPANY,
    READ_POSTING_MAPPING_STATE,
    READ_PRODUCT_PAGE_ROWS,
    READ_PRODUCT_MASTER_ROWS,
    READ_PRODUCT_ACTIVE,
    READ_PRODUCT_ACTIVE_OPTIONS,
    READ_PRODUCT_MASTER_DETAIL,
    READ_PRODUCT_POINT,
    READ_COUNT_ACTIVE_STOCK_PRODUCTS,
    READ_COUNT_ACTIVE_PRODUCTS,
    READ_WAREHOUSE_PAGE_ROWS,
    READ_WAREHOUSE_MASTER_ROWS,
    READ_WAREHOUSE_ACTIVE,
    READ_WAREHOUSES_ACTIVE,
    READ_WAREHOUSE_BRANCH_ACTIVE,
    READ_EXPENSE_SETTINGS_IMAGE,
    READ_MFG_EXISTING_SETTINGS_IMAGE,
    READ_SUPPORT_ORDER_SOURCE_IMAGE,
    READ_CRM_ORDER_SOURCE_IMAGE,
    READ_BILLING_TEST_FINANCE_SCHEMA,
    READ_MFG_EXISTING_POSTING_SCHEMA,
    READ_LEGACY_ATTACHMENT_SCOPE,
    READ_EXPENSE_PREREQUISITE_SETTINGS_SOURCE,
    READ_SUPPLIER_BILL_RECEIPT_IDS,
    READ_POS_PRODUCT_VERSION,
    READ_POS_PRODUCT_IMAGE,
    READ_LANDED_COST_RECEIPT_LINES,
    READ_LANDED_COST_POSTING_CONFIG,
    READ_TRANSIT_ACTIVE_LOCATION,
    READ_TRANSIT_PRODUCT_VERSION,
    READ_POS_WAREHOUSES_ALL,
    READ_MFG_EXISTING_POSTING_CONFIRMATION_IMAGE,
    READ_SALES_NOTIFICATION_LINES,
    READ_PURCHASE_CONTROL_ORDER,
    READ_PURCHASE_CONTROL_INVOICE,
    READ_PURCHASE_CONTROL_RETURN_LINES,
    READ_INITIAL_COUNT_WAREHOUSES,
    READ_INITIAL_COUNT_STOCK_PRODUCT,
    READ_PMS_PRODUCT_OPTIONS,
    READ_PMS_WAREHOUSE_ID,
    READ_PMS_PRODUCT_WAREHOUSE_SCOPE,
    READ_ATTACHMENT_PRODUCT_IMAGE,
];

/// Resolve an allow-listed owner read kind. Any unregistered kind fails closed.
pub fn read_statement(kind: &str) -> Option<&'static NativeCommerceReadStatement> {
    STATEMENTS.iter().find(|statement| statement.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_reads_are_owner_authored_select_only_with_exact_binds() {
        let expected = [
            (RD_PMS_WAREHOUSE_ACTIVE, SQL_PMS_WAREHOUSE_ACTIVE, 1usize),
            (RD_PMS_SETTINGS_EXISTS, SQL_PMS_SETTINGS_EXISTS, 1usize),
            (RD_SALES_EDITOR_PRODUCTS, SQL_SALES_EDITOR_PRODUCTS, 1usize),
            (RD_SALES_EDITOR_WAREHOUSES, SQL_SALES_EDITOR_WAREHOUSES, 3usize),
            (RD_POS_STARTUP_WAREHOUSES, SQL_POS_STARTUP_WAREHOUSES, 3usize),
            (RD_POS_STARTUP_PRODUCTS, SQL_POS_STARTUP_PRODUCTS, 1usize),
            (RD_SETTINGS_BY_COMPANY, SQL_SETTINGS_BY_COMPANY, 1usize),
            (RD_CURRENCY_BY_COMPANY, SQL_CURRENCY_BY_COMPANY, 1usize),
            (RD_POSTING_MAPPING_STATE, SQL_POSTING_MAPPING_STATE, 1usize),
            (RD_PRODUCT_PAGE_ROWS, SQL_PRODUCT_PAGE_ROWS, 1usize),
            (RD_PRODUCT_MASTER_ROWS, SQL_PRODUCT_MASTER_ROWS, 1usize),
            (RD_PRODUCT_ACTIVE, SQL_PRODUCT_ACTIVE, 2usize),
            (RD_PRODUCT_ACTIVE_OPTIONS, SQL_PRODUCT_ACTIVE_OPTIONS, 1usize),
            (RD_PRODUCT_MASTER_DETAIL, SQL_PRODUCT_MASTER_DETAIL, 2usize),
            (RD_PRODUCT_POINT, SQL_PRODUCT_POINT, 2usize),
            (
                RD_COUNT_ACTIVE_STOCK_PRODUCTS,
                SQL_COUNT_ACTIVE_STOCK_PRODUCTS,
                1usize,
            ),
            (RD_COUNT_ACTIVE_PRODUCTS, SQL_COUNT_ACTIVE_PRODUCTS, 1usize),
            (RD_WAREHOUSE_PAGE_ROWS, SQL_WAREHOUSE_PAGE_ROWS, 3usize),
            (RD_WAREHOUSE_MASTER_ROWS, SQL_WAREHOUSE_MASTER_ROWS, 3usize),
            (RD_WAREHOUSE_ACTIVE, SQL_WAREHOUSE_ACTIVE, 4usize),
            (RD_WAREHOUSES_ACTIVE, SQL_WAREHOUSES_ACTIVE, 3usize),
            (RD_WAREHOUSE_BRANCH_ACTIVE, SQL_WAREHOUSE_BRANCH_ACTIVE, 2usize),
            (RD_EXPENSE_SETTINGS_IMAGE, SQL_EXPENSE_SETTINGS_IMAGE, 1usize),
            (
                RD_MFG_EXISTING_SETTINGS_IMAGE,
                SQL_MFG_EXISTING_SETTINGS_IMAGE,
                1usize,
            ),
            (
                RD_SUPPORT_ORDER_SOURCE_IMAGE,
                SQL_SUPPORT_ORDER_SOURCE_IMAGE,
                2usize,
            ),
            (RD_CRM_ORDER_SOURCE_IMAGE, SQL_CRM_ORDER_SOURCE_IMAGE, 2usize),
            (
                RD_BILLING_TEST_FINANCE_SCHEMA,
                SQL_BILLING_TEST_FINANCE_SCHEMA,
                0usize,
            ),
            (
                RD_MFG_EXISTING_POSTING_SCHEMA,
                SQL_MFG_EXISTING_POSTING_SCHEMA,
                0usize,
            ),
            (
                RD_LEGACY_ATTACHMENT_SCOPE,
                SQL_LEGACY_ATTACHMENT_SCOPE,
                1usize,
            ),
            (
                RD_EXPENSE_PREREQUISITE_SETTINGS_SOURCE,
                SQL_EXPENSE_PREREQUISITE_SETTINGS_SOURCE,
                1usize,
            ),
            (
                RD_SUPPLIER_BILL_RECEIPT_IDS,
                SQL_SUPPLIER_BILL_RECEIPT_IDS,
                2usize,
            ),
            (RD_POS_PRODUCT_VERSION, SQL_POS_PRODUCT_VERSION, 2usize),
            (RD_POS_PRODUCT_IMAGE, SQL_POS_PRODUCT_IMAGE, 2usize),
            (
                RD_LANDED_COST_RECEIPT_LINES,
                SQL_LANDED_COST_RECEIPT_LINES,
                1usize,
            ),
            (
                RD_LANDED_COST_POSTING_CONFIG,
                SQL_LANDED_COST_POSTING_CONFIG,
                1usize,
            ),
            (
                RD_TRANSIT_ACTIVE_LOCATION,
                SQL_TRANSIT_ACTIVE_LOCATION,
                3usize,
            ),
            (RD_TRANSIT_PRODUCT_VERSION, SQL_TRANSIT_PRODUCT_VERSION, 3usize),
            (RD_POS_WAREHOUSES_ALL, SQL_POS_WAREHOUSES_ALL, 3usize),
            (
                RD_MFG_EXISTING_POSTING_CONFIRMATION_IMAGE,
                SQL_MFG_EXISTING_POSTING_CONFIRMATION_IMAGE,
                1usize,
            ),
            (RD_SALES_NOTIFICATION_LINES, SQL_SALES_NOTIFICATION_LINES, 1usize),
            (RD_PURCHASE_CONTROL_ORDER, SQL_PURCHASE_CONTROL_ORDER, 1usize),
            (RD_PURCHASE_CONTROL_INVOICE, SQL_PURCHASE_CONTROL_INVOICE, 1usize),
            (
                RD_PURCHASE_CONTROL_RETURN_LINES,
                SQL_PURCHASE_CONTROL_RETURN_LINES,
                6usize,
            ),
            (
                RD_INITIAL_COUNT_WAREHOUSES,
                SQL_INITIAL_COUNT_WAREHOUSES,
                3usize,
            ),
            (
                RD_INITIAL_COUNT_STOCK_PRODUCT,
                SQL_INITIAL_COUNT_STOCK_PRODUCT,
                2usize,
            ),
            (RD_PMS_PRODUCT_OPTIONS, SQL_PMS_PRODUCT_OPTIONS, 1usize),
            (RD_PMS_WAREHOUSE_ID, SQL_PMS_WAREHOUSE_ID, 3usize),
            (
                RD_PMS_PRODUCT_WAREHOUSE_SCOPE,
                SQL_PMS_PRODUCT_WAREHOUSE_SCOPE,
                4usize,
            ),
            (
                RD_ATTACHMENT_PRODUCT_IMAGE,
                SQL_ATTACHMENT_PRODUCT_IMAGE,
                2usize,
            ),
        ];
        for (kind, sql, bind_count) in expected {
            let statement = read_statement(kind).expect("registered owner read kind");
            assert_eq!(statement.kind, kind);
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.bind_count, bind_count);
            assert!(
                sql.starts_with("SELECT ") || sql.starts_with("EXISTS("),
                "owner read must be SELECT-only or a SELECT guard: {sql}"
            );
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
                assert!(!sql.contains(forbidden), "owner read must not mutate: {forbidden}");
            }
            assert!(
                sql.contains("commerce_products")
                    || sql.contains("commerce_warehouses")
                    || sql.contains("commerce_settings")
                    || sql.contains("commerce_orders")
                    || sql.contains("commerce_mfg_existing_posting_confirmations")
                    || sql.contains("to_regclass("),
                "owner read must stay on owner-local Commerce tables or schema probes: {sql}"
            );
            assert_eq!(sql.matches('?').count(), bind_count);
        }
        assert_eq!(STATEMENTS.len(), 49);
    }

    /// The attachment product image keeps its exact alias, projection and
    /// `[product_id, company_id]` bind order.
    #[test]
    fn attachment_product_image_keeps_its_exact_literal() {
        let plan = read_statement(RD_ATTACHMENT_PRODUCT_IMAGE).expect("product image");
        assert_eq!(
            plan.sql,
            "SELECT to_jsonb(p)::text image FROM commerce_products p WHERE p.id=? AND p.company_id=?"
        );
        assert_eq!(plan.bind_count, 2);
        assert_eq!(plan.sql.matches('?').count(), 2);
        assert!(plan.sql.starts_with("SELECT "));
        assert!(!plan.sql.contains(';'));
    }

    /// PMS owner-local reads keep their exact aliases, predicates and bind
    /// positions; the FOR UPDATE stock read stays outside this registry.
    #[test]
    fn pms_owner_local_reads_keep_their_exact_literals() {
        let products = read_statement(RD_PMS_PRODUCT_OPTIONS).expect("product options");
        let warehouse = read_statement(RD_PMS_WAREHOUSE_ID).expect("warehouse id");
        let scope = read_statement(RD_PMS_PRODUCT_WAREHOUSE_SCOPE).expect("scope witness");
        assert_eq!(products.sql, "SELECT id,name FROM commerce_products WHERE company_id=? AND status='active'");
        assert_eq!(warehouse.sql, "SELECT id FROM commerce_warehouses WHERE id=? AND company_id=? AND branch_id=? AND status='active'");
        assert_eq!(scope.sql, "SELECT 1 FROM commerce_products p JOIN commerce_warehouses w ON w.id=? WHERE p.id=? AND p.company_id=? AND w.company_id=p.company_id AND w.branch_id=? AND p.status='active' AND w.status='active'");
        for (statement, binds) in [(products, 1), (warehouse, 3), (scope, 4)] {
            assert_eq!(statement.bind_count, binds);
            assert_eq!(statement.sql.matches('?').count(), binds);
            assert!(statement.sql.starts_with("SELECT "));
            assert!(!statement.sql.contains(';'));
            assert!(!statement.sql.contains("FOR UPDATE"));
        }
    }

    /// Initial-count master reads keep their exact ordering and owner-local
    /// predicates with the same bind positions.
    #[test]
    fn initial_count_master_reads_keep_their_exact_literals() {
        let warehouses = read_statement(RD_INITIAL_COUNT_WAREHOUSES).expect("warehouse choices");
        let product = read_statement(RD_INITIAL_COUNT_STOCK_PRODUCT).expect("stock product");
        assert_eq!(
            warehouses.sql,
            "SELECT id,name,branch_id FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name,id"
        );
        assert_eq!(
            product.sql,
            "SELECT * FROM commerce_products WHERE id=? AND company_id=? AND status='active' AND item_type='stock'"
        );
        assert_eq!(warehouses.bind_count, 3);
        assert_eq!(warehouses.sql.matches("?::text IS NULL OR branch_id=?").count(), 1);
        assert_eq!(product.bind_count, 2);
        for statement in [warehouses, product] {
            assert_eq!(statement.sql.matches('?').count(), statement.bind_count);
            assert!(statement.sql.starts_with("SELECT "));
            assert!(!statement.sql.contains(';'));
        }
    }

    /// Purchase-control reads keep their exact target predicates and the
    /// return-line witness with six ordered branch/department binds.
    #[test]
    fn purchase_control_reads_keep_their_exact_literals() {
        let order = read_statement(RD_PURCHASE_CONTROL_ORDER).expect("order target");
        let invoice = read_statement(RD_PURCHASE_CONTROL_INVOICE).expect("invoice target");
        let returns = read_statement(RD_PURCHASE_CONTROL_RETURN_LINES).expect("return lines");
        assert_eq!(
            order.sql,
            "SELECT DISTINCT o.id,o.kind,o.company_id,o.branch_id,o.department_id FROM commerce_orders o WHERE o.id=?"
        );
        assert_eq!(
            invoice.sql,
            "SELECT DISTINCT o.id,o.kind,o.company_id,o.branch_id,o.department_id FROM commerce_orders o JOIN commerce_fulfillments f ON f.order_id=o.id WHERE f.invoice_id=?"
        );
        assert!(returns.sql.starts_with("SELECT r.id,EXISTS(SELECT 1 FROM commerce_return_lines"));
        assert_eq!(returns.sql.matches("? IS NULL OR o.branch_id=?").count(), 1);
        assert_eq!(returns.sql.matches("? IS NULL OR o.department_id=?").count(), 1);
        assert_eq!(order.bind_count, 1);
        assert_eq!(invoice.bind_count, 1);
        assert_eq!(returns.bind_count, 6);
        assert_eq!(returns.sql.matches('?').count(), 6);
        for statement in [order, invoice, returns] {
            assert!(statement.sql.starts_with("SELECT "));
            assert!(!statement.sql.contains(';'));
            assert!(!statement.sql.contains("FOR UPDATE"));
        }
    }

    /// Sales notification lines keep the exact owner-local join, projection
    /// and single fulfillment bind.
    #[test]
    fn sales_notification_lines_keep_their_exact_literal() {
        let plan = read_statement(RD_SALES_NOTIFICATION_LINES).expect("notification lines");
        assert_eq!(
            plan.sql,
            "SELECT p.sku,fl.quantity_milli,p.unit FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id JOIN commerce_products p ON p.id=ol.product_id WHERE fl.fulfillment_id=? ORDER BY ol.ordinal"
        );
        assert_eq!(plan.bind_count, 1);
        assert_eq!(plan.sql.matches('?').count(), 1);
        assert!(plan.sql.starts_with("SELECT "));
        assert!(!plan.sql.contains(';'));
    }

    /// The existing-posting confirmation image keeps its byte-exact owner
    /// projection and single company bind.
    #[test]
    fn mfg_posting_confirmation_image_keeps_its_exact_literal() {
        let plan = read_statement(RD_MFG_EXISTING_POSTING_CONFIRMATION_IMAGE)
            .expect("mfg confirmation image");
        assert_eq!(
            plan.sql,
            "SELECT to_jsonb(s)::text image FROM commerce_mfg_existing_posting_confirmations s WHERE company_id=?"
        );
        assert_eq!(plan.bind_count, 1);
        assert_eq!(plan.sql.matches('?').count(), 1);
        assert!(plan.sql.starts_with("SELECT "));
        assert!(!plan.sql.contains(';'));
    }

    /// The POS full active-warehouses read keeps its two distinct branch bind
    /// positions and byte-exact `SELECT *` projection.
    #[test]
    fn pos_warehouses_all_keeps_its_exact_literal() {
        let plan = read_statement(RD_POS_WAREHOUSES_ALL).expect("pos warehouses");
        assert_eq!(
            plan.sql,
            "SELECT * FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name"
        );
        assert_eq!(plan.bind_count, 3);
        assert_eq!(plan.sql.matches('?').count(), 3);
        assert_eq!(plan.sql.matches("?::text IS NULL OR branch_id=?").count(), 1);
        assert!(!plan.sql.contains(';'));
    }

    /// Transit guard predicates keep their byte-exact EXISTS text and three
    /// bind positions for the workflow assertion.
    #[test]
    fn transit_guard_predicates_keep_their_exact_literals() {
        let location = read_statement(RD_TRANSIT_ACTIVE_LOCATION).expect("location guard");
        let version = read_statement(RD_TRANSIT_PRODUCT_VERSION).expect("version guard");
        assert_eq!(
            location.sql,
            "EXISTS(SELECT 1 FROM commerce_warehouses WHERE id=? AND company_id=? AND branch_id=? AND status='active')"
        );
        assert_eq!(
            version.sql,
            "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND status='active' AND version=?)"
        );
        for statement in [location, version] {
            assert_eq!(statement.bind_count, 3);
            assert_eq!(statement.sql.matches('?').count(), 3);
            assert!(statement.sql.starts_with("EXISTS(SELECT 1 FROM commerce_"));
            assert!(!statement.sql.contains(';'));
            assert!(!statement.sql.contains("FOR UPDATE"));
        }
    }

    /// Landed-cost reads keep their exact owner-local join, projection and
    /// single bind positions.
    #[test]
    fn landed_cost_reads_keep_their_exact_literals() {
        let lines = read_statement(RD_LANDED_COST_RECEIPT_LINES).expect("receipt lines");
        let config = read_statement(RD_LANDED_COST_POSTING_CONFIG).expect("posting config");
        assert_eq!(
            lines.sql,
            "SELECT fl.*,ol.product_id,p.tracking_mode FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id JOIN commerce_products p ON p.id=ol.product_id WHERE fl.fulfillment_id=? ORDER BY ol.ordinal"
        );
        assert_eq!(config.sql, "SELECT chart_id,inventory_account_id FROM commerce_settings WHERE company_id=?");
        for statement in [lines, config] {
            assert_eq!(statement.bind_count, 1);
            assert_eq!(statement.sql.matches('?').count(), 1);
            assert!(statement.sql.starts_with("SELECT "));
            assert!(!statement.sql.contains(';'));
        }
    }

    /// POS private-image product reads keep their exact alias, projection and
    /// `[product_id, company_id]` bind order.
    #[test]
    fn pos_image_product_reads_keep_their_exact_literals() {
        let version = read_statement(RD_POS_PRODUCT_VERSION).expect("product version");
        let image = read_statement(RD_POS_PRODUCT_IMAGE).expect("product image");
        assert_eq!(version.sql, "SELECT version FROM commerce_products WHERE id=? AND company_id=?");
        assert_eq!(
            image.sql,
            "SELECT to_jsonb(product)::text image FROM commerce_products product WHERE product.id=? AND product.company_id=?"
        );
        for statement in [version, image] {
            assert_eq!(statement.bind_count, 2);
            assert_eq!(statement.sql.matches('?').count(), 2);
            assert!(statement.sql.starts_with("SELECT "));
            assert!(!statement.sql.contains(';'));
        }
    }

    /// Supplier-bill receipt ids keep the exact Purchase join, two bind
    /// positions and stable id order of the retired TypeScript read.
    #[test]
    fn supplier_bill_receipt_ids_keep_their_exact_literal() {
        let plan = read_statement(RD_SUPPLIER_BILL_RECEIPT_IDS).expect("supplier bill ids");
        assert_eq!(
            plan.sql,
            "SELECT f.id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id AND o.company_id=? AND o.kind='purchase' WHERE f.invoice_id=? ORDER BY f.id"
        );
        assert_eq!(plan.bind_count, 2);
        assert_eq!(plan.sql.matches('?').count(), 2);
        assert!(plan.sql.starts_with("SELECT "));
        assert!(!plan.sql.contains(';'));
    }

    /// The expense-posting prerequisite settings source keeps its byte-exact
    /// aliased image projection and single company bind.
    #[test]
    fn expense_prerequisite_settings_source_keeps_its_exact_literal() {
        let plan = read_statement(RD_EXPENSE_PREREQUISITE_SETTINGS_SOURCE)
            .expect("expense prerequisite source");
        assert_eq!(
            plan.sql,
            "SELECT to_jsonb(s)::text source FROM commerce_settings s WHERE s.company_id=?"
        );
        assert_eq!(plan.bind_count, 1);
        assert_eq!(plan.sql.matches('?').count(), 1);
        assert!(plan.sql.starts_with("SELECT "));
        assert!(!plan.sql.contains(';'));
    }

    /// Legacy attachment scope byte parity: the single closed descriptor keeps
    /// both alias expressions and the one id bind exactly as retired.
    #[test]
    fn legacy_attachment_scope_keeps_its_exact_literal() {
        let plan = read_statement(RD_LEGACY_ATTACHMENT_SCOPE).expect("legacy scope");
        assert_eq!(
            plan.sql,
            "SELECT company_id,branch_id branch_id,department_id department_id,version FROM commerce_orders WHERE id=?"
        );
        assert_eq!(plan.bind_count, 1);
        assert_eq!(plan.sql.matches('?').count(), 1);
        assert!(plan.sql.starts_with("SELECT "));
        assert!(!plan.sql.contains(';'));
    }

    /// r15 byte parity: both schema probes are fixed owner-table literals with
    /// no binds; the TS renderer that built them is deleted by this cutover.
    #[test]
    fn schema_ready_probes_keep_their_exact_literals() {
        let billing = read_statement(RD_BILLING_TEST_FINANCE_SCHEMA).expect("billing probe");
        let mfg = read_statement(RD_MFG_EXISTING_POSTING_SCHEMA).expect("mfg probe");
        assert_eq!(billing.sql, "SELECT to_regclass('billing_test_finance_documents') IS NOT NULL ready");
        assert_eq!(mfg.sql, "SELECT to_regclass('commerce_mfg_existing_posting_confirmations') IS NOT NULL ready");
        for statement in [billing, mfg] {
            assert_eq!(statement.bind_count, 0);
            assert_eq!(statement.sql.matches('?').count(), 0);
            assert!(statement.sql.starts_with("SELECT "));
            assert!(!statement.sql.contains(';'));
        }
    }

    /// r25 byte parity: these are composite-query source fragments rather than
    /// standalone scoped reads, so they carry no binds. Ownership of the exact
    /// literal lives here; the cross-owner consumers still embed the TS text
    /// and must migrate before the renderer can be deleted.
    #[test]
    fn settlement_line_source_fragments_keep_their_exact_literals() {
        let head = "SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE ";
        let sales = source_fragment(RD_SALES_SETTLEMENT_LINES).expect("sales fragment");
        let supply = source_fragment(RD_SUPPLY_SETTLEMENT_LINES).expect("supply fragment");
        assert_eq!(sales.sql, format!("{head}parent.kind IN ('quote','sales')"));
        assert_eq!(supply.sql, format!("{head}parent.kind='purchase'"));
        for statement in [sales, supply] {
            assert_eq!(statement.bind_count, 0);
            assert_eq!(statement.sql.matches('?').count(), 0);
            assert!(statement.sql.starts_with("SELECT "));
            assert!(!statement.sql.contains(';'));
            assert!(!statement.sql.contains("FOR UPDATE"));
            assert!(read_statement(statement.kind).is_none());
        }
        assert_eq!(SOURCE_FRAGMENTS.len(), 2);
        assert!(source_fragment("commerce_rd_unknown").is_none());
    }

    /// The two r26 attachment source-image reads share a byte-exact head and
    /// differ only in the trailing quote/sale kind predicate; both carry the
    /// ID and company binds in their retired wire order.
    #[test]
    fn attachment_order_source_images_keep_their_exact_literals() {
        let head = "SELECT to_jsonb(source)::text image FROM commerce_orders source WHERE source.id=? AND source.company_id=?";
        let support = read_statement(RD_SUPPORT_ORDER_SOURCE_IMAGE).expect("support image read");
        let crm = read_statement(RD_CRM_ORDER_SOURCE_IMAGE).expect("crm image read");
        assert_eq!(support.sql, head);
        assert_eq!(crm.sql, format!("{head} AND source.kind IN ('quote','sales')"));
        for statement in [support, crm] {
            assert_eq!(statement.bind_count, 2);
            assert_eq!(statement.sql.matches('?').count(), 2);
            assert!(statement.sql.starts_with("SELECT "));
            assert!(!statement.sql.contains(';'));
            assert!(!statement.sql.contains("FOR UPDATE"));
        }
    }

    /// The two r27 settings-image reads differ only in the company predicate
    /// alias; both keep the retired literal byte-for-byte with one company bind.
    #[test]
    fn posting_setup_settings_image_reads_keep_their_exact_literals() {
        let head = "SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE ";
        let expense = read_statement(RD_EXPENSE_SETTINGS_IMAGE).expect("expense image read");
        let mfg = read_statement(RD_MFG_EXISTING_SETTINGS_IMAGE).expect("mfg image read");
        assert_eq!(expense.sql, format!("{head}s.company_id=?"));
        assert_eq!(mfg.sql, format!("{head}company_id=?"));
        for statement in [expense, mfg] {
            assert_eq!(statement.bind_count, 1);
            assert_eq!(statement.sql.matches('?').count(), 1);
            assert!(statement.sql.starts_with("SELECT "));
            assert!(!statement.sql.contains(';'));
            assert!(!statement.sql.contains("FOR UPDATE"));
        }
    }

    /// The two r28 selectors share a head but differ in predicate and order;
    /// both keep the retired literal byte-for-byte with one company bind.
    #[test]
    fn inventory_count_product_selectors_keep_their_exact_literals() {
        let head = "SELECT id,sku,name,unit,tracking_mode FROM commerce_products WHERE company_id=? AND status='active'";
        let stock = read_statement(RD_COUNT_ACTIVE_STOCK_PRODUCTS).expect("stock selector");
        let active = read_statement(RD_COUNT_ACTIVE_PRODUCTS).expect("active selector");
        assert_eq!(stock.sql, format!("{head} AND item_type='stock' ORDER BY sku,id"));
        assert_eq!(active.sql, format!("{head} ORDER BY sku"));
        for statement in [stock, active] {
            assert_eq!(statement.bind_count, 1);
            assert_eq!(statement.sql.matches('?').count(), 1);
            assert!(statement.sql.starts_with("SELECT "));
            assert!(!statement.sql.contains(';'));
            assert!(!statement.sql.contains("FOR UPDATE"));
        }
    }

    #[test]
    fn unknown_and_mismatched_read_kinds_fail_closed() {
        assert!(read_statement("SELECT 1 FROM commerce_settings").is_none());
        assert!(read_statement("").is_none());
        assert!(read_statement("commerce_rd_unknown").is_none());
        let statement = read_statement(RD_PMS_WAREHOUSE_ACTIVE).expect("warehouse read");
        assert!(statement.bind_count != 2);
    }

    #[test]
    fn warehouse_reads_keep_two_distinct_branch_positions() {
        for kind in [
            RD_WAREHOUSE_PAGE_ROWS,
            RD_WAREHOUSE_MASTER_ROWS,
            RD_WAREHOUSE_ACTIVE,
            RD_WAREHOUSES_ACTIVE,
        ] {
            let statement = read_statement(kind).expect("warehouse read");
            assert_eq!(
                statement.sql.matches("?::text IS NULL OR branch_id=?").count(),
                1,
                "{kind} must retain both branch bind positions"
            );
            assert_eq!(statement.sql.matches('?').count(), statement.bind_count, "{kind}");
        }
        assert_eq!(
            read_statement(RD_WAREHOUSE_BRANCH_ACTIVE)
                .expect("branch-active read")
                .bind_count,
            2
        );
    }
}
