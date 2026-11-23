//! Caller-unit native Manufacturing read plan (`G4 / migrate.manufacturing.read-plan`).
//!
//! This is a **plan registry**, not a consumer cutover. It records the exact
//! reads that live Manufacturing, Planning and Records callers perform today,
//! so a future native owner-read route can serve those callers without
//! replacing a working read with a fail-closed 503 marker. Nothing here is
//! mounted on a route, nothing is fetched over HTTP, and no live TypeScript
//! read is changed by this module.
//!
//! # Live caller inventory (source of truth at this slice)
//!
//! | Caller unit | Live source | Read methods / joins |
//! | --- | --- | --- |
//! | `planning.scenarios` | `server-planning/adapters/production-command-sources-read.ts` | `scenarios`, `recommendations`, `executiveShortage`, `executiveRecommendation`, `comparison`, `scenario`, `scopedRecommendation`; join `manufacturing_specifications` via the manufacturing `scenarioSpecification` / `executiveSpecification` fragments |
//! | `manufacturing.workspace` | `server-manufacturing/adapters/production-command-sources.ts` | `specifications`, `orders`, `openOrders`, `accountingProfiles`; `orders` joins the specification and Commerce output product |
//! | `records.detail` / `production.detail` | same owner source + `adapters/production.ts` | `specification`, `specificationLines`, `detailedOrder`, `scopedOrder`, `orderSpecificationLines`, `orderReservations`, `orderEvents`, `costSnapshot`, `productGenealogy`, `orderPostings`, `orderOperations`, `lotGenealogy`, `materialReservation` |
//! | `product.get` | `server-manufacturing/adapters/production-command-sources.ts` | `specification`, `specificationLines`, `orderSpecificationLines`; material lines join Commerce product tracking mode |
//!
//! # Authority and scoping facts (from `adapters/purchases.ts`)
//!
//! Every production read is resolved on the caller's already-open Holding
//! transaction. `manufacturingSourceAccess` -> `current` requires: the current
//! original `production_scope` assignment still matches the actor
//! (`assignmentId`, `userId`, `role`, `companyId`, `branchId`, `departmentId`),
//! the assignment still selects the `planning` or `manufacturing` module, and
//! `records.read` (plus `records.write` for `records.review`). Company isolation
//! is `company_id = actor.companyId`; branch scoping is the optional
//! `(?::text IS NULL OR branch_id = ?)` predicate with `actor.branchId`. A
//! source witness decides whether `planning.advanced-mrp` is required. On a
//! transient source-view miss a read falls back to the base capabilities: it
//! never turns that miss into a 404, and it never substitutes a 503 marker.
//!
//! The SQL below is the *exact* projection/join of each live caller (the
//! cross-owner fragments are inlined verbatim). Owner SQL stays SELECT-only,
//! carries no transaction control and no fail-closed status literal. The
//! caller keeps its own binds, locks and single SQL snapshot, exactly as it
//! does today. A read is cut over only after a native transport and per-caller
//! parity gate described in `tests/unit/manufacturing/native-read-plan.spec.md`.
use erpx_server_shared::public::BusinessError;

/// Operation id a future native owner-read route will advertise.
pub const OWNER_READ_OPERATION: &str = "manufacturing.owner-read.v1";

// Closed RD_* read-kind identifiers. These names are stable wire values for a
// future native owner-read route; callers never send SQL.
pub const RD_SPECIFICATION: &str = "specification";
pub const RD_SPECIFICATION_VERSION: &str = "specification-version";
pub const RD_SPECIFICATION_LINES: &str = "specification-lines";
pub const RD_ORDERS: &str = "orders";
pub const RD_ORDER_DETAIL: &str = "order-detail";
pub const RD_MATERIAL_RESERVATIONS: &str = "material-reservations";
pub const RD_ORDER_EVENTS: &str = "order-events";
pub const RD_ORDER_POSTINGS: &str = "order-postings";
pub const RD_ORDER_OPERATIONS: &str = "order-operations";
pub const RD_LOT_GENEALOGY: &str = "lot-genealogy";
pub const RD_PRODUCT_GENEALOGY: &str = "product-genealogy";
pub const RD_ACCOUNTING_PROFILES: &str = "accounting-profiles";

