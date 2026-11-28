//! Owner-local native read ports for the Customer Authority portal SELECT tail
//! (`G4 / migrate.customer_authority.portal-read-port`).
//!
//! A plan carries the byte-exact SQL of a retained TypeScript read, its ordered
//! bind positions and the retained authority facts. This is a registry, not a
//! consumer cutover: no route is mounted, no connection is opened and no
//! TypeScript reader is changed here. The caller binds a plan on its own unit
//! and runs it there, so the live TypeScript read stays authoritative until G4
//! retires it. No composition layer is involved.
//!
//! Two retained readers are covered: the portal support response/thread chain
//! and the portal binding reads shared by portal access and portal attachments.
//! Dynamically shaped shells (`portalBindingSource`, the purchase-scoped
//! predicates, the attachment `EXISTS` assertion) are registered as fragments,
//! and the support-table `LOCK TABLE` is registered as a lock. None of those is
//! a bindable data-read plan.
use erpx_platform_shared_server::public::BusinessError;

/// Operation id a future native owner-read transport will advertise.
pub const OWNER_READ_OPERATION: &str = "customer_authority.portal-read-port.v1";

pub const CATEGORY_PORTAL_SUPPORT: &str = "customer_authority.portal-support";
pub const CATEGORY_PORTAL_BINDING: &str = "customer_authority.portal-binding";

// -- portal-support reads -----------------------------------------------------

pub const RD_SUPPORT_INSTALLED: &str = "portal-support.installed";
pub const RD_SUPPORT_LATEST_RESPONSE: &str = "portal-support.latest-response";
pub const RD_SUPPORT_REQUEST_SOURCE: &str = "portal-support.request-source";
pub const RD_SUPPORT_THREADS: &str = "portal-support.threads";
pub const RD_SUPPORT_REQUEST_ID: &str = "portal-support.request-id";
pub const RD_SUPPORT_STAFF_BINDING: &str = "portal-support.staff-binding";

// -- portal-binding reads -----------------------------------------------------

pub const RD_BINDING_LOCK: &str = "portal-binding.lock";
pub const RD_BINDING_IDENTITY_EXISTS: &str = "portal-binding.identity-exists";

/// Exact text the retained TypeScript interpolates for the commerce order
/// projection. Used to render `portal-binding.lock` byte-for-byte and to
/// re-bind the retained source during source-parity checks.
pub const TS_COMMERCE_ORDERS_TOKEN: &str = "${commerceProjectionSources.commerce_orders}";
pub const COMMERCE_ORDERS_SOURCE_SQL: &str = concat!(
    "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",",
    "\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",",
    "\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",",
    "\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",",
    "\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders"
);

/// Exact text the retained TypeScript interpolates into the support-thread
/// LATERAL subquery (`portalSupportResponsesSource`).
pub const TS_RESPONSE_SOURCE_TOKEN: &str = "${portalSupportResponsesSource}";
pub const RESPONSE_SOURCE_SQL: &str = "SELECT * FROM customer_portal_support_responses";

/// A bind position in a planned statement, in wire order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindField {
    BindingId,
    RequestId,
    /// Support-thread and identity binds. `None` is a deliberate SQL NULL for
    /// staff threads; a missing email is still a scope error.
    UserId,
    Email,
    CompanyId,
    /// Work-scope branch/department. `None` is a deliberate SQL NULL used by
    /// the `(?::text IS NULL OR ... = ?)` predicate.
    BranchId,
    DepartmentId,
}
impl BindField {
    /// Whether an absent scope value is a legitimate SQL NULL bind.
    pub fn nullable(self) -> bool {
        matches!(
            self,
            BindField::UserId | BindField::BranchId | BindField::DepartmentId
        )
    }
}

/// Scope facts a kind needs from its caller before its plan can be bound.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScopeRequirement {
    pub binding: bool,
    pub request: bool,
    pub user: bool,
    pub email: bool,
    pub company: bool,
    pub branch: bool,
    pub department: bool,
}
impl ScopeRequirement {
    const DEFAULT: Self = Self {
        binding: false,
        request: false,
        user: false,
        email: false,
        company: false,
        branch: false,
        department: false,
    };
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
    /// Source rows the retained TypeScript caller still locks/verifies. Kept
    /// as documentation of authority that a later cutover must not drop.
    pub authority: &'static str,
}

