//! Owner-local native read ports for the Commercial SELECT tail
//! (`G4 / migrate.commercial.read-ports`).
//!
//! A plan carries the byte-exact SQL of a retained TypeScript read, its ordered
//! bind positions and the retained authority facts. This is a registry, not a
//! consumer cutover: no route is mounted, no connection is opened and no
//! TypeScript reader is changed here. The caller runs a bound plan on its own
//! transaction, so the live TypeScript read stays authoritative until G4
//! retires it.
//!
//! Three retained readers are covered: package grants, the purchase command
//! boundary and the usage/seat projections. Dynamically shaped predicate and
//! observation shells, plus the advisory lock statements, are deliberately NOT
//! data-read plans: they are registered as `FRAGMENTS` and `ADVISORY_LOCKS`
//! respectively. The advisory-lock pair (company fence and holding sweep) stays
//! independent of every read plan and is never bound or executed as a read.
use erpx_server_shared::public::BusinessError;

/// Operation id a future native owner-read transport will advertise.
pub const OWNER_READ_OPERATION: &str = "commercial.read-ports.v1";

pub const CATEGORY_PACKAGE_GRANTS: &str = "commercial.package-grants";
pub const CATEGORY_PURCHASE_BOUNDARY: &str = "commercial.purchase-boundary";
pub const CATEGORY_PROJECTION: &str = "commercial.projection";

// -- package-grants reads -----------------------------------------------------

pub const RD_PURCHASE_FACTS: &str = "package-grants.purchase-facts";
pub const RD_HISTORY_RIGHTS: &str = "package-grants.history-rights";
pub const RD_CURRENT_GRANT: &str = "package-grants.current-grant";
pub const RD_GRANT_RECEIPT: &str = "package-grants.grant-receipt";
pub const RD_REVISION_ANCHOR: &str = "package-grants.revision-anchor";
pub const RD_CHANGE_SET_EXISTS: &str = "package-grants.change-set-exists";
pub const RD_CHANGE_SET_RECEIPT: &str = "package-grants.change-set-receipt";
pub const RD_COMPANY_GRANTS: &str = "package-grants.company-grants";
pub const RD_COMPANY_GRANTS_FOR_SHARE: &str = "package-grants.company-grants-for-share";

// -- purchase-command-boundary reads -----------------------------------------

pub const RD_PURCHASE_GRANT_IDS: &str = "purchase-boundary.company-grant-ids";
pub const RD_HOLDING_GRANT_PAIRS: &str = "purchase-boundary.holding-grant-pairs";

// -- projection reads ---------------------------------------------------------

pub const RD_PRINCIPAL_COVERAGE: &str = "projection.principal-coverage";
pub const RD_SNAPSHOT_HEADER_LOCK: &str = "projection.snapshot-header-lock";
pub const RD_SNAPSHOT_HEADERS: &str = "projection.snapshot-headers";
pub const RD_METER_ROWS: &str = "projection.meter-rows";
pub const RD_CURRENT_SNAPSHOTS: &str = "projection.current-snapshots";
pub const RD_HISTORY_MONTHS: &str = "projection.history-months";
pub const RD_SEAT_COVERAGE: &str = "projection.seat-coverage";
pub const RD_SEAT_ASSIGNMENT_EVENTS: &str = "projection.seat-assignment-events";
pub const RD_SEAT_MEMBER_EVENTS: &str = "projection.seat-member-events";
pub const RD_SEAT_ROLE_EVENTS: &str = "projection.seat-role-events";
pub const RD_SEAT_SNAPSHOT: &str = "projection.seat-snapshot";
pub const RD_SEAT_SUMMARY_ROWS: &str = "projection.seat-summary-rows";

/// A bind position in a planned statement, in wire order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindField {
    CompanyId,
    PackageId,
    EventId,
    Month,
    HoldingId,
    /// Upper time bound for retained event history, bound as a bigint literal.
    End,
}

/// Scope facts a kind needs from its caller before its plan can be bound.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScopeRequirement {
    pub company: bool,
    pub package: bool,
    pub event: bool,
    pub month: bool,
    pub holding: bool,
    pub end: bool,
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
    /// Source rows the retained TypeScript caller still locks. Kept as
    /// documentation of authority that a later cutover must not silently drop.
    pub authority: &'static str,
}

