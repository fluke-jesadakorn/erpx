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

// The actual TS function sends its fixed SQL to this same transaction through
// an in-test adapter; it never substitutes a fabricated marker/scoped value.
async fn ts_link_route(c:&mut sqlx::PgConnection,company:&str,link:&str)->serde_json::Value {
    use std::io::{BufRead,BufReader,Write};use std::process::Stdio;
    let script=r#"import {createInterface} from 'node:readline';if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);const lines=createInterface({input:process.stdin})[Symbol.asyncIterator]();const first=await lines.next();const [company,id]=JSON.parse(first.value);const db={prepare(sql){return {bind(...args){return {async first(){process.stdout.write(JSON.stringify({sql,args})+'\n');const r=await lines.next();return JSON.parse(r.value)}}}}}};try{const route=await m.fieldFileWriterOwnerInDatabase(db,{companyId:company},{kind:'link',id});process.stdout.write(JSON.stringify({route})+'\n')}catch(e){process.stdout.write(JSON.stringify({error:e.code,status:e.status})+'\n')}"#;
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");let module=root.join("packages/server-files/src/adapters/field-file-writer-owner.ts");
    let mut child=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(root).args(["--conditions=react-server","-e",script]).arg(module).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let mut input=child.stdin.take().unwrap();let mut output=BufReader::new(child.stdout.take().unwrap());writeln!(input,"{}",serde_json::json!([company,link])).unwrap();input.flush().unwrap();
    let result=loop {let mut line=String::new();assert!(output.read_line(&mut line).unwrap()>0,"TS routing oracle ended early");let value:serde_json::Value=serde_json::from_str(&line).unwrap();if let Some(sql)=value.get("sql").and_then(serde_json::Value::as_str){let args=value["args"].as_array().unwrap();let mut index=0;let mut adapted=String::new();for ch in sql.chars(){if ch=='?'{index+=1;adapted.push('$');adapted.push_str(&index.to_string());}else{adapted.push(ch);}}assert_eq!(index,args.len());let mut query=sqlx::query_as::<_,(bool,bool)>(sqlx::AssertSqlSafe(adapted));for arg in args{query=query.bind(arg.as_str().expect("routing SQL binds are strings").to_owned());}let (marker,scoped)=query.fetch_one(&mut *c).await.unwrap();writeln!(input,"{}",serde_json::json!({"marker":marker,"scoped":scoped})).unwrap();input.flush().unwrap();}else{break value}};
    drop(input);assert!(child.wait().unwrap().success(),"TS routing oracle failed");result
}

