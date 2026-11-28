//! Native-owned Organization write registry and executor.
//!
//! This is the authoritative owner model for every remaining TypeScript write
//! literal in `modules/organization/server/src/**/*.ts`. The SQL and its
//! positional argument order live here, in the Organization owner crate. A
//! transport calls [`execute_owner_write`] with an allow-listed typed `kind`
//! and caller-supplied values; the executor runs the statement on the
//! caller-owned serializable unit together with receipt/audit/outbox effects.
//!
//! No TypeScript SQL copy, no fallback writer and no mirrored manifest remain
//! for a retired kind.
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{Arguments, Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

// -- attachments-finance-facts ------------------------------------------------
pub const ORG_ATTACHMENT_NODE_VERSION_STEP: &str = "org_attachment_node_version_step";
/// Native-owned foundation Company node image. SELECT-only and node-id keyed
/// over the org-owned `organization_nodes` table; no lock and no cross-owner
/// source. The caller keeps its attachment read authority.
pub const ATTACHMENT_ORGANIZATION_SOURCE_IMAGE: &str = "attachment_organization_source_image";
/// Native-owned attachment source images for an organization node or change.
/// SELECT-only, id + Company keyed over org-owned `organization_nodes` and
/// `organization_node_changes`; the caller keeps its attachment scope check.
pub const ATTACHMENT_SOURCE_NODE_IMAGE: &str = "attachment_source_node_image";
pub const ATTACHMENT_SOURCE_CHANGE_IMAGE: &str = "attachment_source_change_image";
/// Cross-owner attachment version step. Only the allow-listed tables below are
/// admitted; the table name is never accepted as SQL text from a caller.
pub const ATTACHMENT_OWNER_VERSION_STEP: &str = "attachment_owner_version_step";

// -- beta-onboarding-effects --------------------------------------------------
pub const BETA_BRANCH_INSERT: &str = "beta_branch_insert";
pub const BETA_DEPARTMENT_INSERT: &str = "beta_department_insert";

// -- company-business-acceptance ---------------------------------------------
pub const ACCEPTANCE_SUBMIT: &str = "acceptance_submit";
pub const ACCEPTANCE_SUBMITTED_EVENT: &str = "acceptance_submitted_event";
pub const ACCEPTANCE_REVIEW: &str = "acceptance_review";
pub const ACCEPTANCE_REVIEW_EVENT: &str = "acceptance_review_event";
pub const ACCEPTANCE_REVOKE: &str = "acceptance_revoke";
pub const ACCEPTANCE_REVOKED_EVENT: &str = "acceptance_revoked_event";

// -- department-authority -----------------------------------------------------
pub const AUTHORITY_SEAT_UPSERT: &str = "authority_seat_upsert";
pub const AUTHORITY_APPOINTMENT_INSERT: &str = "authority_appointment_insert";
pub const DEPARTMENT_HEAD_UPSERT: &str = "department_head_upsert";
pub const DEPARTMENT_FLOW_VERSION_INSERT: &str = "department_flow_version_insert";

// -- department-authority-reads: owner-local read kinds -----------------------
// These ten `SELECT` predicates are the owner-local half of the retired
// `department-authority-reads.ts` family. Their SQL is authoritative here and
// reached only with a typed kind and positional values (statement mode) on the
// caller's unit. The three cross-owner reads (hr_people/hr_engagements owned by
// people_payroll) stay behind that owner's projection port and are deliberately
// NOT duplicated in this registry.
pub const DEPT_AUTH_READ_SEAT: &str = "dept_auth_read_seat";
pub const DEPT_AUTH_READ_HISTORY: &str = "dept_auth_read_history";
pub const DEPT_AUTH_READ_POLICIES: &str = "dept_auth_read_policies";
pub const DEPT_AUTH_READ_DEPARTMENTS: &str = "dept_auth_read_departments";
pub const DEPT_AUTH_READ_DEPARTMENT_ACTIVE: &str = "dept_auth_read_department_active";
pub const DEPT_AUTH_READ_POSITION_VALID: &str = "dept_auth_read_position_valid";
pub const DEPT_AUTH_READ_FLOW_VERSION: &str = "dept_auth_read_flow_version";
pub const DEPT_AUTH_READ_POLICY: &str = "dept_auth_read_policy";
pub const DEPT_AUTH_READ_LEGACY_HEAD: &str = "dept_auth_read_legacy_head";
/// Deferred workflow assertion predicate for a published department policy.
/// Stored verbatim as the owner's predicate; the cross-owner workflow layer
/// evaluates it inside its own command transaction.
pub const DEPT_AUTH_READ_POLICY_CHECK: &str = "dept_auth_read_policy_check";

// -- organization-work: owner-local change reads ------------------------------
// The two `SELECT` reads retired from `organization-work.ts`. Both are
// Company-keyed reads of the org-owned `organization_node_changes` table; the
// `iam_same_person` predicate is a scalar function, not a cross-owner table,
// so no people_payroll hr_* source is duplicated here.
pub const ORG_WORK_READ_CHANGE_EXISTS: &str = "org_work_read_change_exists";
pub const ORG_WORK_READ_CHANGES: &str = "org_work_read_changes";

// -- change-event snapshots ----------------------------------------------------
/// Native-owned latest change-event snapshot reads (applied draft mapping and
/// submitted relocation snapshot). SELECT-only and id keyed; the application
/// keeps its company and review authority checks.
pub const CHANGE_EVENT_SNAPSHOT_APPLIED: &str = "change_event_snapshot_applied";
pub const CHANGE_EVENT_SNAPSHOT_SUBMITTED: &str = "change_event_snapshot_submitted";

// -- access.ts / organization.ts: owner-local access projection reads ---------
// The `SELECT` reads retired from `adapters/access.ts` and the `levels`/
// `templates` ports of `adapters/organization.ts`. All are Company-keyed reads
// of org-owned tables (`organization_nodes`, `staff_levels` and
// `organization_templates`); no people_payroll hr_* source is needed for these
// scoped projections. The `?::text IS NULL` predicates are the branch and
// department visibility windows and keep the exact `?` placeholder form the
// caller binds on its own unit.
pub const ORG_ACCESS_READ_NODES: &str = "org_access_read_nodes";
pub const ORG_ACCESS_READ_LEVELS: &str = "org_access_read_levels";
/// Native-owned Company template list retired from the `templates` port of
/// `adapters/organization.ts`. SELECT-only and Company keyed over the org-owned
/// `organization_templates` table; the caller keeps every permission check.
pub const ORG_ACCESS_READ_TEMPLATES: &str = "org_access_read_templates";

// -- operations-level-history-effects / people-placement-level-history -------
pub const STAFF_LEVEL_HISTORY_INSERT: &str = "staff_level_history_insert";

// -- operations-staff-settings-facts -----------------------------------------
pub const STAFF_LEVEL_UPDATE: &str = "staff_level_update";
pub const STAFF_LEVEL_INSERT: &str = "staff_level_insert";
pub const STAFF_DEPARTMENT_HEAD_UPSERT: &str = "staff_department_head_upsert";

// -- organization-drafts ------------------------------------------------------
pub const DRAFT_OPEN: &str = "draft_open";
pub const DRAFT_SAVE: &str = "draft_save";
pub const DRAFT_REQUEST_DEPARTMENT: &str = "draft_request_department";
pub const DRAFT_REQUEST_GROUP: &str = "draft_request_group";
pub const DRAFT_RESERVATION_INSERT: &str = "draft_reservation_insert";
pub const DRAFT_SUBMIT: &str = "draft_submit";
pub const DRAFT_REQUEST_APPLIED: &str = "draft_request_applied";
pub const DRAFT_REQUEST_SCHEDULED: &str = "draft_request_scheduled";
pub const DRAFT_POSITION_APPLY: &str = "draft_position_apply";
pub const DRAFT_RESERVATION_DELETE: &str = "draft_reservation_delete";
pub const DRAFT_REQUEST_NEEDS_REVIEW: &str = "draft_request_needs_review";

// -- organization-drafts: orchestration reads ---------------------------------
/// Native-owned, Company-keyed draft reads. SELECT-only and executed on the
/// caller's unit; every permission check stays in the TypeScript application
/// and the transport never substitutes for that check.
pub const DRAFT_READ_ACTIVE: &str = "draft_read_active";
pub const DRAFT_READ_BY_ID: &str = "draft_read_by_id";
/// Native-owned draft request list. SELECT-only, draft-id keyed; the caller
/// passes its Company for transport and keeps its draft authority.
pub const DRAFT_READ_REQUESTS: &str = "draft_read_requests";
pub const DRAFT_READ_RECENT: &str = "draft_read_recent";
pub const DRAFT_READ_PENDING_POSITIONS: &str = "draft_read_pending_positions";
pub const DRAFT_READ_RESERVATIONS: &str = "draft_read_reservations";
pub const DRAFT_READ_LEVEL_LEAVE_DEFAULT: &str = "draft_read_level_leave_default";
/// Native-owned draft leave-default list. SELECT-only and Company keyed over
/// the org-owned `level_leave_defaults` table.
pub const DRAFT_READ_LEAVE_DEFAULTS: &str = "draft_read_leave_defaults";
pub const DRAFT_READ_NODE_CHANGE_STATE: &str = "draft_read_node_change_state";

// -- organization.ts ----------------------------------------------------------
pub const NODE_CREATE: &str = "node_create";
pub const NODE_RELOCATE_COPY: &str = "node_relocate_copy";
pub const NODE_ARCHIVE: &str = "node_archive";
pub const NODE_POSITION_RELOCATE: &str = "node_position_relocate";
pub const RELOCATED_AUTHORITY_SEAT: &str = "relocated_authority_seat";
pub const RELOCATED_AUTHORITY_APPOINTMENT: &str = "relocated_authority_appointment";
pub const RELOCATED_DEPARTMENT_HEAD: &str = "relocated_department_head";
pub const RELOCATED_FLOW_VERSION: &str = "relocated_flow_version";
pub const NODE_UPDATE: &str = "node_update";
pub const CHANGE_REVIEW: &str = "change_review";
pub const CHANGE_EVENT_INSERT: &str = "change_event_insert";
pub const CHANGE_SAVE: &str = "change_save";
pub const CHANGE_CANCEL: &str = "change_cancel";

// -- people-lifecycle-facts ---------------------------------------------------
pub const LIFECYCLE_POSITION_VERSION_STEP: &str = "lifecycle_position_version_step";
/// Native-owned people-lifecycle active node list. SELECT-only and Company
/// keyed over the org-owned `organization_nodes` table; the caller keeps its
/// own lifecycle data authority and receives the retained `{id,name,kind}[]`
/// row shape through the typed kind.
pub const PEOPLE_LIFECYCLE_DATA_NODES: &str = "people_lifecycle_data_nodes";
/// Native-owned lifecycle save position snapshot. SELECT-only and Company
/// keyed with the position id; the caller keeps its lifecycle authority and
/// receives the retained level/branch/department row or null.
pub const LIFECYCLE_READ_SAVE_POSITION: &str = "lifecycle_read_save_position";
/// Native-owned rehire and lifecycle-worker active position snapshot.
/// SELECT-only and Company keyed with the position id; both callers keep
/// their own authority checks and receive the retained active-position
/// row(s) or null.
pub const LIFECYCLE_READ_REHIRE_POSITION: &str = "lifecycle_read_rehire_position";

// -- profile-media-command ----------------------------------------------------
pub const PROFILE_MEDIA_UPLOAD_AVATAR: &str = "profile_media_upload_avatar";
pub const PROFILE_MEDIA_UPLOAD_COVER: &str = "profile_media_upload_cover";
pub const PROFILE_MEDIA_UPDATE_AVATAR: &str = "profile_media_update_avatar";
pub const PROFILE_MEDIA_UPDATE_COVER: &str = "profile_media_update_cover";
pub const PROFILE_MEDIA_REMOVE_AVATAR: &str = "profile_media_remove_avatar";
pub const PROFILE_MEDIA_REMOVE_COVER: &str = "profile_media_remove_cover";

// -- spending-template-owner --------------------------------------------------
pub const SPENDING_TEMPLATE_PUBLISH: &str = "spending_template_publish";

// -- staff-defaults -----------------------------------------------------------
pub const POSITION_PAY_DEFAULT_INSERT: &str = "position_pay_default_insert";
pub const LEVEL_LEAVE_DEFAULT_INSERT: &str = "level_leave_default_insert";
/// Native-owned staff-levels list for operations staff settings. SELECT-only,
/// Company keyed over the org-owned `staff_levels` table; no lock.
pub const STAFF_SETTINGS_LIST_LEVELS: &str = "staff_settings_list_levels";
/// Native-owned department-head request-route fact. SELECT-only and Company +
/// department keyed over the org-owned `department_heads`/`organization_nodes`
/// tables; no lock.
pub const STAFF_POLICY_REQUEST_ROUTE_HEAD: &str = "staff_policy_request_route_head";

// -- presentation-reads (draft / company identity) ------------------------------
/// Native-owned draft/presentation company-name read. Read-only and Company
/// scoped (`id=? AND company_id=?`); permission checks stay in the caller.
pub const PRESENTATION_DRAFT_COMPANY_NAME: &str = "presentation_draft_company_name";
/// Native-owned active Holding company identity read. `id` is the Company node
/// id, so this is Company scoped; read-only.
pub const PRESENTATION_ACTIVE_COMPANY_IDENTITY: &str = "presentation_active_company_identity";
pub const PRESENTATION_ACTIVE_COMPANY_SLUG: &str = "presentation_active_company_slug";
pub const PRESENTATION_ACTIVE_COMPANY_NAME: &str = "presentation_active_company_name";
/// Native-owned active branch display-name read for the current Company.
pub const PRESENTATION_SCOPED_BRANCH_NAMES: &str = "presentation_scoped_branch_names";
/// Native-owned active Level options read for the current Company.
pub const PRESENTATION_LEAVE_DEFAULT_LEVELS: &str = "presentation_leave_default_levels";

// -- roster-reads ---------------------------------------------------------------
/// Retired `organizationRoster` people read: non-ended employee people with
/// their level projection. Read-only; the retained role gate stays in the
/// TypeScript application and transport never substitutes for the check.
pub const ORG_READROSTERPEOPLE: &str = "org_readrosterpeople";
/// Retired `organizationRoster` positions read: Company positions with level
/// numbers. Read-only; same gate/transport contract as the people read.
pub const ORG_READROSTERPOSITIONS: &str = "org_readrosterpositions";

// -- company-business-acceptance reads ----------------------------------------
/// Native-owned Company setup acceptance readiness reads. SELECT-only and
/// Company keyed; the retained authority/scope checks stay in the TypeScript
/// application and transport never substitutes for those checks.
pub const ACCEPTANCE_READ_COMPANY: &str = "acceptance_read_company";
pub const ACCEPTANCE_READ_BRANCHES: &str = "acceptance_read_branches";
pub const ACCEPTANCE_READ_CEO: &str = "acceptance_read_ceo";
pub const ACCEPTANCE_READ_CURRENT: &str = "acceptance_read_current";
pub const ACCEPTANCE_READ_EVENTS: &str = "acceptance_read_events";
/// Native-owned Company-acceptance version assertion predicates. SELECT-only
/// and Company keyed; these are the exact `EXISTS(SELECT ...)` predicates that
/// the retired `company-business-acceptance.ts` version checks inlined. The
/// cross-owner workflow layer stores them verbatim as command assertions.
pub const ACCEPTANCE_SUBMIT_VERSION_CHECK: &str = "acceptance_submit_version_check";
pub const ACCEPTANCE_REVIEW_VERSION_CHECK: &str = "acceptance_review_version_check";
pub const ACCEPTANCE_REVOKE_VERSION_CHECK: &str = "acceptance_revoke_version_check";

// -- company-acceptance-result-projection -------------------------------------
/// Native-owned Company-acceptance result projection. The retired TS module
/// exported this `SELECT` as a string constant; the SQL and column order live
/// here and are reached only by typed kind and positional values. SELECT-only
/// and Company keyed.
pub const COMPANY_ACCEPTANCE_RESULT_PROJECTION: &str = "company_acceptance_result_projection";

// -- profile-facts -------------------------------------------------------------
/// Native-owned active-department fact. SELECT-only and Company keyed; the
/// boolean reduction stays in the TypeScript caller.
pub const PROFILE_ACTIVE_DEPARTMENT: &str = "profile_active_department";

// -- workspace-route-company ---------------------------------------------------
/// Native-owned structural company-route read. SELECT-only and Company scoped
/// via the caller's bound Company key; branch/department/company slugs are
/// optional positional values (`NULL` means "no filter"). The caller keeps the
/// current access check, and this owner never substitutes for that check.
pub const WORKSPACE_ROUTE_COMPANY: &str = "workspace_route_company";

// -- public-marketing-receiving-branch -----------------------------------------
/// Native-owned public-form receiving-branch read. SELECT-only, Company keyed
/// and owner-local (`organization_nodes`); the caller keeps the form authority
/// check and the transport never substitutes for it.
pub const PUBLIC_MARKETING_RECEIVING_BRANCH: &str = "public_marketing_receiving_branch";

// -- work-report-facts ---------------------------------------------------------
/// Native-owned work-report node list. SELECT-only and Company keyed; the
/// single work_projection consumer keeps its own report authority checks.
pub const WORK_REPORT_ORGANIZATION_NODES: &str = "work_report_organization_nodes";

// -- spending-position ---------------------------------------------------------
/// Native-owned active-spending-position fact. SELECT-only and Company keyed;
/// the boolean reduction stays in the TypeScript caller.
pub const SPENDING_ACTIVE_POSITION: &str = "spending_active_position";

// -- pms-rollout-facts ---------------------------------------------------------
/// Native-owned PMS rollout active-branch fact. SELECT-only and Company keyed
/// over the org-owned `organization_nodes` table; the boolean reduction stays
/// in the TypeScript consumer.
pub const PMS_ROLLOUT_ACTIVE_BRANCH: &str = "pms_rollout_active_branch";

// -- calendar-access-facts ----------------------------------------------------
/// Native-owned calendar-access organization-node list. SELECT-only and
/// Company keyed over the org-owned `organization_nodes` table; the
/// work_projection consumer keeps its own access checks and receives the same
/// `CalendarNode[]` row-list shape through the typed kind.
pub const CALENDAR_ACCESS_ORGANIZATION_NODES: &str = "calendar_access_organization_nodes";

// -- finance-starter-sources ---------------------------------------------------
/// Native-owned finance-starter branch/department node list. SELECT-only and
/// Company keyed over the org-owned `organization_nodes` table; the caller
/// keeps its own starter authority and the transport never substitutes for it.
pub const FINANCE_STARTER_ORGANIZATION_NODES: &str = "finance_starter_organization_nodes";

/// Every kind this owner module accepts. Anything else is fail-closed.
pub const KINDS: &[&str] = &[
    ORG_ATTACHMENT_NODE_VERSION_STEP,
    ATTACHMENT_OWNER_VERSION_STEP,
    ATTACHMENT_ORGANIZATION_SOURCE_IMAGE,
    ATTACHMENT_SOURCE_NODE_IMAGE,
    ATTACHMENT_SOURCE_CHANGE_IMAGE,
    BETA_BRANCH_INSERT,
    BETA_DEPARTMENT_INSERT,
    ACCEPTANCE_SUBMIT,
    ACCEPTANCE_SUBMITTED_EVENT,
    ACCEPTANCE_REVIEW,
    ACCEPTANCE_REVIEW_EVENT,
    ACCEPTANCE_REVOKE,
    ACCEPTANCE_REVOKED_EVENT,
    AUTHORITY_SEAT_UPSERT,
    AUTHORITY_APPOINTMENT_INSERT,
    DEPARTMENT_HEAD_UPSERT,
    DEPARTMENT_FLOW_VERSION_INSERT,
    DEPT_AUTH_READ_SEAT,
    DEPT_AUTH_READ_HISTORY,
    DEPT_AUTH_READ_POLICIES,
    DEPT_AUTH_READ_DEPARTMENTS,
    DEPT_AUTH_READ_DEPARTMENT_ACTIVE,
    DEPT_AUTH_READ_POSITION_VALID,
    DEPT_AUTH_READ_FLOW_VERSION,
    DEPT_AUTH_READ_POLICY,
    DEPT_AUTH_READ_LEGACY_HEAD,
    DEPT_AUTH_READ_POLICY_CHECK,
    ORG_WORK_READ_CHANGE_EXISTS,
    ORG_WORK_READ_CHANGES,
    CHANGE_EVENT_SNAPSHOT_APPLIED,
    CHANGE_EVENT_SNAPSHOT_SUBMITTED,
    ORG_ACCESS_READ_NODES,
    ORG_ACCESS_READ_LEVELS,
    ORG_ACCESS_READ_TEMPLATES,
    STAFF_LEVEL_HISTORY_INSERT,
    STAFF_LEVEL_UPDATE,
    STAFF_LEVEL_INSERT,
    STAFF_DEPARTMENT_HEAD_UPSERT,
    DRAFT_OPEN,
    DRAFT_SAVE,
    DRAFT_REQUEST_DEPARTMENT,
    DRAFT_REQUEST_GROUP,
    DRAFT_RESERVATION_INSERT,
    DRAFT_SUBMIT,
    DRAFT_REQUEST_APPLIED,
    DRAFT_REQUEST_SCHEDULED,
    DRAFT_POSITION_APPLY,
    DRAFT_RESERVATION_DELETE,
    DRAFT_REQUEST_NEEDS_REVIEW,
    DRAFT_READ_ACTIVE,
    DRAFT_READ_BY_ID,
    DRAFT_READ_REQUESTS,
    DRAFT_READ_RECENT,
    DRAFT_READ_PENDING_POSITIONS,
    DRAFT_READ_RESERVATIONS,
    DRAFT_READ_LEVEL_LEAVE_DEFAULT,
    DRAFT_READ_LEAVE_DEFAULTS,
    DRAFT_READ_NODE_CHANGE_STATE,
    NODE_CREATE,
    NODE_RELOCATE_COPY,
    NODE_ARCHIVE,
    NODE_POSITION_RELOCATE,
    RELOCATED_AUTHORITY_SEAT,
    RELOCATED_AUTHORITY_APPOINTMENT,
    RELOCATED_DEPARTMENT_HEAD,
    RELOCATED_FLOW_VERSION,
    NODE_UPDATE,
    CHANGE_REVIEW,
    CHANGE_EVENT_INSERT,
    CHANGE_SAVE,
    CHANGE_CANCEL,
    LIFECYCLE_POSITION_VERSION_STEP,
    PEOPLE_LIFECYCLE_DATA_NODES,
    LIFECYCLE_READ_SAVE_POSITION,
    LIFECYCLE_READ_REHIRE_POSITION,
    PROFILE_MEDIA_UPLOAD_AVATAR,
    PROFILE_MEDIA_UPLOAD_COVER,
    PROFILE_MEDIA_UPDATE_AVATAR,
    PROFILE_MEDIA_UPDATE_COVER,
    PROFILE_MEDIA_REMOVE_AVATAR,
    PROFILE_MEDIA_REMOVE_COVER,
    SPENDING_TEMPLATE_PUBLISH,
    POSITION_PAY_DEFAULT_INSERT,
    LEVEL_LEAVE_DEFAULT_INSERT,
    STAFF_SETTINGS_LIST_LEVELS,
    STAFF_POLICY_REQUEST_ROUTE_HEAD,
    PRESENTATION_DRAFT_COMPANY_NAME,
    PRESENTATION_ACTIVE_COMPANY_IDENTITY,
    PRESENTATION_ACTIVE_COMPANY_SLUG,
    PRESENTATION_ACTIVE_COMPANY_NAME,
    PRESENTATION_SCOPED_BRANCH_NAMES,
    PRESENTATION_LEAVE_DEFAULT_LEVELS,
    ORG_READROSTERPEOPLE,
    ORG_READROSTERPOSITIONS,
    ACCEPTANCE_READ_COMPANY,
    ACCEPTANCE_READ_BRANCHES,
    ACCEPTANCE_READ_CEO,
    ACCEPTANCE_READ_CURRENT,
    ACCEPTANCE_READ_EVENTS,
    ACCEPTANCE_SUBMIT_VERSION_CHECK,
    ACCEPTANCE_REVIEW_VERSION_CHECK,
    ACCEPTANCE_REVOKE_VERSION_CHECK,
    COMPANY_ACCEPTANCE_RESULT_PROJECTION,
    PROFILE_ACTIVE_DEPARTMENT,
    WORKSPACE_ROUTE_COMPANY,
    PUBLIC_MARKETING_RECEIVING_BRANCH,
    WORK_REPORT_ORGANIZATION_NODES,
    SPENDING_ACTIVE_POSITION,
    PMS_ROLLOUT_ACTIVE_BRANCH,
    CALENDAR_ACCESS_ORGANIZATION_NODES,
    FINANCE_STARTER_ORGANIZATION_NODES,
];

/// Tables admitted for the generic attachment owner version step. Column is
/// always `version`; only a table from this list can be interpolated.
const ATTACHMENT_VERSION_TABLES: &[&str] = &[
    "organization_node_changes",
    "finance_bank_statements",
    "finance_periods",
    "finance_assets",
    "finance_plan_editions",
    "finance_book_rules",
    "finance_business_billing_runs",
    "iam_reviews",
    "iam_migrations",
    "company_provisioning",
];

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "ORGANIZATION_OWNER_WRITE_INVALID",
        "Review the organization write request.",
    )
}

