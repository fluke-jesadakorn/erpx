//! Owner-local native read ports for the Commerce single-site configuration
//! read family (`G4 / migrate.commerce.residual-read-port`).
//!
//! A plan carries the byte-exact SQL of a retained TypeScript read, its ordered
//! bind positions and the authority facts of the caller. This is a registry,
//! not a consumer cutover: no route is mounted, no connection is opened and no
//! TypeScript reader is changed here. The caller runs a bound plan on its own
//! Holding transaction, so the live TypeScript read stays authoritative until
//! a later G4 retirement task switches it.
//!
//! The family covers the owner-local, single-Company product/warehouse/settings
//! master and configuration reads (including guard predicates) that are still
//! prepared directly by the retained adapters. Every statement stays on a
//! Commerce-owned table, is scoped to one Company (and optional branch) and
//! carries no cross-owner fragment, receipt/audit/outbox ledger or row lock.
//! Row-lock and cross-owner settlement reads are separate retained-authority
//! concerns and are deliberately not recreated here.
use erpx_server_shared::public::BusinessError;

/// Operation id a future native owner-read transport will advertise.
pub const OWNER_READ_OPERATION: &str = "commerce.residual-read-port.v1";

pub const CATEGORY_PRODUCT: &str = "commerce.product-config";
pub const CATEGORY_WAREHOUSE: &str = "commerce.warehouse-config";
pub const CATEGORY_SETTINGS: &str = "commerce.settings-config";

// -- product-config reads (guarded-source-dedupe-sql, commerce-pages, commerce,
//    ecommerce-sources, records-record-details-owned-facts, pos-core-facts) ---

pub const RD_PRODUCT_PAGE_ROWS: &str = "product.page-rows";
pub const RD_PRODUCT_MASTER_ROWS: &str = "product.master-rows";
pub const RD_PRODUCT_ACTIVE: &str = "product.active";
pub const RD_PRODUCT_COMPANY_ACTIVE_GUARD: &str = "product.company-active-guard";
pub const RD_PRODUCT_ACTIVE_OPTIONS: &str = "product.active-options";
pub const RD_PRODUCT_MASTER_DETAIL: &str = "product.master-detail";
pub const RD_PRODUCT_POINT: &str = "product.point";

// -- warehouse-config reads (guarded-source-dedupe-sql, commerce-pages,
//    commerce, ecommerce-sources) --------------------------------------------

pub const RD_WAREHOUSE_PAGE_ROWS: &str = "warehouse.page-rows";
pub const RD_WAREHOUSE_MASTER_ROWS: &str = "warehouse.master-rows";
pub const RD_WAREHOUSE_ACTIVE: &str = "warehouse.active";
pub const RD_WAREHOUSES_ACTIVE: &str = "warehouses.active";
pub const RD_WAREHOUSE_BRANCH_ACTIVE: &str = "warehouse.branch-active";
pub const RD_WAREHOUSE_IDENTITY: &str = "warehouse.identity";

// -- settings-config reads (settings-dedupe-sql, guarded-source-dedupe-sql) --

pub const RD_SETTINGS_BY_COMPANY: &str = "settings.by-company";
pub const RD_CURRENCY_BY_COMPANY: &str = "settings.currency-by-company";
pub const RD_POSTING_MAPPING_STATE: &str = "settings.posting-mapping-state";
pub const RD_POSTING_ABSENT_GUARD: &str = "settings.posting-absent-guard";

/// A bind position in a planned statement, in wire order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindField {
    /// Owner record id (`product`/`warehouse` caller id).
    RecordId,
    CompanyId,
    /// Both `(?::text IS NULL OR branch_id=?)` positions take this value;
    /// retained callers pass their single `branchId` twice. `None` binds SQL
    /// NULL so the unscoped predicate keeps its live behavior.
    BranchId,
}

/// Fields a kind needs from its caller before its plan can be bound.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScopeRequirement {
    pub record: bool,
    pub company: bool,
    pub branch: bool,
}

/// One caller-unit read plan: exact SQL, ordered binds and the retained
/// authority facts that must survive a later cutover.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadPlan {
    pub kind: &'static str,
    pub category: &'static str,
    pub caller: &'static str,
    pub sql: &'static str,
    pub binds: &'static [BindField],
    pub scope: ScopeRequirement,
    /// The retained caller's authority that a cutover must not silently drop.
    pub authority: &'static str,
    /// Cross-owner fragments still inlined in the SQL. Always empty for this
    /// owner-local family; a later cutover must keep them empty.
    pub cross_owner: &'static [&'static str],
}

