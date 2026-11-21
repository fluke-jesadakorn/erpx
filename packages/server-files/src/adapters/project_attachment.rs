//! Files-owned adapter for the native Projects attachment binding port.
//!
//! Projects authorizes the current project owner and expected version. This
//! adapter binds the exact current draft Files links and applies the Project
//! version step inside the caller's retained serializable transaction. It never
//! creates, scans, stores or reads attachment bytes and never commits.
use crate::application::canonical_original::OriginalByteStore;
use erpx_server_projects::public::project_attachment::{
    PgProjectAttachmentFacts, ProjectAttachmentActor, ProjectAttachmentBindingCommand,
    ProjectAttachmentBindingReceipt, ProjectAttachmentFactsRepository, ProjectAttachmentFilesPort,
    ProjectAttachmentFuture, ProjectAttachmentGrant, ProjectAttachmentReadAuthority,
    ProjectAttachmentTargetFacts, ProjectAttachmentTargetType,
    authorize_project_attachment_read_with_connection, bind_project_attachment,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId, TimestampMillis};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
use std::collections::BTreeSet;

type Error = PgTransactionError<BusinessError>;

/// Files uses the same closed record-document binding type for Project sets.
pub const PROJECT_ATTACHMENT_BINDING_TYPE: &str = "record_document";
/// The only purposes registered for the `project` attachment target.
pub const PROJECT_ATTACHMENT_PURPOSES: [&str; 3] = ["project_document", "deliverable", "handoff"];
const MAX_LINKS: usize = 100;
const MAX_BYTES: i64 = 10 * 1024 * 1024;

fn unsupported() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "ATTACHMENT_TARGET_UNSUPPORTED",
        "This attachment target has no native Projects policy.",
    )
}

fn attachment_invalid() -> BusinessError {
    BusinessError::invalid(
        "PROJECT_ATTACHMENT_INVALID",
        "Review the selected links and current project version.",
    )
}

fn attachment_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_ATTACHMENT_CHANGED",
        "The original Project file source or current authority changed.",
    )
}

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *connection)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FILES_PROJECT_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn command_key(
    grant: &ProjectAttachmentGrant,
    command: &ProjectAttachmentBindingCommand,
) -> String {
    let mut links = command.link_ids.clone();
    links.sort();
    let material = format!(
        "{}\0{}\0{}\0{}\0{}",
        grant.company_id.as_str(),
        grant.target_id,
        grant.expected_version,
        grant.actor_id,
        links.join("\0")
    );
    format!(
        "project-attachment-{}",
        &hex::encode(Sha256::digest(material.as_bytes()))[..32]
    )
}

fn event_id(command: &str, set_id: &str) -> String {
    format!(
        "attachment-bind-{}",
        &hex::encode(Sha256::digest(format!("{command}\0{set_id}").as_bytes()))[..32]
    )
}

/// Pure guard shared with the unit tests. Reject unsupported targets, foreign
/// records and non-current versions before the Files ledger is touched.
pub fn validate_project_attachment_binding(
    grant: &ProjectAttachmentGrant,
    command: &ProjectAttachmentBindingCommand,
) -> Result<(), BusinessError> {
    command.validate()?;
    if grant.target_type != ProjectAttachmentTargetType::Project
        || command.target_type != grant.target_type.as_str()
    {
        return Err(unsupported());
    }
    if grant.target_id != command.target_id
        || grant.expected_version != command.expected_version
        || grant.purpose.as_str() != command.purpose
        || grant.expected_version < 1
        || grant.actor_id.is_empty()
        || grant.company_id.as_str().is_empty()
        || grant.witness.is_empty()
        || command.link_ids.len() > MAX_LINKS
    {
        return Err(attachment_invalid());
    }
    Ok(())
}

/// Already-resolved Projects target facts. The adapter that read them must be
/// dropped first so this snapshot and [`PgProjectAttachmentFiles`] can share the
/// same retained connection.
pub struct ProjectAttachmentFactsSnapshot {
    facts: Option<ProjectAttachmentTargetFacts>,
}

impl ProjectAttachmentFactsSnapshot {
    pub fn new(facts: Option<ProjectAttachmentTargetFacts>) -> Self {
        Self { facts }
    }
}

impl ProjectAttachmentFactsRepository for ProjectAttachmentFactsSnapshot {
    type Error = Error;

    fn target<'a>(
        &'a mut self,
        _company: &'a erpx_server_shared::public::CompanyId,
        _target_type: &'a str,
        _target_id: &'a str,
        _user_id: &'a str,
    ) -> ProjectAttachmentFuture<'a, Option<ProjectAttachmentTargetFacts>, Self::Error> {
        Box::pin(async move { Ok(self.facts.clone()) })
    }
}

/// Composition facade: read current Projects facts with the owner adapter,
/// release that borrow, then bind and advance through the Files port on the same
/// retained connection. The owner facts read acquires the Projects row lock, so
/// it remains current for the rest of this transaction.
pub async fn bind_project_attachment_with_current_facts(
    connection: &mut PgConnection,
    actor: &ProjectAttachmentActor,
    command: &ProjectAttachmentBindingCommand,
    now: TimestampMillis,
) -> Result<ProjectAttachmentBindingReceipt, Error> {
    command.validate()?;
    let target_id = RecordId::new(command.target_id.as_str()).map_err(|_| attachment_invalid())?;
    let facts = {
        let mut repository = PgProjectAttachmentFacts::new(&mut *connection);
        ProjectAttachmentFactsRepository::target(
            &mut repository,
            &actor.company_id,
            command.target_type.as_str(),
            target_id.as_str(),
            actor.user_id.as_str(),
        )
        .await?
    };
    let mut repository = ProjectAttachmentFactsSnapshot::new(facts);
    let mut files = PgProjectAttachmentFiles::new(&mut *connection);
    bind_project_attachment(actor, &mut repository, &mut files, command, now).await
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_PROJECT_ATTACHMENT_UNAVAILABLE",
        "The Project file source is temporarily unavailable.",
    )
}

