//! Owner-owned caller-unit read plans for Support. Generated from the retired
//! TypeScript SELECT literals with the exact positional payloads; the mounted
//! `support/command-plan` route resolves the allow-listed kind here and returns
//! the statement for the caller to prepare and execute on its own open unit.
//!
//! This family covers small owner-local Support read sets: service-team
//! coverage/history/name on `service_teams`, `service_team_coverage` and
//! `service_team_revisions`, ticket-portal response/acknowledgement reads on
//! `support_ticket_portal_responses` and `support_ticket_portal_acknowledgements`,
//! ticket/incident event history on `support_ticket_events` and
//! `support_incident_events`, incident-ticket link reads on
//! `support_incident_tickets`, `support_tickets` and `support_incidents`, and
//! ticket source-image snapshots on `support_tickets` used by the private-file
//! and portal-preview consistency witnesses. It also plans the owner-local
//! attachment-migration scope read on `support_tickets` consumed by Files.
//! Reads carry no receipt/audit/outbox ledger: the caller executes them on the
//! same unit as the surrounding command. No SQL is ever accepted from the wire
//! and no TypeScript SQL remains for these kinds.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound and `bind_count` the number
/// of positional arguments the caller must send.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeSupportReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
}

/// Native successor of the TypeScript `serviceCoverageSettingsHistory` read.
/// The caller binds `[company_id]`.
pub const RD_SERVICE_COVERAGE_HISTORY: &str = "support_rd_service_coverage_history";
const SQL_SERVICE_COVERAGE_HISTORY: &str =
    "SELECT t.name title,r.version,r.actor_id actor,r.created_at at,'Service coverage' kind FROM service_team_revisions r JOIN service_teams t ON t.id=r.team_id AND t.company_id=r.company_id WHERE r.company_id=? ORDER BY r.created_at DESC";
const READ_SERVICE_COVERAGE_HISTORY: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SERVICE_COVERAGE_HISTORY,
    sql: SQL_SERVICE_COVERAGE_HISTORY,
    bind_count: 1,
};

/// Native successor of the TypeScript `resolveServiceTeam` coverage read.
/// The caller binds `[company_id, category, company_id, branch_id, department_id]`.
pub const RD_SERVICE_TEAM_COVERAGE: &str = "support_rd_service_team_coverage";
const SQL_SERVICE_TEAM_COVERAGE: &str = "SELECT t.*,c.scope_kind,c.scope_node_id FROM service_team_coverage c JOIN service_teams t ON t.id=c.team_id AND t.company_id=c.company_id WHERE c.company_id=? AND c.category=? AND t.active=1 AND ((c.scope_kind='holding' AND c.scope_node_id=?) OR (c.scope_kind='branch' AND c.scope_node_id=?) OR (c.scope_kind='department' AND c.scope_node_id=?)) ORDER BY CASE c.scope_kind WHEN 'department' THEN 3 WHEN 'branch' THEN 2 ELSE 1 END DESC,t.id";
const READ_SERVICE_TEAM_COVERAGE: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SERVICE_TEAM_COVERAGE,
    sql: SQL_SERVICE_TEAM_COVERAGE,
    bind_count: 5,
};

/// Native successor of the TypeScript service-team name lookups used by the
/// records employee-request and workflow flow-routing facts. The caller binds
/// `[team_id, company_id]` and receives the single team name row.
pub const RD_SERVICE_TEAM_NAME: &str = "support_rd_service_team_name";
const SQL_SERVICE_TEAM_NAME: &str =
    "SELECT name FROM service_teams WHERE id=? AND company_id=?";
const READ_SERVICE_TEAM_NAME: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SERVICE_TEAM_NAME,
    sql: SQL_SERVICE_TEAM_NAME,
    bind_count: 2,
};

/// Native successor of the TypeScript `ticketPortalResponse` read. The caller
/// binds `[binding_id]` and receives the latest published response row.
pub const RD_TICKET_PORTAL_RESPONSE: &str = "support_rd_ticket_portal_response";
const SQL_TICKET_PORTAL_RESPONSE: &str =
    "SELECT id,response_version,message,created_at FROM support_ticket_portal_responses WHERE binding_id=? ORDER BY response_version DESC LIMIT 1";
