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
//! `support_incident_tickets`, the SLA workspace active-ticket read on
//! `support_tickets` and the SLA policy point/approved reads on
//! `support_sla_policies`. It finally plans the package-obligation/Portal read
//! family: the Portal table fence probe, the both-table portal installed probe,
//! the binding/response/identity/ack aggregate and the Company-wide
//! ticket/incident/session/SLA obligation facts.
//! It also plans the Support Calendar family: the scoped open-ticket and
//! open-incident calendar feeds on `support_tickets` and `support_incidents`,
//! carrying the owner-name/team projection and the actor-scoped read predicate
//! as fixed native SQL with only scalar values crossing the wire. It plans the
//! incident-attachment witness family: the incident image/events/recipients
//! projection and the incident-ticket link projection used for the private-file
//! before/after consistency witness. It plans the Sales projection family:
//! support-case counts, open cases and CRM source cases, where order scope,
//! ticket-actor policy and source-id-set predicates stay fixed native SQL and
//! variable id lists cross only as scalar JSON binds. It also plans the SLA
//! alert workspace read and the ticket-recipient read, where the actor id or
//! ticket id leads the owner-scope binds and the eligibility/reviewer-count
//! predicates stay fixed native SQL over the closed Identity/People projections.
//! It finally plans the Support-detail customer-contact-notes read over the
//! closed Sales column projection, scoped by order id and company id.
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

/// Native successor of the TypeScript `supportTicketPortalTicketImage` read.
/// The caller binds `[ticket_id]` and receives the owner's ticket row snapshot
/// used by the portal change-witness. The retired literal is carried verbatim:
/// no Company scope, lock, advisory or assertion is added to this read.
pub const RD_TICKET_PORTAL_TICKET_IMAGE: &str = "support_rd_ticket_portal_ticket_image";
const SQL_TICKET_PORTAL_TICKET_IMAGE: &str =
    "SELECT to_jsonb(ticket)::text image FROM support_tickets ticket WHERE ticket.id=?";
const READ_TICKET_PORTAL_TICKET_IMAGE: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_TICKET_PORTAL_TICKET_IMAGE,
    sql: SQL_TICKET_PORTAL_TICKET_IMAGE,
    bind_count: 1,
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

/// Native successor of the TypeScript SLA policy point read used by the policy
/// detail. The caller binds `[policy_id, company_id]` and receives the single
/// owner-local policy row.
pub const RD_SLA_POLICY_POINT: &str = "support_rd_sla_policy_point";
const SQL_SLA_POLICY_POINT: &str =
    "SELECT * FROM support_sla_policies WHERE id=? AND company_id=?";
const READ_SLA_POLICY_POINT: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SLA_POLICY_POINT,
    sql: SQL_SLA_POLICY_POINT,
    bind_count: 2,
};

/// Native successor of the TypeScript SLA policy approved-list read used by
/// the support detail. The caller binds `[company_id]` and receives the
/// owner's approved policies ordered by name.
pub const RD_SLA_POLICY_APPROVED: &str = "support_rd_sla_policy_approved";
const SQL_SLA_POLICY_APPROVED: &str =
    "SELECT id,name,version FROM support_sla_policies WHERE company_id=? AND status='approved' ORDER BY name";
const READ_SLA_POLICY_APPROVED: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SLA_POLICY_APPROVED,
    sql: SQL_SLA_POLICY_APPROVED,
    bind_count: 1,
};

/// Native successors of the TypeScript SLA policy workspace/work list reads.
/// The caller binds `[user_id, company_id]`: the user id leads the eligibility
/// flag and the company id scopes the policy rows. The policy-recipient and
/// reviewer projections are inlined verbatim from the retired owner literal;
/// no SQL is accepted from the wire and no write ledger is attached to them.
pub const RD_SLA_POLICY_WORKSPACE: &str = "support_rd_sla_policy_workspace";
const SQL_SLA_POLICY_WORKSPACE: &str =
    "SELECT p.*,CASE WHEN EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m WHERE m.company_id=p.company_id AND m.user_id<>p.created_by AND m.scope_kind='holding' AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=p.company_id AND a.user_id=m.user_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=NULL::text) OR (a.scope_kind='department' AND a.scope_node_id=NULL::text)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) AND m.user_id=?) THEN 1 ELSE 0 END actor_eligible FROM support_sla_policies p WHERE p.company_id=? ORDER BY p.created_at DESC,p.id";
const READ_SLA_POLICY_WORKSPACE: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SLA_POLICY_WORKSPACE,
    sql: SQL_SLA_POLICY_WORKSPACE,
    bind_count: 2,
};

pub const RD_SLA_POLICY_WORK: &str = "support_rd_sla_policy_work";
const SQL_SLA_POLICY_WORK: &str =
    "SELECT p.*,CASE WHEN EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m WHERE m.company_id=p.company_id AND m.user_id<>p.created_by AND m.scope_kind='holding' AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=p.company_id AND a.user_id=m.user_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=NULL::text) OR (a.scope_kind='department' AND a.scope_node_id=NULL::text)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) AND m.user_id=?) THEN 1 ELSE 0 END actor_eligible,(SELECT COUNT(DISTINCT m.user_id) FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m WHERE m.company_id=p.company_id AND m.user_id<>p.created_by AND m.scope_kind='holding' AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=p.company_id AND a.user_id=m.user_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=NULL::text) OR (a.scope_kind='department' AND a.scope_node_id=NULL::text)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))) reviewers FROM support_sla_policies p WHERE p.company_id=? ORDER BY p.updated_at DESC";
const READ_SLA_POLICY_WORK: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SLA_POLICY_WORK,
    sql: SQL_SLA_POLICY_WORK,
    bind_count: 2,
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

/// Native successor of the TypeScript `supportTicketPortalInstalled` fence
/// probe. Both portal tables must exist; the caller binds `[]` and receives
/// the `installed` alias. The single-table `support_rd_portal_bindings_installed`
/// kind stays registered unchanged for its own callers.
pub const RD_TICKET_PORTAL_INSTALLED: &str = "support_rd_ticket_portal_installed";
const SQL_TICKET_PORTAL_INSTALLED: &str =
    "SELECT to_regclass('support_ticket_portal_bindings') IS NOT NULL AND to_regclass('support_ticket_portal_acknowledgements') IS NOT NULL installed";
const READ_TICKET_PORTAL_INSTALLED: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_TICKET_PORTAL_INSTALLED,
    sql: SQL_TICKET_PORTAL_INSTALLED,
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

