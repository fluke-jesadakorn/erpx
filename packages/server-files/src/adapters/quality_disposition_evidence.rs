//! Files-owned Quality disposition evidence binding.
//!
//! The port validates an exact draft `disposition_evidence` set for one Quality
//! hold and then binds its links and set to the created scrap Count. It grants
//! no Quality record authority and reads no Quality-owned relation.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
use std::collections::{HashMap, HashSet};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

const MAX_SETS: usize = 10;
const MAX_LINKS: usize = 1000;
const MAX_ID_BYTES: usize = 200;
const MAX_METADATA_BYTES: usize = 1024 * 1024;
const MAX_SAFE: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityDispositionEvidenceManifestItem {
    pub link_id: String,
    pub source_id: String,
    pub purpose_key: String,
    pub checksum_sha256: String,
    pub media_type: String,
    pub bytes: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LinkImage {
    id: String,
    attachment_id: String,
    checksum_sha256: String,
    media_type: String,
    bytes: i64,
    metadata: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SetImage {
    id: String,
    metadata: String,
    links: Vec<LinkImage>,
    manifest: Vec<QualityDispositionEvidenceManifestItem>,
}

pub struct CurrentQualityDispositionEvidenceSets {
    unit: Unit,
    company: CompanyId,
    branch: RecordId,
    owner: String,
    expected_version: i64,
    sets: Vec<SetImage>,
    manifest: Vec<QualityDispositionEvidenceManifestItem>,
}

impl CurrentQualityDispositionEvidenceSets {
    pub fn manifest(&self) -> &[QualityDispositionEvidenceManifestItem] {
        &self.manifest
    }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || snapshot(
                c,
                &self.company,
                &self.branch,
                &self.owner,
                self.expected_version,
                &self
                    .sets
                    .iter()
                    .map(|set| set.id.clone())
                    .collect::<Vec<_>>(),
            )
            .await?
            .as_ref()
                != Some(&self.sets)
        {
            return Err(changed());
        }
        Ok(())
    }

    /// Bind each current draft set and every one of its links to the created
    /// disposition Count in the caller's serializable unit.
    pub async fn apply(
        &self,
        c: &mut PgConnection,
        actor: &str,
        command_key: &str,
        count_id: &str,
        now: i64,
    ) -> Result<(), Error> {
        self.require_current(c).await?;
        if !(0..=MAX_SAFE).contains(&now)
            || actor.is_empty()
            || actor.len() > 1024
            || command_key.is_empty()
            || command_key.len() > 200
            || count_id.is_empty()
            || count_id.len() > 200
        {
            return Err(invalid());
        }
        for set in &self.sets {
            for link in &set.links {
                let updated = sqlx::query(
                    "UPDATE attachment_links SET state='bound',bound_at=$1,binding_type='quality_disposition',binding_id=$2 \
                     WHERE id=$3 AND set_id=$4 AND state='draft'",
                )
                .bind(now)
                .bind(count_id)
                .bind(&link.id)
                .bind(&set.id)
                .execute(&mut *c)
                .await?;
                if updated.rows_affected() != 1 {
                    return Err(changed());
                }
            }
            let updated = sqlx::query(
                "UPDATE attachment_sets SET state='bound',bound_at=$1,bound_command_id=$2,updated_at=$1 \
                 WHERE id=$3 AND company_id=$4 AND state='draft'",
            )
            .bind(now)
            .bind(command_key)
            .bind(&set.id)
            .bind(self.company.as_str())
            .execute(&mut *c)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(changed());
            }
            sqlx::query(
                "INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) \
                 VALUES($1,$2,$3,$4,'set.bound',$5,$6)",
            )
            .bind(event_id(command_key, &set.id))
            .bind(self.company.as_str())
            .bind(&set.id)
            .bind(actor)
            .bind(details(count_id, &set.manifest)?)
            .bind(now)
            .execute(&mut *c)
            .await?;
        }
        self.require_after(c, actor, command_key, count_id, now)
            .await
    }

    pub async fn require_after(
        &self,
        c: &mut PgConnection,
        actor: &str,
        command_key: &str,
        count_id: &str,
        now: i64,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        for set in &self.sets {
            let row = sqlx::query(
                "SELECT state,bound_at,bound_command_id,updated_at,company_id,target_type,target_id,purpose_key,component_kind,component_ref,expected_target_version,required_count,max_files,branch_id \
                 FROM attachment_sets WHERE id=$1 AND company_id=$2",
            )
            .bind(&set.id)
            .bind(self.company.as_str())
            .fetch_optional(&mut *c)
            .await?
            .ok_or_else(changed)?;
            if row.try_get::<String, _>("state")? != "bound"
                || row.try_get::<Option<i64>, _>("bound_at")? != Some(now)
                || row
                    .try_get::<Option<String>, _>("bound_command_id")?
                    .as_deref()
                    != Some(command_key)
                || row.try_get::<i64, _>("updated_at")? != now
                || row.try_get::<String, _>("target_type")? != "quality_hold"
                || row.try_get::<String, _>("target_id")? != self.owner
                || row.try_get::<String, _>("purpose_key")? != "disposition_evidence"
                || row.try_get::<String, _>("component_kind")? != "action"
                || row
                    .try_get::<Option<String>, _>("component_ref")?
                    .as_deref()
                    != Some("disposition")
                || row.try_get::<i64, _>("expected_target_version")? != self.expected_version
                || row
                    .try_get::<Option<String>, _>("branch_id")?
                    .as_deref()
                    .is_some_and(|branch| branch != self.branch.as_str())
            {
                return Err(changed());
            }
            for link in &set.links {
                let row = sqlx::query(
                    "SELECT state,bound_at,binding_type,binding_id FROM attachment_links WHERE id=$1 AND set_id=$2",
                )
                .bind(&link.id)
                .bind(&set.id)
                .fetch_optional(&mut *c)
                .await?
                .ok_or_else(changed)?;
                if row.try_get::<String, _>("state")? != "bound"
                    || row.try_get::<Option<i64>, _>("bound_at")? != Some(now)
                    || row.try_get::<Option<String>, _>("binding_type")?.as_deref()
                        != Some("quality_disposition")
                    || row.try_get::<Option<String>, _>("binding_id")?.as_deref() != Some(count_id)
                {
                    return Err(changed());
                }
            }
            let event: Option<String> = sqlx::query_scalar(
                "SELECT details_json FROM attachment_events WHERE id=$1 AND company_id=$2 AND set_id=$3 AND actor_id=$4 AND action='set.bound' AND created_at=$5",
            )
            .bind(event_id(command_key, &set.id))
            .bind(self.company.as_str())
            .bind(&set.id)
            .bind(actor)
            .bind(now)
            .fetch_optional(&mut *c)
            .await?;
            let event = event.ok_or_else(changed)?;
            let actual: Value = serde_json::from_str(&event).map_err(|_| changed())?;
            let expected: Value =
                serde_json::from_str(&details(count_id, &set.manifest)?).map_err(|_| changed())?;
            if actual != expected {
                return Err(changed());
            }
        }
        Ok(())
    }
}

