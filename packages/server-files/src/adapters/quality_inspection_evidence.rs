//! Files-owned source facts for Quality inspection evidence.
//!
//! The direct port proves only the requested Files rows, their company/branch
//! scope, ownership, ready state and absence of revocation. The set port proves
//! the exact draft membership of Quality inspection sets without granting
//! Quality record authority. Neither port reads Quality-owned relations;
//! composition must retain both witnesses and separately reject an already-used
//! attachment.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use sqlx::{Connection, PgConnection, Row};
use std::collections::{HashMap, HashSet};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

const MAX_IDS: usize = 10;
const MAX_ID_BYTES: usize = 200;
const MAX_METADATA_BYTES: usize = 1024 * 1024;
const MAX_SAFE: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualityInspectionEvidence {
    pub id: String,
    pub original_name: String,
    pub media_type: String,
    pub bytes: i64,
    pub checksum_sha256: String,
    pub source_state: String,
}

fn invalid() -> Error {
    BusinessError::invalid(
        "QUALITY_ATTACHMENT_INVALID",
        "Choose ready, unbound inspection evidence uploaded by the current inspector.",
    )
    .into()
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_ATTACHMENT_CHANGED",
        "The inspection evidence changed. Reload and review before retrying.",
    )
    .into()
}

/// Utility-only lock acquired before the first serializable snapshot with the
/// other owner sources. Quality-owned reference rows remain outside this lock.
pub async fn lock_quality_inspection_evidence_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE attachments,upload_intents,attachment_sets IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("FILES_SERIALIZABLE_REQUIRED").into());
    }
    let held: bool = sqlx::query_scalar("SELECT count(DISTINCT relation)=3 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('attachments'::regclass,'upload_intents'::regclass,'attachment_sets'::regclass)")
        .fetch_one(&mut *c)
        .await?;
    if !held {
        return Err(PersistenceError::new("FILES_INSPECTION_EVIDENCE_LOCKS_REQUIRED").into());
    }
    Ok(
        sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text")
            .fetch_one(c)
            .await?,
    )
}

async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: &RecordId,
    owner: &str,
    creator: &str,
    ids: &[String],
) -> Result<Option<Vec<QualityInspectionEvidence>>, Error> {
    if ids.is_empty()
        || ids.len() > MAX_IDS
        || ids
            .iter()
            .any(|id| id.is_empty() || id.len() > MAX_ID_BYTES)
        || ids.iter().collect::<HashSet<_>>().len() != ids.len()
    {
        return Err(invalid());
    }
    // Revocation and scan state are evaluated in the row predicate, but the
    // legacy writer can leave company_id NULL. Such a row is never accepted as
    // current-company evidence, matching the native upload completion path.
    let rows = sqlx::query(
        "SELECT a.id,a.original_name,a.media_type,a.bytes,a.checksum_sha256,a.source_state \
         FROM attachments a JOIN upload_intents u ON u.id=a.upload_id \
         WHERE a.id=ANY($1::text[]) AND a.company_id=$2 AND a.owner_type='quality_hold' \
           AND a.owner_id=$3 AND a.purpose='quality_inspection_evidence' \
           AND a.created_by=$4 AND a.revoked_at IS NULL AND a.source_state='available' \
           AND u.company_id=$2 AND u.owner_type='quality_hold' AND u.owner_id=$3 \
           AND u.purpose='quality_inspection_evidence' AND u.created_by=$4 AND u.status='ready' \
           AND (u.attachment_set_id IS NULL OR EXISTS(\
             SELECT 1 FROM attachment_sets s WHERE s.id=u.attachment_set_id \
               AND s.company_id=$2 AND s.target_type='quality_hold' AND s.target_id=$3 \
               AND s.purpose_key='quality_inspection_evidence' \
               AND (s.branch_id IS NULL OR s.branch_id=$5))) \
         ORDER BY a.id",
    )
    .bind(ids)
    .bind(company.as_str())
    .bind(owner)
    .bind(creator)
    .bind(branch.as_str())
    .fetch_all(&mut *c)
    .await?;
    if rows.len() != ids.len() {
        return Ok(None);
    }
    let mut by_id = HashMap::with_capacity(rows.len());
    let mut total = 0_usize;
    for row in rows {
        let id: String = row.try_get("id")?;
        let original_name: String = row.try_get("original_name")?;
        let media_type: String = row.try_get("media_type")?;
        let checksum_sha256: String = row.try_get("checksum_sha256")?;
        let bytes: i64 = row.try_get("bytes")?;
        let source_state: String = row.try_get("source_state")?;
        if !(0..=MAX_SAFE).contains(&bytes) {
            return Err(invalid());
        }
        total = total
            .checked_add(id.len())
            .and_then(|n| n.checked_add(original_name.len()))
            .and_then(|n| n.checked_add(media_type.len()))
            .and_then(|n| n.checked_add(checksum_sha256.len()))
            .ok_or_else(invalid)?;
        if total > MAX_METADATA_BYTES {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "FILES_INSPECTION_EVIDENCE_LIMIT",
                "The inspection evidence metadata exceeds the bounded read.",
            )
            .into());
        }
        by_id.insert(
            id.clone(),
            QualityInspectionEvidence {
                id,
                original_name,
                media_type,
                bytes,
                checksum_sha256,
                source_state,
            },
        );
    }
    let mut ordered = Vec::with_capacity(ids.len());
    for id in ids {
        // capture() rejects duplicate ids, so each requested id is present once.
        ordered.push(by_id.remove(id).ok_or_else(invalid)?);
    }
    Ok(Some(ordered))
}

