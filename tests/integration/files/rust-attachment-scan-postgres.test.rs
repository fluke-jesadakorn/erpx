#[path = "../../support/files/scan-facts.rs"]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_files::public::attachment_scan::*;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::{BusinessError, Clock};
use sqlx::Connection;
use std::sync::Arc;

fn clean() -> AttachmentScanEvidence {
    AttachmentScanEvidence::Clean {
        engine: "ClamAV supplied component evidence".into(),
        definition_updated_at: 1,
        preview: None,
    }
}
fn missing() -> AttachmentScanFailure {
    AttachmentScanFailure::from_business(&BusinessError::invalid(
        "ATTACHMENT_OBJECT_MISSING",
        "Owned fixture computation failed.",
    ))
    .unwrap()
}

async fn retain(
    c: &mut sqlx::PgConnection,
    p: &PreparedAttachmentScan,
    clock: &facts::TestClock,
) -> Result<AttachmentScanSettlement, PgTransactionError<BusinessError>> {
    let proof = retain_current_attachment_scan(c, p, &clean(), clock, &mut facts::Ids).await?;
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *c)
        .await?;
    proof.require_current(c, clock).await?;
    Ok(proof.outcome())
}
async fn fail(
    c: &mut sqlx::PgConnection,
    p: &PreparedAttachmentScan,
    clock: &facts::TestClock,
) -> Result<AttachmentScanSettlement, PgTransactionError<BusinessError>> {
    let proof = fail_current_attachment_scan(c, p, &missing(), clock, &mut facts::Ids).await?;
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *c)
        .await?;
    proof.require_current(c, clock).await?;
    Ok(proof.outcome())
}
#[tokio::test]
async fn accepted_uploaded_draft_needs_no_new_person_or_saved_parent_and_retains_exact_atomic_effects()
 {
    fixture::with_database(2,|pool|async move{
  facts::migrate(&pool).await;let id=facts::seed_managed(&pool,"draft").await;let clock=facts::TestClock::new(1000);
  let prepared=Arc::new(facts::claim(&pool,&id,&clock).await.unwrap());
  assert_eq!(prepared.claim().attempt(),1);assert_eq!(prepared.claim().claim_until(),61_000);
  assert_eq!(prepared.identity().media_type,"text/csv");
  let original=prepared.clone();let now=clock.clone();
  let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let p=original.clone();let clock=now.clone();Box::pin(async move{
   assert_eq!(retain(c,&p,&clock).await?,AttachmentScanSettlement::Available);
   Err(BusinessError::invalid("OWNED_ROLLBACK","Caller owns atomic rollback.").into())
  })}).await;
  assert!(result.is_err());
  let state:(String,String,String,i64)=sqlx::query_as("SELECT a.source_state,u.status,j.state,(SELECT count(*)FROM attachment_events WHERE attachment_id=a.id AND action='scan.available')FROM attachments a JOIN upload_intents u ON u.id=a.upload_id JOIN attachment_scan_attempts j ON j.attachment_id=a.id WHERE a.id=$1")
   .bind(id.as_str()).fetch_one(pool.raw()).await.unwrap();
  assert_eq!(state,("scanning".into(),"uploaded".into(),"running".into(),0));
  let original=prepared.clone();let now=clock.clone();
  PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let p=original.clone();let clock=now.clone();Box::pin(async move{
   assert_eq!(retain(c,&p,&clock).await?,AttachmentScanSettlement::Available);Ok(())
  })}).await.unwrap();
  let state:(String,String,String,String,i64,i64)=sqlx::query_as("SELECT a.source_state,u.status,j.state,j.result_code,l.expires_at,(SELECT count(*)FROM attachment_storage_ledger WHERE attachment_id=a.id AND variant='preview')FROM attachments a JOIN upload_intents u ON u.id=a.upload_id JOIN attachment_scan_attempts j ON j.attachment_id=a.id JOIN attachment_links l ON l.id=u.attachment_link_id WHERE a.id=$1")
   .bind(id.as_str()).fetch_one(pool.raw()).await.unwrap();
  assert_eq!(state,("available".into(),"ready".into(),"clean".into(),"OK".into(),1000+7*24*60*60_000,0));
  let events:Vec<(String,String,serde_json::Value)>=sqlx::query_as("SELECT actor_id,action,details_json::jsonb FROM attachment_events WHERE attachment_id=$1 AND action LIKE 'scan.%'").bind(id.as_str()).fetch_all(pool.raw()).await.unwrap();
  assert_eq!(events.len(),1);assert_eq!(events[0].0,"system:jobs");assert_eq!(events[0].1,"scan.available");assert_eq!(events[0].2["preview"],false);
  // Same claimed source cannot be retained twice, even while its old deadline
  // is live. Job state is an actual current source fence, not a DTO assertion.
  let original=prepared.clone();let now=clock.clone();
  assert!(PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let p=original.clone();let clock=now.clone();Box::pin(async move{retain(c,&p,&clock).await?;Ok(())})}).await.is_err());
 }).await;
}