const READ_TICKET_PORTAL_RESPONSE: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_TICKET_PORTAL_RESPONSE,
    sql: SQL_TICKET_PORTAL_RESPONSE,
    bind_count: 1,
};

/// Native successor of the TypeScript `ticketPortalAcknowledgedAt` read. The
/// caller binds `[binding_id, response_id]` and receives the acknowledgement
/// timestamp row.
pub const RD_TICKET_PORTAL_ACKNOWLEDGED_AT: &str = "support_rd_ticket_portal_acknowledged_at";
const SQL_TICKET_PORTAL_ACKNOWLEDGED_AT: &str =
    "SELECT created_at FROM support_ticket_portal_acknowledgements WHERE binding_id=? AND response_id=?";
const READ_TICKET_PORTAL_ACKNOWLEDGED_AT: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_TICKET_PORTAL_ACKNOWLEDGED_AT,
    sql: SQL_TICKET_PORTAL_ACKNOWLEDGED_AT,
    bind_count: 2,
};

/// Native successor of the TypeScript `support_ticket_events` history read used
/// by the support detail. The caller binds `[ticket_id]` and receives the event
/// rows in descending version order.
pub const RD_TICKET_EVENT_HISTORY: &str = "support_rd_ticket_event_history";
const SQL_TICKET_EVENT_HISTORY: &str =
    "SELECT * FROM support_ticket_events WHERE ticket_id=? ORDER BY version DESC";
const READ_TICKET_EVENT_HISTORY: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_TICKET_EVENT_HISTORY,
    sql: SQL_TICKET_EVENT_HISTORY,
    bind_count: 1,
};

/// Native successor of the TypeScript `support_incident_events` history read
/// used by the incident detail. The caller binds `[incident_id]` and receives
/// the event rows in descending version order.
pub const RD_INCIDENT_EVENT_HISTORY: &str = "support_rd_incident_event_history";
const SQL_INCIDENT_EVENT_HISTORY: &str =
    "SELECT * FROM support_incident_events WHERE incident_id=? ORDER BY version DESC";
const READ_INCIDENT_EVENT_HISTORY: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_INCIDENT_EVENT_HISTORY,
    sql: SQL_INCIDENT_EVENT_HISTORY,
    bind_count: 1,
};

/// Native successor of the TypeScript ticket-detail incident link read. The
/// caller binds `[ticket_id]` and receives the active incidents linked to that
/// ticket, newest first.
pub const RD_TICKET_INCIDENT_LINKS: &str = "support_rd_ticket_incident_links";
const SQL_TICKET_INCIDENT_LINKS: &str =
    "SELECT i.id,i.title,i.status FROM support_incident_tickets l JOIN support_incidents i ON i.id=l.incident_id WHERE l.ticket_id=? AND l.active ORDER BY i.updated_at DESC";
const READ_TICKET_INCIDENT_LINKS: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_TICKET_INCIDENT_LINKS,
    sql: SQL_TICKET_INCIDENT_LINKS,
    bind_count: 1,
};

/// Native successor of the TypeScript incident-detail ticket link read. The
/// caller binds `[incident_id]` and receives the active tickets linked to that
/// incident with their linked version.
pub const RD_INCIDENT_TICKET_LINKS: &str = "support_rd_incident_ticket_links";
const SQL_INCIDENT_TICKET_LINKS: &str =
    "SELECT t.id,t.title,t.status,t.version,l.linked_version FROM support_incident_tickets l JOIN support_tickets t ON t.id=l.ticket_id WHERE l.incident_id=? AND l.active ORDER BY l.linked_at,t.id";
const READ_INCIDENT_TICKET_LINKS: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_INCIDENT_TICKET_LINKS,
    sql: SQL_INCIDENT_TICKET_LINKS,
    bind_count: 1,
};