/// A dynamically shaped SELECT (predicate/observation shell, clock CTE,
/// variable-arity `IN (...)` or cross-owner fragment). Not executable on its
/// own; registered so the retained literal stays owner-owned and auditable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadFragment {
    pub name: &'static str,
    pub sql: &'static str,
    pub source: &'static str,
}

/// An advisory lock statement. Independent of every data-read plan: it is
/// never bound, never planned as a read and never executed by a read port.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdvisoryLock {
    pub name: &'static str,
    pub sql: &'static str,
    pub key: &'static str,
}

// Verbatim statement literals from the three authoritative TypeScript readers.
const SQL_PURCHASE_FACTS: &str = concat!(
    "SELECT COALESCE((SELECT jsonb_agg(to_jsonb(g) ORDER BY package_id) FROM company_package_grants g WHERE company_id=?),'[]'::jsonb) grants, ",
    "COALESCE((SELECT jsonb_agg(to_jsonb(h) ORDER BY package_id,package_version) FROM company_package_history_rights h WHERE company_id=?),'[]'::jsonb) history"
);
const SQL_HISTORY_RIGHTS: &str =
    "SELECT * FROM company_package_history_rights WHERE company_id=? ORDER BY package_id,package_version";
const SQL_CURRENT_GRANT: &str =
    "SELECT * FROM company_package_grants WHERE company_id=? AND package_id=?";
const SQL_GRANT_RECEIPT: &str =
    "SELECT request_hash,after_json FROM company_package_grant_events WHERE id=?";
const SQL_REVISION_ANCHOR: &str =
    "SELECT version FROM company_package_revisions WHERE company_id=? FOR UPDATE";
const SQL_CHANGE_SET_EXISTS: &str = "SELECT 1 FROM company_package_change_sets WHERE id=?";
const SQL_CHANGE_SET_RECEIPT: &str =
    "SELECT request_hash,result_json FROM company_package_change_sets WHERE id=?";
const SQL_COMPANY_GRANTS: &str =
    "SELECT * FROM company_package_grants WHERE company_id=? ORDER BY package_id";
const SQL_COMPANY_GRANTS_SHARED: &str =
    "SELECT * FROM company_package_grants WHERE company_id=? ORDER BY package_id FOR SHARE";

const SQL_PURCHASE_GRANT_IDS: &str =
    "SELECT package_id FROM company_package_grants WHERE company_id=? ORDER BY package_id FOR SHARE";
const SQL_HOLDING_GRANT_PAIRS: &str =
    "SELECT company_id,package_id FROM company_package_grants ORDER BY company_id,package_id FOR SHARE";

const SQL_PRINCIPAL_COVERAGE: &str =
    "SELECT since_at FROM billing_principal_status_coverage WHERE singleton=TRUE";
const SQL_SNAPSHOT_HEADER_LOCK: &str =
    "SELECT refreshed_at FROM billing_usage_snapshots WHERE holding_id=? AND month=? FOR UPDATE";
const SQL_SNAPSHOT_HEADERS: &str = concat!(
    "SELECT holding_id,ai_coverage,storage_coverage,seat_coverage,seat_source_count,unreconciled_originals, ",
    "active_seat_count,active_seat_observed_at FROM billing_usage_snapshots WHERE month=?"
);
const SQL_METER_ROWS: &str = concat!(
    "SELECT holding_id,month,source_key,company_id,company_name,branch_id,branch_name, ",
    "meter_kind,funding_source,pricing_status,quantity,input_tokens,output_tokens, ",
    "average_daily_bytes,active_at_period_end,request_status,gateway_status,provider,model, ",
    "units_json::text units_json,cost_status,verified_cost_satang,cost_evidence_key, ",
    "cost_reconciled_at,active_from,active_until,byte_milliseconds::text byte_milliseconds",
    " FROM billing_usage_meters WHERE month=?"
);
const SQL_CURRENT_SNAPSHOTS: &str =
    "SELECT holding_id,active_seat_count,active_seat_observed_at FROM billing_usage_snapshots WHERE month=?";