// Verbatim statement literals from the authoritative TypeScript adapters.
const SQL_PRODUCT_PAGE_ROWS: &str =
    "SELECT *,sku AS page_key FROM commerce_products WHERE company_id=?";
const SQL_PRODUCT_MASTER_ROWS: &str =
    "SELECT * FROM commerce_products WHERE company_id=? ORDER BY sku";
const SQL_PRODUCT_ACTIVE: &str =
    "SELECT * FROM commerce_products WHERE id=? AND company_id=? AND status='active'";
const SQL_PRODUCT_COMPANY_ACTIVE_GUARD: &str =
    "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND status='active')";
const SQL_PRODUCT_ACTIVE_OPTIONS: &str =
    "SELECT id,name FROM commerce_products WHERE company_id=? AND status='active' ORDER BY name";
const SQL_PRODUCT_MASTER_DETAIL: &str =
    "SELECT * FROM commerce_products WHERE id=? AND company_id=?";
const SQL_PRODUCT_POINT: &str =
    "SELECT id FROM commerce_products WHERE id=? AND company_id=? AND status='active'";

const SQL_WAREHOUSE_PAGE_ROWS: &str = "SELECT *,name AS page_key FROM commerce_warehouses WHERE company_id=? AND (?::text IS NULL OR branch_id=?)";
const SQL_WAREHOUSE_MASTER_ROWS: &str = "SELECT * FROM commerce_warehouses WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name";
const SQL_WAREHOUSE_ACTIVE: &str = "SELECT * FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)";
const SQL_WAREHOUSES_ACTIVE: &str = "SELECT id,name,branch_id FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name";
const SQL_WAREHOUSE_BRANCH_ACTIVE: &str = "SELECT id,name FROM commerce_warehouses WHERE company_id=? AND branch_id=? AND status='active'";
const SQL_WAREHOUSE_IDENTITY: &str =
    "SELECT id,company_id,branch_id,status FROM commerce_warehouses";

const SQL_SETTINGS_BY_COMPANY: &str = "SELECT * FROM commerce_settings WHERE company_id=?";
const SQL_CURRENCY_BY_COMPANY: &str = "SELECT currency FROM commerce_settings WHERE company_id=?";
const SQL_POSTING_MAPPING_STATE: &str = concat!(
    "SELECT chart_id \"chartId\",currency,receivable_account_id \"receivableAccountId\",",
    "payable_account_id \"payableAccountId\",revenue_account_id \"revenueAccountId\",",
    "inventory_account_id \"inventoryAccountId\",clearing_account_id \"clearingAccountId\",",
    "cogs_account_id \"cogsAccountId\",input_tax_account_id \"inputTaxAccountId\",",
    "output_tax_account_id \"outputTaxAccountId\",cash_account_id \"cashAccountId\"",
    " FROM commerce_settings WHERE company_id=?"
);
const SQL_POSTING_ABSENT_GUARD: &str =
    "NOT EXISTS(SELECT 1 FROM commerce_settings WHERE company_id=?)";

const NO_SCOPE: ScopeRequirement = ScopeRequirement {
    record: false,
    company: false,
    branch: false,
};
const COMPANY_SCOPE: ScopeRequirement = ScopeRequirement {
    record: false,
    company: true,
    branch: false,
};
const BRANCH_SCOPE: ScopeRequirement = ScopeRequirement {
    record: false,
    company: true,
    branch: true,
};
const RECORD_SCOPE: ScopeRequirement = ScopeRequirement {
    record: true,
    company: true,
    branch: false,
};
const RECORD_BRANCH_SCOPE: ScopeRequirement = ScopeRequirement {
    record: true,
    company: true,
    branch: true,
};

const PRODUCT_AUTHORITY: &str = "commerce_products";
const WAREHOUSE_AUTHORITY: &str = "commerce_warehouses";
const SETTINGS_AUTHORITY: &str = "commerce_settings";
const NO_CROSS: &[&str] = &[];