/// Retained Files after-image for one request. The caller must supply only
/// ids selected by the current Quality inspector for this hold.
pub struct CurrentQualityInspectionEvidence {
    unit: Unit,
    company: CompanyId,
    branch: RecordId,
    owner: String,
    creator: String,
    ids: Vec<String>,
    evidence: Vec<QualityInspectionEvidence>,
}

impl CurrentQualityInspectionEvidence {
    pub fn evidence(&self) -> &[QualityInspectionEvidence] {
        &self.evidence
    }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(
                c,
                &self.company,
                &self.branch,
                &self.owner,
                &self.creator,
                &self.ids,
            )
            .await?
            .as_ref()
                != Some(&self.evidence)
        {
            return Err(changed());
        }
        Ok(())
    }
}

/// Validate direct Quality inspection evidence in the caller's retained unit.
/// `None` means at least one id is missing, revoked, not ready, already owned by
/// another hold, or outside this company/branch source.
pub async fn inspect_quality_inspection_evidence(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: &RecordId,
    owner: &str,
    creator: &str,
    ids: &[String],
) -> Result<Option<CurrentQualityInspectionEvidence>, Error> {
    let captured = unit(c).await?;
    if creator.is_empty() || creator.len() > 1024 || owner.is_empty() || owner.len() > 1024 {
        return Err(invalid());
    }
    let Some(evidence) = capture(c, company, branch, owner, creator, ids).await? else {
        return Ok(None);
    };
    if unit(c).await? != captured {
        return Err(changed());
    }
    Ok(Some(CurrentQualityInspectionEvidence {
        unit: captured,
        company: company.clone(),
        branch: branch.clone(),
        owner: owner.to_owned(),
        creator: creator.to_owned(),
        ids: ids.to_vec(),
        evidence,
    }))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualityInspectionEvidenceSetMembership {
    pub id: String,
    pub link_ids: Vec<String>,
    pub attachment_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EvidenceSet {
    membership: QualityInspectionEvidenceSetMembership,
    metadata: String,
    required_count: i64,
    max_files: i64,
    links: Vec<String>,
}

fn set_limit() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_INSPECTION_EVIDENCE_LIMIT",
        "The inspection evidence metadata exceeds the bounded read.",
    )
    .into()
}

