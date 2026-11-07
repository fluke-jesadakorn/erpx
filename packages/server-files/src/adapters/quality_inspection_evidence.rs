//! Files-owned source facts for direct Quality inspection evidence.
//!
//! This port proves only the requested Files rows, their company/branch scope,
//! ownership, ready state and absence of revocation. It grants no Quality
//! record authority and never reads Quality-owned relations; composition must
//! retain this witness and separately reject an already-used attachment.
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
        if bytes < 0 || bytes > MAX_SAFE {
            return Err(invalid());
        }
        total = total
            .checked_add(id.len())
            .and_then(|n| n.checked_add(original_name.len()))
            .and_then(|n| n.checked_add(media_type.len()))
            .and_then(|n| n.checked_add(checksum_sha256.len()))
            .ok_or_else(|| invalid())?;
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
        ordered.push(by_id.remove(id).ok_or_else(|| invalid())?);
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
