//! Files-owned Projects attachment intake.
//!
//! These functions mirror the retained generic Files intake for the closed
//! `project` target: a draft set, one reservation per file and one completion
//! that publishes quarantined bytes and records the pending native scan. The
//! owner (Projects) authorizes the current project version and writer; Files
//! owns the set/link/source ledger, storage and scan attempts. Nothing here
//! makes a source bindable: bind and content reads still require the native
//! scan receipt (`source_state='available'` plus the clean attempt).
use crate::adapters::local_field_quarantine_writer::{
    FieldQuarantinePublication, LocalFieldQuarantineWriter,
};
use crate::domain::field_visit_upload::{
    FIELD_RESERVATION_MILLIS, FIELD_UPLOAD_MAX_BYTES, validate_field_upload_bytes,
};
use crate::public::project_attachment::{
    PROJECT_ATTACHMENT_PURPOSES, ProjectAttachmentLinkRead, ProjectAttachmentScanRead,
    ProjectAttachmentSetRead,
};
use erpx_server_projects::public::project_attachment::{
    ProjectAttachmentActor, authorize_project_attachment_read_with_connection,
    authorize_project_attachment_stage_with_connection,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const PROJECT_COMPONENT_KIND: &str = "record";
const PROJECT_POLICY_VERSION: i64 = 1;
const PROJECT_CLASSIFICATION: &str = "internal";
const PROJECT_MAX_FILES: i64 = 50;
const PROJECT_MAX_LINKS: usize = 100;
const PROJECT_MEDIA_TYPES: [&str; 6] = [
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/heic",
    "image/heif",
    "application/pdf",
];

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn unsupported() -> BusinessError {
    invalid(
        "ATTACHMENT_TARGET_UNSUPPORTED",
        "This attachment target has no native Projects policy.",
    )
}

fn policy_unknown() -> BusinessError {
    invalid(
        "ATTACHMENT_POLICY_UNKNOWN",
        "This file purpose is not registered for the selected record and context.",
    )
}

fn set_missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_SET_NOT_FOUND",
        "Attachment set not found.",
    )
}

fn upload_missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "UPLOAD_NOT_FOUND",
        "Upload intent not found or expired.",
    )
}

fn idempotency_conflict() -> BusinessError {
    conflict(
        "IDEMPOTENCY_CONFLICT",
        "This command key was already used for another attachment command.",
    )
}

fn upload_conflict() -> BusinessError {
    conflict(
        "UPLOAD_CONFLICT",
        "This upload already contains different bytes.",
    )
}

fn scan_receipt_invalid() -> BusinessError {
    conflict(
        "ATTACHMENT_SCAN_RECEIPT_INVALID",
        "The file scan receipt is missing or inconsistent.",
    )
}

fn valid_purpose(value: &str) -> bool {
    PROJECT_ATTACHMENT_PURPOSES.contains(&value)
}

fn valid_media(value: &str) -> bool {
    PROJECT_MEDIA_TYPES.contains(&value)
}

fn trim_js(value: &str) -> &str {
    value.trim_matches(|c: char| {
        matches!(
            c,
            '\u{0009}'..='\u{000d}'
                | '\u{0020}'
                | '\u{00a0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
        )
    })
}

fn command_key(value: &str) -> Result<(), BusinessError> {
    if !(8..=180).contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.' | b'-'))
    {
        return Err(invalid(
            "COMMAND_KEY_REQUIRED",
            "A valid Idempotency-Key is required.",
        ));
    }
    Ok(())
}

fn stable_id(prefix: &str, company: &str, user: &str, key: &str) -> String {
    let material = format!("{company}\0{user}\0{key}");
    format!(
        "{prefix}-{}",
        &hex::encode(Sha256::digest(material.as_bytes()))[..32]
    )
}

fn event_id() -> String {
    format!("attachment-event-{}", uuid::Uuid::new_v4())
}

fn serializable(connection: &mut PgConnection) -> Result<(), BusinessError> {
    use sqlx::Connection;
    if !connection.is_in_transaction() {
        return Err(unavailable(
            "FILES_PROJECT_TRANSACTION_REQUIRED",
            "A retained transaction is required to stage Project files.",
        ));
    }
    Ok(())
}

/// Trusted runtime configuration. Composition supplies frozen/readiness facts;
/// request input never selects them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectAttachmentIntakeAvailability {
    pub uploads_frozen: bool,
    pub scanner_ready: bool,
    pub preview_ready: bool,
}

