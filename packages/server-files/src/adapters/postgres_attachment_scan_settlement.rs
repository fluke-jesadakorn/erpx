//! Canonical effects use an intended complete image computed before writes.
use super::{
    postgres_attachment_scan::{CurrentAttachmentScanSource, read_current_attachment_scan_source},
    postgres_attachment_scan_receipt::{
        CurrentAttachmentScanSettlement, IntendedAttachmentScanSettlement, PlannedEvent,
        PlannedLedger, PlannedRetry, SettlementWrite,
    },
};
use crate::{
    application::attachment_scan::{
        AttachmentScanClaim, AttachmentScanEvidence, AttachmentScanFailure,
        AttachmentScanSettlement, PreparedAttachmentScan,
    },
    domain::{
        attachment_scan::{DRAFT_EXTENSION_MILLIS, changed, next_scan_attempt, preview_object_key},
        preview::{PREVIEW_EDGE, PREVIEW_OUTPUT_LIMIT, PreviewSourceMedia},
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, IdentifierSource, RecordId};
use serde::Serialize;
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
async fn source(
    c: &mut PgConnection,
    p: &PreparedAttachmentScan,
    clock: &impl Clock,
) -> Result<CurrentAttachmentScanSource, Error> {
    let current = read_current_attachment_scan_source(c, p.claim(), clock).await?;
    if current.identity() != p.identity() {
        return Err(changed().into());
    }
    Ok(current)
}
fn text(v: &str) -> Result<(), Error> {
    if v.is_empty() || v.len() > 4096 || v.contains(['\0', '\r', '\n']) {
        return Err(changed().into());
    }
    Ok(())
}
fn id(prefix: &str, ids: &mut impl IdentifierSource) -> Result<String, Error> {
    Ok(RecordId::new(format!("{prefix}{}", ids.next().as_str()))?
        .as_str()
        .into())
}
fn json(value: &impl Serialize) -> Result<String, Error> {
    let v = serde_json::to_string(value)
        .map_err(|_| changed())?
        .replace('<', "\\u003c");
    if v.len() > 65536 {
        return Err(changed().into());
    }
    Ok(v)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanDetails<'a> {
    engine: &'a str,
    definition_updated_at: i64,
    preview: bool,
}
#[derive(Serialize)]
struct InfectedDetails<'a> {
    signature: &'a str,
    engine: &'a str,
}
#[derive(Serialize)]
struct ErrorDetails<'a> {
    attempt: i64,
    code: &'a str,
}

