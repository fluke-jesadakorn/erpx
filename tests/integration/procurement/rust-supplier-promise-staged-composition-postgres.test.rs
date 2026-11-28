//! Unmounted staged native command with actual Account and selected IAM.
#![allow(dead_code)]
#[path = "../../../platform/composition/src/finance_journal_actor.rs"]
mod real_command_actor;
#[path = "../../support/finance/review-composition.rs"]
mod review;
use erpx_commerce_server::public::supplier_promise_source::{
    SupplierPromiseSourceMode, inspect_supplier_promise_source, lock_supplier_promise_sources,
};
use erpx_commerce_server::public::supplier_promise_version::{
    lock_supplier_promise_order, prepare_supplier_promise_order,
};
use erpx_identity_access_server::public::effective_authority::{
    HoldingWorkSourcePreparation, WorkAuthorizationRequest,
};
use erpx_procurement_server::public::supplier_promise_command::{
    RecordSupplierPromiseCommand, SupplierPromiseLineCommand, lock_supplier_promise_history_writes,
};
use erpx_procurement_server::public::supplier_promise_staged::{
    SupplierPromiseBegin, begin_supplier_promise_stage,
};
use erpx_platform_runtime_server::public::postgres::PgWorkFuture;
use erpx_platform_shared_server::public::{BusinessError, Clock};
use sqlx::PgConnection;
#[derive(Clone)]
struct Sources;
impl HoldingWorkSourcePreparation for Sources {
    fn prepare<'a>(&'a self, c: &'a mut PgConnection) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            lock_supplier_promise_order(c).await?;
            lock_supplier_promise_sources(c).await?;
            lock_supplier_promise_history_writes(c).await?;
            Ok(())
        })
    }
}
#[tokio::test]
async fn staged_native_command_uses_current_account_and_same_unit() {
    review::account::with_system(|system|async move {
  let mut p=review::setup_with_finance(system,false).await;
  let pool=&p.system.holdings[0];
  sqlx::raw_sql(r#"INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_finance_review',2,'["commerce.read","commerce.purchase","module.procurement.read"]','[]','["holding"]','fixture',0);
   UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_finance_review';
   UPDATE assignments SET modules_json='["procurement"]',version=4 WHERE id='finance-fixture-a';
   UPDATE active_work_contexts SET assignment_version=4 WHERE assignment_id='finance-fixture-a';
   INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('supplier','company','supplier','SUP','Supplier','THB',10,10);
   INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps) VALUES('product','company','SKU','Product','each',100,700);
   INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES('order','company','company','purchase','PO','supplier','THB','2026-10-20','2026-11-10','confirmed',10000,700,'fixture-a',10,10);
   INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal) VALUES('line','order','product','Product',10000,0,100,700,10000,700,0);"#).execute(pool.raw()).await.unwrap();
  p.maker_selection.expected.as_mut().unwrap().assignment_version=4;
  let now=p.system.scenario.clock.now();
  let input=RecordSupplierPromiseCommand{expected_version:1,kind:"confirmed".into(),confirmation_date:"2026-10-25".into(),supplier_reference:"SUP-REF".into(),evidence:"Supplier confirmation".into(),attachment_set_ids:vec![],lines:vec![SupplierPromiseLineCommand{order_line_id:"line".into(),quantity:"4.000".into(),promised_date:"2026-10-26".into()}]};
  let replay_input=input.clone();
  let receipt=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),WorkAuthorizationRequest{permission:"commerce.purchase".into(),module:Some("procurement".into()),destination:"procurement".into(),api:true},Sources,move |c,current|{let input=input.clone();Box::pin(async move {
   let projected=real_command_actor::command_actor(current)?;
   assert_eq!(projected.user_id, *current.user_id());
   assert_eq!(projected.assignment_id, current.selection().assignment().id);
   assert_eq!(projected.role, current.selection().assignment().role);
   assert_eq!(projected.company_id, current.selection().workspace().company_id);
   use erpx_workflow_server::public::commands::{CommandField,CommandJson,WorkAuthority,command_fingerprint};
   assert_eq!(projected.branch_id,CommandField::Null);
   assert_eq!(projected.department_id,CommandField::Null);
   assert_eq!(projected.authority,CommandField::Value(WorkAuthority {session_id:current.session_id().as_str().into(),revision:current.selection().revision(),assignment_version:current.selection().assignment().version}));
   // Pinned Bun Zod+JSON.stringify fixture. Only the session is replaced by a
   // fixed literal for independent digest comparison; actual projection above
   // is always derived from the real verified Account session.
   let mut golden_actor=projected.clone();
   golden_actor.authority=CommandField::Value(WorkAuthority{session_id:"fixture-session".into(),revision:7,assignment_version:4});
   let golden_input=CommandJson::parse(r#"{"expectedVersion":1,"kind":"confirmed","confirmationDate":"2026-10-25","supplierReference":"SUP-REF","evidence":"Supplier confirmation","attachmentSetIds":["set-a","set-a"],"lines":[{"orderLineId":"line","quantity":"4.000","promisedDate":"2026-10-26"}]}"#)?;
   assert_eq!(command_fingerprint(&golden_actor,"procurement.promise.record","order",&golden_input)?,"aa4f8dee7640f35dd14a1e17977555dc79fc5949e25d1af29b7512940de28258");
   let company=current.selection().workspace().company_id.clone();
   let order=prepare_supplier_promise_order(c,&company,"order",1).await?;
   let pending=match begin_supplier_promise_stage(c,current,"order",&input,"stage-key-1",now).await? {SupplierPromiseBegin::Fresh(v)=>v,SupplierPromiseBegin::Replay(_)=>panic!("fresh")};
   let source=inspect_supplier_promise_source(c,SupplierPromiseSourceMode::Enabled,&company,None,None,"order").await?.expect("actual order");
   let stage=pending.prepare_enabled(c,current,source,now).await?;
   // No Files evidence in this native-only test; evidence callers must run its staged guards first.
   let inserted=stage.insert_after_source_guard(c,current,now).await?;
   let advanced=order.advance(c,now.get()).await?;
   inserted.finalize(c,current,&advanced,now).await
  })}).await.unwrap();
  assert_eq!((receipt.id.as_str(),receipt.version),("order",2));
  sqlx::query("UPDATE commerce_orders SET state='draft',version=version+1 WHERE id='order'").execute(pool.raw()).await.unwrap();
  let retry_input=replay_input.clone();
  let replay=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),WorkAuthorizationRequest{permission:"commerce.purchase".into(),module:Some("procurement".into()),destination:"procurement".into(),api:true},Sources,move |c,current|{let input=retry_input.clone();Box::pin(async move {
   match begin_supplier_promise_stage(c,current,"order",&input,"stage-key-1",now).await? {SupplierPromiseBegin::Replay(v)=>Ok(v),SupplierPromiseBegin::Fresh(_)=>panic!("native replay expected")}
  })}).await.unwrap();
  assert_eq!(replay,receipt);
  sqlx::query("UPDATE assignments SET revoked_at=1 WHERE id='finance-fixture-a'").execute(pool.raw()).await.unwrap();
  let called=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let observed=called.clone();
  let denied=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),WorkAuthorizationRequest{permission:"commerce.purchase".into(),module:Some("procurement".into()),destination:"procurement".into(),api:true},Sources,move |_c,_current|{observed.store(true,std::sync::atomic::Ordering::SeqCst);Box::pin(async {Ok(())})}).await;
  assert!(denied.is_err());assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
  let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT version FROM commerce_orders WHERE id='order'),(SELECT count(*) FROM procurement_delivery_promises WHERE id='supplier-promise-stage-key-1'),(SELECT count(*) FROM command_receipts WHERE command_id='stage-key-1')").fetch_one(pool.raw()).await.unwrap();assert_eq!(counts,(3,1,1));
 }).await;
}

