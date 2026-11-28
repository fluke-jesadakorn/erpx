#[path = "../../../platform/composition/src/procurement_rfq_receipt.rs"]
mod binding;
#[path = "../../../platform/composition/src/procurement_rfq_assertions.rs"]
mod procurement_rfq_assertions;
#[path = "../../support/finance/review-composition.rs"]
mod review;
use erpx_commercial_server::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, bind_purchase_write_sources,
    current_package_catalogue, lock_purchase_write_sources,
};
use erpx_identity_access_server::public::effective_authority::{
    HoldingWorkSourcePreparation, WorkAuthorizationRequest,
};
use erpx_procurement_server::public::rfq_transition_normalization::normalize_rfq_transition;
use erpx_platform_runtime_server::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_platform_shared_server::public::{BusinessError, Clock, FailureKind};
use erpx_workflow_server::public::{
    commands::CommandField,
    rfq_command_receipt::{RfqReceiptMatch, inspect_rfq_receipt, lock_rfq_receipts},
    source_ordered_json::{
        ScopedWorkAuthority, SourceOrderedFingerprintActor, SourceOrderedJson,
        source_ordered_fingerprint_metadata,
    },
};
use sqlx::PgConnection;

#[derive(Clone)]
struct ReceiptLocks;
impl HoldingWorkSourcePreparation for ReceiptLocks {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            let mut locks = lock_purchase_write_sources(c).await?;
            lock_rfq_receipts(locks.connection()).await
        })
    }
}
fn access() -> WorkAuthorizationRequest {
    WorkAuthorizationRequest {
        permission: "finance.write".into(),
        module: Some("finance".into()),
        destination: "finance/journals".into(),
        api: true,
    }
}