/// Whether a write kind belongs to this native owner module.
pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Authoritative SQL for an owner write kind. Placeholders use `?` exactly as
/// the retained TypeScript statements did; the executor translates them to
/// positional `$N` parameters before binding. No caller supplies SQL text.
fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        ORG_ATTACHMENT_NODE_VERSION_STEP => {
            "UPDATE organization_nodes SET hierarchy_revision=hierarchy_revision+1 WHERE id=? AND hierarchy_revision=?"
        }
        ATTACHMENT_OWNER_VERSION_STEP => "", // built from an allow-listed table in sql_for
        ATTACHMENT_ORGANIZATION_SOURCE_IMAGE => {
            "SELECT to_jsonb(company)::text image FROM organization_nodes company WHERE company.id=? AND company.kind='company'"
        }
        ATTACHMENT_SOURCE_NODE_IMAGE => {
            "SELECT jsonb_build_object('node',to_jsonb(node),'company',(SELECT to_jsonb(company) FROM organization_nodes company WHERE company.id=node.company_id AND company.kind='company'),\n 'branch',(SELECT to_jsonb(branch) FROM organization_nodes branch WHERE branch.id=node.branch_id AND branch.company_id=node.company_id))::text image FROM organization_nodes node WHERE node.id=? AND node.company_id=? AND node.kind IN('company','headquarters','branch','department')"
        }
        ATTACHMENT_SOURCE_CHANGE_IMAGE => {
            "SELECT jsonb_build_object('change',to_jsonb(change),'target',(SELECT to_jsonb(node) FROM organization_nodes node WHERE node.id=change.target_node_id AND node.company_id=change.company_id),\n 'branch',(SELECT to_jsonb(branch) FROM organization_nodes branch WHERE branch.id=change.branch_id AND branch.company_id=change.company_id),\n 'events',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.version,event.id) FROM organization_node_change_events event WHERE event.change_id=change.id AND event.company_id=change.company_id))::text image FROM organization_node_changes change WHERE change.id=? AND change.company_id=?"
        }
        BETA_BRANCH_INSERT => {
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,?,?,?,?) ON CONFLICT DO NOTHING"
        }
        BETA_DEPARTMENT_INSERT => {
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department','team','Team',?,?,?) ON CONFLICT DO NOTHING"
        }
        ACCEPTANCE_SUBMIT => {
            "INSERT INTO company_business_acceptance(company_id,status,version,requested_by,requested_at,reason,updated_at) VALUES(?,'submitted',?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET status='submitted',version=excluded.version,requested_by=excluded.requested_by,reviewed_by=NULL,requested_at=excluded.requested_at,reviewed_at=NULL,reason=excluded.reason,review_reason=NULL,updated_at=excluded.updated_at"
        }
        ACCEPTANCE_SUBMITTED_EVENT => {
            "INSERT INTO company_business_acceptance_events(id,company_id,action,actor_id,version,reason,created_at) VALUES(?,?,'submitted',?,?,?,?)"
        }
        ACCEPTANCE_REVIEW => {
            "UPDATE company_business_acceptance SET status=?,version=?,reviewed_by=?,reviewed_at=?,review_reason=?,updated_at=? WHERE company_id=?"
        }
        ACCEPTANCE_REVIEW_EVENT => {
            "INSERT INTO company_business_acceptance_events(id,company_id,action,actor_id,version,reason,created_at) VALUES(?,?,?,?,?,?,?)"
        }
        ACCEPTANCE_REVOKE => {
            "UPDATE company_business_acceptance SET status='revoked',version=?,reviewed_by=NULL,reviewed_at=?,review_reason=?,updated_at=? WHERE company_id=?"
        }
        ACCEPTANCE_REVOKED_EVENT => {
            "INSERT INTO company_business_acceptance_events(id,company_id,action,actor_id,version,reason,created_at) VALUES(?,?,'revoked',?,?,?,?)"
        }
        AUTHORITY_SEAT_UPSERT => {
            "INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(company_id,kind,scope_id) DO UPDATE SET version=excluded.version,position_id=excluded.position_id,person_id=excluded.person_id,effective_at=excluded.effective_at"
        }
        AUTHORITY_APPOINTMENT_INSERT => {
            "INSERT INTO authority_appointments(id,company_id,kind,scope_id,version,position_id,person_id,effective_at,appointed_by,reason,provenance,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,'explicit',?,?)"
        }
        DEPARTMENT_HEAD_UPSERT => {
            "INSERT INTO department_heads(department_id,company_id,person_id,version,updated_by,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(department_id) DO UPDATE SET person_id=excluded.person_id,version=department_heads.version+1,updated_by=excluded.updated_by,updated_at=excluded.updated_at"
        }
        DEPARTMENT_FLOW_VERSION_INSERT => {
            "INSERT INTO department_flow_versions(id,company_id,department_id,version,steps_json,published_by,reason,command_id,created_at) VALUES(?,?,?,?,CAST(? AS text)::jsonb,?,?,?,?)"
        }
        DEPT_AUTH_READ_SEAT => {
            "SELECT * FROM authority_seats WHERE company_id=? AND kind=? AND scope_id=?"
        }
        DEPT_AUTH_READ_HISTORY => {
            "SELECT * FROM authority_appointments WHERE company_id=? ORDER BY created_at DESC,id DESC LIMIT 200"
        }
        DEPT_AUTH_READ_POLICIES => {
            "SELECT DISTINCT ON (department_id) department_id,version,steps_json,published_by,reason,created_at FROM department_flow_versions WHERE company_id=? ORDER BY department_id,version DESC"
        }
        DEPT_AUTH_READ_DEPARTMENTS => {
            "SELECT id,display_name FROM organization_nodes WHERE company_id=? AND kind='department' AND status='active' ORDER BY display_name"
        }
        DEPT_AUTH_READ_DEPARTMENT_ACTIVE => {
            "SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='department' AND status='active'"
        }
        DEPT_AUTH_READ_POSITION_VALID => {
            "SELECT id FROM organization_positions WHERE id=? AND company_id=? AND active=1 AND (?<>'ceo' OR is_ceo=1) AND (?<>'head' OR department_id=?)"
        }
        DEPT_AUTH_READ_FLOW_VERSION => {
            "SELECT COALESCE(MAX(version),0) version FROM department_flow_versions WHERE company_id=? AND department_id=?"
        }
        DEPT_AUTH_READ_POLICY => {
            "SELECT id,version,steps_json FROM department_flow_versions WHERE company_id=? AND department_id=? ORDER BY version DESC LIMIT 1"
        }
        DEPT_AUTH_READ_LEGACY_HEAD => {
            "SELECT person_id FROM department_heads WHERE company_id=? AND department_id=?"
        }
        DEPT_AUTH_READ_POLICY_CHECK => {
            "EXISTS(SELECT 1 FROM department_flow_versions WHERE id=? AND company_id=? AND version=?)"
        }
        ORG_WORK_READ_CHANGE_EXISTS => {
            "SELECT 1 FROM organization_node_changes WHERE id=? AND company_id=?"
        }
        ORG_WORK_READ_CHANGES => {
            "SELECT c.*,iam_same_person(c.requested_by,?,c.company_id) same_requester FROM organization_node_changes c WHERE c.company_id=? AND (c.requested_by=? OR c.state='submitted' OR c.reviewed_by=?) ORDER BY c.updated_at DESC,c.id"
        }
        CHANGE_EVENT_SNAPSHOT_APPLIED => {
            "SELECT snapshot_json FROM organization_node_change_events WHERE change_id=? AND action='applied' ORDER BY created_at DESC LIMIT 1"
        }
        CHANGE_EVENT_SNAPSHOT_SUBMITTED => {
            "SELECT snapshot_json FROM organization_node_change_events WHERE change_id=? AND action='submitted' ORDER BY created_at DESC,id DESC LIMIT 1"
        }
        ORG_ACCESS_READ_NODES => {
            "SELECT id,kind,slug,display_name,branch_id,status,hierarchy_revision FROM organization_nodes WHERE company_id=? AND kind IN ('company','branch','department') AND (?::text IS NULL OR kind='company' OR id=? OR branch_id=?) AND (?::text IS NULL OR kind!='department' OR id=?) ORDER BY display_name,id"
        }
        ORG_ACCESS_READ_LEVELS => {
            "SELECT id,name,level_number FROM staff_levels WHERE company_id=? AND active=1 AND level_number IS NOT NULL ORDER BY level_number"
        }
        ORG_ACCESS_READ_TEMPLATES => {
            "SELECT id,name,version,positions_json FROM organization_templates WHERE company_id=? ORDER BY name"
        }
        STAFF_LEVEL_HISTORY_INSERT => {
            "INSERT INTO staff_level_history(id,engagement_id,previous_level_id,level_id,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,?,?,?)"
        }
        STAFF_LEVEL_UPDATE => {
            "UPDATE staff_levels SET code=?,name=?,rank=?,active=?,level_number=COALESCE(?,level_number),version=version+1,updated_at=? WHERE id=?"
        }
        STAFF_LEVEL_INSERT => {
            "INSERT INTO staff_levels(id,company_id,code,name,rank,active,level_number,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)"
        }
        STAFF_DEPARTMENT_HEAD_UPSERT => {
            "INSERT INTO department_heads(department_id,company_id,person_id,version,updated_by,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(department_id) DO UPDATE SET person_id=excluded.person_id,version=excluded.version,updated_by=excluded.updated_by,updated_at=excluded.updated_at"
        }
        DRAFT_OPEN => {
            "INSERT INTO organization_drafts(id,company_id,owner_id,definition,created_at,updated_at) VALUES(?,?,?,CAST(? AS text)::jsonb,?,?) ON CONFLICT(company_id,owner_id) WHERE state='draft' DO NOTHING"
        }
        DRAFT_SAVE => {
            "UPDATE organization_drafts SET definition=CAST(? AS text)::jsonb,base_snapshot=CAST(? AS text)::jsonb,state=?,version=version+1,updated_at=? WHERE id=? AND version=? RETURNING *"
        }
        DRAFT_REQUEST_DEPARTMENT => {
            "INSERT INTO organization_draft_requests(id,draft_id,company_id,item_keys,definition,baseline,dependencies,change_id,state,actor_json,effective_at,created_at,updated_at) VALUES(?,?,?,CAST(? AS text)::jsonb,CAST(? AS text)::jsonb,'{}','[]',?,'waiting_approval',CAST(? AS text)::jsonb,?,?,?)"
        }
        DRAFT_REQUEST_GROUP => {
            "INSERT INTO organization_draft_requests(id,draft_id,company_id,item_keys,definition,baseline,dependencies,state,actor_json,effective_at,created_at,updated_at) VALUES(?,?,?,CAST(? AS text)::jsonb,CAST(? AS text)::jsonb,CAST(? AS text)::jsonb,CAST(? AS text)::jsonb,?,CAST(? AS text)::jsonb,?,?,?)"
        }
        DRAFT_RESERVATION_INSERT => {
            "INSERT INTO organization_draft_reservations(company_id,kind,entity_id,request_id) VALUES(?,?,?,?)"
        }
        DRAFT_SUBMIT => {
            "UPDATE organization_drafts SET state='submitted',version=version+1,updated_at=? WHERE id=? AND state='draft' AND version=?"
        }
        DRAFT_REQUEST_APPLIED => {
            "UPDATE organization_draft_requests SET state='applied',applied_at=?,updated_at=? WHERE id=?"
        }
        DRAFT_REQUEST_SCHEDULED => {
            "UPDATE organization_draft_requests SET state='scheduled',updated_at=? WHERE id=?"
        }
        DRAFT_POSITION_APPLY => {
            "UPDATE organization_positions SET branch_id=?,department_id=?,level_id=?,reports_to_position_id=?,version=version+1,updated_at=? WHERE company_id=? AND id=?"
        }
        DRAFT_RESERVATION_DELETE => "DELETE FROM organization_draft_reservations WHERE request_id=?",
        DRAFT_REQUEST_NEEDS_REVIEW => {
            "UPDATE organization_draft_requests SET state='needs_review',last_error=?,updated_at=? WHERE id=? AND state!='applied'"
        }
        DRAFT_READ_ACTIVE => {
            "SELECT * FROM organization_drafts WHERE company_id=? AND owner_id=? AND state='draft'"
        }
        DRAFT_READ_BY_ID => {
            "SELECT * FROM organization_drafts WHERE company_id=? AND owner_id=? AND id=?"
        }
        DRAFT_READ_REQUESTS => {
            "SELECT id,item_keys,state,last_error,change_id FROM organization_draft_requests WHERE draft_id=? ORDER BY created_at,id"
        }
        DRAFT_READ_RECENT => {
            "SELECT id,updated_at,jsonb_array_length(definition->'items') count FROM organization_drafts WHERE company_id=? AND owner_id=? AND state='submitted' ORDER BY updated_at DESC LIMIT 20"
        }
        DRAFT_READ_PENDING_POSITIONS => {
            "SELECT position_id FROM organization_position_moves WHERE company_id=? AND status='scheduled'"
        }
        DRAFT_READ_RESERVATIONS => {
            "SELECT kind,entity_id FROM organization_draft_reservations WHERE company_id=?"
        }
        DRAFT_READ_LEVEL_LEAVE_DEFAULT => {
            "SELECT 1 FROM level_leave_defaults WHERE company_id=? AND level_id=?"
        }
        DRAFT_READ_LEAVE_DEFAULTS => {
            "SELECT level_id,version,policy_version_id FROM level_leave_defaults WHERE company_id=? ORDER BY level_id,version"
        }
        DRAFT_READ_NODE_CHANGE_STATE => {
            "SELECT state FROM organization_node_changes WHERE id=? AND company_id=?"
        }
        NODE_CREATE => {
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES(?,?,?,?,?,?,?,'active',1)"
        }
        NODE_RELOCATE_COPY => {
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) SELECT ?,kind,slug,?,?,company_id,?,'active',1 FROM organization_nodes WHERE id=? AND company_id=?"
        }
        NODE_ARCHIVE => {
            "UPDATE organization_nodes SET status='archived',hierarchy_revision=hierarchy_revision+1 WHERE id=? AND company_id=?"
        }
        NODE_POSITION_RELOCATE => {
            "UPDATE organization_positions SET branch_id=?,department_id=?,updated_at=? WHERE company_id=? AND department_id=? AND active=1"
        }
        RELOCATED_AUTHORITY_SEAT => {
            "INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) SELECT company_id,kind,?,1,position_id,person_id,effective_at FROM authority_seats WHERE company_id=? AND kind='head' AND scope_id=?"
        }
        RELOCATED_AUTHORITY_APPOINTMENT => {
            "INSERT INTO authority_appointments(id,company_id,kind,scope_id,version,position_id,person_id,effective_at,appointed_by,reason,provenance,source_reference,command_id,created_at) SELECT ?,company_id,'head',?,1,position_id,person_id,COALESCE(effective_at,CAST(? AS text)::bigint),?,'Carried forward during department relocation','explicit',?,?,? FROM authority_seats WHERE company_id=? AND kind='head' AND scope_id=?"
        }
        RELOCATED_DEPARTMENT_HEAD => {
            "INSERT INTO department_heads(department_id,company_id,person_id,version,updated_by,updated_at) SELECT ?,company_id,person_id,1,?,CAST(? AS text)::bigint FROM authority_seats WHERE company_id=? AND kind='head' AND scope_id=? AND person_id IS NOT NULL UNION ALL SELECT ?,legacy.company_id,legacy.person_id,1,?,CAST(? AS text)::bigint FROM department_heads legacy WHERE legacy.company_id=? AND legacy.department_id=? AND NOT EXISTS(SELECT 1 FROM authority_seats s WHERE s.company_id=legacy.company_id AND s.kind='head' AND s.scope_id=legacy.department_id)"
        }
        RELOCATED_FLOW_VERSION => {
            "INSERT INTO department_flow_versions(id,company_id,department_id,version,steps_json,published_by,reason,command_id,created_at) SELECT ?,company_id,?,1,steps_json,?,'Carried forward during department relocation',?,? FROM department_flow_versions WHERE company_id=? AND department_id=? ORDER BY version DESC LIMIT 1"
        }
        NODE_UPDATE => {
            "UPDATE organization_nodes SET display_name=?,status=COALESCE(?,status),hierarchy_revision=hierarchy_revision+1 WHERE id=? AND company_id=?"
        }
        CHANGE_REVIEW => {
            "UPDATE organization_node_changes SET state=?,version=?,reviewed_by=?,review_reason=?,reviewed_at=?,applied_at=?,updated_at=? WHERE id=? AND company_id=?"
        }
        CHANGE_EVENT_INSERT => {
            "INSERT INTO organization_node_change_events(id,change_id,company_id,action,actor_id,version,snapshot_json,created_at) VALUES(?,?,?,?,?,?,CAST(? AS text)::jsonb,?)"
        }
        CHANGE_SAVE => {
            "INSERT INTO organization_node_changes(id,company_id,target_node_id,action,node_kind,slug,display_name,branch_id,source_branch_id,expected_node_version,state,version,requested_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET target_node_id=excluded.target_node_id,action=excluded.action,node_kind=excluded.node_kind,slug=excluded.slug,display_name=excluded.display_name,branch_id=excluded.branch_id,source_branch_id=excluded.source_branch_id,expected_node_version=excluded.expected_node_version,state=excluded.state,version=excluded.version,reason=excluded.reason,reviewed_by=NULL,review_reason=NULL,reviewed_at=NULL,updated_at=excluded.updated_at"
        }
        CHANGE_CANCEL => {
            "UPDATE organization_node_changes SET state='cancelled',version=?,updated_at=? WHERE id=? AND company_id=?"
        }
        LIFECYCLE_POSITION_VERSION_STEP => {
            "UPDATE organization_positions SET version=version+1 WHERE company_id=? AND id=?"
        }
        PEOPLE_LIFECYCLE_DATA_NODES => {
            "SELECT id,display_name name,kind FROM organization_nodes WHERE company_id=? AND status='active' ORDER BY display_name,id"
        }
        LIFECYCLE_READ_SAVE_POSITION => {
            "SELECT level_id,branch_id,department_id FROM organization_positions WHERE company_id=? AND id=?"
        }
        LIFECYCLE_READ_REHIRE_POSITION => {
            "SELECT * FROM organization_positions WHERE company_id=? AND id=? AND active=1"
        }
        PROFILE_MEDIA_UPLOAD_AVATAR => {
            "INSERT INTO member_profiles(user_id,avatar_key,avatar_media_type,version,updated_at) VALUES(?,?,?,?,?)"
        }
        PROFILE_MEDIA_UPLOAD_COVER => {
            "INSERT INTO member_profiles(user_id,cover_key,cover_media_type,version,updated_at) VALUES(?,?,?,?,?)"
        }
        PROFILE_MEDIA_UPDATE_AVATAR => {
            "UPDATE member_profiles SET avatar_key=?,avatar_media_type=?,version=?,updated_at=? WHERE user_id=? AND version=?"
        }
        PROFILE_MEDIA_UPDATE_COVER => {
            "UPDATE member_profiles SET cover_key=?,cover_media_type=?,version=?,updated_at=? WHERE user_id=? AND version=?"
        }
        PROFILE_MEDIA_REMOVE_AVATAR => {
            "UPDATE member_profiles SET avatar_key=NULL,avatar_media_type=NULL,version=?,updated_at=? WHERE user_id=? AND version=?"
        }
        PROFILE_MEDIA_REMOVE_COVER => {
            "UPDATE member_profiles SET cover_key=NULL,cover_media_type=NULL,version=?,updated_at=? WHERE user_id=? AND version=?"
        }
        SPENDING_TEMPLATE_PUBLISH => {
            "INSERT INTO spending_approval_templates(company_id,version,exceptions_json,reason,updated_by,updated_at) VALUES(?,?,CAST(? AS text)::jsonb,?,?,?) ON CONFLICT(company_id) DO UPDATE SET version=excluded.version,exceptions_json=excluded.exceptions_json,reason=excluded.reason,updated_by=excluded.updated_by,updated_at=excluded.updated_at"
        }
        POSITION_PAY_DEFAULT_INSERT => {
            "INSERT INTO position_pay_defaults(id,company_id,position_id,version,pay_group_id,currency,monthly_amount_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)"
        }
        LEVEL_LEAVE_DEFAULT_INSERT => {
            "INSERT INTO level_leave_defaults(id,company_id,level_id,version,policy_version_id,created_by,created_at) VALUES(?,?,?,?,?,?,?)"
        }
        STAFF_SETTINGS_LIST_LEVELS => {
            "SELECT * FROM staff_levels WHERE company_id=? ORDER BY level_number ASC NULLS LAST,rank DESC,code"
        }
        STAFF_POLICY_REQUEST_ROUTE_HEAD => {
            "SELECT h.person_id,h.version FROM department_heads h JOIN organization_nodes n ON n.id=h.department_id WHERE h.company_id=? AND h.department_id=? AND n.company_id=h.company_id AND n.status='active' AND n.kind='department'"
        }
        PRESENTATION_DRAFT_COMPANY_NAME => {
            "SELECT display_name FROM organization_nodes WHERE id=? AND company_id=?"
        }
        PRESENTATION_ACTIVE_COMPANY_IDENTITY => {
            "SELECT slug,display_name FROM organization_nodes WHERE id=? AND kind='company' AND status='active'"
        }
        PRESENTATION_ACTIVE_COMPANY_SLUG => {
            "SELECT slug FROM organization_nodes WHERE id=? AND kind='company' AND status='active'"
        }
        PRESENTATION_ACTIVE_COMPANY_NAME => {
            "SELECT display_name FROM organization_nodes WHERE id=? AND kind='company' AND status='active'"
        }
        PRESENTATION_SCOPED_BRANCH_NAMES => {
            "SELECT display_name FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active' AND (?::text IS NULL OR id=?) ORDER BY display_name"
        }
        PRESENTATION_LEAVE_DEFAULT_LEVELS => {
            "SELECT id,name,level_number FROM staff_levels WHERE company_id=? AND active=1 AND level_number IS NOT NULL ORDER BY level_number"
        }
        ORG_READROSTERPEOPLE => {
            "SELECT p.id person_id,p.user_id,p.legal_name,p.email,p.version person_version,p.status person_status,e.id engagement_id,e.version engagement_version,e.company_id,e.branch_id,e.department_id,e.position_id,e.manager_person_id,e.level_id,l.level_number,l.name level_name,e.status,e.start_date,e.end_date,e.job_title FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id LEFT JOIN (SELECT \"id\",\"company_id\",\"code\",\"name\",\"rank\",\"active\",\"version\",\"created_at\",\"updated_at\",\"level_number\" FROM staff_levels) l ON l.id=e.level_id AND l.company_id=e.company_id AND l.active=1 WHERE p.company_id=? AND p.merged_into_id IS NULL AND p.status!='archived' AND e.worker_type='employee' AND e.status!='ended' ORDER BY p.legal_name,e.id"
        }
        ORG_READROSTERPOSITIONS => {
            "SELECT p.*,l.level_number FROM organization_positions p JOIN staff_levels l ON l.id=p.level_id AND l.company_id=p.company_id WHERE p.company_id=? ORDER BY l.level_number,p.title,p.id"
        }
        ACCEPTANCE_READ_COMPANY => {
            "SELECT id,display_name,status FROM organization_nodes WHERE id=? AND company_id=? AND kind='company'"
        }
        ACCEPTANCE_READ_BRANCHES => {
            "SELECT count(*) count FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active'"
        }
        ACCEPTANCE_READ_CEO => {
            "SELECT count(*) count FROM organization_positions WHERE company_id=? AND active=1 AND is_ceo=1"
        }
        ACCEPTANCE_READ_CURRENT => {
            "SELECT * FROM company_business_acceptance WHERE company_id=?"
        }
        ACCEPTANCE_READ_EVENTS => {
            "SELECT id,action,actor_id,version,reason,created_at FROM company_business_acceptance_events WHERE company_id=? ORDER BY version,created_at,id"
        }
        ACCEPTANCE_SUBMIT_VERSION_CHECK => {
            "EXISTS(SELECT 1 FROM company_business_acceptance WHERE company_id=? AND version=? AND status IN ('draft','rejected','revoked'))"
        }
        ACCEPTANCE_REVIEW_VERSION_CHECK => {
            "EXISTS(SELECT 1 FROM company_business_acceptance WHERE company_id=? AND version=? AND status='submitted')"
        }
        ACCEPTANCE_REVOKE_VERSION_CHECK => {
            "EXISTS(SELECT 1 FROM company_business_acceptance WHERE company_id=? AND version=? AND status='accepted')"
        }
        COMPANY_ACCEPTANCE_RESULT_PROJECTION => {
            "SELECT 'company-acceptance-result-'||?||'-'||requested_by,company_id,requested_by,'company_business_acceptance',company_id,version,?,? FROM company_business_acceptance WHERE company_id=?"
        }
        PROFILE_ACTIVE_DEPARTMENT => {
            "SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='department' AND status='active'"
        }
        WORKSPACE_ROUTE_COMPANY => {
            "SELECT c.id,c.slug FROM organization_nodes c WHERE c.kind='company' AND c.status='active' AND (?::text IS NULL OR EXISTS(SELECT 1 FROM organization_nodes b WHERE b.company_id=c.id AND b.parent_id=c.id AND b.kind='branch' AND b.status='active' AND b.slug=?)) AND (?::text IS NULL OR EXISTS(SELECT 1 FROM organization_nodes d WHERE d.company_id=c.id AND d.kind='department' AND d.status='active' AND d.slug=? AND ((?::text IS NULL AND d.branch_id IS NULL AND d.parent_id=c.id) OR (?::text IS NOT NULL AND EXISTS(SELECT 1 FROM organization_nodes b2 WHERE b2.company_id=c.id AND b2.parent_id=c.id AND b2.kind='branch' AND b2.status='active' AND b2.slug=? AND d.branch_id=b2.id AND d.parent_id=b2.id))))) AND (?::text IS NULL OR c.slug=? OR c.id=?) LIMIT 1"
        }
        PUBLIC_MARKETING_RECEIVING_BRANCH => {
            "SELECT id FROM organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters') AND status='active' ORDER BY id LIMIT 1"
        }
        WORK_REPORT_ORGANIZATION_NODES => {
            "SELECT id,display_name FROM organization_nodes WHERE company_id=?"
        }
        SPENDING_ACTIVE_POSITION => {
            "SELECT 1 FROM organization_positions WHERE id=? AND company_id=? AND active=1"
        }
        PMS_ROLLOUT_ACTIVE_BRANCH => {
            "SELECT 1 FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active' LIMIT 1"
        }
        CALENDAR_ACCESS_ORGANIZATION_NODES => {
            "SELECT id,parent_id \"parentId\",display_name name,kind,branch_id \"branchId\" FROM organization_nodes WHERE company_id=? ORDER BY id LIMIT 5001"
        }
        FINANCE_STARTER_ORGANIZATION_NODES => {
            "SELECT id,kind,display_name FROM organization_nodes WHERE company_id=? AND kind IN ('branch','department') AND status='active' ORDER BY kind,id"
        }
        _ => return None,
    })
}