/// Exact live read of `commercePages.products`.
pub const PLAN_PRODUCT_PAGE_ROWS: ReadPlan = ReadPlan {
    kind: RD_PRODUCT_PAGE_ROWS,
    category: CATEGORY_PRODUCT,
    caller: "commerce.pages.products",
    sql: SQL_PRODUCT_PAGE_ROWS,
    binds: &[BindField::CompanyId],
    scope: COMPANY_SCOPE,
    authority: PRODUCT_AUTHORITY,
    cross_owner: NO_CROSS,
};
/// Exact live read of `unit.readCommerceProducts1`.
pub const PLAN_PRODUCT_MASTER_ROWS: ReadPlan = ReadPlan {
    kind: RD_PRODUCT_MASTER_ROWS,
    category: CATEGORY_PRODUCT,
    caller: "commerce.unit.readCommerceProducts1",
    sql: SQL_PRODUCT_MASTER_ROWS,
    binds: &[BindField::CompanyId],
    scope: COMPANY_SCOPE,
    authority: PRODUCT_AUTHORITY,
    cross_owner: NO_CROSS,
};
/// Exact live read of `COMMERCE_PRODUCT_ACTIVE_SQL`.
pub const PLAN_PRODUCT_ACTIVE: ReadPlan = ReadPlan {
    kind: RD_PRODUCT_ACTIVE,
    category: CATEGORY_PRODUCT,
    caller: "commerce.guards.product-active",
    sql: SQL_PRODUCT_ACTIVE,
    binds: &[BindField::RecordId, BindField::CompanyId],
    scope: RECORD_SCOPE,
    authority: PRODUCT_AUTHORITY,
    cross_owner: NO_CROSS,
};
/// Exact live predicate of `COMMERCE_PRODUCT_COMPANY_ACTIVE_GUARD_SQL`.
pub const PLAN_PRODUCT_COMPANY_ACTIVE_GUARD: ReadPlan = ReadPlan {
    kind: RD_PRODUCT_COMPANY_ACTIVE_GUARD,
    category: CATEGORY_PRODUCT,
    caller: "commerce.guards.product-company-active",
    sql: SQL_PRODUCT_COMPANY_ACTIVE_GUARD,
    binds: &[BindField::RecordId, BindField::CompanyId],
    scope: RECORD_SCOPE,
    authority: PRODUCT_AUTHORITY,
    cross_owner: NO_CROSS,
};
/// Exact live read of `ecommerceProductCandidates`.
pub const PLAN_PRODUCT_ACTIVE_OPTIONS: ReadPlan = ReadPlan {
    kind: RD_PRODUCT_ACTIVE_OPTIONS,
    category: CATEGORY_PRODUCT,
    caller: "ecommerce.product-candidates",
    sql: SQL_PRODUCT_ACTIVE_OPTIONS,
    binds: &[BindField::CompanyId],
    scope: COMPANY_SCOPE,
    authority: PRODUCT_AUTHORITY,
    cross_owner: NO_CROSS,
};
/// Exact live read of `RecordDetailsCommerceOwnedFacts.readCommerceMasterDetailRow`.
pub const PLAN_PRODUCT_MASTER_DETAIL: ReadPlan = ReadPlan {
    kind: RD_PRODUCT_MASTER_DETAIL,
    category: CATEGORY_PRODUCT,
    caller: "records.owned-facts.product-master-detail",
    sql: SQL_PRODUCT_MASTER_DETAIL,
    binds: &[BindField::RecordId, BindField::CompanyId],
    scope: RECORD_SCOPE,
    authority: PRODUCT_AUTHORITY,
    cross_owner: NO_CROSS,
};
/// Exact live read of `COMMERCE_PRODUCT_POINT_SQL`.
pub const PLAN_PRODUCT_POINT: ReadPlan = ReadPlan {
    kind: RD_PRODUCT_POINT,
    category: CATEGORY_PRODUCT,
    caller: "pos.core-facts.product-point",
    sql: SQL_PRODUCT_POINT,
    binds: &[BindField::RecordId, BindField::CompanyId],
    scope: RECORD_SCOPE,
    authority: PRODUCT_AUTHORITY,
    cross_owner: NO_CROSS,
};