/// Native successors of the TypeScript Support executive metric projections.
/// The authoritative relations are the Support-owned `support_tickets` and
/// `support_incidents`; the embedded `organization_nodes` (organization) and
/// `assignments` (identity_access) fragments are closed read-only public
/// projections and the POS/Assets/PMS source authorities arrive as scalar id
/// sets from those owners' frozen read ports. The breakdown/overdue label and
/// scope predicates are fixed native SQL; only scalar values cross the wire.
/// Caller binds `[breakdown, breakdown, breakdown, overdue, company, branch,
/// branch, department, department, assignment, user, role, pos_ids_json,
/// asset_ids_json, pms_ids_json, overdue]`.
pub const RD_EXEC_TICKET_METRIC: &str = "support_rd_exec_ticket_metric";
const SQL_EXEC_TICKET_METRIC: &str =
    "SELECT CASE WHEN ?='status' THEN t.status WHEN ?='branch' THEN COALESCE((SELECT n.display_name FROM (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) n WHERE n.id=t.branch_id AND n.company_id=t.company_id),'Company-wide') WHEN ?='department' THEN COALESCE((SELECT n.display_name FROM (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) n WHERE n.id=t.department_id AND n.company_id=t.company_id),'Unassigned') WHEN ?=1 THEN 'Overdue SLA' ELSE t.status END label,count(*)::text value FROM support_tickets t WHERE t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=t.company_id AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=t.branch_id OR g.scope_kind='department' AND g.scope_node_id=t.department_id) AND (iam_role_allows(g.role,g.company_id,'support.read') OR t.source_kind<>'sales_order' AND t.created_by=g.user_id AND iam_role_allows(g.role,g.company_id,'records.read'))) AND (t.source_record_json IS NULL OR (t.source_record_json::jsonb->>'kind'='pos_sale' AND t.source_record_json::jsonb->>'id' IN (SELECT jsonb_array_elements_text(?::jsonb))) OR (t.source_record_json::jsonb->>'kind'='asset' AND t.source_record_json::jsonb->>'id' IN (SELECT jsonb_array_elements_text(?::jsonb))) OR (t.source_record_json::jsonb->>'kind'='pms_record' AND t.source_record_json::jsonb->>'id' IN (SELECT jsonb_array_elements_text(?::jsonb)))) AND t.status NOT IN ('closed','cancelled') AND (?=0 OR (t.sla_clock_json->>'state'='running' AND (t.sla_clock_json->>'dueAt') ~ '^[0-9]+$' AND (t.sla_clock_json->>'dueAt')::bigint < floor(extract(epoch FROM clock_timestamp())*1000)::bigint)) GROUP BY 1 ORDER BY 1";
const READ_EXEC_TICKET_METRIC: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_EXEC_TICKET_METRIC,
    sql: SQL_EXEC_TICKET_METRIC,
    bind_count: 16,
};

/// Incident caller binds `[breakdown, breakdown, breakdown, breakdown, company,
/// branch, branch, department, department, assignment, user, role]`.
pub const RD_EXEC_INCIDENT_METRIC: &str = "support_rd_exec_incident_metric";
const SQL_EXEC_INCIDENT_METRIC: &str =
    "SELECT CASE WHEN ?='status' THEN t.status WHEN ?='severity' THEN t.severity WHEN ?='branch' THEN COALESCE((SELECT n.display_name FROM (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) n WHERE n.id=t.branch_id AND n.company_id=t.company_id),'Company-wide') WHEN ?='department' THEN COALESCE((SELECT n.display_name FROM (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) n WHERE n.id=t.department_id AND n.company_id=t.company_id),'Unassigned') ELSE 'Active support incidents' END label,count(*)::text value FROM support_incidents t WHERE t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=t.company_id AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(g.role,g.company_id,'support.read') AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=t.branch_id OR g.scope_kind='department' AND g.scope_node_id=t.department_id)) AND t.status IN ('open','investigating','mitigated') GROUP BY 1 ORDER BY 1";
const READ_EXEC_INCIDENT_METRIC: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_EXEC_INCIDENT_METRIC,
    sql: SQL_EXEC_INCIDENT_METRIC,
    bind_count: 12,
};

// Fixed closed public projections from People Payroll and Commerce. The source
// owners author these relation lists; Support only composes them read-only.
// These witnesses are compiled with the registry tests that assert the planned
// statements still carry the exact closed fragments.
#[cfg(test)]
const SQL_HR_ENGAGEMENTS: &str = "(SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements)";
#[cfg(test)]
const SQL_HR_PEOPLE: &str = "(SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people)";
#[cfg(test)]
const SQL_COMMERCE_ORDERS: &str = "(SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders)";

// Fixed order-scope, ticket-actor and source-authority predicates. Only their
// scalar values cross the wire; no caller supplies SQL or a relation name.
#[cfg(test)]
const SQL_ORDER_SCOPE_T: &str =
    "t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?)";
#[cfg(test)]
const SQL_ORDER_SCOPE_O: &str =
    "o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)";
#[cfg(test)]
const SQL_TICKET_READ_POLICY_T: &str = "EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=t.company_id AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=t.branch_id OR g.scope_kind='department' AND g.scope_node_id=t.department_id) AND (iam_role_allows(g.role,g.company_id,'support.read') OR t.source_kind<>'sales_order' AND t.created_by=g.user_id AND iam_role_allows(g.role,g.company_id,'records.read')))";
#[cfg(test)]
const SQL_SOURCE_READ_POLICY_T: &str = "(t.source_record_json IS NULL OR (t.source_record_json::jsonb->>'kind'='pos_sale' AND t.source_record_json::jsonb->>'id' IN (SELECT jsonb_array_elements_text(?::jsonb))) OR (t.source_record_json::jsonb->>'kind'='asset' AND t.source_record_json::jsonb->>'id' IN (SELECT jsonb_array_elements_text(?::jsonb))) OR (t.source_record_json::jsonb->>'kind'='pms_record' AND t.source_record_json::jsonb->>'id' IN (SELECT jsonb_array_elements_text(?::jsonb))))";

/// Native successor of the TypeScript incident-attachment image projection.
/// The caller binds `[user_id, incident_id, company_id]` and receives the
/// incident snapshot, its events and its recipient/engagement witnesses.
pub const RD_INCIDENT_ATTACHMENT_SOURCE_IMAGE: &str = "support_rd_incident_attachment_source_image";
const SQL_INCIDENT_ATTACHMENT_SOURCE_IMAGE: &str = "SELECT to_jsonb(t)::text image,(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM support_incident_events event WHERE event.incident_id=t.id)::text events,(SELECT jsonb_agg(jsonb_build_object('person',to_jsonb(person),'engagements',(SELECT jsonb_agg(to_jsonb(engagement) ORDER BY engagement.id) FROM (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) engagement WHERE engagement.person_id=person.id AND engagement.company_id=t.company_id)) ORDER BY person.id) FROM (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) person WHERE person.company_id=t.company_id AND person.user_id IN (?,t.owner_id,t.mitigated_by))::text recipients FROM support_incidents t WHERE t.id=? AND t.company_id=?";
const READ_INCIDENT_ATTACHMENT_SOURCE_IMAGE: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_INCIDENT_ATTACHMENT_SOURCE_IMAGE,
    sql: SQL_INCIDENT_ATTACHMENT_SOURCE_IMAGE,
    bind_count: 3,
};