impl ProjectAttachmentIntakeAvailability {
    fn require_writable(&self) -> Result<(), BusinessError> {
        if self.uploads_frozen {
            return Err(unavailable(
                "ATTACHMENT_UPLOAD_FREEZE",
                "File intake is briefly paused for storage reconciliation.",
            ));
        }
        if !self.scanner_ready || !self.preview_ready {
            return Err(unavailable(
                "ATTACHMENT_SCANNER_UNAVAILABLE",
                "File intake is unavailable until the malware scanner and current signatures are ready.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectAttachmentSetRequest {
    pub target_type: String,
    pub target_id: String,
    pub component_kind: String,
    #[serde(default)]
    pub component_ref: Option<String>,
    pub purpose_key: String,
    pub expected_target_version: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectAttachmentFileRequest {
    pub filename: String,
    pub media_type: String,
    pub bytes: i64,
    #[serde(default)]
    pub caption: Option<String>,
    #[serde(default)]
    pub supersedes_link_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectAttachmentReservation {
    pub upload_intent_id: String,
    pub attachment_link_id: String,
    pub expires_at: i64,
    pub replay: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAttachmentUploadCompletion {
    pub upload_intent_id: String,
    pub attachment_link_id: String,
    pub attachment_set_id: String,
    pub source_id: String,
    pub state: String,
    pub status: String,
    pub scan: ProjectAttachmentScanRead,
    pub replay: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAttachmentUploadStatus {
    pub upload_intent_id: String,
    pub attachment_link_id: String,
    pub attachment_set_id: String,
    pub source_id: Option<String>,
    pub source_state: Option<String>,
    pub link_state: String,
    pub status: String,
    pub scan: ProjectAttachmentScanRead,
    pub bindable: bool,
    pub scanned_at: Option<i64>,
    pub expires_at: i64,
}

/// A scan receipt is usable only when the source is available and the clean
/// attempt completed at the exact recorded scan time with the recorded
/// definition. Incomplete or mismatched receipts never surface as available.
fn receipt_valid(row: &sqlx::postgres::PgRow) -> bool {
    let source_state: Option<String> = row.get("source_state");
    let scanned_at: Option<i64> = row.get("scanned_at");
    let scanner_definition_at: Option<i64> = row.get("scanner_definition_at");
    let scan_state: Option<String> = row.get("scan_state");
    let scan_completed_at: Option<i64> = row.get("scan_completed_at");
    let scan_definition_at: Option<i64> = row.get("scan_definition_at");
    let scan_result: Option<String> = row.get("scan_result_code");
    source_state.as_deref() == Some("available")
        && scanned_at.is_some()
        && scanner_definition_at.is_some()
        && scan_state.as_deref() == Some("clean")
        && scan_completed_at == scanned_at
        && scan_definition_at == scanner_definition_at
        && scan_result.as_deref() == Some("OK")
}

fn scan_projection(row: &sqlx::postgres::PgRow) -> ProjectAttachmentScanRead {
    let attempt: Option<i64> = row.get("scan_attempt");
    let state: Option<String> = row.get("scan_state");
    let error: Option<String> = row.get("scan_error");
    let signature: Option<String> = row.get("scan_signature");
    let definition: Option<i64> = row.get("scan_definition_at");
    let completed: Option<i64> = row.get("scan_completed_at");
    ProjectAttachmentScanRead {
        attempts: attempt.unwrap_or(0),
        last_result: state,
        last_error: error,
        signature,
        definition_updated_at: definition,
        scanned_at: completed,
    }
}

const SET_RECEIPT_SQL: &str = r#"
SELECT a.source_state,a.scanned_at,a.scanner_definition_at,
       j.attempt scan_attempt,j.state scan_state,j.error_code scan_error,
       j.scanner_signature scan_signature,j.scanner_definition_at scan_definition_at,
       j.result_code scan_result_code,j.completed_at scan_completed_at
FROM attachments a
LEFT JOIN LATERAL (
  SELECT attempt,state,error_code,scanner_signature,scanner_definition_at,result_code,completed_at
  FROM attachment_scan_attempts WHERE attachment_id=a.id ORDER BY attempt DESC LIMIT 1
) j ON TRUE
WHERE a.id=$1 AND a.company_id=$2
"#;

async fn upload_receipt(
    connection: &mut PgConnection,
    company: &str,
    attachment_id: &str,
) -> Result<Option<sqlx::postgres::PgRow>, BusinessError> {
    sqlx::query(SET_RECEIPT_SQL)
        .bind(attachment_id)
        .bind(company)
        .fetch_optional(connection)
        .await
        .map_err(|_| unavailable("FILE_ORIGINAL_STORAGE", "The private file is unavailable."))
}

async fn read_set(
    connection: &mut PgConnection,
    company: &str,
    set_id: &str,
) -> Result<ProjectAttachmentSetRead, BusinessError> {
    let row: (
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        i64,
        i64,
        String,
        i64,
        i64,
        String,
        i64,
        i64,
    ) = sqlx::query_as(
        "SELECT id,target_type,target_id,component_kind,component_ref,purpose_key,\
         expected_target_version,policy_version,classification,required_count,max_files,state,\
         created_at,updated_at FROM attachment_sets WHERE id=$1 AND company_id=$2",
    )
    .bind(set_id)
    .bind(company)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| unavailable("FILES_PROJECT_ATTACHMENT_UNAVAILABLE", "The Project file source is temporarily unavailable."))?
    .ok_or_else(set_missing)?;
    let links: Vec<(
        String,
        Option<String>,
        String,
        String,
        Option<i64>,
        Option<String>,
        String,
        Option<String>,
        bool,
        i64,
        i64,
        Option<i64>,
    )> = sqlx::query_as(
        "SELECT l.id,l.attachment_id,COALESCE(a.original_name,u.original_name,'file'),\
         COALESCE(a.media_type,u.media_type,'application/octet-stream'),\
         COALESCE(a.bytes,u.declared_bytes),a.source_state,l.state,l.caption,\
         (a.preview_object_key IS NOT NULL),l.created_at,l.expires_at,l.bound_at \
         FROM attachment_links l \
         LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=$2 \
         LEFT JOIN upload_intents u ON u.id=l.upload_intent_id AND u.company_id=$2 \
         WHERE l.set_id=$1 ORDER BY l.created_at,l.id LIMIT 101",
    )
    .bind(set_id)
    .bind(company)
    .fetch_all(&mut *connection)
    .await
    .map_err(|_| unavailable("FILES_PROJECT_ATTACHMENT_UNAVAILABLE", "The Project file source is temporarily unavailable."))?;
    if links.len() > PROJECT_MAX_LINKS {
        return Err(unavailable(
            "ATTACHMENT_LIST_TOO_LARGE",
            "This Project has too many files to show at once.",
        ));
    }
    Ok(ProjectAttachmentSetRead {
        id: row.0,
        target_type: row.1,
        target_id: row.2,
        component_kind: row.3,
        component_ref: row.4,
        purpose_key: row.5,
        expected_target_version: row.6,
        policy_version: row.7,
        classification: row.8,
        required_count: row.9,
        max_files: row.10,
        state: row.11,
        created_at: row.12,
        updated_at: row.13,
        links: links
            .into_iter()
            .map(
                |(
                    id,
                    source_id,
                    filename,
                    media_type,
                    bytes,
                    source_state,
                    link_state,
                    caption,
                    preview_available,
                    created_at,
                    expires_at,
                    bound_at,
                )| ProjectAttachmentLinkRead {
                    id,
                    source_id,
                    filename,
                    media_type,
                    bytes,
                    source_state,
                    link_state,
                    caption,
                    preview_available,
                    created_at,
                    expires_at,
                    bound_at,
                },
            )
            .collect(),
    })
}

async fn recorded_set(
    connection: &mut PgConnection,
    company: &str,
    user: &str,
    requested: &str,
) -> Result<Option<String>, BusinessError> {
    sqlx::query_scalar::<_, String>(
        "SELECT s.id FROM attachment_sets s WHERE s.company_id=$1 \
         AND (s.id=$2 OR s.id=(SELECT e.set_id FROM attachment_events e \
           WHERE e.id=$3 AND e.company_id=$1 AND e.actor_id=$4 AND e.action='set.reused' \
             AND (e.details_json::jsonb->>'requestedSetId')=$2)) LIMIT 1",
    )
    .bind(company)
    .bind(requested)
    .bind(format!("set-alias-{requested}"))
    .bind(user)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| unavailable("FILES_PROJECT_ATTACHMENT_UNAVAILABLE", "The Project file source is temporarily unavailable."))
}

/// Create or replay the caller's draft set. `availability` is trusted runtime
/// configuration; the owner version is rechecked inside the caller's
/// serializable unit before anything is written.
pub async fn create_project_attachment_set(
    connection: &mut PgConnection,
    actor: &ProjectAttachmentActor,
    request: &ProjectAttachmentSetRequest,
    key: &str,
    availability: ProjectAttachmentIntakeAvailability,
    now: i64,
) -> Result<ProjectAttachmentSetRead, Error> {
    serializable(connection)?;
    command_key(key)?;
    if request.target_type != "project" {
        return Err(unsupported().into());
    }
    if request.component_kind != PROJECT_COMPONENT_KIND
        || !valid_purpose(&request.purpose_key)
        || request.expected_target_version < 1
    {
        return Err(policy_unknown().into());
    }
    let company = actor.company_id.as_str();
    let user = actor.user_id.as_str();
    let component_ref = request
        .component_ref
        .as_deref()
        .map(trim_js)
        .filter(|value| !value.is_empty());
    if component_ref.is_some_and(|value| value.encode_utf16().count() > 180) {
        return Err(invalid(
            "ATTACHMENT_COMPONENT_REF",
            "The file context is too long.",
        )
        .into());
    }
    let requested = stable_id("attachment-set", company, user, key);
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("attachment-set:{company}:{user}:{key}"))
        .execute(&mut *connection)
        .await?;
    if let Some(actual) = recorded_set(connection, company, user, &requested).await? {
        let row = sqlx::query(
            "SELECT target_type,target_id,component_kind,component_ref,purpose_key,\
             expected_target_version FROM attachment_sets WHERE id=$1 AND company_id=$2",
        )
        .bind(&actual)
        .bind(company)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(set_missing)?;
        let same = row.get::<String, _>("target_type") == request.target_type
            && row.get::<String, _>("target_id") == request.target_id
            && row.get::<String, _>("component_kind") == request.component_kind
            && row.get::<Option<String>, _>("component_ref").as_deref() == component_ref
            && row.get::<String, _>("purpose_key") == request.purpose_key
            && row.get::<i64, _>("expected_target_version") == request.expected_target_version;
        if !same {
            return Err(idempotency_conflict().into());
        }
        return read_set(connection, company, &actual).await.map_err(Into::into);
    }
    availability.require_writable()?;
    authorize_project_attachment_stage_with_connection(
        connection,
        actor,
        &request.target_id,
        request.expected_target_version,
        erpx_server_shared::public::TimestampMillis::new(now),
    )
    .await?;
    // Reuse a current native draft set for the same exact slot, mirroring the
    // retained generic create. The alias event makes the caller's key replay
    // the same set without creating a second draft.
    let reusable: Option<String> = sqlx::query_scalar(
        "SELECT candidate.id FROM attachment_sets candidate \
         WHERE candidate.company_id=$1 AND candidate.target_type='project' \
           AND candidate.target_id=$2 AND candidate.component_kind=$3 \
           AND candidate.component_ref IS NOT DISTINCT FROM $4 \
           AND candidate.purpose_key=$5 AND candidate.expected_target_version=$6 \
           AND candidate.created_by=$7 AND candidate.state='draft' \
           AND candidate.policy_version=$8 \
           AND NOT EXISTS(SELECT 1 FROM attachment_events e WHERE e.set_id=candidate.id \
             AND e.company_id=candidate.company_id AND e.action='set.created' \
             AND COALESCE((e.details_json::jsonb->>'compatibilityAdapter')::boolean,false)) \
         ORDER BY candidate.created_at,candidate.id LIMIT 1",
    )
    .bind(company)
    .bind(&request.target_id)
    .bind(PROJECT_COMPONENT_KIND)
    .bind(component_ref)
    .bind(&request.purpose_key)
    .bind(request.expected_target_version)
    .bind(user)
    .bind(PROJECT_POLICY_VERSION)
    .fetch_optional(&mut *connection)
    .await?;
    let actual = if let Some(existing) = reusable {
        let affected = sqlx::query(
            "INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) \
             SELECT $1,company_id,id,$2,'set.reused',$3,$4 FROM attachment_sets \
             WHERE id=$5 AND company_id=$6 AND created_by=$2 AND state='draft'",
        )
        .bind(format!("set-alias-{requested}"))
        .bind(user)
        .bind(serde_json::json!({"requestedSetId": requested}).to_string())
        .bind(now)
        .bind(&existing)
        .bind(company)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(conflict(
                "ATTACHMENT_SET_CHANGED",
                "The original file set changed. Refresh before retrying.",
            )
            .into());
        }
        existing
    } else {
        let audience =
            serde_json::json!({"classification": PROJECT_CLASSIFICATION, "policyVersion": PROJECT_POLICY_VERSION})
                .to_string();
        sqlx::query(
            "INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,\
             target_id,component_kind,component_ref,purpose_key,expected_target_version,\
             policy_version,classification,audience_json,retention_policy_key,\
             retention_policy_version,retain_until,destruction_mode,required_count,max_files,\
             state,created_by,created_at,updated_at) \
             VALUES($1,$2,$3,$4,'project',$5,$6,$7,$8,$9,$10,$11,$12,NULL,NULL,NULL,NULL,0,$13,\
             'draft',$14,$15,$15)",
        )
        .bind(&requested)
        .bind(company)
        .bind(actor.branch_id.as_ref().map(erpx_server_shared::public::RecordId::as_str))
        .bind(actor.department_id.as_ref().map(erpx_server_shared::public::RecordId::as_str))
        .bind(&request.target_id)
        .bind(PROJECT_COMPONENT_KIND)
        .bind(component_ref)
        .bind(&request.purpose_key)
        .bind(request.expected_target_version)
        .bind(PROJECT_POLICY_VERSION)
        .bind(PROJECT_CLASSIFICATION)
        .bind(audience)
        .bind(PROJECT_MAX_FILES)
        .bind(user)
        .bind(now)
        .execute(&mut *connection)
        .await?;
        let details = serde_json::json!({
            "targetType": "project",
            "targetId": request.target_id,
            "purposeKey": request.purpose_key,
            "expectedTargetVersion": request.expected_target_version,
            "compatibilityAdapter": false,
        })
        .to_string()
        .replace('<', "\\u003c");
        sqlx::query(
            "INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,\
             created_at) VALUES($1,$2,$3,$4,'set.created',$5,$6)",
        )
        .bind(event_id())
        .bind(company)
        .bind(&requested)
        .bind(user)
        .bind(details)
        .bind(now)
        .execute(&mut *connection)
        .await?;
        requested.clone()
    };
    let set = read_set(connection, company, &actual).await?;
    if set.target_type != "project"
        || set.target_id != request.target_id
        || set.component_kind != PROJECT_COMPONENT_KIND
        || set.purpose_key != request.purpose_key
        || set.expected_target_version != request.expected_target_version
        || set.state != "draft"
    {
        return Err(conflict(
            "ATTACHMENT_SET_CHANGED",
            "The original file set changed. Refresh before retrying.",
        )
        .into());
    }
    Ok(set)
}

/// Reserve one upload intent and its pending content object. `storage_limit`
/// comes from the Company's purchased policy (composition reads it in this
/// same unit); `None` keeps the retained unlimited fallback.
pub async fn reserve_project_attachment_file(
    connection: &mut PgConnection,
    actor: &ProjectAttachmentActor,
    holding_id: &str,
    set_id: &str,
    request: &ProjectAttachmentFileRequest,
    key: &str,
    availability: ProjectAttachmentIntakeAvailability,
    storage_limit: Option<i64>,
    now: i64,
) -> Result<ProjectAttachmentReservation, Error> {
    serializable(connection)?;
    command_key(key)?;
    if holding_id.is_empty()
        || holding_id.contains('\0')
        || holding_id.split(['/', '\\']).any(|part| matches!(part, "." | ".."))
    {
        return Err(conflict(
            "ATTACHMENT_SET_CHANGED",
            "The original file set changed. Refresh before retrying.",
        )
        .into());
    }
    availability.require_writable()?;
    let company = actor.company_id.as_str();
    let user = actor.user_id.as_str();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("attachment-reserve:{company}"))
        .execute(&mut *connection)
        .await?;
    let set: Option<(
        String,
        String,
        String,
        String,
        i64,
        String,
        i64,
    )> = sqlx::query_as(
        "SELECT target_type,target_id,purpose_key,component_kind,expected_target_version,\
         state,max_files FROM attachment_sets WHERE id=$1 AND company_id=$2 FOR UPDATE",
    )
    .bind(set_id)
    .bind(company)
    .fetch_optional(&mut *connection)
    .await?;
    let (target_type, target_id, purpose, component, expected, state, max_files) =
        set.ok_or_else(set_missing)?;
    if target_type != "project" {
        return Err(unsupported().into());
    }
    if component != PROJECT_COMPONENT_KIND || !valid_purpose(&purpose) {
        return Err(policy_unknown().into());
    }
    let upload = stable_id("attachment-upload", company, user, key);
    let link = format!("attachment-link-{}", &upload[upload.len().saturating_sub(32)..]);
    let recorded: Option<(
        String,
        String,
        String,
        String,
        i64,
        Option<String>,
        Option<String>,
        i64,
    )> = sqlx::query_as(
        "SELECT u.attachment_set_id,u.attachment_link_id,u.original_name,u.media_type,\
         u.declared_bytes,l.caption,l.supersedes_link_id,u.expires_at \
         FROM upload_intents u JOIN attachment_links l ON l.id=u.attachment_link_id \
         WHERE u.id=$1 AND u.company_id=$2 AND u.created_by=$3",
    )
    .bind(&upload)
    .bind(company)
    .bind(user)
    .fetch_optional(&mut *connection)
    .await?;
    if state != "draft" && recorded.is_none() {
        return Err(conflict(
            "ATTACHMENT_SET_LOCKED",
            "This file set is already bound or closed.",
        )
        .into());
    }
    if let Some((
        recorded_set,
        recorded_link,
        recorded_filename,
        recorded_media,
        recorded_bytes,
        recorded_caption,
        recorded_supersedes,
        expires_at,
    )) = recorded
    {
        let filename = trim_js(&request.filename);
        let caption = request
            .caption
            .as_deref()
            .map(trim_js)
            .filter(|value| !value.is_empty());
        if recorded_set != set_id
            || recorded_link != link
            || recorded_filename != filename
            || recorded_media != request.media_type
            || recorded_bytes != request.bytes
            || recorded_caption.as_deref() != caption
            || recorded_supersedes != request.supersedes_link_id
        {
            return Err(idempotency_conflict().into());
        }
        authorize_project_attachment_read_with_connection(
            connection,
            actor,
            &target_id,
            erpx_server_shared::public::TimestampMillis::new(now),
        )
        .await?;
        return Ok(ProjectAttachmentReservation {
            upload_intent_id: upload,
            attachment_link_id: link,
            expires_at,
            replay: true,
        });
    }
    authorize_project_attachment_stage_with_connection(
        connection,
        actor,
        &target_id,
        expected,
        erpx_server_shared::public::TimestampMillis::new(now),
    )
    .await?;
    if !valid_media(&request.media_type) {
        return Err(invalid(
            "ATTACHMENT_MEDIA_TYPE",
            "This file type is not allowed for the selected purpose.",
        )
        .into());
    }
    if !(1..=FIELD_UPLOAD_MAX_BYTES).contains(&request.bytes) {
        return Err(invalid(
            "ATTACHMENT_SIZE",
            "Files for this purpose must be at most 10 MiB.",
        )
        .into());
    }
    let filename = trim_js(&request.filename);
    let caption = request
        .caption
        .as_deref()
        .map(trim_js)
        .filter(|value| !value.is_empty());
    if filename.is_empty()
        || filename.encode_utf16().count() > 255
        || caption.is_some_and(|value| value.encode_utf16().count() > 1000)
    {
        return Err(invalid(
            "ATTACHMENT_METADATA",
            "Check the filename and caption.",
        )
        .into());
    }
    if let Some(id) = request.supersedes_link_id.as_deref() {
        let valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id \
             WHERE l.id=$1 AND s.company_id=$2 AND s.target_type='project' AND s.target_id=$3 \
               AND s.purpose_key=$4 AND l.state='bound')",
        )
        .bind(id)
        .bind(company)
        .bind(&target_id)
        .bind(&purpose)
        .fetch_one(&mut *connection)
        .await?;
        if !valid {
            return Err(invalid(
                "ATTACHMENT_SUPERSEDES_INVALID",
                "A correction may supersede only bound evidence on the same record and purpose.",
            )
            .into());
        }
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attachment_links WHERE set_id=$1 AND state IN ('draft','bound')",
    )
    .bind(set_id)
    .fetch_one(&mut *connection)
    .await?;
    if count >= max_files {
        return Err(invalid(
            "ATTACHMENT_LIMIT",
            "This file set accepts at most its configured number of files.",
        )
        .into());
    }
    let used: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(bytes),0)::bigint FROM attachment_storage_ledger \
         WHERE company_id=$1 AND state IN ('reserved','stored')",
    )
    .bind(company)
    .fetch_one(&mut *connection)
    .await?;
    let pending: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(declared_bytes),0)::bigint FROM upload_intents \
         WHERE company_id=$1 AND status='pending' AND expires_at>$2",
    )
    .bind(company)
    .bind(now)
    .fetch_one(&mut *connection)
    .await?;
    let limit = storage_limit.unwrap_or(9_007_199_254_740_991);
    if used
        .checked_add(pending)
        .and_then(|value| value.checked_add(request.bytes))
        .is_none_or(|value| value > limit)
    {
        return Err(invalid("STORAGE_QUOTA", "Company file storage is full.").into());
    }
    let expires_at = now.checked_add(FIELD_RESERVATION_MILLIS).ok_or_else(|| {
        conflict(
            "ATTACHMENT_TARGET_CHANGED",
            "The record changed. Create a new file set for its current version.",
        )
    })?;
    let object_key = format!("{holding_id}/quarantine/{upload}/original");
    sqlx::query(
        "INSERT INTO attachment_links(id,set_id,caption,state,supersedes_link_id,created_by,\
         created_at,expires_at) VALUES($1,$2,$3,'draft',$4,$5,$6,$7)",
    )
    .bind(&link)
    .bind(set_id)
    .bind(caption)
    .bind(request.supersedes_link_id.as_deref())
    .bind(user)
    .bind(now)
    .bind(expires_at)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,\
         declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,\
         owner_version,attachment_set_id,attachment_link_id,company_id,quarantine_key) \
         VALUES($1,'project',$2,$3,$4,$5,$6,$7,'pending',$8,$9,$10,$10,$11,$12,$13,$14,$7)",
    )
    .bind(&upload)
    .bind(&target_id)
    .bind(&purpose)
    .bind(filename)
    .bind(&request.media_type)
    .bind(request.bytes)
    .bind(&object_key)
    .bind(user)
    .bind(expires_at)
    .bind(now)
    .bind(expected)
    .bind(set_id)
    .bind(&link)
    .bind(company)
    .execute(&mut *connection)
    .await?;
    sqlx::query("UPDATE attachment_links SET upload_intent_id=$1 WHERE id=$2 AND set_id=$3")
        .bind(&upload)
        .bind(&link)
        .bind(set_id)
        .execute(&mut *connection)
        .await?;
    let details = serde_json::json!({
        "filename": filename,
        "mediaType": request.media_type,
        "bytes": request.bytes,
    })
    .to_string()
    .replace('<', "\\u003c");
    sqlx::query(
        "INSERT INTO attachment_events(id,company_id,link_id,set_id,actor_id,action,\
         details_json,created_at) VALUES($1,$2,$3,$4,$5,'file.reserved',$6,$7)",
    )
    .bind(event_id())
    .bind(company)
    .bind(&link)
    .bind(set_id)
    .bind(user)
    .bind(details)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    let exact: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM upload_intents u JOIN attachment_links l \
         ON l.id=u.attachment_link_id JOIN attachment_sets s ON s.id=l.set_id \
         WHERE u.id=$1 AND u.company_id=$2 AND u.created_by=$3 AND u.status='pending' \
           AND u.attachment_set_id=$4 AND u.attachment_link_id=$5 AND u.owner_type='project' \
           AND u.owner_id=$6 AND u.purpose=$7 AND u.owner_version=$8 \
           AND u.original_name=$9 AND u.media_type=$10 AND u.declared_bytes=$11 \
           AND u.quarantine_key=$12 AND u.staging_key=$12 AND u.expires_at=$13 \
           AND l.upload_intent_id=u.id AND l.attachment_id IS NULL AND l.created_by=$3 \
           AND l.state='draft' AND l.caption IS NOT DISTINCT FROM $14 \
           AND l.supersedes_link_id IS NOT DISTINCT FROM $15 \
           AND s.company_id=$2 AND s.state='draft' AND s.expected_target_version=$8)",
    )
    .bind(&upload)
    .bind(company)
    .bind(user)
    .bind(set_id)
    .bind(&link)
    .bind(&target_id)
    .bind(&purpose)
    .bind(expected)
    .bind(filename)
    .bind(&request.media_type)
    .bind(request.bytes)
    .bind(&object_key)
    .bind(expires_at)
    .bind(caption)
    .bind(request.supersedes_link_id.as_deref())
    .fetch_one(&mut *connection)
    .await?;
    if !exact {
        return Err(conflict(
            "ATTACHMENT_SET_CHANGED",
            "The original file set changed. Refresh before retrying.",
        )
        .into());
    }
    Ok(ProjectAttachmentReservation {
        upload_intent_id: upload,
        attachment_link_id: link,
        expires_at,
        replay: false,
    })
}