#[tokio::test]
async fn interrupted_claim_reuses_same_attempt_and_old_worker_cannot_settle_any_source_event_or_retry()
 {
    fixture::with_database(2,|pool|async move{
  facts::migrate(&pool).await;let id=facts::seed_managed(&pool,"reclaim").await;let clock=facts::TestClock::new(1000);
  let old=Arc::new(facts::claim(&pool,&id,&clock).await.unwrap());
  clock.set(61_000);assert!(facts::claim(&pool,&id,&clock).await.is_none()); // B0 uses strict claim_until<now.
  clock.set(61_001);let current=Arc::new(facts::claim(&pool,&id,&clock).await.unwrap());
  assert_eq!(old.claim().attempt(),current.claim().attempt());assert_eq!(current.claim().claim_until(),121_001);
  let p=old.clone();let now=clock.clone();
  let error=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let p=p.clone();let clock=now.clone();Box::pin(async move{fail(c,&p,&clock).await?;Ok(())})}).await;
  assert!(matches!(error,Err(PgTransactionError::Business(ref e))if e.code=="ATTACHMENT_SCAN_SOURCE_CHANGED"));
  let unchanged:(String,String,i64,i64)=sqlx::query_as("SELECT a.source_state,j.state,(SELECT count(*)FROM attachment_scan_attempts WHERE attachment_id=a.id),(SELECT count(*)FROM attachment_events WHERE attachment_id=a.id AND action LIKE 'scan.%')FROM attachments a JOIN attachment_scan_attempts j ON j.attachment_id=a.id WHERE a.id=$1")
   .bind(id.as_str()).fetch_one(pool.raw()).await.unwrap();
  assert_eq!(unchanged,("scanning".into(),"running".into(),1,0));
  let p=current.clone();let now=clock.clone();
  PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let p=p.clone();let clock=now.clone();Box::pin(async move{retain(c,&p,&clock).await?;Ok(())})}).await.unwrap();
 }).await;
}

#[tokio::test]
async fn each_real_error_settlement_retries_exact_b0_schedule_through_six_and_no_more() {
    fixture::with_database(2,|pool|async move{
  facts::migrate(&pool).await;let id=facts::seed_managed(&pool,"retry").await;let clock=facts::TestClock::new(1000);
  let delays=[2000,4000,8000,16000,32000];
  for attempt in 1..=6{
   let prepared=Arc::new(facts::claim(&pool,&id,&clock).await.unwrap());assert_eq!(prepared.claim().attempt(),attempt);
   let p=prepared.clone();let now=clock.clone();
   let outcome=PgUnitOfWork::new(pool.clone()).run::<AttachmentScanSettlement,BusinessError,_>(TransactionOptions::default(),move|c|{let p=p.clone();let clock=now.clone();Box::pin(async move{fail(c,&p,&clock).await})}).await.unwrap();
   assert_eq!(outcome,AttachmentScanSettlement::Error{next_attempt:if attempt<6{Some(attempt+1)}else{None}});
   if attempt<6{
    let due:i64=sqlx::query_scalar("SELECT created_at FROM attachment_scan_attempts WHERE attachment_id=$1 AND attempt=$2").bind(id.as_str()).bind(attempt+1).fetch_one(pool.raw()).await.unwrap();
    assert_eq!(due,clock.now().get()+delays[(attempt-1)as usize]);
    clock.set(due-1);assert!(facts::claim(&pool,&id,&clock).await.is_none());clock.set(due);
   }
  }
  clock.set(clock.now().get()+1_000_000);assert!(facts::claim(&pool,&id,&clock).await.is_none());
  let facts:(i64,i64,String)=sqlx::query_as("SELECT (SELECT count(*)FROM attachment_scan_attempts WHERE attachment_id=a.id),(SELECT count(*)FROM attachment_events WHERE attachment_id=a.id AND action='scan.error'),a.source_state FROM attachments a WHERE a.id=$1").bind(id.as_str()).fetch_one(pool.raw()).await.unwrap();
  assert_eq!(facts,(6,6,"scan_error".into()));
 }).await;
}