/// A dynamically shaped SELECT shell or cross-owner fragment. Not executable on
/// its own; registered so the retained literal stays owner-owned and auditable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadFragment {
    pub name: &'static str,
    pub sql: &'static str,
    pub source: &'static str,
}

/// A source-lease statement owned by the support command chain. Independent of
/// every data-read plan: never bound, never planned as a read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadLock {
    pub name: &'static str,
    pub sql: &'static str,
    pub source: &'static str,
}

// Verbatim statement literals from the authoritative TypeScript readers, with
// the two owner-owned source tokens rendered inline exactly as the callers do.
const SQL_SUPPORT_INSTALLED: &str = concat!(
    "SELECT to_regclass('customer_portal_support_responses') IS NOT NULL AND ",
    "to_regclass('customer_portal_support_acknowledgements') IS NOT NULL AND ",
    "to_regclass('customer_portal_support_closures') IS NOT NULL installed"
);
const SQL_SUPPORT_LATEST_RESPONSE: &str = "SELECT id,response_version,message,created_at,actor_id FROM customer_portal_support_responses WHERE request_id=? ORDER BY response_version DESC LIMIT 1";
const SQL_SUPPORT_REQUEST_SOURCE: &str = concat!(
    "SELECT request.*,binding.version binding_version,binding.created_by binding_prepared_by,",
    "binding.email,binding.status,binding.expires_at,original.branch_id,original.department_id,",
    "original.state,original.version order_version,jsonb_build_array(to_jsonb(request),",
    "to_jsonb(binding),to_jsonb(identity),to_jsonb(original))::text image ",
    "FROM customer_portal_actions request JOIN customer_access_bindings binding ON ",
    "binding.id=request.binding_id AND binding.company_id=request.company_id AND ",
    "binding.order_id=request.order_id AND binding.customer_id=request.customer_id ",
    "JOIN customer_access_identities identity ON identity.binding_id=binding.id AND ",
    "identity.user_id=request.user_id AND identity.email=binding.email ",
    "JOIN commerce_orders original ON original.id=request.order_id AND ",
    "original.company_id=request.company_id AND original.party_id=request.customer_id AND ",
    "original.kind IN ('quote','sales') WHERE request.id=? AND request.action='request_support'"
);
const SQL_SUPPORT_THREADS: &str = concat!(
    "SELECT request.id,request.created_at,request.note,response.id response_id,",
    "response.response_version,response.message,response.created_at published_at,",
    "ack.created_at acknowledged_at,closure.created_at closed_at ",
    "FROM customer_portal_actions request LEFT JOIN LATERAL(",
    "SELECT * FROM customer_portal_support_responses response WHERE response.request_id=request.id ",
    "ORDER BY response.response_version DESC LIMIT 1) response ON true ",
    "LEFT JOIN customer_portal_support_acknowledgements ack ON ack.request_id=request.id AND ",
    "ack.response_id=response.id LEFT JOIN customer_portal_support_closures closure ON ",
    "closure.request_id=request.id WHERE request.binding_id=? AND request.action='request_support' ",
    "AND (?::text IS NULL OR request.user_id=?) ORDER BY request.created_at,request.id"
);
const SQL_SUPPORT_REQUEST_ID: &str = "SELECT id FROM customer_portal_actions WHERE binding_id=? AND user_id=? AND action='request_support'";
const SQL_SUPPORT_STAFF_BINDING: &str = concat!(
    "SELECT binding.id,binding.created_by,binding.status,binding.expires_at ",
    "FROM customer_access_bindings binding JOIN commerce_orders original ON ",
    "original.id=binding.order_id AND original.company_id=binding.company_id AND ",
    "original.party_id=binding.customer_id AND original.kind IN ('quote','sales') ",
    "WHERE binding.id=? AND binding.company_id=? AND (?::text IS NULL OR original.branch_id=?) ",
    "AND (?::text IS NULL OR original.department_id=?)"
);

