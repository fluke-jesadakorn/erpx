//! Actual Field/Commercial/Account witnesses for the Files intake composition.
#[allow(dead_code, reason = "Shared real Account and Field Service fixture")]
#[expect(
    unused_imports,
    reason = "Shared fixture reexports helpers for other targets"
)]
#[path = "../../support/field_service/parts.rs"]
mod facts;

use erpx_server_files::public::field_visit_intake::{
    FieldIntakeAvailability, FieldSetRequest, create_field_set, prepare_field_intake,
};
use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
use erpx_server_shared::public::{Clock, CompanyId, HoldingId};
use erpx_service::field_attachment_intake::{
    FieldAttachmentIntakeAuthority, FieldAttachmentIntakeSources,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[tokio::test]
async fn actual_field_proof_creates_a_set_and_rejects_late_visit_version_change() {
    facts::account::with_system(|system| async move {
        let p = facts::setup_workspace(system).await;
        let clock = p.review.system.scenario.clock.clone();
        let catalogue = Arc::new(erpx_server_commercial::public::purchases::current_package_catalogue().unwrap());
        let created = Arc::new(AtomicBool::new(false));
        let observed = created.clone();
        let result = p.review.authorizer.execute_with_sources(
            "acme.example.test", p.review.maker.clone(), p.selection.clone(),
            WorkAuthorizationRequest { permission: "records.read".into(), module: Some("field_service".into()), destination: "service/orders".into(), api: true },
            FieldAttachmentIntakeSources,
            move |c, actor| { let clock = clock.clone(); let catalogue = catalogue.clone(); let observed = observed.clone(); Box::pin(async move {
                let holding = HoldingId::new("h001").unwrap();
                let company = CompanyId::new("company").unwrap();
                let authority = FieldAttachmentIntakeAuthority::new(actor, &holding, &company, &catalogue, clock.as_ref());
                let now = clock.now().get();
                let read = prepare_field_intake(c, &authority, actor, "issue-order", false, now).await?;
                let write = prepare_field_intake(c, &authority, actor, "issue-order", true, now).await?;
                let input = FieldSetRequest { target_type: "field_service_visit".into(), target_id: "issue-order".into(), component_kind: "record".into(), component_ref: None, purpose_key: "site_photo".into(), expected_target_version: read.scope().version };
                let ready = FieldIntakeAvailability { uploads_frozen: false, scanner_ready: true, preview_ready: true };
                let set = create_field_set(c, &read, Some(&write), &input, "actual-field-intake", &ready, now).await?;
                assert_eq!(set["targetId"], "issue-order");
                observed.store(true, Ordering::SeqCst);
                // Deliberate fixture corruption: the intake proof must notice a
                // Field version change even though bind permits its own advance.
                sqlx::query("UPDATE field_service_orders SET version=version+1 WHERE id='issue-order'").execute(&mut *c).await?;
                let rejected = read.require_current(c, now).await;
                assert!(matches!(&rejected, Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) if error.code == "SERVICE_ATTACHMENT_CHANGED"));
                rejected
            }) },
        ).await;
        assert!(created.load(Ordering::SeqCst), "actual Field proof did not reach Files creation");
        assert!(result.is_err());
        let effects: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM attachment_events WHERE company_id='company' AND action IN ('set.created','set.reused'))").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(effects, (0, 0));
    }).await;
}

#[tokio::test]
async fn actual_field_proof_commits_one_set_and_reuses_its_original_draft() {
    facts::account::with_system(|system| async move {
        let p = facts::setup_workspace(system).await;
        let clock = p.review.system.scenario.clock.clone();
        let catalogue = Arc::new(erpx_server_commercial::public::purchases::current_package_catalogue().unwrap());
        let mut saved = None;
        for (key, replay) in [("real-create", false), ("real-create", true), ("real-alias", false), ("real-alias", true)] {
            let clock = clock.clone();
            let catalogue = catalogue.clone();
            let value = p.review.authorizer.execute_with_sources(
                "acme.example.test", p.review.maker.clone(), p.selection.clone(),
                WorkAuthorizationRequest { permission: "records.read".into(), module: Some("field_service".into()), destination: "service/orders".into(), api: true },
                FieldAttachmentIntakeSources,
                move |c, actor| { let clock = clock.clone(); let catalogue = catalogue.clone(); Box::pin(async move {
                    let holding = HoldingId::new("h001").unwrap();
                    let company = CompanyId::new("company").unwrap();
                    let authority = FieldAttachmentIntakeAuthority::new(actor, &holding, &company, &catalogue, clock.as_ref());
                    let now = clock.now().get();
                    let read = prepare_field_intake(c, &authority, actor, "issue-order", false, now).await?;
                    let write = if replay { None } else { Some(prepare_field_intake(c, &authority, actor, "issue-order", true, now).await?) };
                    let input = FieldSetRequest { target_type: "field_service_visit".into(), target_id: "issue-order".into(), component_kind: "record".into(), component_ref: None, purpose_key: "site_photo".into(), expected_target_version: read.scope().version };
                    let ready = FieldIntakeAvailability { uploads_frozen: false, scanner_ready: true, preview_ready: true };
                    create_field_set(c, &read, write.as_ref(), &input, key, &ready, now).await
                }) },
            ).await.unwrap();
            if let Some(prior) = &saved { assert_eq!(prior, &value); } else { saved = Some(value); }
        }
        let counts: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM attachment_events WHERE action='set.created'),(SELECT count(*) FROM attachment_events WHERE action='set.reused')").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(counts, (1, 1, 1));
    }).await;
}

#[derive(Clone)]
struct FieldFileSavepointSources;
impl erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation for FieldFileSavepointSources {
    fn prepare<'c>(&'c self,c:&'c mut sqlx::PgConnection)->erpx_server_runtime::public::postgres::PgWorkFuture<'c,(),erpx_server_shared::public::BusinessError> {
        Box::pin(async move {
            sqlx::query("SELECT pg_current_xact_id()").execute(&mut *c).await?;
            sqlx::query("SAVEPOINT before_field_file_sources").execute(&mut *c).await?;
            erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation::prepare(&FieldAttachmentIntakeSources,c).await
        })
    }
}

#[tokio::test]
async fn field_file_source_rechecks_utility_locks_after_savepoint_rollback() {
    use erpx_server_field_service::public::file_source::read_field_file_source;
    facts::account::with_system(|system| async move {
        let p=facts::setup_workspace(system).await;
        let observed=Arc::new(AtomicBool::new(false));let checked=observed.clone();
        let _result=p.review.authorizer.execute_with_sources(
            "acme.example.test",p.review.maker.clone(),p.selection.clone(),
            WorkAuthorizationRequest{permission:"records.read".into(),module:Some("field_service".into()),destination:"service/orders".into(),api:true},
            FieldFileSavepointSources,
            move |c,actor| {let checked=checked.clone();Box::pin(async move {
                let modes:i64=sqlx::query_scalar("SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('field_service_orders'::regclass,'field_service_agreements'::regclass,'field_service_part_reservations'::regclass,'field_service_billing_releases'::regclass)").fetch_one(&mut *c).await?;
                assert_eq!(modes,4);
                let source=read_field_file_source(c,actor,"issue-order").await?;
                assert_eq!(source.facts().order_id,"issue-order");source.require_current(c,actor).await?;
                sqlx::query("ROLLBACK TO SAVEPOINT before_field_file_sources").execute(&mut *c).await?;
                assert!(source.require_current(c,actor).await.is_err());
                checked.store(true,Ordering::SeqCst);
                Ok(())
            })},
        ).await;
        assert!(observed.load(Ordering::SeqCst),"selected callback must reach the same-unit lock regression");
    }).await;
}

