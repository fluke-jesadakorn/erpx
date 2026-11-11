//! Files-owned adapter for the native Projects attachment binding port.
//!
//! Projects authorizes the current project owner and expected version. This
//! adapter binds the exact current draft Files links and applies the Project
//! version step inside the caller's retained serializable transaction. It never
//! creates, scans, stores or reads attachment bytes and never commits.
use erpx_server_projects::public::project_attachment::{
    PgProjectAttachmentFacts, ProjectAttachmentActor, ProjectAttachmentBindingCommand,
    ProjectAttachmentBindingReceipt, ProjectAttachmentFactsRepository, ProjectAttachmentFilesPort,
    ProjectAttachmentFuture, ProjectAttachmentGrant, ProjectAttachmentTargetFacts,
    ProjectAttachmentTargetType, bind_project_attachment,
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