/// Native successor of the TypeScript incident-attachment link projection.
/// The caller binds `[incident_id, company_id]` and receives the same-Company
/// ticket links with their order references. The Commerce source image and the
/// lock fence stay with their owners on the caller's unit.
pub const RD_INCIDENT_ATTACHMENT_SOURCE_LINKS: &str = "support_rd_incident_attachment_source_links";
const SQL_INCIDENT_ATTACHMENT_SOURCE_LINKS: &str =
    "SELECT to_jsonb(link)::text image,to_jsonb(ticket)::text ticket,ticket.order_id FROM support_incident_tickets link JOIN support_incidents incident ON incident.id=link.incident_id LEFT JOIN support_tickets ticket ON ticket.id=link.ticket_id AND ticket.company_id=incident.company_id WHERE incident.id=? AND incident.company_id=? ORDER BY link.ticket_id";
const READ_INCIDENT_ATTACHMENT_SOURCE_LINKS: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_INCIDENT_ATTACHMENT_SOURCE_LINKS,
    sql: SQL_INCIDENT_ATTACHMENT_SOURCE_LINKS,
    bind_count: 2,
};

/// Native successors of the TypeScript Sales Support projections. The variable
/// customer-id list is a single JSON bind, so every kind has a fixed bind count
/// and the command-plan route rejects any other arity. Caller binds are the
/// order-scope scalars for `t` and `o`, the ticket-actor policy scalars and the
/// projection's final scalar id(s).
pub const RD_SALES_SUPPORT_COUNTS: &str = "support_rd_sales_support_counts";
const SQL_SALES_SUPPORT_COUNTS: &str = "SELECT t.customer_id,COUNT(*) count FROM support_tickets t JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o ON o.id=t.order_id AND o.company_id=t.company_id AND o.party_id=t.customer_id WHERE t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=t.company_id AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=t.branch_id OR g.scope_kind='department' AND g.scope_node_id=t.department_id) AND (iam_role_allows(g.role,g.company_id,'support.read') OR t.source_kind<>'sales_order' AND t.created_by=g.user_id AND iam_role_allows(g.role,g.company_id,'records.read'))) AND t.customer_id IN (SELECT jsonb_array_elements_text(?::jsonb)) AND t.status NOT IN ('closed','cancelled') GROUP BY t.customer_id";
const READ_SALES_SUPPORT_COUNTS: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SALES_SUPPORT_COUNTS,
    sql: SQL_SALES_SUPPORT_COUNTS,
    bind_count: 14,
};

pub const RD_SALES_OPEN_SUPPORT_CASES: &str = "support_rd_sales_open_support_cases";
const SQL_SALES_OPEN_SUPPORT_CASES: &str = "SELECT t.id,t.title,t.status,t.due_date FROM support_tickets t JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o ON o.id=t.order_id AND o.company_id=t.company_id AND o.party_id=t.customer_id WHERE t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=t.company_id AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=t.branch_id OR g.scope_kind='department' AND g.scope_node_id=t.department_id) AND (iam_role_allows(g.role,g.company_id,'support.read') OR t.source_kind<>'sales_order' AND t.created_by=g.user_id AND iam_role_allows(g.role,g.company_id,'records.read'))) AND t.customer_id=? AND t.status NOT IN ('closed','cancelled') ORDER BY t.updated_at DESC,t.id LIMIT 30";
const READ_SALES_OPEN_SUPPORT_CASES: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SALES_OPEN_SUPPORT_CASES,
    sql: SQL_SALES_OPEN_SUPPORT_CASES,
    bind_count: 14,
};

pub const RD_SALES_CUSTOMER_SOURCE_CASES: &str = "support_rd_sales_customer_source_cases";
const SQL_SALES_CUSTOMER_SOURCE_CASES: &str = "SELECT t.id,t.title,t.status,t.order_id FROM support_tickets t WHERE t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=t.company_id AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=t.branch_id OR g.scope_kind='department' AND g.scope_node_id=t.department_id) AND (iam_role_allows(g.role,g.company_id,'support.read') OR t.source_kind<>'sales_order' AND t.created_by=g.user_id AND iam_role_allows(g.role,g.company_id,'records.read'))) AND (t.source_record_json IS NULL OR (t.source_record_json::jsonb->>'kind'='pos_sale' AND t.source_record_json::jsonb->>'id' IN (SELECT jsonb_array_elements_text(?::jsonb))) OR (t.source_record_json::jsonb->>'kind'='asset' AND t.source_record_json::jsonb->>'id' IN (SELECT jsonb_array_elements_text(?::jsonb))) OR (t.source_record_json::jsonb->>'kind'='pms_record' AND t.source_record_json::jsonb->>'id' IN (SELECT jsonb_array_elements_text(?::jsonb)))) AND t.customer_id=? ORDER BY t.updated_at DESC,t.id LIMIT 50";
const READ_SALES_CUSTOMER_SOURCE_CASES: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SALES_CUSTOMER_SOURCE_CASES,
    sql: SQL_SALES_CUSTOMER_SOURCE_CASES,
    bind_count: 12,
};