#[tokio::test]
async fn field_file_require_held_distinguishes_read_and_bind_modes() {
    use erpx_server_field_service::public::file_source::{FieldFileSourceLockMode as Mode,lock_field_file_bind_sources,lock_field_file_sources,require_field_file_sources_held};
    facts::account::with_system(|system| async move {
        let p=facts::setup_workspace(system).await;let db=p.pool().raw();
        let mut outside=db.acquire().await.unwrap();assert!(require_field_file_sources_held(&mut outside,Mode::Read).await.is_err());drop(outside);
        let mut partial=db.begin().await.unwrap();sqlx::query("LOCK TABLE field_service_orders,field_service_agreements,field_service_part_reservations IN SHARE ROW EXCLUSIVE MODE").execute(&mut *partial).await.unwrap();assert!(require_field_file_sources_held(&mut partial,Mode::Read).await.is_err());assert!(require_field_file_sources_held(&mut partial,Mode::Bind).await.is_err());partial.rollback().await.unwrap();
        let mut read=db.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *read).await.unwrap();lock_field_file_sources(&mut read).await.unwrap();require_field_file_sources_held(&mut read,Mode::Read).await.unwrap();assert!(require_field_file_sources_held(&mut read,Mode::Bind).await.is_err());read.rollback().await.unwrap();
        let mut tx=db.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *tx).await.unwrap();sqlx::query("SAVEPOINT before_field_bind_locks").execute(&mut *tx).await.unwrap();
        lock_field_file_bind_sources(&mut tx).await.unwrap();require_field_file_sources_held(&mut tx,Mode::Read).await.unwrap();require_field_file_sources_held(&mut tx,Mode::Bind).await.unwrap();
        let modes:i64=sqlx::query_scalar("SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode='ShareRowExclusiveLock' AND relation IN ('field_service_orders'::regclass,'field_service_agreements'::regclass,'field_service_part_reservations'::regclass,'field_service_billing_releases'::regclass)").fetch_one(&mut *tx).await.unwrap();assert_eq!(modes,4);
        sqlx::query("ROLLBACK TO SAVEPOINT before_field_bind_locks").execute(&mut *tx).await.unwrap();assert!(require_field_file_sources_held(&mut tx,Mode::Read).await.is_err());assert!(require_field_file_sources_held(&mut tx,Mode::Bind).await.is_err());
        sqlx::query("LOCK TABLE field_service_orders,field_service_agreements,field_service_part_reservations,field_service_billing_releases IN EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();require_field_file_sources_held(&mut tx,Mode::Bind).await.unwrap();tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn selected_field_bind_writer_rejects_read_only_source_fence() {
    use erpx_server_field_service::public::file_source::{bind_field_file_evidence,read_field_file_source};
    use erpx_server_runtime::public::postgres::PgTransactionError;
    facts::account::with_system(|system| async move {
        let p=facts::setup_workspace(system).await;let clock=p.review.system.scenario.clock.clone();
        p.review.authorizer.execute_with_sources(
            "acme.example.test",p.review.maker.clone(),p.selection.clone(),
            WorkAuthorizationRequest{permission:"records.read".into(),module:Some("field_service".into()),destination:"service/orders".into(),api:true},
            FieldAttachmentIntakeSources,
            move |c,actor| {let clock=clock.clone();Box::pin(async move {
                let source=read_field_file_source(c,actor,"issue-order").await?;let before=source.facts().version;
                let error=bind_field_file_evidence(c,actor,&source,"read-fence-must-not-write",before,clock.now().get()).await;
                assert!(matches!(error,Err(PgTransactionError::Persistence(ref e)) if e.code=="FIELD_FILE_SOURCE_LOCKS_REQUIRED"));
                let evidence:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT version FROM field_service_orders WHERE id='issue-order'),(SELECT count(*) FROM field_service_order_events WHERE id='service-file-event-read-fence-must-not-write'),(SELECT count(*) FROM command_receipts WHERE command_id='read-fence-must-not-write')").fetch_one(&mut *c).await?;
                assert_eq!(evidence,(before,0,0));Ok(())
            })},
        ).await.unwrap();
    }).await;
}

#[tokio::test]
async fn files_workflow_finance_batch_classifies_both_global_payment_branches() {
    use erpx_server_files::public::field_attachment_ancestry::{inspect_field_attachment_ancestry,lock_field_attachment_ancestry};
    use erpx_server_workflow::public::record_origin::{inspect_field_payment_receipt_batch,lock_payment_origin_candidate_sources};
    use erpx_server_finance::public::raw_field_payment_allocation_marker::{inspect_field_payment_origin_batch,lock_raw_field_payment_allocation_sources};
    // Uses the actual three owner ports on a migrated Holding DB. This is a
    // routing predicate test, not selected authorization or purchase evidence.
    facts::account::with_system(|system| async move {
        let p=facts::setup_workspace(system).await;let db=p.pool().raw();
        // Fixture Finance allocations deliberately remain uncommitted: the
        // deferred native Service writer guard stays enabled and is not bypassed.
        let mut tx=db.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_payment_origin_candidate_sources(&mut tx).await.unwrap();lock_raw_field_payment_allocation_sources(&mut tx).await.unwrap();lock_field_attachment_ancestry(&mut tx).await.unwrap();
        for (suffix,payment) in [("none","batch-none"),("null","batch-null"),("receipt","batch-receipt"),("alloc","batch-alloc"),("both","batch-both"),("shared","batch-none")] {
            let upload=format!("batch-upload-{suffix}");let attachment=format!("batch-source-{suffix}");let set=format!("batch-set-{suffix}");let link=format!("batch-link-{suffix}");
            sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,company_id) VALUES($1,'asset','fixture','manual','file','text/plain',1,$2,'uploaded','fixture',10000,0,0,'company')").bind(&upload).bind(format!("staging/{suffix}")).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id) VALUES($1,$2,'finance_payment',$3,'manual',$4,1,'file','text/plain',1,'checksum','fixture',0,'company')").bind(&attachment).bind(&upload).bind(payment).bind(format!("object/{suffix}")).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES($1,'company','asset',$2,'record','manual',1,1,'internal','{}','draft','fixture',0,0)").bind(&set).bind(format!("asset-{suffix}")).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES($1,$2,$3,'draft','fixture',0,10000)").bind(&link).bind(&set).bind(&attachment).execute(&mut *tx).await.unwrap();
        }
        // 1093 examines every source link globally, including a withdrawn
        // link belonging to another Company.
        sqlx::query("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('foreign-shared-set','foreign','finance_payment','batch-alloc','record','manual',1,1,'internal','{}','draft','fixture',0,0)").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,withdrawn_at,withdrawn_by,withdrawal_reason,created_by,created_at,expires_at) VALUES('foreign-shared-link','foreign-shared-set','batch-source-shared','withdrawn',1,'fixture','fixture','fixture',0,10000)").execute(&mut *tx).await.unwrap();
        // Each direct-only link has no attachment and refers to one payment
        // through exactly one selected set/upload ancestry path.
        sqlx::query("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('direct-payment-reference','foreign','finance_payment','batch-both','record','manual',1,1,'internal','{}','draft','fixture',0,0)").execute(&mut *tx).await.unwrap();
        for suffix in ["set","upload","upload-set"] {
            let set=format!("direct-payment-set-{suffix}");let upload=format!("direct-payment-upload-{suffix}");let link=format!("direct-payment-link-{suffix}");
            let (target_type,target)=if suffix=="set" {("finance_payment","batch-receipt".to_owned())} else {("asset",format!("direct-payment-asset-{suffix}"))};
            sqlx::query("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES($1,'company',$2,$3,'record','manual',1,1,'internal','{}','draft','fixture',0,0)").bind(&set).bind(target_type).bind(target).execute(&mut *tx).await.unwrap();
            let (owner_type,owner,reference)=if suffix=="upload" {("finance_payment","batch-alloc",None)} else if suffix=="upload-set" {("asset","unrelated",Some("direct-payment-reference"))} else {("asset","unrelated",None)};
            sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,company_id,attachment_set_id) VALUES($1,$2,$3,'manual','file','text/plain',1,$4,'uploaded','fixture',10000,0,0,'company',$5)").bind(&upload).bind(owner_type).bind(owner).bind(format!("direct-payment-staging/{suffix}")).bind(reference).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,upload_intent_id,state,created_by,created_at,expires_at) VALUES($1,$2,NULL,$3,'draft','fixture',0,10000)").bind(link).bind(set).bind(upload).execute(&mut *tx).await.unwrap();
        }
        sqlx::query("UPDATE attachment_sets SET target_type='finance_payment',target_id='batch-receipt',component_ref='source-receipt' WHERE id='batch-set-receipt'").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,created_by,created_at,updated_at) VALUES('batch-foreign-invoice','foreign','customer','receivable','BATCH-FIELD','2026-01-10','2026-01-10','THB',100,0,100,100,'cash','sales','field_service','fixture','fixture',0,0)").execute(&mut *tx).await.unwrap();
        for payment in ["batch-alloc","batch-both"] {
            sqlx::query("INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,created_by,created_at,updated_at) VALUES($1,'company','customer','receipt',$1,'2026-01-10','THB',100,100,'fixture',0,0)").bind(payment).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) VALUES($1,'foreign',$2,'batch-foreign-invoice',100,'fixture',0)").bind(format!("allocation-{payment}")).bind(payment).execute(&mut *tx).await.unwrap();
        }
        for (key,record,result) in [("batch-receipt-command","batch-receipt",r#"{"settlementInvoiceId":"batch-foreign-invoice"}"#),("batch-both-command","other-payment",r#"{"id":"batch-both","settlementInvoiceId":"batch-foreign-invoice"}"#),("batch-null-command","batch-null",r#"{"settlementInvoiceId":null}"#)] {
            sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES($1,'finance.payment.create','fixture','synthetic',$2,$3,0)").bind(key).bind(record).bind(result).execute(&mut *tx).await.unwrap();
        }
        for (suffix,receipt,allocation) in [("set",true,false),("upload",false,true),("upload-set",true,true)] {
            let ancestry=inspect_field_attachment_ancestry(&mut tx,&CompanyId::new("company").unwrap(),&format!("direct-payment-link-{suffix}")).await.unwrap().unwrap();
            let direct_batch=erpx_server_workflow::public::record_origin::inspect_field_direct_payment_receipt_batch(&mut tx,&ancestry).await.unwrap();
            let direct=erpx_server_finance::public::raw_field_payment_allocation_marker::inspect_direct_field_payment_origin(&mut tx,&direct_batch).await.unwrap();
            let source_batch=inspect_field_payment_receipt_batch(&mut tx,&ancestry).await.unwrap();
            let source=inspect_field_payment_origin_batch(&mut tx,&source_batch).await.unwrap();
            let payment=match suffix{"set"=>"batch-receipt","upload"=>"batch-alloc",_=>"batch-both"};
            let oracle:bool=sqlx::query_scalar("SELECT erpx_finance_field_payment_owned($1)").bind(payment).fetch_one(&mut *tx).await.unwrap();
            assert_eq!((direct.receipt_marked(),direct.allocation_marked()),(receipt,allocation),"direct {suffix}");assert_eq!(direct.marked(),oracle);assert!(!source.marked(),"direct must not enter attachment-source predicate {suffix}");
            direct.require_current(&mut tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
            if suffix=="set" {
                sqlx::query("SAVEPOINT direct_payment_drift").execute(&mut *tx).await.unwrap();
                // A new matching Workflow receipt changes the retained direct
                // batch without attempting an illegal Field invoice mutation.
                sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES('direct-payment-late','finance.payment.create','fixture','synthetic','batch-receipt','{\"settlementInvoiceId\":\"batch-foreign-invoice\"}',0)").execute(&mut *tx).await.unwrap();
                assert!(direct.require_current(&mut tx).await.is_err());
                sqlx::query("ROLLBACK TO SAVEPOINT direct_payment_drift").execute(&mut *tx).await.unwrap();direct.require_current(&mut tx).await.unwrap();
            }
        }
        for (suffix,direct_expected,source_expected) in [("receipt",true,true),("alloc",false,true),("shared",false,true)] {
            let ancestry=inspect_field_attachment_ancestry(&mut tx,&CompanyId::new("company").unwrap(),&format!("batch-link-{suffix}")).await.unwrap().unwrap();
            let direct_batch=erpx_server_workflow::public::record_origin::inspect_field_direct_payment_receipt_batch(&mut tx,&ancestry).await.unwrap();
            let direct=erpx_server_finance::public::raw_field_payment_allocation_marker::inspect_direct_field_payment_origin(&mut tx,&direct_batch).await.unwrap();
            let source_batch=inspect_field_payment_receipt_batch(&mut tx,&ancestry).await.unwrap();let source=inspect_field_payment_origin_batch(&mut tx,&source_batch).await.unwrap();
            assert_eq!((direct.marked(),source.marked()),(direct_expected,source_expected),"{suffix}");
            direct.require_current(&mut tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
        }
        for (suffix,receipt,allocation) in [("none",false,false),("null",false,false),("receipt",true,false),("alloc",false,true),("both",true,true)] {
            let ancestry=inspect_field_attachment_ancestry(&mut tx,&CompanyId::new("company").unwrap(),&format!("batch-link-{suffix}")).await.unwrap().unwrap();
            let batch=inspect_field_payment_receipt_batch(&mut tx,&ancestry).await.unwrap();
            let origin=inspect_field_payment_origin_batch(&mut tx,&batch).await.unwrap();
            assert_eq!((origin.receipt_marked(),origin.allocation_marked(),origin.marked()),(receipt,allocation,receipt||allocation),"{suffix}");origin.require_current(&mut tx).await.unwrap();
            if suffix=="none" {
                sqlx::query("SAVEPOINT late_batch_receipt").execute(&mut *tx).await.unwrap();
                sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES('batch-late','finance.payment.create','fixture','synthetic','batch-none','{\"settlementInvoiceId\":\"batch-foreign-invoice\"}',0)").execute(&mut *tx).await.unwrap();
                assert!(origin.require_current(&mut tx).await.is_err());
                sqlx::query("ROLLBACK TO SAVEPOINT late_batch_receipt").execute(&mut *tx).await.unwrap();origin.require_current(&mut tx).await.unwrap();
            }
        }
        tx.rollback().await.unwrap();
        let staged:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_allocations WHERE id IN ('allocation-batch-alloc','allocation-batch-both')),(SELECT count(*) FROM command_receipts WHERE command_id IN ('batch-receipt-command','batch-both-command','batch-null-command')),(SELECT count(*) FROM attachment_links WHERE id LIKE 'batch-link-%')").fetch_one(db).await.unwrap();
        assert_eq!(staged,(0,0,0));
    }).await;
}

