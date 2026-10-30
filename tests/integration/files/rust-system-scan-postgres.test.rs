#[allow(
    dead_code,
    reason = "shared real source fixture serves integration and native provider branches"
)]
#[path = "../../support/files/system-scan-facts.rs"]
mod system;
use erpx_server_files::public::attachment_scan::{AttachmentScanScope, AttachmentScanTransactions};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{CompanyId, HoldingId};
use erpx_service::files_system_scan::*;
use std::collections::BTreeMap;
use system::{clock, config, facts, fixture, lease, missing, state};
#[tokio::test]
async fn actual_leased_registry_schema_and_system_source_claim_require_no_person_or_saved_parent() {
    fixture::account::with_system(|s| async move {
        let id = facts::seed_managed(&s.holdings[0], "assembled").await;
        let worker = assemble_development_files_scan(config(&s, "company-owned").await, clock())
            .await
            .unwrap();
        let (control, holding) = worker.schema_observations();
        assert_eq!(control.len(), 64);
        assert_eq!(holding.len(), 64);
        assert_eq!(worker.files_binding(), "HOLDING_H001_FILES");
        let prepared = worker
            .claim_and_prepare(Some(id.as_str()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(prepared.claim().scope(), worker.scope());
        assert_eq!(prepared.claim().attempt(), 1);
        assert_eq!(prepared.claim().claim_until(), 61_000);
        assert_eq!(
            state(&s, id.as_str()).await,
            ("scanning".into(), "running".into(), 1, 0)
        );
        worker.fail_current(prepared, missing()).await.unwrap();
        assert_eq!(
            state(&s, id.as_str()).await,
            ("scan_error".into(), "error".into(), 2, 1)
        );
        worker.close().await;
    })
    .await;
}
#[tokio::test]
async fn startup_rejects_missing_configured_slots_and_same_control_holding_database_without_adopting_labels()
 {
    fixture::account::with_system(|s| async move {
        let registry = fixture::account::fixture::account::holding_registry();
        let scope = AttachmentScanScope {
            holding_id: HoldingId::new("h001").unwrap(),
            company_id: CompanyId::new("company-owned").unwrap(),
        };
        assert!(
            DevelopmentFilesScanConfiguration::new(
                lease(&s.control).await,
                BTreeMap::new(),
                registry.clone(),
                scope.clone()
            )
            .is_err()
        );
        let same = DevelopmentFilesScanConfiguration::new(
            lease(&s.control).await,
            BTreeMap::from([
                ("HOLDING_H001_DB".into(), lease(&s.control).await),
                ("HOLDING_H002_DB".into(), lease(&s.holdings[1]).await),
            ]),
            registry,
            scope,
        )
        .unwrap();
        assert!(
            assemble_development_files_scan(same, clock())
                .await
                .is_err()
        );
    })
    .await;
}
#[tokio::test]
async fn each_phase_reacquires_live_binding_and_no_control_or_holding_transaction_survives_prepare()
{
    fixture::account::with_system(|s|async move{
  let id=facts::seed_managed(&s.holdings[0],"binding").await;let worker=assemble_development_files_scan(config(&s,"company-owned").await,clock()).await.unwrap();
  sqlx::query("UPDATE holding_bindings SET status='suspended'WHERE holding_id='h001'").execute(s.control.raw()).await.unwrap();
  assert!(worker.claim_and_prepare(Some(id.as_str())).await.is_err());assert_eq!(state(&s,id.as_str()).await,("quarantined".into(),"pending".into(),1,0));
  sqlx::query("UPDATE holding_bindings SET status='active'WHERE holding_id='h001'").execute(s.control.raw()).await.unwrap();
  let p=worker.claim_and_prepare(Some(id.as_str())).await.unwrap().unwrap();
  for pool in [&s.control,&s.holdings[0]] {let n:i64=sqlx::query_scalar("SELECT count(*)FROM pg_stat_activity WHERE datname=current_database()AND pid<>pg_backend_pid()AND xact_start IS NOT NULL").fetch_one(pool.raw()).await.unwrap();assert_eq!(n,0);}
  // This real writer must commit while computation may hold only owned data.
  tokio::time::timeout(std::time::Duration::from_secs(2),sqlx::query("UPDATE holding_bindings SET files_binding='HOLDING_H001_FILES_CHANGED'WHERE holding_id='h001'").execute(s.control.raw())).await.unwrap().unwrap();
  assert!(worker.fail_current(p,missing()).await.is_err());assert_eq!(state(&s,id.as_str()).await,("scanning".into(),"running".into(),1,0));worker.close().await;
 }).await;
}
#[tokio::test]
async fn stale_reclaimed_token_and_cross_company_prepared_data_never_settle_any_effect() {
    fixture::account::with_system(|s| async move {
        let id = facts::seed_managed(&s.holdings[0], "reclaim").await;
        let time = clock();
        let worker =
            assemble_development_files_scan(config(&s, "company-owned").await, time.clone())
                .await
                .unwrap();
        let p = worker
            .claim_and_prepare(Some(id.as_str()))
            .await
            .unwrap()
            .unwrap();
        time.set(61_001);
        let current = worker
            .claim_and_prepare(Some(id.as_str()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(current.claim().attempt(), 1);
        assert!(worker.fail_current(p, missing()).await.is_err());
        assert_eq!(
            state(&s, id.as_str()).await,
            ("scanning".into(), "running".into(), 1, 0)
        );
        let foreign =
            assemble_development_files_scan(config(&s, "other-company").await, time.clone())
                .await
                .unwrap();
        assert!(
            foreign
                .claim_and_prepare(Some(id.as_str()))
                .await
                .unwrap()
                .is_none()
        );
        assert!(foreign.fail_current(current, missing()).await.is_err());
        assert_eq!(
            state(&s, id.as_str()).await,
            ("scanning".into(), "running".into(), 1, 0)
        );
        foreign.close().await;
        worker.close().await;
    })
    .await;
}
#[tokio::test]
async fn actual_retention_40001_retries_whole_holding_phase_without_duplicate_events_or_attempts() {
    fixture::account::with_system(|s|async move{
  let id=facts::seed_managed(&s.holdings[0],"retry-whole").await;let worker=assemble_development_files_scan(config(&s,"company-owned").await,clock()).await.unwrap();let p=worker.claim_and_prepare(Some(id.as_str())).await.unwrap().unwrap();
  sqlx::raw_sql("CREATE SEQUENCE owned_files_retry;CREATE FUNCTION owned_files_serialization()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.attachment_id='attachment-upload-retry-whole'AND NEW.action='scan.error'AND nextval('owned_files_retry')=1 THEN RAISE EXCEPTION 'owned serialization proof'USING ERRCODE='40001';END IF;RETURN NEW;END$$;CREATE TRIGGER owned_files_serialization BEFORE INSERT ON attachment_events FOR EACH ROW EXECUTE FUNCTION owned_files_serialization();").execute(s.holdings[0].raw()).await.unwrap();
  worker.fail_current(p,missing()).await.unwrap();assert_eq!(state(&s,id.as_str()).await,("scan_error".into(),"error".into(),2,1));
  let attempts:i64=sqlx::query_scalar("SELECT last_value FROM owned_files_retry").fetch_one(s.holdings[0].raw()).await.unwrap();assert_eq!(attempts,2);worker.close().await;
 }).await;
}
#[tokio::test]
async fn actual_deferred_after_image_mutation_rolls_back_the_whole_settlement_before_holding_commit()
 {
    fixture::account::with_system(|s|async move{
  let id=facts::seed_managed(&s.holdings[0],"deferred-service").await;let worker=assemble_development_files_scan(config(&s,"company-owned").await,clock()).await.unwrap();let p=worker.claim_and_prepare(Some(id.as_str())).await.unwrap().unwrap();
  sqlx::raw_sql("CREATE FUNCTION owned_files_late_mutation()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.attachment_id='attachment-upload-deferred-service'AND NEW.action='scan.error'THEN UPDATE upload_intents SET updated_at=updated_at+1 WHERE id='upload-deferred-service';END IF;RETURN NULL;END$$;CREATE CONSTRAINT TRIGGER owned_files_late_mutation AFTER INSERT ON attachment_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION owned_files_late_mutation();").execute(s.holdings[0].raw()).await.unwrap();
  let error=worker.fail_current(p,missing()).await;assert!(matches!(error,Err(PgTransactionError::Business(ref e))if e.code=="ATTACHMENT_SCAN_SOURCE_CHANGED"));assert_eq!(state(&s,id.as_str()).await,("scanning".into(),"running".into(),1,0));worker.close().await;
 }).await;
}

#[tokio::test]
async fn field_candidate_cursor_survives_worker_restart_and_skips_bad_sources_without_verdicts() {
 fixture::account::with_system(|s| async move {
  let first=facts::seed_source_with_receipt(&s.holdings[0],"cursor-a",facts::TestOrigin::ManagedField,"text/csv",facts::bytes(),false).await;
  let second=facts::seed_source_with_receipt(&s.holdings[0],"cursor-b",facts::TestOrigin::ManagedField,"text/csv",facts::bytes(),false).await;
  let valid=facts::seed_source(&s.holdings[0],"cursor-c",facts::TestOrigin::ManagedField,"text/csv",facts::bytes()).await;
  let worker=assemble_development_files_scan(config(&s,"company-owned").await,clock()).await.unwrap().field_only();
  assert_eq!(worker.next_field_candidate().await.unwrap().unwrap(),first);
  assert!(worker.claim_and_prepare(Some(first.as_str())).await.is_err());
  assert_eq!(worker.next_field_candidate().await.unwrap().unwrap(),second);
  assert!(worker.claim_and_prepare(Some(second.as_str())).await.is_err());
  worker.close().await;
  let worker=assemble_development_files_scan(config(&s,"company-owned").await,clock()).await.unwrap().field_only();
  assert_eq!(worker.next_field_candidate().await.unwrap().unwrap(),valid);
  assert!(worker.claim_and_prepare(Some(valid.as_str())).await.unwrap().is_some());
  for bad in [&first,&second] {assert_eq!(state(&s,bad.as_str()).await,("quarantined".into(),"pending".into(),1,0));}
  worker.close().await;
 }).await;
}

#[tokio::test]
async fn cleanup_phase_uses_current_control_binding_and_exact_configured_company() {
 fixture::account::with_system(|s|async move {
  sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id,status) VALUES('company-owned','company','cleanup-company','Cleanup Company','company-owned','active') ON CONFLICT(id) DO UPDATE SET status='active'").execute(s.holdings[0].raw()).await.unwrap();
  let worker=assemble_development_files_scan(config(&s,"company-owned").await,clock()).await.unwrap();
  let selected=worker.run_field_cleanup_phase(|c,scope| Box::pin(async move {
   sqlx::query("INSERT INTO field_file_scan_cursor(company_id,after_created_at,after_job_id) VALUES($1,1,'cleanup-proof')").bind(scope.company_id.as_str()).execute(c).await?;
   Ok(scope.company_id.as_str().to_owned())
  })).await.unwrap();assert_eq!(selected,"company-owned");
  sqlx::query("UPDATE holding_bindings SET status='suspended' WHERE holding_id='h001'").execute(s.control.raw()).await.unwrap();
  let result=worker.run_field_cleanup_phase(|c,scope| Box::pin(async move {
   sqlx::query("UPDATE field_file_scan_cursor SET after_job_id='must-not-write' WHERE company_id=$1").bind(scope.company_id.as_str()).execute(c).await?;Ok(())
  })).await;
  assert!(result.is_err());let value:String=sqlx::query_scalar("SELECT after_job_id FROM field_file_scan_cursor WHERE company_id='company-owned'").fetch_one(s.holdings[0].raw()).await.unwrap();assert_eq!(value,"cleanup-proof");worker.close().await;
 }).await;
}

#[path = "../../support/files/field-cleanup-composition-tests.rs"]
mod field_cleanup_composition_tests;