/// Native successor of the TypeScript Support evidence-items read. The caller
/// binds `[ticket_id]` and receives the ticket's evidence rows, including
/// revoked and legally-held rows, composed over the closed read-only Files
/// owner projections. Only the scalar ticket id crosses the wire.
pub const RD_EVIDENCE_ITEMS: &str = "support_rd_evidence_items";
const SQL_EVIDENCE_ITEMS: &str =
    "SELECT a.id,a.original_name,a.media_type,a.bytes,a.checksum_sha256,a.created_at,a.created_by,a.revoked_at,a.legal_hold FROM (SELECT \"id\",\"upload_id\",\"owner_type\",\"owner_id\",\"purpose\",\"object_key\",\"revision\",\"original_name\",\"media_type\",\"bytes\",\"checksum_sha256\",\"created_by\",\"legal_hold\",\"created_at\",\"revoked_at\",\"company_id\",\"detected_media_type\",\"source_state\",\"scanned_at\",\"scanner_definition_at\",\"preview_object_key\",\"preview_media_type\",\"preview_checksum_sha256\",\"preview_bytes\",\"destroyed_at\",\"legacy_sanitized_only\" FROM attachments) a WHERE a.owner_type='support_ticket' AND a.owner_id=? AND (a.company_id IS NULL OR EXISTS(SELECT 1 FROM (SELECT \"id\",\"set_id\",\"attachment_id\",\"upload_intent_id\",\"caption\",\"state\",\"supersedes_link_id\",\"created_by\",\"created_at\",\"expires_at\",\"bound_at\",\"binding_type\",\"binding_id\",\"withdrawn_at\",\"withdrawn_by\",\"withdrawal_reason\" FROM attachment_links) l JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"target_type\",\"target_id\",\"component_kind\",\"component_ref\",\"purpose_key\",\"expected_target_version\",\"policy_version\",\"classification\",\"audience_json\",\"retention_policy_key\",\"retention_policy_version\",\"retain_until\",\"destruction_mode\",\"required_count\",\"max_files\",\"state\",\"created_by\",\"created_at\",\"updated_at\",\"bound_at\",\"bound_command_id\" FROM attachment_sets) s ON s.id=l.set_id WHERE l.attachment_id=a.id AND l.state='bound' AND s.company_id=a.company_id AND s.target_type='support_ticket' AND s.target_id=a.owner_id)) ORDER BY a.created_at,a.id";
const READ_EVIDENCE_ITEMS: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_EVIDENCE_ITEMS,
    sql: SQL_EVIDENCE_ITEMS,
    bind_count: 1,
};

/// Native successor of the TypeScript Support SLA-alert workspace read. The
/// caller binds `[user_id, company_id, branch_id, branch_id, department_id,
/// department_id]`: the user id leads the `can_ack` eligibility flag, then the
/// owner scope follows. The eligibility/reviewer-count predicates stay fixed
/// native SQL over the closed Identity/People projections; only scalar values
/// cross the wire and no write ledger is attached.
pub const RD_SLA_ALERTS: &str = "support_rd_sla_alerts";
const SQL_SLA_ALERTS: &str =
    "SELECT s.*,t.title,t.company_id,t.branch_id,t.department_id, CASE WHEN EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m WHERE m.company_id=t.company_id AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=t.company_id AND a.user_id=m.user_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=t.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) AND m.user_id=?) THEN 1 ELSE 0 END can_ack, (SELECT COUNT(DISTINCT m.user_id) FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m WHERE m.company_id=t.company_id AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=t.company_id AND a.user_id=m.user_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=t.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))) recipients FROM support_sla_alerts s JOIN support_tickets t ON t.id=s.ticket_id WHERE t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) ORDER BY s.breached_at,s.id";
const READ_SLA_ALERTS: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SLA_ALERTS,
    sql: SQL_SLA_ALERTS,
    bind_count: 6,
};

/// Native successor of the TypeScript Support ticket-recipient read. The caller
/// binds `[ticket_id, company_id, branch_id, branch_id, department_id,
/// department_id]`; the eligibility predicate stays fixed native SQL over the
/// closed Identity/People projections and only scalar values cross the wire.
pub const RD_TICKET_RECIPIENTS: &str = "support_rd_ticket_recipients";
const SQL_TICKET_RECIPIENTS: &str =
    "SELECT DISTINCT m.user_id id,p.legal_name name FROM support_tickets t JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m ON m.company_id=t.company_id JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.company_id=t.company_id AND p.user_id=m.user_id WHERE t.id=? AND t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=t.company_id AND a.user_id=m.user_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=t.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) ORDER BY p.legal_name";
const READ_TICKET_RECIPIENTS: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_TICKET_RECIPIENTS,
    sql: SQL_TICKET_RECIPIENTS,
    bind_count: 6,
};

/// Native successor of the TypeScript Support-detail customer-contact-notes
/// read. The caller binds `[order_id, company_id]` and receives the active
/// notes on that order, newest occurrence first, over the closed Sales column
/// projection. No lock, advisory or audit ledger is attached to this read.
pub const RD_CUSTOMER_CONTACT_NOTES: &str = "support_rd_customer_contact_notes";
const SQL_CUSTOMER_CONTACT_NOTES: &str =
    "SELECT id,version,channel,occurred_at,body FROM (SELECT \"id\",\"company_id\",\"customer_id\",\"order_id\",\"channel\",\"occurred_at\",\"body\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\" FROM customer_contact_notes) customer_contact_notes WHERE order_id=? AND company_id=? AND status='active' ORDER BY occurred_at DESC";
const READ_CUSTOMER_CONTACT_NOTES: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_CUSTOMER_CONTACT_NOTES,
    sql: SQL_CUSTOMER_CONTACT_NOTES,
    bind_count: 2,
};

/// Native successor of the TypeScript service-team members named read used by
/// the service-team handlers. The caller binds `[team_id, company_id]` and
/// receives the member/user id, responsibility and the People legal-name (with
/// a user-id fallback) over the closed People projection. No lock, advisory or
/// audit ledger is attached to this read.
pub const RD_SERVICE_TEAM_MEMBERS_NAMED: &str = "support_rd_service_team_members_named";
const SQL_SERVICE_TEAM_MEMBERS_NAMED: &str =
    "SELECT m.user_id,m.responsibility,COALESCE(p.legal_name,m.user_id) name FROM service_team_members m JOIN service_teams t ON t.id=m.team_id AND t.company_id=m.company_id LEFT JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.company_id=m.company_id AND p.user_id=m.user_id AND p.merged_into_id IS NULL WHERE t.id=? AND t.company_id=? AND t.active=1 ORDER BY m.user_id";
const READ_SERVICE_TEAM_MEMBERS_NAMED: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SERVICE_TEAM_MEMBERS_NAMED,
    sql: SQL_SERVICE_TEAM_MEMBERS_NAMED,
    bind_count: 2,
};