#[tokio::test]
async fn files_finance_inventory_batch_classifies_global_field_journals() {
    use erpx_server_files::public::field_attachment_ancestry::{CurrentFieldAttachmentAncestry,inspect_field_attachment_ancestry,lock_field_attachment_ancestry};
    use erpx_server_workflow::public::record_origin::FieldPaymentTargets;
    use erpx_server_inventory::public::field_journal_consumption_marker::{FieldJournalTargets,inspect_field_journal_consumption_batch,lock_field_journal_consumption_batch_sources};
    use erpx_server_finance::public::raw_field_journal_marker::{inspect_field_journal_marker_batch,lock_raw_field_journal_marker_sources_nowait};
    struct InventorySource<'a>(&'a CurrentFieldAttachmentAncestry);
    impl FieldJournalTargets for InventorySource<'_> {
        fn relation_identifier(&self)->&str{FieldPaymentTargets::relation_identifier(self.0)}
        fn require_current<'a>(&'a self,c:&'a mut sqlx::PgConnection)->erpx_server_runtime::public::postgres::PgWorkFuture<'a,(),erpx_server_shared::public::BusinessError>{
            Box::pin(async move{self.0.require_current(c).await})
        }
    }
    // Raw routing evidence only. Deferred owner write guards remain enabled;
    // deliberately staged synthetic operational lineage is never committed.
    facts::account::with_system(|system| async move {
        let p=facts::setup_workspace(system).await;let db=p.pool().raw();
        let mut tx=db.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_raw_field_journal_marker_sources_nowait(&mut tx).await.unwrap();lock_field_journal_consumption_batch_sources(&mut tx).await.unwrap();lock_field_attachment_ancestry(&mut tx).await.unwrap();
        for (suffix,journal) in [("none","journal-none"),("finance","journal-finance"),("inventory","journal-inventory"),("both","journal-both")] {
            let upload=format!("journal-upload-{suffix}");let attachment=format!("journal-source-{suffix}");let set=format!("journal-set-{suffix}");let link=format!("journal-link-{suffix}");
            sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,company_id) VALUES($1,'asset','fixture','manual','file','text/plain',1,$2,'uploaded','fixture',10000,0,0,'company')").bind(&upload).bind(format!("journal-staging/{suffix}")).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id) VALUES($1,$2,'finance_journal',$3,'manual',$4,1,'file','text/plain',1,'checksum','fixture',0,'company')").bind(&attachment).bind(&upload).bind(journal).bind(format!("journal-object/{suffix}")).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES($1,'company','asset',$2,'record','manual',1,1,'internal','{}','draft','fixture',0,0)").bind(&set).bind(format!("journal-asset-{suffix}")).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES($1,$2,$3,'draft','fixture',0,10000)").bind(&link).bind(&set).bind(&attachment).execute(&mut *tx).await.unwrap();
        }
        sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES('journal-link-null','journal-set-none',NULL,'draft','fixture',0,10000)").execute(&mut *tx).await.unwrap();
        {
            let ancestry=inspect_field_attachment_ancestry(&mut tx,&CompanyId::new("company").unwrap(),"journal-link-null").await.unwrap().unwrap();
            let adapter=InventorySource(&ancestry);
            let f=inspect_field_journal_marker_batch(&mut tx,&ancestry).await.unwrap();let i=inspect_field_journal_consumption_batch(&mut tx,&adapter).await.unwrap();
            assert!(!f.marked() && !i.present());f.require_current(&mut tx).await.unwrap();i.require_current(&mut tx).await.unwrap();
        }
        // Selected-link direct paths are deliberately separate from the six
        // attachment-source paths; these links have no attachment.
        sqlx::query("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('direct-journal-reference','foreign','finance_journal','journal-finance','record','manual',1,1,'internal','{}','draft','fixture',0,0)").execute(&mut *tx).await.unwrap();
        for suffix in ["set","upload","upload-set"] {
            let set=format!("direct-journal-set-{suffix}");let upload=format!("direct-journal-upload-{suffix}");let link=format!("direct-journal-link-{suffix}");
            let (target_type,target)=if suffix=="set" {("finance_journal","journal-finance".to_owned())} else {("asset",format!("unrelated-{suffix}"))};
            sqlx::query("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES($1,'company',$2,$3,'record','manual',1,1,'internal','{}','draft','fixture',0,0)").bind(&set).bind(target_type).bind(target).execute(&mut *tx).await.unwrap();
            let (owner_type,owner,reference)=if suffix=="upload" {("finance_journal","journal-finance",None)} else if suffix=="upload-set" {("asset","unrelated",Some("direct-journal-reference"))} else {("asset","unrelated",None)};
            sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,company_id,attachment_set_id) VALUES($1,$2,$3,'manual','file','text/plain',1,$4,'uploaded','fixture',10000,0,0,'company',$5)").bind(&upload).bind(owner_type).bind(owner).bind(format!("direct-journal-staging/{suffix}")).bind(reference).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,upload_intent_id,state,created_by,created_at,expires_at) VALUES($1,$2,NULL,$3,'draft','fixture',0,10000)").bind(link).bind(set).bind(upload).execute(&mut *tx).await.unwrap();
        }
        // Add both Inventory-only and combined markers on each direct path.
        // All rows are staged in this rollback-only unit before journals exist.
        for mode in ["inventory","both"] {
            let journal=if mode=="inventory"{"journal-inventory"}else{"journal-both"};
            let reference=format!("direct-journal-reference-{mode}");
            sqlx::query("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES($1,'foreign','finance_journal',$2,'record','manual',1,1,'internal','{}','draft','fixture',0,0)").bind(&reference).bind(journal).execute(&mut *tx).await.unwrap();
            for path in ["set","upload","upload-set"] {
                let set=format!("inv-direct-set-{mode}-{path}");let upload=format!("inv-direct-upload-{mode}-{path}");let link=format!("inv-direct-link-{mode}-{path}");
                let (kind,target)=if path=="set" {("finance_journal",journal.to_owned())}else{("asset",format!("inv-direct-asset-{mode}-{path}"))};
                sqlx::query("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES($1,'company',$2,$3,'record','manual',1,1,'internal','{}','draft','fixture',0,0)").bind(&set).bind(kind).bind(target).execute(&mut *tx).await.unwrap();
                let (owner_type,owner)=if path=="upload"{("finance_journal",journal)}else{("asset","unrelated")};let upload_set=if path=="upload-set"{Some(reference.as_str())}else{None};
                sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,company_id,attachment_set_id) VALUES($1,$2,$3,'manual','file','text/plain',1,$4,'uploaded','fixture',10000,0,0,'company',$5)").bind(&upload).bind(owner_type).bind(owner).bind(format!("inv-direct-staging/{mode}/{path}")).bind(upload_set).execute(&mut *tx).await.unwrap();
                sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,upload_intent_id,state,created_by,created_at,expires_at) VALUES($1,$2,NULL,$3,'draft','fixture',0,10000)").bind(link).bind(set).bind(upload).execute(&mut *tx).await.unwrap();
            }
        }
        sqlx::query("UPDATE attachment_sets SET target_type='finance_journal',target_id='journal-both',component_ref='source-both' WHERE id='journal-set-both'").execute(&mut *tx).await.unwrap();
        for (id,source) in [("journal-finance","field_service_parts"),("journal-inventory","manual"),("journal-both","field_service_parts")] {
            sqlx::query("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,memo,status,created_by,created_at,updated_at) VALUES($1,'foreign','chart','period',$1,'2026-01-10','THB',$2,'fixture','fixture','draft','fixture',0,0)").bind(id).bind(source).execute(&mut *tx).await.unwrap();
        }
        for id in ["journal-inventory","journal-both"] {
            sqlx::query("INSERT INTO operational_inventory_consumptions(id,company_id,branch_id,source_kind,source_id,source_version,warehouse_id,product_id,quantity_milli,value_minor,currency,movement_date,journal_id,reason,created_by,created_at) VALUES($1,'foreign','branch','field_service','fixture',1,'warehouse','product',1,0,'THB','2026-01-10',$2,'fixture','fixture',0)").bind(format!("consumption-{id}")).bind(id).execute(&mut *tx).await.unwrap();
        }
        for suffix in ["set","upload","upload-set"] {
            let ancestry=inspect_field_attachment_ancestry(&mut tx,&CompanyId::new("company").unwrap(),&format!("direct-journal-link-{suffix}")).await.unwrap().unwrap();
            let direct=erpx_server_finance::public::raw_field_journal_marker::inspect_direct_field_journal_marker(&mut tx,&ancestry).await.unwrap();
            let source=inspect_field_journal_marker_batch(&mut tx,&ancestry).await.unwrap();
            let oracle:bool=sqlx::query_scalar("SELECT erpx_finance_field_journal_owned('journal-finance',NULL)").fetch_one(&mut *tx).await.unwrap();
            assert_eq!(direct.marked(),oracle,"direct journal {suffix}");assert!(oracle);assert!(!source.marked(),"direct must not enter source predicate {suffix}");
            direct.require_current(&mut tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
            if suffix=="set" {
                sqlx::query("SAVEPOINT direct_journal_drift").execute(&mut *tx).await.unwrap();
                sqlx::query("UPDATE finance_journals SET source_type='manual' WHERE id='journal-finance'").execute(&mut *tx).await.unwrap();
                assert!(direct.require_current(&mut tx).await.is_err());
                sqlx::query("ROLLBACK TO SAVEPOINT direct_journal_drift").execute(&mut *tx).await.unwrap();direct.require_current(&mut tx).await.unwrap();
            }
        }
        for mode in ["inventory","both"] {
            for path in ["set","upload","upload-set"] {
                let ancestry=inspect_field_attachment_ancestry(&mut tx,&CompanyId::new("company").unwrap(),&format!("inv-direct-link-{mode}-{path}")).await.unwrap().unwrap();let adapter=InventorySource(&ancestry);
                let f=erpx_server_finance::public::raw_field_journal_marker::inspect_direct_field_journal_marker(&mut tx,&ancestry).await.unwrap();
                let i=inspect_field_journal_consumption_batch(&mut tx,&adapter).await.unwrap();
                assert_eq!((f.marked(),i.direct_present(),i.present()),(mode=="both",true,false),"{mode}/{path}");
                let journal=if mode=="both"{"journal-both"}else{"journal-inventory"};
                let (oracle,inventory_oracle):(bool,bool)=sqlx::query_as("SELECT erpx_finance_field_journal_owned($1,NULL),EXISTS(SELECT 1 FROM operational_inventory_consumptions p WHERE p.journal_id=$1 AND p.source_kind='field_service')").bind(journal).fetch_one(&mut *tx).await.unwrap();
                assert_eq!(i.direct_present(),inventory_oracle,"inventory oracle {mode}/{path}");
                assert_eq!(f.marked()||i.direct_present(),oracle,"1097 oracle {mode}/{path}");
                f.require_current(&mut tx).await.unwrap();i.require_current(&mut tx).await.unwrap();
            }
        }
        for (suffix,source,direct) in [("both",true,true),("inventory",true,false)] {
            let ancestry=inspect_field_attachment_ancestry(&mut tx,&CompanyId::new("company").unwrap(),&format!("journal-link-{suffix}")).await.unwrap().unwrap();let adapter=InventorySource(&ancestry);
            let i=inspect_field_journal_consumption_batch(&mut tx,&adapter).await.unwrap();
            assert_eq!((i.present(),i.direct_present()),(source,direct),"{suffix}");i.require_current(&mut tx).await.unwrap();
        }
        for (suffix,finance,inventory) in [("none",false,false),("finance",true,false),("inventory",false,true),("both",true,true)] {
            let ancestry=inspect_field_attachment_ancestry(&mut tx,&CompanyId::new("company").unwrap(),&format!("journal-link-{suffix}")).await.unwrap().unwrap();
            let adapter=InventorySource(&ancestry);
            let f=inspect_field_journal_marker_batch(&mut tx,&ancestry).await.unwrap();
            let i=inspect_field_journal_consumption_batch(&mut tx,&adapter).await.unwrap();
            assert_eq!((f.marked(),i.present()),(finance,inventory),"{suffix}");f.require_current(&mut tx).await.unwrap();i.require_current(&mut tx).await.unwrap();
            if suffix=="none" {
                sqlx::query("SAVEPOINT journal_marker_late").execute(&mut *tx).await.unwrap();
                sqlx::query("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,memo,status,created_by,created_at,updated_at) VALUES('journal-none','foreign','chart','period','J-NONE','2026-01-10','THB','field_service_parts','fixture','fixture','draft','fixture',0,0)").execute(&mut *tx).await.unwrap();
                assert!(f.require_current(&mut tx).await.is_err());i.require_current(&mut tx).await.unwrap();
                sqlx::query("ROLLBACK TO SAVEPOINT journal_marker_late").execute(&mut *tx).await.unwrap();f.require_current(&mut tx).await.unwrap();
            }
        }
        tx.rollback().await.unwrap();
        let persisted:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_journals WHERE id IN ('journal-finance','journal-inventory','journal-both')),(SELECT count(*) FROM operational_inventory_consumptions WHERE id IN ('consumption-journal-inventory','consumption-journal-both')),(SELECT count(*) FROM attachment_links WHERE id LIKE 'journal-link-%')").fetch_one(db).await.unwrap();assert_eq!(persisted,(0,0,0));
    }).await;
}