const SQL_HISTORY_MONTHS: &str = "SELECT month FROM billing_usage_snapshots WHERE holding_id=?";
const SQL_SEAT_COVERAGE: &str = "SELECT started_at FROM billing_seat_history_coverage WHERE singleton=1";
const SQL_SEAT_ASSIGNMENT_EVENTS: &str = concat!(
    "SELECT event_id,assignment_id,captured_at,present,state::text state_json",
    " FROM billing_seat_assignment_events WHERE captured_at<?",
    " ORDER BY assignment_id,captured_at,event_id"
);
const SQL_SEAT_MEMBER_EVENTS: &str = concat!(
    "SELECT event_id,company_id,user_id,captured_at,present,state::text state_json",
    " FROM billing_seat_member_events WHERE captured_at<?",
    " ORDER BY company_id,user_id,captured_at,event_id"
);
const SQL_SEAT_ROLE_EVENTS: &str = concat!(
    "SELECT event_id,company_id,role_id,captured_at,present,state::text state_json",
    " FROM billing_seat_role_events WHERE captured_at<?",
    " ORDER BY company_id,role_id,captured_at,event_id"
);
const SQL_SEAT_SNAPSHOT: &str =
    "SELECT snapshot_id,seat_coverage,seat_source_count FROM billing_usage_snapshots WHERE holding_id=? AND month=?";
const SQL_SEAT_SUMMARY_ROWS: &str =
    "SELECT intervals_json::text intervals_json,snapshot_id FROM billing_seat_usage WHERE holding_id=? AND month=?";

const COMPANY: ScopeRequirement = ScopeRequirement {
    company: true,
    ..ScopeRequirement::DEFAULT
};
const COMPANY_PACKAGE: ScopeRequirement = ScopeRequirement {
    company: true,
    package: true,
    ..ScopeRequirement::DEFAULT
};
const EVENT: ScopeRequirement = ScopeRequirement {
    event: true,
    ..ScopeRequirement::DEFAULT
};
const HOLDING_MONTH: ScopeRequirement = ScopeRequirement {
    holding: true,
    month: true,
    ..ScopeRequirement::DEFAULT
};
const MONTH: ScopeRequirement = ScopeRequirement {
    month: true,
    ..ScopeRequirement::DEFAULT
};
const HOLDING: ScopeRequirement = ScopeRequirement {
    holding: true,
    ..ScopeRequirement::DEFAULT
};
const END: ScopeRequirement = ScopeRequirement {
    end: true,
    ..ScopeRequirement::DEFAULT
};

impl ScopeRequirement {
    const DEFAULT: Self = Self {
        company: false,
        package: false,
        event: false,
        month: false,
        holding: false,
        end: false,
    };
}

const PACKAGE_LOCKS: &str =
    "company_package_grants (ROW EXCLUSIVE / advisory Company fence), company_package_revisions (FOR UPDATE on revision anchor)";
const BOUNDARY_LOCKS: &str =
    "company_package_grants,company_package_history_rights (ROW EXCLUSIVE / advisory before aggregate and IAM locks)";
const PROJECTION_LOCKS: &str =
    "billing_usage_snapshots (FOR UPDATE on header), caller-owned control transaction";