/// Files-owned current set facts. Composition needs the exact target, purpose,
/// version and state before it can route a bind; this read never exposes bytes
/// or attachment source content.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAttachmentSetFacts {
    pub set_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub target_type: String,
    pub target_id: String,
    pub component_kind: String,
    pub purpose_key: String,
    pub expected_target_version: i64,
    pub state: String,
}

pub async fn project_attachment_set_facts(
    connection: &mut PgConnection,
    company_id: &str,
    set_id: &str,
) -> Result<Option<ProjectAttachmentSetFacts>, BusinessError> {
    serializable(&mut *connection)
        .await
        .map_err(|_| unavailable())?;
    let row: Option<(
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
        String,
        i64,
        String,
    )> = sqlx::query_as(
        "SELECT id,company_id,branch_id,department_id,target_type,target_id,component_kind,\
         purpose_key,expected_target_version,state \
         FROM attachment_sets WHERE id=$1 AND company_id=$2",
    )
    .bind(set_id)
    .bind(company_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| unavailable())?;
    Ok(row.map(
        |(
            set_id,
            company_id,
            branch_id,
            department_id,
            target_type,
            target_id,
            component_kind,
            purpose_key,
            expected_target_version,
            state,
        )| ProjectAttachmentSetFacts {
            set_id,
            company_id,
            branch_id,
            department_id,
            target_type,
            target_id,
            component_kind,
            purpose_key,
            expected_target_version,
            state,
        },
    ))
}

/// Files-owned read of the links a current Project set may bind. Only draft
/// links participate; the bind port rechecks Company, set, purpose, scan and
/// source state for every selected link. Ordered deterministically and bounded
/// by the same one-hundred link limit as the binding command.
pub async fn project_attachment_link_ids(
    connection: &mut PgConnection,
    company_id: &str,
    set_id: &str,
) -> Result<Vec<String>, BusinessError> {
    serializable(&mut *connection)
        .await
        .map_err(|_| unavailable())?;
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT l.id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id \
         WHERE s.id=$1 AND s.company_id=$2 AND l.state='draft' \
         ORDER BY l.created_at,l.id LIMIT 101",
    )
    .bind(set_id)
    .bind(company_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(|_| unavailable())?;
    if rows.len() > MAX_LINKS {
        return Err(attachment_invalid());
    }
    Ok(rows)
}

/// Files-owned metadata for the links this Project command bound. This is the
/// existing record-document manifest shape without bytes or storage keys.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAttachmentManifestItem {
    pub link_id: String,
    pub source_id: String,
    pub purpose_key: String,
    pub checksum_sha256: String,
    pub media_type: String,
    pub bytes: i64,
}

pub async fn project_attachment_bound_manifest(
    connection: &mut PgConnection,
    company_id: &str,
    set_id: &str,
    target_id: &str,
) -> Result<Vec<ProjectAttachmentManifestItem>, BusinessError> {
    serializable(&mut *connection)
        .await
        .map_err(|_| unavailable())?;
    let rows: Vec<(
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<i64>,
    )> = sqlx::query_as(
        "SELECT l.id,l.attachment_id,s.purpose_key,a.checksum_sha256,a.media_type,a.bytes \
             FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id \
             LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id \
             WHERE s.id=$1 AND s.company_id=$2 AND s.target_id=$3 AND l.state='bound' \
               AND l.binding_type=$4 AND l.binding_id=$3 \
             ORDER BY l.created_at,l.id LIMIT 101",
    )
    .bind(set_id)
    .bind(company_id)
    .bind(target_id)
    .bind(PROJECT_ATTACHMENT_BINDING_TYPE)
    .fetch_all(&mut *connection)
    .await
    .map_err(|_| unavailable())?;
    if rows.len() > MAX_LINKS {
        return Err(attachment_invalid());
    }
    rows.into_iter()
        .map(
            |(link_id, source_id, purpose_key, checksum_sha256, media_type, bytes)| {
                let source_id = source_id.filter(|value| !value.is_empty());
                let checksum_sha256 = checksum_sha256.filter(|value| valid_hash(value));
                let media_type = media_type.filter(|value| !value.is_empty());
                let bytes = bytes.filter(|value| (1..=MAX_BYTES).contains(value));
                match (source_id, checksum_sha256, media_type, bytes) {
                    (Some(source_id), Some(checksum_sha256), Some(media_type), Some(bytes))
                        if valid_purpose(&purpose_key) =>
                    {
                        Ok(ProjectAttachmentManifestItem {
                            link_id,
                            source_id,
                            purpose_key,
                            checksum_sha256,
                            media_type,
                            bytes,
                        })
                    }
                    _ => Err(attachment_changed()),
                }
            },
        )
        .collect()
}

/// One retained Project attachment link shape. It intentionally mirrors the
/// Files `AttachmentLinkDto` returned by the generic attachment list read; no
/// storage key or checksum crosses the owner boundary.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectAttachmentLinkRead {
    pub id: String,
    pub source_id: Option<String>,
    pub filename: String,
    pub media_type: String,
    pub bytes: Option<i64>,
    pub source_state: Option<String>,
    pub link_state: String,
    pub caption: Option<String>,
    pub preview_available: bool,
    pub created_at: i64,
    pub expires_at: i64,
    pub bound_at: Option<i64>,
}

