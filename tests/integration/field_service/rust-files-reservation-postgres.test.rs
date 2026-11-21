//! Actual SERIALIZABLE Files reservation effects and retry over leased PostgreSQL.
#[allow(dead_code, unused_imports)]
#[path = "../../support/field_service/parts.rs"]
mod facts;
use erpx_server_files::public::{
    field_visit_intake::{FieldIntakeAuthority, FieldIntakeScope},
    field_visit_upload::{FieldFileReservation, FieldStorageQuota, reserve_field_file},
};
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, WorkAuthorizationRequest,
};
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, HoldingId};
use sqlx::PgConnection;
// Isolated Files-owner fixture; production authority is supplied by composition.
#[derive(Clone)]
struct CleanupFixture(sqlx::PgPool);
impl erpx_server_files::public::field_visit_upload::FieldCleanupTransactions for CleanupFixture {
    async fn metadata<T,F>(&self, work:F)->Result<T,erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>>
    where T:Send+'static, F:for<'a> Fn(&'a mut PgConnection,&'a erpx_server_shared::public::CompanyId)->PgWorkFuture<'a,T,BusinessError>+Clone+Send+Sync+'static {
        let mut tx=self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await?;
        let company=erpx_server_shared::public::CompanyId::new("company").unwrap();
        let result=work(&mut tx,&company).await?; tx.commit().await?; Ok(result)
    }
    async fn physical<T,F>(&self, work:F)->Result<T,erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>>
    where T:Send+'static, F:for<'a> FnOnce(&'a mut PgConnection,&'a erpx_server_shared::public::CompanyId)->PgWorkFuture<'a,T,BusinessError>+Send+'static {
        let mut tx=self.0.begin().await?;
        sqlx::query("LOCK TABLE attachment_links, attachment_sets, attachment_storage_ledger, attachments, field_upload_object_cleanup_claims, field_upload_publication_pins, upload_intents IN SHARE MODE").execute(&mut *tx).await?;
        let company=erpx_server_shared::public::CompanyId::new("company").unwrap();
        let result=work(&mut tx,&company).await?; tx.commit().await?; Ok(result)
    }
}
struct Authority;
impl FieldIntakeAuthority for Authority {
    fn authorize<'a>(
        &'a self,
        _: &'a mut PgConnection,
        actor: &'a AuthorizedWorkContext,
        _: &'a str,
        _: bool,
    ) -> PgWorkFuture<'a, FieldIntakeScope, BusinessError> {
        Box::pin(async move {
            Ok(FieldIntakeScope {
                company_id: actor.selection().workspace().company_id.clone(),
                branch_id: Some("branch".into()),
                department_id: None,
                version: 3,
            })
        })
    }
    fn revalidate<'a>(
        &'a self,
        _: &'a mut PgConnection,
        _: &'a AuthorizedWorkContext,
        _: &'a str,
        _: bool,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}
struct DenialCurrent { fail_after: bool, calls: std::sync::atomic::AtomicUsize }
impl erpx_server_files::public::field_visit_upload::FieldDenialActorCurrent for DenialCurrent {
    fn require_current<'a>(&'a self, _: &'a mut PgConnection, _: &'a AuthorizedWorkContext, _: i64) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.fail_after && n > 0 { return Err(BusinessError::invalid("FIXTURE_CURRENT_CHANGED", "Current actor changed.").into()); }
            Ok(())
        })
    }
}
struct Quota;
impl FieldStorageQuota for Quota {
    fn storage_limit<'a>(
        &'a self,
        _: &'a mut PgConnection,
        _: &'a str,
    ) -> PgWorkFuture<'a, Option<i64>, BusinessError> {
        Box::pin(async { Ok(Some(1000)) })
    }
}
#[tokio::test]
async fn reservation_replay_is_exact_and_creates_one_owner_effect() {
    facts::account::with_system(|system|async move {
  let p=facts::setup_workspace(system).await;let now=p.review.system.scenario.clock.now().get();
  sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES('reserve-set','company','branch','field_service_visit','issue-order','record','site_photo',3,1,'internal','{}','fixture-a',$1,$1)").bind(now).execute(p.pool().raw()).await.unwrap();
  let result=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),WorkAuthorizationRequest{permission:"records.read".into(),module:Some("field_service".into()),destination:"field-service".into(),api:true},p.review.configuration.clone(),move|c,actor|Box::pin(async move{
   let input=FieldFileReservation{filename:"  image.png ".into(),media_type:"image/png".into(),bytes:20,caption:None,supersedes_link_id:None};let holding=HoldingId::new("h001").unwrap();
   let first=reserve_field_file(c,&Authority,&Quota,actor,&holding,"reserve-set",&input,"reservation-key",now).await?;
   let replay=reserve_field_file(c,&Authority,&Quota,actor,&holding,"reserve-set",&input,"reservation-key",now).await?;
   assert!(!first.replay);assert!(replay.replay);assert_eq!(first.upload_intent_id,replay.upload_intent_id);assert_eq!(first.attachment_link_id,replay.attachment_link_id);
   let mut changed=input.clone();changed.bytes+=1;assert!(reserve_field_file(c,&Authority,&Quota,actor,&holding,"reserve-set",&changed,"reservation-key",now).await.is_err());Ok(())
  })).await;result.unwrap();
  let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM upload_intents WHERE attachment_set_id='reserve-set'),(SELECT count(*) FROM attachment_links WHERE set_id='reserve-set'),(SELECT count(*) FROM attachment_events WHERE set_id='reserve-set' AND action='file.reserved')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(counts,(1,1,1));
 }).await;
}