/// Exact live read of `commercePages.warehouses`.
pub const PLAN_WAREHOUSE_PAGE_ROWS: ReadPlan = ReadPlan {
    kind: RD_WAREHOUSE_PAGE_ROWS,
    category: CATEGORY_WAREHOUSE,
    caller: "commerce.pages.warehouses",
    sql: SQL_WAREHOUSE_PAGE_ROWS,
    binds: &[
        BindField::CompanyId,
        BindField::BranchId,
        BindField::BranchId,
    ],
    scope: BRANCH_SCOPE,
    authority: WAREHOUSE_AUTHORITY,
    cross_owner: NO_CROSS,
};
/// Exact live read of `unit.readCommerceWarehouses1`.
pub const PLAN_WAREHOUSE_MASTER_ROWS: ReadPlan = ReadPlan {
    kind: RD_WAREHOUSE_MASTER_ROWS,
    category: CATEGORY_WAREHOUSE,
    caller: "commerce.unit.readCommerceWarehouses1",
    sql: SQL_WAREHOUSE_MASTER_ROWS,
    binds: &[
        BindField::CompanyId,
        BindField::BranchId,
        BindField::BranchId,
    ],
    scope: BRANCH_SCOPE,
    authority: WAREHOUSE_AUTHORITY,
    cross_owner: NO_CROSS,
};
/// Exact live read of `COMMERCE_WAREHOUSE_ACTIVE_SQL`.
pub const PLAN_WAREHOUSE_ACTIVE: ReadPlan = ReadPlan {
    kind: RD_WAREHOUSE_ACTIVE,
    category: CATEGORY_WAREHOUSE,
    caller: "commerce.guards.warehouse-active",
    sql: SQL_WAREHOUSE_ACTIVE,
    binds: &[
        BindField::RecordId,
        BindField::CompanyId,
        BindField::BranchId,
        BindField::BranchId,
    ],
    scope: RECORD_BRANCH_SCOPE,
    authority: WAREHOUSE_AUTHORITY,
    cross_owner: NO_CROSS,
};
/// Exact live read of `COMMERCE_ACTIVE_WAREHOUSES_SQL`.
pub const PLAN_WAREHOUSES_ACTIVE: ReadPlan = ReadPlan {
    kind: RD_WAREHOUSES_ACTIVE,
    category: CATEGORY_WAREHOUSE,
    caller: "ecommerce.warehouse-candidates",
    sql: SQL_WAREHOUSES_ACTIVE,
    binds: &[
        BindField::CompanyId,
        BindField::BranchId,
        BindField::BranchId,
    ],
    scope: BRANCH_SCOPE,
    authority: WAREHOUSE_AUTHORITY,
    cross_owner: NO_CROSS,
};
/// Exact live read of `COMMERCE_WAREHOUSE_BRANCH_ACTIVE_SQL`.
pub const PLAN_WAREHOUSE_BRANCH_ACTIVE: ReadPlan = ReadPlan {
    kind: RD_WAREHOUSE_BRANCH_ACTIVE,
    category: CATEGORY_WAREHOUSE,
    caller: "commerce.guards.warehouse-branch-active",
    sql: SQL_WAREHOUSE_BRANCH_ACTIVE,
    binds: &[BindField::CompanyId, BindField::BranchId],
    scope: BRANCH_SCOPE,
    authority: WAREHOUSE_AUTHORITY,
    cross_owner: NO_CROSS,
};
/// Exact live read of `COMMERCE_WAREHOUSE_IDENTITY_SQL`.
pub const PLAN_WAREHOUSE_IDENTITY: ReadPlan = ReadPlan {
    kind: RD_WAREHOUSE_IDENTITY,
    category: CATEGORY_WAREHOUSE,
    caller: "commerce.guards.warehouse-identity",
    sql: SQL_WAREHOUSE_IDENTITY,
    binds: &[],
    scope: NO_SCOPE,
    authority: WAREHOUSE_AUTHORITY,
    cross_owner: NO_CROSS,
};

