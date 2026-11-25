//! Native Customer 360 read-plan registry.
//!
//! Closed read kinds copied byte-for-byte from the TypeScript reads they
//! replace. A caller asks the mounted owner route for a plan and executes the
//! returned statement on its own already-open unit; no TypeScript SQL mirror
//! remains. The two kinds here retire the owner-local customer portal action
//! reads from `portal.ts`. Locks, cross-owner projections and the released
//! timeline/directory/activity ports are unchanged.
use erpx_server_shared::public::BusinessError;

/// Allow-listed native Customer 360 read kind: persisted portal action by request key.
pub const RD_PORTAL_ACTION_BY_KEY: &str = "c360_rd_portal_action_by_key";
/// Allow-listed native Customer 360 read kind: persisted portal action by binding and action.
pub const RD_PORTAL_ACTION_BY_BINDING: &str = "c360_rd_portal_action_by_binding";
/// Allow-listed native Customer 360 read kind: audit/outbox existence guard in the portal package facts.
pub const RD_OBLIGATION_AUDIT_EXISTS: &str = "c360_rd_obligation_audit_exists";
/// Allow-listed native Customer 360 read kind: action UNION branch head of the obligation facts.
pub const RD_OBLIGATION_ACTION_HEAD: &str = "c360_rd_obligation_action_head";
/// Allow-listed native Customer 360 read kind: acknowledgement UNION branch head of the obligation facts.
pub const RD_OBLIGATION_ACK_HEAD: &str = "c360_rd_obligation_ack_head";
/// Allow-listed native Customer 360 read kind: one-value Company scope CTE source.
pub const RD_COMPANY_SCOPE: &str = "c360_rd_company_scope";
/// Allow-listed native Customer 360 read kind: one-value Company scope source aliased for the portal release probe.
pub const RD_COMPANY_SCOPE_PORTAL_RELEASE: &str = "c360_rd_company_scope_portal_release";

/// Exact zero-bind Company scope source retired from `customer-read-sources.ts`.
const SQL_COMPANY_SCOPE: &str = "(SELECT ?::text company_id)";
/// Exact zero-bind aliased Company scope source retired from `customer-read-sources.ts`.
const SQL_COMPANY_SCOPE_PORTAL_RELEASE: &str = "(SELECT ?::text company_id) portal_release";

/// Exact zero-bind action branch head retired from `obligation-fact-head.ts`.
const SQL_OBLIGATION_ACTION_HEAD: &str = "SELECT 'action' kind,request.id,to_jsonb(request) fact,NOT ";
/// Exact zero-bind acknowledgement branch head retired from `obligation-fact-head.ts`.
const SQL_OBLIGATION_ACK_HEAD: &str = "SELECT 'acknowledgement',ack.id,to_jsonb(ack),NOT ";

/// Exact zero-bind guard retired from the customer_360 obligation facts CTE.
const SQL_OBLIGATION_AUDIT_EXISTS: &str = "EXISTS(SELECT 1 FROM audit_events audit JOIN outbox_events event ON event.command_id=audit.command_id AND event.event_type=audit.action AND event.aggregate_id=request.order_id AND event.aggregate_version=request.source_version WHERE audit.command_id=request.request_key AND audit.actor_id=request.user_id AND audit.assignment_id='external-customer' AND audit.action='customer.portal.'||request.action AND audit.record_id=request.order_id AND audit.record_version=request.source_version AND audit.authority_json::jsonb->>'bindingId'=request.binding_id AND event.payload_json::jsonb->>'bindingId'=request.binding_id AND event.payload_json::jsonb->>'customerId'=request.customer_id AND event.payload_json::jsonb->>'userId'=request.user_id AND event.payload_json::jsonb->>'note'=request.note)";