#[tokio::test]
async fn upload_short_units_publish_real_quarantine_and_replay() {
    use erpx_server_files::public::field_visit_upload::{
        FieldContentVariant, FieldUploadPreparation, LocalFieldQuarantineWriter,
        complete_field_upload, finish_field_content, prepare_field_content, prepare_field_upload,
        publish_pinned_field_upload, read_field_content_bytes, read_field_link_status,
        release_field_publication_pin,
    };
    use std::os::unix::fs::PermissionsExt;
    facts::account::with_system(|system| async move {
        let p=facts::setup_workspace(system).await;
        let now=p.review.system.scenario.clock.now().get();
        sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES('upload-set','company','branch','field_service_visit','issue-order','record','site_photo',3,1,'internal','{}','fixture-a',$1,$1)").bind(now).execute(p.pool().raw()).await.unwrap();
        let request=||WorkAuthorizationRequest{permission:"records.read".into(),module:Some("field_service".into()),destination:"field-service".into(),api:true};
        let reserved=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|Box::pin(async move{
            let input=FieldFileReservation{filename:"image.png".into(),media_type:"image/png".into(),bytes:24,caption:None,supersedes_link_id:None};
            reserve_field_file(c,&Authority,&Quota,actor,&HoldingId::new("h001").unwrap(),"upload-set",&input,"upload-key",now).await
        })).await.unwrap();
        let body=b"\x89PNGabcdefgh\0\0\0\0IEND\xaeB`\x82".to_vec();assert_eq!(body.len(),24);
        let wrong_id=reserved.upload_intent_id.clone();let wrong_body=body.clone();
        let wrong=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|{let id=wrong_id.clone();let body=wrong_body.clone();Box::pin(async move{prepare_field_upload(c,&Authority,actor,&id,"image/jpeg",body,now).await})}).await;
        assert!(wrong.is_err());
        let no_pins:i64=sqlx::query_scalar("SELECT count(*) FROM field_upload_publication_pins WHERE upload_intent_id=$1").bind(&reserved.upload_intent_id).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(no_pins,0);
        let upload=reserved.upload_intent_id.clone();let prepare_id=upload.clone();let first_bytes=body.clone();
        let prep=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|{let prepare_id=prepare_id.clone();let first_bytes=first_bytes.clone();Box::pin(async move{prepare_field_upload(c,&Authority,actor,&prepare_id,"image/png",first_bytes,now).await})}).await.unwrap();
        let (publication,pin)=match prep{FieldUploadPreparation::Pending { publication, pin }=>(publication,pin),FieldUploadPreparation::Replay(_)=>panic!("fresh upload replayed")};
        let second_id=upload.clone();let second_bytes=body.clone();
        let second=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|{let second_id=second_id.clone();let second_bytes=second_bytes.clone();Box::pin(async move{prepare_field_upload(c,&Authority,actor,&second_id,"image/png",second_bytes,now).await})}).await.unwrap();
        let (second_publication,second_pin)=match second{FieldUploadPreparation::Pending{publication,pin}=>(publication,pin),FieldUploadPreparation::Replay(_)=>panic!("second upload replayed")};
        let root=tempfile::Builder::new().permissions(std::fs::Permissions::from_mode(0o700)).tempdir().unwrap();let store=LocalFieldQuarantineWriter::open(root.path(),2).unwrap();let first_receipt=publish_pinned_field_upload(&store,p.pool().raw(),&pin,publication,p.review.system.scenario.clock.as_ref()).await.unwrap();assert!(first_receipt.created());let receipt=publish_pinned_field_upload(&store,p.pool().raw(),&second_pin,second_publication,p.review.system.scenario.clock.as_ref()).await.unwrap();assert!(!receipt.created());
        let first_pin=std::sync::Arc::new(pin);p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,_actor|{let pin=first_pin.clone();Box::pin(async move{release_field_publication_pin(c,&pin,now).await})}).await.unwrap();
        let pins:(i64,i64)=sqlx::query_as("SELECT count(*) FILTER(WHERE state='active'),count(*) FILTER(WHERE state='released') FROM field_upload_publication_pins WHERE upload_intent_id=$1").bind(&upload).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(pins,(1,1));
        let receipt=std::sync::Arc::new(receipt);let pin=std::sync::Arc::new(second_pin);let commit_clock=p.review.system.scenario.clock.clone();let commit_id=upload.clone();let result=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|{let receipt=receipt.clone();let pin=pin.clone();let commit_id=commit_id.clone();let clock=commit_clock.clone();Box::pin(async move{complete_field_upload(c,&Authority,actor,&commit_id,&receipt,&pin,clock.as_ref()).await})}).await.unwrap();assert_eq!(result.source_state,"quarantined");assert!(!result.replay);
        let replay_id=upload.clone();let replay=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|{let replay_id=replay_id.clone();let body=body.clone();Box::pin(async move{prepare_field_upload(c,&Authority,actor,&replay_id,"image/png",body,now).await})}).await.unwrap();assert!(matches!(replay,FieldUploadPreparation::Replay(_)));
        let link=reserved.attachment_link_id.clone();let status=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|{let link=link.clone();Box::pin(async move{read_field_link_status(c,&Authority,actor,&link,false,now).await})}).await.unwrap();assert_eq!(status["sourceState"],"quarantined");assert_eq!(status["scan"]["attempts"],1);assert_eq!(status["bindable"],false);
        // The separately authorized denial unit commits only while its trusted
        // current-actor witness succeeds both before and after the evidence write.
        for fail_after in [false, true] {
            let denial_link=reserved.attachment_link_id.clone();
            let clock=p.review.system.scenario.clock.clone();
            let result=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|{
                let link=denial_link.clone();let clock=clock.clone();Box::pin(async move {
                    let current=DenialCurrent{fail_after,calls:std::sync::atomic::AtomicUsize::new(0)};
                    erpx_server_files::public::field_visit_upload::record_field_content_denial(c,&current,actor,&link,"original","SOURCE_DENIED",clock.as_ref()).await
                })
            }).await;
            assert_eq!(result.is_err(),fail_after);
        }
        let denied:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_access_events WHERE link_id=$1 AND outcome='denied' AND reason='SOURCE_DENIED'").bind(&reserved.attachment_link_id).fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(denied,1);
        // Synthetic available transition isolates content authority and physical
        // reader proof; real scanner/provider settlement is a separate gate.
        sqlx::query("UPDATE attachments SET source_state='scanning' WHERE id=$1").bind(&result.source_id).execute(p.pool().raw()).await.unwrap();
        sqlx::query("UPDATE attachments SET source_state='available',scanned_at=$2,scanner_definition_at=$2 WHERE id=$1").bind(&result.source_id).bind(now).execute(p.pool().raw()).await.unwrap();
        use erpx_server_files::public::attachment_scan::{QuarantinedOriginalInput, QuarantinedOriginalStore, QuarantinedReadFailure, LocalSanitizationStore};
        use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
        struct CountingStore { real: LocalSanitizationStore, calls: Arc<AtomicUsize> }
        impl QuarantinedOriginalStore for CountingStore {
            async fn read(&self, input: &QuarantinedOriginalInput) -> Result<Vec<u8>, QuarantinedReadFailure> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                self.real.read(input).await
            }
        }
        let reader=CountingStore{real:LocalSanitizationStore::open(root.path(),2).unwrap(),calls:Arc::new(AtomicUsize::new(0))};
        // The opaque session is real; revoking its selected assignment must
        // reject the initial unit before a physical store read can be invoked.
        sqlx::query("UPDATE assignments SET revoked_at=$1,version=version+1 WHERE id='field-fixture-a'").bind(now).execute(p.pool().raw()).await.unwrap();
        let denied_link=reserved.attachment_link_id.clone();
        let denied=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|{let link=denied_link.clone();Box::pin(async move{prepare_field_content(c,&Authority,actor,&link,FieldContentVariant::Original,false,now).await})}).await;
        if let Ok(witness)=denied { let _=read_field_content_bytes(&reader,witness).await; panic!("revoked initial authority returned a content witness"); }
        assert_eq!(reader.calls.load(Ordering::SeqCst),0,"initial denial must not touch the real store");
        sqlx::query("UPDATE assignments SET revoked_at=NULL,version=3 WHERE id='field-fixture-a'").execute(p.pool().raw()).await.unwrap();
        let content_link=reserved.attachment_link_id.clone();
        let witness=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|{let link=content_link.clone();Box::pin(async move{prepare_field_content(c,&Authority,actor,&link,FieldContentVariant::Original,false,now).await})}).await.unwrap();
        let read=read_field_content_bytes(&reader,witness).await.unwrap();
        assert_eq!(reader.calls.load(Ordering::SeqCst),1);
        // A real SQLSTATE 40001 forces the first finalization transaction to
        // retry. The already-read sealed bytes are reused without another I/O.
        let audits_before:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_access_events WHERE link_id=$1 AND outcome='allowed'").bind(&reserved.attachment_link_id).fetch_one(p.pool().raw()).await.unwrap();
        sqlx::raw_sql("CREATE SEQUENCE field_read_retry_attempt; CREATE FUNCTION field_read_retry_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF nextval('field_read_retry_attempt')=1 THEN RAISE EXCEPTION 'declared serialization fault' USING ERRCODE='40001'; END IF; RETURN NEW; END $$; CREATE TRIGGER field_read_retry_fault BEFORE INSERT ON attachment_access_events FOR EACH ROW WHEN (NEW.outcome='allowed') EXECUTE FUNCTION field_read_retry_fault()").execute(p.pool().raw()).await.unwrap();
        let response=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|{let read=read.clone();Box::pin(async move{finish_field_content(c,&Authority,actor,read,false,now).await})}).await.unwrap();
        let attempts:i64=sqlx::query_scalar("SELECT last_value FROM field_read_retry_attempt").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(attempts,2);
        let audits:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_access_events WHERE link_id=$1 AND outcome='allowed'").bind(&reserved.attachment_link_id).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(audits,audits_before+1);
        assert_eq!(reader.calls.load(Ordering::SeqCst),1,"finalization retry must not replay physical I/O");
        sqlx::raw_sql("DROP TRIGGER field_read_retry_fault ON attachment_access_events; DROP FUNCTION field_read_retry_fault(); DROP SEQUENCE field_read_retry_attempt").execute(p.pool().raw()).await.unwrap();
        assert_eq!(response.bytes,b"\x89PNGabcdefgh\0\0\0\0IEND\xaeB`\x82");assert_eq!(response.media_type,"image/png");assert!(!response.inline);
        // Hold an actual committed source writer before the reader's first
        // owner snapshot. Count native source-preparation attempts, so the
        // original no-retry and no-physical-read observations remain explicit.
        #[derive(Clone)] struct CountedPreparation(Arc<AtomicUsize>);
        impl erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation for CountedPreparation {
            fn prepare<'c>(&'c self,c:&'c mut PgConnection)->PgWorkFuture<'c,(),BusinessError>{
                self.0.fetch_add(1,Ordering::SeqCst);
                erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation::prepare(&erpx_service::field_attachment_intake::FieldAttachmentIntakeSources,c)
            }
        }
        let attempts=Arc::new(AtomicUsize::new(0));let mut writer=p.pool().raw().begin().await.unwrap();
        sqlx::query("UPDATE attachment_links SET state='withdrawn',withdrawn_by=created_by,withdrawn_at=$2,withdrawal_reason='Declared committed source withdrawal' WHERE id=$1").bind(&reserved.attachment_link_id).bind(now).execute(&mut *writer).await.unwrap();
        let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        let authorizer=p.review.authorizer.clone();let proof=p.review.maker.clone();let selection=p.selection.clone();let withdrawn=reserved.attachment_link_id.clone();let seen=attempts.clone();
        let pending=tokio::spawn(async move{authorizer.execute_with_sources("acme.example.test",proof,selection,WorkAuthorizationRequest{permission:"records.read".into(),module:Some("field_service".into()),destination:"field-service".into(),api:true},CountedPreparation(seen),move|c,actor|{let link=withdrawn.clone();Box::pin(async move{prepare_field_content(c,&Authority,actor,&link,FieldContentVariant::Original,false,now).await})}).await});
        let mut blocked=false;for _ in 0..200 {blocked=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1::int=ANY(pg_blocking_pids(pid)))").bind(writer_pid).fetch_one(p.pool().raw()).await.unwrap();if blocked||pending.is_finished(){break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
        writer.commit().await.unwrap();assert!(blocked,"native reader must wait for the committed source writer");
        let rejected=pending.await.unwrap();assert!(matches!(rejected,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(ref e)) if e.code=="ATTACHMENT_LINK_NOT_FOUND"));
        assert_eq!(attempts.load(Ordering::SeqCst),1,"withdrawn source read must not retry");assert_eq!(reader.calls.load(Ordering::SeqCst),1,"withdrawn initial source must not open the physical object");
        reader.real.stop();reader.real.drain().await;
        let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM attachment_scan_attempts WHERE attachment_id=$1),(SELECT count(*) FROM attachment_storage_ledger WHERE attachment_id=$1),(SELECT count(*) FROM attachment_events WHERE attachment_id=$1 AND action='file.quarantined')").bind(&result.source_id).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(counts,(1,1,1));store.stop();store.drain().await;
    }).await;
}