/// One retained Project attachment set shape. It mirrors the Files
/// `AttachmentSetDto` returned by the generic attachment list read, including
/// the nested links and bounded counts.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectAttachmentSetRead {
    pub id: String,
    pub target_type: String,
    pub target_id: String,
    pub component_kind: String,
    pub component_ref: Option<String>,
    pub purpose_key: String,
    pub expected_target_version: i64,
    pub policy_version: i64,
    pub classification: String,
    pub required_count: i64,
    pub max_files: i64,
    pub state: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub links: Vec<ProjectAttachmentLinkRead>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectAttachmentScanRead {
    pub attempts: i64,
    pub last_result: Option<String>,
    pub last_error: Option<String>,
    pub signature: Option<String>,
    pub definition_updated_at: Option<i64>,
    pub scanned_at: Option<i64>,
}

/// Exact retained `attachmentLinkStatus` projection for one Project link.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectAttachmentLinkStatusRead {
    pub link_id: String,
    pub set_id: String,
    pub source_id: Option<String>,
    pub source_state: Option<String>,
    pub link_state: String,
    pub scan: ProjectAttachmentScanRead,
    pub preview_available: bool,
    pub bindable: bool,
    pub expires_at: i64,
    pub bound_at: Option<i64>,
    pub withdrawn_at: Option<i64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectAttachmentContentVariant {
    Original,
    Preview,
}

impl ProjectAttachmentContentVariant {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "original" => Ok(Self::Original),
            "preview" => Ok(Self::Preview),
            _ => Err(BusinessError::invalid(
                "ATTACHMENT_VARIANT",
                "Choose original or preview.",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Preview => "preview",
        }
    }
}

/// Verified private bytes plus the retained content-disposition facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectAttachmentContentRead {
    pub media_type: String,
    pub filename: String,
    pub body: Vec<u8>,
    pub inline: bool,
}

const MAX_SETS: usize = 100;
const MAX_SET_RESPONSE_BYTES: i64 = 2 * 1024 * 1024;
const PROJECT_SETS_SQL: &str = r#"
SELECT CASE WHEN octet_length(image::text) <= $5 THEN image END
FROM (
  SELECT COALESCE(jsonb_agg(row_json ORDER BY created_at DESC, id DESC), '[]'::jsonb) AS image
  FROM (
    SELECT s.created_at, s.id, jsonb_build_object(
      'id', s.id,
      'targetType', s.target_type,
      'targetId', s.target_id,
      'componentKind', s.component_kind,
      'componentRef', s.component_ref,
      'purposeKey', s.purpose_key,
      'expectedTargetVersion', s.expected_target_version,
      'policyVersion', s.policy_version,
      'classification', s.classification,
      'requiredCount', s.required_count,
      'maxFiles', s.max_files,
      'state', s.state,
      'createdAt', s.created_at,
      'updatedAt', s.updated_at,
      'links', COALESCE((
        SELECT jsonb_agg(jsonb_build_object(
          'id', links.id,
          'sourceId', links.attachment_id,
          'filename', COALESCE(links.original_name, links.intent_original_name, 'file'),
          'mediaType', COALESCE(links.media_type, links.intent_media_type, 'application/octet-stream'),
          'bytes', COALESCE(links.bytes, links.declared_bytes),
          'sourceState', links.source_state,
          'linkState', links.state,
          'caption', links.caption,
          'previewAvailable', links.preview_object_key IS NOT NULL,
          'createdAt', links.created_at,
          'expiresAt', links.expires_at,
          'boundAt', links.bound_at
        ) ORDER BY links.created_at, links.id)
        FROM (
          SELECT l.id,l.attachment_id,l.state,l.caption,l.created_at,l.expires_at,l.bound_at,
                 a.original_name,a.media_type,a.bytes,a.source_state,a.preview_object_key,
                 u.original_name intent_original_name,u.media_type intent_media_type,
                 u.declared_bytes
          FROM attachment_links l
          LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
          LEFT JOIN upload_intents u ON u.id=l.upload_intent_id AND u.company_id=s.company_id
          WHERE l.set_id=s.id
            AND (l.attachment_id IS NULL OR a.id IS NOT NULL)
            AND (l.upload_intent_id IS NULL OR u.id IS NOT NULL)
          ORDER BY l.created_at,l.id
          LIMIT 101
        ) links
      ), '[]'::jsonb)
    ) AS row_json
    FROM attachment_sets s
    WHERE s.company_id=$1
      AND s.target_type='project'
      AND s.target_id=$2
      AND (s.state='bound' OR (s.state='draft' AND s.created_by=$3))
    ORDER BY s.created_at DESC, s.id DESC
    LIMIT $4
  ) bounded
) sized
"#;

const PROJECT_LINKS_SQL: &str = r#"
SELECT CASE WHEN octet_length(image::text) <= $5 THEN image END
FROM (
  SELECT COALESCE(jsonb_agg(jsonb_build_object(
    'id', links.id,
    'sourceId', links.attachment_id,
    'filename', COALESCE(links.original_name, links.intent_original_name, 'file'),
    'mediaType', COALESCE(links.media_type, links.intent_media_type, 'application/octet-stream'),
    'bytes', COALESCE(links.bytes, links.declared_bytes),
    'sourceState', links.source_state,
    'linkState', links.state,
    'caption', links.caption,
    'previewAvailable', links.preview_object_key IS NOT NULL,
    'createdAt', links.created_at,
    'expiresAt', links.expires_at,
    'boundAt', links.bound_at
  ) ORDER BY links.created_at, links.id), '[]'::jsonb) AS image
  FROM (
    SELECT l.id,l.attachment_id,l.state,l.caption,l.created_at,l.expires_at,l.bound_at,
           a.original_name,a.media_type,a.bytes,a.source_state,a.preview_object_key,
           u.original_name intent_original_name,u.media_type intent_media_type,u.declared_bytes
    FROM attachment_links l
    JOIN attachment_sets s ON s.id=l.set_id
    LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
    LEFT JOIN upload_intents u ON u.id=l.upload_intent_id AND u.company_id=s.company_id
    WHERE l.set_id=$1
      AND s.company_id=$2
      AND s.target_type='project'
      AND s.target_id=$3
      AND (s.state='bound' OR (s.state='draft' AND s.created_by=$4))
      AND (l.attachment_id IS NULL OR a.id IS NOT NULL)
      AND (l.upload_intent_id IS NULL OR u.id IS NOT NULL)
    ORDER BY l.created_at,l.id
    LIMIT $6
  ) links
) sized
"#;