const SQL_BINDING_LOCK: &str = concat!(
    "SELECT b.id binding_id,b.company_id,b.order_id,b.customer_id,b.email,b.status,b.expires_at,",
    "b.version binding_version,o.version order_version,o.number,o.state ",
    "FROM customer_access_bindings b JOIN (",
    "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",",
    "\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",",
    "\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",",
    "\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",",
    "\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders",
    ") o ON o.id=b.order_id AND o.company_id=b.company_id ",
    "WHERE b.id=? AND lower(b.email)=? AND o.party_id=b.customer_id AND o.kind IN ('sales','quote') FOR UPDATE"
);
const SQL_BINDING_IDENTITY_EXISTS: &str =
    "SELECT 1 FROM customer_access_identities WHERE binding_id=? AND user_id=? AND email=?";

const BINDING: ScopeRequirement = ScopeRequirement {
    binding: true,
    ..ScopeRequirement::DEFAULT
};
const BINDING_USER: ScopeRequirement = ScopeRequirement {
    binding: true,
    user: true,
    ..ScopeRequirement::DEFAULT
};
const BINDING_EMAIL: ScopeRequirement = ScopeRequirement {
    binding: true,
    email: true,
    ..ScopeRequirement::DEFAULT
};
const BINDING_USER_EMAIL: ScopeRequirement = ScopeRequirement {
    binding: true,
    user: true,
    email: true,
    ..ScopeRequirement::DEFAULT
};
const REQUEST: ScopeRequirement = ScopeRequirement {
    request: true,
    ..ScopeRequirement::DEFAULT
};
const STAFF: ScopeRequirement = ScopeRequirement {
    binding: true,
    company: true,
    ..ScopeRequirement::DEFAULT
};

const SUPPORT_LOCKS: &str = "customer_portal_support_responses,customer_portal_support_acknowledgements,customer_portal_support_closures (SHARE ROW EXCLUSIVE)";
const BINDING_LOCKS: &str =
    "customer_access_bindings (FOR UPDATE), customer_access_identities (existence proof)";

/// The closed read registry, in stable wire order.
pub const PLANS: &[ReadPlan] = &[
    ReadPlan {
        kind: RD_SUPPORT_INSTALLED,
        category: CATEGORY_PORTAL_SUPPORT,
        caller: "portal-support-sources.portalSupportInstalled",
        sql: SQL_SUPPORT_INSTALLED,
        binds: &[],
        scope: ScopeRequirement::DEFAULT,
        authority: SUPPORT_LOCKS,
    },
    ReadPlan {
        kind: RD_SUPPORT_LATEST_RESPONSE,
        category: CATEGORY_PORTAL_SUPPORT,
        caller: "portal-support.response",
        sql: SQL_SUPPORT_LATEST_RESPONSE,
        binds: &[BindField::RequestId],
        scope: REQUEST,
        authority: SUPPORT_LOCKS,
    },
    ReadPlan {
        kind: RD_SUPPORT_REQUEST_SOURCE,
        category: CATEGORY_PORTAL_SUPPORT,
        caller: "portal-support.source",
        sql: SQL_SUPPORT_REQUEST_SOURCE,
        binds: &[BindField::RequestId],
        scope: REQUEST,
        authority: SUPPORT_LOCKS,
    },
    ReadPlan {
        kind: RD_SUPPORT_THREADS,
        category: CATEGORY_PORTAL_SUPPORT,
        caller: "portal-support.threads",
        sql: SQL_SUPPORT_THREADS,
        binds: &[BindField::BindingId, BindField::UserId, BindField::UserId],
        scope: BINDING,
        authority: SUPPORT_LOCKS,
    },
    ReadPlan {
        kind: RD_SUPPORT_REQUEST_ID,
        category: CATEGORY_PORTAL_SUPPORT,
        caller: "portal-support.customerThreads",
        sql: SQL_SUPPORT_REQUEST_ID,
        binds: &[BindField::BindingId, BindField::UserId],
        scope: BINDING_USER,
        authority: SUPPORT_LOCKS,
    },
    ReadPlan {
        kind: RD_SUPPORT_STAFF_BINDING,
        category: CATEGORY_PORTAL_SUPPORT,
        caller: "portal-support.portalSupportThreadsForStaff",
        sql: SQL_SUPPORT_STAFF_BINDING,
        binds: &[
            BindField::BindingId,
            BindField::CompanyId,
            BindField::BranchId,
            BindField::BranchId,
            BindField::DepartmentId,
            BindField::DepartmentId,
        ],
        scope: STAFF,
        authority: BINDING_LOCKS,
    },
    ReadPlan {
        kind: RD_BINDING_LOCK,
        category: CATEGORY_PORTAL_BINDING,
        caller: "portal-access.lockCustomerPortalBinding",
        sql: SQL_BINDING_LOCK,
        binds: &[BindField::BindingId, BindField::Email],
        scope: BINDING_EMAIL,
        authority: BINDING_LOCKS,
    },
    ReadPlan {
        kind: RD_BINDING_IDENTITY_EXISTS,
        category: CATEGORY_PORTAL_BINDING,
        caller: "portal-access.customerPortalIdentityExists",
        sql: SQL_BINDING_IDENTITY_EXISTS,
        binds: &[BindField::BindingId, BindField::UserId, BindField::Email],
        scope: BINDING_USER_EMAIL,
        authority: BINDING_LOCKS,
    },
];