fn invalid() -> Error {
    BusinessError::invalid(
        "QUALITY_ATTACHMENT_INVALID",
        "Choose ready, unbound disposition evidence uploaded by the current Quality manager.",
    )
    .into()
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_ATTACHMENT_CHANGED",
        "The disposition evidence changed. Reload and review before retrying.",
    )
    .into()
}

fn limit() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_QUALITY_DISPOSITION_LIMIT",
        "The disposition evidence metadata exceeds the bounded read.",
    )
    .into()
}

/// Utility-only locks; acquire before the first serializable snapshot with the
/// other owner sources.
pub async fn lock_quality_disposition_evidence_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());
    }
    sqlx::query(
        "LOCK TABLE attachments,upload_intents,attachment_sets,attachment_links,attachment_events IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}

fn event_id(command_key: &str, set_id: &str) -> String {
    format!(
        "attachment-bind-{}",
        &hex::encode(Sha256::digest(
            format!("{command_key}\0{set_id}").as_bytes()
        ))[..32]
    )
}

fn details(
    count_id: &str,
    manifest: &[QualityDispositionEvidenceManifestItem],
) -> Result<String, Error> {
    let manifest = manifest
        .iter()
        .map(|item| {
            json!({
                "linkId": item.link_id,
                "sourceId": item.source_id,
                "purposeKey": item.purpose_key,
                "checksumSha256": item.checksum_sha256,
                "mediaType": item.media_type,
                "bytes": item.bytes,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&json!({
        "bindingType": "quality_disposition",
        "bindingId": count_id,
        "manifest": manifest,
    }))
    .map(|value| value.replace('<', "\\u003c"))
    .map_err(|_| changed())
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FILES_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}

async fn snapshot(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: &RecordId,
    owner: &str,
    expected_version: i64,
    set_ids: &[String],
) -> Result<Option<Vec<SetImage>>, Error> {
    if set_ids.is_empty()
        || set_ids.len() > MAX_SETS
        || expected_version < 1
        || owner.is_empty()
        || owner.len() > 1024
        || set_ids
            .iter()
            .any(|id| id.is_empty() || id.len() > MAX_ID_BYTES)
        || set_ids.iter().collect::<HashSet<_>>().len() != set_ids.len()
    {
        return Err(invalid());
    }
    let rows = sqlx::query(
        "SELECT id,branch_id,target_type,target_id,purpose_key,component_kind,component_ref,state,expected_target_version,required_count,max_files \
         FROM attachment_sets WHERE company_id=$1 AND id=ANY($2::text[])",
    )
    .bind(company.as_str())
    .bind(set_ids)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() != set_ids.len() {
        return Ok(None);
    }
    let mut sets = HashMap::with_capacity(rows.len());
    for row in rows {
        let id: String = row.try_get("id")?;
        let branch_id: Option<String> = row.try_get("branch_id")?;
        let target_type: String = row.try_get("target_type")?;
        let target_id: String = row.try_get("target_id")?;
        let purpose_key: String = row.try_get("purpose_key")?;
        let component_kind: String = row.try_get("component_kind")?;
        let component_ref: Option<String> = row.try_get("component_ref")?;
        let state: String = row.try_get("state")?;
        let expected_target_version: i64 = row.try_get("expected_target_version")?;
        let required_count: i64 = row.try_get("required_count")?;
        let max_files: i64 = row.try_get("max_files")?;
        if target_type != "quality_hold"
            || target_id != owner
            || purpose_key != "disposition_evidence"
            || component_kind != "action"
            || component_ref.as_deref() != Some("disposition")
            || state != "draft"
            || expected_target_version != expected_version
            || branch_id
                .as_deref()
                .is_some_and(|value| value != branch.as_str())
            || required_count < 0
            || required_count > MAX_LINKS as i64
            || !(1..=100).contains(&max_files)
        {
            return Ok(None);
        }
        let metadata = serde_json::to_string(&(
            id.as_str(),
            branch_id.as_deref(),
            target_type.as_str(),
            target_id.as_str(),
            purpose_key.as_str(),
            component_kind.as_str(),
            component_ref.as_deref(),
            state.as_str(),
            expected_target_version,
            required_count,
            max_files,
        ))
        .map_err(|_| limit())?;
        sets.insert(
            id,
            SetImage {
                id: String::new(),
                metadata,
                links: Vec::new(),
                manifest: Vec::new(),
            },
        );
    }
    let rows = sqlx::query(
        "SELECT l.set_id,l.id,l.attachment_id,l.state,a.original_name,a.media_type,a.bytes,a.checksum_sha256,a.source_state,a.owner_type,a.owner_id,a.purpose,a.revoked_at \
         FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id AND s.company_id=$1 \
         LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id \
         WHERE l.set_id=ANY($2::text[]) ORDER BY l.set_id,l.created_at,l.id LIMIT 1001",
    )
    .bind(company.as_str())
    .bind(set_ids)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > MAX_LINKS {
        return Err(limit());
    }
    let mut total = 0_usize;
    for row in rows {
        let set_id: String = row.try_get("set_id")?;
        let link_id: String = row.try_get("id")?;
        let state: String = row.try_get("state")?;
        let attachment_id: String = match row.try_get("attachment_id")? {
            Some(value) => value,
            None => return Ok(None),
        };
        let media_type: Option<String> = row.try_get("media_type")?;
        let checksum_sha256: Option<String> = row.try_get("checksum_sha256")?;
        let bytes: Option<i64> = row.try_get("bytes")?;
        let source_state: Option<String> = row.try_get("source_state")?;
        let owner_type: Option<String> = row.try_get("owner_type")?;
        let owner_id: Option<String> = row.try_get("owner_id")?;
        let purpose: Option<String> = row.try_get("purpose")?;
        let revoked_at: Option<i64> = row.try_get("revoked_at")?;
        if state != "draft"
            || owner_type.as_deref() != Some("quality_hold")
            || owner_id.as_deref() != Some(owner)
            || purpose.as_deref() != Some("disposition_evidence")
            || revoked_at.is_some()
            || source_state.as_deref() != Some("available")
            || media_type.as_deref().is_none_or(str::is_empty)
            || checksum_sha256.as_deref().is_none_or(str::is_empty)
            || bytes.is_none_or(|value| !(0..=MAX_SAFE).contains(&value))
        {
            return Ok(None);
        }
        let media_type = media_type.unwrap();
        let checksum_sha256 = checksum_sha256.unwrap();
        let bytes = bytes.unwrap();
        total = total
            .checked_add(link_id.len())
            .and_then(|n| n.checked_add(attachment_id.len()))
            .and_then(|n| n.checked_add(media_type.len()))
            .and_then(|n| n.checked_add(checksum_sha256.len()))
            .ok_or_else(limit)?;
        if total > MAX_METADATA_BYTES {
            return Err(limit());
        }
        let metadata = serde_json::to_string(&(
            link_id.as_str(),
            attachment_id.as_str(),
            media_type.as_str(),
            checksum_sha256.as_str(),
            bytes,
            "available",
        ))
        .map_err(|_| limit())?;
        let set = match sets.get_mut(&set_id) {
            Some(value) => value,
            None => return Ok(None),
        };
        set.manifest.push(QualityDispositionEvidenceManifestItem {
            link_id: link_id.clone(),
            source_id: attachment_id.clone(),
            purpose_key: "disposition_evidence".into(),
            checksum_sha256: checksum_sha256.clone(),
            media_type: media_type.clone(),
            bytes,
        });
        set.links.push(LinkImage {
            id: link_id,
            attachment_id,
            checksum_sha256,
            media_type,
            bytes,
            metadata,
        });
    }
    let mut ordered = Vec::with_capacity(set_ids.len());
    for id in set_ids {
        let mut set = match sets.remove(id) {
            Some(value) => value,
            None => return Ok(None),
        };
        if set.links.is_empty() || set.links.len() > MAX_LINKS {
            return Ok(None);
        }
        set.id = id.clone();
        ordered.push(set);
    }
    Ok(Some(ordered))
}

/// Validate an exact draft disposition evidence set for this hold and branch.
/// `None` means a set is missing, belongs to another owner/version, is not a
/// current draft, or contains a link whose attachment is not ready.
pub async fn inspect_quality_disposition_evidence_sets(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: &RecordId,
    owner: &str,
    expected_version: i64,
    set_ids: &[String],
) -> Result<Option<CurrentQualityDispositionEvidenceSets>, Error> {
    let captured = unit(c).await?;
    let Some(sets) = snapshot(c, company, branch, owner, expected_version, set_ids).await? else {
        return Ok(None);
    };
    if unit(c).await? != captured {
        return Err(changed());
    }
    let manifest = sets
        .iter()
        .flat_map(|set| set.manifest.clone())
        .collect::<Vec<_>>();
    Ok(Some(CurrentQualityDispositionEvidenceSets {
        unit: captured,
        company: company.clone(),
        branch: branch.clone(),
        owner: owner.to_owned(),
        expected_version,
        sets,
        manifest,
    }))
}