pub async fn retain_current_attachment_scan(
    c: &mut PgConnection,
    p: &PreparedAttachmentScan,
    evidence: &AttachmentScanEvidence,
    clock: &impl Clock,
    ids: &mut impl IdentifierSource,
) -> Result<CurrentAttachmentScanSettlement, Error> {
    let current = source(c, p, clock).await?;
    let completed = clock.now().get();
    let write = match evidence {
        AttachmentScanEvidence::Infected {
            engine,
            signature,
            definition_updated_at,
        } => {
            text(engine)?;
            text(signature)?;
            let details = json(&InfectedDetails { signature, engine })?;
            SettlementWrite {
                outcome: AttachmentScanSettlement::MalwareRejected,
                state: "infected",
                engine: Some(engine.clone()),
                signature: Some(signature.clone()),
                definition: Some(*definition_updated_at),
                code: None,
                completed,
                preview_key: None,
                preview_media: None,
                preview_checksum: None,
                preview_bytes: None,
                event: PlannedEvent {
                    id: id("attachment-event-", ids)?,
                    action: "scan.malware_rejected",
                    details,
                },
                retry: None,
                ledger: None,
            }
        }
        AttachmentScanEvidence::Clean {
            engine,
            definition_updated_at,
            preview,
        } => {
            text(engine)?;
            let media = PreviewSourceMedia::try_from(p.identity.media_type.as_str())
                .map_err(|_| changed())?;
            let (key, pm, sha, bytes) = match preview {
                None if !media.needs_preview() => (None, None, None, None),
                Some(a) => {
                    let expected = if media == PreviewSourceMedia::Pdf {
                        "image/jpeg"
                    } else {
                        "image/webp"
                    };
                    if !media.needs_preview()
                        || a.media_type != expected
                        || a.source_sha256 != p.identity.checksum_sha256
                        || a.source_data_sha256 != p.identity.source_sha256
                        || a.key
                            != preview_object_key(
                                current.original_key(),
                                &a.checksum_sha256,
                                &a.media_type,
                            )?
                        || a.bytes < 1
                        || a.bytes > PREVIEW_OUTPUT_LIMIT as i64
                        || a.width == 0
                        || a.height == 0
                        || a.width > PREVIEW_EDGE
                        || a.height > PREVIEW_EDGE
                    {
                        return Err(changed().into());
                    }
                    (
                        Some(a.key.clone()),
                        Some(a.media_type.clone()),
                        Some(a.checksum_sha256.clone()),
                        Some(a.bytes),
                    )
                }
                None => return Err(changed().into()),
            };
            let details = json(&CleanDetails {
                engine,
                definition_updated_at: *definition_updated_at,
                preview: key.is_some(),
            })?;
            let ledger = if let (Some(key), Some(bytes)) = (&key, bytes) {
                Some(PlannedLedger {
                    id: id("attachment-storage-", ids)?,
                    key: key.clone(),
                    bytes,
                })
            } else {
                None
            };
            SettlementWrite {
                outcome: AttachmentScanSettlement::Available,
                state: "clean",
                engine: Some(engine.clone()),
                signature: None,
                definition: Some(*definition_updated_at),
                code: None,
                completed,
                preview_key: key,
                preview_media: pm,
                preview_checksum: sha,
                preview_bytes: bytes,
                event: PlannedEvent {
                    id: id("attachment-event-", ids)?,
                    action: "scan.available",
                    details,
                },
                retry: None,
                ledger,
            }
        }
    };
    let intended = IntendedAttachmentScanSettlement::capture(c, current, p, write, clock).await?;
    intended.require_before(c, clock).await?;
    apply(c, p.claim(), intended.write()).await?;
    intended.confirm(c, clock).await
}
pub async fn fail_current_attachment_scan(
    c: &mut PgConnection,
    p: &PreparedAttachmentScan,
    failure: &AttachmentScanFailure,
    clock: &impl Clock,
    ids: &mut impl IdentifierSource,
) -> Result<CurrentAttachmentScanSettlement, Error> {
    let current = source(c, p, clock).await?;
    let code = failure.code();
    if code.is_empty()
        || code.len() > 160
        || !code
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(changed().into());
    }
    let completed = clock.now().get();
    let next = next_scan_attempt(p.claim.attempt, completed)?;
    let retry = match next {
        Some((attempt, due)) => Some(PlannedRetry {
            id: id("attachment-scan-", ids)?,
            attempt,
            due,
        }),
        None => None,
    };
    let details = json(&ErrorDetails {
        attempt: p.claim.attempt,
        code,
    })?;
    let write = SettlementWrite {
        outcome: AttachmentScanSettlement::Error {
            next_attempt: next.map(|v| v.0),
        },
        state: "error",
        engine: None,
        signature: None,
        definition: None,
        code: Some(code.into()),
        completed,
        preview_key: None,
        preview_media: None,
        preview_checksum: None,
        preview_bytes: None,
        event: PlannedEvent {
            id: id("attachment-event-", ids)?,
            action: "scan.error",
            details,
        },
        retry,
        ledger: None,
    };
    let intended = IntendedAttachmentScanSettlement::capture(c, current, p, write, clock).await?;
    intended.require_before(c, clock).await?;
    apply(c, p.claim(), intended.write()).await?;
    intended.confirm(c, clock).await
}
async fn apply(
    c: &mut PgConnection,
    claim: &AttachmentScanClaim,
    w: &SettlementWrite,
) -> Result<(), Error> {
    let n = if w.state == "error" {
        sqlx::query("UPDATE attachment_scan_attempts SET state='error',error_code=$1,completed_at=$2,claim_until=NULL WHERE id=$3 AND attachment_id=$4 AND state='running'AND lease_token=$5 AND claimed_at=$6 AND claim_until=$7 AND claim_until>$2").bind(&w.code).bind(w.completed).bind(&claim.job_id).bind(&claim.source_id).bind(&claim.token).bind(claim.claimed_at).bind(claim.claim_until).execute(&mut *c).await?.rows_affected()
    } else {
        sqlx::query("UPDATE attachment_scan_attempts SET state=$1,scanner_engine=$2,scanner_signature=$3,scanner_definition_at=$4,result_code=CASE WHEN $1='clean'THEN'OK'ELSE'FOUND'END,completed_at=$5,claim_until=NULL WHERE id=$6 AND attachment_id=$7 AND state='running'AND lease_token=$8 AND claimed_at=$9 AND claim_until=$10 AND claim_until>$5").bind(w.state).bind(&w.engine).bind(&w.signature).bind(w.definition).bind(w.completed).bind(&claim.job_id).bind(&claim.source_id).bind(&claim.token).bind(claim.claimed_at).bind(claim.claim_until).execute(&mut *c).await?.rows_affected()
    };
    if n != 1 {
        return Err(changed().into());
    }
    let n=match w.outcome{
 AttachmentScanSettlement::Available=>sqlx::query("UPDATE attachments SET source_state='available',detected_media_type=COALESCE(detected_media_type,media_type),scanned_at=$1,scanner_definition_at=$2,preview_object_key=$3,preview_media_type=$4,preview_checksum_sha256=$5,preview_bytes=$6 WHERE id=$7 AND company_id=$8 AND source_state='scanning'").bind(w.completed).bind(w.definition).bind(&w.preview_key).bind(&w.preview_media).bind(&w.preview_checksum).bind(w.preview_bytes).bind(&claim.source_id).bind(claim.scope.company_id.as_str()).execute(&mut *c).await?.rows_affected(),
 AttachmentScanSettlement::MalwareRejected=>sqlx::query("UPDATE attachments SET source_state='malware_rejected',detected_media_type=COALESCE(detected_media_type,media_type),scanned_at=$1,scanner_definition_at=$2 WHERE id=$3 AND company_id=$4 AND source_state='scanning'").bind(w.completed).bind(w.definition).bind(&claim.source_id).bind(claim.scope.company_id.as_str()).execute(&mut *c).await?.rows_affected(),
 AttachmentScanSettlement::Error{..}=>sqlx::query("UPDATE attachments SET source_state='scan_error'WHERE id=$1 AND company_id=$2 AND source_state='scanning'").bind(&claim.source_id).bind(claim.scope.company_id.as_str()).execute(&mut *c).await?.rows_affected(),
 };
    if n != 1 {
        return Err(changed().into());
    }
    if matches!(w.outcome, AttachmentScanSettlement::Available) {
        let n=sqlx::query("UPDATE upload_intents SET status='ready',updated_at=$1 WHERE id=(SELECT upload_id FROM attachments WHERE id=$2 AND company_id=$3)AND company_id=$3 AND status='uploaded'").bind(w.completed).bind(&claim.source_id).bind(claim.scope.company_id.as_str()).execute(&mut *c).await?.rows_affected();
        if n != 1 {
            return Err(changed().into());
        }
        sqlx::query("UPDATE attachment_links SET expires_at=$1 WHERE upload_intent_id=(SELECT upload_id FROM attachments WHERE id=$2 AND company_id=$3)AND state='draft'").bind(w.completed.checked_add(DRAFT_EXTENSION_MILLIS).ok_or_else(changed)?).bind(&claim.source_id).bind(claim.scope.company_id.as_str()).execute(&mut *c).await?;
    }
    if let Some(l) = &w.ledger {
        sqlx::query("INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at)VALUES($1,$2,$3,'preview',$4,$5,'stored',$6)").bind(&l.id).bind(claim.scope.company_id.as_str()).bind(&claim.source_id).bind(&l.key).bind(l.bytes).bind(w.completed).execute(&mut *c).await?;
    }
    sqlx::query("INSERT INTO attachment_events(id,company_id,attachment_id,actor_id,action,details_json,created_at)VALUES($1,$2,$3,'system:jobs',$4,$5,$6)").bind(&w.event.id).bind(claim.scope.company_id.as_str()).bind(&claim.source_id).bind(w.event.action).bind(&w.event.details).bind(w.completed).execute(&mut *c).await?;
    if let Some(r) = &w.retry {
        sqlx::query("INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at)VALUES($1,$2,$3,'pending',$4)").bind(&r.id).bind(&claim.source_id).bind(r.attempt).bind(r.due).execute(&mut *c).await?;
    }
    Ok(())
}