/// The closed read registry, in stable wire order.
pub const PLANS: &[ReadPlan] = &[
    // package-grants
    ReadPlan {
        kind: RD_PURCHASE_FACTS,
        category: CATEGORY_PACKAGE_GRANTS,
        caller: "package-grants.readCompanyPurchaseFacts",
        sql: SQL_PURCHASE_FACTS,
        binds: &[BindField::CompanyId, BindField::CompanyId],
        scope: COMPANY,
        authority: PACKAGE_LOCKS,
    },
    ReadPlan {
        kind: RD_HISTORY_RIGHTS,
        category: CATEGORY_PACKAGE_GRANTS,
        caller: "package-grants.readCompanyPackageHistoryRights",
        sql: SQL_HISTORY_RIGHTS,
        binds: &[BindField::CompanyId],
        scope: COMPANY,
        authority: PACKAGE_LOCKS,
    },
    ReadPlan {
        kind: RD_CURRENT_GRANT,
        category: CATEGORY_PACKAGE_GRANTS,
        caller: "package-grants.packageGrantStore.current",
        sql: SQL_CURRENT_GRANT,
        binds: &[BindField::CompanyId, BindField::PackageId],
        scope: COMPANY_PACKAGE,
        authority: PACKAGE_LOCKS,
    },
    ReadPlan {
        kind: RD_GRANT_RECEIPT,
        category: CATEGORY_PACKAGE_GRANTS,
        caller: "package-grants.packageGrantStore.receipt",
        sql: SQL_GRANT_RECEIPT,
        binds: &[BindField::EventId],
        scope: EVENT,
        authority: PACKAGE_LOCKS,
    },
    ReadPlan {
        kind: RD_REVISION_ANCHOR,
        category: CATEGORY_PACKAGE_GRANTS,
        caller: "package-grants.batchUnit.all (locking)",
        sql: SQL_REVISION_ANCHOR,
        binds: &[BindField::CompanyId],
        scope: COMPANY,
        authority: PACKAGE_LOCKS,
    },
    ReadPlan {
        kind: RD_CHANGE_SET_EXISTS,
        category: CATEGORY_PACKAGE_GRANTS,
        caller: "commercial-read-tail-r5-sources.receiptProbe",
        sql: SQL_CHANGE_SET_EXISTS,
        binds: &[BindField::EventId],
        scope: EVENT,
        authority: PACKAGE_LOCKS,
    },
    ReadPlan {
        kind: RD_CHANGE_SET_RECEIPT,
        category: CATEGORY_PACKAGE_GRANTS,
        caller: "package-grants.batchUnit.receipt",
        sql: SQL_CHANGE_SET_RECEIPT,
        binds: &[BindField::EventId],
        scope: EVENT,
        authority: PACKAGE_LOCKS,
    },
    ReadPlan {
        kind: RD_COMPANY_GRANTS,
        category: CATEGORY_PACKAGE_GRANTS,
        caller: "package-grants.readCompanyPackageGrants",
        sql: SQL_COMPANY_GRANTS,
        binds: &[BindField::CompanyId],
        scope: COMPANY,
        authority: PACKAGE_LOCKS,
    },
    ReadPlan {
        kind: RD_COMPANY_GRANTS_FOR_SHARE,
        category: CATEGORY_PACKAGE_GRANTS,
        caller: "package-grants.readCompanyPackageGrants (forShare)",
        sql: SQL_COMPANY_GRANTS_SHARED,
        binds: &[BindField::CompanyId],
        scope: COMPANY,
        authority: PACKAGE_LOCKS,
    },
    // purchase-command-boundary
    ReadPlan {
        kind: RD_PURCHASE_GRANT_IDS,
        category: CATEGORY_PURCHASE_BOUNDARY,
        caller: "purchase-command-boundary.companyPurchaseReadLocks",
        sql: SQL_PURCHASE_GRANT_IDS,
        binds: &[BindField::CompanyId],
        scope: COMPANY,
        authority: BOUNDARY_LOCKS,
    },
    ReadPlan {
        kind: RD_HOLDING_GRANT_PAIRS,
        category: CATEGORY_PURCHASE_BOUNDARY,
        caller: "purchase-command-boundary.holdingPurchaseReadLocks",
        sql: SQL_HOLDING_GRANT_PAIRS,
        binds: &[],
        scope: ScopeRequirement::DEFAULT,
        authority: BOUNDARY_LOCKS,
    },
    // projection
    ReadPlan {
        kind: RD_PRINCIPAL_COVERAGE,
        category: CATEGORY_PROJECTION,
        caller: "projection.transaction.principalCoverage",
        sql: SQL_PRINCIPAL_COVERAGE,
        binds: &[],
        scope: ScopeRequirement::DEFAULT,
        authority: PROJECTION_LOCKS,
    },
    ReadPlan {
        kind: RD_SNAPSHOT_HEADER_LOCK,
        category: CATEGORY_PROJECTION,
        caller: "projection.transaction.lockHeader",
        sql: SQL_SNAPSHOT_HEADER_LOCK,
        binds: &[BindField::HoldingId, BindField::Month],
        scope: HOLDING_MONTH,
        authority: PROJECTION_LOCKS,
    },
    ReadPlan {
        kind: RD_SNAPSHOT_HEADERS,
        category: CATEGORY_PROJECTION,
        caller: "projection.repository.projectionFacts",
        sql: SQL_SNAPSHOT_HEADERS,
        binds: &[BindField::Month],
        scope: MONTH,
        authority: PROJECTION_LOCKS,
    },
    ReadPlan {
        kind: RD_METER_ROWS,
        category: CATEGORY_PROJECTION,
        caller: "projection.repository.projectionFacts",
        sql: SQL_METER_ROWS,
        binds: &[BindField::Month],
        scope: MONTH,
        authority: PROJECTION_LOCKS,
    },
    ReadPlan {
        kind: RD_CURRENT_SNAPSHOTS,
        category: CATEGORY_PROJECTION,
        caller: "projection.repository.projectionFacts",
        sql: SQL_CURRENT_SNAPSHOTS,
        binds: &[BindField::Month],
        scope: MONTH,
        authority: PROJECTION_LOCKS,
    },
    ReadPlan {
        kind: RD_HISTORY_MONTHS,
        category: CATEGORY_PROJECTION,
        caller: "projection.repository.history",
        sql: SQL_HISTORY_MONTHS,
        binds: &[BindField::HoldingId],
        scope: HOLDING,
        authority: PROJECTION_LOCKS,
    },
    ReadPlan {
        kind: RD_SEAT_COVERAGE,
        category: CATEGORY_PROJECTION,
        caller: "seats.repository.history",
        sql: SQL_SEAT_COVERAGE,
        binds: &[],
        scope: ScopeRequirement::DEFAULT,
        authority: PROJECTION_LOCKS,
    },
    ReadPlan {
        kind: RD_SEAT_ASSIGNMENT_EVENTS,
        category: CATEGORY_PROJECTION,
        caller: "seats.repository.history",
        sql: SQL_SEAT_ASSIGNMENT_EVENTS,
        binds: &[BindField::End],
        scope: END,
        authority: PROJECTION_LOCKS,
    },
    ReadPlan {
        kind: RD_SEAT_MEMBER_EVENTS,
        category: CATEGORY_PROJECTION,
        caller: "seats.repository.history",
        sql: SQL_SEAT_MEMBER_EVENTS,
        binds: &[BindField::End],
        scope: END,
        authority: PROJECTION_LOCKS,
    },
    ReadPlan {
        kind: RD_SEAT_ROLE_EVENTS,
        category: CATEGORY_PROJECTION,
        caller: "seats.repository.history",
        sql: SQL_SEAT_ROLE_EVENTS,
        binds: &[BindField::End],
        scope: END,
        authority: PROJECTION_LOCKS,
    },
    ReadPlan {
        kind: RD_SEAT_SNAPSHOT,
        category: CATEGORY_PROJECTION,
        caller: "seats.repository.snapshot",
        sql: SQL_SEAT_SNAPSHOT,
        binds: &[BindField::HoldingId, BindField::Month],
        scope: HOLDING_MONTH,
        authority: PROJECTION_LOCKS,
    },
    ReadPlan {
        kind: RD_SEAT_SUMMARY_ROWS,
        category: CATEGORY_PROJECTION,
        caller: "seats.repository.summaryRows",
        sql: SQL_SEAT_SUMMARY_ROWS,
        binds: &[BindField::HoldingId, BindField::Month],
        scope: HOLDING_MONTH,
        authority: PROJECTION_LOCKS,
    },
];