fn read_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_READ_CHANGED",
        "The file metadata changed while it was being read.",
    )
}

fn list_too_large() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ATTACHMENT_LIST_TOO_LARGE",
        "This Project has too many files to show at once.",
    )
}

fn set_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_SET_NOT_FOUND",
        "Attachment set not found.",
    )
}

fn link_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_LINK_NOT_FOUND",
        "File link not found.",
    )
}

fn not_available() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_NOT_AVAILABLE",
        "The file is still quarantined or was rejected.",
    )
}

fn preview_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_PREVIEW_NOT_FOUND",
        "A safe preview is not available.",
    )
}

async fn authorize_project_read(
    connection: &mut PgConnection,
    actor: &ProjectAttachmentActor,
    project_id: &str,
    now: TimestampMillis,
) -> Result<ProjectAttachmentReadAuthority, BusinessError> {
    authorize_project_attachment_read_with_connection(&mut *connection, actor, project_id, now)
        .await
}

/// Resolve the owning Project id for a Files set without changing target
/// visibility. Missing and non-project targets both return `None`, so callers
/// keep the retained hidden not-found response.
async fn project_attachment_set_target_id(
    connection: &mut PgConnection,
    company_id: &str,
    set_id: &str,
) -> Result<Option<String>, BusinessError> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT target_id FROM attachment_sets \
         WHERE id=$1 AND company_id=$2 AND target_type='project'",
    )
    .bind(set_id)
    .bind(company_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| unavailable())?;
    Ok(row.map(|(target_id,)| target_id))
}

/// Resolve the owning Project id for a Files link. Missing, foreign and
/// non-project links all return `None`, so callers keep the retained hidden
/// not-found response.
async fn project_attachment_link_target_id(
    connection: &mut PgConnection,
    company_id: &str,
    link_id: &str,
) -> Result<Option<String>, BusinessError> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT s.target_id FROM attachment_links l \
         JOIN attachment_sets s ON s.id=l.set_id AND s.company_id=$2 \
         WHERE l.id=$1 AND s.target_type='project'",
    )
    .bind(link_id)
    .bind(company_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| unavailable())?;
    Ok(row.map(|(target_id,)| target_id))
}

/// List one visible Project set's links when the caller knows only the set id.
/// The Project target is resolved from the set row and then the retained read
/// applies Project authorization, visibility, bounds and the exact link shape.
/// Missing and non-project sets stay hidden as `ATTACHMENT_SET_NOT_FOUND`.
pub async fn project_attachment_links_by_set(
    connection: &mut PgConnection,
    actor: &ProjectAttachmentActor,
    set_id: &str,
    now: TimestampMillis,
) -> Result<Vec<ProjectAttachmentLinkRead>, BusinessError> {
    let project_id =
        project_attachment_set_target_id(&mut *connection, actor.company_id.as_str(), set_id)
            .await?
            .ok_or_else(set_not_found)?;
    project_attachment_links(connection, actor, &project_id, set_id, now).await
}

/// Exact retained Project link status when the caller knows only the link id.
/// The Project target is resolved from the link's set and then the retained
/// status read applies Project authorization, draft privacy, scan and access
/// evidence. Missing and non-project links stay hidden as
/// `ATTACHMENT_LINK_NOT_FOUND`.
pub async fn project_attachment_link_status_by_link(
    connection: &mut PgConnection,
    actor: &ProjectAttachmentActor,
    link_id: &str,
    now: TimestampMillis,
) -> Result<ProjectAttachmentLinkStatusRead, BusinessError> {
    let project_id =
        project_attachment_link_target_id(&mut *connection, actor.company_id.as_str(), link_id)
            .await?
            .ok_or_else(link_not_found)?;
    project_attachment_link_status(connection, actor, &project_id, link_id, now).await
}

/// Read one Project link's verified private bytes when the caller knows only
/// the link id. The Project target is resolved from the link's set and then the
/// retained content read applies Project authorization, scan/revocation,
/// checksum, media-type and size gates. Missing and non-project links stay
/// hidden as `ATTACHMENT_LINK_NOT_FOUND`.
pub async fn project_attachment_content_by_link<S: OriginalByteStore>(
    connection: &mut PgConnection,
    store: &S,
    actor: &ProjectAttachmentActor,
    link_id: &str,
    variant: ProjectAttachmentContentVariant,
    now: TimestampMillis,
) -> Result<ProjectAttachmentContentRead, BusinessError> {
    let project_id =
        project_attachment_link_target_id(&mut *connection, actor.company_id.as_str(), link_id)
            .await?
            .ok_or_else(link_not_found)?;
    project_attachment_content(connection, store, actor, &project_id, link_id, variant, now).await
}