// Test-only bridge: source and selected Account context are real; feature bits
// are fixtures, not production feature configuration or purchase authority.
mod evidence_bridge {
    use super::*;
    use erpx_commerce_server::public::supplier_promise_source::CurrentSupplierPromiseSource;
    use erpx_files_server::public::supplier_promise_binding::*;
    use erpx_identity_access_server::public::effective_authority::AuthorizedWorkContext;
    use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};

    type FilesError = PgTransactionError<SupplierPromiseEvidenceError>;
    fn bridge_error() -> FilesError { PersistenceError::new("TEST_EVIDENCE_OWNER_CHANGED").into() }
    pub struct Before<'a> {
        pub facts: SupplierPromiseEvidenceFacts,
        pub source: &'a CurrentSupplierPromiseSource,
        pub current: &'a AuthorizedWorkContext,
    }
    impl SupplierPromiseEvidenceOwner for Before<'_> {
        fn facts(&self) -> &SupplierPromiseEvidenceFacts { &self.facts }
        fn features(&self) -> SupplierEvidenceFeatures { SupplierEvidenceFeatures { enabled:true, read_only:false, uploads_frozen:false } }
        async fn require_before(&self, c: &mut PgConnection) -> Result<(), FilesError> {
            if self.current.user_id().as_str() != self.facts.actor_id || self.current.selection().workspace().company_id.as_str() != self.facts.company_id || self.source.order().id != self.facts.order_id || self.source.order().version != self.facts.order_version {
                return Err(bridge_error());
            }
            self.current.require_permission("commerce.purchase").map_err(|_| bridge_error())?;
            self.source.require_same_unit(c).await.map_err(|_| bridge_error())
        }
        async fn require_after(&self, _: &mut PgConnection) -> Result<(), FilesError> { Err(bridge_error()) }
    }
    pub struct After<'a> {
        pub facts: SupplierPromiseEvidenceFacts,
        pub order: &'a erpx_commerce_server::public::supplier_promise_version::RetainedSupplierPromiseOrder,
        pub record: &'a erpx_procurement_server::public::supplier_promise_command::CurrentSupplierPromiseRecord,
    }
    impl SupplierPromiseEvidenceOwner for After<'_> {
        fn facts(&self) -> &SupplierPromiseEvidenceFacts { &self.facts }
        fn features(&self) -> SupplierEvidenceFeatures { SupplierEvidenceFeatures { enabled:true, read_only:false, uploads_frozen:false } }
        async fn require_before(&self, _: &mut PgConnection) -> Result<(), FilesError> { Err(bridge_error()) }
        async fn require_after(&self, c: &mut PgConnection) -> Result<(), FilesError> {
            if self.order.company() != self.facts.company_id || self.order.order_id() != self.facts.order_id || self.order.version() != self.facts.order_version + 1 { return Err(bridge_error()); }
            self.order.require_current(c).await.map_err(|_| bridge_error())?;
            self.record.require_current(c).await.map_err(|_| bridge_error())
        }
    }
}