#[tokio::test]
async fn expired_unreferenced_upload_reconciles_only_after_owner_deadline() {
    use erpx_server_files::public::field_visit_upload::{
        FieldCleanupOutcome, FieldUploadPreparation, LocalFieldQuarantineWriter,
        prepare_field_upload, publish_pinned_field_upload, reconcile_field_upload_object,
        release_field_publication_pin,
    };
    use std::{
        os::unix::fs::PermissionsExt,
        sync::{Arc, atomic::Ordering},
    };
    facts::account::with_system(|system|async move{
        let p=facts::setup_workspace(system).await;let now=p.review.system.scenario.clock.now().get();
        sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES('gc-set','company','branch','field_service_visit','issue-order','record','site_photo',3,1,'internal','{}','fixture-a',$1,$1)").bind(now).execute(p.pool().raw()).await.unwrap();
        let request=||WorkAuthorizationRequest{permission:"records.read".into(),module:Some("field_service".into()),destination:"field-service".into(),api:true};
        let reserved=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|Box::pin(async move{let input=FieldFileReservation{filename:"photo.png".into(),media_type:"image/png".into(),bytes:24,caption:None,supersedes_link_id:None};reserve_field_file(c,&Authority,&Quota,actor,&HoldingId::new("h001").unwrap(),"gc-set",&input,"gc-upload",now).await})).await.unwrap();
        let upload=reserved.upload_intent_id.clone();let id=upload.clone();let body=b"\x89PNGabcdefgh\0\0\0\0IEND\xaeB`\x82".to_vec();
        let prep=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|{let id=id.clone();let body=body.clone();Box::pin(async move{prepare_field_upload(c,&Authority,actor,&id,"image/png",body,now).await})}).await.unwrap();
        let (publication,pin)=match prep{FieldUploadPreparation::Pending{publication,pin}=>(publication,pin),_=>panic!("expected pending")};let key=publication.key.clone();
        let stale_publication=publication.clone();let root=tempfile::Builder::new().permissions(std::fs::Permissions::from_mode(0o700)).tempdir().unwrap();let store=LocalFieldQuarantineWriter::open(root.path(),2).unwrap();let receipt=publish_pinned_field_upload(&store,p.pool().raw(),&pin,publication,p.review.system.scenario.clock.as_ref()).await.unwrap();assert!(receipt.created());
        assert_eq!(reconcile_field_upload_object(&store,&CleanupFixture(p.pool().raw().clone()),&upload,p.review.system.scenario.clock.clone()).await.unwrap(),FieldCleanupOutcome::NotEligible);
        let still_live=publish_pinned_field_upload(&store,p.pool().raw(),&pin,stale_publication.clone(),p.review.system.scenario.clock.as_ref()).await.unwrap();assert!(!still_live.created(),"a live publisher retains its already published object");
        let pin=Arc::new(pin);let stale_pin=pin.clone();p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,_|{let pin=pin.clone();Box::pin(async move{release_field_publication_pin(c,&pin,now).await})}).await.unwrap();
        let gc_now=reserved.expires_at+1;p.review.system.scenario.clock.0.store(gc_now,Ordering::SeqCst);
        // A different retained upload referencing the same physical key prevents deletion.
        sqlx::query("INSERT INTO upload_intents SELECT (jsonb_populate_record(NULL::upload_intents,to_jsonb(u)||jsonb_build_object('id','shared-key-fixture','staging_key','fixture-other-staging','attachment_set_id',NULL,'attachment_link_id',NULL))).* FROM upload_intents u WHERE u.id=$1").bind(&upload).execute(p.pool().raw()).await.unwrap();
        assert_eq!(reconcile_field_upload_object(&store,&CleanupFixture(p.pool().raw().clone()),&upload,p.review.system.scenario.clock.clone()).await.unwrap(),FieldCleanupOutcome::NotEligible);
        sqlx::query("DELETE FROM upload_intents WHERE id='shared-key-fixture'").execute(p.pool().raw()).await.unwrap();
        // Real two-worker stale-lease test: hold the claim row so worker one
        // owns the OS flock while waiting for the database. Worker two must
        // not reclaim/delete, even after the first worker's lease is stale.
        let old_token=uuid::Uuid::new_v4();sqlx::query("INSERT INTO field_upload_object_cleanup_claims(company_id,object_key,upload_intent_id,token,claimed_at,claim_until) VALUES('company',$1,$2,$3,$4,$5)").bind(&key).bind(&upload).bind(old_token).bind(gc_now-100).bind(gc_now-1).execute(p.pool().raw()).await.unwrap();
        let mut blocked=p.pool().raw().begin().await.unwrap();sqlx::query("UPDATE field_upload_object_cleanup_claims SET claimed_at=claimed_at WHERE token=$1").bind(old_token).execute(&mut *blocked).await.unwrap();
        let gc_store=store.clone();let gc_pool=p.pool().raw().clone();let gc_clock=p.review.system.scenario.clock.clone();let gc_id=upload.clone();
        let first=tokio::spawn(async move{reconcile_field_upload_object(&gc_store,&CleanupFixture(gc_pool),&gc_id,gc_clock).await});
        let mut waiting=false;for _ in 0..100 {waiting=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'INSERT INTO field_upload_object_cleanup_claims%')").fetch_one(p.pool().raw()).await.unwrap();if waiting{break;}tokio::time::sleep(std::time::Duration::from_millis(50)).await;}assert!(waiting,"first cleanup worker did not reach its locked claim");
        p.review.system.scenario.clock.0.store(gc_now+31_000,Ordering::SeqCst);
        assert!(reconcile_field_upload_object(&store,&CleanupFixture(p.pool().raw().clone()),&upload,p.review.system.scenario.clock.clone()).await.is_err(),"second worker must not pass the held OS lock");
        assert!(publish_pinned_field_upload(&store,p.pool().raw(),&stale_pin,stale_publication,p.review.system.scenario.clock.as_ref()).await.is_err(),"expired publisher must not pass the held object lock");
        blocked.commit().await.unwrap();assert!(first.await.unwrap().is_err(),"expired first worker must not delete or clear claim");
        assert_eq!(reconcile_field_upload_object(&store,&CleanupFixture(p.pool().raw().clone()),&upload,p.review.system.scenario.clock.clone()).await.unwrap(),FieldCleanupOutcome::Cleaned);
        use sha2::{Digest,Sha256};let folder=root.path().join(hex::encode(Sha256::digest(key.as_bytes())));assert!(!folder.join("current.json").exists());
        let facts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM upload_intents WHERE id=$1),(SELECT count(*) FROM field_upload_publication_pins WHERE upload_intent_id=$1),(SELECT count(*) FROM field_upload_object_cleanup_claims WHERE upload_intent_id=$1)").bind(&upload).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(facts,(1,1,0));store.stop();store.drain().await;
    }).await;
}