/// List the current Project attachment sets visible to the actor. Bound sets are
/// visible to current project readers; draft sets remain private to their
/// creator, exactly as the retained Files list read. The complete image is
/// bounded by count and serialized bytes, and never exposes storage facts.
pub async fn project_attachment_sets(
    connection: &mut PgConnection,
    actor: &ProjectAttachmentActor,
    project_id: &str,
    now: TimestampMillis,
) -> Result<Vec<ProjectAttachmentSetRead>, BusinessError> {
    authorize_project_read(&mut *connection, actor, project_id, now).await?;
    let image: Option<serde_json::Value> = sqlx::query_scalar(PROJECT_SETS_SQL)
        .bind(actor.company_id.as_str())
        .bind(project_id)
        .bind(actor.user_id.as_str())
        .bind((MAX_SETS + 1) as i64)
        .bind(MAX_SET_RESPONSE_BYTES)
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| unavailable())?;
    let sets: Vec<ProjectAttachmentSetRead> =
        serde_json::from_value(image.ok_or_else(list_too_large)?).map_err(|_| read_changed())?;
    if sets.len() > MAX_SETS || sets.iter().any(|set| set.links.len() > MAX_LINKS) {
        return Err(list_too_large());
    }
    Ok(sets)
}

/// List one visible Project set's links with the exact retained link shape.
pub async fn project_attachment_links(
    connection: &mut PgConnection,
    actor: &ProjectAttachmentActor,
    project_id: &str,
    set_id: &str,
    now: TimestampMillis,
) -> Result<Vec<ProjectAttachmentLinkRead>, BusinessError> {
    authorize_project_read(&mut *connection, actor, project_id, now).await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM attachment_sets s WHERE s.id=$1 AND s.company_id=$2 \
         AND s.target_type='project' AND s.target_id=$3 \
         AND (s.state='bound' OR (s.state='draft' AND s.created_by=$4)))",
    )
    .bind(set_id)
    .bind(actor.company_id.as_str())
    .bind(project_id)
    .bind(actor.user_id.as_str())
    .fetch_one(&mut *connection)
    .await
    .map_err(|_| unavailable())?;
    if !visible {
        return Err(set_not_found());
    }
    let image: Option<serde_json::Value> = sqlx::query_scalar(PROJECT_LINKS_SQL)
        .bind(set_id)
        .bind(actor.company_id.as_str())
        .bind(project_id)
        .bind(actor.user_id.as_str())
        .bind(MAX_SET_RESPONSE_BYTES)
        .bind((MAX_LINKS + 1) as i64)
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| unavailable())?;
    let links: Vec<ProjectAttachmentLinkRead> =
        serde_json::from_value(image.ok_or_else(list_too_large)?).map_err(|_| read_changed())?;
    if links.len() > MAX_LINKS {
        return Err(list_too_large());
    }
    Ok(links)
}