// Dynamically shaped shells. The predicate is generated from the versioned
// catalogue by `crate::adapters::purchased_capability_sql`; these literals are
// the retained TypeScript templates that surround it. They are not read plans.
// The four `EXISTS(SELECT 1 ...)` predicate arms (purchase/parent for history
// and for the active/draining purposes) are owned by that native predicate
// builder; the shared history-source literal is registered here as a fragment.
const FRAG_PURCHASE_SCOPE_CLOCK: &str =
    "WITH purchase_scope AS MATERIALIZED (SELECT ?::text company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint now) ";
const FRAG_ASSERTION_SELECT: &str =
    "SELECT CASE WHEN ${predicate} THEN 1 ELSE 0 END ok FROM purchase_scope";
const FRAG_OBSERVATION_SELECT: &str = concat!(
    "SELECT true commercial_purchase_observation,purchase_scope.company_id,?::text capability,?::text purpose,",
    "${predicate} allowed,purchase_scope.now observed_at,"
);
const FRAG_PURCHASE_GRANT_FACTS: &str = concat!(
    "COALESCE((SELECT jsonb_agg(jsonb_build_object('companyId',g.company_id,'packageId',g.package_id,",
    "'packageVersion',g.package_version,'state',g.state,'startsAt',g.starts_at,'endsAt',g.ends_at,",
    "'version',g.version)) FROM company_package_grants g WHERE g.company_id=purchase_scope.company_id),'[]'::jsonb) grants"
);
const FRAG_PURCHASE_HISTORY_FACTS: &str = concat!(
    "COALESCE((SELECT jsonb_agg(jsonb_build_object('companyId',h.company_id,'packageId',h.package_id,",
    "'packageVersion',h.package_version,'sourceGrantVersion',h.source_grant_version,",
    "'sourceEventId',h.source_event_id,'retainedAt',h.retained_at))",
    " FROM company_package_history_rights h WHERE h.company_id=purchase_scope.company_id),'[]'::jsonb)"
);
const FRAG_PURCHASE_CAPABILITY_SOURCES: &str = concat!(
    "SELECT package_id,package_version FROM company_package_grants WHERE company_id=${company} AND starts_at<=${now} AND state<>'inactive'",
    " UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=${company} AND retained_at<=${now}",
    " AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>''"
);
const FRAG_PRINCIPAL_EVENTS: &str = concat!(
    "SELECT id,user_id,status,effective_at FROM billing_principal_status_events",
    " WHERE effective_at<? AND user_id IN (${billablePlaceholders})",
    " ORDER BY user_id,effective_at,id"
);

