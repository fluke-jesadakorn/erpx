//! Owner-local native read plans for the Assets SELECT tail
//! (`G4 / migrate.assets.read-ports`).
//!
//! A plan carries the byte-exact SQL of a retained TypeScript read, its ordered
//! binds and the cross-owner fragments inlined by that read. This is a registry,
//! not a consumer cutover: no route is mounted, no connection is opened and no
//! TypeScript reader is changed here. The caller runs a bound plan on its own
//! transaction, so the live TypeScript read stays authoritative until G4
//! retires it. Field-service reference guards and package-obligation locks are
//! separate retained-authority concerns and are deliberately not recreated.
use erpx_platform_shared_server::public::BusinessError;

/// Operation id a future native owner-read transport will advertise.
pub const OWNER_READ_OPERATION: &str = "assets.owner-read.v1";

pub const RD_FS_ASSET_SOURCE_ROWS: &str = "field-service-asset-source-rows";
pub const RD_FS_ASSET_ATTACHMENT_IMAGE: &str = "field-service-asset-attachment-image";
pub const RD_PKG_ASSET_SOURCES: &str = "package-asset-sources";
pub const RD_PKG_LINKS: &str = "package-finance-links";
pub const RD_OBLIGATIONS_BASE: &str = "package-obligations-base";
pub const RD_OBLIGATIONS_PREVENTIVE: &str = "package-obligations-preventive";
pub const RD_OBLIGATIONS_PARTS: &str = "package-obligations-parts";
/// Cross-owner package clock scalar, inlined by the retired Assets clock CTE.
pub const RD_PACKAGE_CLOCK: &str = "package-clock-now-ms";

/// A bind position in a planned statement, in wire order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindField {
    AssetId,
    CompanyId,
    /// `packageId === 'assets'`; bound as a PG boolean literal.
    IsBase,
}

/// Scope facts a kind needs from its caller before its plan can be bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScopeRequirement {
    pub asset: bool,
    pub company: bool,
    pub is_base: bool,
}

/// One caller-unit read plan: exact SQL, ordered binds and the retained
/// authority facts that must survive a later cutover.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadPlan {
    pub kind: &'static str,
    pub caller: &'static str,
    pub sql: &'static str,
    pub binds: &'static [BindField],
    pub scope: ScopeRequirement,
    /// Cross-owner fragments inlined verbatim by the TypeScript authority.
    pub cross_owner: &'static [&'static str],
    /// Source locks the retained TypeScript caller still acquires.
    pub authority: &'static str,
}

// Verbatim source fragments from the cross-owner TypeScript adapters that the
// Assets obligation source inlines. Macros (rather than named consts) let
// `concat!` build byte-exact statements without a Rust dependency on another
// owner's crate.
macro_rules! placements {
    () => {
        "SELECT id,company_id,branch_id,kind,status FROM organization_nodes"
    };
}
macro_rules! financial {
    () => {
        "SELECT id,company_id,status,currency,cost_minor,residual_minor,accumulated_depreciation_minor,activation_journal_id,version FROM finance_assets"
    };
}
macro_rules! links {
    () => {
        "SELECT * FROM operational_asset_finance_links"
    };
}
macro_rules! consumptions {
    () => {
        "SELECT * FROM operational_inventory_consumptions WHERE source_kind='maintenance'"
    };
}
macro_rules! preventive_refs {
    () => {
        "SELECT company_id,asset_id FROM maintenance_preventive_plans"
    };
}
macro_rules! parts_refs {
    () => {
        concat!(
            "SELECT consumption.company_id,work.asset_id FROM (",
            consumptions!(),
            ") consumption LEFT JOIN maintenance_work_orders work ON work.id=consumption.source_id AND work.company_id=consumption.company_id"
        )
    };
}

/// Verbatim cross-owner clock expression from the Commercial read authority.
macro_rules! package_clock {
    () => {
        "floor(extract(epoch FROM clock_timestamp())*1000)::bigint"
    };
}
const PLAN_PACKAGE_CLOCK: &str =
    "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms";
const PLAN_FS_SOURCE_ROWS: &str =
    "SELECT id,company_id,branch_id,department_id,lifecycle_status,warranty_end,version FROM operational_assets";
const PLAN_FS_IMAGE: &str =
    "SELECT to_jsonb(source)::text image FROM operational_assets source WHERE source.id=? AND source.company_id=?";
macro_rules! asset_sources {
    () => {
        concat!(
            "SELECT asset.*,COALESCE(EXISTS(SELECT 1 FROM (",
            placements!(),
            ") branch WHERE branch.id=asset.branch_id AND branch.company_id=asset.company_id AND branch.kind IN ('branch','headquarters')) AND (asset.department_id IS NULL OR EXISTS(SELECT 1 FROM (",
            placements!(),
            ") department WHERE department.id=asset.department_id AND department.company_id=asset.company_id AND department.branch_id=asset.branch_id AND department.kind='department')),false) source_valid FROM operational_assets asset"
        )
    };
}
const PLAN_ASSET_SOURCES: &str = asset_sources!();