#[derive(Clone)]
struct EvidenceSources;
impl HoldingWorkSourcePreparation for EvidenceSources {
    fn prepare<'a>(&'a self, c: &'a mut PgConnection) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            // Test-only stronger purchase fence; not a statement of TS mode parity.
            erpx_commercial_server::public::purchases::lock_purchase_read_sources(c).await?;
            lock_supplier_promise_order(c).await?;
            lock_supplier_promise_sources(c).await?;
            lock_supplier_promise_history_writes(c).await?;
            erpx_files_server::public::supplier_promise_binding::lock_supplier_promise_evidence_sources(c).await.map_err(|_| erpx_platform_runtime_server::public::postgres::PersistenceError::new("TEST_FILES_LOCK"))?;
            Ok(())
        })
    }
}

#[tokio::test]
async fn staged_evidence_joins_real_owners_and_purchased_authority() {
    use erpx_commercial_server::public::purchases::{current_package_catalogue, CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseRequest};
    use erpx_files_server::public::supplier_promise_binding::*;
    use erpx_procurement_server::public::supplier_promise_command::inspect_supplier_promise_record;
    use erpx_procurement_server::public::supplier_promise_staged::begin_supplier_promise_evidence_stage;
    use erpx_platform_runtime_server::public::postgres::PersistenceError;
    use erpx_platform_shared_server::public::HoldingId;
    review::account::with_system(|system| async move {
        let mut p=review::setup_with_finance(system,false).await;
        let pool=&p.system.holdings[0];
        let now=p.system.scenario.clock.now();
        sqlx::raw_sql(r#"INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_finance_review',2,'["commerce.read","commerce.purchase","module.procurement.read"]','[]','["holding"]','fixture',0);
UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_finance_review';
UPDATE assignments SET modules_json='["procurement"]',version=4 WHERE id='finance-fixture-a';
UPDATE active_work_contexts SET assignment_version=4 WHERE assignment_id='finance-fixture-a';
INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('supplier','company','supplier','SUP','Supplier','THB',10,10);
INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps) VALUES('product','company','SKU','Product','each',100,700);
INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES('order','company','company','purchase','PO','supplier','THB','2026-10-20','2026-11-10','confirmed',10000,700,'fixture-a',10,10);
INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal) VALUES('line','order','product','Product',10000,0,100,700,10000,700,0);
INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) VALUES('upload','holding','company','delivery','proof.png','image/png',10,'staging','ready','fixture-a',4102444800000,0,0);
INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) VALUES('attachment','upload','holding','company','delivery','original',1,'proof.png','image/png',10,repeat('a',64),'fixture-a',0,'company','image/png','available',1,1);
INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) VALUES('set-1','company',NULL,NULL,'purchase_order','order','action','supplier-promise','supplier_promise',1,1,'internal','[]',1,2,'draft','fixture-a',0,0);
INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES('link','set-1','attachment','draft','fixture-a',0,4102444800000);"#).execute(pool.raw()).await.unwrap();
        let mut grant=pool.raw().begin().await.unwrap();
        sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES('company','supply',1,'active',0,$1,1,'Fixture purchase','Fixture evidence',$2)").bind(now.get()+3_600_000).bind(now.get()).execute(&mut *grant).await.unwrap();
        sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'grant-supply','company',package_id,'fixture-b','fixture-hash',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company' AND package_id='supply'").bind(now.get()).execute(&mut *grant).await.unwrap();
        grant.commit().await.unwrap();
        p.maker_selection.expected.as_mut().unwrap().assignment_version=4;
        let input=RecordSupplierPromiseCommand{expected_version:1,kind:"confirmed".into(),confirmation_date:"2026-10-25".into(),supplier_reference:"SUP-REF".into(),evidence:"Supplier confirmation".into(),attachment_set_ids:vec!["set-1".into()],lines:vec![SupplierPromiseLineCommand{order_line_id:"line".into(),quantity:"4.000".into(),promised_date:"2026-10-26".into()}]};
        let replay_input=input.clone();
        let receipt=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),WorkAuthorizationRequest{permission:"commerce.purchase".into(),module:Some("procurement".into()),destination:"procurement".into(),api:true},EvidenceSources,move |c,current|{let input=input.clone();Box::pin(async move {
            let company=current.selection().workspace().company_id.clone();
            let holding=HoldingId::new("h001")?;
            let catalogue=current_package_catalogue()?;
            let request=PurchaseRequest{capabilities:vec!["supply.purchasing".into()],purpose:CapabilityPurpose::SettleExisting,mode:PackageEnforcementMode::Enforced};
            let purchase=PgCompanyPurchaseReader::new(holding.clone(),company.clone(),c,&catalogue).inspect(&request,now).await?.require_allowed()?.clone();
            sqlx::query("SAVEPOINT revoked_purchase").execute(&mut *c).await?;
            sqlx::query("UPDATE company_package_grants SET state='inactive',version=version+1 WHERE company_id='company' AND package_id='supply'").execute(&mut *c).await?;
            assert!(PgCompanyPurchaseReader::new(holding.clone(),company.clone(),c,&catalogue).revalidate(&purchase,now).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT revoked_purchase").execute(&mut *c).await?;
            sqlx::query("SAVEPOINT missing_evidence").execute(&mut *c).await?;
            let missing_order=prepare_supplier_promise_order(c,&company,"order",1).await?;
            let missing_pending=match begin_supplier_promise_evidence_stage(c,current,"order",&input,"evidence-key",now).await? {SupplierPromiseBegin::Fresh(v)=>v,_=>panic!("fresh")};
            let missing_source=inspect_supplier_promise_source(c,SupplierPromiseSourceMode::Enabled,&company,None,None,"order").await?.unwrap();
            let missing_stage=missing_pending.prepare_enabled(c,current,missing_source,now).await?;
            let missing_insert=missing_stage.insert_after_source_guard(c,current,now).await?;
            let missing_advance=missing_order.advance(c,now.get()).await?;
            assert!(missing_insert.finalize(c,current,&missing_advance,now).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT missing_evidence").execute(&mut *c).await?;
            sqlx::query("SAVEPOINT wrong_evidence").execute(&mut *c).await?;
            let wrong_order=prepare_supplier_promise_order(c,&company,"order",1).await?;
            let wrong_pending=match begin_supplier_promise_evidence_stage(c,current,"order",&input,"evidence-key",now).await? {SupplierPromiseBegin::Fresh(v)=>v,_=>panic!("fresh")};
            let wrong_source=inspect_supplier_promise_source(c,SupplierPromiseSourceMode::Enabled,&company,None,None,"order").await?.unwrap();
            let wrong_bridge_source=inspect_supplier_promise_source(c,SupplierPromiseSourceMode::Enabled,&company,None,None,"order").await?.unwrap();
            let wrong_stage=wrong_pending.prepare_enabled(c,current,wrong_source,now).await?;
            let wrong_facts=SupplierPromiseEvidenceFacts{company_id:company.as_str().into(),order_id:"order".into(),order_version:1,actor_id:current.user_id().as_str().into(),command_key:"different-key".into(),promise_id:"supplier-promise-different-key".into()};
            let wrong_before=evidence_bridge::Before{facts:wrong_facts,source:&wrong_bridge_source,current};
            let wrong_files=prepare_supplier_promise_evidence(c,&wrong_before,&company,"order",1,&input.attachment_set_ids,now.get()).await.map_err(|_| PersistenceError::new("TEST_FILES_PREPARE"))?;
            let wrong_guard=validate_supplier_promise_evidence_guards(c,&wrong_before,&wrong_files).await.map_err(|_| PersistenceError::new("TEST_FILES_GUARD"))?;
            let wrong_insert=wrong_stage.insert_after_source_guard(c,current,now).await?;
            let wrong_advance=wrong_order.advance(c,now.get()).await?;
            let wrong_applied=apply_supplier_promise_evidence(c,&wrong_before,&wrong_guard).await.map_err(|_| PersistenceError::new("TEST_FILES_APPLY"))?;
            assert!(wrong_insert.finalize_with_evidence(c,current,&wrong_advance,&wrong_applied,now).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT wrong_evidence").execute(&mut *c).await?;
            let order=prepare_supplier_promise_order(c,&company,"order",1).await?;
            assert!(begin_supplier_promise_stage(c,current,"order",&input,"evidence-key",now).await.is_err());
            let pending=match begin_supplier_promise_evidence_stage(c,current,"order",&input,"evidence-key",now).await? {SupplierPromiseBegin::Fresh(v)=>v,_=>panic!("fresh")};
            let source=inspect_supplier_promise_source(c,SupplierPromiseSourceMode::Enabled,&company,None,None,"order").await?.unwrap();
            let bridge_source=inspect_supplier_promise_source(c,SupplierPromiseSourceMode::Enabled,&company,None,None,"order").await?.unwrap();
            let stage=pending.prepare_enabled(c,current,source,now).await?;
            let facts=SupplierPromiseEvidenceFacts{company_id:company.as_str().into(),order_id:"order".into(),order_version:1,actor_id:current.user_id().as_str().into(),command_key:"evidence-key".into(),promise_id:stage.promise_id().into()};
            let before=evidence_bridge::Before{facts:facts.clone(),source:&bridge_source,current};
            let files=prepare_supplier_promise_evidence(c,&before,&company,"order",1,&input.attachment_set_ids,now.get()).await.map_err(|_| PersistenceError::new("TEST_FILES_PREPARE"))?;
            let guarded=validate_supplier_promise_evidence_guards(c,&before,&files).await.map_err(|_| PersistenceError::new("TEST_FILES_GUARD"))?;
            let inserted=stage.insert_after_source_guard(c,current,now).await?;
            let record=inspect_supplier_promise_record(c,&company,"order",&facts.actor_id,"evidence-key",1).await?;
            let advanced=order.advance(c,now.get()).await?;
            let retained=apply_supplier_promise_evidence(c,&before,&guarded).await.map_err(|_| PersistenceError::new("TEST_FILES_APPLY"))?;
            let after=evidence_bridge::After{facts,order:&advanced,record:&record};
            retained.require_current(c,&after).await.map_err(|_| PersistenceError::new("TEST_FILES_DIAGNOSTIC"))?;
            PgCompanyPurchaseReader::new(holding,company,c,&catalogue).revalidate(&purchase,now).await?.require_allowed()?;
            inserted.finalize_with_evidence(c,current,&advanced,&retained,now).await
        })}).await.unwrap();
        assert_eq!((receipt.id.as_str(),receipt.version),("order",2));
        // Native receipt replay happens before fresh source/Files preparation.
        sqlx::query("UPDATE commerce_orders SET state='draft',version=version+1 WHERE id='order'").execute(pool.raw()).await.unwrap();
        let retry=replay_input.clone();
        let replay=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),WorkAuthorizationRequest{permission:"commerce.purchase".into(),module:Some("procurement".into()),destination:"procurement".into(),api:true},EvidenceSources,move |c,current|{let input=retry.clone();Box::pin(async move {
            match begin_supplier_promise_evidence_stage(c,current,"order",&input,"evidence-key",now).await? {SupplierPromiseBegin::Replay(v)=>Ok(v),_=>panic!("expected native receipt replay")}
        })}).await.unwrap();
        assert_eq!(replay,receipt);
        let mut altered=replay_input;
        altered.evidence="Changed supplier confirmation".into();
        let conflict=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),WorkAuthorizationRequest{permission:"commerce.purchase".into(),module:Some("procurement".into()),destination:"procurement".into(),api:true},EvidenceSources,move |c,current|{let input=altered.clone();Box::pin(async move {
            match begin_supplier_promise_evidence_stage(c,current,"order",&input,"evidence-key",now).await? {SupplierPromiseBegin::Replay(v)=>Ok(v),_=>panic!("unexpected fresh command")}
        })}).await;
        assert!(conflict.is_err());
    }).await;
}