/// Native successor of the TypeScript work-team coverage witness. The caller
/// binds `[company_id, user_id, branch_id, branch_id, branch_id, branch_id,
/// department_id, department_id]` and receives the first active coverage row;
/// the node projection and team-scope predicates stay fixed native SQL with
/// only scalar values crossing the wire. No lock, advisory or audit ledger is
/// attached to this read.
pub const RD_WORK_TEAM_COVERAGE: &str = "support_rd_work_team_coverage";
const SQL_WORK_TEAM_COVERAGE: &str =
    "SELECT 1 FROM service_team_members m\n    JOIN service_teams t ON t.id=m.team_id AND t.company_id=m.company_id AND t.active=1\n    JOIN service_team_coverage c ON c.team_id=t.id AND c.company_id=t.company_id\n    JOIN (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) n ON n.id=c.scope_node_id AND n.company_id=c.company_id AND n.status='active'\n    LEFT JOIN (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) b ON b.id=n.branch_id AND b.company_id=n.company_id AND b.kind='branch'\n    WHERE m.company_id=? AND m.user_id=? AND (\n      (c.scope_kind='holding' AND n.kind='company' AND n.id=m.company_id) OR\n      (c.scope_kind='branch' AND n.kind='branch' AND (CAST(? AS TEXT) IS NULL OR n.id=?)) OR\n      (c.scope_kind='department' AND n.kind='department' AND (n.branch_id IS NULL OR b.status='active') AND (CAST(? AS TEXT) IS NULL OR n.branch_id=?) AND (CAST(? AS TEXT) IS NULL OR n.id=?))\n    ) LIMIT 1";
const READ_WORK_TEAM_COVERAGE: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_WORK_TEAM_COVERAGE,
    sql: SQL_WORK_TEAM_COVERAGE,
    bind_count: 8,
};

/// Native successors of the TypeScript active-organization-node scope probes.
/// The company/branch callers bind `[node_id, company_id]`; the department
/// caller binds `[department_id, company_id, branch_id]`. The node kind and the
/// `status='active'` predicate stay fixed native SQL over the closed Organization
/// projection, and the department probe keeps the original
/// `branch_id IS NOT DISTINCT FROM ?` scope match. No lock, advisory or audit
/// ledger is attached to these reads.
pub const RD_SERVICE_SCOPE_COMPANY: &str = "support_rd_service_scope_company";
const SQL_SERVICE_SCOPE_COMPANY: &str =
    "SELECT 1 FROM (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) organization_nodes WHERE id=? AND company_id=? AND kind='company' AND status='active'";
const READ_SERVICE_SCOPE_COMPANY: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SERVICE_SCOPE_COMPANY,
    sql: SQL_SERVICE_SCOPE_COMPANY,
    bind_count: 2,
};

pub const RD_SERVICE_SCOPE_BRANCH: &str = "support_rd_service_scope_branch";
const SQL_SERVICE_SCOPE_BRANCH: &str =
    "SELECT 1 FROM (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'";
const READ_SERVICE_SCOPE_BRANCH: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SERVICE_SCOPE_BRANCH,
    sql: SQL_SERVICE_SCOPE_BRANCH,
    bind_count: 2,
};

pub const RD_SERVICE_SCOPE_DEPARTMENT: &str = "support_rd_service_scope_department";
const SQL_SERVICE_SCOPE_DEPARTMENT: &str =
    "SELECT 1 FROM (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) organization_nodes WHERE id=? AND company_id=? AND kind='department' AND branch_id IS NOT DISTINCT FROM ? AND status='active'";
const READ_SERVICE_SCOPE_DEPARTMENT: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SERVICE_SCOPE_DEPARTMENT,
    sql: SQL_SERVICE_SCOPE_DEPARTMENT,
    bind_count: 3,
};

/// Native successor of the TypeScript service-team option organization-node
/// list. The caller binds `[company_id, branch_id, branch_id, branch_id,
/// department_id, department_id]`; the closed Organization node projection,
/// the active/kind predicates and the branch/department scope fallbacks stay
/// fixed native SQL with only scalar values crossing the wire.
pub const RD_SERVICE_TEAM_OPTION_NODES: &str = "support_rd_service_team_option_nodes";
const SQL_SERVICE_TEAM_OPTION_NODES: &str =
    "SELECT id,kind,display_name,branch_id FROM (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) organization_nodes WHERE company_id=? AND status='active' AND kind IN ('company','branch','department') AND (?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR id=? OR kind='branch') ORDER BY display_name";
const READ_SERVICE_TEAM_OPTION_NODES: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SERVICE_TEAM_OPTION_NODES,
    sql: SQL_SERVICE_TEAM_OPTION_NODES,
    bind_count: 6,
};