macro_rules! obligations {
    ($refs:expr) => {
        concat!(
            "WITH scope AS(SELECT ?::text company_id,?::boolean is_base),assets AS MATERIALIZED(SELECT asset.* FROM (",
            asset_sources!(),
            ") asset,scope WHERE asset.company_id=scope.company_id AND (scope.is_base OR EXISTS(SELECT 1 FROM (",
            $refs,
            ") source WHERE source.asset_id=asset.id AND source.company_id=asset.company_id)))\n SELECT 'asset' kind,asset.id,asset.version,asset.lifecycle_status state,NOT asset.source_valid OR (SELECT is_base FROM scope) AND (asset.lifecycle_status IN ('maintenance','retirement_pending') OR asset.cost_minor>0 AND NOT EXISTS(SELECT 1 FROM (",
            financial!(),
            ") financial JOIN (",
            links!(),
            ") link ON link.asset_id=asset.id AND link.finance_asset_id=financial.id WHERE financial.id=asset.finance_asset_id AND financial.company_id=asset.company_id AND financial.currency=asset.currency AND financial.cost_minor=asset.cost_minor AND link.cost_minor=asset.cost_minor AND link.currency=asset.currency AND link.asset_version<=asset.version AND link.finance_asset_version<=financial.version AND financial.activation_journal_id IS NOT NULL AND (asset.lifecycle_status='retired' AND financial.status='retired' OR asset.lifecycle_status<>'retired' AND financial.status='in_service'))) FROM assets asset\n UNION ALL SELECT 'asset-link',link.id,link.asset_version,'ambiguous-source',true FROM (",
            links!(),
            ") link JOIN assets asset ON asset.id=link.asset_id LEFT JOIN (",
            financial!(),
            ") financial ON financial.id=link.finance_asset_id AND financial.company_id=asset.company_id WHERE financial.id IS NULL OR link.currency<>asset.currency OR link.cost_minor<>asset.cost_minor OR financial.cost_minor<>link.cost_minor OR financial.currency<>link.currency OR link.asset_version>asset.version OR link.finance_asset_version>financial.version\n ORDER BY kind,id"
        )
    };
}

const PLAN_OBLIGATIONS_BASE: &str = obligations!(parts_refs!());
const PLAN_OBLIGATIONS_PREVENTIVE: &str = obligations!(preventive_refs!());
const PLAN_OBLIGATIONS_PARTS: &str = obligations!(parts_refs!());

const NO_SCOPE: ScopeRequirement = ScopeRequirement {
    asset: false,
    company: false,
    is_base: false,
};
const IMAGE_SCOPE: ScopeRequirement = ScopeRequirement {
    asset: true,
    company: true,
    is_base: false,
};
const OBLIGATION_SCOPE: ScopeRequirement = ScopeRequirement {
    asset: false,
    company: true,
    is_base: true,
};
const FS_CROSS: &[&str] = &[];
const PKG_CROSS_PREVENTIVE: &[&str] = &[
    placements!(),
    financial!(),
    links!(),
    preventive_refs!(),
];
const PKG_CROSS_PARTS: &[&str] = &[
    placements!(),
    financial!(),
    links!(),
    consumptions!(),
];
const FS_LOCKS: &str = "operational_assets";
const PKG_LOCKS: &str = "operational_assets,operational_asset_events,operational_asset_finance_links,organization_nodes,finance_assets,maintenance_work_orders,maintenance_preventive_plans,operational_inventory_consumptions";
const CLOCK_AUTHORITY: &str = "package-clock-no-locks";

/// Exact cross-owner clock scalar retired from the TypeScript clock CTE. The
/// caller embeds the returned statement as a materialized CTE body; no TS
/// statement token remains on the purchase/metric read paths.
pub const PLAN_PACKAGE_CLOCK_READ: ReadPlan = ReadPlan {
    kind: RD_PACKAGE_CLOCK,
    caller: "assets.package-clock",
    sql: PLAN_PACKAGE_CLOCK,
    binds: &[],
    scope: NO_SCOPE,
    cross_owner: &[package_clock!()],
    authority: CLOCK_AUTHORITY,
};