/// Exact retained Project link status, including the latest scan attempt and
/// the retained `bindable` derivation. Draft links stay private to their
/// creator; the allowed status read is recorded as Files access evidence.
pub async fn project_attachment_link_status(
    connection: &mut PgConnection,
    actor: &ProjectAttachmentActor,
    project_id: &str,
    link_id: &str,
    now: TimestampMillis,
) -> Result<ProjectAttachmentLinkStatusRead, BusinessError> {
    let authority = authorize_project_read(&mut *connection, actor, project_id, now).await?;
    let row = sqlx::query(
        "SELECT l.id,l.set_id,l.created_by,l.state link_state,l.expires_at,l.bound_at,\
         l.withdrawn_at,s.target_type,s.target_id,s.expected_target_version,\
         a.id source_id,a.source_state,a.preview_object_key IS NOT NULL preview_available \
         FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id \
         LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id \
         WHERE l.id=$1 AND s.company_id=$2 AND s.target_type='project' AND s.target_id=$3 \
           AND (l.attachment_id IS NULL OR a.id IS NOT NULL)",
    )
    .bind(link_id)
    .bind(actor.company_id.as_str())
    .bind(project_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| unavailable())?
    .ok_or_else(link_not_found)?;
    let link_state: String = row.get("link_state");
    let created_by: String = row.get("created_by");
    if link_state == "draft" && created_by != actor.user_id {
        return Err(link_not_found());
    }
    let source_id: Option<String> = row.get("source_id");
    let scan = match source_id.as_deref() {
        Some(attachment_id) => {
            let scan: Option<(
                i64,
                String,
                Option<String>,
                Option<String>,
                Option<i64>,
                Option<i64>,
            )> = sqlx::query_as(
                "SELECT attempt,state,error_code,scanner_signature,scanner_definition_at,\
                     completed_at FROM attachment_scan_attempts WHERE attachment_id=$1 \
                     ORDER BY attempt DESC LIMIT 1",
            )
            .bind(attachment_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(|_| unavailable())?;
            match scan {
                Some((
                    attempts,
                    last_result,
                    last_error,
                    signature,
                    definition_updated_at,
                    scanned_at,
                )) => ProjectAttachmentScanRead {
                    attempts,
                    last_result: Some(last_result),
                    last_error,
                    signature,
                    definition_updated_at,
                    scanned_at,
                },
                None => ProjectAttachmentScanRead {
                    attempts: 0,
                    last_result: None,
                    last_error: None,
                    signature: None,
                    definition_updated_at: None,
                    scanned_at: None,
                },
            }
        }
        None => ProjectAttachmentScanRead {
            attempts: 0,
            last_result: None,
            last_error: None,
            signature: None,
            definition_updated_at: None,
            scanned_at: None,
        },
    };
    let source_state: Option<String> = row.get("source_state");
    let expected_target_version: i64 = row.get("expected_target_version");
    let bindable = source_state.as_deref() == Some("available")
        && link_state == "draft"
        && authority.version == expected_target_version;
    if let Some(attachment_id) = source_id.as_deref() {
        sqlx::query(
            "INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,\
             variant,outcome,created_at) VALUES($1,$2,$3,$4,$5,'status','allowed',$6)",
        )
        .bind(format!("attachment-access-{}", uuid::Uuid::new_v4()))
        .bind(actor.company_id.as_str())
        .bind(attachment_id)
        .bind(link_id)
        .bind(actor.user_id.as_str())
        .bind(now.get())
        .execute(&mut *connection)
        .await
        .map_err(|_| unavailable())?;
    }
    Ok(ProjectAttachmentLinkStatusRead {
        link_id: row.get("id"),
        set_id: row.get("set_id"),
        source_id,
        source_state,
        link_state,
        scan,
        preview_available: row.get("preview_available"),
        bindable,
        expires_at: row.get("expires_at"),
        bound_at: row.get("bound_at"),
        withdrawn_at: row.get("withdrawn_at"),
    })
}

/// Read one Project link's verified private bytes. Quarantine/scan state,
/// revocation and destruction, checksum, media type and the ten-megabyte bound
/// are all rechecked before the injected store is called; the returned body is
/// re-hashed against the stored checksum and its length.
pub async fn project_attachment_content<S: OriginalByteStore>(
    connection: &mut PgConnection,
    store: &S,
    actor: &ProjectAttachmentActor,
    project_id: &str,
    link_id: &str,
    variant: ProjectAttachmentContentVariant,
    now: TimestampMillis,
) -> Result<ProjectAttachmentContentRead, BusinessError> {
    authorize_project_read(&mut *connection, actor, project_id, now).await?;
    let row = sqlx::query(
        "SELECT l.id,l.created_by,l.state link_state,s.state set_state,s.target_type,\
         s.target_id,s.company_id,a.id source_id,a.source_state,a.original_name,a.media_type,\
         a.bytes,a.checksum_sha256,a.object_key,a.preview_object_key,a.preview_bytes,\
         a.preview_checksum_sha256,a.preview_media_type,a.revoked_at,a.destroyed_at,\
         a.scanned_at,a.scanner_definition_at \
         FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id \
         LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id \
         WHERE l.id=$1 AND s.company_id=$2 AND s.target_type='project' AND s.target_id=$3 \
           AND a.id IS NOT NULL",
    )
    .bind(link_id)
    .bind(actor.company_id.as_str())
    .bind(project_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| unavailable())?
    .ok_or_else(link_not_found)?;
    let link_state: String = row.get("link_state");
    let created_by: String = row.get("created_by");
    if link_state == "withdrawn" || link_state == "expired" {
        return Err(link_not_found());
    }
    if link_state == "draft" && created_by != actor.user_id {
        return Err(link_not_found());
    }
    let source_state: Option<String> = row.get("source_state");
    let scanned_at: Option<i64> = row.get("scanned_at");
    let scanner_definition_at: Option<i64> = row.get("scanner_definition_at");
    let revoked_at: Option<i64> = row.get("revoked_at");
    let destroyed_at: Option<i64> = row.get("destroyed_at");
    let source_bytes: Option<i64> = row.get("bytes");
    let source_checksum: Option<String> = row.get("checksum_sha256");
    let source_media_type: Option<String> = row.get("media_type");
    if source_state.as_deref() != Some("available")
        || scanned_at.is_none()
        || scanner_definition_at.is_none()
        || revoked_at.is_some()
        || destroyed_at.is_some()
        || source_checksum
            .as_deref()
            .is_none_or(|value| !valid_hash(value))
        || source_media_type.as_deref().is_none_or(str::is_empty)
        || source_bytes.is_none_or(|value| !(1..=MAX_BYTES).contains(&value))
    {
        return Err(not_available());
    }
    let (object_key, bytes, checksum, media_type, filename, inline) = match variant {
        ProjectAttachmentContentVariant::Original => {
            let object_key: Option<String> = row.get("object_key");
            let filename: Option<String> = row.get("original_name");
            (
                object_key.ok_or_else(preview_not_found)?,
                source_bytes.ok_or_else(not_available)?,
                source_checksum.ok_or_else(not_available)?,
                source_media_type.ok_or_else(not_available)?,
                filename.unwrap_or_else(|| "file".into()),
                false,
            )
        }
        ProjectAttachmentContentVariant::Preview => {
            let object_key: Option<String> = row.get("preview_object_key");
            let bytes: Option<i64> = row.get("preview_bytes");
            let checksum: Option<String> = row.get("preview_checksum_sha256");
            let media_type: Option<String> = row.get("preview_media_type");
            let filename: Option<String> = row.get("original_name");
            let checksum = checksum.filter(|value| valid_hash(value));
            (
                object_key.ok_or_else(preview_not_found)?,
                bytes.ok_or_else(preview_not_found)?,
                checksum.ok_or_else(preview_not_found)?,
                media_type
                    .filter(|value| !value.is_empty())
                    .ok_or_else(preview_not_found)?,
                filename.unwrap_or_else(|| "preview".into()),
                true,
            )
        }
    };
    if !(1..=MAX_BYTES).contains(&bytes) {
        return Err(not_available());
    }
    let body = store
        .read_verified(&object_key, bytes as usize, &checksum, MAX_BYTES as usize)
        .await?;
    if body.len() != bytes as usize || hex::encode(Sha256::digest(&body)) != checksum {
        return Err(read_changed());
    }
    let source_id: Option<String> = row.get("source_id");
    if let Some(attachment_id) = source_id.as_deref() {
        sqlx::query(
            "INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,\
             variant,outcome,created_at) VALUES($1,$2,$3,$4,$5,$6,'allowed',$7)",
        )
        .bind(format!("attachment-access-{}", uuid::Uuid::new_v4()))
        .bind(actor.company_id.as_str())
        .bind(attachment_id)
        .bind(link_id)
        .bind(actor.user_id.as_str())
        .bind(variant.as_str())
        .bind(now.get())
        .execute(&mut *connection)
        .await
        .map_err(|_| unavailable())?;
    }
    Ok(ProjectAttachmentContentRead {
        media_type,
        filename,
        body,
        inline,
    })
}

struct ProjectLinkRow {
    link_id: String,
    set_id: String,
    source_id: String,
    purpose_key: String,
    checksum_sha256: String,
    media_type: String,
    bytes: i64,
}

fn valid_purpose(value: &str) -> bool {
    PROJECT_ATTACHMENT_PURPOSES
        .iter()
        .any(|purpose| *purpose == value)
}

fn details(
    rows: &[ProjectLinkRow],
    binding_id: &str,
    purpose: &str,
) -> Result<String, BusinessError> {
    let items = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "linkId": row.link_id,
                "sourceId": row.source_id,
                "purposeKey": row.purpose_key,
                "checksumSha256": row.checksum_sha256,
                "mediaType": row.media_type,
                "bytes": row.bytes,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&serde_json::json!({
        "bindingType": PROJECT_ATTACHMENT_BINDING_TYPE,
        "bindingId": binding_id,
        "purpose": purpose,
        "manifest": items,
    }))
    .map(|value| value.replace('<', "\\u003c"))
    .map_err(|_| attachment_invalid())
}