#[tokio::test]
async fn no_temp_files_workflow_finance_routing_retains_separate_path_vectors() {
    use erpx_server_files::public::field_attachment_routing::{inspect_field_attachment_routing,lock_field_attachment_routing_sources};
    use erpx_server_workflow::public::record_origin::{inspect_field_payment_routing,lock_field_payment_routing_sources};
    use erpx_server_finance::public::field_attachment_routing::{inspect_finance_field_routing,lock_field_attachment_finance_routing,PathFlags};
    use erpx_server_inventory::public::field_journal_consumption_marker::{FieldJournalRoutingSource,inspect_field_journal_routing,lock_field_journal_routing_sources};
    struct InventorySource<'a>(&'a erpx_server_files::public::field_attachment_routing::CurrentFieldAttachmentRouting);
    impl FieldJournalRoutingSource for InventorySource<'_> {
        fn selected_link_id(&self)->&str{self.0.selected_link_id()}
        fn company_id(&self)->&str{self.0.company_id()}
        fn ancestry_sql(&self)->&'static str{erpx_server_files::public::field_attachment_routing::FIELD_ATTACHMENT_ANCESTRY_SQL}
        fn require_current<'a>(&'a self,c:&'a mut sqlx::PgConnection)->erpx_server_runtime::public::postgres::PgWorkFuture<'a,(),erpx_server_shared::public::BusinessError>{Box::pin(async move{self.0.require_current(c).await})}
    }
    // Owner-proof fixture uses the existing test administrator. This proves no
    // TEMP is used in the path, not restricted application-role permissions.
    // Finance allocations remain in this rollback-only unit with all guards on.
    facts::account::with_system(|system|async move {
        let p=facts::setup_workspace(system).await;let db=p.pool().raw();
        let mut tx=db.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_field_payment_routing_sources(&mut tx).await.unwrap();lock_field_attachment_finance_routing(&mut tx).await.unwrap();lock_field_journal_routing_sources(&mut tx).await.unwrap();lock_field_attachment_routing_sources(&mut tx).await.unwrap();
        sqlx::query("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,created_by,created_at,updated_at) VALUES('nt-field-invoice','foreign','customer','receivable','NT-FIELD','2026-01-10','2026-01-10','THB',100,0,100,100,'cash','sales','field_service','fixture','fixture',0,0)").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,created_by,created_at,updated_at) VALUES('nt-allocated','company','customer','receipt','NT-PAY','2026-01-10','THB',100,100,'fixture',0,0)").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) VALUES('nt-allocation','foreign','nt-allocated','nt-field-invoice',100,'fixture',0)").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,memo,status,created_by,created_at,updated_at) VALUES('nt-journal','foreign','chart','period','NT-JOURNAL','2026-01-10','THB','field_service_parts','fixture','fixture','draft','fixture',0,0)").execute(&mut *tx).await.unwrap();
        // Synthetic consumption remains uncommitted; deferred Field owner
        // checks stay enabled and are not represented as valid lineage.
        sqlx::query("INSERT INTO operational_inventory_consumptions(id,company_id,branch_id,source_kind,source_id,source_version,warehouse_id,product_id,quantity_milli,value_minor,currency,movement_date,journal_id,reason,created_by,created_at) VALUES('nt-inventory','foreign','branch','field_service','fixture',1,'warehouse','product',1,0,'THB','2026-01-10','nt-journal','fixture','fixture',0)").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES('nt-receipt','finance.payment.create','fixture','fixture','nt-payment','{\"settlementInvoiceId\":\"nt-field-invoice\"}',0)").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('nt-foreign-set','foreign','finance_journal','nt-journal','record','manual',1,1,'internal','{}','draft','fixture',0,0)").execute(&mut *tx).await.unwrap();
        for path in ["set","upload","upload-set","none"] {
            let set=format!("nt-set-{path}");let upload=format!("nt-upload-{path}");let link=format!("nt-link-{path}");
            let (kind,target)=if path=="set" {("finance_payment","nt-payment")} else if path=="none" {("finance_payment","nt-late")} else {("asset",path)};
            sqlx::query("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES($1,'company',$2,$3,'record','manual',1,1,'internal','{}','draft','fixture',0,0)").bind(&set).bind(kind).bind(target).execute(&mut *tx).await.unwrap();
            let (owner,owner_id,other_set)=if path=="upload" {("finance_payment","nt-allocated",None)} else if path=="upload-set" {("asset","unrelated",Some("nt-foreign-set"))}else{("asset","unrelated",None)};
            sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,company_id,attachment_set_id) VALUES($1,$2,$3,'manual','file','text/plain',1,$4,'uploaded','fixture',10000,0,0,'company',$5)").bind(&upload).bind(owner).bind(owner_id).bind(format!("nt/{path}")).bind(other_set).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,upload_intent_id,state,created_by,created_at,expires_at) VALUES($1,$2,NULL,$3,'draft','fixture',0,10000)").bind(&link).bind(set).bind(upload).execute(&mut *tx).await.unwrap();
        }
        let temp_before:i64=sqlx::query_scalar("SELECT count(*) FROM pg_class WHERE relnamespace=pg_my_temp_schema()").fetch_one(&mut *tx).await.unwrap();
        let no=PathFlags{direct_set:false,direct:false,source_attachment:false,source:false};
        for (path,receipt,allocation,journal) in [("set",true,false,false),("upload",false,true,false),("upload-set",false,false,true),("none",false,false,false)] {
            let ancestry=inspect_field_attachment_routing(&mut tx,&erpx_server_shared::public::CompanyId::new("company").unwrap(),&format!("nt-link-{path}")).await.unwrap().unwrap();
            let workflow=inspect_field_payment_routing(&mut tx,&ancestry).await.unwrap();let finance=inspect_finance_field_routing(&mut tx,&workflow).await.unwrap();let v=finance.vector();let inventory_source=InventorySource(&ancestry);let inventory=inspect_field_journal_routing(&mut tx,&inventory_source).await.unwrap();let iv=inventory.vector();assert_eq!((iv.direct_set,iv.direct,iv.source,iv.source_attachment),(false,journal,false,false));inventory.require_current(&mut tx).await.unwrap();
            assert_eq!(v.payment_receipt,PathFlags{direct_set:path=="set" && receipt,direct:receipt,source_attachment:false,source:false},"{path}");assert_eq!(v.payment_allocation,PathFlags{direct_set:false,direct:allocation,source_attachment:false,source:false},"{path}");assert_eq!(v.journal,PathFlags{direct_set:false,direct:journal,source_attachment:false,source:false},"{path}");
            let oracle:bool=if path=="upload-set" {sqlx::query_scalar("SELECT erpx_finance_field_journal_owned('nt-journal',NULL)").fetch_one(&mut *tx).await.unwrap()} else {sqlx::query_scalar("SELECT erpx_finance_field_payment_owned($1)").bind(if path=="set"{"nt-payment"}else if path=="upload"{"nt-allocated"}else{"nt-late"}).fetch_one(&mut *tx).await.unwrap()};assert_eq!(oracle,receipt||allocation||journal);finance.require_current(&mut tx).await.unwrap();
            if path=="none" {assert_eq!(v.payment_receipt,no);sqlx::query("SAVEPOINT nt_late").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES('nt-late-command','finance.payment.create','fixture','fixture','nt-late','{\"settlementInvoiceId\":\"nt-field-invoice\"}',0)").execute(&mut *tx).await.unwrap();assert!(finance.require_current(&mut tx).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT nt_late").execute(&mut *tx).await.unwrap();finance.require_current(&mut tx).await.unwrap();}
        }
        // Source-only routing comes from the persisted attachment owner even
        // when the selected link/set itself is an unrelated target.
        sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,company_id) VALUES('nt-source-upload','asset','other','manual','file','text/plain',1,'nt/source','uploaded','fixture',10000,0,0,'company')").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id) VALUES('nt-source','nt-source-upload','finance_payment','nt-payment','manual','nt/source-object',1,'file','text/plain',1,'checksum','fixture',0,'company')").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('nt-source-set','company','asset','unrelated','record','manual',1,1,'internal','{}','draft','fixture',0,0)").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES('nt-source-link','nt-source-set','nt-source','draft','fixture',0,10000)").execute(&mut *tx).await.unwrap();
        let ancestry=inspect_field_attachment_routing(&mut tx,&erpx_server_shared::public::CompanyId::new("company").unwrap(),"nt-source-link").await.unwrap().unwrap();let workflow=inspect_field_payment_routing(&mut tx,&ancestry).await.unwrap();let finance=inspect_finance_field_routing(&mut tx,&workflow).await.unwrap();let vector=finance.vector();
        assert_eq!(vector.payment_receipt,PathFlags{direct_set:false,direct:false,source_attachment:true,source:true});assert_eq!(vector.payment_allocation,no);assert_eq!(vector.journal,no);
        let oracle:bool=sqlx::query_scalar("SELECT erpx_files_field_attachment_owned('nt-source')").fetch_one(&mut *tx).await.unwrap();assert!(oracle);finance.require_current(&mut tx).await.unwrap();
        // A distinct real Files source carries the same journal through the
        // attachment branch, independent of direct selected-link routing.
        sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,company_id) VALUES('nt-journal-source-upload','asset','other','manual','file','text/plain',1,'nt/journal-source','uploaded','fixture',10000,0,0,'company')").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id) VALUES('nt-journal-source','nt-journal-source-upload','finance_journal','nt-journal','manual','nt/journal-source-object',1,'file','text/plain',1,'checksum','fixture',0,'company')").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES('nt-journal-source-link','nt-source-set','nt-journal-source','draft','fixture',0,10000)").execute(&mut *tx).await.unwrap();
        let journal_source=inspect_field_attachment_routing(&mut tx,&erpx_server_shared::public::CompanyId::new("company").unwrap(),"nt-journal-source-link").await.unwrap().unwrap();let journal_workflow=inspect_field_payment_routing(&mut tx,&journal_source).await.unwrap();let journal_finance=inspect_finance_field_routing(&mut tx,&journal_workflow).await.unwrap();let journal_adapter=InventorySource(&journal_source);let journal_inventory=inspect_field_journal_routing(&mut tx,&journal_adapter).await.unwrap();
        assert_eq!(journal_finance.vector().journal,PathFlags{direct_set:false,direct:false,source_attachment:true,source:true});let iv=journal_inventory.vector();assert!(!iv.direct_set && !iv.direct && iv.source && iv.source_attachment);journal_finance.require_current(&mut tx).await.unwrap();journal_inventory.require_current(&mut tx).await.unwrap();
        let legacy:bool=sqlx::query_scalar("SELECT erpx_files_field_attachment_owned('nt-journal-source')").fetch_one(&mut *tx).await.unwrap();assert!(legacy);
        // A foreign withdrawn link may make the global source predicate true,
        // but it is not the selected link's direct attachment owner.
        sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,company_id) VALUES('nt-shared-upload','asset','other','manual','file','text/plain',1,'nt/shared','uploaded','fixture',10000,0,0,'company')").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id) VALUES('nt-shared','nt-shared-upload','asset','other','manual','nt/shared-object',1,'file','text/plain',1,'checksum','fixture',0,'company')").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('nt-foreign-payment-set','foreign','finance_payment','nt-payment','record','manual',1,1,'internal','{}','draft','fixture',0,0)").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES('nt-shared-selected','nt-source-set','nt-shared','draft','fixture',0,10000)").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at,withdrawn_at,withdrawn_by,withdrawal_reason) VALUES('nt-shared-foreign-payment','nt-foreign-payment-set','nt-shared','withdrawn','fixture',0,10000,1,'fixture','fixture'),('nt-shared-foreign-journal','nt-foreign-set','nt-shared','withdrawn','fixture',0,10000,1,'fixture','fixture')").execute(&mut *tx).await.unwrap();
        let shared=inspect_field_attachment_routing(&mut tx,&erpx_server_shared::public::CompanyId::new("company").unwrap(),"nt-shared-selected").await.unwrap().unwrap();let shared_workflow=inspect_field_payment_routing(&mut tx,&shared).await.unwrap();let shared_finance=inspect_finance_field_routing(&mut tx,&shared_workflow).await.unwrap();let shared_vector=shared_finance.vector();
        assert_eq!(shared_vector.payment_receipt,PathFlags{direct_set:false,direct:false,source_attachment:false,source:true});assert_eq!(shared_vector.payment_allocation,no);assert_eq!(shared_vector.journal,PathFlags{direct_set:false,direct:false,source_attachment:false,source:true});
        let shared_adapter=InventorySource(&shared);let shared_inventory=inspect_field_journal_routing(&mut tx,&shared_adapter).await.unwrap();let shared_iv=shared_inventory.vector();
        assert!(!shared_iv.direct_set && !shared_iv.direct && shared_iv.source && !shared_iv.source_attachment);
        shared_inventory.require_current(&mut tx).await.unwrap();shared_finance.require_current(&mut tx).await.unwrap();let legacy:bool=sqlx::query_scalar("SELECT erpx_files_field_attachment_owned('nt-shared')").fetch_one(&mut *tx).await.unwrap();assert!(legacy);
        // A direct journal and payment on the same link prove legacy priority;
        // a separate FieldVisit-only link proves that branch independently.
        sqlx::query("INSERT INTO public.attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('nt-dual-set','company','finance_journal','nt-journal','record','manual',1,1,'internal','{}','draft','fixture',0,0),('nt-visit-set','company','field_service_visit','nt-visit','record','manual',1,1,'internal','{}','draft','fixture',0,0)").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO public.upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,company_id) VALUES('nt-dual-upload','finance_payment','nt-allocated','manual','file','text/plain',1,'nt/dual','uploaded','fixture',10000,0,0,'company')").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO public.attachment_links(id,set_id,attachment_id,upload_intent_id,state,created_by,created_at,expires_at) VALUES('nt-dual-link','nt-dual-set',NULL,'nt-dual-upload','draft','fixture',0,10000),('nt-visit-link','nt-visit-set',NULL,NULL,'draft','fixture',0,10000)").execute(&mut *tx).await.unwrap();
        // This is the actual legacy link selector shape, including separate
        // global marker and selected Company scope; not an authority oracle.
        let selector_sql=r#"SELECT
          EXISTS(SELECT 1 FROM public.attachment_links l LEFT JOIN public.attachment_sets s ON s.id=l.set_id LEFT JOIN public.upload_intents u ON u.id=l.upload_intent_id LEFT JOIN public.attachment_sets us ON us.id=u.attachment_set_id LEFT JOIN public.attachments a ON a.id=l.attachment_id WHERE l.id=$1 AND ((s.target_type='finance_journal' AND public.erpx_finance_field_journal_owned(s.target_id,NULL)) OR (u.owner_type='finance_journal' AND public.erpx_finance_field_journal_owned(u.owner_id,NULL)) OR (us.target_type='finance_journal' AND public.erpx_finance_field_journal_owned(us.target_id,NULL)) OR (a.owner_type='finance_journal' AND public.erpx_finance_field_journal_owned(a.owner_id,NULL)))),
          EXISTS(SELECT 1 FROM public.attachment_links l LEFT JOIN public.attachment_sets s ON s.id=l.set_id LEFT JOIN public.upload_intents u ON u.id=l.upload_intent_id LEFT JOIN public.attachment_sets us ON us.id=u.attachment_set_id LEFT JOIN public.attachments a ON a.id=l.attachment_id WHERE l.id=$1 AND ((s.target_type='finance_payment' AND public.erpx_finance_field_payment_owned(s.target_id)) OR (u.owner_type='finance_payment' AND public.erpx_finance_field_payment_owned(u.owner_id)) OR (us.target_type='finance_payment' AND public.erpx_finance_field_payment_owned(us.target_id)) OR (a.owner_type='finance_payment' AND public.erpx_finance_field_payment_owned(a.owner_id)))),
          EXISTS(SELECT 1 FROM public.attachment_links l LEFT JOIN public.attachment_sets s ON s.id=l.set_id LEFT JOIN public.upload_intents u ON u.id=l.upload_intent_id LEFT JOIN public.attachment_sets us ON us.id=u.attachment_set_id WHERE l.id=$1 AND (s.target_type='field_service_visit' OR u.owner_type='field_service_visit' OR us.target_type='field_service_visit' OR public.erpx_files_field_attachment_owned(l.attachment_id))),
          EXISTS(SELECT 1 FROM public.attachment_links l JOIN public.attachment_sets s ON s.id=l.set_id WHERE l.id=$1 AND s.company_id=$2)"#;
        for (id,expected) in [("nt-link-set","payment"),("nt-link-upload","payment"),("nt-link-upload-set","journal"),("nt-link-none","other"),("nt-source-link","payment"),("nt-journal-source-link","journal"),("nt-shared-selected","field"),("nt-dual-link","journal"),("nt-visit-link","field")] {
            let (journal,payment,field,scoped):(bool,bool,bool,bool)=sqlx::query_as(selector_sql).bind(id).bind("company").fetch_one(&mut *tx).await.unwrap();assert!(scoped,"{id}");
            let ancestry=inspect_field_attachment_routing(&mut tx,&erpx_server_shared::public::CompanyId::new("company").unwrap(),id).await.unwrap().unwrap();
            let workflow=inspect_field_payment_routing(&mut tx,&ancestry).await.unwrap();let finance=inspect_finance_field_routing(&mut tx,&workflow).await.unwrap();let f=finance.vector();
            let adapter=InventorySource(&ancestry);let inventory=inspect_field_journal_routing(&mut tx,&adapter).await.unwrap();let i=inventory.vector();let visit=ancestry.field_visit_routing();
            let actual_journal=f.journal.direct||f.journal.source_attachment||i.direct||i.source_attachment;
            let actual_payment=f.payment_receipt.direct||f.payment_receipt.source_attachment||f.payment_allocation.direct||f.payment_allocation.source_attachment;
            let actual_field=visit.direct||visit.source||f.payment_receipt.source||f.payment_allocation.source||f.journal.source||i.source;
            assert_eq!((actual_journal,actual_payment,actual_field),(journal,payment,field),"legacy selector bits: {id}");
            let chosen=if actual_journal{"journal"}else if actual_payment{"payment"}else if actual_field{"field"}else{"other"};assert_eq!(chosen,expected,"legacy precedence: {id}");
            ancestry.require_current(&mut tx).await.unwrap();workflow.require_current(&mut tx).await.unwrap();finance.require_current(&mut tx).await.unwrap();inventory.require_current(&mut tx).await.unwrap();
        }
        for (id,expected_marker) in [("nt-shared-foreign-journal",true),("nt-missing-selector",false)] {
            let (journal,payment,field,scoped):(bool,bool,bool,bool)=sqlx::query_as(selector_sql).bind(id).bind("company").fetch_one(&mut *tx).await.unwrap();
            assert_eq!(journal||payment||field,expected_marker,"global marker {id}");assert!(!scoped,"foreign/missing {id}");
            assert!(inspect_field_attachment_routing(&mut tx,&erpx_server_shared::public::CompanyId::new("company").unwrap(),id).await.unwrap().is_none());
        }
        let temp_after:i64=sqlx::query_scalar("SELECT count(*) FROM pg_class WHERE relnamespace=pg_my_temp_schema()").fetch_one(&mut *tx).await.unwrap();assert_eq!(temp_after,temp_before,"routing must not create temp relations");
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn initial_count_terminal_history_matches_actual_supply_sql_and_version_chain() {
    use erpx_server_files::public::count_history_attachment_sources::*;
    use erpx_server_inventory::public::{count_terminal_history_source::*,initial_count_history_source::*};
    use erpx_server_workflow::public::record_origin::*;
    use erpx_service::initial_count_terminal_history::inspect_initial_count_terminal_history;
    facts::account::with_system(|system| async move {
        let p=facts::setup_workspace(system).await;
        let mut tx=p.pool().raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_initial_count_history_sources(&mut tx).await.unwrap();lock_count_terminal_history_source(&mut tx).await.unwrap();lock_initial_count_history_receipt_sources(&mut tx).await.unwrap();lock_count_history_attachment_sources(&mut tx).await.unwrap();
        let counts=inspect_initial_count_history_source(&mut tx).await.unwrap();let decisions=inspect_count_terminal_history_source(&mut tx).await.unwrap();let receipts=inspect_initial_count_history_receipt_join(&mut tx).await.unwrap();let files=inspect_count_history_attachment_sources(&mut tx).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        for (id,version,status,action,auth,result_version) in [("terminal-direct",2_i64,"posted","post","company",2_i64),("terminal-chain",4,"posted","post","company",2),("terminal-gap",4,"posted","post","company",2),("terminal-wrong-actor",3,"posted","post","company",2),("terminal-cancel",2,"cancelled","cancel","company",2),("terminal-reject",2,"rejected","reject","company",2),("terminal-bad-auth",2,"posted","post","foreign",2),("terminal-bad-version",2,"posted","post","company",3)] {
            sqlx::query("INSERT INTO public.inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,created_by,prepared_by,created_at,updated_at) VALUES($1,'company','branch','warehouse',$1,'2026-01-10','fixture',$2,$3,'maker','maker',0,0)").bind(id).bind(status).bind(version).execute(&mut *tx).await.unwrap();
            let cmd=format!("{id}-{action}");let result=serde_json::json!({"status":status,"version":result_version}).to_string();let authority=serde_json::json!({"companyId":auth}).to_string();sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES($1,$2,'user','fixture',$3,$4,$5,0)").bind(&cmd).bind(format!("inventory.count.{action}")).bind(id).bind(result).bind(authority).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO public.inventory_count_decisions(id,count_id,command_id,action,actor_id,reviewed_version,reason,created_at) VALUES($1,$2,$3,$4,'user',1,'fixture',0)").bind(format!("{id}-decision")).bind(id).bind(&cmd).bind(action).execute(&mut *tx).await.unwrap();
        }
        async fn binding(c:&mut sqlx::PgConnection,target:&str,previous:i64,actor:&str,compat:bool)->Result<(),sqlx::Error>{let set=format!("{target}-set-{previous}");let cmd=format!("{target}-bind-{previous}");let event=format!("{target}-event-{previous}");
            sqlx::query("INSERT INTO public.attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at,bound_at,bound_command_id) VALUES($1,'company','stock_count',$2,'record','count_sheet',$3,1,'internal','{}','bound','user',0,0,1,$4)").bind(&set).bind(target).bind(previous).bind(&cmd).execute(&mut *c).await?;
            let details=serde_json::json!({"bindingId":target,"bindingType":if compat {"legacy_upload_adapter"}else{"record_document"}}).to_string();sqlx::query("INSERT INTO public.attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES($1,'company',$2,$3,'set.bound',$4,1)").bind(&event).bind(&set).bind(actor).bind(details).execute(&mut *c).await?;
            let result=if compat {serde_json::json!({"attachmentSetId":set,"ownerVersion":previous+1})}else{serde_json::json!({"setId":set,"version":previous+1})}.to_string();sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES($1,$2,'user','fixture',$3,$4,'{\"companyId\":\"company\"}',1)").bind(&cmd).bind(if compat {"attachment.compatibility.bind"}else{"attachment.record.bind"}).bind(target).bind(result).execute(&mut *c).await?;Ok(())}
        binding(&mut tx,"terminal-chain",2,"user",false).await.unwrap();binding(&mut tx,"terminal-chain",3,"user",true).await.unwrap();binding(&mut tx,"terminal-gap",2,"user",false).await.unwrap();binding(&mut tx,"terminal-wrong-actor",2,"other",false).await.unwrap();
        let dir=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/server-inventory/src/adapters/supply-terminal-receipts.ts");let script=r#"if(process.version!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');const m=await import(process.argv[1]);process.stdout.write(JSON.stringify(m.supplyInventoryTerminalReceipts));"#;
        let output=std::process::Command::new(std::env::var("ERPX_NODE_BIN").unwrap_or_else(|_|"node".into())).arg("--conditions=react-server").arg("--input-type=module").arg("-e").arg(script).arg(dir).output().unwrap();assert!(output.status.success(),"actual TS: {}",String::from_utf8_lossy(&output.stderr));let actual:String=serde_json::from_slice(&output.stdout).unwrap();let oracle=format!("SELECT EXISTS(SELECT 1 FROM ({actual}) t WHERE t.kind='count' AND t.company_id=$1 AND t.id=$2)");
        for (id,expected) in [("terminal-direct",true),("terminal-chain",true),("terminal-gap",false),("terminal-wrong-actor",false),("terminal-cancel",true),("terminal-reject",true),("terminal-bad-auth",false),("terminal-bad-version",false),("terminal-missing",false)] {let real:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(oracle.as_str())).bind("company").bind(id).fetch_one(&mut *tx).await.unwrap();let observed=inspect_initial_count_terminal_history(&mut tx,&counts,&decisions,&receipts,&files,&company,id).await.unwrap();assert_eq!((observed.matches(),real),(expected,expected),"{id}");observed.require_current(&mut tx).await.unwrap();}
        let foreign=CompanyId::new("foreign").unwrap();let foreign_observed=inspect_initial_count_terminal_history(&mut tx,&counts,&decisions,&receipts,&files,&foreign,"terminal-direct").await.unwrap();assert!(!foreign_observed.matches());assert!(!sqlx::query_scalar::<_,bool>(sqlx::AssertSqlSafe(oracle.as_str())).bind("foreign").bind("terminal-direct").fetch_one(&mut *tx).await.unwrap());
        let gap=inspect_initial_count_terminal_history(&mut tx,&counts,&decisions,&receipts,&files,&company,"terminal-gap").await.unwrap();assert!(!gap.matches());sqlx::query("SAVEPOINT close_gap").execute(&mut *tx).await.unwrap();binding(&mut tx,"terminal-gap",3,"user",true).await.unwrap();assert!(gap.require_current(&mut tx).await.is_err());assert!(sqlx::query_scalar::<_,bool>(sqlx::AssertSqlSafe(oracle.as_str())).bind("company").bind("terminal-gap").fetch_one(&mut *tx).await.unwrap());sqlx::query("ROLLBACK TO SAVEPOINT close_gap").execute(&mut *tx).await.unwrap();gap.require_current(&mut tx).await.unwrap();
        // Synthetic source rows remain uncommitted and all owner guards enabled.
        tx.rollback().await.unwrap();let mut other=p.pool().raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();assert!(gap.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut saved=p.pool().raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();sqlx::query("SAVEPOINT before_count_sources").execute(&mut *saved).await.unwrap();lock_initial_count_history_sources(&mut saved).await.unwrap();lock_count_terminal_history_source(&mut saved).await.unwrap();lock_initial_count_history_receipt_sources(&mut saved).await.unwrap();lock_count_history_attachment_sources(&mut saved).await.unwrap();let saved_counts=inspect_initial_count_history_source(&mut saved).await.unwrap();let saved_decisions=inspect_count_terminal_history_source(&mut saved).await.unwrap();let saved_receipts=inspect_initial_count_history_receipt_join(&mut saved).await.unwrap();let saved_files=inspect_count_history_attachment_sources(&mut saved).await.unwrap();let saved_witness=inspect_initial_count_terminal_history(&mut saved,&saved_counts,&saved_decisions,&saved_receipts,&saved_files,&company,"terminal-missing").await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_count_sources").execute(&mut *saved).await.unwrap();assert!(saved_witness.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM public.inventory_counts WHERE id LIKE 'terminal-%'").fetch_one(p.pool().raw()).await.unwrap(),0);
    }).await;
}

