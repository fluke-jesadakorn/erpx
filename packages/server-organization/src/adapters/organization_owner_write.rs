//! Native-owned Organization write registry and executor.
//!
//! This is the authoritative owner model for every remaining TypeScript write
//! literal in `packages/server-organization/src/**/*.ts`. The SQL and its
//! positional argument order live here, in the Organization owner crate. A
//! transport calls [`execute_owner_write`] with an allow-listed typed `kind`
//! and caller-supplied values; the executor runs the statement on the
//! caller-owned serializable unit together with receipt/audit/outbox effects.
//!
//! No TypeScript SQL copy, no fallback writer and no mirrored manifest remain
//! for a retired kind.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{Arguments, Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

// -- attachments-finance-facts ------------------------------------------------
pub const ORG_ATTACHMENT_NODE_VERSION_STEP: &str = "org_attachment_node_version_step";
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

// -- roster-reads ---------------------------------------------------------------
/// Retired `organizationRoster` people read: non-ended employee people with
/// their level projection. Read-only; the retained role gate stays in the
/// TypeScript application and transport never substitutes for the check.
pub const ORG_READROSTERPEOPLE: &str = "org_readrosterpeople";
/// Retired `organizationRoster` positions read: Company positions with level
/// numbers. Read-only; same gate/transport contract as the people read.
pub const ORG_READROSTERPOSITIONS: &str = "org_readrosterpositions";

/// Every kind this owner module accepts. Anything else is fail-closed.
pub const KINDS: &[&str] = &[
    ORG_ATTACHMENT_NODE_VERSION_STEP,
    ATTACHMENT_OWNER_VERSION_STEP,
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
    PROFILE_MEDIA_UPLOAD_AVATAR,
    PROFILE_MEDIA_UPLOAD_COVER,
    PROFILE_MEDIA_UPDATE_AVATAR,
    PROFILE_MEDIA_UPDATE_COVER,
    PROFILE_MEDIA_REMOVE_AVATAR,
    PROFILE_MEDIA_REMOVE_COVER,
    SPENDING_TEMPLATE_PUBLISH,
    POSITION_PAY_DEFAULT_INSERT,
    LEVEL_LEAVE_DEFAULT_INSERT,
    ORG_READROSTERPEOPLE,
    ORG_READROSTERPOSITIONS,
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
        ORG_READROSTERPEOPLE => {
            "SELECT p.id person_id,p.user_id,p.legal_name,p.email,p.version person_version,p.status person_status,e.id engagement_id,e.version engagement_version,e.company_id,e.branch_id,e.department_id,e.position_id,e.manager_person_id,e.level_id,l.level_number,l.name level_name,e.status,e.start_date,e.end_date,e.job_title FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id LEFT JOIN (SELECT \"id\",\"company_id\",\"code\",\"name\",\"rank\",\"active\",\"version\",\"created_at\",\"updated_at\",\"level_number\" FROM staff_levels) l ON l.id=e.level_id AND l.company_id=e.company_id AND l.active=1 WHERE p.company_id=? AND p.merged_into_id IS NULL AND p.status!='archived' AND e.worker_type='employee' AND e.status!='ended' ORDER BY p.legal_name,e.id"
        }
        ORG_READROSTERPOSITIONS => {
            "SELECT p.*,l.level_number FROM organization_positions p JOIN staff_levels l ON l.id=p.level_id AND l.company_id=p.company_id WHERE p.company_id=? ORDER BY l.level_number,p.title,p.id"
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
}