/// A bind position in a planned statement, in wire order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindField {
    /// Generic record id (`specification`/`order-detail` caller id).
    Id,
    CompanyId,
    /// Optional: a `None` scope binds SQL NULL and keeps the unscoped branch predicate.
    BranchId,
    SpecificationId,
    OrderId,
    Version,
}

/// Fields a kind needs from the caller unit before its plan can be bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScopeRequirement {
    pub company: bool,
    pub branch: bool,
    pub order: bool,
    pub specification: bool,
    pub version: bool,
}

/// Authority facts the live caller already enforces, restated on the plan so a
/// cutover cannot silently drop a module, permission or source witness check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Authority {
    pub permission: &'static str,
    /// `planning`, `manufacturing` or `planning|manufacturing`.
    pub module: &'static str,
    /// Purchase source witness capability family.
    pub source: &'static str,
}

/// One caller-unit read plan: exact SQL, ordered binds and the authority facts
/// that must remain true when a caller is eventually switched to it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadPlan {
    pub kind: &'static str,
    pub caller: &'static str,
    pub sql: &'static str,
    pub binds: &'static [BindField],
    pub scope: ScopeRequirement,
    pub authority: Authority,
    /// Cross-owner fragments still present in the SQL. They stay inside the
    /// caller's transaction until their owner exposes a native subquery.
    pub cross_owner: &'static [&'static str],
}

const READ: Authority = Authority {
    permission: "records.read",
    module: "planning|manufacturing",
    source: "records.source",
};
const WRITE_REVIEW: Authority = Authority {
    permission: "records.write+records.review",
    module: "manufacturing",
    source: "manufacturing.execution",
};

/// Exact live SQL for `productionCommandSources.specification`.
pub const PLAN_SPECIFICATION: ReadPlan = ReadPlan {
    kind: RD_SPECIFICATION,
    caller: "records.detail|product.get|planning.scenarios",
    sql: "SELECT * FROM manufacturing_specifications WHERE id=? AND company_id=?",
    binds: &[BindField::Id, BindField::CompanyId],
    scope: ScopeRequirement {
        company: true,
        branch: false,
        order: false,
        specification: false,
        version: false,
    },
    authority: READ,
    cross_owner: &[],
};

/// Exact live SQL for `productionCommandSources.specification` with a version.
pub const PLAN_SPECIFICATION_VERSION: ReadPlan = ReadPlan {
    kind: RD_SPECIFICATION_VERSION,
    caller: "planning.scenarios|manufacturing.release",
    sql: "SELECT * FROM manufacturing_specifications WHERE id=? AND company_id=? AND version=?",
    binds: &[BindField::Id, BindField::CompanyId, BindField::Version],
    scope: ScopeRequirement {
        company: true,
        branch: false,
        order: false,
        specification: false,
        version: true,
    },
    authority: WRITE_REVIEW,
    cross_owner: &[],
};

/// Exact live SQL for `productionCommandSources.specificationLines`.
pub const PLAN_SPECIFICATION_LINES: ReadPlan = ReadPlan {
    kind: RD_SPECIFICATION_LINES,
    caller: "product.get|records.detail",
    sql: "SELECT * FROM manufacturing_specification_lines WHERE specification_id=? ORDER BY ordinal",
    binds: &[BindField::SpecificationId],
    scope: ScopeRequirement {
        company: false,
        branch: false,
        order: false,
        specification: true,
        version: false,
    },
    authority: READ,
    cross_owner: &[],
};

/// Exact live SQL for `productionCommandSources.orders` (Commerce fragments inlined).
pub const PLAN_ORDERS: ReadPlan = ReadPlan {
    kind: RD_ORDERS,
    caller: "manufacturing.workspace",
    sql: "SELECT o.*,s.name specification_name,p.name output_name FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id JOIN commerce_products p ON p.id=s.output_product_id WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.updated_at DESC",
    binds: &[
        BindField::CompanyId,
        BindField::BranchId,
        BindField::BranchId,
    ],
    scope: ScopeRequirement {
        company: true,
        branch: true,
        order: false,
        specification: false,
        version: false,
    },
    authority: READ,
    cross_owner: &["JOIN commerce_products p ON p.id=s.output_product_id"],
};