/// Exact live read of `fieldServiceAssetSourceRows`.
pub const PLAN_FS_SOURCE_ROWS_READ: ReadPlan = ReadPlan {
    kind: RD_FS_ASSET_SOURCE_ROWS,
    caller: "field_service.asset-source",
    sql: PLAN_FS_SOURCE_ROWS,
    binds: &[],
    scope: NO_SCOPE,
    cross_owner: FS_CROSS,
    authority: FS_LOCKS,
};
/// Exact live read of `fieldServiceAssetAttachmentSourceImage`.
pub const PLAN_FS_IMAGE_READ: ReadPlan = ReadPlan {
    kind: RD_FS_ASSET_ATTACHMENT_IMAGE,
    caller: "field_service.attachment-source",
    sql: PLAN_FS_IMAGE,
    binds: &[BindField::AssetId, BindField::CompanyId],
    scope: IMAGE_SCOPE,
    cross_owner: FS_CROSS,
    authority: FS_LOCKS,
};
pub const PLAN_ASSET_SOURCES_READ: ReadPlan = ReadPlan {
    kind: RD_PKG_ASSET_SOURCES,
    caller: "assets.package-obligations",
    sql: PLAN_ASSET_SOURCES,
    binds: &[],
    scope: NO_SCOPE,
    cross_owner: &[placements!()],
    authority: PKG_LOCKS,
};
pub const PLAN_LINKS_READ: ReadPlan = ReadPlan {
    kind: RD_PKG_LINKS,
    caller: "assets.package-obligations",
    sql: links!(),
    binds: &[],
    scope: NO_SCOPE,
    cross_owner: &[],
    authority: PKG_LOCKS,
};
pub const PLAN_OBLIGATIONS_BASE_READ: ReadPlan = ReadPlan {
    kind: RD_OBLIGATIONS_BASE,
    caller: "assets.package-obligations",
    sql: PLAN_OBLIGATIONS_BASE,
    binds: &[BindField::CompanyId, BindField::IsBase],
    scope: OBLIGATION_SCOPE,
    cross_owner: PKG_CROSS_PARTS,
    authority: PKG_LOCKS,
};
pub const PLAN_OBLIGATIONS_PREVENTIVE_READ: ReadPlan = ReadPlan {
    kind: RD_OBLIGATIONS_PREVENTIVE,
    caller: "assets.package-obligations",
    sql: PLAN_OBLIGATIONS_PREVENTIVE,
    binds: &[BindField::CompanyId, BindField::IsBase],
    scope: OBLIGATION_SCOPE,
    cross_owner: PKG_CROSS_PREVENTIVE,
    authority: PKG_LOCKS,
};
pub const PLAN_OBLIGATIONS_PARTS_READ: ReadPlan = ReadPlan {
    kind: RD_OBLIGATIONS_PARTS,
    caller: "assets.package-obligations",
    sql: PLAN_OBLIGATIONS_PARTS,
    binds: &[BindField::CompanyId, BindField::IsBase],
    scope: OBLIGATION_SCOPE,
    cross_owner: PKG_CROSS_PARTS,
    authority: PKG_LOCKS,
};

/// The closed registry, in stable wire order.
pub const PLANS: &[ReadPlan] = &[
    PLAN_PACKAGE_CLOCK_READ,
    PLAN_FS_SOURCE_ROWS_READ,
    PLAN_FS_IMAGE_READ,
    PLAN_ASSET_SOURCES_READ,
    PLAN_LINKS_READ,
    PLAN_OBLIGATIONS_BASE_READ,
    PLAN_OBLIGATIONS_PREVENTIVE_READ,
    PLAN_OBLIGATIONS_PARTS_READ,
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
        "ASSETS_READ_PLAN_INVALID",
        "Review the Assets owner read plan.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_registry_of_eight_caller_unit_reads() {
        assert_eq!(PLANS.len(), 8);
        assert_eq!(kinds().count(), 8);
        assert_eq!(
            kinds().collect::<std::collections::BTreeSet<_>>().len(),
            8,
            "read kinds must be unique"
        );
        for kind in kinds() {
            assert!(plan(kind).is_some(), "{kind}");
            assert!(is_owner_read(kind), "{kind}");
        }
        assert_eq!(plan("bogus.read"), None);
        assert!(!is_owner_read("asset-write"));
    }

    #[test]
    fn plans_are_select_only_and_bind_counts_match() {
        // The three obligation variants share one statement body.
        for entry in PLANS {
            let sql = entry.sql;
            assert!(sql.starts_with("SELECT ") || sql.starts_with("WITH "), "{}", entry.kind);
            assert!(!sql.contains(';'), "{}", entry.kind);
            for banned in ["INSERT", "UPDATE ", "DELETE", "FOR UPDATE", "503"] {
                assert!(!sql.contains(banned), "{} contains {banned}", entry.kind);
            }
            assert_eq!(
                sql.matches('?').count(),
                entry.binds.len(),
                "{} bind count",
                entry.kind
            );
            assert!(!entry.caller.is_empty(), "{}", entry.kind);
            assert!(!entry.authority.is_empty(), "{}", entry.kind);
            for fragment in entry.cross_owner {
                assert!(sql.contains(fragment), "{} missing {fragment}", entry.kind);
            }
        }
    }

    #[test]
    fn clock_plan_carries_the_cross_owner_scalar_without_binds() {
        let clock = plan(RD_PACKAGE_CLOCK).expect("clock plan");
        assert_eq!(
            clock.sql,
            concat!("SELECT ", package_clock!(), " now_ms")
        );
        assert!(clock.sql.contains(package_clock!()));
        assert!(clock.binds.is_empty());
        assert!(clock.cross_owner.contains(&package_clock!()));
    }

    #[test]
    fn obligation_variants_share_one_authority_body() {
        assert_ne!(PLAN_OBLIGATIONS_BASE, PLAN_OBLIGATIONS_PREVENTIVE);
        assert_eq!(PLAN_OBLIGATIONS_BASE, PLAN_OBLIGATIONS_PARTS);
    }
}