#[tokio::test]
async fn opaque_phase_proof_rejects_same_backend_next_transaction_and_old_snapshot_tuple_wait() {
    fixture::with_database(2,|pool|async move{
  facts::migrate(&pool).await;let id=facts::seed_managed(&pool,"fresh").await;let clock=facts::TestClock::new(1000);
  let prepared=facts::claim(&pool,&id,&clock).await.unwrap();
  let mut connection=pool.raw().acquire().await.unwrap();
  let mut first=connection.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *first).await.unwrap();
  let proof=read_current_attachment_scan_source(&mut first,prepared.claim(),&clock).await.unwrap();first.commit().await.unwrap();
  let mut second=connection.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *second).await.unwrap();
  assert!(proof.require_current(&mut second,prepared.claim(),&clock).await.is_err());second.rollback().await.unwrap();
  let mut old=connection.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *old).await.unwrap();
  sqlx::query("SELECT count(*)FROM attachments").execute(&mut *old).await.unwrap();
  // The writer commits before source acquisition. A later table lock would
  // not refresh this snapshot; the actual retained tuple fence must raise40001.
  sqlx::query("UPDATE upload_intents SET updated_at=updated_at+1 WHERE id=(SELECT upload_id FROM attachments WHERE id=$1)").bind(id.as_str()).execute(pool.raw()).await.unwrap();
  let error=read_current_attachment_scan_source(&mut old,prepared.claim(),&clock).await;
  assert!(matches!(error,Err(PgTransactionError::Persistence(ref e))if e.sql_state()==Some("40001")));
  old.rollback().await.unwrap();drop(connection);
  let p=Arc::new(prepared);let now=clock.clone();
  PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let p=p.clone();let clock=now.clone();Box::pin(async move{retain(c,&p,&clock).await?;Ok(())})}).await.unwrap();
 }).await;
}

#[tokio::test]
async fn intended_images_reject_source_and_audit_trigger_rewrites_instead_of_blessing_returned_rows()
 {
    fixture::with_database(2,|pool|async move{
  facts::migrate(&pool).await;
  sqlx::raw_sql("CREATE FUNCTION owned_scan_source_rewrite()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.id='attachment-upload-rewrite-source'AND NEW.source_state='available'THEN NEW.scanned_at:=NEW.scanned_at+1;END IF;RETURN NEW;END$$;CREATE TRIGGER owned_scan_source_rewrite BEFORE UPDATE ON attachments FOR EACH ROW EXECUTE FUNCTION owned_scan_source_rewrite();CREATE FUNCTION owned_scan_event_rewrite()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.attachment_id='attachment-upload-rewrite-event'AND NEW.action='scan.available'THEN NEW.details_json:='{\"unexpected\":true}';END IF;RETURN NEW;END$$;CREATE TRIGGER owned_scan_event_rewrite BEFORE INSERT ON attachment_events FOR EACH ROW EXECUTE FUNCTION owned_scan_event_rewrite();").execute(pool.raw()).await.unwrap();
  for name in["rewrite-source","rewrite-event"]{
   let id=facts::seed_managed(&pool,name).await;let clock=facts::TestClock::new(1000);let p=Arc::new(facts::claim(&pool,&id,&clock).await.unwrap());let now=clock.clone();
   let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let p=p.clone();let clock=now.clone();Box::pin(async move{retain(c,&p,&clock).await?;Ok(())})}).await;
   assert!(matches!(result,Err(PgTransactionError::Business(ref e))if e.code=="ATTACHMENT_SCAN_SOURCE_CHANGED"),"{name}");
   let state:(String,String,String,i64)=sqlx::query_as("SELECT a.source_state,u.status,j.state,(SELECT count(*)FROM attachment_events WHERE attachment_id=a.id AND action LIKE 'scan.%')FROM attachments a JOIN upload_intents u ON u.id=a.upload_id JOIN attachment_scan_attempts j ON j.attachment_id=a.id WHERE a.id=$1").bind(id.as_str()).fetch_one(pool.raw()).await.unwrap();
   assert_eq!(state,("scanning".into(),"uploaded".into(),"running".into(),0));
  }
 }).await;
}