fn sql_for(kind: &str, values: &[Value]) -> Option<String> {
    if kind == ATTACHMENT_OWNER_VERSION_STEP {
        let table = values.first()?.as_str()?;
        if !ATTACHMENT_VERSION_TABLES.contains(&table) {
            return None;
        }
        // Table is an allow-listed identifier, never caller SQL text. The
        // leading table value is consumed by the executor below.
        return Some(format!(
            "UPDATE {table} SET version=version+1,updated_at=? WHERE id=? AND version=?"
        ));
    }
    sql(kind).map(str::to_owned)
}

/// Actor snapshot supplied by composition for receipt/audit/outbox writes.
#[derive(Clone, Debug)]
pub struct OwnerWriteActor {
    pub user_id: String,
    pub assignment_id: String,
    pub company_id: CompanyId,
    pub authority_json: String,
}

/// Result of one owner write. `statement` mode returns the authoritative owner
/// SQL for a caller that must execute it inside its own cross-owner unit; no
/// ledger is written in that mode because the caller's command owns it.
#[derive(Clone, Debug, serde::Serialize)]
pub struct OwnerWriteResult {
    pub kind: String,
    pub rows: u64,
    pub version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<Value>,
}

/// Resolve the authoritative statement without executing it. Used only by the
/// fail-closed `statement` mode for cross-owner transactional effects.
pub fn owner_statement(kind: &str, values: &[Value]) -> Result<(String, Vec<Value>), Error> {
    if !supported(kind) || values.is_empty() || values.len() > 64 {
        return Err(invalid().into());
    }
    let statement = sql_for(kind, values).ok_or_else(invalid)?;
    let mut bound = values.to_vec();
    if kind == ATTACHMENT_OWNER_VERSION_STEP {
        bound.remove(0);
    }
    if bound.len() != statement.matches('?').count() {
        return Err(invalid().into());
    }
    Ok((statement, bound))
}