#[tokio::test]
async fn field_file_source_accepts_large_persisted_reservation_history() {
    use erpx_server_field_service::public::file_source::read_field_file_source;
    // Read the original retired owner function at its immutable cutover parent.
    // Actual owner dependency exports still supply the SQL fragments; the
    // original query is executed in the same leased Holding transaction.
    let script=r#"
if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');
const {execFileSync}=await import('node:child_process');
const source=execFileSync('git',['show','e4be8bddb^:packages/server-field_service/src/adapters/purchases.ts'],{encoding:'utf8'});
const scopeMatch=source.match(/const scope=("[^"\n]+");/);
const fnMatch=source.match(/export function serviceOwnerSourceQuery\([\s\S]*?\n\}/);
if(!scopeMatch||!fnMatch)throw Error('ORIGINAL_SOURCE_MISSING');
const {fieldServiceTechnicianPredicate}=await import('@erpx/server-identity_access/public/field-service-authority');
const {fieldServiceCustomerSourceRows}=await import('@erpx/server-finance/public/field-service-references');
const {fieldServiceAssetSourceRows}=await import('@erpx/server-assets/public/field-service-references');
const support=execFileSync('git',['show','e4be8bddb^:packages/server-support/src/adapters/field-service-sources.ts'],{encoding:'utf8'});
const ticketMatch=support.match(/export const fieldServiceTicketSourceRows = ("[^"\n]+");/);if(!ticketMatch)throw Error('ORIGINAL_TICKET_SOURCE_MISSING');
const fieldServiceTicketSourceRows=JSON.parse(ticketMatch[1]);
const js=new Bun.Transpiler({loader:'ts'}).transformSync(fnMatch[0].replace(/^export /,''));
const query=Function('scope','fieldServiceTechnicianPredicate','fieldServiceCustomerSourceRows','fieldServiceAssetSourceRows','fieldServiceTicketSourceRows',js+'\nreturn serviceOwnerSourceQuery;')(JSON.parse(scopeMatch[1]),fieldServiceTechnicianPredicate,fieldServiceCustomerSourceRows,fieldServiceAssetSourceRows,fieldServiceTicketSourceRows)({kind:'order',id:'issue-order'},true);
let n=0;process.stdout.write(query.replace(/\?/g,()=>'$'+(++n)));
"#;
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(root).arg("--conditions=react-server").arg("-e").arg(script).output().unwrap();
    assert!(out.status.success(),"original TypeScript Field source oracle unavailable");let query=String::from_utf8(out.stdout).unwrap();
    let source_sql=format!("SELECT octet_length(image),jsonb_array_length(image::jsonb->'parts'),encode(sha256(convert_to((image::jsonb->'parts')::text,'UTF8')),'hex') FROM ({query}) original");
    facts::account::with_system(|system| async move {
        let p=facts::setup_workspace(system).await;
        // Historical schema-valid released rows: 2,800-char reasons satisfy
        // the original schema's 3,000-char limit; no stock remains reserved.
        sqlx::query("INSERT INTO field_service_part_reservations(id,order_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,reason,release_reason,created_at) SELECT 'large-history-'||n,'issue-order','warehouse','product',1,0,1,10000,'fixture-a',repeat('r',2800),'Synthetic historical release',0 FROM generate_series(1,850) n").execute(p.pool().raw()).await.unwrap();
        let observed=Arc::new(AtomicBool::new(false));let checked=observed.clone();
        let result=p.review.authorizer.execute_with_sources(
            "acme.example.test",p.review.maker.clone(),p.selection.clone(),
            WorkAuthorizationRequest{permission:"records.read".into(),module:Some("field_service".into()),destination:"service/orders".into(),api:true},
            FieldAttachmentIntakeSources,
            move |c,actor| {let checked=checked.clone();let source_sql=source_sql.clone();Box::pin(async move {
                let args=||sqlx::query_as::<_,(i32,i32,String)>(sqlx::AssertSqlSafe(source_sql.as_str())).bind("issue-order").bind("company").bind(None::<&str>).bind(None::<&str>).bind(None::<&str>).bind(None::<&str>);
                let before=args().fetch_one(&mut *c).await?;assert!(before.0>2*1024*1024);assert_eq!(before.1,851);
                let witness=read_field_file_source(&mut *c,actor,"issue-order").await?;assert_eq!(witness.facts().order_id,"issue-order");witness.require_current(&mut *c,actor).await?;
                sqlx::query("SAVEPOINT history_drift").execute(&mut *c).await?;
                sqlx::query("UPDATE field_service_part_reservations SET release_reason='Changed historical release' WHERE id='large-history-1'").execute(&mut *c).await?;
                let after=args().fetch_one(&mut *c).await?;assert_ne!(before.2,after.2);assert!(witness.require_current(&mut *c,actor).await.is_err());
                sqlx::query("ROLLBACK TO SAVEPOINT history_drift").execute(&mut *c).await?;witness.require_current(&mut *c,actor).await?;
                checked.store(true,Ordering::SeqCst);Ok(())
            })},
        ).await;
        assert!(result.is_ok(),"Field source with retained large history must succeed");assert!(observed.load(Ordering::SeqCst));
    }).await;
}
