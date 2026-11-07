//! Intended complete owned row images, computed BEFORE effects. No result from
//! an UPDATE/trigger is allowed to redefine the expected source or audit image.
use super::postgres_attachment_scan::CurrentAttachmentScanSource;
use crate::{
    application::attachment_scan::{
        AttachmentScanClaim, AttachmentScanSettlement, PreparedAttachmentScan,
    },
    domain::attachment_scan::{DRAFT_EXTENSION_MILLIS, changed},
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use sqlx::PgConnection;
use std::collections::BTreeMap;
type Error = PgTransactionError<BusinessError>;
type LinkDigestRow = (
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    String,
);

pub(super) struct PlannedEvent {
    pub id: String,
    pub action: &'static str,
    pub details: String,
}
pub(super) struct PlannedRetry {
    pub id: String,
    pub attempt: i64,
    pub due: i64,
}
pub(super) struct PlannedLedger {
    pub id: String,
    pub key: String,
    pub bytes: i64,
}
pub(super) struct SettlementWrite {
    pub outcome: AttachmentScanSettlement,
    pub state: &'static str,
    pub engine: Option<String>,
    pub signature: Option<String>,
    pub definition: Option<i64>,
    pub code: Option<String>,
    pub completed: i64,
    pub preview_key: Option<String>,
    pub preview_media: Option<String>,
    pub preview_checksum: Option<String>,
    pub preview_bytes: Option<i64>,
    pub event: PlannedEvent,
    pub retry: Option<PlannedRetry>,
    pub ledger: Option<PlannedLedger>,
}
struct Header {
    before: [String; 3],
    after: [String; 3],
}
pub(super) struct IntendedAttachmentScanSettlement {
    before_source: CurrentAttachmentScanSource,
    claim: AttachmentScanClaim,
    write: SettlementWrite,
    header: Header,
    links_before: BTreeMap<String, String>,
    links_after: BTreeMap<String, String>,
    events_before: BTreeMap<String, String>,
    events_after: BTreeMap<String, String>,
    ledger_after: Option<String>,
    retry_after: Option<String>,
}
/// Opaque same-unit confirmation; no Clone, Deserialize or public constructor.
/// The outer coordinator repeats this AFTER all remaining effects and deferred
/// constraints, then commits. It must never reconstruct an expectation from SQL.
pub struct CurrentAttachmentScanSettlement {
    intended: IntendedAttachmentScanSettlement,
}
impl CurrentAttachmentScanSettlement {
    pub fn outcome(&self) -> AttachmentScanSettlement {
        self.intended.write.outcome
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        clock: &impl Clock,
    ) -> Result<(), Error> {
        self.intended.require_after(c, clock).await
    }
}
impl IntendedAttachmentScanSettlement {
    pub(super) fn write(&self) -> &SettlementWrite {
        &self.write
    }
    pub(super) async fn capture(
        c: &mut PgConnection,
        source: CurrentAttachmentScanSource,
        prepared: &PreparedAttachmentScan,
        write: SettlementWrite,
        clock: &impl Clock,
    ) -> Result<Self, Error> {
        source.require_current(c, prepared.claim(), clock).await?;
        let claim = prepared.claim().copy_binding_data();
        let header = header_plan(c, &claim, &write).await?;
        let (links_before, links_after) = link_plan(c, &claim, &write).await?;
        let events_before = scan_events(c, &claim).await?;
        let event_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_events WHERE id=$1)")
                .bind(&write.event.id)
                .fetch_one(&mut *c)
                .await?;
        if event_exists || events_before.contains_key(&write.event.id) {
            return Err(changed().into());
        }
        let mut events_after = events_before.clone();
        events_after.insert(
            write.event.id.clone(),
            event_digest(c, &claim, &write).await?,
        );
        let ledger_after = if let Some(ledger) = &write.ledger {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM attachment_storage_ledger WHERE object_key=$1 OR id=$2)",
            )
            .bind(&ledger.key)
            .bind(&ledger.id)
            .fetch_one(&mut *c)
            .await?;
            // A published physical object is adoptable after a crash; a contradictory
            // already-accounted row while the source is still scanning is not blessed.
            if exists {
                return Err(changed().into());
            }
            Some(ledger_digest(c, &claim, ledger, write.completed).await?)
        } else {
            None
        };
        let retry_after = if let Some(retry) = &write.retry {
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_scan_attempts WHERE(attachment_id=$1 AND attempt=$2)OR id=$3)").bind(&claim.source_id).bind(retry.attempt).bind(&retry.id).fetch_one(&mut *c).await?;
            if exists {
                return Err(changed().into());
            }
            Some(retry_digest(c, &claim, retry).await?)
        } else {
            None
        };
        let value = Self {
            before_source: source,
            claim,
            write,
            header,
            links_before,
            links_after,
            events_before,
            events_after,
            ledger_after,
            retry_after,
        };
        value.require_before(c, clock).await?;
        Ok(value)
    }
    pub(super) async fn require_before(
        &self,
        c: &mut PgConnection,
        clock: &impl Clock,
    ) -> Result<(), Error> {
        self.before_source
            .require_current(c, &self.claim, clock)
            .await?;
        if actual_header(c, &self.claim).await? != self.header.before
            || actual_links(c, &self.claim).await? != self.links_before
            || scan_events(c, &self.claim).await? != self.events_before
        {
            return Err(changed().into());
        }
        Ok(())
    }
    pub(super) async fn confirm(
        self,
        c: &mut PgConnection,
        clock: &impl Clock,
    ) -> Result<CurrentAttachmentScanSettlement, Error> {
        self.require_after(c, clock).await?;
        Ok(CurrentAttachmentScanSettlement { intended: self })
    }
    async fn require_after(&self, c: &mut PgConnection, clock: &impl Clock) -> Result<(), Error> {
        self.before_source
            .require_intended_settlement(
                c,
                &self.claim,
                self.write.outcome,
                self.write.preview_key.is_some(),
                clock,
            )
            .await?;
        if actual_header(c, &self.claim).await? != self.header.after
            || actual_links(c, &self.claim).await? != self.links_after
            || scan_events(c, &self.claim).await? != self.events_after
        {
            return Err(changed().into());
        }
        if let Some(ledger) = &self.write.ledger {
            let actual:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(l)::text,'UTF8')),'hex')FROM attachment_storage_ledger l WHERE id=$1 AND object_key=$2 AND company_id=$3 AND attachment_id=$4 FOR SHARE OF l")
    .bind(&ledger.id).bind(&ledger.key).bind(self.claim.scope.company_id.as_str()).bind(&self.claim.source_id).fetch_optional(&mut *c).await?;
            if actual != self.ledger_after {
                return Err(changed().into());
            }
        }
        if let Some(retry) = &self.write.retry {
            let actual:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(j)::text,'UTF8')),'hex')FROM attachment_scan_attempts j WHERE attachment_id=$1 AND attempt=$2 AND id=$3 FOR SHARE OF j")
    .bind(&self.claim.source_id).bind(retry.attempt).bind(&retry.id).fetch_optional(&mut *c).await?;
            if actual != self.retry_after {
                return Err(changed().into());
            }
        }
        Ok(())
    }
}
async fn actual_header(
    c: &mut PgConnection,
    claim: &AttachmentScanClaim,
) -> Result<[String; 3], Error> {
    let (job,source,upload):(String,String,String)=sqlx::query_as("SELECT encode(sha256(convert_to(to_jsonb(j)::text,'UTF8')),'hex'),encode(sha256(convert_to(to_jsonb(a)::text,'UTF8')),'hex'),encode(sha256(convert_to(to_jsonb(u)::text,'UTF8')),'hex')FROM attachment_scan_attempts j JOIN attachments a ON a.id=j.attachment_id JOIN upload_intents u ON u.id=a.upload_id WHERE j.id=$1 AND a.company_id=$2 FOR UPDATE OF j,a FOR SHARE OF u")
  .bind(&claim.job_id).bind(claim.scope.company_id.as_str()).fetch_optional(c).await?.ok_or_else(changed)?;
    Ok([job, source, upload])
}
async fn header_plan(
    c: &mut PgConnection,
    claim: &AttachmentScanClaim,
    w: &SettlementWrite,
) -> Result<Header, Error> {
    let (jb,ab,ub,ja,aa,ua):(String,String,String,String,String,String)=sqlx::query_as(r#"
 SELECT encode(sha256(convert_to(to_jsonb(j)::text,'UTF8')),'hex'),encode(sha256(convert_to(to_jsonb(a)::text,'UTF8')),'hex'),encode(sha256(convert_to(to_jsonb(u)::text,'UTF8')),'hex'),
 encode(sha256(convert_to((to_jsonb(j)||CASE WHEN $3='error' THEN jsonb_build_object('state',$3::text,'error_code',$7::text,'completed_at',$8::bigint,'claim_until',NULL) ELSE jsonb_build_object('state',$3::text,'scanner_engine',$4::text,'scanner_signature',$5::text,'scanner_definition_at',$6::bigint,'result_code',CASE WHEN $3='clean'THEN'OK'ELSE'FOUND'END,'completed_at',$8::bigint,'claim_until',NULL)END)::text,'UTF8')),'hex'),
 encode(sha256(convert_to((to_jsonb(a)||CASE WHEN $3='clean' THEN jsonb_build_object('source_state','available','detected_media_type',COALESCE(a.detected_media_type,a.media_type),'scanned_at',$8::bigint,'scanner_definition_at',$6::bigint,'preview_object_key',$9::text,'preview_media_type',$10::text,'preview_checksum_sha256',$11::text,'preview_bytes',$12::bigint)WHEN $3='infected'THEN jsonb_build_object('source_state','malware_rejected','detected_media_type',COALESCE(a.detected_media_type,a.media_type),'scanned_at',$8::bigint,'scanner_definition_at',$6::bigint)ELSE jsonb_build_object('source_state','scan_error')END)::text,'UTF8')),'hex'),
 encode(sha256(convert_to((to_jsonb(u)||CASE WHEN $3='clean'THEN jsonb_build_object('status','ready','updated_at',$8::bigint)ELSE'{}'::jsonb END)::text,'UTF8')),'hex')
 FROM attachment_scan_attempts j JOIN attachments a ON a.id=j.attachment_id JOIN upload_intents u ON u.id=a.upload_id WHERE j.id=$1 AND a.company_id=$2 FOR UPDATE OF j,a FOR SHARE OF u
 "#).bind(&claim.job_id).bind(claim.scope.company_id.as_str()).bind(w.state).bind(&w.engine).bind(&w.signature).bind(w.definition).bind(&w.code).bind(w.completed).bind(&w.preview_key).bind(&w.preview_media).bind(&w.preview_checksum).bind(w.preview_bytes).fetch_one(c).await?;
    Ok(Header {
        before: [jb, ab, ub],
        after: [ja, aa, ua],
    })
}
fn limited() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "ATTACHMENT_SCAN_RECEIPT_LIMIT",
        "The current file scan receipt exceeds its bounded source closure.",
    )
    .into()
}
async fn link_plan(
    c: &mut PgConnection,
    claim: &AttachmentScanClaim,
    w: &SettlementWrite,
) -> Result<(BTreeMap<String, String>, BTreeMap<String, String>), Error> {
    let rows:Vec<LinkDigestRow>=sqlx::query_as("SELECT CASE WHEN octet_length(l.id)<=200 THEN l.id ELSE NULL END,CASE WHEN octet_length(s.company_id)<=200 THEN s.company_id ELSE NULL END,CASE WHEN octet_length(l.attachment_id)<=200 THEN l.attachment_id ELSE NULL END,encode(sha256(convert_to(to_jsonb(l)::text,'UTF8')),'hex'),encode(sha256(convert_to((to_jsonb(l)||CASE WHEN $3::boolean AND l.state='draft'THEN jsonb_build_object('expires_at',$4::bigint)ELSE'{}'::jsonb END)::text,'UTF8')),'hex')FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.upload_intent_id=(SELECT upload_id FROM attachments WHERE id=$1 AND company_id=$2)ORDER BY l.id LIMIT 33 FOR SHARE OF l,s")
  .bind(&claim.source_id).bind(claim.scope.company_id.as_str()).bind(matches!(w.outcome,AttachmentScanSettlement::Available)).bind(w.completed.checked_add(DRAFT_EXTENSION_MILLIS).ok_or_else(changed)?).fetch_all(c).await?;
    if rows.len() > 32 {
        return Err(limited());
    }
    let mut before = BTreeMap::new();
    let mut after = BTreeMap::new();
    for (id, company, source, b, a) in rows {
        let id = id.ok_or_else(limited)?;
        if company.as_deref() != Some(claim.scope.company_id.as_str())
            || source.as_deref() != Some(claim.source_id.as_str())
        {
            return Err(changed().into());
        }
        before.insert(id.clone(), b);
        after.insert(id, a);
    }
    Ok((before, after))
}
async fn actual_links(
    c: &mut PgConnection,
    claim: &AttachmentScanClaim,
) -> Result<BTreeMap<String, String>, Error> {
    let rows:Vec<(Option<String>,String)>=sqlx::query_as("SELECT CASE WHEN octet_length(l.id)<=200 THEN l.id ELSE NULL END,encode(sha256(convert_to(to_jsonb(l)::text,'UTF8')),'hex')FROM attachment_links l WHERE l.upload_intent_id=(SELECT upload_id FROM attachments WHERE id=$1 AND company_id=$2)ORDER BY l.id LIMIT 33 FOR SHARE OF l")
  .bind(&claim.source_id).bind(claim.scope.company_id.as_str()).fetch_all(c).await?;
    if rows.len() > 32 {
        return Err(limited());
    }
    rows.into_iter()
        .map(|(id, hash)| Ok((id.ok_or_else(limited)?, hash)))
        .collect()
}
async fn scan_events(
    c: &mut PgConnection,
    claim: &AttachmentScanClaim,
) -> Result<BTreeMap<String, String>, Error> {
    let rows:Vec<(Option<String>,String)>=sqlx::query_as("SELECT CASE WHEN octet_length(e.id)<=200 THEN e.id ELSE NULL END,encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex')FROM attachment_events e WHERE attachment_id=$1 AND action LIKE 'scan.%'ORDER BY id LIMIT 33 FOR SHARE OF e")
  .bind(&claim.source_id).fetch_all(c).await?;
    if rows.len() > 32 {
        return Err(limited());
    }
    rows.into_iter()
        .map(|(id, hash)| Ok((id.ok_or_else(limited)?, hash)))
        .collect()
}
async fn event_digest(
    c: &mut PgConnection,
    claim: &AttachmentScanClaim,
    w: &SettlementWrite,
) -> Result<String, Error> {
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('id',$1::text,'company_id',$2::text,'attachment_id',$3::text,'link_id',NULL,'set_id',NULL,'actor_id','system:jobs','action',$4::text,'details_json',$5::text,'created_at',$6::bigint)::text,'UTF8')),'hex')")
  .bind(&w.event.id).bind(claim.scope.company_id.as_str()).bind(&claim.source_id).bind(w.event.action).bind(&w.event.details).bind(w.completed).fetch_one(c).await?)
}
async fn ledger_digest(
    c: &mut PgConnection,
    claim: &AttachmentScanClaim,
    l: &PlannedLedger,
    at: i64,
) -> Result<String, Error> {
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('id',$1::text,'company_id',$2::text,'attachment_id',$3::text,'variant','preview','object_key',$4::text,'bytes',$5::bigint,'state','stored','created_at',$6::bigint,'destroyed_at',NULL)::text,'UTF8')),'hex')")
  .bind(&l.id).bind(claim.scope.company_id.as_str()).bind(&claim.source_id).bind(&l.key).bind(l.bytes).bind(at).fetch_one(c).await?)
}
async fn retry_digest(
    c: &mut PgConnection,
    claim: &AttachmentScanClaim,
    r: &PlannedRetry,
) -> Result<String, Error> {
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('id',$1::text,'attachment_id',$2::text,'attempt',$3::bigint,'state','pending','scanner_engine','clamd','scanner_signature',NULL,'scanner_definition_at',NULL,'claimed_at',NULL,'claim_until',NULL,'lease_token',NULL,'result_code',NULL,'error_code',NULL,'created_at',$4::bigint,'completed_at',NULL)::text,'UTF8')),'hex')")
  .bind(&r.id).bind(&claim.source_id).bind(r.attempt).bind(r.due).fetch_one(c).await?)
}