#[tokio::test]
async fn cleanup_reclaims_after_pointer_unlink_using_persisted_version() {
    use erpx_server_files::public::field_visit_upload::{
        FieldCleanupOutcome, FieldUploadPreparation, LocalFieldQuarantineWriter,
        prepare_field_upload, publish_pinned_field_upload, reconcile_field_upload_object,
        release_field_publication_pin,
    };
    use std::{
        os::unix::fs::PermissionsExt,
        sync::{Arc, atomic::Ordering},
    };
    facts::account::with_system(|system|async move{
        let p=facts::setup_workspace(system).await;let now=p.review.system.scenario.clock.now().get();
        sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES('gc-set','company','branch','field_service_visit','issue-order','record','site_photo',3,1,'internal','{}','fixture-a',$1,$1)").bind(now).execute(p.pool().raw()).await.unwrap();
        let request=||WorkAuthorizationRequest{permission:"records.read".into(),module:Some("field_service".into()),destination:"field-service".into(),api:true};
        let reserved=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|Box::pin(async move{let input=FieldFileReservation{filename:"photo.png".into(),media_type:"image/png".into(),bytes:24,caption:None,supersedes_link_id:None};reserve_field_file(c,&Authority,&Quota,actor,&HoldingId::new("h001").unwrap(),"gc-set",&input,"gc-upload",now).await})).await.unwrap();
        let upload=reserved.upload_intent_id.clone();let id=upload.clone();let body=b"\x89PNGabcdefgh\0\0\0\0IEND\xaeB`\x82".to_vec();
        let prep=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|{let id=id.clone();let body=body.clone();Box::pin(async move{prepare_field_upload(c,&Authority,actor,&id,"image/png",body,now).await})}).await.unwrap();
        let (publication,pin)=match prep{FieldUploadPreparation::Pending{publication,pin}=>(publication,pin),_=>panic!("expected pending")};let key=publication.key.clone();
        let root=tempfile::Builder::new().permissions(std::fs::Permissions::from_mode(0o700)).tempdir().unwrap();let store=LocalFieldQuarantineWriter::open(root.path(),2).unwrap();let receipt=publish_pinned_field_upload(&store,p.pool().raw(),&pin,publication,p.review.system.scenario.clock.as_ref()).await.unwrap();assert!(receipt.created());
        assert_eq!(reconcile_field_upload_object(&store,&CleanupFixture(p.pool().raw().clone()),&upload,p.review.system.scenario.clock.clone()).await.unwrap(),FieldCleanupOutcome::NotEligible);
        let pin=Arc::new(pin);p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,_|{let pin=pin.clone();Box::pin(async move{release_field_publication_pin(c,&pin,now).await})}).await.unwrap();
        let gc_now=reserved.expires_at+1;p.review.system.scenario.clock.0.store(gc_now,Ordering::SeqCst);
        use sha2::{Digest,Sha256};let folder=root.path().join(hex::encode(Sha256::digest(key.as_bytes())));
        let manifest:serde_json::Value=serde_json::from_slice(&std::fs::read(folder.join("current.json")).unwrap()).unwrap();
        let version=manifest["version"].as_str().unwrap();let digest=manifest["etag"].as_str().unwrap();let size=manifest["size"].as_i64().unwrap();
        sqlx::query("INSERT INTO field_upload_object_cleanup_claims(company_id,object_key,upload_intent_id,token,claimed_at,claim_until,object_version,checksum_sha256,bytes) VALUES('company',$1,$2,$3,$4,$5,$6,$7,$8)").bind(&key).bind(&upload).bind(uuid::Uuid::new_v4()).bind(gc_now-100).bind(gc_now-1).bind(version).bind(digest).bind(size).execute(p.pool().raw()).await.unwrap();
        // Model a crash at the durable pointer-unlinked boundary; the staged
        // DB identity must let the next actual reconciler remove only this body.
        std::fs::remove_file(folder.join("current.json")).unwrap();assert!(folder.join(version).exists());
        assert_eq!(reconcile_field_upload_object(&store,&CleanupFixture(p.pool().raw().clone()),&upload,p.review.system.scenario.clock.clone()).await.unwrap(),FieldCleanupOutcome::Cleaned);
        assert!(!folder.join("current.json").exists());assert!(!folder.join(version).exists());
        let facts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM upload_intents WHERE id=$1),(SELECT count(*) FROM field_upload_publication_pins WHERE upload_intent_id=$1),(SELECT count(*) FROM field_upload_object_cleanup_claims WHERE upload_intent_id=$1)").bind(&upload).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(facts,(1,1,0));store.stop();store.drain().await;
    }).await;
}