/// Exact live read of `COMMERCE_SETTINGS_BY_COMPANY_SQL`.
pub const PLAN_SETTINGS_BY_COMPANY: ReadPlan = ReadPlan {
    kind: RD_SETTINGS_BY_COMPANY,
    category: CATEGORY_SETTINGS,
    caller: "commerce.settings.by-company",
    sql: SQL_SETTINGS_BY_COMPANY,
    binds: &[BindField::CompanyId],
    scope: COMPANY_SCOPE,
    authority: SETTINGS_AUTHORITY,
    cross_owner: NO_CROSS,
};
/// Exact live read of `COMMERCE_CURRENCY_BY_COMPANY_SQL`.
pub const PLAN_CURRENCY_BY_COMPANY: ReadPlan = ReadPlan {
    kind: RD_CURRENCY_BY_COMPANY,
    category: CATEGORY_SETTINGS,
    caller: "commerce.settings.currency-by-company",
    sql: SQL_CURRENCY_BY_COMPANY,
    binds: &[BindField::CompanyId],
    scope: COMPANY_SCOPE,
    authority: SETTINGS_AUTHORITY,
    cross_owner: NO_CROSS,
};
/// Exact live read of `COMMERCE_POSTING_MAPPING_STATE_SQL`.
pub const PLAN_POSTING_MAPPING_STATE: ReadPlan = ReadPlan {
    kind: RD_POSTING_MAPPING_STATE,
    category: CATEGORY_SETTINGS,
    caller: "commerce.settings.posting-mapping",
    sql: SQL_POSTING_MAPPING_STATE,
    binds: &[BindField::CompanyId],
    scope: COMPANY_SCOPE,
    authority: SETTINGS_AUTHORITY,
    cross_owner: NO_CROSS,
};
/// Exact live predicate of `COMMERCE_POSTING_ABSENT_SQL`.
pub const PLAN_POSTING_ABSENT_GUARD: ReadPlan = ReadPlan {
    kind: RD_POSTING_ABSENT_GUARD,
    category: CATEGORY_SETTINGS,
    caller: "commerce.guards.posting-absent",
    sql: SQL_POSTING_ABSENT_GUARD,
    binds: &[BindField::CompanyId],
    scope: COMPANY_SCOPE,
    authority: SETTINGS_AUTHORITY,
    cross_owner: NO_CROSS,
};

/// The closed registry, in stable wire order.
pub const PLANS: &[ReadPlan] = &[
    PLAN_PRODUCT_PAGE_ROWS,
    PLAN_PRODUCT_MASTER_ROWS,
    PLAN_PRODUCT_ACTIVE,
    PLAN_PRODUCT_COMPANY_ACTIVE_GUARD,
    PLAN_PRODUCT_ACTIVE_OPTIONS,
    PLAN_PRODUCT_MASTER_DETAIL,
    PLAN_PRODUCT_POINT,
    PLAN_WAREHOUSE_PAGE_ROWS,
    PLAN_WAREHOUSE_MASTER_ROWS,
    PLAN_WAREHOUSE_ACTIVE,
    PLAN_WAREHOUSES_ACTIVE,
    PLAN_WAREHOUSE_BRANCH_ACTIVE,
    PLAN_WAREHOUSE_IDENTITY,
    PLAN_SETTINGS_BY_COMPANY,
    PLAN_CURRENCY_BY_COMPANY,
    PLAN_POSTING_MAPPING_STATE,
    PLAN_POSTING_ABSENT_GUARD,
];

pub fn plan(kind: &str) -> Option<&'static ReadPlan> {
    PLANS.iter().find(|entry| entry.kind == kind)
}

pub fn is_owner_read(kind: &str) -> bool {
    plan(kind).is_some()
}

pub fn kinds() -> impl Iterator<Item = &'static str> {
    PLANS.iter().map(|entry| entry.kind)
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_READ_PLAN_INVALID",
        "Review the Commerce owner read plan.",
    )
}