async fn capture_sets(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: &RecordId,
    owner: &str,
    expected_version: i64,
    set_ids: &[String],
) -> Result<Option<Vec<EvidenceSet>>, Error> {
    if set_ids.is_empty()
        || set_ids.len() > MAX_IDS
        || expected_version < 1
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
            || purpose_key != "quality_inspection_evidence"
            || component_kind != "action"
            || component_ref.as_deref() != Some("inspection")
            || state != "draft"
            || expected_target_version != expected_version
            || branch_id
                .as_deref()
                .is_some_and(|value| value != branch.as_str())
            || required_count < 0
            || required_count > MAX_IDS as i64
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
        .map_err(|_| set_limit())?;
        sets.insert(
            id,
            EvidenceSet {
                membership: QualityInspectionEvidenceSetMembership {
                    id: String::new(),
                    link_ids: Vec::new(),
                    attachment_ids: Vec::new(),
                },
                metadata,
                required_count,
                max_files,
                links: Vec::new(),
            },
        );
    }
    let rows = sqlx::query(
        "SELECT l.set_id,l.id,l.attachment_id,a.original_name,a.media_type,a.bytes,a.checksum_sha256,a.source_state,a.owner_type,a.owner_id,a.purpose,a.revoked_at \
         FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id AND s.company_id=$1 \
         LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id \
         WHERE l.set_id=ANY($2::text[]) AND l.state='draft' ORDER BY l.created_at,l.id LIMIT 11",
    )
    .bind(company.as_str())
    .bind(set_ids)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > MAX_IDS {
        return Err(set_limit());
    }
    let mut total = 0_usize;
    for row in rows {
        let set_id: String = row.try_get("set_id")?;
        let link_id: String = row.try_get("id")?;
        let attachment_id: String = match row.try_get("attachment_id")? {
            Some(value) => value,
            None => return Ok(None),
        };
        let original_name: String = match row.try_get("original_name")? {
            Some(value) => value,
            None => return Ok(None),
        };
        let media_type: String = match row.try_get("media_type")? {
            Some(value) => value,
            None => return Ok(None),
        };
        let checksum_sha256: String = match row.try_get("checksum_sha256")? {
            Some(value) => value,
            None => return Ok(None),
        };
        let bytes: i64 = match row.try_get("bytes")? {
            Some(value) => value,
            None => return Ok(None),
        };
        let source_state: String = match row.try_get("source_state")? {
            Some(value) => value,
            None => return Ok(None),
        };
        let owner_type: Option<String> = row.try_get("owner_type")?;
        let owner_id: Option<String> = row.try_get("owner_id")?;
        let purpose: Option<String> = row.try_get("purpose")?;
        let revoked_at: Option<i64> = row.try_get("revoked_at")?;
        if owner_type.as_deref() != Some("quality_hold")
            || owner_id.as_deref() != Some(owner)
            || purpose.as_deref() != Some("quality_inspection_evidence")
            || revoked_at.is_some()
            || source_state != "available"
            || !(0..=MAX_SAFE).contains(&bytes)
        {
            return Ok(None);
        }
        total = total
            .checked_add(link_id.len())
            .and_then(|n| n.checked_add(attachment_id.len()))
            .and_then(|n| n.checked_add(original_name.len()))
            .and_then(|n| n.checked_add(media_type.len()))
            .and_then(|n| n.checked_add(checksum_sha256.len()))
            .ok_or_else(set_limit)?;
        if total > MAX_METADATA_BYTES {
            return Err(set_limit());
        }
        let image = serde_json::to_string(&(
            link_id.as_str(),
            attachment_id.as_str(),
            original_name.as_str(),
            media_type.as_str(),
            bytes,
            checksum_sha256.as_str(),
            source_state.as_str(),
        ))
        .map_err(|_| set_limit())?;
        let set = match sets.get_mut(&set_id) {
            Some(value) => value,
            None => return Ok(None),
        };
        set.membership.link_ids.push(link_id);
        set.membership.attachment_ids.push(attachment_id);
        set.links.push(image);
    }
    let mut ordered = Vec::with_capacity(set_ids.len());
    for id in set_ids {
        let mut set = match sets.remove(id) {
            Some(value) => value,
            None => return Ok(None),
        };
        if set.links.is_empty()
            || set.links.len() < set.required_count as usize
            || set.links.len() as i64 > set.max_files
        {
            return Ok(None);
        }
        set.membership.id = id.clone();
        ordered.push(set);
    }
    Ok(Some(ordered))
}

/// Retained Files after-image for the exact draft membership of one or more
/// Quality inspection sets. It grants no Quality record authority.
pub struct CurrentQualityInspectionEvidenceSets {
    unit: Unit,
    company: CompanyId,
    branch: RecordId,
    owner: String,
    expected_version: i64,
    sets: Vec<EvidenceSet>,
    memberships: Vec<QualityInspectionEvidenceSetMembership>,
    attachment_ids: Vec<String>,
}

impl CurrentQualityInspectionEvidenceSets {
    /// Stable set order followed by link creation order, with the first
    /// occurrence of each attachment retained.
    pub fn attachment_ids(&self) -> &[String] {
        &self.attachment_ids
    }

    /// Per-set link and attachment membership in the same stable order.
    pub fn memberships(&self) -> &[QualityInspectionEvidenceSetMembership] {
        &self.memberships
    }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture_sets(
                c,
                &self.company,
                &self.branch,
                &self.owner,
                self.expected_version,
                &self
                    .sets
                    .iter()
                    .map(|set| set.membership.id.clone())
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
}

/// Validate that each requested set is a current draft Quality inspection set
/// for this hold and branch, and resolve its exact available membership.
/// `None` means a set is missing, belongs to another owner/version, is closed,
/// is empty, or contains a link whose attachment is not ready/available.
pub async fn inspect_quality_inspection_evidence_sets(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: &RecordId,
    owner: &str,
    expected_version: i64,
    set_ids: &[String],
) -> Result<Option<CurrentQualityInspectionEvidenceSets>, Error> {
    let captured = unit(c).await?;
    if owner.is_empty() || owner.len() > 1024 {
        return Err(invalid());
    }
    let Some(sets) = capture_sets(c, company, branch, owner, expected_version, set_ids).await?
    else {
        return Ok(None);
    };
    if unit(c).await? != captured {
        return Err(changed());
    }
    let memberships = sets.iter().map(|set| set.membership.clone()).collect();
    let mut attachment_ids = Vec::new();
    for set in &sets {
        for id in &set.membership.attachment_ids {
            if !attachment_ids.contains(id) {
                attachment_ids.push(id.clone());
            }
        }
    }
    Ok(Some(CurrentQualityInspectionEvidenceSets {
        unit: captured,
        company: company.clone(),
        branch: branch.clone(),
        owner: owner.to_owned(),
        expected_version,
        sets,
        memberships,
        attachment_ids,
    }))
}