/// Native successor of the TypeScript portal-preview ticket source image read.
/// The caller binds `[ticket_id, company_id]` and receives the owned snapshot
/// used as the before/after consistency witness.
pub const RD_TICKET_SOURCE_IMAGE: &str = "support_rd_ticket_source_image";
const SQL_TICKET_SOURCE_IMAGE: &str =
    "SELECT to_jsonb(ticket)::text image FROM support_tickets ticket WHERE ticket.id=? AND ticket.company_id=?";
const READ_TICKET_SOURCE_IMAGE: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_TICKET_SOURCE_IMAGE,
    sql: SQL_TICKET_SOURCE_IMAGE,
    bind_count: 2,
};

/// Native successor of the TypeScript private-file attachment ticket source
/// image read. The caller binds `[ticket_id, company_id]` and receives the
/// owned row snapshot plus the source/order/customer references needed by the
/// attachment consistency witness.
pub const RD_TICKET_ATTACHMENT_SOURCE_IMAGE: &str = "support_rd_ticket_attachment_source_image";
const SQL_TICKET_ATTACHMENT_SOURCE_IMAGE: &str =
    "SELECT to_jsonb(t)::text image,t.source_record_json,t.order_id,t.customer_id FROM support_tickets t WHERE t.id=? AND t.company_id=?";
const READ_TICKET_ATTACHMENT_SOURCE_IMAGE: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_TICKET_ATTACHMENT_SOURCE_IMAGE,
    sql: SQL_TICKET_ATTACHMENT_SOURCE_IMAGE,
    bind_count: 2,
};

/// Native successor of the TypeScript `legacyAttachmentScopesupport` read.
/// The caller binds `[ticket_id]` and receives the Company/branch/department
/// scope and version used by the Files legacy attachment owner witness.
pub const RD_ATTACHMENT_MIGRATION_SCOPE: &str = "support_rd_attachment_migration_scope";
const SQL_ATTACHMENT_MIGRATION_SCOPE: &str =
    "SELECT company_id,branch_id branch_id,department_id department_id,version FROM support_tickets WHERE id=?";
const READ_ATTACHMENT_MIGRATION_SCOPE: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_ATTACHMENT_MIGRATION_SCOPE,
    sql: SQL_ATTACHMENT_MIGRATION_SCOPE,
    bind_count: 1,
};

const STATEMENTS: &[NativeSupportReadStatement] = &[
    READ_SERVICE_COVERAGE_HISTORY,
    READ_SERVICE_TEAM_COVERAGE,
    READ_SERVICE_TEAM_NAME,
    READ_TICKET_PORTAL_RESPONSE,
    READ_TICKET_PORTAL_ACKNOWLEDGED_AT,
    READ_TICKET_EVENT_HISTORY,
    READ_INCIDENT_EVENT_HISTORY,
    READ_TICKET_INCIDENT_LINKS,
    READ_INCIDENT_TICKET_LINKS,
    READ_TICKET_SOURCE_IMAGE,
    READ_TICKET_ATTACHMENT_SOURCE_IMAGE,
    READ_ATTACHMENT_MIGRATION_SCOPE,
];