/// Exact live SQL for `productionCommandSources.detailedOrder`.
pub const PLAN_ORDER_DETAIL: ReadPlan = ReadPlan {
    kind: RD_ORDER_DETAIL,
    caller: "records.detail|production.detail",
    sql: "SELECT o.*,s.name specification_name,s.mode,p.name output_name,p.tracking_mode output_tracking_mode FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id JOIN commerce_products p ON p.id=s.output_product_id WHERE o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?)",
    binds: &[
        BindField::Id,
        BindField::CompanyId,
        BindField::BranchId,
        BindField::BranchId,
    ],
    scope: ScopeRequirement {
        company: true,
        branch: true,
        order: false,
        specification: false,
        version: false,
    },
    authority: READ,
    cross_owner: &["JOIN commerce_products p ON p.id=s.output_product_id"],
};

/// Exact live SQL for `productionCommandSources.orderReservations`.
pub const PLAN_MATERIAL_RESERVATIONS: ReadPlan = ReadPlan {
    kind: RD_MATERIAL_RESERVATIONS,
    caller: "records.detail|production.detail",
    sql: "SELECT r.*,p.name product_name,p.tracking_mode FROM manufacturing_material_reservations r JOIN commerce_products p ON p.id=r.product_id WHERE r.order_id=? ORDER BY p.name",
    binds: &[BindField::OrderId],
    scope: ScopeRequirement {
        company: false,
        branch: false,
        order: true,
        specification: false,
        version: false,
    },
    authority: READ,
    cross_owner: &["JOIN commerce_products p ON p.id=r.product_id"],
};

/// Exact live SQL for `productionCommandSources.orderEvents`.
pub const PLAN_ORDER_EVENTS: ReadPlan = ReadPlan {
    kind: RD_ORDER_EVENTS,
    caller: "records.detail|production.detail",
    sql: "SELECT * FROM manufacturing_order_events WHERE order_id=? ORDER BY version",
    binds: &[BindField::OrderId],
    scope: ScopeRequirement {
        company: false,
        branch: false,
        order: true,
        specification: false,
        version: false,
    },
    authority: READ,
    cross_owner: &[],
};

/// Exact live SQL for `productionCommandSources.orderPostings` (Finance journal fragment inlined).
pub const PLAN_ORDER_POSTINGS: ReadPlan = ReadPlan {
    kind: RD_ORDER_POSTINGS,
    caller: "records.detail|finance.reconciliation",
    sql: "SELECT p.*,j.status journal_status FROM manufacturing_order_postings p JOIN finance_journals j ON j.id=p.journal_id WHERE p.order_id=? ORDER BY p.source_order_version",
    binds: &[BindField::OrderId],
    scope: ScopeRequirement {
        company: false,
        branch: false,
        order: true,
        specification: false,
        version: false,
    },
    authority: READ,
    cross_owner: &["JOIN finance_journals j ON j.id=p.journal_id"],
};

/// Exact live SQL for `productionCommandSources.orderOperations` (Planning resource + operation events).
pub const PLAN_ORDER_OPERATIONS: ReadPlan = ReadPlan {
    kind: RD_ORDER_OPERATIONS,
    caller: "records.detail|production.detail",
    sql: "SELECT op.*,r.name resource_name,e.labor_minutes,e.machine_minutes,e.evidence,e.created_at completed_at FROM manufacturing_specification_operations op JOIN production_resources r ON r.id=op.resource_id LEFT JOIN manufacturing_operation_events e ON e.operation_id=op.id AND e.order_id=? WHERE op.specification_id=? ORDER BY op.ordinal",
    binds: &[BindField::OrderId, BindField::SpecificationId],
    scope: ScopeRequirement {
        company: false,
        branch: false,
        order: true,
        specification: true,
        version: false,
    },
    authority: READ,
    cross_owner: &["JOIN production_resources r ON r.id=op.resource_id"],
};

/// Exact live SQL for `productionCommandSources.lotGenealogy` (Inventory lot fragments inlined).
pub const PLAN_LOT_GENEALOGY: ReadPlan = ReadPlan {
    kind: RD_LOT_GENEALOGY,
    caller: "records.detail|production.detail",
    sql: "SELECT g.*,i.lot_id input_lot_id,il.lot_code input_lot_code,il.serial_number input_serial_number,ol.lot_id output_lot_id,l.lot_code output_lot_code,l.serial_number output_serial_number FROM manufacturing_lot_genealogy g JOIN manufacturing_material_lot_issues i ON i.id=g.input_issue_id JOIN inventory_lots il ON il.id=i.lot_id JOIN manufacturing_output_lots ol ON ol.id=g.output_lot_id JOIN inventory_lots l ON l.id=ol.lot_id WHERE g.order_id=? ORDER BY g.id",
    binds: &[BindField::OrderId],
    scope: ScopeRequirement {
        company: false,
        branch: false,
        order: true,
        specification: false,
        version: false,
    },
    authority: READ,
    cross_owner: &[
        "JOIN inventory_lots il ON il.id=i.lot_id",
        "JOIN inventory_lots l ON l.id=ol.lot_id",
    ],
};

