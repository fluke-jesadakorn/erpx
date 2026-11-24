//! Owner-owned caller-unit read plans for Commerce. Generated from the retired
//! TypeScript SELECT literals with the exact positional payloads; the mounted
//! `commerce/command-plan` route resolves the allow-listed kind here and returns
//! the statement for the caller to prepare and execute on its own open unit.
//!
//! This family covers small owner-local Commerce master/setup reads on
//! `commerce_warehouses`, `commerce_products` and `commerce_settings`. Reads carry no receipt/audit/
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
        ];
        for (kind, sql, bind_count) in expected {
            let statement = read_statement(kind).expect("registered owner read kind");
            assert_eq!(statement.kind, kind);
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.bind_count, bind_count);
            assert!(sql.starts_with("SELECT "), "owner read must be SELECT-only: {sql}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
                assert!(!sql.contains(forbidden), "owner read must not mutate: {forbidden}");
            }
            assert!(
                sql.contains("commerce_products") || sql.contains("commerce_warehouses") || sql.contains("commerce_settings"),
                "owner read must stay on owner-local Commerce tables: {sql}"
            );
            assert_eq!(sql.matches('?').count(), bind_count);
        }
        assert_eq!(STATEMENTS.len(), 15);
    }

    #[test]
    fn unknown_and_mismatched_read_kinds_fail_closed() {
        assert!(read_statement("SELECT 1 FROM commerce_settings").is_none());
        assert!(read_statement("").is_none());
        assert!(read_statement("commerce_rd_unknown").is_none());
        let statement = read_statement(RD_PMS_WAREHOUSE_ACTIVE).expect("warehouse read");
        assert!(statement.bind_count != 2);
    }
}