#[tokio::test]
async fn late_link_mutation_rolls_back_completion_and_keeps_pin_for_recovery() {
    use erpx_server_files::public::field_visit_upload::{
        FieldUploadPreparation, LocalFieldQuarantineWriter, complete_field_upload,
        prepare_field_upload, publish_pinned_field_upload,
    };
    use std::{os::unix::fs::PermissionsExt, sync::Arc};
    facts::account::with_system(|system|async move{
        let p=facts::setup_workspace(system).await;let now=p.review.system.scenario.clock.now().get();
        sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES('tamper-set','company','branch','field_service_visit','issue-order','record','site_photo',3,1,'internal','{}','fixture-a',$1,$1)").bind(now).execute(p.pool().raw()).await.unwrap();
        let request=||WorkAuthorizationRequest{permission:"records.read".into(),module:Some("field_service".into()),destination:"field-service".into(),api:true};
        let reserved=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|Box::pin(async move{let input=FieldFileReservation{filename:"photo.png".into(),media_type:"image/png".into(),bytes:24,caption:None,supersedes_link_id:None};reserve_field_file(c,&Authority,&Quota,actor,&HoldingId::new("h001").unwrap(),"tamper-set",&input,"tamper-upload",now).await})).await.unwrap();
        let id=reserved.upload_intent_id.clone();let prep=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|{let id=id.clone();Box::pin(async move{prepare_field_upload(c,&Authority,actor,&id,"image/png",b"\x89PNGabcdefgh\0\0\0\0IEND\xaeB`\x82".to_vec(),now).await})}).await.unwrap();
        let (publication,pin)=match prep{FieldUploadPreparation::Pending{publication,pin}=>(publication,pin),_=>panic!("expected pending")};let root=tempfile::Builder::new().permissions(std::fs::Permissions::from_mode(0o700)).tempdir().unwrap();let store=LocalFieldQuarantineWriter::open(root.path(),2).unwrap();let receipt=Arc::new(publish_pinned_field_upload(&store,p.pool().raw(),&pin,publication,p.review.system.scenario.clock.as_ref()).await.unwrap());let pin=Arc::new(pin);
        sqlx::raw_sql("CREATE FUNCTION field_upload_test_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE attachment_links SET caption='tampered' WHERE id=NEW.link_id; RETURN NEW; END $$; CREATE TRIGGER field_upload_test_tamper AFTER INSERT ON attachment_events FOR EACH ROW WHEN (NEW.action='file.quarantined') EXECUTE FUNCTION field_upload_test_tamper()").execute(p.pool().raw()).await.unwrap();
        let upload=reserved.upload_intent_id.clone();let clock=p.review.system.scenario.clock.clone();let result=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),request(),p.review.configuration.clone(),move|c,actor|{let upload=upload.clone();let receipt=receipt.clone();let pin=pin.clone();let clock=clock.clone();Box::pin(async move{complete_field_upload(c,&Authority,actor,&upload,&receipt,&pin,clock.as_ref()).await})}).await;
        assert!(result.is_err());let values:(i64,i64,i64,Option<String>)=sqlx::query_as("SELECT (SELECT count(*) FROM attachments WHERE upload_id=$1),(SELECT count(*) FROM attachment_scan_attempts WHERE attachment_id='attachment-'||$1),(SELECT count(*) FROM field_upload_publication_pins WHERE upload_intent_id=$1 AND state='active'),(SELECT caption FROM attachment_links WHERE id=$2)").bind(&reserved.upload_intent_id).bind(&reserved.attachment_link_id).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(values,(0,0,1,None));store.stop();store.drain().await;
    }).await;
}

