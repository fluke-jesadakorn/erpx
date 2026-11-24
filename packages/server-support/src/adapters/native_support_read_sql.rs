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
//! attachment-migration scope read on `support_tickets` consumed by Files and
//! the service-team option reads on `service_teams`, `service_team_revisions`,
//! `service_team_members` and `service_team_coverage`. It also plans the
//! incident-detail ticket candidate read on `support_tickets` and
//! `support_incident_tickets`, and the SLA workspace active-ticket read on
//! `support_tickets`. It finally plans the package-obligation/Portal read
//! family: the Portal table fence probe, the binding/response/identity/ack
//! aggregate and the Company-wide ticket/incident/session/SLA obligation facts.
//! It also plans the Support Calendar family: the scoped open-ticket and
//! open-incident calendar feeds on `support_tickets` and `support_incidents`,
//! carrying the owner-name/team projection and the actor-scoped read predicate
//! as fixed native SQL with only scalar values crossing the wire.
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

/// Native successor of the TypeScript incident-detail ticket candidate read.
/// The caller binds `[company_id, branch_id, department_id, incident_id]` and
/// receives the linkable sales-order tickets not already linked to the incident.
pub const RD_INCIDENT_TICKET_CANDIDATES: &str = "support_rd_incident_ticket_candidates";
const SQL_INCIDENT_TICKET_CANDIDATES: &str =
    "SELECT id,title,version FROM support_tickets WHERE company_id=? AND branch_id=? AND department_id IS NOT DISTINCT FROM ?::text AND source_kind='sales_order' AND id NOT IN(SELECT ticket_id FROM support_incident_tickets WHERE incident_id=? AND active) ORDER BY updated_at DESC LIMIT 200";
const READ_INCIDENT_TICKET_CANDIDATES: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_INCIDENT_TICKET_CANDIDATES,
    sql: SQL_INCIDENT_TICKET_CANDIDATES,
    bind_count: 4,
};

/// Native successor of the TypeScript `supportSlaWorkspace` active-ticket read.
/// The caller binds `[company_id, branch_id, branch_id, department_id, department_id]`
/// and receives the scoped tickets carrying a live SLA clock.
pub const RD_SLA_WORKSPACE_TICKETS: &str = "support_rd_sla_workspace_tickets";
const SQL_SLA_WORKSPACE_TICKETS: &str =
    "SELECT id,title,status,owner_id,sla_policy_id,sla_policy_version,sla_clock_json FROM support_tickets WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) AND sla_clock_json IS NOT NULL AND status NOT IN ('closed','cancelled') ORDER BY updated_at DESC,id";
const READ_SLA_WORKSPACE_TICKETS: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SLA_WORKSPACE_TICKETS,
    sql: SQL_SLA_WORKSPACE_TICKETS,
    bind_count: 5,
};

/// Native successors of the TypeScript package-obligation/Portal read family.
/// The caller binds `[]` for the Portal table fence probe, `[now, company_id]`
/// for the binding aggregate and `[company_id, company_id, company_id, now,
/// company_id, now]` for the obligation facts. Only revisions/counts leave the
/// owner; customer content and staff identities never cross this read.
pub const RD_PORTAL_BINDINGS_INSTALLED: &str = "support_rd_portal_bindings_installed";
const SQL_PORTAL_BINDINGS_INSTALLED: &str =
    "SELECT to_regclass('support_ticket_portal_bindings') IS NOT NULL installed";
const READ_PORTAL_BINDINGS_INSTALLED: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_PORTAL_BINDINGS_INSTALLED,
    sql: SQL_PORTAL_BINDINGS_INSTALLED,
    bind_count: 0,
};

pub const RD_SUPPORT_PACKAGE_PORTAL_BINDINGS: &str = "support_rd_support_package_portal_bindings";
const SQL_SUPPORT_PACKAGE_PORTAL_BINDINGS: &str =
    "SELECT binding.id,to_jsonb(binding) binding,(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM support_ticket_portal_events event WHERE event.binding_id=binding.id) events,(SELECT jsonb_agg(to_jsonb(identity) ORDER BY identity.binding_id) FROM support_ticket_portal_identities identity WHERE identity.binding_id=binding.id) identities,(SELECT jsonb_agg(to_jsonb(response) ORDER BY response.id) FROM support_ticket_portal_responses response WHERE response.binding_id=binding.id) responses,(SELECT jsonb_agg(to_jsonb(ack) ORDER BY ack.id) FROM support_ticket_portal_acknowledgements ack WHERE ack.binding_id=binding.id) acknowledgements,binding.status='draft' OR binding.status='approved' AND binding.expires_at>? pending FROM support_ticket_portal_bindings binding WHERE binding.company_id=? ORDER BY binding.id";