#[tokio::test]
async fn global_field_attachment_marker_matches_installed_1093_1097() {
    use erpx_server_files::public::field_attachment_routing::{inspect_field_attachment_routing,lock_field_attachment_routing_sources,observe_global_attachment_ancestry};
    use erpx_server_finance::public::field_attachment_routing::{lock_field_attachment_finance_routing,inspect_global_field_payment_finance_sources,inspect_global_field_journal_finance_source};
    use erpx_server_inventory::public::field_history_consumption::{lock_field_history_consumption_join_sources,inspect_field_history_consumption_join};
    use erpx_server_workflow::public::record_origin::{lock_field_history_receipt_join_sources,inspect_field_history_receipt_join};
    use erpx_service::field_attachment_global_marker::FieldAttachmentMarkerSources;
    use erpx_service::field_attachment_link_routing::{validate_field_link_selector,FieldLinkRoute,FieldLinkRoutingSources};
    use erpx_server_files::public::field_attachment_routing::{observe_unscoped_attachment_link,observe_unscoped_global_attachment_ancestry};
    for id in ["","bad\0id"] {let error=validate_field_link_selector(id).unwrap_err();assert_eq!(error.code,"ATTACHMENT_NOT_FOUND");assert_eq!(error.kind,erpx_server_shared::public::FailureKind::NotFound);}
    fixture::account::with_system(|system|async move {
        let db=&system.holdings[0];let attachment=facts::seed_managed(db,"global-marker").await;let attachment_id=attachment.as_str().to_owned();
        let mut tx=db.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();
        lock_field_history_receipt_join_sources(&mut tx).await.unwrap();lock_field_attachment_finance_routing(&mut tx).await.unwrap();lock_field_history_consumption_join_sources(&mut tx).await.unwrap();lock_field_attachment_routing_sources(&mut tx).await.unwrap();
        let workflow=inspect_field_history_receipt_join(&mut tx).await.unwrap();let payments=inspect_global_field_payment_finance_sources(&mut tx).await.unwrap();let journals=inspect_global_field_journal_finance_source(&mut tx).await.unwrap();let inventory=inspect_field_history_consumption_join(&mut tx).await.unwrap();let company=CompanyId::new("company-owned").unwrap();
        assert!(inspect_field_attachment_routing(&mut tx,&company,"missing").await.unwrap().is_none());assert!(inspect_field_attachment_routing(&mut tx,&CompanyId::new("foreign").unwrap(),"link-global-marker").await.unwrap().is_none());
        async fn check(c:&mut sqlx::PgConnection,company:&CompanyId,link:&str,attachment:Option<&str>,workflow:&erpx_server_workflow::public::record_origin::CurrentFieldHistoryReceiptJoin,payments:&erpx_server_finance::public::field_attachment_routing::CurrentGlobalFieldPaymentFinanceSources,journals:&erpx_server_finance::public::field_attachment_routing::CurrentGlobalFieldJournalFinanceSource,inventory:&erpx_server_inventory::public::field_history_consumption::CurrentFieldHistoryConsumptionJoin)->bool{
            let file=inspect_field_attachment_routing(&mut *c,company,link).await.unwrap().unwrap();let ancestry=observe_global_attachment_ancestry(&mut *c,&file).await.unwrap();let sources=FieldAttachmentMarkerSources{files:&ancestry,workflow,payments,journals,inventory};let observed=sources.inspect(&mut *c).await.unwrap();let actual:bool=sqlx::query_scalar("SELECT public.erpx_files_field_attachment_owned($1)").bind(attachment).fetch_one(&mut *c).await.unwrap();assert_eq!(observed.is_field_marker(),actual);observed.require_current(c).await.unwrap();actual
        }
        async fn compare_link(c:&mut sqlx::PgConnection,company:&CompanyId,link:&str,workflow:&erpx_server_workflow::public::record_origin::CurrentFieldHistoryReceiptJoin,payments:&erpx_server_finance::public::field_attachment_routing::CurrentGlobalFieldPaymentFinanceSources,journals:&erpx_server_finance::public::field_attachment_routing::CurrentGlobalFieldJournalFinanceSource,inventory:&erpx_server_inventory::public::field_history_consumption::CurrentFieldHistoryConsumptionJoin)->serde_json::Value{
            let expected=ts_link_route(&mut *c,company.as_str(),link).await;
            let selector=observe_unscoped_attachment_link(&mut *c,company,link).await.unwrap();let ancestry=observe_unscoped_global_attachment_ancestry(&mut *c,&selector).await.unwrap();let sources=FieldLinkRoutingSources{files:&ancestry,workflow,payments,journals,inventory};
            let actual=match sources.inspect(&mut *c).await {Ok(value)=>{let route=match value.route(){FieldLinkRoute::Field=>"field",FieldLinkRoute::Other=>"other"};value.require_current(&mut *c).await.unwrap();serde_json::json!({"route":route})},Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e))=>serde_json::json!({"error":e.code,"status":match e.kind{erpx_server_shared::public::FailureKind::NotFound=>404,_=>panic!("unexpected link routing error")}}),Err(e)=>panic!("link routing persistence error: {e:?}")};assert_eq!(actual,expected,"link={link}");actual
        }
        for id in ["","bad\0id"] {let expected=ts_link_route(&mut tx,company.as_str(),id).await;assert_eq!(expected,serde_json::json!({"error":"ATTACHMENT_NOT_FOUND","status":404}));}
        assert_eq!(compare_link(&mut tx,&company,"link-global-marker",&workflow,&payments,&journals,&inventory).await["route"],"other");
        assert_eq!(compare_link(&mut tx,&company,"missing",&workflow,&payments,&journals,&inventory).await["route"],"other");
        assert!(!check(&mut tx,&company,"link-global-marker",Some(&attachment_id),&workflow,&payments,&journals,&inventory).await);
        sqlx::query("INSERT INTO public.attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES('global-null','set-global-marker',NULL,'draft','fixture',0,1)").execute(&mut *tx).await.unwrap();assert!(!check(&mut tx,&company,"global-null",None,&workflow,&payments,&journals,&inventory).await);assert_eq!(compare_link(&mut tx,&company,"global-null",&workflow,&payments,&journals,&inventory).await["route"],"other");
        // Hold the absent decision, then add an actual related foreign link:
        // the full marker must change although the selected Company stays put.
        let file=inspect_field_attachment_routing(&mut tx,&company,"link-global-marker").await.unwrap().unwrap();let ancestry=observe_global_attachment_ancestry(&mut tx,&file).await.unwrap();let sources=FieldAttachmentMarkerSources{files:&ancestry,workflow:&workflow,payments:&payments,journals:&journals,inventory:&inventory};let absent=sources.inspect(&mut tx).await.unwrap();assert!(!absent.is_field_marker());
        let link_selection=observe_unscoped_attachment_link(&mut tx,&company,"link-global-marker").await.unwrap();let link_ancestry=observe_unscoped_global_attachment_ancestry(&mut tx,&link_selection).await.unwrap();let link_sources=FieldLinkRoutingSources{files:&link_ancestry,workflow:&workflow,payments:&payments,journals:&journals,inventory:&inventory};let retained_link=link_sources.inspect(&mut tx).await.unwrap();assert_eq!(retained_link.route(),FieldLinkRoute::Other);
        sqlx::query("SAVEPOINT visit_marker").execute(&mut *tx).await.unwrap();sqlx::raw_sql("INSERT INTO public.attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('foreign-visit-set','foreign','field_service_visit','visit','record','receipt',1,1,'confidential','{}','draft','fixture',0,0); INSERT INTO public.attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES('foreign-visit-link','foreign-visit-set','attachment-upload-global-marker','draft','fixture',0,1)").execute(&mut *tx).await.unwrap();assert!(absent.require_current(&mut tx).await.is_err());assert!(retained_link.require_current(&mut tx).await.is_err());assert!(check(&mut tx,&company,"link-global-marker",Some(&attachment_id),&workflow,&payments,&journals,&inventory).await);assert_eq!(compare_link(&mut tx,&company,"link-global-marker",&workflow,&payments,&journals,&inventory).await["route"],"field");assert_eq!(compare_link(&mut tx,&company,"foreign-visit-link",&workflow,&payments,&journals,&inventory).await["error"],"ATTACHMENT_NOT_FOUND");
        // A retained Field decision loses the selected Company's link scope.
        // Draft link reassignment is allowed by the installed Files guard.
        let selected_field=observe_unscoped_attachment_link(&mut tx,&company,"link-global-marker").await.unwrap();let field_ancestry=observe_unscoped_global_attachment_ancestry(&mut tx,&selected_field).await.unwrap();let field_sources=FieldLinkRoutingSources{files:&field_ancestry,workflow:&workflow,payments:&payments,journals:&journals,inventory:&inventory};let retained_field=field_sources.inspect(&mut tx).await.unwrap();assert_eq!(retained_field.route(),FieldLinkRoute::Field);
        sqlx::query("SAVEPOINT selected_scope_drift").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO public.attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('foreign-rehomed-set','foreign','expense_claim','scope-change','record','receipt',1,1,'confidential','{}','draft','fixture',0,0)").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.attachment_links SET set_id='foreign-rehomed-set' WHERE id='link-global-marker'").execute(&mut *tx).await.unwrap();
        assert_eq!(compare_link(&mut tx,&company,"link-global-marker",&workflow,&payments,&journals,&inventory).await,serde_json::json!({"error":"ATTACHMENT_NOT_FOUND","status":404}));
        let error=retained_field.require_current(&mut tx).await.err().expect("scope drift must conflict");assert!(matches!(error,erpx_server_runtime::public::postgres::PgTransactionError::Business(ref e) if e.kind==erpx_server_shared::public::FailureKind::Conflict && e.code=="FILES_ATTACHMENT_ROUTING_CHANGED"));
        sqlx::query("ROLLBACK TO SAVEPOINT selected_scope_drift").execute(&mut *tx).await.unwrap();retained_field.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT visit_marker").execute(&mut *tx).await.unwrap();absent.require_current(&mut tx).await.unwrap();retained_link.require_current(&mut tx).await.unwrap();
        // Real installed 1092/1097 branches below are synthetic rollback-only
        // sources, not claims of valid Field business lineage or actor access.
        sqlx::raw_sql("INSERT INTO public.organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('foreign','company','marker-foreign','Foreign Company',NULL,'foreign',NULL,'active'),('branch','branch','marker-branch','Foreign Branch','foreign','foreign','branch','active'); INSERT INTO public.finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('marker-chart','foreign','Marker','THB','fixture',0,0); INSERT INTO public.finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES('marker-period','foreign','Period','2026-01-01','2026-12-31',0); INSERT INTO public.finance_accounts(id,chart_id,company_id,code,name,category,created_at,updated_at) VALUES('marker-debit','marker-chart','foreign','100','Debit','asset',0,0),('marker-credit','marker-chart','foreign','400','Credit','revenue',0,0); INSERT INTO public.finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('marker-party','foreign','customer','P','Party','THB',0,0); INSERT INTO public.finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,created_by,created_at,updated_at) VALUES('marker-invoice','foreign','marker-party','receivable','I','2026-01-01','2026-01-02','THB',1,0,1,1,'marker-debit','marker-credit','field_service','order','fixture',0,0); INSERT INTO public.finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,memo,status,created_by,created_at,updated_at) VALUES('field-journal','foreign','marker-chart','marker-period','JF','2026-01-01','THB','field_service_parts','order','Field','draft','fixture',0,0),('inventory-journal','foreign','marker-chart','marker-period','JI','2026-01-01','THB','maintenance_parts','order','Inventory','draft','fixture',0,0); INSERT INTO public.commerce_warehouses(id,company_id,branch_id,code,name) VALUES('marker-warehouse','foreign','branch','W','Warehouse'); INSERT INTO public.commerce_products(id,company_id,sku,name,price_minor) VALUES('marker-product','foreign','P','Product',1); INSERT INTO public.operational_inventory_consumptions(id,company_id,branch_id,source_kind,source_id,source_version,warehouse_id,product_id,quantity_milli,value_minor,currency,movement_date,journal_id,reason,created_by,created_at) VALUES('marker-consumption','foreign','branch','field_service','order',1,'marker-warehouse','marker-product',1,0,'THB','2026-01-01','inventory-journal','Fixture','fixture',0); INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES('marker-payment-receipt','finance.payment.create','fixture','fixture','foreign-payment','{\"settlementInvoiceId\":\"marker-invoice\"}',0)").execute(&mut *tx).await.unwrap();
        sqlx::raw_sql("INSERT INTO public.finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,status,created_by,created_at,updated_at) VALUES('allocated-payment','foreign','marker-party','receipt','P-1','2026-01-01','THB',1,1,'draft','fixture',0,0); INSERT INTO public.finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) VALUES('marker-allocation','foreign','allocated-payment','marker-invoice',1,'fixture',0)").execute(&mut *tx).await.unwrap();
        for (index,kind,id) in [(1,"finance_payment","foreign-payment"),(2,"finance_journal","field-journal"),(3,"finance_journal","inventory-journal"),(4,"finance_payment","allocated-payment")] {
            sqlx::query("SAVEPOINT related_marker").execute(&mut *tx).await.unwrap();let set=format!("related-set-{index}");let link=format!("related-link-{index}");sqlx::query("INSERT INTO public.attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES($1,'foreign',$2,$3,'record','receipt',1,1,'confidential','{}','draft','fixture',0,0)").bind(&set).bind(kind).bind(id).execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO public.attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES($1,$2,$3,'draft','fixture',0,1)").bind(&link).bind(&set).bind(&attachment_id).execute(&mut *tx).await.unwrap();assert!(check(&mut tx,&company,"link-global-marker",Some(&attachment_id),&workflow,&payments,&journals,&inventory).await);assert_eq!(compare_link(&mut tx,&company,"link-global-marker",&workflow,&payments,&journals,&inventory).await["route"],"field");assert_eq!(compare_link(&mut tx,&company,&link,&workflow,&payments,&journals,&inventory).await["error"],"ATTACHMENT_NOT_FOUND");assert_eq!(compare_link(&mut tx,&CompanyId::new("foreign").unwrap(),&link,&workflow,&payments,&journals,&inventory).await["route"],"field");sqlx::query("ROLLBACK TO SAVEPOINT related_marker").execute(&mut *tx).await.unwrap();assert!(!check(&mut tx,&company,"link-global-marker",Some(&attachment_id),&workflow,&payments,&journals,&inventory).await);
        }
        // Isolate all six owner-type aliases in actual Files ancestry. Each
        // fixture starts neutral except the one selected Field Visit path.
        for which in 0..6 {
            sqlx::query("SAVEPOINT alias_case").execute(&mut *tx).await.unwrap();
            let selected_set=format!("alias-selected-set-{which}");let source_set=format!("alias-source-set-{which}");let related_set=format!("alias-related-set-{which}");let related_upload_set=format!("alias-related-upload-set-{which}");let source_upload=format!("alias-source-upload-{which}");let related_upload=format!("alias-related-upload-{which}");let source_id=format!("alias-attachment-{which}");let selected_link=format!("alias-selected-link-{which}");let related_link=format!("alias-related-link-{which}");
            for (set,company_id,is_field) in [(&selected_set,"company-owned",false),(&source_set,"foreign",which==2),(&related_set,"foreign",which==3),(&related_upload_set,"foreign",which==5)] {
                let kind=if is_field{"field_service_visit"}else{"expense_claim"};let target=if is_field{format!("visit-{which}")}else{format!("neutral-{set}")};
                sqlx::query("INSERT INTO public.attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,'record','receipt',1,1,'confidential','{}','draft','fixture',0,0)").bind(set).bind(company_id).bind(kind).bind(target).execute(&mut *tx).await.unwrap();
            }
            for (upload,set,is_field) in [(&source_upload,&source_set,which==1),(&related_upload,&related_upload_set,which==4)] {
                let kind=if is_field{"field_service_visit"}else{"expense_claim"};let owner=if is_field{format!("visit-{which}")}else{format!("neutral-{upload}")};let key=format!("quarantine/{upload}");
                sqlx::query("INSERT INTO public.upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,company_id,attachment_set_id) VALUES($1,$2,$3,'receipt','fixture','text/plain',1,$4,'uploaded','fixture',1,0,0,'foreign',$5)").bind(upload).bind(kind).bind(owner).bind(key).bind(set).execute(&mut *tx).await.unwrap();
            }
            let attachment_kind=if which==0{"field_service_visit"}else{"expense_claim"};let attachment_owner=if which==0{format!("visit-{which}")}else{format!("neutral-{source_id}")};
            sqlx::query("INSERT INTO public.attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,source_state) VALUES($1,$2,$3,$4,'receipt',$5,1,'fixture','text/plain',1,$6,'fixture',0,'company-owned','quarantined')").bind(&source_id).bind(&source_upload).bind(attachment_kind).bind(attachment_owner).bind(format!("quarantine/{source_id}")).bind("a".repeat(64)).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO public.attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES($1,$2,$3,'draft','fixture',0,1)").bind(&selected_link).bind(&selected_set).bind(&source_id).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO public.attachment_links(id,set_id,attachment_id,upload_intent_id,state,created_by,created_at,expires_at) VALUES($1,$2,$3,$4,'draft','fixture',0,1)").bind(&related_link).bind(&related_set).bind(&source_id).bind(&related_upload).execute(&mut *tx).await.unwrap();
            let selected=inspect_field_attachment_routing(&mut tx,&company,&selected_link).await.unwrap().unwrap();let ancestry=observe_global_attachment_ancestry(&mut tx,&selected).await.unwrap();
            let sql=format!("WITH {} SELECT COALESCE(a.attachment_owner_type='field_service_visit',false),COALESCE(a.upload_owner_type='field_service_visit',false),COALESCE(a.upload_set_type='field_service_visit',false),COALESCE(l.set_type='field_service_visit',false),COALESCE(l.upload_owner_type='field_service_visit',false),COALESCE(l.upload_set_type='field_service_visit',false) FROM file_global_attachment_source a CROSS JOIN file_global_related_links l WHERE l.link_id=$3",ancestry.cte_sql());
            let flags:(bool,bool,bool,bool,bool,bool)=sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind(&selected_link).bind(company.as_str()).bind(&related_link).fetch_one(&mut *tx).await.unwrap();let flags=[flags.0,flags.1,flags.2,flags.3,flags.4,flags.5];for (i,flag) in flags.iter().enumerate(){assert_eq!(*flag,i==which,"alias {which} actual {i}");}
            assert!(check(&mut tx,&company,&selected_link,Some(&source_id),&workflow,&payments,&journals,&inventory).await);
            sqlx::query("ROLLBACK TO SAVEPOINT alias_case").execute(&mut *tx).await.unwrap();
        }
        // 1062's installed BEFORE INSERT trigger casts every receipt's
        // result_json to jsonb, so malformed text cannot be legitimately
        // persisted for either operation. Test that barrier without disabling
        // a trigger; post-insert marker SQLSTATE parity is therefore unproven.
        for (key,operation) in [("malformed-unrelated","fixture.unrelated"),("malformed-payment","finance.payment.create")] {
            sqlx::query("SAVEPOINT malformed_receipt").execute(&mut *tx).await.unwrap();
            let failure=sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES($1,$2,'fixture','fixture','unmatched','{',0)").bind(key).bind(operation).execute(&mut *tx).await.expect_err("installed guard must reject malformed JSON");
            assert_eq!(failure.as_database_error().and_then(|e|e.code()).as_deref(),Some("22P02"));
            sqlx::query("ROLLBACK TO SAVEPOINT malformed_receipt").execute(&mut *tx).await.unwrap();
            assert!(!check(&mut tx,&company,"link-global-marker",Some(&attachment_id),&workflow,&payments,&journals,&inventory).await);
        }
        tx.rollback().await.unwrap();
    }).await;
}
