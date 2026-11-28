#[path = "../../../platform/composition/src/procurement_rfq_target.rs"]
mod binding;
#[path = "../../../platform/composition/src/procurement_rfq_decision.rs"]
mod decision;
#[path = "../../../platform/composition/src/procurement_rfq_approval.rs"]
mod procurement_rfq_approval;
#[path = "../../../platform/composition/src/procurement_rfq_access.rs"]
mod request_access;
#[path = "../../../platform/composition/src/procurement_rfq_detail.rs"]
mod request_detail;
#[path = "../../../platform/composition/src/procurement_rfq_quantity.rs"]
mod procurement_rfq_quantity;
use request_access as procurement_rfq_access;
#[path = "../../../platform/composition/src/procurement_rfq_request.rs"]
mod procurement_rfq_request;
#[path = "../../../platform/composition/src/procurement_rfq_assertions.rs"]
mod procurement_rfq_assertions;
use procurement_rfq_assertions as rfq_assertions;
#[path = "../../../platform/composition/src/procurement_rfq_receipt.rs"]
mod rfq_receipt;
#[path = "../../../platform/composition/src/procurement_rfq_materialization.rs"]
mod procurement_rfq_materialization;
use procurement_rfq_materialization as rfq_materialization;
#[path = "../../support/finance/review-composition.rs"]
mod review;
use erpx_identity_access_server::public::effective_authority::{
    HoldingWorkSourcePreparation, WorkAuthorizationRequest,
};
use erpx_procurement_server::public::{
    rfq_target_source::{lock_rfq_target_sources, observe_rfq_target},
    rfq_transition::RfqTransitionAction,
    rfq_transition_normalization::normalize_rfq_transition,
};
use erpx_records_server::public::rfq_request_source::{
    lock_rfq_request_sources, observe_rfq_quotation_row, observe_rfq_request,
};
use erpx_platform_runtime_server::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_platform_shared_server::public::{BusinessError, Clock, FailureKind};
use erpx_workflow_server::public::source_ordered_json::SourceOrderedJson;
use sqlx::PgConnection;
use std::sync::Arc;
#[derive(Clone)]
struct TargetLocks;
impl HoldingWorkSourcePreparation for TargetLocks {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_rfq_target_sources(&mut *c).await?;
            lock_rfq_request_sources(&mut *c).await?;
            erpx_workflow_server::public::rfq_command_receipt::lock_rfq_receipts(c).await
        })
    }
}
#[tokio::test]
async fn actual_procurement_actor_is_checked_against_full_rfq_target() {
    let oracle=std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required")).current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).args(["--conditions=react-server","--import=tsx","-e","const {procurementCommandSourceImage}=require('./modules/procurement/server/src/adapters/command-source.ts');process.stdout.write(procurementCommandSourceImage('rfq','r'))"]).output().unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let ts_query = Arc::new(format!(
        "SELECT ({})::text FROM public.procurement_rfqs r WHERE r.id=$1",
        String::from_utf8(oracle.stdout).unwrap()
    ));
    review::account::with_system(move|system|async move {
        let mut p=review::setup_with_finance(system,false).await;let db=p.system.holdings[0].raw();
        sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_finance_review',2,'[\"commerce.read\",\"commerce.purchase\",\"module.procurement.read\"]','[]','[\"holding\",\"branch\"]','fixture',0); UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_finance_review'; UPDATE assignments SET modules_json='[\"procurement\"]',version=4,valid_from=0,valid_until=9000000000000 WHERE id='finance-fixture-a'; UPDATE active_work_contexts SET assignment_version=4 WHERE assignment_id='finance-fixture-a'; INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES('warehouse','company','company','RFQ-W','Warehouse','active',1); INSERT INTO procurement_rfqs(id,company_id,branch_id,warehouse_id,currency,title,demand_evidence,needed_date,status,created_by,version,created_at,updated_at) VALUES('rfq','company','company','warehouse','THB','Original','fixture','2026-11-20','collecting','fixture-a',1,0,15),('foreign-rfq','other','company','warehouse','THB','Foreign','fixture','2026-11-20','collecting','fixture-a',1,0,15);").execute(db).await.unwrap();
        sqlx::raw_sql("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('supplier','company','supplier','SUP','Supplier','THB',0,0); INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps) VALUES('product1','company','P1','Product 1','each',100,700),('product2','company','P2','Product 2','each',100,700),('product3','company','P3','Product 3','each',100,700); INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES('order','company','company','purchase','PO','supplier','THB','2026-11-01','2026-11-20','draft',100,7,'fixture-a',0,0); INSERT INTO employee_requests(id,company_id,kind,category,requester_user_id,created_by,state,items_json,approval_json,evidence_version,created_at,updated_at) VALUES('request','company','purchase','purchasing','requester','creator','in_progress','[]','{\"decision\":\"approved\"}',1,0,0); INSERT INTO procurement_rfq_lines(id,rfq_id,product_id,description,unit,quantity_milli,ordinal) VALUES('line1','rfq','product1','Product 1','each',1000,0),('line2','rfq','product2','Product 2','each',1000,1); INSERT INTO procurement_rfq_offers VALUES('offer1','rfq','supplier',1,'ref','evidence','2026-11-19','2026-11-20','THB',100,7,'fixture-a',0); INSERT INTO procurement_rfq_offer_lines VALUES('price1','offer1','line1',100,700,100,7);").execute(db).await.unwrap();
        sqlx::query("INSERT INTO procurement_rfqs(id,company_id,branch_id,warehouse_id,currency,title,demand_evidence,needed_date,status,created_by,version,created_at,updated_at) VALUES('other-rfq','company','company','warehouse','THB','Other','fixture','2026-11-20','collecting','fixture-a',1,0,15)").execute(db).await.unwrap();
        p.maker_selection.expected.as_mut().unwrap().assignment_version=4;let now=p.system.scenario.clock.now().get();let other_pool=db.clone();
        let result=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),WorkAuthorizationRequest{permission:"commerce.purchase".into(),module:Some("procurement".into()),destination:"procurement".into(),api:true},TargetLocks,move|c,current|{let ts_query=ts_query.clone();let other_pool=other_pool.clone();Box::pin(async move{
            let target=observe_rfq_target(c,current,"rfq").await?.expect("selected company target");
            let original:String=sqlx::query_scalar(sqlx::AssertSqlSafe(ts_query.as_str())).bind("rfq").fetch_one(&mut *c).await?;assert_eq!(target.facts().image_json,original);
            assert!(observe_rfq_target(c,current,"missing").await?.is_none());
            assert!(observe_rfq_target(c,current,"foreign-rfq").await?.is_none());
            use rfq_materialization::{RfqDetailProjection as Projection,RfqMaterialization as Materialization};
            assert_eq!(rfq_materialization::rfq_row_materialization(c,"rfq","company",None,None).await?,Some(Materialization::Safe));
            sqlx::query("SAVEPOINT range_row").execute(&mut *c).await?;
            sqlx::query("UPDATE procurement_rfqs SET created_at=9007199254740991 WHERE id='rfq'").execute(&mut *c).await?;
            assert_eq!(rfq_materialization::rfq_row_materialization(c,"rfq","company",None,None).await?,Some(Materialization::Safe));
            sqlx::query("UPDATE procurement_rfqs SET created_at=9007199254740992 WHERE id='rfq'").execute(&mut *c).await?;
            assert_eq!(rfq_materialization::rfq_row_materialization(c,"rfq","company",None,None).await?,Some(Materialization::NumberOutOfRange));
            sqlx::query("UPDATE procurement_rfqs SET created_at=-9007199254740992 WHERE id='rfq'").execute(&mut *c).await?;
            assert_eq!(rfq_materialization::rfq_row_materialization(c,"rfq","company",None,None).await?,Some(Materialization::NumberOutOfRange));
            sqlx::query("ROLLBACK TO SAVEPOINT range_row").execute(&mut *c).await?;
            for (projection, update) in [
                (Projection::Lines,"INSERT INTO procurement_rfq_lines(id,rfq_id,product_id,description,unit,quantity_milli,ordinal) VALUES('range-line','rfq','product3','Product 3','each',9007199254740992,2)"),
                (Projection::Offers,"INSERT INTO procurement_rfq_offers VALUES('range-offer','rfq','supplier',2,'ref','evidence','2026-11-19','2026-11-20','THB',9007199254740992,0,'fixture-a',0)"),
                (Projection::OfferLines,"INSERT INTO procurement_rfq_offer_lines VALUES('range-price','offer1','line2',9007199254740992,0,0,0)"),
            ] {
                sqlx::query("SAVEPOINT range_detail").execute(&mut *c).await?;
                sqlx::query(update).execute(&mut *c).await?;
                assert_eq!(rfq_materialization::rfq_detail_materialization(c,"rfq",projection).await?,Materialization::NumberOutOfRange);
                sqlx::query("ROLLBACK TO SAVEPOINT range_detail").execute(&mut *c).await?;
            }
            sqlx::query("SAVEPOINT range_event").execute(&mut *c).await?;
            sqlx::query("INSERT INTO procurement_rfq_events(id,rfq_id,version,action,actor_id,evidence,created_at) VALUES('range-event','rfq',2,'submit','fixture-a','reason',9007199254740992)").execute(&mut *c).await?;
            assert_eq!(rfq_materialization::rfq_detail_materialization(c,"rfq",Projection::Events).await?,Materialization::NumberOutOfRange);
            sqlx::query("ROLLBACK TO SAVEPOINT range_event").execute(&mut *c).await?;
            let witness_input=normalize_rfq_transition(&SourceOrderedJson::parse(r#"{"action":"submit","expectedVersion":1,"offerId":"offer1","reason":"reason"}"#)?).unwrap();
            let other_input=normalize_rfq_transition(&SourceOrderedJson::parse(r#"{"action":"cancel","expectedVersion":1,"reason":"reason"}"#)?).unwrap();
            let assertion=rfq_assertions::prepare_rfq_assertion_witness(c,current,&target,"witness-key",&witness_input).await.unwrap();
            let missing=erpx_workflow_server::public::rfq_command_receipt::inspect_rfq_receipt(c,"witness-key").await?;
            let fresh=rfq_receipt::fresh_rfq_replay_witness(c,current,&missing,&witness_input,"witness-key","rfq").await?.unwrap();
            assert!(rfq_receipt::rfq_observations_match(c,&fresh,&assertion,current,&target,&witness_input,"witness-key").await);
            assertion.require_same_observation(c,"witness-key").await.unwrap();
            assertion.require_for(c,current,&target,&witness_input,"witness-key").await.unwrap();
            assert!(assertion.require_for(c,current,&target,&other_input,"witness-key").await.is_err());
            let second_observation=observe_rfq_target(c,current,"rfq").await?.unwrap();
            assert!(assertion.require_for(c,current,&second_observation,&witness_input,"witness-key").await.is_err());
            let other_target=observe_rfq_target(c,current,"other-rfq").await?.unwrap();
            assert!(assertion.require_for(c,current,&other_target,&witness_input,"witness-key").await.is_err());
            assert!(!rfq_receipt::rfq_observations_match(c,&fresh,&assertion,current,&other_target,&witness_input,"witness-key").await);
            assert!(assertion.require_same_observation(c,"other-key").await.is_err());
            let mut other=other_pool.begin().await?;
            assert!(assertion.require_same_observation(&mut other,"witness-key").await.is_err());
            other.rollback().await?;
            sqlx::query("SAVEPOINT target_witness_change").execute(&mut *c).await?;
            sqlx::query("UPDATE procurement_rfqs SET title='changed' WHERE id='rfq'").execute(&mut *c).await?;
            assert!(assertion.require_same_observation(c,"witness-key").await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT target_witness_change").execute(&mut *c).await?;
            assertion.require_same_observation(c,"witness-key").await.unwrap();
            sqlx::query("DELETE FROM command_assertions WHERE command_id='witness-key'").execute(&mut *c).await?;
            assert!(assertion.require_same_observation(c,"witness-key").await.is_err());
            let image:serde_json::Value=serde_json::from_str(&target.facts().image_json).unwrap();assert!(image["events"].is_null());assert_eq!(image["lines"].as_array().unwrap().len(),2);assert_eq!(image["offers"].as_array().unwrap().len(),1);assert_eq!(image["offerLines"].as_array().unwrap().len(),1);assert!(image["handoffs"].is_null());
            binding::require_enabled_rfq_target(c,current,&target,RfqTransitionAction::Submit,now).await?;
            let submit=normalize_rfq_transition(&SourceOrderedJson::parse(r#"{"action":"submit","expectedVersion":1,"offerId":"offer1","reason":"reason"}"#)?).unwrap();
            let prepared=decision::prepare_rfq_decision(c,current,&target,None,&submit,"2026-11-15").await?;
            assert!(matches!(prepared,decision::RfqDecisionPreparation::Decision(d) if d.status=="review" && d.selected_offer_id.as_deref()==Some("offer1")));
            let bad=normalize_rfq_transition(&SourceOrderedJson::parse(r#"{"action":"submit","expectedVersion":1,"offerId":"missing","reason":"reason"}"#)?).unwrap();
            let denied=decision::prepare_rfq_decision(c,current,&target,None,&bad,"2026-11-15").await.err().expect("unknown offer must fail");
            assert!(matches!(denied,PgTransactionError::Business(e) if e.kind==FailureKind::Conflict && e.code=="RFQ_CURRENT_OFFER_REQUIRED"));
            sqlx::query("SAVEPOINT linked_request").execute(&mut *c).await?;
            sqlx::query("UPDATE procurement_rfqs SET request_id='request',request_version=1,request_evidence_version=1,request_items_json='[]' WHERE id='rfq'").execute(&mut *c).await?;
            let linked=observe_rfq_target(c,current,"rfq").await?.unwrap();
            sqlx::query("SAVEPOINT records_number_range").execute(&mut *c).await?;
            sqlx::query("UPDATE employee_requests SET version=9007199254740992 WHERE id='request'").execute(&mut *c).await?;
            let oversized=observe_rfq_quotation_row(c,&current.selection().workspace().company_id,"request").await?;
            assert!(matches!(request_detail::prepare_rfq_request_detail_enabled(c,current,&linked,&oversized,now).await?,request_detail::RfqDetailPreparation::DatabaseNumberOutOfRange));
            sqlx::query("UPDATE employee_requests SET state='queued',evidence_version=9007199254740992 WHERE id='request'").execute(&mut *c).await?;
            assert_eq!(rfq_materialization::rfq_records_materialization(c,"request","company",false).await?,Some(Materialization::NumberOutOfRange));
            assert!(matches!(rfq_materialization::rfq_enabled_detail_then_source_state(c,"rfq",Some("request"),"company").await,rfq_materialization::RfqDetailAndSourceProbe::Source(Some(Materialization::NumberOutOfRange))));
            assert!(matches!(procurement_rfq_request::rfq_source_after_enabled_detail(c,&current.selection().workspace().company_id,"rfq",Some("request"),"collecting",Some(1),Some("[]"),None).await,procurement_rfq_request::RfqSourceAfterDetail::SourceNumberOutOfRange));
            sqlx::query("ROLLBACK TO SAVEPOINT records_number_range").execute(&mut *c).await?;
            sqlx::query("SAVEPOINT mixed_detail_failure").execute(&mut *c).await?;
            sqlx::query("INSERT INTO procurement_rfq_lines(id,rfq_id,product_id,description,unit,quantity_milli,ordinal) VALUES('mixed-range-line','other-rfq','product3','Product 3','each',9007199254740992,2)").execute(&mut *c).await?;
            sqlx::query("ALTER TABLE procurement_rfq_offers RENAME TO rfq_offers_missing_probe").execute(&mut *c).await?;
            assert!(matches!(rfq_materialization::rfq_enabled_detail_then_source_state(c,"other-rfq",Some("request"),"company").await,rfq_materialization::RfqDetailAndSourceProbe::DetailNumberOutOfRange));
            let aborted=sqlx::query("SELECT 1").execute(&mut *c).await.unwrap_err();
            assert_eq!(aborted.as_database_error().unwrap().code().as_deref(),Some("25P02"));
            sqlx::query("ROLLBACK TO SAVEPOINT mixed_detail_failure").execute(&mut *c).await?;
            sqlx::query("SAVEPOINT sql_detail_failure").execute(&mut *c).await?;
            sqlx::query("ALTER TABLE procurement_rfq_offers RENAME TO rfq_offers_missing_probe").execute(&mut *c).await?;
            match rfq_materialization::rfq_enabled_detail_then_source_state(c,"rfq",Some("request"),"company").await {rfq_materialization::RfqDetailAndSourceProbe::Sql(error)=>assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("42P01")),other=>panic!("expected first SQL error: {other:?}")}
            let aborted=sqlx::query("SELECT 1").execute(&mut *c).await.unwrap_err();
            assert_eq!(aborted.as_database_error().unwrap().code().as_deref(),Some("25P02"));
            sqlx::query("ROLLBACK TO SAVEPOINT sql_detail_failure").execute(&mut *c).await?;
            sqlx::query("SAVEPOINT event_number_range").execute(&mut *c).await?;
            sqlx::query("INSERT INTO employee_request_events(id,company_id,request_id,request_version,action,actor_id,created_at) VALUES('range-request-event','company','request',1,'edit','fixture-a',9007199254740992)").execute(&mut *c).await?;
            let event_error=request_access::rfq_request_detail_events(c,"company","request").await.err().expect("unsafe event time must fail");
            assert!(matches!(event_error,PgTransactionError::Persistence(e) if e.code=="DATABASE_NUMBER_OUT_OF_RANGE"));
            sqlx::query("ROLLBACK TO SAVEPOINT event_number_range").execute(&mut *c).await?;
            sqlx::query("SAVEPOINT attachment_number_range").execute(&mut *c).await?;
            sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) VALUES('range-upload','employee_request','request','request_evidence','file','text/plain',1,'range-staging','ready','fixture-a',100,0,0)").execute(&mut *c).await?;
            sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at) VALUES('range-attachment','range-upload','employee_request','request','request_evidence','range-object',1,'file','text/plain',9007199254740992,'checksum','fixture-a',0)").execute(&mut *c).await?;
            let file_error=erpx_files_server::public::rfq_request_attachments::read_rfq_request_attachments(c,"request").await.err().expect("unsafe attachment bytes must fail");
            assert!(matches!(file_error,PgTransactionError::Persistence(e) if e.code=="DATABASE_NUMBER_OUT_OF_RANGE"));
            sqlx::query("ROLLBACK TO SAVEPOINT attachment_number_range").execute(&mut *c).await?;
            let historical=rfq_assertions::prepare_rfq_assertion_witness(c,current,&linked,"linked-witness-key",&witness_input).await.unwrap();
            sqlx::query("SAVEPOINT request_after_assertion").execute(&mut *c).await?;
            sqlx::query("UPDATE employee_requests SET state='queued' WHERE id='request'").execute(&mut *c).await?;
            // The witness records the earlier assertion; it deliberately does
            // not claim the requisition is still approved after later effects.
            historical.require_for(c,current,&linked,&witness_input,"linked-witness-key").await.unwrap();
            sqlx::query("ROLLBACK TO SAVEPOINT request_after_assertion").execute(&mut *c).await?;
            sqlx::query("DELETE FROM command_assertions WHERE command_id='linked-witness-key'").execute(&mut *c).await?;
            let source=observe_rfq_request(c,&current.selection().workspace().company_id,"request").await?;
            assert!(matches!(procurement_rfq_request::rfq_source_after_enabled_detail(c,&current.selection().workspace().company_id,"rfq",Some("request"),"collecting",Some(1),Some("[]"),Some(&source)).await,procurement_rfq_request::RfqSourceAfterDetail::Blocked(Ok(false))));
            let prepared=decision::prepare_rfq_decision(c,current,&linked,Some(&source),&submit,"2026-11-15").await?;
            assert!(matches!(prepared,decision::RfqDecisionPreparation::Decision(d) if d.status=="review"));
            sqlx::query("SAVEPOINT changed_request").execute(&mut *c).await?;
            sqlx::query("UPDATE employee_requests SET state='queued' WHERE id='request'").execute(&mut *c).await?;
            assert!(decision::prepare_rfq_decision(c,current,&linked,Some(&source),&submit,"2026-11-15").await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT changed_request").execute(&mut *c).await?;
            let fresh=observe_rfq_request(c,&current.selection().workspace().company_id,"request").await?;
            assert!(matches!(decision::prepare_rfq_decision(c,current,&linked,Some(&fresh),&submit,"2026-11-15").await?,decision::RfqDecisionPreparation::Decision(_)));
            let quotation=observe_rfq_quotation_row(c,&current.selection().workspace().company_id,"request").await?;
            assert!(matches!(request_access::initial_rfq_request_access(c,current,&quotation,now).await?,request_access::RfqInitialRequestAccess::NotFound));
            request_access::validate_rfq_request_service_scope(c,current,&quotation).await?;
            sqlx::query("SAVEPOINT invalid_request_branch").execute(&mut *c).await?;
            sqlx::query("UPDATE employee_requests SET branch_id='missing-branch' WHERE id='request'").execute(&mut *c).await?;
            let invalid_branch=observe_rfq_quotation_row(c,&current.selection().workspace().company_id,"request").await?;
            let scope_error=request_access::validate_rfq_request_service_scope(c,current,&invalid_branch).await.err().expect("inactive branch must fail");
            assert!(matches!(scope_error,PgTransactionError::Business(e) if e.kind==FailureKind::InvalidInput && e.code=="LOCATION_INVALID"));
            sqlx::query("ROLLBACK TO SAVEPOINT invalid_request_branch").execute(&mut *c).await?;
            sqlx::query("SAVEPOINT unavailable_request_company").execute(&mut *c).await?;
            sqlx::query("UPDATE organization_nodes SET status='suspended' WHERE id='company'").execute(&mut *c).await?;
            let unavailable=request_access::validate_rfq_request_service_scope(c,current,&quotation).await.err().expect("inactive company must fail");
            assert!(matches!(unavailable,PgTransactionError::Business(e) if e.kind==FailureKind::Conflict && e.code=="COMPANY_UNAVAILABLE"));
            sqlx::query("ROLLBACK TO SAVEPOINT unavailable_request_company").execute(&mut *c).await?;
            sqlx::query("SAVEPOINT quotation_policy").execute(&mut *c).await?;
            sqlx::query("UPDATE employee_requests SET approval_json=$1 WHERE id='request'").bind(r#"{"decision":"approved","decidedBy":"fixture-a"}"#).execute(&mut *c).await?;
            let reviewer_source=observe_rfq_quotation_row(c,&current.selection().workspace().company_id,"request").await?;
            assert!(matches!(request_access::initial_rfq_request_access(c,current,&reviewer_source,now).await?,request_access::RfqInitialRequestAccess::DetailRequired { owner: false, handlers } if handlers.is_empty()));
            let unrelated=observe_rfq_quotation_row(c,&current.selection().workspace().company_id,"other-request").await?;
            assert!(request_detail::prepare_rfq_request_detail_enabled(c,current,&linked,&unrelated,now).await.is_err());
            let detail=match request_detail::prepare_rfq_request_detail_enabled(c,current,&linked,&reviewer_source,now).await? {request_detail::RfqDetailPreparation::QuotationChecksRequired(f)=>f,_=>panic!("detail required")};
            assert!(detail.items_json()=="[]" && !detail.owner() && detail.handlers().is_empty() && detail.events().is_empty() && detail.team_name().is_none() && detail.children().is_empty() && detail.attachments().is_empty());
            erpx_identity_access_server::public::rfq_request_membership::require_rfq_request_actor_unit(c,current).await?;
            let mut wrong_unit=other_pool.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await?;
            assert!(erpx_identity_access_server::public::rfq_request_membership::require_rfq_request_actor_unit(&mut wrong_unit,current).await.is_err());
            assert!(request_detail::prepare_rfq_quotation_gate(&mut wrong_unit,&detail).await.is_err());wrong_unit.rollback().await?;
            assert!(matches!(request_detail::prepare_rfq_quotation_gate(c,&detail).await?,request_detail::RfqQuotationGate::NotEligible));
            sqlx::query("SAVEPOINT quotation_handler").execute(&mut *c).await?;
            sqlx::raw_sql("INSERT INTO service_teams(id,company_id,name,active,created_by,created_at,updated_at) VALUES('quotation-team','company','Purchasing',1,'fixture-a',0,0); INSERT INTO service_team_members(company_id,team_id,user_id,responsibility) VALUES('company','quotation-team','fixture-a','member'); UPDATE employee_requests SET team_id='quotation-team',assignee_user_id='fixture-a' WHERE id='request'").execute(&mut *c).await?;
            let handler_source=observe_rfq_quotation_row(c,&current.selection().workspace().company_id,"request").await?;
            let handler_detail=match request_detail::prepare_rfq_request_detail_enabled(c,current,&linked,&handler_source,now).await? {request_detail::RfqDetailPreparation::QuotationChecksRequired(f)=>f,_=>panic!("handler detail required")};
            assert!(matches!(request_detail::prepare_rfq_quotation_gate(c,&handler_detail).await?,request_detail::RfqQuotationGate::AllocationRequired));
            let items=match request_detail::prepare_rfq_quotation_items(c,&handler_detail).await? {request_detail::RfqQuotationItemsStage::Ready(v)=>v,_=>panic!("quotation items required")};
            assert_eq!(items.mapped().json,"[]");assert!(items.mapped().fully_allocated);items.require_current(c).await?;
            sqlx::query("UPDATE employee_requests SET items_json=$1 WHERE id='request'").bind(r#"[{"quantity":"1","unit":1e999,"extra":"retained"},{"quantity":"1"}]"#).execute(&mut *c).await?;
            let item_source=observe_rfq_quotation_row(c,&current.selection().workspace().company_id,"request").await?;
            let item_detail=match request_detail::prepare_rfq_request_detail_enabled(c,current,&linked,&item_source,now).await? {request_detail::RfqDetailPreparation::QuotationChecksRequired(f)=>f,_=>panic!("item detail required")};
            let mapped=match request_detail::prepare_rfq_quotation_items(c,&item_detail).await? {request_detail::RfqQuotationItemsStage::Ready(v)=>v,_=>panic!("mapped items required")};
            assert_eq!(mapped.mapped().json,r#"[{"quantity":"1","unit":null,"extra":"retained","requestedMilli":1000,"allocatedMilli":0,"remainingMilli":1000},{"quantity":"1","unit":"each","requestedMilli":1000,"allocatedMilli":0,"remainingMilli":1000}]"#);
            assert!(!mapped.mapped().unit_strictly_equals(0,&"each".encode_utf16().collect::<Vec<_>>()));
            mapped.require_current(c).await?;
            let mut other_unit=other_pool.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await?;
            assert!(mapped.require_current(&mut other_unit).await.is_err());other_unit.rollback().await?;
            sqlx::query("SAVEPOINT allocation_drift").execute(&mut *c).await?;
            sqlx::raw_sql("INSERT INTO procurement_rfqs(id,company_id,branch_id,warehouse_id,currency,title,demand_evidence,needed_date,status,created_by,version,created_at,updated_at) VALUES('other-source-rfq','company','company','warehouse','THB','Other','fixture','2026-11-20','collecting','fixture-a',1,0,0); INSERT INTO procurement_rfq_lines(id,rfq_id,product_id,description,unit,quantity_milli,ordinal,request_item_ordinal,request_quantity_milli,request_unit,request_units_per_product_unit_milli) VALUES('other-source-line','other-source-rfq','product3','Product','each',500,0,1,500,'each',1000); UPDATE procurement_rfqs SET request_id='request' WHERE id='other-source-rfq'").execute(&mut *c).await?;
            assert!(mapped.require_current(c).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT allocation_drift").execute(&mut *c).await?;mapped.require_current(c).await?;
            sqlx::query("ROLLBACK TO SAVEPOINT quotation_handler").execute(&mut *c).await?;
            sqlx::query("SAVEPOINT detail_children").execute(&mut *c).await?;
            sqlx::raw_sql("INSERT INTO employee_requests(id,company_id,kind,category,requester_user_id,created_by,state,items_json,approval_json,parent_request_id,created_at,updated_at) VALUES('child-visible','company','service','x','fixture-a','creator','draft','[]','{}','request',1,0),('child-hidden','company','service','x','other','other','draft','[]','{}','request',2,0)").execute(&mut *c).await?;
            let detail=request_detail::prepare_rfq_request_detail_enabled(c,current,&linked,&reviewer_source,now).await?;
            assert!(matches!(detail,request_detail::RfqDetailPreparation::QuotationChecksRequired(f) if f.children().len()==1 && f.children()[0].id=="child-visible"));
            sqlx::query("UPDATE employee_requests SET approval_json='{' WHERE id='child-hidden'").execute(&mut *c).await?;
            assert!(matches!(request_detail::prepare_rfq_request_detail_enabled(c,current,&linked,&reviewer_source,now).await?,request_detail::RfqDetailPreparation::JsonSyntax));
            sqlx::query("ROLLBACK TO SAVEPOINT detail_children").execute(&mut *c).await?;
            sqlx::query("SAVEPOINT invalid_items").execute(&mut *c).await?;
            sqlx::query("UPDATE employee_requests SET items_json='[broken' WHERE id='request'").execute(&mut *c).await?;
            let invalid_items=observe_rfq_quotation_row(c,&current.selection().workspace().company_id,"request").await?;
            assert!(matches!(request_detail::prepare_rfq_request_detail_enabled(c,current,&linked,&invalid_items,now).await?,request_detail::RfqDetailPreparation::JsonSyntax));
            sqlx::query("ROLLBACK TO SAVEPOINT invalid_items").execute(&mut *c).await?;
            sqlx::query("UPDATE employee_requests SET approval_json=$1 WHERE id='request'").bind(r#"{"route":{"steps":[{"userId":"fixture-a","snapshot":{"departmentPolicyVersion":3}}]}}"#).execute(&mut *c).await?;
            let policy_source=observe_rfq_quotation_row(c,&current.selection().workspace().company_id,"request").await?;
            assert!(matches!(request_access::initial_rfq_request_access(c,current,&policy_source,now).await?,request_access::RfqInitialRequestAccess::DepartmentPolicyRequired));
            sqlx::query("UPDATE employee_requests SET approval_json='null' WHERE id='request'").execute(&mut *c).await?;
            let null_source=observe_rfq_quotation_row(c,&current.selection().workspace().company_id,"request").await?;
            assert!(matches!(request_access::initial_rfq_request_access(c,current,&null_source,now).await?,request_access::RfqInitialRequestAccess::PropertyTypeError));
            sqlx::query("ROLLBACK TO SAVEPOINT quotation_policy").execute(&mut *c).await?;
            sqlx::query("UPDATE procurement_rfqs SET status='approved',selected_offer_id='offer1' WHERE id='rfq'").execute(&mut *c).await?;
            let approved=observe_rfq_target(c,current,"rfq").await?.unwrap();
            let cancel=normalize_rfq_transition(&SourceOrderedJson::parse(r#"{"action":"cancel","expectedVersion":1,"reason":"reason"}"#)?).unwrap();
            assert!(matches!(decision::prepare_rfq_decision(c,current,&approved,Some(&fresh),&cancel,"2026-11-15").await?,decision::RfqDecisionPreparation::QuotationSourceRequired));
            sqlx::query("SAVEPOINT approved_blocked").execute(&mut *c).await?;
            sqlx::query("UPDATE employee_requests SET state='queued' WHERE id='request'").execute(&mut *c).await?;
            let blocked_source=observe_rfq_request(c,&current.selection().workspace().company_id,"request").await?;
            assert!(matches!(decision::prepare_rfq_decision(c,current,&approved,Some(&blocked_source),&cancel,"2026-11-15").await?,decision::RfqDecisionPreparation::QuotationSourceRequired));
            sqlx::query("ROLLBACK TO SAVEPOINT approved_blocked").execute(&mut *c).await?;
            sqlx::query("SAVEPOINT approved_missing").execute(&mut *c).await?;
            sqlx::query("UPDATE employee_requests SET company_id='other' WHERE id='request'").execute(&mut *c).await?;
            let missing_source=observe_rfq_request(c,&current.selection().workspace().company_id,"request").await?;
            let missing=decision::prepare_rfq_decision(c,current,&approved,Some(&missing_source),&submit,"2026-11-15").await.err().expect("missing source must continue to the blocked transition check");
            assert!(matches!(missing,PgTransactionError::Business(e) if e.kind==FailureKind::Conflict && e.code=="RFQ_REQUEST_CHANGED"));
            sqlx::query("ROLLBACK TO SAVEPOINT approved_missing").execute(&mut *c).await?;
            sqlx::query("ROLLBACK TO SAVEPOINT linked_request").execute(&mut *c).await?;target.require_current(c).await?;
            let mut other=other_pool.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await?;
            use erpx_identity_access_server::public::rfq_target_authority::{require_rfq_target_permissions,RfqScopePermission};
            let crossed=require_rfq_target_permissions(&mut other,current,"company",Some("company"),None,&[RfqScopePermission::Purchase],now).await.err().expect("another transaction cannot reuse the actor");
            assert!(matches!(crossed,PgTransactionError::Business(e) if e.kind==FailureKind::Forbidden && e.code=="PROCUREMENT_ACCESS_CHANGED"));other.rollback().await?;
            let denied=binding::require_enabled_rfq_target(c,current,&target,RfqTransitionAction::Approve,now).await.err().expect("approval permission must be checked");
            assert!(matches!(denied,PgTransactionError::Business(e) if e.kind==FailureKind::Forbidden && e.code=="PROCUREMENT_ACCESS_CHANGED"));
            sqlx::query("SAVEPOINT revoked_assignment").execute(&mut *c).await?;
            sqlx::query("UPDATE assignments SET revoked_at=1 WHERE id='finance-fixture-a'").execute(&mut *c).await?;
            let revoked=binding::require_enabled_rfq_target(c,current,&target,RfqTransitionAction::Submit,now).await.err().expect("revoked selected assignment must fail");
            assert!(matches!(revoked,PgTransactionError::Business(e) if e.kind==FailureKind::Forbidden && e.code=="PROCUREMENT_ACCESS_CHANGED"));
            sqlx::query("ROLLBACK TO SAVEPOINT revoked_assignment").execute(&mut *c).await?;binding::require_enabled_rfq_target(c,current,&target,RfqTransitionAction::Submit,now).await?;
            sqlx::query("SAVEPOINT rfq_child").execute(&mut *c).await?;
            sqlx::query("INSERT INTO procurement_rfq_events(id,rfq_id,version,action,actor_id,evidence,selected_offer_id,created_at) VALUES('child','rfq',2,'submit','fixture-a','reason',NULL,20)").execute(&mut *c).await?;
            assert!(target.require_current(c).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT rfq_child").execute(&mut *c).await?;target.require_current(c).await?;
            sqlx::query("SAVEPOINT rfq_line").execute(&mut *c).await?;
            sqlx::query("INSERT INTO procurement_rfq_lines(id,rfq_id,product_id,description,unit,quantity_milli,ordinal) VALUES('line3','rfq','product3','Product 3','each',1000,2)").execute(&mut *c).await?;
            assert!(target.require_current(c).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT rfq_line").execute(&mut *c).await?;target.require_current(c).await?;
            sqlx::query("SAVEPOINT rfq_offer").execute(&mut *c).await?;
            sqlx::query("INSERT INTO procurement_rfq_offers VALUES('offer2','rfq','supplier',2,'ref','evidence','2026-11-19','2026-11-20','THB',100,7,'fixture-a',0)").execute(&mut *c).await?;
            assert!(target.require_current(c).await.is_err());
            let latest=observe_rfq_target(c,current,"rfq").await?.unwrap();
            let stale=decision::prepare_rfq_decision(c,current,&latest,None,&submit,"2026-11-15").await.err().expect("older supplier offer must not be current");
            assert!(matches!(stale,PgTransactionError::Business(e) if e.code=="RFQ_CURRENT_OFFER_REQUIRED"));
            let new_offer=normalize_rfq_transition(&SourceOrderedJson::parse(r#"{"action":"submit","expectedVersion":1,"offerId":"offer2","reason":"reason"}"#)?).unwrap();
            assert!(matches!(decision::prepare_rfq_decision(c,current,&latest,None,&new_offer,"2026-11-15").await?,decision::RfqDecisionPreparation::Decision(d) if d.selected_offer_id.as_deref()==Some("offer2")));
            sqlx::query("ROLLBACK TO SAVEPOINT rfq_offer").execute(&mut *c).await?;target.require_current(c).await?;
            sqlx::query("SAVEPOINT rfq_offer_line").execute(&mut *c).await?;
            sqlx::query("INSERT INTO procurement_rfq_offer_lines VALUES('price2','offer1','line2',100,700,100,7)").execute(&mut *c).await?;
            assert!(target.require_current(c).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT rfq_offer_line").execute(&mut *c).await?;target.require_current(c).await?;
            sqlx::query("SAVEPOINT rfq_handoff").execute(&mut *c).await?;
            sqlx::query("INSERT INTO procurement_request_orders(request_id,order_id,rfq_id,request_version,request_evidence_version,linked_by,created_at) VALUES('request','order','rfq',1,1,'fixture-a',0)").execute(&mut *c).await?;
            assert!(target.require_current(c).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT rfq_handoff").execute(&mut *c).await?;target.require_current(c).await?;
            sqlx::query("SAVEPOINT rfq_parent").execute(&mut *c).await?;
            sqlx::query("UPDATE procurement_rfqs SET title='Changed' WHERE id='rfq'").execute(&mut *c).await?;
            assert!(target.require_current(c).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT rfq_parent").execute(&mut *c).await?;target.require_current(c).await?;
            Ok(true)
        })}).await.unwrap();assert!(result);
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('branch1','branch','north','North','company','company','branch1','active'),('branch2','branch','south','South','company','company','branch2','active'); UPDATE assignments SET scope_kind='branch',scope_node_id='branch1',branch_id='branch1',version=5 WHERE id='finance-fixture-a'; UPDATE active_work_contexts SET assignment_version=5 WHERE assignment_id='finance-fixture-a'; UPDATE procurement_rfqs SET branch_id='branch1' WHERE id='rfq'; INSERT INTO procurement_rfqs(id,company_id,branch_id,warehouse_id,currency,title,demand_evidence,needed_date,status,created_by,version,created_at,updated_at) VALUES('other-branch','company','branch2','warehouse','THB','Other branch','fixture','2026-11-20','collecting','fixture-a',1,0,15);").execute(db).await.unwrap();
        p.maker_selection.expected.as_mut().unwrap().assignment_version=5;p.maker_selection.branch_slug=Some("north".into());
        let scoped=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),WorkAuthorizationRequest{permission:"commerce.purchase".into(),module:Some("procurement".into()),destination:"procurement".into(),api:true},TargetLocks,move|c,current|Box::pin(async move{
            let target=observe_rfq_target(c,current,"rfq").await?.expect("selected branch target");
            binding::require_enabled_rfq_target(c,current,&target,RfqTransitionAction::Submit,now).await?;
            assert!(observe_rfq_target(c,current,"other-branch").await?.is_none());
            use erpx_identity_access_server::public::rfq_target_authority::{require_rfq_target_permissions,RfqScopePermission};
            let denied=require_rfq_target_permissions(c,current,"company",Some("branch2"),None,&[RfqScopePermission::Purchase],now).await.err().expect("wrong branch must be denied");
            assert!(matches!(denied,PgTransactionError::Business(e) if e.kind==FailureKind::Forbidden && e.code=="PROCUREMENT_ACCESS_CHANGED"));
            let quotation=observe_rfq_quotation_row(c,&current.selection().workspace().company_id,"request").await?;
            assert!(matches!(request_access::initial_rfq_request_access(c,current,&quotation,now).await?,request_access::RfqInitialRequestAccess::NotFound));
            let outside=request_access::validate_rfq_request_service_scope(c,current,&quotation).await.err().expect("outside workspace must fail before node reads");
            assert!(matches!(outside,PgTransactionError::Business(e) if e.kind==FailureKind::Forbidden && e.code=="REQUEST_SCOPE_FORBIDDEN"));
            Ok(true)
        })).await.unwrap();assert!(scoped);
    }).await;
}