// Retained dynamic shells and cross-owner fragments. Each is owner-owned text
// and source-bound; none is a bindable data-read plan.
const FRAG_SUPPORT_RESPONSE_SOURCE: &str = RESPONSE_SOURCE_SQL;
const FRAG_SUPPORT_ACK_SOURCE: &str = "SELECT * FROM customer_portal_support_acknowledgements";
const FRAG_SUPPORT_CLOSURE_SOURCE: &str = "SELECT * FROM customer_portal_support_closures";
const FRAG_BINDING_SOURCE: &str = concat!(
    "SELECT ${columns}b.company_id,o.branch_id,o.department_id,b.version ",
    "FROM customer_access_bindings b JOIN (${commerceProjectionSources.commerce_orders}) o ",
    "ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id"
);
const FRAG_BINDING_SCOPE: &str = concat!(
    "${portalBindingSource('b.id,')} ",
    "JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=? AND i.email=? ",
    "WHERE b.id=? AND b.email=? AND b.status='approved' AND b.expires_at>? AND ${purchase} ",
    "AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')"
);
const FRAG_BINDING_ATTACHMENT_FACTS: &str = concat!(
    "${portalBindingSource()} ",
    "${facts.external ? \"JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=? AND i.email=?\" : \"\"} ",
    "WHERE b.id=? AND b.company_id=? ",
    "${facts.external ? \"AND b.status='approved' AND b.expires_at>?\" : \"\"} ",
    "AND ${await customerPortalPurchaseScope(db,'b')} AND o.kind IN ('sales','quote') ",
    "${facts.external ? \"AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')\" : \"\"}"
);
const FRAG_BINDING_VERSION_GUARD: &str = concat!(
    "EXISTS(SELECT 1 FROM customer_access_bindings WHERE id=? AND company_id=? AND version=? ",
    "AND status='approved' AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)"
);

pub const FRAGMENTS: &[ReadFragment] = &[
    ReadFragment {
        name: "portal-support.response-source",
        sql: FRAG_SUPPORT_RESPONSE_SOURCE,
        source: "portal-support-sources.ts:supportSource('responses')",
    },
    ReadFragment {
        name: "portal-support.acknowledgement-source",
        sql: FRAG_SUPPORT_ACK_SOURCE,
        source: "portal-support-sources.ts:supportSource('acknowledgements')",
    },
    ReadFragment {
        name: "portal-support.closure-source",
        sql: FRAG_SUPPORT_CLOSURE_SOURCE,
        source: "portal-support-sources.ts:supportSource('closures')",
    },
    ReadFragment {
        name: "portal-binding.source",
        sql: FRAG_BINDING_SOURCE,
        source: "portal-binding-source.ts:portalBindingSource",
    },
    ReadFragment {
        name: "portal-binding.scope-shell",
        sql: FRAG_BINDING_SCOPE,
        source: "portal-access.ts:readCustomerPortalBindingScope",
    },
    ReadFragment {
        name: "portal-binding.attachment-facts-shell",
        sql: FRAG_BINDING_ATTACHMENT_FACTS,
        source: "attachments-portal-facts.ts:readPortalBinding",
    },
    ReadFragment {
        name: "portal-binding.version-guard",
        sql: FRAG_BINDING_VERSION_GUARD,
        source: "attachments-portal-facts.ts:versionGuard assertion",
    },
];