#[tokio::test]
async fn attachment_company_scope_uses_selected_actor_and_rejects_unconfigured_company() {
    use erpx_service::field_service_parts::FieldServicePartsEnvironment;
    use erpx_service::field_service_reservation_read::{FieldServiceReservationReadConfiguration,FieldServiceReservationReadService};
    use std::{collections::BTreeSet,sync::Arc};
    facts::account::with_system(|system| async move {
        let p=facts::setup_workspace(system).await;
        let now=p.review.system.scenario.clock.now().get();
        sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('scope-set','company','branch','field_service_visit','issue-order','record','technician_report',1,1,'confidential','{}','draft','fixture-a',$1,$1)").bind(now).execute(p.pool().raw()).await.unwrap();
        sqlx::query("INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at) VALUES('scope-link','scope-set','draft','fixture-a',$1,$2)").bind(now).bind(now+3600000).execute(p.pool().raw()).await.unwrap();
        let config=||FieldServiceReservationReadConfiguration::new(FieldServicePartsEnvironment::Development,HoldingId::new("h001").unwrap(),CompanyId::new("other").unwrap(),Arc::new(erpx_server_commercial::public::purchases::current_package_catalogue().unwrap())).unwrap().with_attachments(false);
        let enabled=FieldServiceReservationReadService::new(p.review.authorizer.clone(),config().with_field_attachment_companies(BTreeSet::from([CompanyId::new("company").unwrap()])),p.review.system.scenario.clock.clone());
        let selection=p.selection.clone();
        let response=enabled.attachment_status("acme.example.test",p.review.maker.clone(),move||Ok(selection),"scope-link".into()).await;
        assert!(response.is_ok(),"configured selected Company should reach its retained Field record");
        let denied=FieldServiceReservationReadService::new(p.review.authorizer.clone(),config().with_field_attachment_companies(BTreeSet::from([CompanyId::new("other").unwrap()])),p.review.system.scenario.clock.clone());
        let selection=p.selection.clone();
        assert!(denied.attachment_status("acme.example.test",p.review.maker.clone(),move||Ok(selection),"scope-link".into()).await.is_err());
        let fixed=FieldServiceReservationReadService::new(p.review.authorizer.clone(),config(),p.review.system.scenario.clock.clone());
        let selection=p.selection.clone();
        assert!(fixed.attachment_status("acme.example.test",p.review.maker.clone(),move||Ok(selection),"scope-link".into()).await.is_err());
    }).await;
}