/// Complete one reserved upload. Bytes are validated and published to the
/// private quarantine object, then the Files ledger records the source with a
/// pending native scan attempt. A source is never returned as available without
/// the exact clean scan receipt.
pub async fn complete_project_attachment_upload(
    connection: &mut PgConnection,
    writer: &LocalFieldQuarantineWriter,
    actor: &ProjectAttachmentActor,
    upload_id: &str,
    content_type: &str,
    bytes: Vec<u8>,
    availability: ProjectAttachmentIntakeAvailability,
    now: i64,
) -> Result<ProjectAttachmentUploadCompletion, Error> {
    serializable(connection)?;
    availability.require_writable()?;
    let company = actor.company_id.as_str();
    let user = actor.user_id.as_str();
    let row = sqlx::query(
        "SELECT u.attachment_set_id,u.attachment_link_id,u.owner_type,u.owner_id,u.purpose,\
         u.original_name,u.media_type,u.declared_bytes,u.quarantine_key,u.staging_key,u.status,\
         u.expires_at,u.owner_version,u.revision,s.target_type,s.target_id,s.component_kind,\
         s.purpose_key,s.expected_target_version,s.state set_state,l.state link_state,\
         l.created_by link_creator,l.attachment_id linked_source \
         FROM upload_intents u JOIN attachment_sets s ON s.id=u.attachment_set_id \
           AND s.company_id=u.company_id \
         JOIN attachment_links l ON l.id=u.attachment_link_id AND l.set_id=s.id \
         WHERE u.id=$1 AND u.company_id=$2 AND u.created_by=$3 FOR UPDATE OF u,s,l",
    )
    .bind(upload_id)
    .bind(company)
    .bind(user)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(upload_missing)?;
    let target_type: String = row.get("owner_type");
    let target: String = row.get("owner_id");
    let component: String = row.get("component_kind");
    let purpose: String = row.get("purpose");
    if target_type != "project" || row.get::<String, _>("target_type") != "project" {
        return Err(unsupported().into());
    }
    if component != PROJECT_COMPONENT_KIND
        || !valid_purpose(&purpose)
        || row.get::<String, _>("target_id") != target
        || row.get::<String, _>("purpose_key") != purpose
    {
        return Err(policy_unknown().into());
    }
    let set: String = row.get("attachment_set_id");
    let link: String = row.get("attachment_link_id");
    let media: String = row.get("media_type");
    let declared: i64 = row.get("declared_bytes");
    let expiry: i64 = row.get("expires_at");
    let status: String = row.get("status");
    let replay = matches!(status.as_str(), "uploaded" | "ready");
    if replay {
        authorize_project_attachment_read_with_connection(
            connection,
            actor,
            &target,
            erpx_server_shared::public::TimestampMillis::new(now),
        )
        .await?;
    } else {
        authorize_project_attachment_stage_with_connection(
            connection,
            actor,
            &target,
            row.get::<i64, _>("expected_target_version"),
            erpx_server_shared::public::TimestampMillis::new(now),
        )
        .await?;
    }
    if trim_js(content_type) != media {
        return Err(invalid(
            "UPLOAD_TYPE",
            "The file contents do not match the selected type.",
        )
        .into());
    }
    let checksum = validate_field_upload_bytes(&media, declared, &bytes)?;
    let source = format!("attachment-{upload_id}");
    if replay {
        let stored: Option<(String, String)> = sqlx::query_as(
            "SELECT checksum_sha256,source_state FROM attachments WHERE id=$1 AND company_id=$2",
        )
        .bind(&source)
        .bind(company)
        .fetch_optional(&mut *connection)
        .await?;
        let (stored_checksum, state) = stored.ok_or_else(upload_conflict)?;
        if stored_checksum != checksum {
            return Err(upload_conflict().into());
        }
        let receipt = upload_receipt(connection, company, &source)
            .await?
            .ok_or_else(upload_conflict)?;
        if state == "available" && !receipt_valid(&receipt) {
            return Err(scan_receipt_invalid().into());
        }
        return Ok(ProjectAttachmentUploadCompletion {
            upload_intent_id: upload_id.into(),
            attachment_link_id: link,
            attachment_set_id: set,
            source_id: source,
            state,
            status: scan_status(&receipt),
            scan: scan_projection(&receipt),
            replay: true,
        });
    }
    if status != "pending" || expiry <= now {
        return Err(upload_missing().into());
    }
    if row.get::<String, _>("set_state") != "draft"
        || row.get::<String, _>("link_state") != "draft"
        || row.get::<Option<String>, _>("linked_source").is_some()
        || row.get::<i64, _>("owner_version") != row.get::<i64, _>("expected_target_version")
        || row.get::<String, _>("link_creator") != user
    {
        return Err(conflict(
            "ATTACHMENT_TARGET_CHANGED",
            "The record changed after this file reservation. Create a file set for its current version.",
        )
        .into());
    }
    let object_key: String = row
        .get::<Option<String>, _>("quarantine_key")
        .or_else(|| row.get::<Option<String>, _>("staging_key"))
        .ok_or_else(upload_conflict)?;
    if object_key.is_empty()
        || object_key.contains('\0')
        || object_key.split(['/', '\\']).any(|part| matches!(part, "." | ".."))
    {
        return Err(upload_conflict().into());
    }
    let filename: String = row.get("original_name");
    let revision: i64 = row.get("revision");
    let publication = FieldQuarantinePublication {
        key: object_key.clone(),
        bytes: bytes.clone(),
        media_type: media.clone(),
        checksum_sha256: checksum.clone(),
        uploaded_at: now,
    };
    let object_lock = writer.lock_object(&object_key).await?;
    let published = writer.publish_locked(publication, object_lock).await?;
    if !published.created() {
        // The immutable object already existed. The store verified it matches
        // the exact bytes; this is the retained no-overwrite replay behavior.
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("attachment-upload:{company}:{upload_id}"))
        .execute(&mut *connection)
        .await?;
    let current = sqlx::query(
        "SELECT status,expires_at FROM upload_intents WHERE id=$1 AND company_id=$2 \
         AND created_by=$3 FOR UPDATE",
    )
    .bind(upload_id)
    .bind(company)
    .bind(user)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(upload_missing)?;
    let current_status: String = current.get("status");
    if matches!(current_status.as_str(), "uploaded" | "ready") {
        // Another completion won the same upload unit. Replay only with the
        // exact stored checksum and a usable native receipt.
        let stored: Option<(String, String)> = sqlx::query_as(
            "SELECT checksum_sha256,source_state FROM attachments WHERE id=$1 AND company_id=$2",
        )
        .bind(&source)
        .bind(company)
        .fetch_optional(&mut *connection)
        .await?;
        let (stored_checksum, state) = stored.ok_or_else(upload_conflict)?;
        if stored_checksum != checksum {
            return Err(upload_conflict().into());
        }
        let receipt = upload_receipt(connection, company, &source)
            .await?
            .ok_or_else(upload_conflict)?;
        if state == "available" && !receipt_valid(&receipt) {
            return Err(scan_receipt_invalid().into());
        }
        return Ok(ProjectAttachmentUploadCompletion {
            upload_intent_id: upload_id.into(),
            attachment_link_id: link,
            attachment_set_id: set,
            source_id: source,
            state,
            status: scan_status(&receipt),
            scan: scan_projection(&receipt),
            replay: true,
        });
    }
    if current_status != "pending" || current.get::<i64, _>("expires_at") <= now {
        return Err(upload_missing().into());
    }
    let byte_length = bytes.len() as i64;
    sqlx::query(
        "INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,\
         original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,\
         detected_media_type,source_state) \
         VALUES($1,$2,'project',$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$8,'quarantined')",
    )
    .bind(&source)
    .bind(upload_id)
    .bind(&target)
    .bind(&purpose)
    .bind(&object_key)
    .bind(revision)
    .bind(&filename)
    .bind(&media)
    .bind(byte_length)
    .bind(&checksum)
    .bind(user)
    .bind(now)
    .bind(company)
    .execute(&mut *connection)
    .await?;
    let updated = sqlx::query(
        "UPDATE upload_intents SET status='uploaded',actual_bytes=$1,checksum_sha256=$2,\
         detected_media_type=$3,final_key=$4,updated_at=$5 \
         WHERE id=$6 AND company_id=$7 AND status='pending'",
    )
    .bind(byte_length)
    .bind(&checksum)
    .bind(&media)
    .bind(&object_key)
    .bind(now)
    .bind(upload_id)
    .bind(company)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if updated != 1 {
        return Err(upload_conflict().into());
    }
    let linked = sqlx::query(
        "UPDATE attachment_links SET attachment_id=$1 WHERE id=$2 AND set_id=$3 \
         AND state='draft' AND attachment_id IS NULL",
    )
    .bind(&source)
    .bind(&link)
    .bind(&set)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if linked != 1 {
        return Err(upload_conflict().into());
    }
    let scan_id = format!("attachment-scan-{}", uuid::Uuid::new_v4());
    let ledger_id = format!("attachment-storage-{}", uuid::Uuid::new_v4());
    let event = event_id();
    sqlx::query(
        "INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at) \
         VALUES($1,$2,1,'pending',$3)",
    )
    .bind(&scan_id)
    .bind(&source)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,\
         bytes,state,created_at) VALUES($1,$2,$3,'original',$4,$5,'stored',$6)",
    )
    .bind(&ledger_id)
    .bind(company)
    .bind(&source)
    .bind(&object_key)
    .bind(byte_length)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    let details = serde_json::json!({
        "checksum": checksum,
        "bytes": byte_length,
        "detectedMediaType": media,
    })
    .to_string()
    .replace('<', "\\u003c");
    sqlx::query(
        "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,\
         action,details_json,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,'file.quarantined',$7,$8)",
    )
    .bind(&event)
    .bind(company)
    .bind(&source)
    .bind(&link)
    .bind(&set)
    .bind(user)
    .bind(details)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    let exact: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM upload_intents u \
         JOIN attachment_links l ON l.id=u.attachment_link_id \
         JOIN attachment_sets s ON s.id=l.set_id \
         JOIN attachments a ON a.id=l.attachment_id \
         WHERE u.id=$1 AND u.company_id=$2 AND u.status='uploaded' \
           AND u.actual_bytes=$3 AND u.checksum_sha256=$4 AND u.detected_media_type=$5 \
           AND u.final_key=$6 AND l.id=$7 AND l.state='draft' AND l.set_id=$8 \
           AND l.upload_intent_id=u.id AND a.id=$9 AND a.upload_id=u.id \
           AND a.company_id=$2 AND a.object_key=$6 AND a.bytes=$3 \
           AND a.checksum_sha256=$4 AND a.detected_media_type=$5 \
           AND a.source_state='quarantined' AND s.state='draft' AND s.target_id=$10 \
           AND s.expected_target_version=$11) \
         AND (SELECT count(*) FROM attachment_scan_attempts WHERE attachment_id=$9)=1 \
         AND EXISTS(SELECT 1 FROM attachment_scan_attempts WHERE id=$12 AND attachment_id=$9 \
           AND attempt=1 AND state='pending') \
         AND (SELECT count(*) FROM attachment_storage_ledger WHERE attachment_id=$9 \
           AND variant='original')=1 \
         AND EXISTS(SELECT 1 FROM attachment_storage_ledger WHERE id=$13 AND company_id=$2 \
           AND attachment_id=$9 AND variant='original' AND object_key=$6 AND bytes=$3 \
           AND state='stored') \
         AND EXISTS(SELECT 1 FROM attachment_events WHERE id=$14 AND company_id=$2 \
           AND attachment_id=$9 AND link_id=$7 AND set_id=$8 AND actor_id=$15 \
           AND action='file.quarantined')",
    )
    .bind(upload_id)
    .bind(company)
    .bind(byte_length)
    .bind(&checksum)
    .bind(&media)
    .bind(&object_key)
    .bind(&link)
    .bind(&set)
    .bind(&source)
    .bind(&target)
    .bind(row.get::<i64, _>("expected_target_version"))
    .bind(&scan_id)
    .bind(&ledger_id)
    .bind(&event)
    .bind(user)
    .fetch_one(&mut *connection)
    .await?;
    if !exact {
        return Err(upload_conflict().into());
    }
    let receipt = upload_receipt(connection, company, &source)
        .await?
        .ok_or_else(upload_conflict)?;
    Ok(ProjectAttachmentUploadCompletion {
        upload_intent_id: upload_id.into(),
        attachment_link_id: link,
        attachment_set_id: set,
        source_id: source,
        state: "quarantined".into(),
        status: "scan_pending".into(),
        scan: scan_projection(&receipt),
        replay: false,
    })
}