/// Remove whitespace so TypeScript template indentation cannot change token
/// identity when a retained literal is compared to a registered statement.
pub fn norm(value: &str) -> String {
    value.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Count `?` bind placeholders outside single-quoted SQL literals.
pub fn placeholder_count(sql: &str) -> usize {
    let mut count = 0;
    let mut quoted = false;
    let mut chars = sql.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                } else {
                    quoted = false;
                }
            }
        } else if ch == '\'' {
            quoted = true;
        } else if ch == '?' {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    const TS_GUARDS: &str = include_str!("../adapters/guarded-source-dedupe-sql.ts");
    const TS_SETTINGS: &str = include_str!("../adapters/settings-dedupe-sql.ts");
    const TS_PAGES: &str = include_str!("../adapters/commerce-pages.ts");
    const TS_UNIT: &str = include_str!("../adapters/commerce.ts");
    const TS_ECOMMERCE: &str = include_str!("../adapters/ecommerce-sources.ts");
    const TS_RECORDS: &str = include_str!("../adapters/records-record-details-owned-facts.ts");
    const TS_POS: &str = include_str!("../adapters/pos-core-facts.ts");

    fn source(kind: &str) -> &'static str {
        match kind {
            RD_PRODUCT_PAGE_ROWS | RD_WAREHOUSE_PAGE_ROWS => TS_PAGES,
            RD_PRODUCT_MASTER_ROWS | RD_WAREHOUSE_MASTER_ROWS => TS_UNIT,
            RD_PRODUCT_ACTIVE
            | RD_PRODUCT_COMPANY_ACTIVE_GUARD
            | RD_WAREHOUSE_ACTIVE
            | RD_WAREHOUSES_ACTIVE
            | RD_WAREHOUSE_BRANCH_ACTIVE
            | RD_WAREHOUSE_IDENTITY
            | RD_POSTING_ABSENT_GUARD => TS_GUARDS,
            RD_PRODUCT_ACTIVE_OPTIONS => TS_ECOMMERCE,
            RD_PRODUCT_MASTER_DETAIL => TS_RECORDS,
            RD_PRODUCT_POINT => TS_POS,
            RD_SETTINGS_BY_COMPANY | RD_CURRENCY_BY_COMPANY | RD_POSTING_MAPPING_STATE => {
                TS_SETTINGS
            }
            other => panic!("no retained authority source for {other}"),
        }
    }

    #[test]
    fn closed_registry_of_seventeen_owner_local_reads() {
        assert_eq!(PLANS.len(), 17);
        assert_eq!(kinds().count(), 17);
        assert_eq!(
            kinds().collect::<std::collections::BTreeSet<_>>().len(),
            17,
            "read kinds must be unique"
        );
        for kind in kinds() {
            assert!(plan(kind).is_some(), "{kind}");
            assert!(is_owner_read(kind), "{kind}");
        }
        assert_eq!(plan("bogus.read"), None);
        assert!(!is_owner_read("commerce-order-read"));
    }

    #[test]
    fn every_plan_is_select_only_with_exact_bind_count_and_no_locks() {
        for entry in PLANS {
            let statement = entry.sql;
            assert!(
                statement.starts_with("SELECT ")
                    || statement.starts_with("EXISTS(")
                    || statement.starts_with("NOT EXISTS("),
                "{}: {statement}",
                entry.kind
            );
            for banned in [
                "INSERT ",
                "UPDATE ",
                "DELETE ",
                "RETURNING",
                "FOR UPDATE",
                "LOCK ",
                "503",
                "UNAVAILABLE",
            ] {
                assert!(
                    !statement.contains(banned),
                    "{} contains {banned}",
                    entry.kind
                );
            }
            assert_eq!(
                placeholder_count(statement),
                entry.binds.len(),
                "{} bind count",
                entry.kind
            );
            assert!(
                statement.contains("commerce_products")
                    || statement.contains("commerce_warehouses")
                    || statement.contains("commerce_settings"),
                "{} must stay on a Commerce-owned table",
                entry.kind
            );
            assert!(!entry.caller.is_empty(), "{}", entry.kind);
            assert!(
                entry.authority.contains("commerce_"),
                "{} authority",
                entry.kind
            );
            assert!(entry.cross_owner.is_empty(), "{}", entry.kind);
            assert!(!entry.category.is_empty(), "{}", entry.kind);
        }
    }

    #[test]
    fn bind_positions_match_scope_requirements() {
        for entry in PLANS {
            let needs_record = entry.binds.contains(&BindField::RecordId);
            let needs_company = entry.binds.contains(&BindField::CompanyId);
            let needs_branch = entry.binds.contains(&BindField::BranchId);
            assert_eq!(entry.scope.record, needs_record, "{}", entry.kind);
            assert_eq!(entry.scope.company, needs_company, "{}", entry.kind);
            assert_eq!(entry.scope.branch, needs_branch, "{}", entry.kind);
        }
    }

    #[test]
    fn byte_parity_with_retained_typescript_statements() {
        for entry in PLANS {
            assert!(
                norm(source(entry.kind)).contains(&norm(entry.sql)),
                "{}: exact SQL missing from its TypeScript authority",
                entry.kind
            );
        }
    }
}