#[tokio::test]
async fn field_draft_expiry_retains_history_and_is_idempotent() {
    use erpx_server_files::public::field_visit_upload::{expire_field_drafts,next_field_cleanup_candidate};
    facts::account::with_system(|system| async move {
        let p=facts::setup_workspace(system).await;let now=p.review.system.scenario.clock.now().get();
        sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('expire-set','company','branch','field_service_visit','issue-order','record','site_photo',3,1,'internal','{}','draft','fixture-a',$1,$1)").bind(now).execute(p.pool().raw()).await.unwrap();
        let reserved=p.review.authorizer.execute_with_sources("acme.example.test",p.review.maker.clone(),p.selection.clone(),WorkAuthorizationRequest{permission:"records.read".into(),module:Some("field_service".into()),destination:"field-service".into(),api:true},p.review.configuration.clone(),move|c,actor|Box::pin(async move{let input=FieldFileReservation{filename:"photo.png".into(),media_type:"image/png".into(),bytes:24,caption:None,supersedes_link_id:None};reserve_field_file(c,&Authority,&Quota,actor,&HoldingId::new("h001").unwrap(),"expire-set",&input,"expiry-key",now).await})).await.unwrap();
        p.review.system.scenario.clock.0.store(reserved.expires_at+1,std::sync::atomic::Ordering::SeqCst);
        let company=CompanyId::new("company").unwrap();let clock=p.review.system.scenario.clock.as_ref();
        for expected in [3,0] { let mut tx=p.pool().raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();assert_eq!(expire_field_drafts(&mut tx,&company,clock).await.unwrap(),expected);tx.commit().await.unwrap(); }
        let state:(String,String,String,i64,i64)=sqlx::query_as("SELECT s.state,l.state,u.status,(SELECT count(*) FROM attachment_events WHERE set_id=s.id AND action='link.expired'),(SELECT count(*) FROM attachment_events WHERE set_id=s.id AND action='set.expired') FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id JOIN upload_intents u ON u.attachment_link_id=l.id WHERE s.id='expire-set'").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(state,("expired".into(),"expired".into(),"expired".into(),1,1));
        let mut tx=p.pool().raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();assert_eq!(next_field_cleanup_candidate(&mut tx,&company,None,clock.now().get()).await.unwrap(),Some(reserved.upload_intent_id));tx.commit().await.unwrap();
    }).await;
}