fn scan_status(receipt: &sqlx::postgres::PgRow) -> String {
    let source_state: Option<String> = receipt.get("source_state");
    match source_state.as_deref() {
        Some("available") if receipt_valid(receipt) => "available".into(),
        Some("available") => "scan_receipt_invalid".into(),
        Some("malware_rejected") => "malware_rejected".into(),
        Some("scan_error") => "scan_error".into(),
        Some("destroyed") => "destroyed".into(),
        _ => "scan_pending".into(),
    }
}

/// Status of one staged upload. The caller's current read authority is required;
/// draft uploads stay private to their creator. `bindable` is derived only from
/// the exact clean native receipt.
pub async fn project_attachment_upload_status(
    connection: &mut PgConnection,
    actor: &ProjectAttachmentActor,
    upload_id: &str,
    now: i64,
) -> Result<ProjectAttachmentUploadStatus, Error> {
    serializable(connection)?;
    let company = actor.company_id.as_str();
    let user = actor.user_id.as_str();
    let row = sqlx::query(
        "SELECT u.attachment_set_id,u.attachment_link_id,u.owner_id,u.expires_at,\
         s.target_id,s.expected_target_version,s.state set_state,l.state link_state,\
         l.created_by link_creator,l.attachment_id,l.bound_at,l.expires_at link_expires \
         FROM upload_intents u JOIN attachment_sets s ON s.id=u.attachment_set_id \
           AND s.company_id=u.company_id \
         JOIN attachment_links l ON l.id=u.attachment_link_id AND l.set_id=s.id \
         WHERE u.id=$1 AND u.company_id=$2 AND u.created_by=$3",
    )
    .bind(upload_id)
    .bind(company)
    .bind(user)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(upload_missing)?;
    if row.get::<String, _>("set_state") != "bound"
        && row.get::<String, _>("link_creator") != user
    {
        return Err(upload_missing().into());
    }
    let target: String = row.get("target_id");
    let authority = authorize_project_attachment_read_with_connection(
        connection,
        actor,
        &target,
        erpx_server_shared::public::TimestampMillis::new(now),
    )
    .await?;
    let source: Option<String> = row.get("attachment_id");
    let (scan, source_state, scanned_at, bindable) = match source.as_deref() {
        Some(source) => {
            let receipt = upload_receipt(connection, company, source)
                .await?
                .ok_or_else(upload_conflict)?;
            let state: Option<String> = receipt.get("source_state");
            let scanned_at: Option<i64> = receipt.get("scanned_at");
            let valid = receipt_valid(&receipt);
            if state.as_deref() == Some("available") && !valid {
                return Err(scan_receipt_invalid().into());
            }
            (
                scan_projection(&receipt),
                state,
                scanned_at,
                valid
                    && row.get::<String, _>("link_state") == "draft"
                    && row.get::<String, _>("set_state") == "draft"
                    && authority.version == row.get::<i64, _>("expected_target_version"),
            )
        }
        None => (
            ProjectAttachmentScanRead {
                attempts: 0,
                last_result: None,
                last_error: None,
                signature: None,
                definition_updated_at: None,
                scanned_at: None,
            },
            None,
            None,
            false,
        ),
    };
    let status = match source_state.as_deref() {
        Some("available") => "available",
        Some("malware_rejected") => "malware_rejected",
        Some("scan_error") => "scan_error",
        Some("destroyed") => "destroyed",
        Some("quarantined") | Some("scanning") | Some("legacy_unscanned") => "scan_pending",
        _ => "reserved",
    }
    .to_owned();
    Ok(ProjectAttachmentUploadStatus {
        upload_intent_id: upload_id.into(),
        attachment_link_id: row.get("attachment_link_id"),
        attachment_set_id: row.get("attachment_set_id"),
        source_id: source,
        source_state,
        link_state: row.get("link_state"),
        status,
        scan,
        bindable,
        scanned_at,
        expires_at: row.get("link_expires"),
    })
}