const STATEMENTS: &[NativeSupportReadStatement] = &[
    READ_SERVICE_COVERAGE_HISTORY,
    READ_SERVICE_TEAM_COVERAGE,
    READ_SERVICE_TEAM_NAME,
    READ_TICKET_PORTAL_RESPONSE,
    READ_TICKET_PORTAL_ACKNOWLEDGED_AT,
    READ_TICKET_PORTAL_TICKET_IMAGE,
    READ_TICKET_EVENT_HISTORY,
    READ_INCIDENT_EVENT_HISTORY,
    READ_TICKET_INCIDENT_LINKS,
    READ_INCIDENT_TICKET_LINKS,
    READ_INCIDENT_TICKET_CANDIDATES,
    READ_SLA_WORKSPACE_TICKETS,
    READ_SLA_POLICY_POINT,
    READ_SLA_POLICY_APPROVED,
    READ_SLA_POLICY_WORKSPACE,
    READ_SLA_POLICY_WORK,
    READ_PORTAL_BINDINGS_INSTALLED,
    READ_TICKET_PORTAL_INSTALLED,
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
    READ_EXEC_TICKET_METRIC,
    READ_EXEC_INCIDENT_METRIC,
    READ_INCIDENT_ATTACHMENT_SOURCE_IMAGE,
    READ_INCIDENT_ATTACHMENT_SOURCE_LINKS,
    READ_SALES_SUPPORT_COUNTS,
    READ_SALES_OPEN_SUPPORT_CASES,
    READ_SALES_CUSTOMER_SOURCE_CASES,
    READ_EVIDENCE_ITEMS,
    READ_SLA_ALERTS,
    READ_TICKET_RECIPIENTS,
    READ_CUSTOMER_CONTACT_NOTES,
    READ_SERVICE_TEAM_MEMBERS_NAMED,
    READ_WORK_TEAM_COVERAGE,
    READ_SERVICE_SCOPE_COMPANY,
    READ_SERVICE_SCOPE_BRANCH,
    READ_SERVICE_SCOPE_DEPARTMENT,
    READ_SERVICE_TEAM_OPTION_NODES,
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
                RD_TICKET_PORTAL_TICKET_IMAGE,
                SQL_TICKET_PORTAL_TICKET_IMAGE,
                1usize,
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
            (RD_SLA_POLICY_POINT, SQL_SLA_POLICY_POINT, 2usize),
            (RD_SLA_POLICY_APPROVED, SQL_SLA_POLICY_APPROVED, 1usize),
            (RD_SLA_POLICY_WORKSPACE, SQL_SLA_POLICY_WORKSPACE, 2usize),
            (RD_SLA_POLICY_WORK, SQL_SLA_POLICY_WORK, 2usize),
            (
                RD_PORTAL_BINDINGS_INSTALLED,
                SQL_PORTAL_BINDINGS_INSTALLED,
                0usize,
            ),
            (
                RD_TICKET_PORTAL_INSTALLED,
                SQL_TICKET_PORTAL_INSTALLED,
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
            (RD_EXEC_TICKET_METRIC, SQL_EXEC_TICKET_METRIC, 16usize),
            (RD_EXEC_INCIDENT_METRIC, SQL_EXEC_INCIDENT_METRIC, 12usize),
            (
                RD_INCIDENT_ATTACHMENT_SOURCE_IMAGE,
                SQL_INCIDENT_ATTACHMENT_SOURCE_IMAGE,
                3usize,
            ),
            (
                RD_INCIDENT_ATTACHMENT_SOURCE_LINKS,
                SQL_INCIDENT_ATTACHMENT_SOURCE_LINKS,
                2usize,
            ),
            (RD_SALES_SUPPORT_COUNTS, SQL_SALES_SUPPORT_COUNTS, 14usize),
            (
                RD_SALES_OPEN_SUPPORT_CASES,
                SQL_SALES_OPEN_SUPPORT_CASES,
                14usize,
            ),
            (
                RD_SALES_CUSTOMER_SOURCE_CASES,
                SQL_SALES_CUSTOMER_SOURCE_CASES,
                12usize,
            ),
            (RD_EVIDENCE_ITEMS, SQL_EVIDENCE_ITEMS, 1usize),
            (RD_SLA_ALERTS, SQL_SLA_ALERTS, 6usize),
            (RD_TICKET_RECIPIENTS, SQL_TICKET_RECIPIENTS, 6usize),
            (RD_CUSTOMER_CONTACT_NOTES, SQL_CUSTOMER_CONTACT_NOTES, 2usize),
            (RD_SERVICE_TEAM_MEMBERS_NAMED, SQL_SERVICE_TEAM_MEMBERS_NAMED, 2usize),
            (RD_WORK_TEAM_COVERAGE, SQL_WORK_TEAM_COVERAGE, 8usize),
            (RD_SERVICE_SCOPE_COMPANY, SQL_SERVICE_SCOPE_COMPANY, 2usize),
            (RD_SERVICE_SCOPE_BRANCH, SQL_SERVICE_SCOPE_BRANCH, 2usize),
            (RD_SERVICE_SCOPE_DEPARTMENT, SQL_SERVICE_SCOPE_DEPARTMENT, 3usize),
            (RD_SERVICE_TEAM_OPTION_NODES, SQL_SERVICE_TEAM_OPTION_NODES, 6usize),
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
                    sql.contains("support_incidents") || sql.contains("support_sla_policies") ||
                    sql.contains("attachment_links") || sql.contains("customer_contact_notes") ||
                    sql.contains("organization_nodes"),
                "owner read must stay on owner-local Support tables, closed Files/Organization/Sales projections: {sql}"
            );
            assert_eq!(sql.matches('?').count(), binds);
        }
        assert_eq!(STATEMENTS.len(), 46);
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
        let portal_ticket = read_statement(RD_TICKET_PORTAL_TICKET_IMAGE).expect("portal ticket image read");
        assert_eq!(portal_ticket.bind_count, 1);
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
        let policy_point = read_statement(RD_SLA_POLICY_POINT).expect("SLA policy point read");
        assert_eq!(policy_point.bind_count, 2);
        let policy_approved = read_statement(RD_SLA_POLICY_APPROVED).expect("SLA policy approved read");
        assert_eq!(policy_approved.bind_count, 1);
        let installed = read_statement(RD_PORTAL_BINDINGS_INSTALLED).expect("portal fence probe");
        assert_eq!(installed.bind_count, 0);
        let ticket_portal_installed = read_statement(RD_TICKET_PORTAL_INSTALLED).expect("both-table portal installed probe");
        assert_eq!(ticket_portal_installed.bind_count, 0);
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
        let image = read_statement(RD_INCIDENT_ATTACHMENT_SOURCE_IMAGE).expect("incident attachment image");
        assert_eq!(image.bind_count, 3);
        let links = read_statement(RD_INCIDENT_ATTACHMENT_SOURCE_LINKS).expect("incident attachment links");
        assert_eq!(links.bind_count, 2);
        let counts = read_statement(RD_SALES_SUPPORT_COUNTS).expect("sales support counts");
        assert_eq!(counts.bind_count, 14);
        let cases = read_statement(RD_SALES_OPEN_SUPPORT_CASES).expect("sales open support cases");
        assert_eq!(cases.bind_count, 14);
        let source_cases = read_statement(RD_SALES_CUSTOMER_SOURCE_CASES).expect("sales customer source cases");
        assert_eq!(source_cases.bind_count, 12);
        let alerts = read_statement(RD_SLA_ALERTS).expect("SLA alerts read");
        assert_eq!(alerts.bind_count, 6);
        let recipients = read_statement(RD_TICKET_RECIPIENTS).expect("ticket recipients read");
        assert_eq!(recipients.bind_count, 6);
        let notes = read_statement(RD_CUSTOMER_CONTACT_NOTES).expect("customer contact notes read");
        assert_eq!(notes.bind_count, 2);
        let named_members = read_statement(RD_SERVICE_TEAM_MEMBERS_NAMED)
            .expect("service team members named read");
        assert_eq!(named_members.bind_count, 2);
        let coverage = read_statement(RD_WORK_TEAM_COVERAGE).expect("work team coverage read");
        assert_eq!(coverage.bind_count, 8);
        let company_node = read_statement(RD_SERVICE_SCOPE_COMPANY).expect("company scope node read");
        assert_eq!(company_node.bind_count, 2);
        let branch_node = read_statement(RD_SERVICE_SCOPE_BRANCH).expect("branch scope node read");
        assert_eq!(branch_node.bind_count, 2);
        let department_node = read_statement(RD_SERVICE_SCOPE_DEPARTMENT).expect("department scope node read");
        assert_eq!(department_node.bind_count, 3);
        let nodes = read_statement(RD_SERVICE_TEAM_OPTION_NODES).expect("team option node list read");
        assert_eq!(nodes.bind_count, 6);
    }

    #[test]
    fn sla_alert_and_ticket_recipient_reads_keep_fixed_owner_predicates() {
        let alerts = read_statement(RD_SLA_ALERTS).expect("SLA alerts read");
        assert!(alerts.sql.starts_with("SELECT s.*,t.title,t.company_id,t.branch_id,t.department_id,"));
        assert!(alerts.sql.contains("FROM support_sla_alerts s JOIN support_tickets t ON t.id=s.ticket_id"));
        assert!(alerts.sql.contains("iam_role_allows(m.role,m.company_id,'support.manage')"));
        assert!(alerts.sql.contains("'support.work'") && alerts.sql.contains("'support.read'"));
        assert!(alerts.sql.contains(SQL_HR_ENGAGEMENTS));
        assert!(alerts.sql.ends_with("ORDER BY s.breached_at,s.id"));
        assert_eq!(alerts.sql.matches('?').count(), 6);
        let recipients = read_statement(RD_TICKET_RECIPIENTS).expect("ticket recipients read");
        assert!(recipients.sql.starts_with("SELECT DISTINCT m.user_id id,p.legal_name name FROM support_tickets t JOIN"));
        assert!(recipients.sql.contains("a.user_id=m.user_id"));
        assert!(recipients.sql.ends_with("ORDER BY p.legal_name"));
        assert_eq!(recipients.sql.matches('?').count(), 6);
    }

    #[test]
    fn incident_attachment_and_sales_reads_keep_fixed_owner_predicates() {
        let image = read_statement(RD_INCIDENT_ATTACHMENT_SOURCE_IMAGE).expect("image read");
        assert!(image.sql.contains("support_incident_events"));
        assert!(image.sql.contains("support_incidents t WHERE t.id=? AND t.company_id=?"));
        assert!(image.sql.contains(SQL_HR_ENGAGEMENTS));
        assert!(image.sql.contains(SQL_HR_PEOPLE));
        let links = read_statement(RD_INCIDENT_ATTACHMENT_SOURCE_LINKS).expect("links read");
        assert!(links.sql.contains("support_incident_tickets link JOIN support_incidents"));
        assert!(links.sql.contains("LEFT JOIN support_tickets ticket"));
        let counts = read_statement(RD_SALES_SUPPORT_COUNTS).expect("counts read");
        assert!(counts.sql.contains(SQL_COMMERCE_ORDERS));
        assert!(counts.sql.contains(SQL_ORDER_SCOPE_T));
        assert!(counts.sql.contains(SQL_ORDER_SCOPE_O));
        assert!(counts.sql.contains(SQL_TICKET_READ_POLICY_T));
        assert!(counts.sql.contains("jsonb_array_elements_text(?::jsonb)"));
        let cases = read_statement(RD_SALES_OPEN_SUPPORT_CASES).expect("cases read");
        assert!(cases.sql.contains(SQL_COMMERCE_ORDERS));
        assert!(cases.sql.contains(SQL_ORDER_SCOPE_T));
        assert!(cases.sql.contains(SQL_ORDER_SCOPE_O));
        assert!(cases.sql.contains(SQL_TICKET_READ_POLICY_T));
        let source = read_statement(RD_SALES_CUSTOMER_SOURCE_CASES).expect("source cases read");
        assert!(source.sql.contains(SQL_ORDER_SCOPE_T));
        assert!(source.sql.contains(SQL_TICKET_READ_POLICY_T));
        assert!(source.sql.contains(SQL_SOURCE_READ_POLICY_T));
        for kind in [
            RD_INCIDENT_ATTACHMENT_SOURCE_IMAGE,
            RD_INCIDENT_ATTACHMENT_SOURCE_LINKS,
            RD_SALES_SUPPORT_COUNTS,
            RD_SALES_OPEN_SUPPORT_CASES,
            RD_SALES_CUSTOMER_SOURCE_CASES,
        ] {
            let statement = read_statement(kind).expect("registered owner read kind");
            assert!(statement.sql.starts_with("SELECT "));
            assert_eq!(statement.sql.matches('?').count(), statement.bind_count);
            assert!(!statement.sql.contains(';'));
        }
    }

    #[test]
    fn sla_policy_reads_render_the_retired_owner_literals_byte_for_byte() {
        let point = read_statement(RD_SLA_POLICY_POINT).expect("SLA policy point read");
        assert_eq!(point.sql, "SELECT * FROM support_sla_policies WHERE id=? AND company_id=?");
        assert_eq!(point.bind_count, 2);
        let approved = read_statement(RD_SLA_POLICY_APPROVED).expect("SLA policy approved read");
        assert_eq!(
            approved.sql,
            "SELECT id,name,version FROM support_sla_policies WHERE company_id=? AND status='approved' ORDER BY name"
        );
        assert_eq!(approved.bind_count, 1);
        assert!(read_statement("support_policy_select").is_none());
    }

    #[test]
    fn ticket_portal_ticket_image_read_renders_the_retired_owner_literal_byte_for_byte() {
        let image = read_statement(RD_TICKET_PORTAL_TICKET_IMAGE).expect("portal ticket image read");
        assert_eq!(
            image.sql,
            "SELECT to_jsonb(ticket)::text image FROM support_tickets ticket WHERE ticket.id=?"
        );
        assert_eq!(image.bind_count, 1);
        assert_eq!(image.sql.matches('?').count(), 1);
        assert!(image.sql.starts_with("SELECT "));
        assert!(!image.sql.contains(';'));
        assert!(read_statement("support_rd_ticket_portal_ticket_image_select").is_none());
    }

    #[test]
    fn ticket_portal_installed_read_renders_the_retired_owner_literal_byte_for_byte() {
        let installed = read_statement(RD_TICKET_PORTAL_INSTALLED).expect("portal installed probe");
        assert_eq!(
            installed.sql,
            "SELECT to_regclass('support_ticket_portal_bindings') IS NOT NULL AND to_regclass('support_ticket_portal_acknowledgements') IS NOT NULL installed"
        );
        assert_eq!(installed.bind_count, 0);
        assert_eq!(installed.sql.matches('?').count(), 0);
        assert!(installed.sql.ends_with(" IS NOT NULL installed"));
        assert!(installed.sql.starts_with("SELECT "));
        assert!(!installed.sql.contains(';'));
        assert!(read_statement("support_rd_ticket_portal_installed_select").is_none());
    }

    #[test]
    fn customer_contact_notes_read_renders_the_retired_owner_literal_byte_for_byte() {
        let notes = read_statement(RD_CUSTOMER_CONTACT_NOTES).expect("customer contact notes read");
        assert_eq!(
            notes.sql,
            "SELECT id,version,channel,occurred_at,body FROM (SELECT \"id\",\"company_id\",\"customer_id\",\"order_id\",\"channel\",\"occurred_at\",\"body\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\" FROM customer_contact_notes) customer_contact_notes WHERE order_id=? AND company_id=? AND status='active' ORDER BY occurred_at DESC"
        );
        assert_eq!(notes.bind_count, 2);
        assert_eq!(notes.sql.matches('?').count(), 2);
        assert!(notes.sql.starts_with("SELECT "));
        assert!(!notes.sql.contains(';'));
        assert!(read_statement("support_rd_customer_contact_notes_select").is_none());
    }

    #[test]
    fn service_team_members_named_read_renders_the_retired_owner_literal_byte_for_byte() {
        let members = read_statement(RD_SERVICE_TEAM_MEMBERS_NAMED)
            .expect("service team members named read");
        assert_eq!(
            members.sql,
            "SELECT m.user_id,m.responsibility,COALESCE(p.legal_name,m.user_id) name FROM service_team_members m JOIN service_teams t ON t.id=m.team_id AND t.company_id=m.company_id LEFT JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.company_id=m.company_id AND p.user_id=m.user_id AND p.merged_into_id IS NULL WHERE t.id=? AND t.company_id=? AND t.active=1 ORDER BY m.user_id"
        );
        assert_eq!(members.bind_count, 2);
        assert_eq!(members.sql.matches('?').count(), 2);
        assert!(members.sql.starts_with("SELECT "));
        assert!(members.sql.contains(SQL_HR_PEOPLE));
        assert!(members.sql.contains("COALESCE(p.legal_name,m.user_id) name"));
        assert!(members.sql.ends_with("WHERE t.id=? AND t.company_id=? AND t.active=1 ORDER BY m.user_id"));
        assert!(!members.sql.contains(';'));
        assert!(read_statement("support_rd_service_team_members_named_select").is_none());
    }

    #[test]
    fn work_team_coverage_read_renders_the_retired_owner_literal_byte_for_byte() {
        let coverage = read_statement(RD_WORK_TEAM_COVERAGE).expect("work team coverage read");
        assert_eq!(
            coverage.sql,
            "SELECT 1 FROM service_team_members m\n    JOIN service_teams t ON t.id=m.team_id AND t.company_id=m.company_id AND t.active=1\n    JOIN service_team_coverage c ON c.team_id=t.id AND c.company_id=t.company_id\n    JOIN (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) n ON n.id=c.scope_node_id AND n.company_id=c.company_id AND n.status='active'\n    LEFT JOIN (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) b ON b.id=n.branch_id AND b.company_id=n.company_id AND b.kind='branch'\n    WHERE m.company_id=? AND m.user_id=? AND (\n      (c.scope_kind='holding' AND n.kind='company' AND n.id=m.company_id) OR\n      (c.scope_kind='branch' AND n.kind='branch' AND (CAST(? AS TEXT) IS NULL OR n.id=?)) OR\n      (c.scope_kind='department' AND n.kind='department' AND (n.branch_id IS NULL OR b.status='active') AND (CAST(? AS TEXT) IS NULL OR n.branch_id=?) AND (CAST(? AS TEXT) IS NULL OR n.id=?))\n    ) LIMIT 1"
        );
        assert_eq!(coverage.bind_count, 8);
        assert_eq!(coverage.sql.matches('?').count(), 8);
        assert!(coverage.sql.starts_with("SELECT 1 FROM service_team_members m"));
        assert!(coverage.sql.contains("FROM organization_nodes"));
        assert!(coverage.sql.ends_with("    ) LIMIT 1"));
        assert!(!coverage.sql.contains(';'));
        assert!(read_statement("support_rd_work_team_coverage_select").is_none());
    }

    #[test]
    fn service_scope_org_node_reads_render_the_retired_owner_literals_byte_for_byte() {
        let nodes = "(SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes)";
        let company = read_statement(RD_SERVICE_SCOPE_COMPANY).expect("company scope node read");
        assert_eq!(
            company.sql,
            format!("SELECT 1 FROM {nodes} organization_nodes WHERE id=? AND company_id=? AND kind='company' AND status='active'")
        );
        assert_eq!(company.bind_count, 2);
        assert_eq!(company.sql.matches('?').count(), 2);
        let branch = read_statement(RD_SERVICE_SCOPE_BRANCH).expect("branch scope node read");
        assert_eq!(
            branch.sql,
            format!("SELECT 1 FROM {nodes} organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'")
        );
        assert_eq!(branch.bind_count, 2);
        assert_eq!(branch.sql.matches('?').count(), 2);
        let department = read_statement(RD_SERVICE_SCOPE_DEPARTMENT).expect("department scope node read");
        assert_eq!(
            department.sql,
            format!("SELECT 1 FROM {nodes} organization_nodes WHERE id=? AND company_id=? AND kind='department' AND branch_id IS NOT DISTINCT FROM ? AND status='active'")
        );
        assert_eq!(department.bind_count, 3);
        assert_eq!(department.sql.matches('?').count(), 3);
        for statement in [company, branch, department] {
            assert!(statement.sql.starts_with("SELECT 1 FROM (SELECT"));
            assert!(statement.sql.contains("FROM organization_nodes) organization_nodes WHERE id=?"));
            assert!(statement.sql.ends_with("AND status='active'"));
            assert!(!statement.sql.contains(';'));
        }
        assert!(department.sql.contains("branch_id IS NOT DISTINCT FROM ?"));
        assert!(read_statement("support_rd_service_scope_company_select").is_none());
        assert!(read_statement("support_rd_service_scope_branch_select").is_none());
        assert!(read_statement("support_rd_service_scope_department_select").is_none());
    }

    #[test]
    fn service_team_option_nodes_read_renders_the_retired_owner_literal_byte_for_byte() {
        let nodes = read_statement(RD_SERVICE_TEAM_OPTION_NODES).expect("team option node list read");
        assert_eq!(
            nodes.sql,
            "SELECT id,kind,display_name,branch_id FROM (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) organization_nodes WHERE company_id=? AND status='active' AND kind IN ('company','branch','department') AND (?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR id=? OR kind='branch') ORDER BY display_name"
        );
        assert_eq!(nodes.bind_count, 6);
        assert_eq!(nodes.sql.matches('?').count(), 6);
        assert!(nodes.sql.starts_with("SELECT id,kind,display_name,branch_id FROM (SELECT"));
        assert!(nodes.sql.contains("kind IN ('company','branch','department')"));
        assert!(nodes.sql.contains("(?::text IS NULL OR id=? OR branch_id=?)"));
        assert!(nodes.sql.contains("(?::text IS NULL OR id=? OR kind='branch')"));
        assert!(nodes.sql.ends_with("ORDER BY display_name"));
        assert!(!nodes.sql.contains(';'));
        assert!(read_statement("support_rd_service_team_option_nodes_select").is_none());
    }
}
