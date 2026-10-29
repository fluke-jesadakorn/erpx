//! Borrowed Files-only source/lease reader. No Person/foreign owner SQL and no
//! current permission inferred from created_by or caller source labels.
use crate::{
    application::attachment_scan::{
        AttachmentScanClaim, AttachmentScanInputIdentity, PreparedAttachmentScan,
        QuarantinedOriginalInput,
    },
    domain::attachment_scan::{
        AttachmentScanScope, OriginalStorageFacts, SCAN_LEASE_MILLIS, ScanOrigin,
        ScanRelationshipFacts, ScanSourceFacts, ScanUploadFacts, changed,
        require_system_scan_source,
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, IdentifierSource, RecordId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Eq, PartialEq)]
struct TransactionIdentity {
    database: String,
    database_oid: i64,
    backend: i32,
    xid: String,
    postmaster: String,
}
async fn transaction(c: &mut PgConnection) -> Result<TransactionIdentity, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ATTACHMENT_SCAN_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("ATTACHMENT_SCAN_SERIALIZABLE_REQUIRED").into());
    }
    let (database, database_oid, backend, xid, postmaster): (String, i64, i32, String, String) = sqlx::query_as("SELECT current_database(),(SELECT oid::bigint FROM pg_database WHERE datname=current_database()),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?;
    Ok(TransactionIdentity {
        database,
        database_oid,
        backend,
        xid,
        postmaster,
    })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ScanJob {
    id: String,
    attachment_id: String,
    attempt: i64,
    state: String,
    claimed_at: Option<i64>,
    claim_until: Option<i64>,
    lease_token: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ScanEvent {
    id: String,
    company_id: String,
    attachment_id: String,
    link_id: String,
    set_id: String,
    actor_id: String,
    action: String,
    details: serde_json::Value,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct Projection {
    job: ScanJob,
    other_open_jobs: i64,
    source: ScanSourceFacts,
    upload: ScanUploadFacts,
    relationship: ScanRelationshipFacts,
    original: OriginalStorageFacts,
    event: ScanEvent,
}

// Only named owner fields cross the wire. The server-side byte cap applies
// BEFORE transfer; no fetch_all, arbitrary parent rows, scanner stderr or file
// bytes enter SQL. Tuple locks preserve 40001 if a source writer committed
// after this transaction's original snapshot. No global SHARE lock serializes
// unrelated scan claims or introduces a shared-to-write upgrade deadlock.
const SOURCE: &str = r#"
WITH projected AS MATERIALIZED (
 SELECT jsonb_build_object(
  'other_open_jobs',(SELECT count(*)FROM attachment_scan_attempts competing WHERE competing.attachment_id=a.id AND competing.id<>j.id AND competing.state IN('pending','running')),
  'job',jsonb_build_object('id',j.id,'attachment_id',j.attachment_id,'attempt',j.attempt,'state',j.state,'claimed_at',j.claimed_at,'claim_until',j.claim_until,'lease_token',j.lease_token),
  'source',jsonb_build_object('id',a.id,'upload_id',a.upload_id,'company_id',a.company_id,'owner_type',a.owner_type,'owner_id',a.owner_id,'purpose',a.purpose,'object_key',a.object_key,'revision',a.revision,'original_name',a.original_name,'media_type',a.media_type,'bytes',a.bytes,'checksum',a.checksum_sha256,'created_by',a.created_by,'created_at',a.created_at,'detected_media_type',a.detected_media_type,'state',a.source_state,'revoked_at',a.revoked_at,'destroyed_at',a.destroyed_at,'has_preview',a.preview_object_key IS NOT NULL OR a.preview_media_type IS NOT NULL OR a.preview_checksum_sha256 IS NOT NULL OR a.preview_bytes IS NOT NULL),
  'upload',jsonb_build_object('id',u.id,'company_id',u.company_id,'owner_type',u.owner_type,'owner_id',u.owner_id,'purpose',u.purpose,'original_name',u.original_name,'media_type',u.media_type,'declared_bytes',u.declared_bytes,'actual_bytes',u.actual_bytes,'checksum',u.checksum_sha256,'quarantine_key',u.quarantine_key,'final_key',u.final_key,'staging_key',u.staging_key,'revision',u.revision,'status',u.status,'created_by',u.created_by,'set_id',u.attachment_set_id,'link_id',u.attachment_link_id,'detected_media_type',u.detected_media_type),
  'relationship',jsonb_build_object('set_id',s.id,'company_id',s.company_id,'target_type',s.target_type,'target_id',s.target_id,'purpose',s.purpose_key,'link_id',l.id,'link_source_id',l.attachment_id,'link_upload_id',l.upload_intent_id),
  'original',jsonb_build_object('company_id',o.company_id,'source_id',o.attachment_id,'object_key',o.object_key,'bytes',o.bytes,'variant',o.variant,'state',o.state),
  'event',jsonb_build_object('id',e.id,'company_id',e.company_id,'attachment_id',e.attachment_id,'link_id',e.link_id,'set_id',e.set_id,'actor_id',e.actor_id,'action',e.action,'details',e.details)
 )::text AS image
 FROM attachment_scan_attempts j
 JOIN attachments a ON a.id=j.attachment_id
 JOIN upload_intents u ON u.id=a.upload_id
 JOIN attachment_links l ON l.id=u.attachment_link_id
 JOIN attachment_sets s ON s.id=u.attachment_set_id AND l.set_id=s.id
 JOIN attachment_storage_ledger o ON o.attachment_id=a.id AND o.object_key=a.object_key AND o.variant='original'
 JOIN LATERAL (
  SELECT ev.id,ev.company_id,ev.attachment_id,ev.link_id,ev.set_id,ev.actor_id,ev.action,CASE WHEN octet_length(ev.details_json)<=16384 THEN ev.details_json::jsonb ELSE '{}'::jsonb END AS details FROM attachment_events ev WHERE ev.attachment_id=a.id AND ev.link_id=l.id AND ev.set_id=s.id
   AND ((ev.action='file.quarantined' AND ev.actor_id=a.created_by
        AND (CASE WHEN octet_length(ev.details_json)<=16384 THEN ev.details_json::jsonb ELSE '{}'::jsonb END)->>'checksum'=a.checksum_sha256
        AND (CASE WHEN octet_length(ev.details_json)<=16384 THEN ev.details_json::jsonb ELSE '{}'::jsonb END)->'bytes'=to_jsonb(a.bytes)
        AND (CASE WHEN octet_length(ev.details_json)<=16384 THEN ev.details_json::jsonb ELSE '{}'::jsonb END)->>'detectedMediaType'=a.media_type)
    OR (ev.action='legacy.backfilled' AND ev.actor_id='system:migration'
        AND (CASE WHEN octet_length(ev.details_json)<=16384 THEN ev.details_json::jsonb ELSE '{}'::jsonb END)->>'ownerType'=a.owner_type
        AND (CASE WHEN octet_length(ev.details_json)<=16384 THEN ev.details_json::jsonb ELSE '{}'::jsonb END)->>'ownerId'=a.owner_id
        AND (CASE WHEN octet_length(ev.details_json)<=16384 THEN ev.details_json::jsonb ELSE '{}'::jsonb END)->>'purposeKey'=s.purpose_key)
    OR (ev.action='legacy.specialized_backfilled' AND ev.actor_id='system:migration'
        AND (CASE WHEN octet_length(ev.details_json)<=16384 THEN ev.details_json::jsonb ELSE '{}'::jsonb END)->>'kind' IN ('messaging','legal_document','legal_external','pos_product_image')))
  ORDER BY ev.created_at,ev.id LIMIT 1 FOR SHARE OF ev
 ) e ON true
 WHERE j.id=$1 AND a.company_id=$2
 FOR UPDATE OF j,a FOR SHARE OF u,l,s,o
)
SELECT octet_length(image)<=65536,CASE WHEN octet_length(image)<=65536 THEN image ELSE NULL END FROM projected
"#;

fn event_origin(p: &Projection) -> Result<ScanOrigin, Error> {
    let s = &p.source;
    let e = &p.event;
    if e.company_id != s.company_id
        || e.attachment_id != s.id
        || e.link_id != p.relationship.link_id
        || e.set_id != p.relationship.set_id
    {
        return Err(changed().into());
    }
    match e.action.as_str() {
        "file.quarantined" if e.actor_id == s.created_by => Ok(ScanOrigin::Managed),
        "legacy.backfilled" if e.actor_id == "system:migration" => {
            let digest = hex::encode(Sha256::digest(s.id.as_bytes()));
            if e.id != format!("attachment-event-legacy-{}", &digest[..32])
                || p.relationship.set_id != format!("attachment-set-legacy-{}", &digest[..32])
                || p.relationship.link_id != format!("attachment-link-legacy-{}", &digest[..32])
            {
                return Err(changed().into());
            }
            Ok(ScanOrigin::Legacy)
        }
        "legacy.specialized_backfilled" if e.actor_id == "system:migration" => {
            let kind = e
                .details
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(changed)?;
            let id = e
                .details
                .get("legacyId")
                .and_then(serde_json::Value::as_str)
                .filter(|v| !v.is_empty() && v.len() <= 4096)
                .ok_or_else(changed)?;
            let digest = hex::encode(Sha256::digest(format!("{kind}\0{id}").as_bytes()));
            if s.id != format!("attachment-specialized-{}", &digest[..32])
                || p.upload.id != format!("attachment-upload-specialized-{}", &digest[..32])
                || p.relationship.set_id != format!("attachment-set-specialized-{}", &digest[..32])
                || p.relationship.link_id
                    != format!("attachment-link-specialized-{}", &digest[..32])
                || e.id != format!("attachment-event-specialized-{}", &digest[..32])
            {
                return Err(changed().into());
            }
            let expected = match kind {
                "messaging" => ("messaging_room", "message_attachment"),
                "legal_document" => ("legal_item", "legal_document"),
                "legal_external" => ("legal_item", "legal_portal_submission"),
                "pos_product_image" => ("product", "product_image"),
                _ => return Err(changed().into()),
            };
            if (s.owner_type.as_str(), s.purpose.as_str()) != expected {
                return Err(changed().into());
            }
            Ok(ScanOrigin::Specialized)
        }
        _ => Err(changed().into()),
    }
}
async fn raw_projection(
    c: &mut PgConnection,
    claim: &AttachmentScanClaim,
) -> Result<Projection, Error> {
    let (admitted, raw): (bool, Option<String>) = sqlx::query_as(SOURCE)
        .bind(&claim.job_id)
        .bind(claim.scope.company_id.as_str())
        .fetch_optional(&mut *c)
        .await?
        .ok_or_else(changed)?;
    if !admitted {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "ATTACHMENT_SCAN_SOURCE_LIMIT",
            "The current file scan source exceeds the bounded projection.",
        )
        .into());
    }
    let p: Projection =
        serde_json::from_str(raw.as_deref().ok_or_else(changed)?).map_err(|_| changed())?;
    Ok(p)
}
async fn projection(
    c: &mut PgConnection,
    claim: &AttachmentScanClaim,
    now: i64,
) -> Result<Projection, Error> {
    let p = raw_projection(c, claim).await?;
    if p.other_open_jobs != 0
        || p.job.id != claim.job_id
        || p.job.attachment_id != claim.source_id
        || p.job.attempt != claim.attempt
        || p.job.state != "running"
        || p.job.claimed_at != Some(claim.claimed_at)
        || p.job.claim_until != Some(claim.claim_until)
        || p.job.lease_token.as_deref() != Some(claim.token.as_str())
        || now >= claim.claim_until
        || p.source.state != "scanning"
    {
        return Err(changed().into());
    }
    require_system_scan_source(
        &claim.scope,
        &p.source,
        &p.upload,
        &p.relationship,
        &p.original,
        event_origin(&p)?,
    )?;
    Ok(p)
}

/// Fresh transaction-bound proof never crosses native computation.
pub struct CurrentAttachmentScanSource {
    transaction: TransactionIdentity,
    projection: Projection,
    identity: AttachmentScanInputIdentity,
}
impl CurrentAttachmentScanSource {
    pub fn identity(&self) -> &AttachmentScanInputIdentity {
        &self.identity
    }
    pub(super) fn original_key(&self) -> &str {
        &self.projection.source.object_key
    }
    pub(super) async fn require_intended_settlement(
        &self,
        c: &mut PgConnection,
        claim: &AttachmentScanClaim,
        outcome: crate::application::attachment_scan::AttachmentScanSettlement,
        has_preview: bool,
        clock: &impl Clock,
    ) -> Result<(), Error> {
        if transaction(c).await? != self.transaction || clock.now().get() >= claim.claim_until {
            return Err(changed().into());
        }
        let mut expected = self.projection.clone();
        expected.job.claim_until = None;
        expected.other_open_jobs = i64::from(matches!(
            outcome,
            crate::application::attachment_scan::AttachmentScanSettlement::Error {
                next_attempt: Some(_)
            }
        ));
        match outcome {
            crate::application::attachment_scan::AttachmentScanSettlement::Available => {
                expected.job.state = "clean".into();
                expected.source.state = "available".into();
                expected.source.has_preview = has_preview;
                expected.source.detected_media_type = Some(expected.source.media_type.clone());
                expected.upload.status = "ready".into();
            }
            crate::application::attachment_scan::AttachmentScanSettlement::MalwareRejected => {
                expected.job.state = "infected".into();
                expected.source.state = "malware_rejected".into();
                expected.source.detected_media_type = Some(expected.source.media_type.clone());
            }
            crate::application::attachment_scan::AttachmentScanSettlement::Error { .. } => {
                expected.job.state = "error".into();
                expected.source.state = "scan_error".into();
            }
        }
        if raw_projection(c, claim).await? != expected {
            return Err(changed().into());
        }
        Ok(())
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        claim: &AttachmentScanClaim,
        clock: &impl Clock,
    ) -> Result<(), Error> {
        if transaction(c).await? != self.transaction {
            return Err(changed().into());
        }
        let fresh = projection(c, claim, clock.now().get()).await?;
        if data_identity(&fresh)? != self.identity {
            return Err(changed().into());
        }
        Ok(())
    }
}
fn data_identity(p: &Projection) -> Result<AttachmentScanInputIdentity, Error> {
    // Mutable legal-hold/read revocation, link/set lifecycle, uploaded expiry
    // and access events do not become invented system scan admission gates.
    // Read permission is freshly checked by the separate preview read port.
    let mut source = p.source.clone();
    source.revoked_at = None;
    let data = serde_json::to_vec(&(&source, &p.upload, &p.relationship, &p.original, &p.event))
        .map_err(|_| changed())?;
    if data.len() > 65536 {
        return Err(changed().into());
    }
    Ok(AttachmentScanInputIdentity {
        source_sha256: hex::encode(Sha256::digest(data)),
        checksum_sha256: p.source.checksum.clone(),
        media_type: p.source.media_type.clone(),
        bytes: usize::try_from(p.source.bytes).map_err(|_| changed())?,
    })
}

pub async fn read_current_attachment_scan_source(
    c: &mut PgConnection,
    claim: &AttachmentScanClaim,
    clock: &impl Clock,
) -> Result<CurrentAttachmentScanSource, Error> {
    let tx = transaction(c).await?;
    if tx.database != claim.database
        || tx.database_oid != claim.database_oid
        || tx.postmaster != claim.postmaster
    {
        return Err(changed().into());
    }
    let projection = projection(c, claim, clock.now().get()).await?;
    let identity = data_identity(&projection)?;
    Ok(CurrentAttachmentScanSource {
        transaction: tx,
        projection,
        identity,
    })
}

pub async fn claim_attachment_scan(
    c: &mut PgConnection,
    scope: &AttachmentScanScope,
    filter: Option<&RecordId>,
    clock: &impl Clock,
    identifiers: &mut impl IdentifierSource,
) -> Result<Option<AttachmentScanClaim>, Error> {
    let tx = transaction(c).await?;
    let now = clock.now().get();
    let until = now.checked_add(SCAN_LEASE_MILLIS).ok_or_else(changed)?;
    let candidate: Option<(Option<String>,Option<String>,i64)> = sqlx::query_as(
        "SELECT CASE WHEN octet_length(j.id)<=200 THEN j.id ELSE NULL END,CASE WHEN octet_length(j.attachment_id)<=200 THEN j.attachment_id ELSE NULL END,j.attempt FROM attachment_scan_attempts j JOIN attachments a ON a.id=j.attachment_id WHERE a.company_id=$1 AND j.state IN ('pending','running') AND j.created_at<=$2 AND(j.claim_until IS NULL OR j.claim_until<$2) AND($3::text IS NULL OR a.id=$3) ORDER BY j.created_at,j.id FOR UPDATE OF j SKIP LOCKED LIMIT 1")
        .bind(scope.company_id.as_str()).bind(now).bind(filter.map(RecordId::as_str)).fetch_optional(&mut *c).await?;
    let Some((job_id, source_id, attempt)) = candidate else {
        return Ok(None);
    };
    let (Some(job_id), Some(source_id)) = (job_id, source_id) else {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "ATTACHMENT_SCAN_SOURCE_LIMIT",
            "The current file scan source exceeds the bounded projection.",
        )
        .into());
    };
    let token = format!("attachment-scan-lease-{}", identifiers.next().as_str());
    let updated = sqlx::query("UPDATE attachment_scan_attempts SET state='running',claimed_at=$1,claim_until=$2,lease_token=$3 WHERE id=$4")
        .bind(now).bind(until).bind(&token).bind(&job_id).execute(&mut *c).await?;
    if updated.rows_affected() != 1 {
        return Err(changed().into());
    }
    sqlx::query("UPDATE attachments SET source_state='scanning' WHERE id=$1 AND company_id=$2 AND source_state IN('quarantined','scan_error')")
        .bind(&source_id).bind(scope.company_id.as_str()).execute(&mut *c).await?;
    let claim = AttachmentScanClaim {
        scope: scope.clone(),
        database: tx.database,
        database_oid: tx.database_oid,
        postmaster: tx.postmaster,
        job_id,
        source_id,
        token,
        attempt,
        claimed_at: now,
        claim_until: until,
    };
    // If relationships/origin/current lease are invalid, the outer UoW rolls
    // back this claim too; it cannot commit a state change without a source.
    read_current_attachment_scan_source(c, &claim, clock).await?;
    Ok(Some(claim))
}

pub async fn prepare_current_attachment_scan(
    c: &mut PgConnection,
    claim: AttachmentScanClaim,
    clock: &impl Clock,
) -> Result<PreparedAttachmentScan, Error> {
    let current = read_current_attachment_scan_source(c, &claim, clock).await?;
    let original = QuarantinedOriginalInput {
        key: current.original_key().into(),
        bytes: current.identity.bytes,
        checksum_sha256: current.identity.checksum_sha256.clone(),
    };
    current.require_current(c, &claim, clock).await?;
    Ok(PreparedAttachmentScan {
        claim,
        identity: current.identity,
        original,
    })
}