const FRAGMENTS: &[ReadFragment] = &[
    ReadFragment {
        name: "purchase-boundary.scope-clock",
        sql: FRAG_PURCHASE_SCOPE_CLOCK,
        source: "purchase-command-boundary.ts:packageClockSql CTE",
    },
    ReadFragment {
        name: "purchase-boundary.assertion-select",
        sql: FRAG_ASSERTION_SELECT,
        source: "purchase-command-boundary.ts:guard assertion",
    },
    ReadFragment {
        name: "purchase-boundary.observation-select",
        sql: FRAG_OBSERVATION_SELECT,
        source: "purchase-command-boundary.ts:shadow observation",
    },
    ReadFragment {
        name: "purchase-boundary.grant-facts",
        sql: FRAG_PURCHASE_GRANT_FACTS,
        source: "purchase-command-boundary.ts:grants jsonb_agg",
    },
    ReadFragment {
        name: "purchase-boundary.history-facts",
        sql: FRAG_PURCHASE_HISTORY_FACTS,
        source: "purchase-command-boundary.ts:historyObservationSql",
    },
    ReadFragment {
        name: "purchase-boundary.capability-sources",
        sql: FRAG_PURCHASE_CAPABILITY_SOURCES,
        source: "purchased-capability-sql.ts:history sources",
    },
    ReadFragment {
        name: "projection.principal-events",
        sql: FRAG_PRINCIPAL_EVENTS,
        source: "projection.ts:principalEvents (variable-arity IN list)",
    },
];

/// The advisory-lock statements are kept independent of read plans on purpose.
/// The purchase-boundary pair (`company-fence` + `holding-sweep`) is acquired
/// before aggregate/IAM locks in the caller's transaction; it is not a data
/// read, takes no read port and is never bound or executed as one.
pub const ADVISORY_COMPANY_FENCE: &str = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))";
pub const ADVISORY_HOLDING_SWEEP: &str = concat!(
    "SELECT pg_advisory_xact_lock(hashtextextended('commercial.package:company:'||company_id,0))",
    " FROM (SELECT DISTINCT company_id FROM company_package_grants ORDER BY company_id) companies"
);
pub const ADVISORY_PACKAGE_KEY: &str = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))";

pub const ADVISORY_LOCKS: &[AdvisoryLock] = &[
    AdvisoryLock {
        name: "purchase-boundary.company-fence",
        sql: ADVISORY_COMPANY_FENCE,
        key: "commercial.package:company:${companyId}",
    },
    AdvisoryLock {
        name: "purchase-boundary.holding-sweep",
        sql: ADVISORY_HOLDING_SWEEP,
        key: "commercial.package:company:<each distinct company_id>",
    },
    AdvisoryLock {
        name: "package-grants.package-key",
        sql: ADVISORY_PACKAGE_KEY,
        key: "commercial.package:${key}",
    },
];