/// Exact live SQL for `productionCommandSources.productGenealogy`.
pub const PLAN_PRODUCT_GENEALOGY: ReadPlan = ReadPlan {
    kind: RD_PRODUCT_GENEALOGY,
    caller: "records.detail|production.detail",
    sql: "SELECT * FROM manufacturing_genealogy WHERE order_id=?",
    binds: &[BindField::OrderId],
    scope: ScopeRequirement {
        company: false,
        branch: false,
        order: true,
        specification: false,
        version: false,
    },
    authority: READ,
    cross_owner: &[],
};

/// Exact live SQL for `productionCommandSources.accountingProfiles`.
pub const PLAN_ACCOUNTING_PROFILES: ReadPlan = ReadPlan {
    kind: RD_ACCOUNTING_PROFILES,
    caller: "manufacturing.workspace|costing.setup",
    sql: "SELECT id,status,version,currency,chart_id,inventory_account_id,wip_account_id,labor_account_id,overhead_account_id FROM manufacturing_accounting_profiles WHERE company_id=? ORDER BY updated_at DESC",
    binds: &[BindField::CompanyId],
    scope: ScopeRequirement {
        company: true,
        branch: false,
        order: false,
        specification: false,
        version: false,
    },
    authority: READ,
    cross_owner: &[],
};

/// The closed registry, in stable wire order.
pub const PLANS: &[ReadPlan] = &[
    PLAN_SPECIFICATION,
    PLAN_SPECIFICATION_VERSION,
    PLAN_SPECIFICATION_LINES,
    PLAN_ORDERS,
    PLAN_ORDER_DETAIL,
    PLAN_MATERIAL_RESERVATIONS,
    PLAN_ORDER_EVENTS,
    PLAN_ORDER_POSTINGS,
    PLAN_ORDER_OPERATIONS,
    PLAN_LOT_GENEALOGY,
    PLAN_PRODUCT_GENEALOGY,
    PLAN_ACCOUNTING_PROFILES,
];

/// Resolve a registered read kind. Unknown kinds are rejected at planning time;
/// they never cause a live read to fail or fall back to a 503 marker.
pub fn plan(kind: &str) -> Option<&'static ReadPlan> {
    PLANS.iter().find(|entry| entry.kind == kind)
}

pub fn is_owner_read(kind: &str) -> bool {
    plan(kind).is_some()
}

pub fn kinds() -> impl Iterator<Item = &'static str> {
    PLANS.iter().map(|entry| entry.kind)
}