#[tokio::test]
async fn real_selected_identity_binds_rfq_receipt_fingerprint() {
    review::account::with_system(|system|async move {
        let p=review::setup_with_finance(system,false).await;
        let db=p.system.holdings[0].raw();let role="r".repeat(129);
        sqlx::query("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*',$1,'RFQ boundary',false,1,'active') ON CONFLICT DO NOTHING").bind(&role).execute(db).await.unwrap();
        sqlx::query("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*',$1,1,'[\"finance.read\",\"finance.write\",\"module.finance.read\"]','[]','[\"holding\"]','fixture',0) ON CONFLICT DO NOTHING").bind(&role).execute(db).await.unwrap();
        sqlx::query("UPDATE assignments SET role=$1 WHERE id='finance-fixture-a'").bind(&role).execute(db).await.unwrap();
        sqlx::query("UPDATE active_work_contexts SET revision=0 WHERE assignment_id='finance-fixture-a'").execute(db).await.unwrap();
        let mut selection=p.maker_selection.clone();selection.expected.as_mut().unwrap().revision=0;let now=p.system.scenario.clock.now();
        let raw=r#"{"reason":"  reason  ","expectedVersion":1,"action":"submit"}"#;
        let (hash,user)=p.authorizer.execute("acme.example.test",p.maker.clone(),selection.clone(),access(),move|_,current|Box::pin(async move{
            let input=normalize_rfq_transition(&SourceOrderedJson::parse(raw)?).unwrap();let sel=current.selection();let a=sel.assignment();let w=sel.workspace();
            let actor=SourceOrderedFingerprintActor{user_id:current.user_id().as_str(),assignment_id:a.id.as_str(),role:&a.role,company_id:w.company_id.as_str(),branch:w.branch_id.as_ref().map(|x|CommandField::Value(x.as_str())).unwrap_or(CommandField::Null),department:w.department_id.as_ref().map(|x|CommandField::Value(x.as_str())).unwrap_or(CommandField::Null),authority:CommandField::Value(ScopedWorkAuthority{session_id:current.session_id().as_str(),revision:sel.revision(),assignment_version:a.version})};
            Ok((source_ordered_fingerprint_metadata(&actor,"procurement.rfq.submit","rfq",input.json()),current.user_id().as_str().to_owned()))
        })).await.unwrap();
        sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('rfq-receipt-test','procurement.rfq.submit',$2,$1,'rfq','{\"id\":\"rfq\",\"version\":2}',0,'{\"companyId\":\"company\"}')").bind(hash).bind(user).execute(db).await.unwrap();
        let matched=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),selection.clone(),access(),ReceiptLocks,move|c,current|Box::pin(async move{
            let input=normalize_rfq_transition(&SourceOrderedJson::parse(raw)?).unwrap();
            let receipt=inspect_rfq_receipt(c,"rfq-receipt-test").await?;
            assert_eq!(binding::rfq_purchase_purpose(c,current,PackageEnforcementMode::Disabled,&input,None).await?,None);
            assert_eq!(binding::rfq_purchase_purpose(c,current,PackageEnforcementMode::Shadow,&input,Some(&receipt)).await?,Some(CapabilityPurpose::History));
            assert_eq!(binding::rfq_purchase_purpose(c,current,PackageEnforcementMode::Enforced,&input,Some(&receipt)).await?,Some(CapabilityPurpose::History));
            let catalogue=current_package_catalogue()?;
            let locks=bind_purchase_write_sources(c).await?;
            assert!(binding::inspect_rfq_purchase(c,current,PackageEnforcementMode::Disabled,&input,None,&catalogue,now).await?.is_none());
            assert!(binding::prepare_rfq_purchase(c,current,PackageEnforcementMode::Disabled,&input,None,None,&catalogue,"unused",now).await?.is_none());
            let shadow=binding::inspect_rfq_purchase(c,current,PackageEnforcementMode::Shadow,&input,Some(&receipt),&catalogue,now).await?.unwrap();
            assert!(shadow.access.iter().all(|x|x.allowed_by_mode));
            let pending=binding::prepare_rfq_purchase(c,current,PackageEnforcementMode::Shadow,&input,Some(&receipt),Some(&locks),&catalogue,"rfq-shadow-boundary",now).await?.unwrap();
            assert_eq!(pending.read_observation().access[0].purpose,CapabilityPurpose::History);
            assert!(!pending.read_observation().access[0].decision.allowed);
            let active=pending.begin(c,current,&locks,&catalogue).await?;
            let observation=active.complete(c,current,&locks,&catalogue).await?;
            assert!(observation.observation.is_some());
            let denied=binding::inspect_rfq_purchase(c,current,PackageEnforcementMode::Enforced,&input,Some(&receipt),&catalogue,now).await.err().expect("missing supply grant must deny");
            assert!(matches!(denied,PgTransactionError::Business(e) if e.kind==FailureKind::Forbidden && e.code=="PACKAGE_REQUIRED" && e.message=="This package does not permit the requested work."));
            let cancel=normalize_rfq_transition(&SourceOrderedJson::parse(r#"{"action":"cancel","expectedVersion":1,"reason":"reason"}"#)?).unwrap();
            assert_eq!(binding::rfq_purchase_purpose(c,current,PackageEnforcementMode::Enforced,&cancel,Some(&receipt)).await?,Some(CapabilityPurpose::SettleExisting));
            let missing=inspect_rfq_receipt(c,"missing-rfq").await?;
            assert!(binding::fresh_rfq_replay_witness(c,current,&receipt,&input,"rfq-receipt-test","rfq").await?.is_none());
            let fresh=binding::fresh_rfq_replay_witness(c,current,&missing,&input,"missing-rfq","rfq").await?.unwrap();
            assert!(fresh.require_for(c,current,&input,"missing-rfq","rfq").await?);
            assert!(!fresh.require_for(c,current,&cancel,"missing-rfq","rfq").await?);
            assert!(!fresh.require_for(c,current,&input,"another-key","rfq").await?);
            assert!(!fresh.require_for(c,current,&input,"missing-rfq","other-rfq").await?);
            assert!(binding::fresh_rfq_replay_witness(c,current,&missing,&input,"different-key","rfq").await?.is_none());
            assert_eq!(binding::rfq_purchase_purpose(c,current,PackageEnforcementMode::Enforced,&input,Some(&missing)).await?,Some(CapabilityPurpose::NewWork));
            assert!(matches!(binding::compare_rfq_receipt(c,current,&receipt,"different",&input).await?,RfqReceiptMatch::Conflict));
            assert_eq!(binding::rfq_purchase_purpose(c,current,PackageEnforcementMode::Enforced,&input,Some(&receipt)).await?,Some(CapabilityPurpose::History));
            Ok(matches!(binding::compare_rfq_receipt(c,current,&receipt,"rfq",&input).await?,RfqReceiptMatch::Match(Some(_))))
        })).await.unwrap();
        assert!(matched);
        let mut grant=db.begin().await.unwrap();
        sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES('company','supply',1,'active',0,$1,1,'Fixture purchase','Fixture evidence',$2)").bind(9_000_000_000_000_i64).bind(now.get()).execute(&mut *grant).await.unwrap();
        sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'grant-supply-rfq','company',package_id,'fixture-a','fixture',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company' AND package_id='supply'").bind(now.get()).execute(&mut *grant).await.unwrap();
        grant.commit().await.unwrap();
        let admitted=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),selection,access(),ReceiptLocks,move|c,current|Box::pin(async move{
            let input=normalize_rfq_transition(&SourceOrderedJson::parse(raw)?).unwrap();let receipt=inspect_rfq_receipt(c,"rfq-receipt-test").await?;let catalogue=current_package_catalogue()?;
            let locks=bind_purchase_write_sources(c).await?;
            sqlx::query("SAVEPOINT changed_grant").execute(&mut *c).await?;
            let changed=binding::prepare_rfq_purchase(c,current,PackageEnforcementMode::Enforced,&input,Some(&receipt),Some(&locks),&catalogue,"rfq-changed-boundary",now).await?.unwrap();
            let changed=changed.begin(c,current,&locks,&catalogue).await?;
            sqlx::query("UPDATE company_package_grants SET state='inactive' WHERE company_id='company' AND package_id='supply'").execute(&mut *c).await?;
            // The database retains History rights when an existing grant is withdrawn.
            changed.complete(c,current,&locks,&catalogue).await?;
            sqlx::query("ROLLBACK TO SAVEPOINT changed_grant").execute(&mut *c).await?;
            sqlx::query("SAVEPOINT changed_new_work").execute(&mut *c).await?;
            let missing=inspect_rfq_receipt(c,"fresh-rfq-key").await?;
            let changed=binding::prepare_rfq_purchase(c,current,PackageEnforcementMode::Enforced,&input,Some(&missing),Some(&locks),&catalogue,"rfq-new-work-boundary",now).await?.unwrap();
            let changed=changed.begin(c,current,&locks,&catalogue).await?;
            sqlx::query("UPDATE company_package_grants SET state='inactive' WHERE company_id='company' AND package_id='supply'").execute(&mut *c).await?;
            let denied=changed.complete(c,current,&locks,&catalogue).await.err().expect("withdrawn grant must fail the new-work post-effect guard");
            assert!(matches!(denied,PgTransactionError::Business(e) if e.kind==FailureKind::Conflict && e.code=="PACKAGE_CAPABILITY_CHANGED"));
            sqlx::query("ROLLBACK TO SAVEPOINT changed_new_work").execute(&mut *c).await?;
            let pending=binding::prepare_rfq_purchase(c,current,PackageEnforcementMode::Enforced,&input,Some(&receipt),Some(&locks),&catalogue,"rfq-enforced-boundary",now).await?.unwrap();
            let active=pending.begin(c,current,&locks,&catalogue).await?;
            let _observation=active.complete(c,current,&locks,&catalogue).await?;
            let remaining:i64=sqlx::query_scalar("SELECT count(*) FROM command_assertions WHERE command_id='rfq-enforced-boundary'").fetch_one(c).await?;
            Ok(remaining==0)
        })).await.unwrap();
        assert!(admitted);
    }).await;
}