/// The support source lease stays independent of every data-read plan: it is
/// never bound, never planned as a read and never executed by the read port.
pub const LOCK_SUPPORT_SOURCES: &str = concat!(
    "LOCK TABLE customer_portal_support_responses,",
    "customer_portal_support_acknowledgements,",
    "customer_portal_support_closures IN SHARE ROW EXCLUSIVE MODE"
);
pub const LOCKS: &[ReadLock] = &[ReadLock {
    name: "portal-support.source-lease",
    sql: LOCK_SUPPORT_SOURCES,
    source: "portal-support-sources.ts:portalSupportSourceReadLocks",
}];

/// Look up a registered data-read plan by kind.
pub fn plan(kind: &str) -> Option<&'static ReadPlan> {
    PLANS.iter().find(|entry| entry.kind == kind)
}

/// Look up a retained dynamic fragment by name.
pub fn fragment(name: &str) -> Option<&'static ReadFragment> {
    FRAGMENTS.iter().find(|entry| entry.name == name)
}

/// Look up a retained source lease by name.
pub fn lock(name: &str) -> Option<&'static ReadLock> {
    LOCKS.iter().find(|entry| entry.name == name)
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
        "CUSTOMER_AUTHORITY_READ_PLAN_INVALID",
        "Review the Customer Authority portal read plan.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn closed_registry_of_caller_unit_reads() {
        assert_eq!(kinds().count(), 8);
        assert_eq!(
            kinds().collect::<BTreeSet<_>>().len(),
            8,
            "read kinds must be unique"
        );
        for kind in kinds() {
            assert!(plan(kind).is_some(), "{kind}");
            assert!(is_owner_read(kind), "{kind}");
        }
        assert!(plan("bogus.read").is_none());
        assert!(!is_owner_read("customer.portal.support.respond"));
    }

    #[test]
    fn both_retained_readers_are_covered() {
        for category in [CATEGORY_PORTAL_SUPPORT, CATEGORY_PORTAL_BINDING] {
            assert!(
                PLANS.iter().any(|entry| entry.category == category),
                "{category}"
            );
        }
    }

    #[test]
    fn plans_are_select_only_with_matching_binds() {
        const LOCKED: &[&str] = &[RD_BINDING_LOCK];
        for entry in PLANS {
            let sql = entry.sql;
            assert!(sql.starts_with("SELECT "), "{}", entry.kind);
            assert!(!sql.contains(';'), "{}", entry.kind);
            for banned in ["INSERT", "DELETE", "LOCK TABLE", "pg_advisory"] {
                assert!(!sql.contains(banned), "{} contains {banned}", entry.kind);
            }
            let locked = sql.contains("FOR UPDATE") || sql.contains("FOR SHARE");
            assert_eq!(
                locked,
                LOCKED.contains(&entry.kind),
                "{} lock set",
                entry.kind
            );
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
    fn nullable_binds_match_the_live_call_sites() {
        for (kind, field) in [
            (RD_SUPPORT_THREADS, BindField::UserId),
            (RD_SUPPORT_STAFF_BINDING, BindField::BranchId),
            (RD_SUPPORT_STAFF_BINDING, BindField::DepartmentId),
        ] {
            let plan = plan(kind).unwrap();
            assert!(plan.binds.contains(&field), "{kind} must bind {field:?}");
            assert!(field.nullable(), "{field:?}");
        }
        assert!(!BindField::BindingId.nullable());
        assert!(!BindField::Email.nullable());
    }

    #[test]
    fn fragments_and_locks_are_not_read_plans() {
        assert_eq!(fragments().count(), 7);
        for name in fragments() {
            let fragment = fragment(name).expect("fragment");
            assert!(!is_owner_read(name), "{name} must not be a read plan");
            assert!(
                fragment.sql.contains("SELECT") || fragment.sql.contains("${"),
                "{name}"
            );
            assert!(!fragment.source.is_empty(), "{name}");
        }
        assert_eq!(LOCKS.len(), 1);
        for entry in LOCKS {
            assert!(lock(entry.name).is_some());
            assert!(entry.sql.starts_with("LOCK TABLE"), "{}", entry.name);
            assert!(!is_owner_read(entry.name), "{}", entry.name);
        }
    }
}