/// Invalid-kind validation error for a future native owner-read transport.
pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_READ_PLAN_INVALID",
        "Review the Manufacturing owner read plan.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_registry_of_twelve_caller_unit_reads() {
        assert_eq!(PLANS.len(), 12);
        assert_eq!(kinds().count(), 12);
        for kind in kinds() {
            assert!(plan(kind).is_some(), "{kind}");
            assert!(is_owner_read(kind), "{kind}");
        }
        assert_eq!(
            kinds().collect::<std::collections::BTreeSet<_>>().len(),
            12,
            "read kinds must be unique"
        );
        assert_eq!(plan("bogus.read"), None);
        assert!(!is_owner_read("specification-prune"));
    }

    #[test]
    fn plans_are_select_only_without_fail_closed_markers() {
        for entry in PLANS {
            let sql = entry.sql;
            assert!(sql.starts_with("SELECT "), "{}", entry.kind);
            assert!(!sql.contains(';'), "{}", entry.kind);
            for banned in [
                "INSERT",
                "UPDATE ",
                "DELETE",
                "FOR UPDATE",
                "503",
                "UNAVAILABLE",
                "fail closed",
            ] {
                assert!(!sql.contains(banned), "{} contains {banned}", entry.kind);
            }
            assert_eq!(
                sql.matches('?').count(),
                entry.binds.len(),
                "{} bind count",
                entry.kind
            );
        }
    }

    #[test]
    fn every_plan_carries_authority_and_cross_owner_disclosure() {
        for entry in PLANS {
            assert!(!entry.caller.is_empty(), "{}", entry.kind);
            assert!(!entry.authority.permission.is_empty(), "{}", entry.kind);
            assert!(!entry.authority.module.is_empty(), "{}", entry.kind);
            assert!(!entry.authority.source.is_empty(), "{}", entry.kind);
            for fragment in entry.cross_owner {
                assert!(
                    entry.sql.contains(fragment),
                    "{} missing {fragment}",
                    entry.kind
                );
            }
        }
    }

    #[test]
    fn byte_parity_with_live_caller_sql() {
        assert_eq!(
            plan(RD_SPECIFICATION).unwrap().sql,
            "SELECT * FROM manufacturing_specifications WHERE id=? AND company_id=?"
        );
        assert_eq!(
            plan(RD_SPECIFICATION_VERSION).unwrap().sql,
            "SELECT * FROM manufacturing_specifications WHERE id=? AND company_id=? AND version=?"
        );
        assert_eq!(
            plan(RD_SPECIFICATION_LINES).unwrap().sql,
            "SELECT * FROM manufacturing_specification_lines WHERE specification_id=? ORDER BY ordinal"
        );
        assert_eq!(
            plan(RD_ORDERS).unwrap().sql,
            "SELECT o.*,s.name specification_name,p.name output_name FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id JOIN commerce_products p ON p.id=s.output_product_id WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.updated_at DESC"
        );
        assert_eq!(
            plan(RD_ORDER_DETAIL).unwrap().sql,
            "SELECT o.*,s.name specification_name,s.mode,p.name output_name,p.tracking_mode output_tracking_mode FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id JOIN commerce_products p ON p.id=s.output_product_id WHERE o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?)"
        );
        assert_eq!(
            plan(RD_MATERIAL_RESERVATIONS).unwrap().sql,
            "SELECT r.*,p.name product_name,p.tracking_mode FROM manufacturing_material_reservations r JOIN commerce_products p ON p.id=r.product_id WHERE r.order_id=? ORDER BY p.name"
        );
        assert_eq!(
            plan(RD_ORDER_EVENTS).unwrap().sql,
            "SELECT * FROM manufacturing_order_events WHERE order_id=? ORDER BY version"
        );
        assert_eq!(
            plan(RD_ORDER_POSTINGS).unwrap().sql,
            "SELECT p.*,j.status journal_status FROM manufacturing_order_postings p JOIN finance_journals j ON j.id=p.journal_id WHERE p.order_id=? ORDER BY p.source_order_version"
        );
        assert_eq!(
            plan(RD_ORDER_OPERATIONS).unwrap().sql,
            "SELECT op.*,r.name resource_name,e.labor_minutes,e.machine_minutes,e.evidence,e.created_at completed_at FROM manufacturing_specification_operations op JOIN production_resources r ON r.id=op.resource_id LEFT JOIN manufacturing_operation_events e ON e.operation_id=op.id AND e.order_id=? WHERE op.specification_id=? ORDER BY op.ordinal"
        );
        assert_eq!(
            plan(RD_LOT_GENEALOGY).unwrap().sql,
            "SELECT g.*,i.lot_id input_lot_id,il.lot_code input_lot_code,il.serial_number input_serial_number,ol.lot_id output_lot_id,l.lot_code output_lot_code,l.serial_number output_serial_number FROM manufacturing_lot_genealogy g JOIN manufacturing_material_lot_issues i ON i.id=g.input_issue_id JOIN inventory_lots il ON il.id=i.lot_id JOIN manufacturing_output_lots ol ON ol.id=g.output_lot_id JOIN inventory_lots l ON l.id=ol.lot_id WHERE g.order_id=? ORDER BY g.id"
        );
        assert_eq!(
            plan(RD_PRODUCT_GENEALOGY).unwrap().sql,
            "SELECT * FROM manufacturing_genealogy WHERE order_id=?"
        );
        assert_eq!(
            plan(RD_ACCOUNTING_PROFILES).unwrap().sql,
            "SELECT id,status,version,currency,chart_id,inventory_account_id,wip_account_id,labor_account_id,overhead_account_id FROM manufacturing_accounting_profiles WHERE company_id=? ORDER BY updated_at DESC"
        );
    }
}