/// Files-owned binding and Project version-step implementation. The adapter
/// borrows the caller's connection so the bind, audit event and project version
/// are one unit.
pub struct PgProjectAttachmentFiles<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgProjectAttachmentFiles<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn lock(&mut self) -> Result<(), Error> {
        serializable(&mut *self.connection).await?;
        sqlx::query(
            "LOCK TABLE attachment_sets,attachment_links,attachments,attachment_events \
             IN SHARE ROW EXCLUSIVE MODE",
        )
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn rows(&mut self, ids: &[String]) -> Result<Vec<ProjectLinkRow>, Error> {
        let rows = sqlx::query(
            "SELECT l.id,l.set_id,l.caption,l.state link_state,s.state set_state,\
             s.target_type,s.target_id,s.component_kind,s.purpose_key,s.expected_target_version,\
             s.policy_version,s.branch_id,s.department_id,l.attachment_id,a.source_state,\
             a.company_id,a.checksum_sha256,a.media_type,a.bytes,a.revoked_at,a.destroyed_at,\
             a.scanned_at,a.scanner_definition_at \
             FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id \
             LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id \
             WHERE l.id=ANY($1) ORDER BY l.id",
        )
        .bind(ids)
        .fetch_all(&mut *self.connection)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| ProjectLinkRow {
                link_id: row.get("id"),
                set_id: row.get("set_id"),
                source_id: row
                    .get::<Option<String>, _>("attachment_id")
                    .unwrap_or_default(),
                purpose_key: row.get("purpose_key"),
                checksum_sha256: row
                    .get::<Option<String>, _>("checksum_sha256")
                    .unwrap_or_default(),
                media_type: row
                    .get::<Option<String>, _>("media_type")
                    .unwrap_or_default(),
                bytes: row.get::<Option<i64>, _>("bytes").unwrap_or_default(),
            })
            .collect())
    }
}