/// Resolve an allow-listed owner read kind. Any unregistered kind fails closed.
pub fn read_statement(kind: &str) -> Option<&'static NativeSupportReadStatement> {
    STATEMENTS.iter().find(|statement| statement.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_team_reads_are_owner_authored_select_only() {
        let expected = [
            (
                RD_SERVICE_COVERAGE_HISTORY,
                SQL_SERVICE_COVERAGE_HISTORY,
                1usize,
            ),
            (
                RD_SERVICE_TEAM_COVERAGE,
                SQL_SERVICE_TEAM_COVERAGE,
                5usize,
            ),
            (
                RD_SERVICE_TEAM_NAME,
                SQL_SERVICE_TEAM_NAME,
                2usize,
            ),
            (
                RD_TICKET_PORTAL_RESPONSE,
                SQL_TICKET_PORTAL_RESPONSE,
                1usize,
            ),
            (
                RD_TICKET_PORTAL_ACKNOWLEDGED_AT,
                SQL_TICKET_PORTAL_ACKNOWLEDGED_AT,
                2usize,
            ),
            (
                RD_TICKET_EVENT_HISTORY,
                SQL_TICKET_EVENT_HISTORY,
                1usize,
            ),
            (
                RD_INCIDENT_EVENT_HISTORY,
                SQL_INCIDENT_EVENT_HISTORY,
                1usize,
            ),
            (
                RD_TICKET_INCIDENT_LINKS,
                SQL_TICKET_INCIDENT_LINKS,
                1usize,
            ),
            (
                RD_INCIDENT_TICKET_LINKS,
                SQL_INCIDENT_TICKET_LINKS,
                1usize,
            ),
            (RD_TICKET_SOURCE_IMAGE, SQL_TICKET_SOURCE_IMAGE, 2usize),
            (
                RD_TICKET_ATTACHMENT_SOURCE_IMAGE,
                SQL_TICKET_ATTACHMENT_SOURCE_IMAGE,
                2usize,
            ),
            (
                RD_ATTACHMENT_MIGRATION_SCOPE,
                SQL_ATTACHMENT_MIGRATION_SCOPE,
                1usize,
            ),
        ];
        for (kind, sql, binds) in expected {
            let statement = read_statement(kind).expect("registered owner read kind");
            assert_eq!(statement.kind, kind);
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.bind_count, binds);
            assert!(sql.starts_with("SELECT "), "owner read must be SELECT-only: {sql}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
                assert!(!sql.contains(forbidden), "owner read must not mutate: {forbidden}");
            }
            assert!(
                sql.contains("service_team") || sql.contains("support_ticket_portal") ||
                    sql.contains("support_ticket_events") || sql.contains("support_incident_events") ||
                    sql.contains("support_incident_tickets") || sql.contains("support_tickets"),
                "owner read must stay on owner-local Support tables: {sql}"
            );
            assert_eq!(sql.matches('?').count(), binds);
        }
        assert_eq!(STATEMENTS.len(), 12);
    }

    #[test]
    fn unknown_and_mismatched_read_kinds_fail_closed() {
        assert!(read_statement("SELECT 1 FROM service_teams").is_none());
        assert!(read_statement("").is_none());
        assert!(read_statement("support_rd_unknown").is_none());
        let statement = read_statement(RD_SERVICE_TEAM_COVERAGE).expect("coverage read");
        assert_eq!(statement.bind_count, 5);
        assert!(statement.bind_count != 1);
        let name = read_statement(RD_SERVICE_TEAM_NAME).expect("team name read");
        assert_eq!(name.bind_count, 2);
        let response = read_statement(RD_TICKET_PORTAL_RESPONSE).expect("portal response read");
        assert_eq!(response.bind_count, 1);
        let ack = read_statement(RD_TICKET_PORTAL_ACKNOWLEDGED_AT).expect("portal ack read");
        assert_eq!(ack.bind_count, 2);
        let ticket_events = read_statement(RD_TICKET_EVENT_HISTORY).expect("ticket event history");
        assert_eq!(ticket_events.bind_count, 1);
        let incident_events = read_statement(RD_INCIDENT_EVENT_HISTORY).expect("incident event history");
        assert_eq!(incident_events.bind_count, 1);
        let ticket_links = read_statement(RD_TICKET_INCIDENT_LINKS).expect("ticket incident links");
        assert_eq!(ticket_links.bind_count, 1);
        let incident_links = read_statement(RD_INCIDENT_TICKET_LINKS).expect("incident ticket links");
        assert_eq!(incident_links.bind_count, 1);
        let source_image = read_statement(RD_TICKET_SOURCE_IMAGE).expect("ticket source image");
        assert_eq!(source_image.bind_count, 2);
        let attachment_image = read_statement(RD_TICKET_ATTACHMENT_SOURCE_IMAGE).expect("ticket attachment source image");
        assert_eq!(attachment_image.bind_count, 2);
        let migration_scope = read_statement(RD_ATTACHMENT_MIGRATION_SCOPE).expect("attachment migration scope");
        assert_eq!(migration_scope.bind_count, 1);
    }
}