/// Look up a registered data-read plan by kind.
pub fn plan(kind: &str) -> Option<&'static ReadPlan> {
    PLANS.iter().find(|entry| entry.kind == kind)
}

/// Look up a retained dynamic fragment by name.
pub fn fragment(name: &str) -> Option<&'static ReadFragment> {
    FRAGMENTS.iter().find(|entry| entry.name == name)
}

pub fn is_owner_read(kind: &str) -> bool {
    plan(kind).is_some()
}

pub fn kinds() -> impl Iterator<Item = &'static str> {
    PLANS.iter().map(|entry| entry.kind)
}

pub fn fragments() -> impl Iterator<Item = &'static str> {
    FRAGMENTS.iter().map(|entry| entry.name)
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCIAL_READ_PLAN_INVALID",
        "Review the Commercial owner read plan.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_registry_of_caller_unit_reads() {
        assert_eq!(kinds().count(), 23);
        assert_eq!(
            kinds().collect::<std::collections::BTreeSet<_>>().len(),
            23,
            "read kinds must be unique"
        );
        for kind in kinds() {
            assert!(plan(kind).is_some(), "{kind}");
            assert!(is_owner_read(kind), "{kind}");
        }
        assert_eq!(plan("bogus.read"), None);
        assert!(!is_owner_read("commercial.owner-write.v1"));
    }

    #[test]
    fn all_three_retained_readers_are_covered() {
        for category in [
            CATEGORY_PACKAGE_GRANTS,
            CATEGORY_PURCHASE_BOUNDARY,
            CATEGORY_PROJECTION,
        ] {
            assert!(
                PLANS.iter().any(|entry| entry.category == category),
                "{category}"
            );
        }
    }

    #[test]
    fn plans_are_select_only_with_matching_binds() {
        const LOCKED: &[&str] = &[
            RD_REVISION_ANCHOR,
            RD_COMPANY_GRANTS_FOR_SHARE,
            RD_PURCHASE_GRANT_IDS,
            RD_HOLDING_GRANT_PAIRS,
            RD_SNAPSHOT_HEADER_LOCK,
        ];
        for entry in PLANS {
            let sql = entry.sql;
            assert!(sql.starts_with("SELECT ") || sql.starts_with("WITH "), "{}", entry.kind);
            assert!(!sql.contains(';'), "{}", entry.kind);
            for banned in ["INSERT", "DELETE", "LOCK TABLE", "pg_advisory"] {
                assert!(!sql.contains(banned), "{} contains {banned}", entry.kind);
            }
            let locked = sql.contains("FOR UPDATE") || sql.contains("FOR SHARE");
            assert_eq!(locked, LOCKED.contains(&entry.kind), "{} lock set", entry.kind);
            assert_eq!(
                sql.matches('?').count(),
                entry.binds.len(),
                "{} bind count",
                entry.kind
            );
            assert!(!entry.caller.is_empty(), "{}", entry.kind);
            assert!(!entry.authority.is_empty(), "{}", entry.kind);
        }
    }

    #[test]
    fn fragments_are_dynamic_and_not_read_plans() {
        assert_eq!(fragments().count(), 7);
        for name in fragments() {
            let fragment = fragment(name).expect("fragment");
            assert!(!is_owner_read(name), "{name} must not be a read plan");
            assert!(fragment.sql.contains("SELECT"), "{name}");
            assert!(!fragment.source.is_empty(), "{name}");
        }
    }

    #[test]
    fn advisory_locks_stay_independent_of_read_plans() {
        assert_eq!(ADVISORY_LOCKS.len(), 3);
        for lock in ADVISORY_LOCKS {
            assert!(lock.sql.contains("pg_advisory"), "{}", lock.name);
            assert!(plan(lock.name).is_none(), "{} is not a data read", lock.name);
        }
        // The required purchase-boundary pair is independently registered.
        assert_eq!(ADVISORY_LOCKS[0].name, "purchase-boundary.company-fence");
        assert_eq!(ADVISORY_LOCKS[1].name, "purchase-boundary.holding-sweep");
    }
}