async fn ready(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("ORGANIZATION_OWNER_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("ORGANIZATION_OWNER_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Translate `?` placeholders to positional `$N` parameters.
fn positional(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 8);
    let mut index = 0usize;
    for ch in sql.chars() {
        if ch == '?' {
            index += 1;
            out.push('$');
            out.push_str(&index.to_string());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Bind caller values in order. JSON objects/arrays are bound as JSON text;
/// the owner SQL casts them where a jsonb column is expected.
fn arguments(values: &[Value]) -> Result<sqlx::postgres::PgArguments, Error> {
    let mut args = sqlx::postgres::PgArguments::default();
    for value in values {
        match value {
            Value::Null => args.add(Option::<String>::None).map_err(|_| invalid())?,
            Value::Bool(flag) => args.add(*flag).map_err(|_| invalid())?,
            Value::Number(number) => {
                if let Some(integer) = number.as_i64() {
                    args.add(integer).map_err(|_| invalid())?;
                } else if let Some(unsigned) = number.as_u64() {
                    args.add(unsigned as i64).map_err(|_| invalid())?;
                } else if let Some(float) = number.as_f64() {
                    args.add(float).map_err(|_| invalid())?;
                } else {
                    return Err(invalid().into());
                }
            }
            Value::String(text) => args.add(text.clone()).map_err(|_| invalid())?,
            Value::Array(_) | Value::Object(_) => {
                args.add(serde_json::to_string(value).map_err(|_| invalid())?)
                    .map_err(|_| invalid())?;
            }
        }
    }
    Ok(args)
}

async fn ledger(
    connection: &mut PgConnection,
    actor: &OwnerWriteActor,
    kind: &str,
    key: &str,
    rows: u64,
    now: i64,
) -> Result<(), Error> {
    let result = serde_json::json!({"kind": kind, "rows": rows}).to_string();
    let changed = sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
         result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
         ON CONFLICT(command_id) DO NOTHING",
    )
    .bind(key)
    .bind(format!("organization.owner-write.{kind}"))
    .bind(&actor.user_id)
    .bind(key)
    .bind(actor.company_id.as_str())
    .bind(result)
    .bind(now)
    .bind(&actor.authority_json)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "IDEMPOTENCY_CONFLICT",
            "This command was used for a different organization change.",
        )
        .into());
    }
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
         record_id,record_version,purpose,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$5,$9,$10) ON CONFLICT(id) DO NOTHING",
    )
    .bind(format!("organization-owner-audit-{key}"))
    .bind(key)
    .bind(&actor.user_id)
    .bind(&actor.assignment_id)
    .bind(format!("organization.owner-write.{kind}"))
    .bind("organization_owner_write")
    .bind(actor.company_id.as_str())
    .bind(1_i64)
    .bind(now)
    .bind(&actor.authority_json)
    .execute(&mut *connection)
    .await?;
    let authority: Value = serde_json::from_str(&actor.authority_json).unwrap_or(Value::Null);
    let payload = serde_json::json!({
        "authority": authority,
        "kind": kind,
        "companyId": actor.company_id.as_str(),
        "rows": rows,
    })
    .to_string();
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
         aggregate_version,payload_json,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
    )
    .bind(format!("organization-owner-outbox-{key}"))
    .bind(key)
    .bind(format!("organization.owner-write.{kind}"))
    .bind("organization_owner_write")
    .bind(actor.company_id.as_str())
    .bind(1_i64)
    .bind(payload)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// Execute one allow-listed owner write on the caller-owned serializable unit.