/// Exact one-bind statement retired from `portalRepository.replay`.
const SQL_PORTAL_ACTION_BY_KEY: &str = "SELECT id,binding_id,order_id,action,status,created_at,request_hash,user_id FROM customer_portal_actions WHERE request_key=?";
/// Exact two-bind statement retired from `portalRepository.existing`.
const SQL_PORTAL_ACTION_BY_BINDING: &str = "SELECT id,binding_id,order_id,action,status,created_at,request_hash,user_id FROM customer_portal_actions WHERE binding_id=? AND action=?";

/// One planned read: the exact retired statement and its positional bind count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_360_NATIVE_READ_INVALID",
        "Use one registered Customer 360 native read plan.",
    )
}

/// Resolve a registered kind. Unknown kinds fail closed; no caller can supply SQL.
pub fn plan_for(kind: &str, binds: usize) -> Result<NativeReadStatement, BusinessError> {
    let planned = match kind {
        RD_PORTAL_ACTION_BY_KEY => NativeReadStatement {
            kind: RD_PORTAL_ACTION_BY_KEY,
            sql: SQL_PORTAL_ACTION_BY_KEY,
            bind_count: 1,
        },
        RD_PORTAL_ACTION_BY_BINDING => NativeReadStatement {
            kind: RD_PORTAL_ACTION_BY_BINDING,
            sql: SQL_PORTAL_ACTION_BY_BINDING,
            bind_count: 2,
        },
        RD_OBLIGATION_AUDIT_EXISTS => NativeReadStatement {
            kind: RD_OBLIGATION_AUDIT_EXISTS,
            sql: SQL_OBLIGATION_AUDIT_EXISTS,
            bind_count: 0,
        },
        RD_OBLIGATION_ACTION_HEAD => NativeReadStatement {
            kind: RD_OBLIGATION_ACTION_HEAD,
            sql: SQL_OBLIGATION_ACTION_HEAD,
            bind_count: 0,
        },
        RD_OBLIGATION_ACK_HEAD => NativeReadStatement {
            kind: RD_OBLIGATION_ACK_HEAD,
            sql: SQL_OBLIGATION_ACK_HEAD,
            bind_count: 0,
        },
        RD_COMPANY_SCOPE => NativeReadStatement {
            kind: RD_COMPANY_SCOPE,
            sql: SQL_COMPANY_SCOPE,
            bind_count: 0,
        },
        RD_COMPANY_SCOPE_PORTAL_RELEASE => NativeReadStatement {
            kind: RD_COMPANY_SCOPE_PORTAL_RELEASE,
            sql: SQL_COMPANY_SCOPE_PORTAL_RELEASE,
            bind_count: 0,
        },
        _ => return Err(invalid()),
    };
    if binds != planned.bind_count {
        return Err(invalid());
    }
    Ok(planned)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Byte-oracles copied verbatim from the retired portal.ts action helper.
    const TS_ACTION_SELECT: &str = "SELECT id,binding_id,order_id,action,status,created_at,request_hash,user_id FROM customer_portal_actions WHERE ";
    const TS_REPLAY_PREDICATE: &str = "request_key=?";
    const TS_EXISTING_PREDICATE: &str = "binding_id=? AND action=?";

    #[test]
    fn plans_are_byte_identical_to_the_retired_typescript_fragments() {
        let key = plan_for(RD_PORTAL_ACTION_BY_KEY, 1).expect("registered kind");
        assert_eq!(
            key.sql,
            format!("{TS_ACTION_SELECT}{TS_REPLAY_PREDICATE}")
        );
        assert_eq!(key.bind_count, 1);

        let existing = plan_for(RD_PORTAL_ACTION_BY_BINDING, 2).expect("registered kind");
        assert_eq!(
            existing.sql,
            format!("{TS_ACTION_SELECT}{TS_EXISTING_PREDICATE}")
        );
        assert_eq!(existing.bind_count, 2);
    }

    #[test]
    fn obligation_audit_guard_is_byte_identical_to_the_retired_typescript_literal() {
        let planned = plan_for(RD_OBLIGATION_AUDIT_EXISTS, 0).expect("registered kind");
        // Verbatim pre-cut literal from package-obligations.ts.
        const TS_AUDIT_EXISTS: &str = "EXISTS(SELECT 1 FROM audit_events audit JOIN outbox_events event ON event.command_id=audit.command_id AND event.event_type=audit.action AND event.aggregate_id=request.order_id AND event.aggregate_version=request.source_version WHERE audit.command_id=request.request_key AND audit.actor_id=request.user_id AND audit.assignment_id='external-customer' AND audit.action='customer.portal.'||request.action AND audit.record_id=request.order_id AND audit.record_version=request.source_version AND audit.authority_json::jsonb->>'bindingId'=request.binding_id AND event.payload_json::jsonb->>'bindingId'=request.binding_id AND event.payload_json::jsonb->>'customerId'=request.customer_id AND event.payload_json::jsonb->>'userId'=request.user_id AND event.payload_json::jsonb->>'note'=request.note)";
        assert_eq!(planned.sql, TS_AUDIT_EXISTS);
        assert_eq!(planned.bind_count, 0);
    }

    #[test]
    fn obligation_union_heads_are_byte_identical_to_the_deleted_helper() {
        // Verbatim pre-cut renderings from the r9 BEFORE=15 baseline.
        const TS_ACTION: &str = "SELECT 'action' kind,request.id,to_jsonb(request) fact,NOT ";
        const TS_ACK: &str = "SELECT 'acknowledgement',ack.id,to_jsonb(ack),NOT ";
        let action = plan_for(RD_OBLIGATION_ACTION_HEAD, 0).expect("registered kind");
        let ack = plan_for(RD_OBLIGATION_ACK_HEAD, 0).expect("registered kind");
        assert_eq!(action.sql, TS_ACTION);
        assert_eq!(ack.sql, TS_ACK);
        assert_eq!(action.bind_count, 0);
        assert_eq!(ack.bind_count, 0);
    }

    #[test]
    fn company_scope_sources_are_byte_identical_to_the_deleted_helper() {
        // Verbatim pre-cut renderings from customer-read-sources.ts.
        const TS_SCOPE: &str = "(SELECT ?::text company_id)";
        const TS_RELEASE: &str = "(SELECT ?::text company_id) portal_release";
        let scope = plan_for(RD_COMPANY_SCOPE, 0).expect("registered kind");
        let release = plan_for(RD_COMPANY_SCOPE_PORTAL_RELEASE, 0).expect("registered kind");
        assert_eq!(scope.sql, TS_SCOPE);
        assert_eq!(release.sql, TS_RELEASE);
        assert_eq!(scope.bind_count, 0);
        assert_eq!(release.bind_count, 0);
    }

    #[test]
    fn unknown_kind_or_wrong_bind_count_fails_closed() {
        assert!(plan_for("c360_rd_unknown", 1).is_err());
        assert!(plan_for(RD_PORTAL_ACTION_BY_KEY, 0).is_err());
        assert!(plan_for(RD_PORTAL_ACTION_BY_KEY, 2).is_err());
        assert!(plan_for(RD_PORTAL_ACTION_BY_BINDING, 1).is_err());
        assert!(plan_for(RD_OBLIGATION_AUDIT_EXISTS, 1).is_err());
        assert!(plan_for(RD_OBLIGATION_ACTION_HEAD, 1).is_err());
        assert!(plan_for(RD_OBLIGATION_ACK_HEAD, 1).is_err());
        assert!(plan_for(RD_COMPANY_SCOPE, 1).is_err());
        assert!(plan_for(RD_COMPANY_SCOPE_PORTAL_RELEASE, 1).is_err());
        assert!(!SQL_PORTAL_ACTION_BY_KEY.contains(";"));
        assert!(!SQL_PORTAL_ACTION_BY_BINDING.contains(";"));
    }
}