impl ProjectAttachmentFilesPort for PgProjectAttachmentFiles<'_> {
    type Error = Error;

    fn bind<'a>(
        &'a mut self,
        grant: ProjectAttachmentGrant,
        command: &'a ProjectAttachmentBindingCommand,
        now: i64,
    ) -> ProjectAttachmentFuture<'a, ProjectAttachmentBindingReceipt, Self::Error> {
        Box::pin(async move {
            validate_project_attachment_binding(&grant, command)?;
            if !(0..=9_007_199_254_740_991).contains(&now) {
                return Err(attachment_invalid().into());
            }
            self.lock().await?;
            let ids = command.link_ids.clone();
            let rows = self.rows(&ids).await?;
            if rows.len() != ids.len()
                || rows
                    .iter()
                    .map(|row| row.link_id.as_str())
                    .collect::<BTreeSet<_>>()
                    .len()
                    != ids.len()
            {
                return Err(attachment_invalid().into());
            }
            let mut set_id = None;
            for row in &rows {
                set_id.get_or_insert_with(|| row.set_id.clone());
                if Some(&row.set_id) != set_id.as_ref()
                    || !valid_purpose(&row.purpose_key)
                    || row.source_id.is_empty()
                    || !valid_hash(&row.checksum_sha256)
                    || row.media_type.is_empty()
                    || !(1..=MAX_BYTES).contains(&row.bytes)
                {
                    return Err(attachment_invalid().into());
                }
            }
            let set_id = set_id.ok_or_else(attachment_invalid)?;
            // Re-read the exact Company/Project/version/policy/scan fields for
            // every selected link. A single foreign, stale or unscanned row
            // aborts the complete bind rather than silently skipping evidence.
            let checks = sqlx::query(
                "SELECT l.id,l.state link_state,s.state set_state,s.target_type,s.target_id,\
                 s.component_kind,s.purpose_key,s.expected_target_version,s.policy_version,\
                 s.branch_id,s.department_id,l.attachment_id,a.source_state,a.company_id,\
                 a.checksum_sha256,a.media_type,a.bytes,a.revoked_at,a.destroyed_at,\
                 a.scanned_at,a.scanner_definition_at \
                 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id \
                 LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id \
                 WHERE l.id=ANY($1) AND s.company_id=$2 AND s.id=$3",
            )
            .bind(&ids)
            .bind(grant.company_id.as_str())
            .bind(&set_id)
            .fetch_all(&mut *self.connection)
            .await?;
            if checks.len() != ids.len() {
                return Err(attachment_changed().into());
            }
            let command_id = command_key(&grant, command);
            for row in &checks {
                let target_type: String = row.get("target_type");
                let target_id: String = row.get("target_id");
                let component_kind: String = row.get("component_kind");
                let purpose_key: String = row.get("purpose_key");
                let expected_version: i64 = row.get("expected_target_version");
                let policy_version: i64 = row.get("policy_version");
                let link_state: String = row.get("link_state");
                let set_state: String = row.get("set_state");
                let branch_id: Option<String> = row.get("branch_id");
                let department_id: Option<String> = row.get("department_id");
                let source_state: Option<String> = row.get("source_state");
                let company_id: Option<String> = row.get("company_id");
                let checksum: Option<String> = row.get("checksum_sha256");
                let media_type: Option<String> = row.get("media_type");
                let bytes: Option<i64> = row.get("bytes");
                let revoked_at: Option<i64> = row.get("revoked_at");
                let destroyed_at: Option<i64> = row.get("destroyed_at");
                let scanned_at: Option<i64> = row.get("scanned_at");
                let scanner_definition_at: Option<i64> = row.get("scanner_definition_at");
                if target_type != "project"
                    || target_id != grant.target_id
                    || component_kind != "record"
                    || !valid_purpose(&purpose_key)
                    || expected_version != grant.expected_version
                    || policy_version != 1
                    || link_state != "draft"
                    || set_state != "draft"
                    || branch_id
                        .as_deref()
                        .is_some_and(|value| Some(value) != grant.branch_id.as_deref())
                    || department_id
                        .as_deref()
                        .is_some_and(|value| Some(value) != grant.department_id.as_deref())
                    || source_state.as_deref() != Some("available")
                    || company_id.as_deref() != Some(grant.company_id.as_str())
                    || checksum.as_deref().is_none_or(|value| !valid_hash(value))
                    || media_type.as_deref().is_none_or(str::is_empty)
                    || bytes.is_none_or(|value| !(1..=MAX_BYTES).contains(&value))
                    || revoked_at.is_some()
                    || destroyed_at.is_some()
                    || scanned_at.is_none()
                    || scanner_definition_at.is_none()
                {
                    return Err(attachment_changed().into());
                }
            }
            for row in &rows {
                let updated = sqlx::query(
                    "UPDATE attachment_links SET state='bound',bound_at=$1,binding_type=$2,\
                     binding_id=$3,caption=COALESCE($4,caption) \
                     WHERE id=$5 AND set_id=$6 AND state='draft'",
                )
                .bind(now)
                .bind(PROJECT_ATTACHMENT_BINDING_TYPE)
                .bind(&grant.target_id)
                .bind(command.caption.as_deref())
                .bind(&row.link_id)
                .bind(&set_id)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if updated != 1 {
                    return Err(attachment_changed().into());
                }
            }
            let updated = sqlx::query(
                "UPDATE attachment_sets SET state='bound',bound_at=$1,bound_command_id=$2,\
                 updated_at=$1 WHERE id=$3 AND company_id=$4 AND state='draft'",
            )
            .bind(now)
            .bind(&command_id)
            .bind(&set_id)
            .bind(grant.company_id.as_str())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(attachment_changed().into());
            }
            sqlx::query(
                "INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,\
                 details_json,created_at) VALUES($1,$2,$3,$4,'set.bound',$5,$6)",
            )
            .bind(event_id(&command_id, &set_id))
            .bind(grant.company_id.as_str())
            .bind(&set_id)
            .bind(&grant.actor_id)
            .bind(details(&rows, &grant.target_id, grant.purpose.as_str())?)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            // Re-read the exact retained state in the same unit.
            let bound: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM attachment_sets WHERE id=$1 AND company_id=$2 \
                 AND state='bound' AND bound_command_id=$3 AND bound_at=$4",
            )
            .bind(&set_id)
            .bind(grant.company_id.as_str())
            .bind(&command_id)
            .bind(now)
            .fetch_one(&mut *self.connection)
            .await?;
            if bound != 1 {
                return Err(attachment_changed().into());
            }
            let bound_links: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM attachment_links WHERE set_id=$1 AND id=ANY($2) \
                 AND state='bound' AND binding_type=$3 AND binding_id=$4 AND bound_at=$5",
            )
            .bind(&set_id)
            .bind(&ids)
            .bind(PROJECT_ATTACHMENT_BINDING_TYPE)
            .bind(&grant.target_id)
            .bind(now)
            .fetch_one(&mut *self.connection)
            .await?;
            if bound_links != ids.len() as i64 {
                return Err(attachment_changed().into());
            }
            Ok(ProjectAttachmentBindingReceipt {
                set_id,
                link_ids: command.link_ids.clone(),
                target_version: 0,
            })
        })
    }

    fn advance_target<'a>(
        &'a mut self,
        grant: &'a ProjectAttachmentGrant,
        now: i64,
    ) -> ProjectAttachmentFuture<'a, i64, Self::Error> {
        Box::pin(async move {
            if grant.target_type != ProjectAttachmentTargetType::Project
                || grant.expected_version < 1
                || grant.actor_id.is_empty()
                || grant.company_id.as_str().is_empty()
                || grant.witness.is_empty()
                || !(0..=9_007_199_254_740_991).contains(&now)
            {
                return Err(attachment_invalid().into());
            }
            serializable(&mut *self.connection).await?;
            let updated = sqlx::query(
                "UPDATE staff_projects SET version=version+1,updated_at=$1 \
                 WHERE id=$2 AND company_id=$3 AND version=$4 AND lifecycle_status='active' \
                 AND iam_same_person(owner_user_id,$5,company_id)",
            )
            .bind(now)
            .bind(&grant.target_id)
            .bind(grant.company_id.as_str())
            .bind(grant.expected_version)
            .bind(&grant.actor_id)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(attachment_changed().into());
            }
            let version: i64 = sqlx::query_scalar(
                "SELECT version FROM staff_projects WHERE id=$1 AND company_id=$2",
            )
            .bind(&grant.target_id)
            .bind(grant.company_id.as_str())
            .fetch_one(&mut *self.connection)
            .await?;
            if version != grant.expected_version + 1 {
                return Err(attachment_changed().into());
            }
            Ok(version)
        })
    }
}