const READ_SUPPORT_PACKAGE_PORTAL_BINDINGS: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SUPPORT_PACKAGE_PORTAL_BINDINGS,
    sql: SQL_SUPPORT_PACKAGE_PORTAL_BINDINGS,
    bind_count: 2,
};

pub const RD_SUPPORT_PACKAGE_OBLIGATION_FACTS: &str = "support_rd_support_package_obligation_facts";
const SQL_SUPPORT_PACKAGE_OBLIGATION_FACTS: &str =
    "SELECT 'ticket' kind,id,version,status state FROM support_tickets WHERE company_id=? AND source_kind<>'field_service' AND status NOT IN ('closed','cancelled') UNION ALL SELECT 'incident',id,version,status FROM support_incidents WHERE company_id=? AND status NOT IN ('closed','cancelled') UNION ALL SELECT 'session',id,version,status FROM support_sessions WHERE company_id=? AND status IN ('pending','approved') AND expires_at>? UNION ALL SELECT 'sla',t.id,COALESCE(a.version,0),CASE WHEN a.id IS NULL THEN 'undrained' ELSE 'unacknowledged' END FROM support_tickets t LEFT JOIN support_sla_alerts a ON a.ticket_id=t.id WHERE t.company_id=? AND a.acknowledged_by IS NULL AND (a.id IS NOT NULL OR (t.sla_clock_json->>'breachedAt')::bigint IS NOT NULL OR t.sla_clock_json->>'state'='running' AND ((t.sla_clock_json->>'dueAt')::bigint<? OR t.status IN ('closed','cancelled'))) ORDER BY kind,id";
const READ_SUPPORT_PACKAGE_OBLIGATION_FACTS: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SUPPORT_PACKAGE_OBLIGATION_FACTS,
    sql: SQL_SUPPORT_PACKAGE_OBLIGATION_FACTS,
    bind_count: 6,
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

/// Native successors of the TypeScript service-team option reads. The caller
/// binds `[company_id]` for the team list and `[team_id, company_id]` (or
/// `[company_id, team_id]` for history) for the per-team projections.
pub const RD_SERVICE_TEAM_OPTIONS: &str = "support_rd_service_team_options";
const SQL_SERVICE_TEAM_OPTIONS: &str =
    "SELECT * FROM service_teams WHERE company_id=? ORDER BY name,id";
const READ_SERVICE_TEAM_OPTIONS: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SERVICE_TEAM_OPTIONS,
    sql: SQL_SERVICE_TEAM_OPTIONS,
    bind_count: 1,
};

pub const RD_SERVICE_TEAM_OPTION_HISTORY: &str = "support_rd_service_team_option_history";
const SQL_SERVICE_TEAM_OPTION_HISTORY: &str =
    "SELECT version,configuration_json,actor_id,created_at FROM service_team_revisions WHERE company_id=? AND team_id=? ORDER BY version DESC LIMIT 100";
const READ_SERVICE_TEAM_OPTION_HISTORY: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SERVICE_TEAM_OPTION_HISTORY,
    sql: SQL_SERVICE_TEAM_OPTION_HISTORY,
    bind_count: 2,
};

pub const RD_SERVICE_TEAM_OPTION_HANDLERS: &str = "support_rd_service_team_option_handlers";
const SQL_SERVICE_TEAM_OPTION_HANDLERS: &str =
    "SELECT user_id,responsibility FROM service_team_members WHERE team_id=? AND company_id=? ORDER BY user_id";
const READ_SERVICE_TEAM_OPTION_HANDLERS: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SERVICE_TEAM_OPTION_HANDLERS,
    sql: SQL_SERVICE_TEAM_OPTION_HANDLERS,
    bind_count: 2,
};

pub const RD_SERVICE_TEAM_OPTION_COVERAGE: &str = "support_rd_service_team_option_coverage";
const SQL_SERVICE_TEAM_OPTION_COVERAGE: &str =
    "SELECT category,scope_kind,scope_node_id FROM service_team_coverage WHERE team_id=? AND company_id=? ORDER BY category,scope_node_id";
const READ_SERVICE_TEAM_OPTION_COVERAGE: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SERVICE_TEAM_OPTION_COVERAGE,
    sql: SQL_SERVICE_TEAM_OPTION_COVERAGE,
    bind_count: 2,
};

