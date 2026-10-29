//! Disposable Files database facts only; no Person/owner grant or fake policy.
use erpx_server_files::public::attachment_scan::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgUnitOfWork, TransactionOptions, load_migrations,
    migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, HoldingId, IdentifierSource, RecordId, TimestampMillis,
};
use sha2::{Digest, Sha256};
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

#[derive(Clone)]
pub struct TestClock(pub Arc<AtomicI64>);
impl TestClock {
    pub fn new(now: i64) -> Self {
        Self(Arc::new(AtomicI64::new(now)))
    }
    pub fn set(&self, now: i64) {
        self.0.store(now, Ordering::SeqCst)
    }
}
impl Clock for TestClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(self.0.load(Ordering::SeqCst))
    }
}
pub struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).unwrap()
    }
}
pub fn scope() -> AttachmentScanScope {
    AttachmentScanScope {
        holding_id: HoldingId::new("holding-owned").unwrap(),
        company_id: CompanyId::new("company-owned").unwrap(),
    }
}
pub fn bytes() -> &'static [u8] {
    b"owned,source\n1,2\n"
}
pub async fn migrate(pool: &DatabasePool) {
    let migrations = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}
pub enum TestOrigin {
    Managed,
    Legacy,
    Specialized,
}
pub async fn seed_managed(pool: &DatabasePool, name: &str) -> RecordId {
    seed_source(pool, name, TestOrigin::Managed, "text/csv", bytes()).await
}
pub async fn seed_source(
    pool: &DatabasePool,
    name: &str,
    origin: TestOrigin,
    media: &str,
    body: &[u8],
) -> RecordId {
    seed_source_with_receipt(pool, name, origin, media, body, true).await
}
pub async fn seed_source_with_receipt(
    pool: &DatabasePool,
    name: &str,
    origin: TestOrigin,
    media: &str,
    body: &[u8],
    valid_receipt: bool,
) -> RecordId {
    let sha = hex::encode(Sha256::digest(body));
    let key = format!("quarantine/{name}/original");
    let owner_id = format!("not-yet-saved-{name}");
    let (
        source,
        upload,
        set,
        link,
        event,
        owner,
        purpose,
        set_purpose,
        action,
        actor,
        details,
        detected,
    ) = match origin {
        TestOrigin::Managed => {
            let upload = format!("upload-{name}");
            (
                format!("attachment-{upload}"),
                upload,
                format!("set-{name}"),
                format!("link-{name}"),
                format!("origin-{name}"),
                "expense_claim",
                "receipt",
                "receipt",
                "file.quarantined",
                "inactive-original-uploader",
                serde_json::json!({"checksum":if valid_receipt{sha.as_str()}else{"unproved"},"bytes":body.len(),"detectedMediaType":media}),
                Some(media),
            )
        }
        TestOrigin::Legacy => {
            let source = format!("old-source-{name}");
            let digest = hex::encode(Sha256::digest(source.as_bytes()));
            (
                source,
                format!("old-upload-{name}"),
                format!("attachment-set-legacy-{}", &digest[..32]),
                format!("attachment-link-legacy-{}", &digest[..32]),
                format!("attachment-event-legacy-{}", &digest[..32]),
                "invoice_payment",
                "bank_payment_slip",
                "payment_slip",
                "legacy.backfilled",
                "system:migration",
                serde_json::json!({"ownerType":"invoice_payment","ownerId":owner_id,"purposeKey":"payment_slip"}),
                None,
            )
        }
        TestOrigin::Specialized => {
            let digest = hex::encode(Sha256::digest(format!("legal_document\0{name}").as_bytes()));
            (
                format!("attachment-specialized-{}", &digest[..32]),
                format!("attachment-upload-specialized-{}", &digest[..32]),
                format!("attachment-set-specialized-{}", &digest[..32]),
                format!("attachment-link-specialized-{}", &digest[..32]),
                format!("attachment-event-specialized-{}", &digest[..32]),
                "legal_item",
                "legal_document",
                "legal_document",
                "legacy.specialized_backfilled",
                "system:migration",
                serde_json::json!({"kind":"legal_document","legacyId":name}),
                None,
            )
        }
    };
    // No parent/Person is inserted. Accepted uploaded draft sanitization is a
    // Files system operation, independently from current read authority.
    sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,quarantine_key,status,created_by,expires_at,created_at,updated_at,company_id,detected_media_type)VALUES($1,$2,$9,$3,'owned-source',$4,$5,$5,$6,$7,$7,$7,'uploaded','inactive-original-uploader',1,0,0,'company-owned',$8)")
      .bind(&upload).bind(owner).bind(purpose).bind(media).bind(body.len()as i64).bind(&sha).bind(&key).bind(detected).bind(&owner_id).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,source_state,detected_media_type)VALUES($1,$2,$3,$10,$4,$5,1,'owned-source',$6,$7,$8,'inactive-original-uploader',0,'company-owned','quarantined',$9)")
      .bind(&source).bind(&upload).bind(owner).bind(purpose).bind(&key).bind(media).bind(body.len()as i64).bind(&sha).bind(detected).bind(&owner_id).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at)VALUES($1,'company-owned',$2,$4,'record',$3,1,1,'confidential','{}','draft','inactive-original-uploader',0,0)")
      .bind(&set).bind(owner).bind(set_purpose).bind(&owner_id).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,upload_intent_id,state,created_by,created_at,expires_at)VALUES($1,$2,$3,$4,'draft','inactive-original-uploader',0,1)")
      .bind(&link).bind(&set).bind(&source).bind(&upload).execute(pool.raw()).await.unwrap();
    sqlx::query("UPDATE upload_intents SET attachment_set_id=$1,attachment_link_id=$2 WHERE id=$3")
        .bind(&set)
        .bind(&link)
        .bind(&upload)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at)VALUES($1,'company-owned',$2,'original',$3,$4,'stored',0)")
      .bind(format!("original-ledger-{name}")).bind(&source).bind(&key).bind(body.len()as i64).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at)VALUES($1,'company-owned',$2,$3,$4,$5,$6,$7,0)")
      .bind(&event).bind(&source).bind(&link).bind(&set).bind(actor).bind(action).bind(details.to_string()).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at)VALUES($1,$2,1,'pending',0)")
      .bind(format!("scan-{name}")).bind(&source).execute(pool.raw()).await.unwrap();
    RecordId::new(source).unwrap()
}
pub async fn claim(
    pool: &DatabasePool,
    id: &RecordId,
    clock: &TestClock,
) -> Option<PreparedAttachmentScan> {
    let id = id.clone();
    let clock = clock.clone();
    PgUnitOfWork::new(pool.clone())
        .run::<Option<PreparedAttachmentScan>, BusinessError, _>(
            TransactionOptions::default(),
            move |c| {
                let id = id.clone();
                let clock = clock.clone();
                Box::pin(async move {
                    let Some(claim) =
                        claim_attachment_scan(c, &scope(), Some(&id), &clock, &mut Ids).await?
                    else {
                        return Ok(None);
                    };
                    prepare_current_attachment_scan(c, claim, &clock)
                        .await
                        .map(Some)
                })
            },
        )
        .await
        .unwrap()
}