/// Only `values` are caller supplied; the SQL is always selected here.
pub async fn execute_owner_write(
    connection: &mut PgConnection,
    company: &CompanyId,
    actor: &OwnerWriteActor,
    kind: &str,
    values: &[Value],
    key: &str,
    now: i64,
) -> Result<OwnerWriteResult, Error> {
    ready(connection).await?;
    if !supported(kind) || values.is_empty() || values.len() > 64 {
        return Err(invalid().into());
    }
    let statement = sql_for(kind, values).ok_or_else(invalid)?;
    let mut bound = values.to_vec();
    if kind == ATTACHMENT_OWNER_VERSION_STEP {
        bound.remove(0); // allow-listed table identifier is consumed here
    }
    let expected = statement.matches('?').count();
    if bound.len() != expected {
        return Err(invalid().into());
    }
    let args = arguments(&bound)?;
    let sql = positional(&statement);
    let rows = sqlx::query_with(sqlx::AssertSqlSafe(sql), args)
        .execute(&mut *connection)
        .await?
        .rows_affected();
    ledger(connection, actor, kind, key, rows, now).await?;
    Ok(OwnerWriteResult {
        kind: kind.to_owned(),
        rows,
        version: 1,
        sql: None,
        values: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_parameterized_sql_and_is_allow_listed() {
        for kind in KINDS {
            assert!(supported(kind), "kind must be allow listed: {kind}");
            if *kind == ATTACHMENT_OWNER_VERSION_STEP {
                continue;
            }
            let statement = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(statement.contains('?'), "write must be parameterized: {kind}");
        }
    }

    #[test]
    fn unknown_kinds_are_not_supported() {
        assert!(!supported("not_an_owner_write"));
        assert!(sql("not_an_owner_write").is_none());
    }

    #[test]
    fn positional_translation_preserves_casts() {
        assert_eq!(
            positional("UPDATE t SET j=?::jsonb WHERE id=?"),
            "UPDATE t SET j=$1::jsonb WHERE id=$2"
        );
    }

    #[test]
    fn attachment_table_is_allow_listed() {
        assert!(sql_for(ATTACHMENT_OWNER_VERSION_STEP, &[Value::from("organization_node_changes")]).is_some());
        assert!(sql_for(ATTACHMENT_OWNER_VERSION_STEP, &[Value::from("secret_table")]).is_none());
    }

    /// Draft/presentation company-identity reads are native-owned, read-only
    /// and Company scoped. Permission checks stay in the caller; transport only
    /// selects the statement text and binds caller-supplied positional values.
    #[test]
    fn presentation_read_kinds_are_select_only_and_company_scoped() {
        let kinds = [
            (PRESENTATION_DRAFT_COMPANY_NAME, 2usize),
            (PRESENTATION_ACTIVE_COMPANY_IDENTITY, 1),
            (PRESENTATION_ACTIVE_COMPANY_SLUG, 1),
            (PRESENTATION_ACTIVE_COMPANY_NAME, 1),
            (PRESENTATION_SCOPED_BRANCH_NAMES, 3),
            (PRESENTATION_LEAVE_DEFAULT_LEVELS, 1),
        ];
        for (kind, expected) in kinds {
            assert!(supported(kind), "read kind must be allow listed: {kind}");
            let statement = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(
                statement.starts_with("SELECT "),
                "read kind must be SELECT-only: {kind}"
            );
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
                assert!(
                    !statement.contains(forbidden),
                    "read kind must not mutate: {kind} contains {forbidden}"
                );
            }
            assert!(
                statement.contains("company_id=?") || statement.contains("WHERE id=?"),
                "read kind must be Company scoped: {kind}"
            );
            assert_eq!(
                statement.matches('?').count(),
                expected,
                "read kind parameter count: {kind}"
            );
        }
        // The Company key is always the first bound value, so the statement
        // remains a Company-keyed native read even when the SQL uses `id`.
        let (sql, values) = owner_statement(
            PRESENTATION_DRAFT_COMPANY_NAME,
            &[Value::from("company-a"), Value::from("company-b")],
        )
        .expect("draft company name statement");
        assert_eq!(values.len(), 2);
        assert!(sql.contains("company_id=?"));
        assert!(owner_statement(PRESENTATION_DRAFT_COMPANY_NAME, &[Value::from("company-a")]).is_err());
    }

    /// The retired roster read port is read-only: fixed `SELECT` statements
    /// with Company scope, never a mutation. The retained `organizationRoster`
    /// permission checks stay in the TypeScript application; transport only
    /// selects the statement text.
    #[test]
    fn roster_read_kinds_are_select_only() {
        for kind in [ORG_READROSTERPEOPLE, ORG_READROSTERPOSITIONS] {
            assert!(supported(kind), "read kind must be allow listed: {kind}");
            let statement =
                sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(
                statement.starts_with("SELECT "),
                "read kind must be SELECT-only: {kind}"
            );
            for forbidden in ["INSERT", "UPDATE", "DELETE", "RETURNING", ";"] {
                assert!(
                    !statement.contains(forbidden),
                    "read kind must not mutate: {kind} contains {forbidden}"
                );
            }
            assert_eq!(
                statement.matches('?').count(),
                1,
                "roster read binds exactly the Company: {kind}"
            );
        }
    }

    /// Draft orchestration reads are native-owned, SELECT-only and Company
    /// keyed. The TypeScript application keeps every permission check; the
    /// transport only selects the statement text and binds caller values.
    #[test]
    fn draft_orchestration_read_kinds_are_select_only_and_company_scoped() {
        let kinds = [
            (DRAFT_READ_ACTIVE, 2usize),
            (DRAFT_READ_BY_ID, 3),
            (DRAFT_READ_RECENT, 2),
            (DRAFT_READ_PENDING_POSITIONS, 1),
            (DRAFT_READ_RESERVATIONS, 1),
            (DRAFT_READ_LEVEL_LEAVE_DEFAULT, 2),
            (DRAFT_READ_LEAVE_DEFAULTS, 1),
            (DRAFT_READ_NODE_CHANGE_STATE, 2),
        ];
        for (kind, expected) in kinds {
            assert!(supported(kind), "draft read kind must be allow listed: {kind}");
            let statement = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(
                statement.starts_with("SELECT "),
                "draft read must be SELECT-only: {kind}"
            );
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
                assert!(
                    !statement.contains(forbidden),
                    "draft read must not mutate or lock: {kind} contains {forbidden}"
                );
            }
            assert!(
                statement.contains("company_id=?"),
                "draft read must be Company scoped: {kind}"
            );
            assert_eq!(
                statement.matches('?').count(),
                expected,
                "draft read parameter count: {kind}"
            );
        }
        let (statement, values) = owner_statement(
            DRAFT_READ_BY_ID,
            &[Value::from("company"), Value::from("owner"), Value::from("draft")],
        )
        .expect("draft read by id statement");
        assert_eq!(values.len(), 3);
        assert!(statement.contains("company_id=?"));
        assert!(owner_statement(DRAFT_READ_ACTIVE, &[Value::from("company")]).is_err());
    }

    /// Company setup acceptance reads are native-owned, SELECT-only and
    /// Company keyed. The TypeScript application keeps every authority and
    /// scope check; transport only selects the statement text and binds the
    /// caller's Company values.
    #[test]
    fn acceptance_read_kinds_are_select_only_and_company_scoped() {
        let kinds = [
            (ACCEPTANCE_READ_COMPANY, 2usize),
            (ACCEPTANCE_READ_BRANCHES, 1),
            (ACCEPTANCE_READ_CEO, 1),
            (ACCEPTANCE_READ_CURRENT, 1),
            (ACCEPTANCE_READ_EVENTS, 1),
        ];
        for (kind, expected) in kinds {
            assert!(supported(kind), "acceptance read kind must be allow listed: {kind}");
            let statement =
                sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(
                statement.starts_with("SELECT "),
                "acceptance read must be SELECT-only: {kind}"
            );
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
                assert!(
                    !statement.contains(forbidden),
                    "acceptance read must not mutate or lock: {kind} contains {forbidden}"
                );
            }
            assert!(
                statement.contains("company_id=?"),
                "acceptance read must be Company scoped: {kind}"
            );
            assert_eq!(
                statement.matches('?').count(),
                expected,
                "acceptance read parameter count: {kind}"
            );
        }
        let (statement, values) = owner_statement(
            ACCEPTANCE_READ_COMPANY,
            &[Value::from("company"), Value::from("company")],
        )
        .expect("acceptance company statement");
        assert_eq!(values.len(), 2);
        assert!(statement.contains("company_id=?"));
        assert!(owner_statement(ACCEPTANCE_READ_CURRENT, &[Value::from("company")]).is_ok());
        assert!(owner_statement(ACCEPTANCE_READ_CURRENT, &[]).is_err());
    }

    /// The three version assertion predicates retired from
    /// `company-business-acceptance.ts` are native-owned, SELECT-only,
    /// Company-keyed and carry the exact `EXISTS(SELECT ...)` text the workflow
    /// command-assertion layer stores verbatim. They must not mutate, lock, or
    /// duplicate any cross-owner source.
    #[test]
    fn acceptance_version_check_kinds_are_select_only_company_scoped_and_cast_safe() {
        let kinds = [
            (
                ACCEPTANCE_SUBMIT_VERSION_CHECK,
                "EXISTS(SELECT 1 FROM company_business_acceptance WHERE company_id=? AND version=? AND status IN ('draft','rejected','revoked'))",
            ),
            (
                ACCEPTANCE_REVIEW_VERSION_CHECK,
                "EXISTS(SELECT 1 FROM company_business_acceptance WHERE company_id=? AND version=? AND status='submitted')",
            ),
            (
                ACCEPTANCE_REVOKE_VERSION_CHECK,
                "EXISTS(SELECT 1 FROM company_business_acceptance WHERE company_id=? AND version=? AND status='accepted')",
            ),
        ];
        assert_eq!(kinds.len(), 3);
        for (kind, expected) in kinds {
            assert!(supported(kind), "version check kind must be allow listed: {kind}");
            let statement = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert_eq!(statement, expected, "version check SQL must match the retired literal: {kind}");
            assert!(statement.starts_with("EXISTS(SELECT "), "version check must be SELECT-only: {kind}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
                assert!(!statement.contains(forbidden), "version check must not mutate: {kind} contains {forbidden}");
            }
            assert!(statement.contains("company_id=?"), "version check must be Company scoped: {kind}");
            assert_eq!(statement.matches('?').count(), 2, "version check parameter count: {kind}");
            assert!(owner_statement(kind, &[Value::from("company"), Value::from(1_i64)]).is_ok());
            assert!(owner_statement(kind, &[Value::from("company")]).is_err());
            assert!(owner_statement(kind, &[]).is_err());
        }
    }

    /// The projection retired from `company-acceptance-result-projection.ts`
    /// is native-owned, SELECT-only and Company keyed. The TypeScript module
    /// no longer exports SQL text; it sends only the typed kind and the
    /// caller's positional values through `nativeOrgAll`.
    #[test]
    fn company_acceptance_result_projection_kind_is_select_only_and_company_scoped() {
        let expected = "SELECT 'company-acceptance-result-'||?||'-'||requested_by,company_id,requested_by,'company_business_acceptance',company_id,version,?,? FROM company_business_acceptance WHERE company_id=?";
        assert!(supported(COMPANY_ACCEPTANCE_RESULT_PROJECTION));
        assert_eq!(
            sql(COMPANY_ACCEPTANCE_RESULT_PROJECTION).unwrap(),
            expected,
            "projection SQL must match the retired literal"
        );
        assert!(expected.starts_with("SELECT "));
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
            assert!(!expected.contains(forbidden), "projection must not mutate: {forbidden}");
        }
        assert!(expected.contains("company_id=?"), "projection must be Company scoped");
        assert_eq!(expected.matches('?').count(), 4);
        let (statement, values) = owner_statement(
            COMPANY_ACCEPTANCE_RESULT_PROJECTION,
            &[
                Value::from("key"),
                Value::from(1_i64),
                Value::from(2_i64),
                Value::from("company"),
            ],
        )
        .expect("projection statement");
        assert_eq!(statement, expected);
        assert_eq!(values.len(), 4);
        assert!(positional(expected).contains("company_id=$4"));
        assert!(owner_statement(
            COMPANY_ACCEPTANCE_RESULT_PROJECTION,
            &[Value::from("key")],
        )
        .is_err());
    }

    /// The active-department fact retired from `profile-facts.ts` is a
    /// Company-keyed SELECT; the boolean reduction stays in the caller.
    #[test]
    fn profile_active_department_kind_is_select_only_and_company_scoped() {
        let expected = "SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='department' AND status='active'";
        assert!(supported(PROFILE_ACTIVE_DEPARTMENT));
        assert_eq!(sql(PROFILE_ACTIVE_DEPARTMENT).unwrap(), expected);
        assert!(expected.starts_with("SELECT "));
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
            assert!(!expected.contains(forbidden), "fact must not mutate: {forbidden}");
        }
        assert!(expected.contains("company_id=?"), "fact must be Company scoped");
        assert_eq!(expected.matches('?').count(), 2);
        let (statement, values) = owner_statement(
            PROFILE_ACTIVE_DEPARTMENT,
            &[Value::from("department"), Value::from("company")],
        )
        .expect("active department statement");
        assert_eq!(statement, expected);
        assert_eq!(values.len(), 2);
        assert_eq!(positional(expected), "SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 AND kind='department' AND status='active'");
        assert!(owner_statement(PROFILE_ACTIVE_DEPARTMENT, &[Value::from("department")]).is_err());
    }

    /// The dynamic structural route retired from `workspace-route-company.ts`
    /// is one fixed SELECT-only statement. Optional branch/department/company
    /// slugs are bound as NULL, which disables the matching join window without
    /// changing SQL text; the caller keeps the current access check.
    #[test]
    fn workspace_route_company_kind_is_select_only_company_scoped_and_null_safe() {
        let expected = sql(WORKSPACE_ROUTE_COMPANY)
            .unwrap_or_else(|| panic!("missing SQL for {WORKSPACE_ROUTE_COMPANY}"));
        assert!(supported(WORKSPACE_ROUTE_COMPANY));
        assert!(expected.starts_with("SELECT "));
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
            assert!(!expected.contains(forbidden), "route read must not mutate: {forbidden}");
        }
        assert!(expected.contains("company_id=c.id"), "route read must scope by Company");
        assert!(!expected.contains("hr_people") && !expected.contains("hr_engagements"));
        assert_eq!(expected.matches('?').count(), 10);
        // Null optional slugs keep the same statement: the `?::text IS NULL`
        // windows short-circuit each join without changing the SQL text.
        let nulls: Vec<Value> = std::iter::repeat_with(|| Value::Null).take(10).collect();
        let (statement, values) =
            owner_statement(WORKSPACE_ROUTE_COMPANY, &nulls).expect("null-slug route statement");
        assert_eq!(statement, expected);
        assert_eq!(values.len(), 10);
        assert!(positional(expected).contains("$1::text IS NULL"));
        assert!(positional(expected).contains("$8::text IS NULL OR c.slug=$9 OR c.id=$10"));
        let full = [
            Value::from("branch"),
            Value::from("branch"),
            Value::from("department"),
            Value::from("department"),
            Value::from("branch"),
            Value::from("branch"),
            Value::from("branch"),
            Value::from("company"),
            Value::from("company"),
            Value::from("company"),
        ];
        assert!(owner_statement(WORKSPACE_ROUTE_COMPANY, &full).is_ok());
        assert!(owner_statement(WORKSPACE_ROUTE_COMPANY, &nulls[..9]).is_err());
    }

    /// The public-form receiving-branch read retired from
    /// `public-marketing-receiving-branch.ts` is native-owned, SELECT-only,
    /// Company keyed and owner-local. Its single marketing consumer keeps the
    /// form authority check; the typed kind sends only the Company value.
    #[test]
    fn public_marketing_receiving_branch_kind_is_select_only_and_company_scoped() {
        let expected = "SELECT id FROM organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters') AND status='active' ORDER BY id LIMIT 1";
        assert!(supported(PUBLIC_MARKETING_RECEIVING_BRANCH));
        assert_eq!(
            sql(PUBLIC_MARKETING_RECEIVING_BRANCH).unwrap(),
            expected,
            "receiving-branch SQL must match the retired literal"
        );
        assert!(expected.starts_with("SELECT "));
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
            assert!(!expected.contains(forbidden), "receiving-branch read must not mutate: {forbidden}");
        }
        assert!(expected.contains("company_id=?"), "receiving-branch read must be Company scoped");
        assert!(expected.contains("organization_nodes"), "receiving-branch read must stay owner-local");
        assert!(!expected.contains("hr_people") && !expected.contains("hr_engagements"));
        assert_eq!(expected.matches('?').count(), 1);
        let (statement, values) = owner_statement(
            PUBLIC_MARKETING_RECEIVING_BRANCH,
            &[Value::from("company")],
        )
        .expect("receiving-branch statement");
        assert_eq!(statement, expected);
        assert_eq!(values.len(), 1);
        assert_eq!(positional(expected), "SELECT id FROM organization_nodes WHERE company_id=$1 AND kind IN ('branch','headquarters') AND status='active' ORDER BY id LIMIT 1");
        assert!(owner_statement(PUBLIC_MARKETING_RECEIVING_BRANCH, &[]).is_err());
    }

    /// The work-report node list retired from `work-report-facts.ts` is
    /// native-owned, SELECT-only and Company keyed. Its single work_projection
    /// consumer keeps its own report authority checks and receives the same
    /// `{results}` shape through the typed kind.
    #[test]
    fn work_report_organization_nodes_kind_is_select_only_and_company_scoped() {
        let expected = "SELECT id,display_name FROM organization_nodes WHERE company_id=?";
        assert!(supported(WORK_REPORT_ORGANIZATION_NODES));
        assert_eq!(
            sql(WORK_REPORT_ORGANIZATION_NODES).unwrap(),
            expected,
            "work-report nodes SQL must match the retired literal"
        );
        assert!(expected.starts_with("SELECT "));
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
            assert!(!expected.contains(forbidden), "work-report nodes read must not mutate: {forbidden}");
        }
        assert!(expected.contains("company_id=?"), "work-report nodes read must be Company scoped");
        assert!(expected.contains("organization_nodes"), "work-report nodes read must stay owner-local");
        assert_eq!(expected.matches('?').count(), 1);
        let (statement, values) =
            owner_statement(WORK_REPORT_ORGANIZATION_NODES, &[Value::from("company")])
                .expect("work-report nodes statement");
        assert_eq!(statement, expected);
        assert_eq!(values.len(), 1);
        assert_eq!(positional(expected), "SELECT id,display_name FROM organization_nodes WHERE company_id=$1");
        assert!(owner_statement(WORK_REPORT_ORGANIZATION_NODES, &[]).is_err());
    }

    /// The active-spending-position fact retired from `spending-position.ts`
    /// is native-owned, SELECT-only and Company keyed over the org-owned
    /// `organization_positions` table. The boolean reduction stays in the
    /// TypeScript caller, which sends only the typed kind and positional values.
    #[test]
    fn spending_active_position_kind_is_select_only_and_company_scoped() {
        let expected = "SELECT 1 FROM organization_positions WHERE id=? AND company_id=? AND active=1";
        assert!(supported(SPENDING_ACTIVE_POSITION));
        assert_eq!(
            sql(SPENDING_ACTIVE_POSITION).unwrap(),
            expected,
            "spending position SQL must match the retired literal"
        );
        assert!(expected.starts_with("SELECT "));
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
            assert!(!expected.contains(forbidden), "spending position read must not mutate: {forbidden}");
        }
        assert!(expected.contains("company_id=?"), "spending position read must be Company scoped");
        assert!(expected.contains("organization_positions"), "spending position read must stay owner-local");
        assert!(!expected.contains("hr_people") && !expected.contains("hr_engagements"));
        assert_eq!(expected.matches('?').count(), 2);
        let (statement, values) = owner_statement(
            SPENDING_ACTIVE_POSITION,
            &[Value::from("position"), Value::from("company")],
        )
        .expect("spending position statement");
        assert_eq!(statement, expected);
        assert_eq!(values.len(), 2);
        assert_eq!(positional(expected), "SELECT 1 FROM organization_positions WHERE id=$1 AND company_id=$2 AND active=1");
        assert!(owner_statement(SPENDING_ACTIVE_POSITION, &[Value::from("position")]).is_err());
        assert!(owner_statement(SPENDING_ACTIVE_POSITION, &[]).is_err());
    }

    /// The PMS rollout active-branch fact retired from `pms-rollout-facts.ts`
    /// is native-owned, SELECT-only and Company keyed over the org-owned
    /// `organization_nodes` table. The boolean reduction stays in the
    /// TypeScript consumer, which sends only the typed kind and Company value.
    #[test]
    fn pms_rollout_active_branch_kind_is_select_only_and_company_scoped() {
        let expected = "SELECT 1 FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active' LIMIT 1";
        assert!(supported(PMS_ROLLOUT_ACTIVE_BRANCH));
        assert_eq!(
            sql(PMS_ROLLOUT_ACTIVE_BRANCH).unwrap(),
            expected,
            "PMS rollout SQL must match the retired literal"
        );
        assert!(expected.starts_with("SELECT "));
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
            assert!(!expected.contains(forbidden), "PMS rollout read must not mutate: {forbidden}");
        }
        assert!(expected.contains("company_id=?"), "PMS rollout read must be Company scoped");
        assert!(expected.contains("organization_nodes"), "PMS rollout read must stay owner-local");
        assert_eq!(expected.matches('?').count(), 1);
        let (statement, values) =
            owner_statement(PMS_ROLLOUT_ACTIVE_BRANCH, &[Value::from("company")])
                .expect("PMS rollout statement");
        assert_eq!(statement, expected);
        assert_eq!(values.len(), 1);
        assert_eq!(positional(expected), "SELECT 1 FROM organization_nodes WHERE company_id=$1 AND kind='branch' AND status='active' LIMIT 1");
        assert!(owner_statement(PMS_ROLLOUT_ACTIVE_BRANCH, &[]).is_err());
    }

    /// The calendar-access organization-node list retired from
    /// `calendar-access-facts.ts` is native-owned, SELECT-only and Company
    /// keyed over the org-owned `organization_nodes` table. The work_projection
    /// consumer keeps its own access checks and receives the same `CalendarNode[]`
    /// row-list shape through the typed kind.
    #[test]
    fn calendar_access_organization_nodes_kind_is_select_only_and_company_scoped() {
        let expected = "SELECT id,parent_id \"parentId\",display_name name,kind,branch_id \"branchId\" FROM organization_nodes WHERE company_id=? ORDER BY id LIMIT 5001";
        assert!(supported(CALENDAR_ACCESS_ORGANIZATION_NODES));
        assert_eq!(
            sql(CALENDAR_ACCESS_ORGANIZATION_NODES).unwrap(),
            expected,
            "calendar-access nodes SQL must match the retired literal"
        );
        assert!(expected.starts_with("SELECT "));
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
            assert!(!expected.contains(forbidden), "calendar-access nodes read must not mutate: {forbidden}");
        }
        assert!(expected.contains("company_id=?"), "calendar-access nodes read must be Company scoped");
        assert!(expected.contains("organization_nodes"), "calendar-access nodes read must stay owner-local");
        assert!(expected.contains("ORDER BY id LIMIT 5001"), "calendar-access nodes read must keep its order and bound");
        assert_eq!(expected.matches('?').count(), 1);
        let (statement, values) =
            owner_statement(CALENDAR_ACCESS_ORGANIZATION_NODES, &[Value::from("company")])
                .expect("calendar-access nodes statement");
        assert_eq!(statement, expected);
        assert_eq!(values.len(), 1);
        assert_eq!(positional(expected), "SELECT id,parent_id \"parentId\",display_name name,kind,branch_id \"branchId\" FROM organization_nodes WHERE company_id=$1 ORDER BY id LIMIT 5001");
        assert!(owner_statement(CALENDAR_ACCESS_ORGANIZATION_NODES, &[]).is_err());
    }

    /// Owner-local reads retired from `department-authority-reads.ts`: ten
    /// fixed, Company-keyed SELECT predicates, never a mutation. The three
    /// cross-owner reads (hr_people/hr_engagements from people_payroll) are
    /// deliberately absent: this owner must not duplicate that source.
    #[test]
    fn department_authority_read_kinds_are_select_only_and_company_scoped() {
        let kinds = [
            (DEPT_AUTH_READ_SEAT, 3usize),
            (DEPT_AUTH_READ_HISTORY, 1),
            (DEPT_AUTH_READ_POLICIES, 1),
            (DEPT_AUTH_READ_DEPARTMENTS, 1),
            (DEPT_AUTH_READ_DEPARTMENT_ACTIVE, 2),
            (DEPT_AUTH_READ_POSITION_VALID, 5),
            (DEPT_AUTH_READ_FLOW_VERSION, 2),
            (DEPT_AUTH_READ_POLICY, 2),
            (DEPT_AUTH_READ_LEGACY_HEAD, 2),
            (DEPT_AUTH_READ_POLICY_CHECK, 3),
        ];
        assert_eq!(kinds.len(), 10);
        for (kind, expected) in kinds {
            assert!(supported(kind), "read kind must be allow listed: {kind}");
            let statement = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(
                statement.starts_with("SELECT ") || statement.starts_with("EXISTS(SELECT "),
                "read kind must be SELECT-only: {kind}"
            );
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
                assert!(
                    !statement.contains(forbidden),
                    "read kind must not mutate: {kind} contains {forbidden}"
                );
            }
            assert!(
                statement.contains("company_id=?"),
                "read kind must be Company scoped: {kind}"
            );
            assert!(
                !statement.contains("hr_people") && !statement.contains("hr_engagements"),
                "owner-local read must not duplicate cross-owner people sources: {kind}"
            );
            assert_eq!(
                statement.matches('?').count(),
                expected,
                "read kind parameter count: {kind}"
            );
        }
        let (statement, values) = owner_statement(
            DEPT_AUTH_READ_SEAT,
            &[
                Value::from("company"),
                Value::from("head"),
                Value::from("department"),
            ],
        )
        .expect("seat statement");
        assert_eq!(values.len(), 3);
        assert!(statement.contains("company_id=?"));
        assert!(owner_statement(DEPT_AUTH_READ_SEAT, &[Value::from("company")]).is_err());
        // The deferred policy predicate keeps the exact EXISTS form the
        // cross-owner workflow assertion wraps in CASE WHEN (<predicate>).
        assert_eq!(
            sql(DEPT_AUTH_READ_POLICY_CHECK).unwrap(),
            "EXISTS(SELECT 1 FROM department_flow_versions WHERE id=? AND company_id=? AND version=?)"
        );
    }

    /// Owner-local reads retired from `organization-work.ts`: two fixed,
    /// Company-keyed SELECT statements over the org-owned
    /// `organization_node_changes` table. `iam_same_person` is a scalar
    /// function, not a cross-owner people source, so no hr_* table is
    /// duplicated here.
    #[test]
    fn organization_work_read_kinds_are_select_only_and_company_scoped() {
        let kinds = [(ORG_WORK_READ_CHANGE_EXISTS, 2usize), (ORG_WORK_READ_CHANGES, 4)];
        assert_eq!(kinds.len(), 2);
        for (kind, expected) in kinds {
            assert!(supported(kind), "read kind must be allow listed: {kind}");
            let statement = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(
                statement.starts_with("SELECT "),
                "read kind must be SELECT-only: {kind}"
            );
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
                assert!(
                    !statement.contains(forbidden),
                    "read kind must not mutate or lock: {kind} contains {forbidden}"
                );
            }
            assert!(
                statement.contains("company_id=?"),
                "read kind must be Company scoped: {kind}"
            );
            assert!(
                statement.contains("organization_node_changes"),
                "read kind must stay on the org-owned table: {kind}"
            );
            assert!(
                !statement.contains("hr_people") && !statement.contains("hr_engagements"),
                "owner-local read must not duplicate cross-owner people sources: {kind}"
            );
            assert_eq!(
                statement.matches('?').count(),
                expected,
                "read kind parameter count: {kind}"
            );
        }
        let (statement, values) =
            owner_statement(ORG_WORK_READ_CHANGE_EXISTS, &[Value::from("change"), Value::from("company")])
                .expect("change exists statement");
        assert_eq!(values.len(), 2);
        assert!(statement.contains("company_id=?"));
        assert!(owner_statement(ORG_WORK_READ_CHANGE_EXISTS, &[Value::from("change")]).is_err());
    }

    /// Owner-local reads retired from `adapters/access.ts` and the `levels`/
    /// `templates` ports of `adapters/organization.ts`: fixed, Company-keyed
    /// SELECT statements over the org-owned `organization_nodes`, `staff_levels`
    /// and `organization_templates` tables. No people_payroll hr_* source is
    /// duplicated. The `?::text IS NULL` predicates are the retained
    /// branch/department visibility windows; the statement translation keeps
    /// the cast on the numbered parameter so a null scope still means "no
    /// scope filter".
    #[test]
    fn organization_access_read_kinds_are_select_only_company_scoped_and_cast_safe() {
        let kinds = [
            (ORG_ACCESS_READ_NODES, 6usize),
            (ORG_ACCESS_READ_LEVELS, 1),
            (ORG_ACCESS_READ_TEMPLATES, 1),
        ];
        assert_eq!(kinds.len(), 3);
        for (kind, expected) in kinds {
            assert!(supported(kind), "read kind must be allow listed: {kind}");
            let statement = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(
                statement.starts_with("SELECT "),
                "read kind must be SELECT-only: {kind}"
            );
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
                assert!(
                    !statement.contains(forbidden),
                    "read kind must not mutate or lock: {kind} contains {forbidden}"
                );
            }
            assert!(
                statement.contains("company_id=?"),
                "read kind must be Company scoped: {kind}"
            );
            assert!(
                !statement.contains("hr_people") && !statement.contains("hr_engagements"),
                "owner-local read must not duplicate cross-owner people sources: {kind}"
            );
            assert_eq!(
                statement.matches('?').count(),
                expected,
                "read kind parameter count: {kind}"
            );
        }
        // Exact owner SQL for the scoped node list, including both branch and
        // department `?::text IS NULL` windows and the `!=` department guard.
        let nodes = sql(ORG_ACCESS_READ_NODES).unwrap();
        assert!(nodes.contains("organization_nodes"));
        assert_eq!(nodes.matches("?::text IS NULL").count(), 2);
        assert!(nodes.contains("kind!='department'"));
        // ? placeholders translate to numbered parameters with the cast intact.
        let positional_nodes = positional(nodes);
        assert!(!positional_nodes.contains('?'));
        assert!(positional_nodes.contains("company_id=$1"));
        assert!(positional_nodes.contains("$2::text IS NULL"));
        assert!(positional_nodes.contains("id=$3 OR branch_id=$4"));
        assert!(positional_nodes.contains("$5::text IS NULL"));
        assert!(positional_nodes.contains("id=$6"));
        assert_eq!(
            positional("SELECT 1 FROM organization_nodes WHERE (?::text IS NULL OR id=?)"),
            "SELECT 1 FROM organization_nodes WHERE ($1::text IS NULL OR id=$2)"
        );
        // A null scope is bound as JSON null; the arity gate stays fail-closed.
        let (statement, values) = owner_statement(
            ORG_ACCESS_READ_NODES,
            &[
                Value::from("company"),
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
            ],
        )
        .expect("scoped nodes statement");
        assert_eq!(values.len(), 6);
        assert_eq!(values[0], Value::from("company"));
        assert!(values[1..].iter().all(Value::is_null));
        assert!(statement.contains("?::text IS NULL"));
        assert!(owner_statement(
            ORG_ACCESS_READ_NODES,
            &[Value::from("company"), Value::Null],
        )
        .is_err());
        // The two list reads keep the exact retired literal bytes: the levels
        // port reuses the registered access kind, the templates port uses the
        // new kind added for this tail slice. Row shape is unchanged and the
        // arity gate stays fail-closed for both.
        assert_eq!(
            sql(ORG_ACCESS_READ_LEVELS).unwrap(),
            "SELECT id,name,level_number FROM staff_levels WHERE company_id=? AND active=1 AND level_number IS NOT NULL ORDER BY level_number"
        );
        assert_eq!(
            sql(ORG_ACCESS_READ_TEMPLATES).unwrap(),
            "SELECT id,name,version,positions_json FROM organization_templates WHERE company_id=? ORDER BY name"
        );
        let (levels, values) = owner_statement(ORG_ACCESS_READ_LEVELS, &[Value::from("company")])
            .expect("active levels statement");
        assert_eq!(values.len(), 1);
        assert!(levels.contains("active=1"));
        assert!(owner_statement(ORG_ACCESS_READ_LEVELS, &[]).is_err());
        let (templates, values) = owner_statement(ORG_ACCESS_READ_TEMPLATES, &[Value::from("company")])
            .expect("organization templates statement");
        assert_eq!(values.len(), 1);
        assert!(templates.contains("ORDER BY name"));
        assert!(owner_statement(ORG_ACCESS_READ_TEMPLATES, &[]).is_err());
    }

    #[test]
    fn finance_starter_nodes_kind_is_byte_identical_and_read_only() {
        assert!(supported(FINANCE_STARTER_ORGANIZATION_NODES));
        const RETIRED: &str = "SELECT id,kind,display_name FROM organization_nodes WHERE company_id=? AND kind IN ('branch','department') AND status='active' ORDER BY kind,id";
        assert_eq!(sql(FINANCE_STARTER_ORGANIZATION_NODES).unwrap(), RETIRED);
        let digest = {
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            hasher.update(RETIRED.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        assert_eq!(
            digest,
            "884e6239956ea25739936acc37fdee49c341f7dfa062478905c548507826a501"
        );
        let (statement, values) =
            owner_statement(FINANCE_STARTER_ORGANIZATION_NODES, &[Value::from("company")])
                .expect("finance starter nodes statement");
        assert_eq!(statement, RETIRED);
        assert_eq!(values, vec![Value::from("company")]);
        // Wrong arity and non-text Company keys fail closed.
        assert!(owner_statement(FINANCE_STARTER_ORGANIZATION_NODES, &[]).is_err());
        assert!(owner_statement(
            FINANCE_STARTER_ORGANIZATION_NODES,
            &[Value::from("company"), Value::from("company")],
        )
        .is_err());
        let upper = statement.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT "));
        assert!(!upper.contains(" INSERT ") && !upper.contains(" UPDATE ") && !upper.contains(" DELETE "));
        assert!(!upper.contains(" FOR UPDATE ") && !upper.contains(" FOR SHARE ") && !upper.contains(" LOCK TABLE "));
    }

    #[test]
    fn people_lifecycle_data_nodes_kind_is_byte_identical_and_read_only() {
        assert!(supported(PEOPLE_LIFECYCLE_DATA_NODES));
        const RETIRED: &str = "SELECT id,display_name name,kind FROM organization_nodes WHERE company_id=? AND status='active' ORDER BY display_name,id";
        assert_eq!(sql(PEOPLE_LIFECYCLE_DATA_NODES).unwrap(), RETIRED);
        let digest = {
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            hasher.update(RETIRED.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        assert_eq!(
            digest,
            "540b876572f469cd4d0092a855ae17077479bf52f073e39a0f7fb49ef10ca2b8"
        );
        let (statement, values) =
            owner_statement(PEOPLE_LIFECYCLE_DATA_NODES, &[Value::from("company")])
                .expect("people lifecycle data nodes statement");
        assert_eq!(statement, RETIRED);
        assert_eq!(values, vec![Value::from("company")]);
        // Wrong arity and a missing Company key fail closed.
        assert!(owner_statement(PEOPLE_LIFECYCLE_DATA_NODES, &[]).is_err());
        assert!(owner_statement(
            PEOPLE_LIFECYCLE_DATA_NODES,
            &[Value::from("company"), Value::from("company")],
        )
        .is_err());
        let upper = statement.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT "));
        assert!(!upper.contains(" INSERT ") && !upper.contains(" UPDATE ") && !upper.contains(" DELETE "));
        assert!(!upper.contains(" FOR UPDATE ") && !upper.contains(" FOR SHARE ") && !upper.contains(" LOCK TABLE "));
    }

    #[test]
    fn lifecycle_read_save_position_kind_is_byte_identical_and_read_only() {
        assert!(supported(LIFECYCLE_READ_SAVE_POSITION));
        const RETIRED: &str = "SELECT level_id,branch_id,department_id FROM organization_positions WHERE company_id=? AND id=?";
        assert_eq!(sql(LIFECYCLE_READ_SAVE_POSITION).unwrap(), RETIRED);
        let digest = {
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            hasher.update(RETIRED.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        assert_eq!(
            digest,
            "3890b4ecb403e95cc95d4e0a80fb9ca8403a313d6b4b487e0a3247d4fa6c4911"
        );
        let (statement, values) = owner_statement(
            LIFECYCLE_READ_SAVE_POSITION,
            &[Value::from("company"), Value::from("position")],
        )
        .expect("lifecycle save position statement");
        assert_eq!(statement, RETIRED);
        assert_eq!(values, vec![Value::from("company"), Value::from("position")]);
        // Missing either positional value fails closed.
        assert!(owner_statement(LIFECYCLE_READ_SAVE_POSITION, &[Value::from("company")]).is_err());
        assert!(owner_statement(
            LIFECYCLE_READ_SAVE_POSITION,
            &[Value::from("company"), Value::from("position"), Value::from("extra")],
        )
        .is_err());
        let upper = statement.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT "));
        assert!(!upper.contains(" INSERT ") && !upper.contains(" UPDATE ") && !upper.contains(" DELETE "));
        assert!(!upper.contains(" FOR UPDATE ") && !upper.contains(" FOR SHARE ") && !upper.contains(" LOCK TABLE "));
    }

    #[test]
    fn lifecycle_read_rehire_position_kind_is_byte_identical_and_read_only() {
        assert!(supported(LIFECYCLE_READ_REHIRE_POSITION));
        const RETIRED: &str =
            "SELECT * FROM organization_positions WHERE company_id=? AND id=? AND active=1";
        assert_eq!(sql(LIFECYCLE_READ_REHIRE_POSITION).unwrap(), RETIRED);
        let digest = {
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            hasher.update(RETIRED.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        assert_eq!(
            digest,
            "5e19e3c48061ce902c502bdeb5668f2731ef1ce622754de690c0203bbbba87c3"
        );
        let (statement, values) = owner_statement(
            LIFECYCLE_READ_REHIRE_POSITION,
            &[Value::from("company"), Value::from("position")],
        )
        .expect("lifecycle rehire position statement");
        assert_eq!(statement, RETIRED);
        assert_eq!(values, vec![Value::from("company"), Value::from("position")]);
        // Missing either positional value fails closed.
        assert!(owner_statement(LIFECYCLE_READ_REHIRE_POSITION, &[Value::from("company")]).is_err());
        assert!(owner_statement(
            LIFECYCLE_READ_REHIRE_POSITION,
            &[Value::from("company"), Value::from("position"), Value::from("extra")],
        )
        .is_err());
        let upper = statement.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT "));
        assert!(!upper.contains(" INSERT ") && !upper.contains(" UPDATE ") && !upper.contains(" DELETE "));
        assert!(!upper.contains(" FOR UPDATE ") && !upper.contains(" FOR SHARE ") && !upper.contains(" LOCK TABLE "));
    }

    #[test]
    fn draft_read_leave_defaults_kind_is_byte_identical_and_read_only() {
        assert!(supported(DRAFT_READ_LEAVE_DEFAULTS));
        const RETIRED: &str = "SELECT level_id,version,policy_version_id FROM level_leave_defaults WHERE company_id=? ORDER BY level_id,version";
        assert_eq!(sql(DRAFT_READ_LEAVE_DEFAULTS).unwrap(), RETIRED);
        let digest = {
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            hasher.update(RETIRED.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        assert_eq!(
            digest,
            "39766df8411791c865f7e18e2345f03211fd68b13bce02adc1d6b5b54409b7f7"
        );
        let (statement, values) =
            owner_statement(DRAFT_READ_LEAVE_DEFAULTS, &[Value::from("company")])
                .expect("draft leave defaults statement");
        assert_eq!(statement, RETIRED);
        assert_eq!(values, vec![Value::from("company")]);
        // Missing the Company key fails closed.
        assert!(owner_statement(DRAFT_READ_LEAVE_DEFAULTS, &[]).is_err());
        assert!(owner_statement(
            DRAFT_READ_LEAVE_DEFAULTS,
            &[Value::from("company"), Value::from("company")],
        )
        .is_err());
        let upper = statement.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT "));
        assert!(!upper.contains(" INSERT ") && !upper.contains(" UPDATE ") && !upper.contains(" DELETE "));
        assert!(!upper.contains(" FOR UPDATE ") && !upper.contains(" FOR SHARE ") && !upper.contains(" LOCK TABLE "));
    }

    #[test]
    fn change_event_snapshot_kinds_are_byte_identical_and_read_only() {
        const APPLIED: &str = "SELECT snapshot_json FROM organization_node_change_events WHERE change_id=? AND action='applied' ORDER BY created_at DESC LIMIT 1";
        const SUBMITTED: &str = "SELECT snapshot_json FROM organization_node_change_events WHERE change_id=? AND action='submitted' ORDER BY created_at DESC,id DESC LIMIT 1";
        for (kind, retired, digest) in [
            (
                CHANGE_EVENT_SNAPSHOT_APPLIED,
                APPLIED,
                "c2440b239c1bd6e293774e4b15ebc6c9515966a4b787401fd5a3bc995f4cd880",
            ),
            (
                CHANGE_EVENT_SNAPSHOT_SUBMITTED,
                SUBMITTED,
                "cf2d32fbb2e088beaddb7b0ada30f1f82ecfd2a3da5a261594ebcafbf15f3e7c",
            ),
        ] {
            assert!(supported(kind));
            assert_eq!(sql(kind).unwrap(), retired);
            let actual = {
                use sha2::Digest;
                let mut hasher = sha2::Sha256::new();
                hasher.update(retired.as_bytes());
                format!("{:x}", hasher.finalize())
            };
            assert_eq!(actual, digest);
            let (statement, values) =
                owner_statement(kind, &[Value::from("change")]).expect("snapshot statement");
            assert_eq!(statement, retired);
            assert_eq!(values, vec![Value::from("change")]);
            // Missing or extra positional values fail closed.
            assert!(owner_statement(kind, &[]).is_err());
            assert!(owner_statement(kind, &[Value::from("change"), Value::from("extra")]).is_err());
            let upper = statement.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT "));
            assert!(!upper.contains(" INSERT ") && !upper.contains(" UPDATE ") && !upper.contains(" DELETE "));
            assert!(!upper.contains(" FOR UPDATE ") && !upper.contains(" FOR SHARE ") && !upper.contains(" LOCK TABLE "));
        }
    }

    #[test]
    fn attachment_organization_source_image_is_byte_identical_and_read_only() {
        assert!(supported(ATTACHMENT_ORGANIZATION_SOURCE_IMAGE));
        const RETIRED: &str = "SELECT to_jsonb(company)::text image FROM organization_nodes company WHERE company.id=? AND company.kind='company'";
        assert_eq!(sql(ATTACHMENT_ORGANIZATION_SOURCE_IMAGE).unwrap(), RETIRED);
        let digest = {
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            hasher.update(RETIRED.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        assert_eq!(
            digest,
            "17d882597c91fc202c157067d2c6d6ef72fdc077bf010ba1fd4a356aee72aff1"
        );
        let (statement, values) =
            owner_statement(ATTACHMENT_ORGANIZATION_SOURCE_IMAGE, &[Value::from("company")])
                .expect("attachment organization source image statement");
        assert_eq!(statement, RETIRED);
        assert_eq!(values, vec![Value::from("company")]);
        // Missing or extra positional values fail closed.
        assert!(owner_statement(ATTACHMENT_ORGANIZATION_SOURCE_IMAGE, &[]).is_err());
        assert!(owner_statement(
            ATTACHMENT_ORGANIZATION_SOURCE_IMAGE,
            &[Value::from("company"), Value::from("extra")],
        )
        .is_err());
        let upper = statement.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT "));
        assert!(!upper.contains(" INSERT ") && !upper.contains(" UPDATE ") && !upper.contains(" DELETE "));
        assert!(!upper.contains(" FOR UPDATE ") && !upper.contains(" FOR SHARE ") && !upper.contains(" LOCK TABLE "));
    }

    #[test]
    fn attachment_source_image_kinds_are_byte_identical_and_read_only() {
        const NODE: &str = "SELECT jsonb_build_object('node',to_jsonb(node),'company',(SELECT to_jsonb(company) FROM organization_nodes company WHERE company.id=node.company_id AND company.kind='company'),\n 'branch',(SELECT to_jsonb(branch) FROM organization_nodes branch WHERE branch.id=node.branch_id AND branch.company_id=node.company_id))::text image FROM organization_nodes node WHERE node.id=? AND node.company_id=? AND node.kind IN('company','headquarters','branch','department')";
        const CHANGE: &str = "SELECT jsonb_build_object('change',to_jsonb(change),'target',(SELECT to_jsonb(node) FROM organization_nodes node WHERE node.id=change.target_node_id AND node.company_id=change.company_id),\n 'branch',(SELECT to_jsonb(branch) FROM organization_nodes branch WHERE branch.id=change.branch_id AND branch.company_id=change.company_id),\n 'events',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.version,event.id) FROM organization_node_change_events event WHERE event.change_id=change.id AND event.company_id=change.company_id))::text image FROM organization_node_changes change WHERE change.id=? AND change.company_id=?";
        for (kind, retired, digest) in [
            (
                ATTACHMENT_SOURCE_NODE_IMAGE,
                NODE,
                "4afd53d02e091f8a396967449f970ef417ca6fbef48631927a80ef4862c4869c",
            ),
            (
                ATTACHMENT_SOURCE_CHANGE_IMAGE,
                CHANGE,
                "8bf5a70d64fb60dec0e17c73402665ba99e52b0b9da09ee9d16f4e14070b3769",
            ),
        ] {
            assert!(supported(kind));
            assert_eq!(sql(kind).unwrap(), retired);
            let actual = {
                use sha2::Digest;
                let mut hasher = sha2::Sha256::new();
                hasher.update(retired.as_bytes());
                format!("{:x}", hasher.finalize())
            };
            assert_eq!(actual, digest);
            let (statement, values) = owner_statement(kind, &[Value::from("id"), Value::from("company")])
                .expect("attachment source image statement");
            assert_eq!(statement, retired);
            assert_eq!(values, vec![Value::from("id"), Value::from("company")]);
            // Missing or extra positional values fail closed.
            assert!(owner_statement(kind, &[Value::from("id")]).is_err());
            assert!(owner_statement(
                kind,
                &[Value::from("id"), Value::from("company"), Value::from("extra")],
            )
            .is_err());
            let upper = statement.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT "));
            assert!(!upper.contains(" INSERT ") && !upper.contains(" UPDATE ") && !upper.contains(" DELETE "));
            assert!(!upper.contains(" FOR UPDATE ") && !upper.contains(" FOR SHARE ") && !upper.contains(" LOCK TABLE "));
        }
    }

    #[test]
    fn staff_settings_list_levels_is_byte_identical_and_read_only() {
        assert!(supported(STAFF_SETTINGS_LIST_LEVELS));
        const RETIRED: &str =
            "SELECT * FROM staff_levels WHERE company_id=? ORDER BY level_number ASC NULLS LAST,rank DESC,code";
        assert_eq!(sql(STAFF_SETTINGS_LIST_LEVELS).unwrap(), RETIRED);
        let digest = {
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            hasher.update(RETIRED.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        assert_eq!(
            digest,
            "b1fdab87dcf08acf4996e6279eae4cf67715507f6d61d3d448fbdc4a1453bfdf"
        );
        let (statement, values) =
            owner_statement(STAFF_SETTINGS_LIST_LEVELS, &[Value::from("company")])
                .expect("staff settings list levels statement");
        assert_eq!(statement, RETIRED);
        assert_eq!(values, vec![Value::from("company")]);
        // Missing or extra positional values fail closed.
        assert!(owner_statement(STAFF_SETTINGS_LIST_LEVELS, &[]).is_err());
        assert!(owner_statement(
            STAFF_SETTINGS_LIST_LEVELS,
            &[Value::from("company"), Value::from("extra")],
        )
        .is_err());
        let upper = statement.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT "));
        assert!(!upper.contains(" INSERT ") && !upper.contains(" UPDATE ") && !upper.contains(" DELETE "));
        assert!(!upper.contains(" FOR UPDATE ") && !upper.contains(" FOR SHARE ") && !upper.contains(" LOCK TABLE "));
    }

    #[test]
    fn staff_policy_request_route_head_is_byte_identical_and_read_only() {
        assert!(supported(STAFF_POLICY_REQUEST_ROUTE_HEAD));
        const RETIRED: &str = "SELECT h.person_id,h.version FROM department_heads h JOIN organization_nodes n ON n.id=h.department_id WHERE h.company_id=? AND h.department_id=? AND n.company_id=h.company_id AND n.status='active' AND n.kind='department'";
        assert_eq!(sql(STAFF_POLICY_REQUEST_ROUTE_HEAD).unwrap(), RETIRED);
        let digest = {
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            hasher.update(RETIRED.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        assert_eq!(
            digest,
            "03c113f88d99c3cfaa8ee4f03f8616f8a0a32df2f3efe037b31e5d57f104d250"
        );
        let (statement, values) = owner_statement(
            STAFF_POLICY_REQUEST_ROUTE_HEAD,
            &[Value::from("company"), Value::from("department")],
        )
        .expect("staff policy request route head statement");
        assert_eq!(statement, RETIRED);
        assert_eq!(values, vec![Value::from("company"), Value::from("department")]);
        // Missing or extra positional values fail closed.
        assert!(owner_statement(STAFF_POLICY_REQUEST_ROUTE_HEAD, &[Value::from("company")]).is_err());
        assert!(owner_statement(
            STAFF_POLICY_REQUEST_ROUTE_HEAD,
            &[Value::from("company"), Value::from("department"), Value::from("extra")],
        )
        .is_err());
        let upper = statement.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT "));
        assert!(!upper.contains(" INSERT ") && !upper.contains(" UPDATE ") && !upper.contains(" DELETE "));
        assert!(!upper.contains(" FOR UPDATE ") && !upper.contains(" FOR SHARE ") && !upper.contains(" LOCK TABLE "));
    }

    #[test]
    fn draft_read_requests_is_byte_identical_and_read_only() {
        assert!(supported(DRAFT_READ_REQUESTS));
        const RETIRED: &str = "SELECT id,item_keys,state,last_error,change_id FROM organization_draft_requests WHERE draft_id=? ORDER BY created_at,id";
        assert_eq!(sql(DRAFT_READ_REQUESTS).unwrap(), RETIRED);
        let digest = {
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            hasher.update(RETIRED.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        assert_eq!(
            digest,
            "acd62f94cd510e0d5fc8d05fa9bb9b0b1e53e88c28810a9669f85d479fcb7438"
        );
        let (statement, values) =
            owner_statement(DRAFT_READ_REQUESTS, &[Value::from("draft")])
                .expect("draft read requests statement");
        assert_eq!(statement, RETIRED);
        assert_eq!(values, vec![Value::from("draft")]);
        // Missing or extra positional values fail closed.
        assert!(owner_statement(DRAFT_READ_REQUESTS, &[]).is_err());
        assert!(owner_statement(DRAFT_READ_REQUESTS, &[Value::from("draft"), Value::from("extra")]).is_err());
        let upper = statement.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT "));
        assert!(!upper.contains(" INSERT ") && !upper.contains(" UPDATE ") && !upper.contains(" DELETE "));
        assert!(!upper.contains(" FOR UPDATE ") && !upper.contains(" FOR SHARE ") && !upper.contains(" LOCK TABLE "));
    }
}