/// Native successors of the TypeScript Support Calendar fixed-SQL reads. The
/// authoritative relations are the Support-owned `support_tickets` and
/// `support_incidents`; the embedded `hr_people` (people_payroll) and
/// `assignments` (identity_access) fragments are closed read-only public
/// projections already authored natively for Support detail reads. The actor
/// predicate is fixed native SQL; only its scalar scope binds cross the wire.
/// Ticket caller binds `[tz, company, branch, branch, department, department,
/// scopes_json, assignment_id, user_id, role, from, to, mode, owner, owner,
/// owner_filter, owner_filter, search, search, limit]`.
pub const RD_CALENDAR_SUPPORT_TICKETS: &str = "support_rd_calendar_support_tickets";
const SQL_CALENDAR_SUPPORT_TICKETS: &str =
    "SELECT t.id,t.title,t.source_record_json,t.target_date due_date,CASE WHEN t.sla_clock_json->>'state'='running' THEN (t.sla_clock_json->>'dueAt')::bigint ELSE NULL END sla_due_at,t.status,t.version,t.branch_id,t.department_id,(SELECT legal_name FROM (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) hr_people WHERE company_id=t.company_id AND user_id=t.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name FROM (SELECT s.*,CASE WHEN s.sla_clock_json->>'state'='running' THEN to_char(to_timestamp((s.sla_clock_json->>'dueAt')::bigint/1000.0) AT TIME ZONE ?,'YYYY-MM-DD') ELSE s.due_date END target_date FROM support_tickets s) t WHERE t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND coalesce(t.department_id,t.branch_id,t.company_id) IN (SELECT jsonb_array_elements_text(?::jsonb)) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=t.company_id AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=t.branch_id OR g.scope_kind='department' AND g.scope_node_id=t.department_id) AND (iam_role_allows(g.role,g.company_id,'support.read') OR t.source_kind<>'sales_order' AND t.created_by=g.user_id AND iam_role_allows(g.role,g.company_id,'records.read'))) AND t.status NOT IN ('closed','cancelled') AND t.target_date>=? AND t.target_date<? AND (?='team' OR t.owner_id=? OR t.created_by=?) AND (?::text IS NULL OR t.owner_id IN(SELECT user_id FROM (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) hr_people WHERE id=? AND company_id=t.company_id)) AND (?='' OR t.title ILIKE '%'||?||'%') ORDER BY t.target_date,t.id LIMIT ?";
const READ_CALENDAR_SUPPORT_TICKETS: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_CALENDAR_SUPPORT_TICKETS,
    sql: SQL_CALENDAR_SUPPORT_TICKETS,
    bind_count: 20,
};

/// Incident caller binds `[company, branch, branch, department, department,
/// scopes_json, assignment_id, user_id, role, from, to, mode, owner, owner,
/// owner_filter, owner_filter, search, search, limit]`.
pub const RD_CALENDAR_SUPPORT_INCIDENTS: &str = "support_rd_calendar_support_incidents";
const SQL_CALENDAR_SUPPORT_INCIDENTS: &str =
    "SELECT t.id,t.title,t.due_date,t.status,t.version,t.branch_id,t.department_id FROM support_incidents t WHERE t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND coalesce(t.department_id,t.branch_id,t.company_id) IN (SELECT jsonb_array_elements_text(?::jsonb)) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=t.company_id AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(g.role,g.company_id,'support.read') AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=t.branch_id OR g.scope_kind='department' AND g.scope_node_id=t.department_id)) AND t.status NOT IN ('closed','cancelled') AND t.due_date>=? AND t.due_date<? AND (?='team' OR t.owner_id=? OR t.created_by=?) AND (?::text IS NULL OR t.owner_id IN(SELECT user_id FROM (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) hr_people WHERE id=? AND company_id=t.company_id)) AND (?='' OR t.title ILIKE '%'||?||'%') ORDER BY t.due_date,t.id LIMIT ?";