#[tokio::test]
async fn final_receipt_refuses_late_and_deferred_changes_and_cannot_be_reused_after_commit() {
    fixture::with_database(2,|pool|async move{
  facts::migrate(&pool).await;
  sqlx::raw_sql("CREATE FUNCTION owned_scan_deferred_rewrite()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.attachment_id='attachment-upload-deferred'AND NEW.action='scan.available'THEN UPDATE upload_intents SET updated_at=updated_at+1 WHERE id='upload-deferred';END IF;RETURN NULL;END$$;CREATE CONSTRAINT TRIGGER owned_scan_deferred_rewrite AFTER INSERT ON attachment_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION owned_scan_deferred_rewrite();").execute(pool.raw()).await.unwrap();
  for name in["late","deferred"]{
   let id=facts::seed_managed(&pool,name).await;let clock=facts::TestClock::new(1000);let p=Arc::new(facts::claim(&pool,&id,&clock).await.unwrap());let now=clock.clone();
   let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let p=p.clone();let clock=now.clone();Box::pin(async move{
    let proof=retain_current_attachment_scan(c,&p,&clean(),&clock,&mut facts::Ids).await?;
    if name=="late"{sqlx::query("UPDATE upload_intents SET updated_at=updated_at+1 WHERE id='upload-late'").execute(&mut *c).await?;}
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await?;proof.require_current(c,&clock).await?;Ok(())
   })}).await;
   assert!(matches!(result,Err(PgTransactionError::Business(ref e))if e.code=="ATTACHMENT_SCAN_SOURCE_CHANGED"));
   let count:i64=sqlx::query_scalar("SELECT count(*)FROM attachment_events WHERE attachment_id=$1 AND action LIKE 'scan.%'").bind(id.as_str()).fetch_one(pool.raw()).await.unwrap();assert_eq!(count,0);
  }
  let id=facts::seed_managed(&pool,"post-commit").await;let clock=facts::TestClock::new(1000);let p=Arc::new(facts::claim(&pool,&id,&clock).await.unwrap());let now=clock.clone();
  let proof=Arc::new(PgUnitOfWork::new(pool.clone()).run::<CurrentAttachmentScanSettlement,BusinessError,_>(TransactionOptions::default(),move|c|{let p=p.clone();let clock=now.clone();Box::pin(async move{
   let proof=retain_current_attachment_scan(c,&p,&clean(),&clock,&mut facts::Ids).await?;sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await?;proof.require_current(c,&clock).await?;Ok(proof)
  })}).await.unwrap());let now=clock.clone();
  let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let proof=proof.clone();let clock=now.clone();Box::pin(async move{proof.require_current(c,&clock).await})}).await;
  assert!(matches!(result,Err(PgTransactionError::Business(ref e))if e.code=="ATTACHMENT_SCAN_SOURCE_CHANGED"));
 }).await;
}

