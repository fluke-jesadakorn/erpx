//! Files-owned implementation of the shared Records business-evidence port.
//!
//! The caller supplies the exact Company/branch/department scope and the
//! registered owner policy facts. Files validates current set targets, links,
//! scanned attachment sources and context, then binds the exact draft links and
//! sets inside the caller's serializable owner unit. It never commits.
use erpx_server_records::public::business_evidence::{
    BusinessEvidenceBinding, BusinessEvidenceFilesPort, EvidenceContext, EvidencePolicyFact,
    EvidenceSetFact,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_BINDING_CHANGED",
        "The original file set changed. Reload the files and try again.",
    )
    .into()
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::InvalidInput, code, message).into()
}

fn missing() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_SET_NOT_FOUND",
        "File set not found.",
    )
    .into()
}

fn conflict(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
}

fn empty() -> Error {
    BusinessError::new(
        FailureKind::InvalidInput,
        "ATTACHMENT_SET_EMPTY",
        "Bind at least one available file.",
    )
    .into()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedEvidenceLink {
    pub set_id: String,
    pub link_id: String,
    pub source_id: String,
    pub purpose_key: String,
    pub component_kind: String,
    pub component_ref: Option<String>,
    pub checksum_sha256: String,
    pub media_type: String,
    pub bytes: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedBusinessEvidence {
    pub set_ids: Vec<String>,
    pub link_ids: Vec<String>,
    pub manifest: Vec<PreparedEvidenceLink>,
    pub target_type: String,
    pub target_id: String,
    pub expected_target_version: i64,
    pub policy_version: i64,
    pub binding_type: String,
    pub binding_id: String,
    pub command_id: String,
}

/// Acquire the Files owner bundle lock before the first protected read or bind.
/// The enclosing owner unit supplies the transaction and its earlier source
/// locks; this helper performs no transaction check of its own.
pub async fn lock_business_evidence_sources(c: &mut PgConnection) -> Result<(), Error> {
    sqlx::query(
        "LOCK TABLE attachments,upload_intents,attachment_sets,attachment_links,attachment_events \
         IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

pub struct PgBusinessEvidenceFiles<'c> {
    connection: &'c mut PgConnection,
    company: CompanyId,
    actor_id: String,
    branch_id: Option<String>,
    department_id: Option<String>,
    policies: Vec<EvidencePolicyFact>,
    now: TimestampMillis,
}

impl<'c> PgBusinessEvidenceFiles<'c> {
    pub fn new(
        connection: &'c mut PgConnection,
        company: CompanyId,
        actor_id: String,
        branch_id: Option<String>,
        department_id: Option<String>,
        policies: Vec<EvidencePolicyFact>,
        now: TimestampMillis,
    ) -> Self {
        Self {
            connection,
            company,
            actor_id,
            branch_id,
            department_id,
            policies,
            now,
        }
    }

    async fn unit(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());
        }
        let level: String = sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?;
        if level != "serializable" {
            return Err(PersistenceError::new("FILES_SERIALIZABLE_REQUIRED").into());
        }
        // Files owner bundle lock, acquired before the first Files read/bind.
        // The enclosing owner unit must already hold its own source locks.
        lock_business_evidence_sources(&mut *self.connection).await?;
        Ok(())
    }

    /// Re-read the exact bound sets and links after every owner effect in the
    /// same serializable unit. Any drift, missing row, extra link or foreign
    /// Company row is a binding conflict.
    pub async fn require_current(
        &mut self,
        prepared: &PreparedBusinessEvidence,
    ) -> Result<(), Error> {
        self.unit().await?;
        let sets = sqlx::query(
            "SELECT id,company_id,target_type,target_id,expected_target_version,policy_version,state,bound_command_id,\
             component_kind,component_ref,purpose_key \
             FROM attachment_sets WHERE id = ANY($1)",
        )
        .bind(&prepared.set_ids)
        .fetch_all(&mut *self.connection)
        .await?;
        if sets.len() != prepared.set_ids.len() {
            return Err(changed());
        }
        for row in &sets {
            let id: String = row.try_get("id")?;
            let company: String = row.try_get("company_id")?;
            let target_type: String = row.try_get("target_type")?;
            let target_id: String = row.try_get("target_id")?;
            let version: i64 = row.try_get("expected_target_version")?;
            let policy_version: i64 = row.try_get("policy_version")?;
            let state: String = row.try_get("state")?;
            let command: Option<String> = row.try_get("bound_command_id")?;
            let component_kind: String = row.try_get("component_kind")?;
            let component_ref: Option<String> = row.try_get("component_ref")?;
            let purpose: String = row.try_get("purpose_key")?;
            let manifest_matches = prepared
                .manifest
                .iter()
                .filter(|item| item.set_id == id)
                .all(|item| {
                    item.component_kind == component_kind
                        && item.component_ref == component_ref
                        && item.purpose_key == purpose
                });
            if !prepared.set_ids.iter().any(|selected| selected == &id)
                || company != self.company.as_str()
                || target_type != prepared.target_type
                || target_id != prepared.target_id
                || version != prepared.expected_target_version
                || policy_version != prepared.policy_version
                || state != "bound"
                || command.as_deref() != Some(prepared.command_id.as_str())
                || !manifest_matches
            {
                return Err(changed());
            }
        }
        let links = sqlx::query(
            "SELECT l.id,l.set_id,l.state,l.binding_type,l.binding_id,l.attachment_id,\
             a.company_id,a.checksum_sha256,a.media_type,a.bytes \
             FROM attachment_links l \
             LEFT JOIN attachments a ON a.id=l.attachment_id \
             WHERE l.set_id = ANY($1)",
        )
        .bind(&prepared.set_ids)
        .fetch_all(&mut *self.connection)
        .await?;
        if links.len() != prepared.link_ids.len() || links.len() != prepared.manifest.len() {
            return Err(changed());
        }
        for row in &links {
            let id: String = row.try_get("id")?;
            let set_id: String = row.try_get("set_id")?;
            let state: String = row.try_get("state")?;
            let binding_type: Option<String> = row.try_get("binding_type")?;
            let binding_id: Option<String> = row.try_get("binding_id")?;
            let attachment_id: Option<String> = row.try_get("attachment_id")?;
            let company: Option<String> = row.try_get("company_id")?;
            let checksum: Option<String> = row.try_get("checksum_sha256")?;
            let media_type: Option<String> = row.try_get("media_type")?;
            let bytes: Option<i64> = row.try_get("bytes")?;
            let manifest = prepared
                .manifest
                .iter()
                .find(|item| item.link_id == id && item.set_id == set_id);
            if !prepared.link_ids.iter().any(|selected| selected == &id)
                || !prepared.set_ids.iter().any(|selected| selected == &set_id)
                || state != "bound"
                || binding_type.as_deref() != Some(prepared.binding_type.as_str())
                || binding_id.as_deref() != Some(prepared.binding_id.as_str())
                || attachment_id.is_none()
                || company.as_deref() != Some(self.company.as_str())
                || manifest.is_none_or(|item| {
                    Some(item.source_id.as_str()) != attachment_id.as_deref()
                        || Some(item.checksum_sha256.as_str()) != checksum.as_deref()
                        || Some(item.media_type.as_str()) != media_type.as_deref()
                        || Some(item.bytes) != bytes
                })
            {
                return Err(changed());
            }
        }
        Ok(())
    }
}

impl BusinessEvidenceFilesPort for PgBusinessEvidenceFiles<'_> {
    type Error = Error;
    type Prepared = PreparedBusinessEvidence;

    async fn read_set_targets(
        &mut self,
        ids: &[String],
    ) -> Result<Vec<EvidenceSetFact>, Self::Error> {
        self.unit().await?;
        let rows = sqlx::query(
            "SELECT id,target_type FROM attachment_sets \
             WHERE id = ANY($1) AND company_id=$2 \
             AND (branch_id IS NULL OR branch_id=$3) \
             AND (department_id IS NULL OR department_id=$4)",
        )
        .bind(ids)
        .bind(self.company.as_str())
        .bind(self.branch_id.as_deref())
        .bind(self.department_id.as_deref())
        .fetch_all(&mut *self.connection)
        .await?;
        rows.iter()
            .map(|row| {
                Ok::<_, Error>(EvidenceSetFact {
                    id: row.try_get("id")?,
                    target_type: row.try_get("target_type")?,
                })
            })
            .collect()
    }

    fn policy_facts(&self) -> &[EvidencePolicyFact] {
        &self.policies
    }

    async fn prepare_binding(
        &mut self,
        input: BusinessEvidenceBinding,
    ) -> Result<Self::Prepared, Self::Error> {
        self.unit().await?;
        if input.target_id.is_empty()
            || input.expected_target_version < 1
            || input.command_id.is_empty()
            || input.binding_type.is_empty()
            || input.binding_id.is_empty()
            || input.selection.set_ids.is_empty()
            || input.selection.set_ids.len() > 20
        {
            return Err(invalid(
                "ATTACHMENT_SETS_REQUIRED",
                "Choose one to twenty file sets.",
            ));
        }
        let rows = sqlx::query(
            "SELECT id,target_type,target_id,expected_target_version,policy_version,component_kind,\
             component_ref,purpose_key,required_count,max_files,state \
             FROM attachment_sets WHERE id = ANY($1) AND company_id=$2 \
             AND (branch_id IS NULL OR branch_id=$3) \
             AND (department_id IS NULL OR department_id=$4) FOR UPDATE",
        )
        .bind(&input.selection.set_ids)
        .bind(self.company.as_str())
        .bind(self.branch_id.as_deref())
        .bind(self.department_id.as_deref())
        .fetch_all(&mut *self.connection)
        .await?;
        if rows.len() != input.selection.set_ids.len() {
            return Err(missing());
        }
        let mut link_ids = Vec::new();
        let mut manifest = Vec::new();
        for row in &rows {
            let id: String = row.try_get("id")?;
            let target_type: String = row.try_get("target_type")?;
            let target_id: String = row.try_get("target_id")?;
            let version: i64 = row.try_get("expected_target_version")?;
            let policy_version: i64 = row.try_get("policy_version")?;
            let component_kind: String = row.try_get("component_kind")?;
            let component_ref: Option<String> = row.try_get("component_ref")?;
            let purpose: String = row.try_get("purpose_key")?;
            let required: i64 = row.try_get("required_count")?;
            let maximum: i64 = row.try_get("max_files")?;
            let state: String = row.try_get("state")?;
            if target_type != input.selection.target_type || target_id != input.target_id {
                return Err(invalid(
                    "ATTACHMENT_SET_TARGET",
                    "A file set belongs to another record.",
                ));
            }
            if !input.selection.contexts.iter().any(|context| {
                context.purpose_key == purpose
                    && context.component_kind == component_kind
                    && (context.component_ref.is_none()
                        || context.component_ref == component_ref)
            }) {
                return Err(invalid(
                    "ATTACHMENT_BINDING_CONTEXT",
                    "Choose only files prepared for this exact domain action and component.",
                ));
            }
            if state != "draft" {
                return Err(conflict(
                    "ATTACHMENT_SET_LOCKED",
                    "A file set was already bound or closed.",
                ));
            }
            if version != input.expected_target_version
                || policy_version != input.selection.policy_version
            {
                return Err(conflict(
                    "ATTACHMENT_SET_STALE",
                    "Create a file set for the current record version.",
                ));
            }
            let links = sqlx::query(
                "SELECT l.id,l.state,l.attachment_id,a.source_state,a.checksum_sha256,\
                 a.media_type,a.bytes \
                 FROM attachment_links l \
                 LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=$2 \
                 WHERE l.set_id=$1 ORDER BY l.created_at,l.id",
            )
            .bind(&id)
            .bind(self.company.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            if links.is_empty() {
                return Err(empty());
            }
            if (links.len() as i64) > maximum {
                return Err(changed());
            }
            if links.iter().any(|link| {
                link.try_get::<String, _>("state")
                    .map(|state| state != "draft")
                    .unwrap_or(true)
            }) {
                return Err(conflict(
                    "ATTACHMENT_SET_LOCKED",
                    "A file set was already bound or closed.",
                ));
            }
            if links.iter().any(|link| {
                link.try_get::<Option<String>, _>("source_state")
                    .ok()
                    .flatten()
                    .as_deref()
                    != Some("available")
                    || link
                        .try_get::<Option<String>, _>("attachment_id")
                        .ok()
                        .flatten()
                        .is_none()
                    || link
                        .try_get::<Option<String>, _>("checksum_sha256")
                        .ok()
                        .flatten()
                        .is_none()
                    || link
                        .try_get::<Option<String>, _>("media_type")
                        .ok()
                        .flatten()
                        .is_none()
                    || link
                        .try_get::<Option<i64>, _>("bytes")
                        .ok()
                        .flatten()
                        .is_none()
            }) {
                return Err(conflict(
                    "ATTACHMENT_SCAN_PENDING",
                    "Every active file must pass malware scanning before this action.",
                ));
            }
            if (links.len() as i64) < required {
                return Err(invalid(
                    "ATTACHMENT_REQUIRED",
                    "The reviewed file set does not meet its required count.",
                ));
            }
            for link in &links {
                let link_id: String = link.try_get("id")?;
                let source_id: String = link
                    .try_get::<Option<String>, _>("attachment_id")?
                    .ok_or_else(changed)?;
                let checksum: String = link
                    .try_get::<Option<String>, _>("checksum_sha256")?
                    .ok_or_else(changed)?;
                let media_type: String = link
                    .try_get::<Option<String>, _>("media_type")?
                    .ok_or_else(changed)?;
                let bytes: i64 = link
                    .try_get::<Option<i64>, _>("bytes")?
                    .ok_or_else(changed)?;
                let updated = sqlx::query(
                    "UPDATE attachment_links SET state='bound',bound_at=$1,\
                     binding_type=$2,binding_id=$3 \
                     WHERE id=$4 AND set_id=$5 AND state='draft'",
                )
                .bind(self.now.get())
                .bind(&input.binding_type)
                .bind(&input.binding_id)
                .bind(&link_id)
                .bind(&id)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if updated != 1 {
                    return Err(changed());
                }
                link_ids.push(link_id.clone());
                manifest.push(PreparedEvidenceLink {
                    set_id: id.clone(),
                    link_id,
                    source_id,
                    purpose_key: purpose.clone(),
                    component_kind: component_kind.clone(),
                    component_ref: component_ref.clone(),
                    checksum_sha256: checksum,
                    media_type,
                    bytes,
                });
            }
            let updated = sqlx::query(
                "UPDATE attachment_sets SET state='bound',bound_at=$1,\
                 bound_command_id=$2,updated_at=$1 \
                 WHERE id=$3 AND company_id=$4 AND state='draft'",
            )
            .bind(self.now.get())
            .bind(&input.command_id)
            .bind(&id)
            .bind(self.company.as_str())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(changed());
            }
            let event = format!("attachment-bind-{}-{}", input.command_id, id);
            sqlx::query(
                "INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,\
                 details_json,created_at) VALUES($1,$2,$3,$4,'set.bound',$5,$6)",
            )
            .bind(event)
            .bind(self.company.as_str())
            .bind(&id)
            .bind(&self.actor_id)
            .bind(
                serde_json::json!({
                    "bindingType": input.binding_type,
                    "bindingId": input.binding_id,
                    "targetId": input.target_id,
                    "expectedTargetVersion": input.expected_target_version,
                    "manifest": manifest
                        .iter()
                        .filter(|item| item.set_id == id)
                        .map(|item| serde_json::json!({
                            "linkId": item.link_id,
                            "sourceId": item.source_id,
                            "purposeKey": item.purpose_key,
                            "checksumSha256": item.checksum_sha256,
                            "mediaType": item.media_type,
                            "bytes": item.bytes,
                        }))
                        .collect::<Vec<_>>(),
                })
                .to_string(),
            )
            .bind(self.now.get())
            .execute(&mut *self.connection)
            .await?;
        }
        // Re-read the exact bound state within the same unit.
        let bound: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM attachment_sets WHERE id=ANY($1) AND company_id=$2 \
             AND state='bound' AND bound_command_id=$3 AND bound_at=$4",
        )
        .bind(&input.selection.set_ids)
        .bind(self.company.as_str())
        .bind(&input.command_id)
        .bind(self.now.get())
        .fetch_one(&mut *self.connection)
        .await?;
        if bound != input.selection.set_ids.len() as i64 {
            return Err(changed());
        }
        Ok(PreparedBusinessEvidence {
            set_ids: input.selection.set_ids,
            link_ids,
            manifest,
            target_type: input.selection.target_type,
            target_id: input.target_id,
            expected_target_version: input.expected_target_version,
            policy_version: input.selection.policy_version,
            binding_type: input.binding_type,
            binding_id: input.binding_id,
            command_id: input.command_id,
        })
    }
}

/// Owner policy facts are a caller-owned static registry slice. Files does not
/// grant a context that the owner has not registered.
pub fn policy_fact(
    target_type: &str,
    purpose_key: &str,
    component_kinds: &[&str],
    policy_version: i64,
) -> EvidencePolicyFact {
    EvidencePolicyFact {
        target_type: target_type.to_string(),
        purpose_key: purpose_key.to_string(),
        component_kinds: component_kinds
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        policy_version,
    }
}

pub fn evidence_context(
    purpose_key: &str,
    component_kind: &str,
    component_ref: Option<&str>,
) -> EvidenceContext {
    EvidenceContext {
        purpose_key: purpose_key.to_string(),
        component_kind: component_kind.to_string(),
        component_ref: component_ref.map(str::to_owned),
    }
}