const READ_CALENDAR_SUPPORT_INCIDENTS: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_CALENDAR_SUPPORT_INCIDENTS,
    sql: SQL_CALENDAR_SUPPORT_INCIDENTS,
    bind_count: 19,
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
    READ_INCIDENT_TICKET_CANDIDATES,
    READ_SLA_WORKSPACE_TICKETS,
    READ_PORTAL_BINDINGS_INSTALLED,
    READ_SUPPORT_PACKAGE_PORTAL_BINDINGS,
    READ_SUPPORT_PACKAGE_OBLIGATION_FACTS,
    READ_TICKET_SOURCE_IMAGE,
    READ_TICKET_ATTACHMENT_SOURCE_IMAGE,
    READ_ATTACHMENT_MIGRATION_SCOPE,
    READ_SERVICE_TEAM_OPTIONS,
    READ_SERVICE_TEAM_OPTION_HISTORY,
    READ_SERVICE_TEAM_OPTION_HANDLERS,
    READ_SERVICE_TEAM_OPTION_COVERAGE,
    READ_CALENDAR_SUPPORT_TICKETS,
    READ_CALENDAR_SUPPORT_INCIDENTS,
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
            (
                RD_INCIDENT_TICKET_CANDIDATES,
                SQL_INCIDENT_TICKET_CANDIDATES,
                4usize,
            ),
            (
                RD_SLA_WORKSPACE_TICKETS,
                SQL_SLA_WORKSPACE_TICKETS,
                5usize,
            ),
            (
                RD_PORTAL_BINDINGS_INSTALLED,
                SQL_PORTAL_BINDINGS_INSTALLED,
                0usize,
            ),
            (
                RD_SUPPORT_PACKAGE_PORTAL_BINDINGS,
                SQL_SUPPORT_PACKAGE_PORTAL_BINDINGS,
                2usize,
            ),
            (
                RD_SUPPORT_PACKAGE_OBLIGATION_FACTS,
                SQL_SUPPORT_PACKAGE_OBLIGATION_FACTS,
                6usize,
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
            (
                RD_SERVICE_TEAM_OPTIONS,
                SQL_SERVICE_TEAM_OPTIONS,
                1usize,
            ),
            (
                RD_SERVICE_TEAM_OPTION_HISTORY,
                SQL_SERVICE_TEAM_OPTION_HISTORY,
                2usize,
            ),
            (
                RD_SERVICE_TEAM_OPTION_HANDLERS,
                SQL_SERVICE_TEAM_OPTION_HANDLERS,
                2usize,
            ),
            (
                RD_SERVICE_TEAM_OPTION_COVERAGE,
                SQL_SERVICE_TEAM_OPTION_COVERAGE,
                2usize,
            ),
            (
                RD_CALENDAR_SUPPORT_TICKETS,
                SQL_CALENDAR_SUPPORT_TICKETS,
                20usize,
            ),
            (
                RD_CALENDAR_SUPPORT_INCIDENTS,
                SQL_CALENDAR_SUPPORT_INCIDENTS,
                19usize,
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
                    sql.contains("support_incident_tickets") || sql.contains("support_tickets") ||
                    sql.contains("support_incidents"),
                "owner read must stay on owner-local Support tables: {sql}"
            );
            assert_eq!(sql.matches('?').count(), binds);
        }
        assert_eq!(STATEMENTS.len(), 23);
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
        let candidates = read_statement(RD_INCIDENT_TICKET_CANDIDATES).expect("incident ticket candidates");
        assert_eq!(candidates.bind_count, 4);
        let sla_tickets = read_statement(RD_SLA_WORKSPACE_TICKETS).expect("SLA workspace tickets");
        assert_eq!(sla_tickets.bind_count, 5);
        let installed = read_statement(RD_PORTAL_BINDINGS_INSTALLED).expect("portal fence probe");
        assert_eq!(installed.bind_count, 0);
        let bindings = read_statement(RD_SUPPORT_PACKAGE_PORTAL_BINDINGS).expect("portal binding aggregate");
        assert_eq!(bindings.bind_count, 2);
        let facts = read_statement(RD_SUPPORT_PACKAGE_OBLIGATION_FACTS).expect("obligation facts");
        assert_eq!(facts.bind_count, 6);
        let source_image = read_statement(RD_TICKET_SOURCE_IMAGE).expect("ticket source image");
        assert_eq!(source_image.bind_count, 2);
        let attachment_image = read_statement(RD_TICKET_ATTACHMENT_SOURCE_IMAGE).expect("ticket attachment source image");
        assert_eq!(attachment_image.bind_count, 2);
        let migration_scope = read_statement(RD_ATTACHMENT_MIGRATION_SCOPE).expect("attachment migration scope");
        assert_eq!(migration_scope.bind_count, 1);
        let options = read_statement(RD_SERVICE_TEAM_OPTIONS).expect("service team options");
        assert_eq!(options.bind_count, 1);
        let option_history = read_statement(RD_SERVICE_TEAM_OPTION_HISTORY).expect("service team option history");
        assert_eq!(option_history.bind_count, 2);
        let option_handlers = read_statement(RD_SERVICE_TEAM_OPTION_HANDLERS).expect("service team option handlers");
        assert_eq!(option_handlers.bind_count, 2);
        let option_coverage = read_statement(RD_SERVICE_TEAM_OPTION_COVERAGE).expect("service team option coverage");
        assert_eq!(option_coverage.bind_count, 2);
        let calendar_tickets = read_statement(RD_CALENDAR_SUPPORT_TICKETS).expect("calendar ticket feed");
        assert_eq!(calendar_tickets.bind_count, 20);
        let calendar_incidents = read_statement(RD_CALENDAR_SUPPORT_INCIDENTS).expect("calendar incident feed");
        assert_eq!(calendar_incidents.bind_count, 19);
    }
}