#[tokio::test]
async fn real_legacy_alias_and_specialized_origins_sanitize_without_invented_parent_or_person_grants()
 {
    fixture::with_database(2,|pool|async move{
  facts::migrate(&pool).await;let clock=facts::TestClock::new(1000);
  for(name,origin)in[("legacy",facts::TestOrigin::Legacy),("specialized",facts::TestOrigin::Specialized)]{
   let id=facts::seed_source(&pool,name,origin,"image/png",include_bytes!("../../../fixtures/files/preview/png-alpha.input")).await;
   let p=Arc::new(facts::claim(&pool,&id,&clock).await.unwrap());let now=clock.clone();
   PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let p=p.clone();let clock=now.clone();Box::pin(async move{
    let proof=retain_current_attachment_scan(c,&p,&AttachmentScanEvidence::Infected{engine:"Actual scanner component evidence".into(),signature:"Owned.Test.Signature".into(),definition_updated_at:1},&clock,&mut facts::Ids).await?;
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await?;proof.require_current(c,&clock).await?;assert_eq!(proof.outcome(),AttachmentScanSettlement::MalwareRejected);Ok(())
   })}).await.unwrap();
   let state:(String,String,String,i64)=sqlx::query_as("SELECT a.source_state,u.status,s.purpose_key,(SELECT count(*)FROM attachment_storage_ledger WHERE attachment_id=a.id AND variant='preview')FROM attachments a JOIN upload_intents u ON u.id=a.upload_id JOIN attachment_sets s ON s.id=u.attachment_set_id WHERE a.id=$1").bind(id.as_str()).fetch_one(pool.raw()).await.unwrap();
   assert_eq!(state,("malware_rejected".into(),"uploaded".into(),if name=="legacy"{"payment_slip"}else{"legal_document"}.into(),0));
  }
  let id=facts::seed_source_with_receipt(&pool,"unproved",facts::TestOrigin::Managed,"text/csv",facts::bytes(),false).await;
  let now=clock.clone();let requested=id.clone();
  let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let clock=now.clone();let id=requested.clone();Box::pin(async move{
   let Some(claim)=claim_attachment_scan(c,&facts::scope(),Some(&id),&clock,&mut facts::Ids).await?else{panic!("actual pending source should be claimed before origin check")};prepare_current_attachment_scan(c,claim,&clock).await?;Ok(())
  })}).await;
  assert!(matches!(result,Err(PgTransactionError::Business(ref e))if e.code=="ATTACHMENT_SCAN_SOURCE_CHANGED"));
  let state:(String,String,i64)=sqlx::query_as("SELECT a.source_state,j.state,(SELECT count(*)FROM attachment_events WHERE attachment_id=a.id AND action LIKE 'scan.%')FROM attachments a JOIN attachment_scan_attempts j ON j.attachment_id=a.id WHERE a.id=$1").bind(id.as_str()).fetch_one(pool.raw()).await.unwrap();assert_eq!(state,("quarantined".into(),"pending".into(),0));
  let now=clock.clone();let requested=id.clone();
  PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let clock=now.clone();let id=requested.clone();Box::pin(async move{
   let mut wrong=facts::scope();wrong.company_id=erpx_server_shared::public::CompanyId::new("outside-company").unwrap();assert!(claim_attachment_scan(c,&wrong,Some(&id),&clock,&mut facts::Ids).await?.is_none());Ok(())
  })}).await.unwrap();
  let overlap=facts::seed_managed(&pool,"overlap").await;sqlx::query("INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at)VALUES('historical-overlap',$1,2,'pending',0)").bind(overlap.as_str()).execute(pool.raw()).await.unwrap();let now=clock.clone();
  let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let clock=now.clone();let id=overlap.clone();Box::pin(async move{claim_attachment_scan(c,&facts::scope(),Some(&id),&clock,&mut facts::Ids).await?;Ok(())})}).await;
  assert!(matches!(result,Err(PgTransactionError::Business(ref e))if e.code=="ATTACHMENT_SCAN_SOURCE_CHANGED"));
 }).await;
}
fn write_original(
    root: &std::path::Path,
    p: &PreparedAttachmentScan,
    body: &[u8],
) -> std::path::PathBuf {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;
    let directory = root.join(hex::encode(Sha256::digest(p.original().key().as_bytes())));
    std::fs::create_dir(&directory).unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    let version = uuid::Uuid::new_v4().to_string();
    let file = directory.join(&version);
    std::fs::write(&file, body).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    let manifest = directory.join("current.json");
    std::fs::write(&manifest,serde_json::to_vec(&serde_json::json!({"key":p.original().key(),"version":version,"size":body.len(),"etag":p.original().checksum_sha256()})).unwrap()).unwrap();
    std::fs::set_permissions(manifest, std::fs::Permissions::from_mode(0o600)).unwrap();
    file
}
#[tokio::test]
async fn actual_private_storage_stages_without_database_lease_adopts_crash_object_and_retains_exact_ledger()
 {
    fixture::with_database(2,|pool|async move{
  use erpx_server_files::public::preview::{PreviewArtifact,PreviewMedia};use sha2::{Digest,Sha256};use std::os::unix::fs::{PermissionsExt,symlink};
  facts::migrate(&pool).await;let clock=facts::TestClock::new(1000);
  let body=include_bytes!("../../../fixtures/files/preview/png-alpha.input");let output=include_bytes!("../../../fixtures/files/preview/png-alpha.webp");
  let id=facts::seed_source(&pool,"storage",facts::TestOrigin::Managed,"image/png",body).await;let prepared=Arc::new(facts::claim(&pool,&id,&clock).await.unwrap());
  let root=tempfile::Builder::new().permissions(std::fs::Permissions::from_mode(0o700)).tempdir().unwrap();let store=LocalSanitizationStore::open(root.path(),2).unwrap();let original=write_original(root.path(),&prepared,body);
  assert_eq!(store.read(prepared.original()).await.ok().unwrap(),body);
  // No owner connection/transaction survives prepare into physical I/O.
  let active:i64=sqlx::query_scalar("SELECT count(*)FROM pg_stat_activity WHERE datname=current_database()AND pid<>pg_backend_pid()AND xact_start IS NOT NULL").fetch_one(pool.raw()).await.unwrap();assert_eq!(active,0);
  let artifact=||PreviewArtifact::from_rendered(output.to_vec(),PreviewMedia::Webp).unwrap();
  let csv_id=facts::seed_managed(&pool,"storage-no-preview").await;let csv=facts::claim(&pool,&csv_id,&clock).await.unwrap();let entries=std::fs::read_dir(root.path()).unwrap().count();
  assert!(stage_attachment_preview(&csv,artifact(),&store,&clock).await.is_err());assert_eq!(std::fs::read_dir(root.path()).unwrap().count(),entries);
  let abandoned=stage_attachment_preview(&prepared,artifact(),&store,&clock).await.unwrap();drop(abandoned);
  let before:(String,i64)=sqlx::query_as("SELECT source_state,(SELECT count(*)FROM attachment_storage_ledger WHERE attachment_id=a.id AND variant='preview')FROM attachments a WHERE id=$1").bind(id.as_str()).fetch_one(pool.raw()).await.unwrap();assert_eq!(before,("scanning".into(),0));
  let staged=stage_attachment_preview(&prepared,artifact(),&store,&clock).await.unwrap();
  let expected_key=preview_object_key(prepared.original().key(),&hex::encode(Sha256::digest(output)),"image/webp").unwrap();let dir=root.path().join(hex::encode(Sha256::digest(expected_key.as_bytes())));
  let current:serde_json::Value=serde_json::from_slice(&std::fs::read(dir.join("current.json")).unwrap()).unwrap();let version=current["version"].as_str().unwrap();assert_eq!(std::fs::read(dir.join(version)).unwrap(),output);assert_eq!(std::fs::read_dir(&dir).unwrap().count(),2);assert_eq!(current["customMetadata"]["sanitized"],"true");
  let p=prepared.clone();let now=clock.clone();let evidence=Arc::new(AttachmentScanEvidence::Clean{engine:"ClamAV component evidence".into(),definition_updated_at:1,preview:Some(staged)});
  PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let p=p.clone();let clock=now.clone();let evidence=evidence.clone();Box::pin(async move{
   let proof=retain_current_attachment_scan(c,&p,&evidence,&clock,&mut facts::Ids).await?;sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await?;proof.require_current(c,&clock).await?;Ok(())
  })}).await.unwrap();
  let after:(String,String,String,i64,i64)=sqlx::query_as("SELECT a.source_state,a.preview_object_key,l.object_key,l.bytes,(SELECT count(*)FROM attachment_events WHERE attachment_id=a.id AND action='scan.available')FROM attachments a JOIN attachment_storage_ledger l ON l.attachment_id=a.id AND l.variant='preview'WHERE a.id=$1").bind(id.as_str()).fetch_one(pool.raw()).await.unwrap();assert_eq!(after,("available".into(),expected_key.clone(),expected_key,output.len()as i64,1));
  // Existing immutable bytes can never be silently overwritten/adopted after
  // corruption, and quarantine reads retain actual no-follow/mode/hash checks.
  std::fs::write(dir.join(version),b"corrupt").unwrap();assert!(stage_attachment_preview(&prepared,artifact(),&store,&clock).await.is_err());assert_eq!(std::fs::read(dir.join(version)).unwrap(),b"corrupt");
  let original_fd=std::fs::OpenOptions::new().write(true).open(&original).unwrap();original_fd.set_len(10*1024*1024+1).unwrap();assert!(matches!(store.read(prepared.original()).await,Err(QuarantinedReadFailure::Oversize)));original_fd.set_len(1).unwrap();assert!(matches!(store.read(prepared.original()).await,Err(QuarantinedReadFailure::Metadata)));drop(original_fd);
  std::fs::write(&original,vec![0;body.len()]).unwrap();assert!(matches!(store.read(prepared.original()).await,Err(QuarantinedReadFailure::Checksum)));
  std::fs::set_permissions(&original,std::fs::Permissions::from_mode(0o644)).unwrap();assert!(matches!(store.read(prepared.original()).await,Err(QuarantinedReadFailure::Metadata)));
  std::fs::remove_file(&original).unwrap();let outside=root.path().join("outside");std::fs::write(&outside,body).unwrap();symlink(&outside,&original).unwrap();assert!(matches!(store.read(prepared.original()).await,Err(QuarantinedReadFailure::Metadata)));
  std::fs::remove_file(&original).unwrap();nix::unistd::mkfifo(&original,nix::sys::stat::Mode::from_bits_truncate(0o600)).unwrap();assert!(matches!(store.read(prepared.original()).await,Err(QuarantinedReadFailure::Metadata)));
  store.stop();store.drain().await;assert!(store.require_available().is_err());
 }).await;
}
