#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_legal::public::record_policy::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, HoldingId, RecordId, UserId};
use sqlx::PgConnection;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
type Error = PgTransactionError<BusinessError>;
fn id(v: &str) -> RecordId {
    RecordId::new(v).unwrap()
}
fn actor() -> LegalRecordActor {
    LegalRecordActor {
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        user_id: UserId::new("reader").unwrap(),
        branch_id: None,
        department_id: None,
        role: "legal_counsel".into(),
        permissions: ["legal.read", "legal.edit", "legal.export"]
            .map(str::to_owned)
            .into(),
    }
}
fn request(operation: LegalRecordOperation) -> LegalRecordRequest {
    LegalRecordRequest {
        item_id: id("item"),
        document_id: Some(id("document")),
        operation,
        mode: LegalCommercialMode::Enforced,
    }
}
/// Explicit synthetic foreign-owner policy only. These exercise the real Legal
/// repository; production must compose the actual PMS and Commercial readers.
#[derive(Default)]
struct External {
    deny_purchase: bool,
    deny_release: bool,
}
impl PgLegalRecordExternalPolicy for External {
    async fn lock_sources(&mut self, _: &mut PgConnection) -> Result<(), Error> {
        Ok(())
    }
    async fn release(
        &mut self,
        _: &mut PgConnection,
        _: &CompanyId,
        _: i64,
    ) -> Result<LegalReleaseObservation, Error> {
        Ok(LegalReleaseObservation {
            allowed: !self.deny_release,
            revision: format!("synthetic-release-{}", self.deny_release),
        })
    }
    async fn purchases(
        &mut self,
        _: &mut PgConnection,
        _: &CompanyId,
        r: &[LegalPurchaseRequirement],
        _: LegalCommercialMode,
        _: i64,
    ) -> Result<LegalPurchaseObservation, Error> {
        assert!(r.iter().any(|r| r.purpose == LegalPurchasePurpose::History));
        Ok(LegalPurchaseObservation {
            allowed: !self.deny_purchase,
            revision: format!("synthetic-purchase-{}", self.deny_purchase),
        })
    }
}
async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,status)VALUES('company','company','acme','Synthetic',NULL,'company','active');INSERT INTO legal_settings(company_id,enabled)VALUES('company',true);INSERT INTO legal_items(id,company_id,kind,title,classification,country,jurisdiction,language,currency,timezone,priority,owner_id,created_by,created_at,updated_at)VALUES('item','company','contract','Synthetic','confidential','TH','TH','en','THB','Asia/Bangkok','normal','reader','reader',0,0);INSERT INTO legal_contracts VALUES('item','{}');INSERT INTO legal_members VALUES('item','reader','read');INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,created_by,created_at)VALUES('document','item',1,'synthetic.pdf','application/pdf',repeat('a',64),1,'x','reader',0)").execute(pool.raw()).await.unwrap();
}
fn policy<'a>(c: &'a mut PgConnection, e: &'a mut External) -> PgLegalRecordPolicy<'a, External> {
    PgLegalRecordPolicy::new(
        HoldingId::new("holding").unwrap(),
        CompanyId::new("company").unwrap(),
        c,
        e,
    )
}
async fn inspect(
    pool: &DatabasePool,
    operation: LegalRecordOperation,
    external: External,
) -> Result<LegalRecordAccess, Error> {
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let mut external = External {
                deny_purchase: external.deny_purchase,
                deny_release: external.deny_release,
            };
            Box::pin(async move {
                let mut p = policy(c, &mut external);
                p.prepare_sources().await?;
                p.inspect(&actor(), &request(operation), 100).await
            })
        })
        .await
}
#[tokio::test]
async fn current_confidential_membership_editor_disposal_and_hold_policy() {
    fixture::with_database(2,|pool|async move{
 setup(&pool).await;assert!(inspect(&pool,LegalRecordOperation::Document,External::default()).await.is_ok());
 assert!(matches!(inspect(&pool,LegalRecordOperation::Edit,External::default()).await,Err(Error::Business(e))if e.code=="LEGAL_EDITOR_REQUIRED"));
 sqlx::query("INSERT INTO legal_holds(id,item_id,reason,state,created_by,created_at)VALUES('hold','item','Synthetic preservation','active','reader',0)").execute(pool.raw()).await.unwrap();assert!(inspect(&pool,LegalRecordOperation::Document,External::default()).await.unwrap().held);
 sqlx::raw_sql("INSERT INTO legal_destructions(id,item_id,policy_id,policy_version,requested_by,state,reason,manifest,created_at)VALUES('destruction','item','synthetic-policy',1,'reader','executing','Synthetic denial','[]',0);INSERT INTO legal_disposals(document_id,destruction_id,checksum,disposed_at)VALUES('document','destruction',repeat('a',64),0)").execute(pool.raw()).await.unwrap();
 assert!(inspect(&pool,LegalRecordOperation::Metadata,External::default()).await.is_ok());assert!(matches!(inspect(&pool,LegalRecordOperation::Document,External::default()).await,Err(Error::Business(e))if e.code=="LEGAL_DOCUMENT_DISPOSED"));
}).await;
}
#[tokio::test]
async fn current_source_and_external_policy_changes_roll_back_owner_effects() {
    fixture::with_database(2,|pool|async move{
 setup(&pool).await;assert!(matches!(inspect(&pool,LegalRecordOperation::Processing,External{deny_purchase:true,..Default::default()}).await,Err(Error::Business(e))if e.code=="PACKAGE_REQUIRED"));
 assert!(matches!(inspect(&pool,LegalRecordOperation::Processing,External{deny_release:true,..Default::default()}).await,Err(Error::Business(e))if e.code=="FEATURE_COMING_SOON"));
 let result=PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{let mut e=External::default();let mut p=policy(c,&mut e);p.prepare_sources().await?;let read=p.inspect(&actor(),&request(LegalRecordOperation::Document),100).await?;drop(p);sqlx::query("DELETE FROM legal_members WHERE item_id='item'").execute(&mut *c).await?;policy(c,&mut e).revalidate(&read.witness,101).await})).await;
 assert!(matches!(result,Err(Error::Business(e))if e.code=="LEGAL_NOT_FOUND"));assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM legal_members").fetch_one(pool.raw()).await.unwrap(),1);
}).await;
}
#[tokio::test]
async fn witness_never_crosses_transactions_even_on_the_same_backend() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let read = inspect(&pool, LegalRecordOperation::Document, External::default())
            .await
            .unwrap();
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
                let witness = read.witness.clone();
                Box::pin(async move {
                    let mut e = External::default();
                    policy(c, &mut e).revalidate(&witness, 101).await
                })
            })
            .await;
        assert!(matches!(result,Err(Error::Business(e))if e.code=="LEGAL_SOURCE_CHANGED"));
    })
    .await;
}
#[tokio::test]
async fn deleted_membership_after_an_earlier_snapshot_preserves_whole_attempt_retry() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let attempts = Arc::new(AtomicUsize::new(0));
        let observed = attempts.clone();
        let writer = pool.clone();
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
                let writer = writer.clone();
                let attempts = attempts.clone();
                Box::pin(async move {
                    if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                        sqlx::query("SELECT 1 FROM legal_members")
                            .execute(&mut *c)
                            .await?;
                        sqlx::query("DELETE FROM legal_members WHERE item_id='item'")
                            .execute(writer.raw())
                            .await?;
                    }
                    let mut e = External::default();
                    let mut p = policy(c, &mut e);
                    p.prepare_sources().await?;
                    p.inspect(&actor(), &request(LegalRecordOperation::Document), 100)
                        .await
                })
            })
            .await;
        assert!(matches!(result,Err(Error::Business(e))if e.code=="LEGAL_NOT_FOUND"));
        assert_eq!(
            observed.load(Ordering::SeqCst),
            2,
            "known deleted tuple must raise40001 before a fresh denial"
        );
    })
    .await;
}
#[tokio::test]
async fn newly_inserted_destruction_committed_before_first_source_lock_is_visible() {
    fixture::with_database(3,|pool|async move{
 setup(&pool).await;let mut writer=pool.raw().begin().await.unwrap();sqlx::query("INSERT INTO legal_destructions(id,item_id,policy_id,policy_version,requested_by,state,reason,manifest,created_at)VALUES('destruction','item','synthetic-policy',1,'reader','executing','Synthetic denial','[]',0)").execute(&mut *writer).await.unwrap();
 let owner_pool=pool.clone();let reader=tokio::spawn(async move{inspect(&owner_pool,LegalRecordOperation::Document,External::default()).await});
 tokio::time::timeout(std::time::Duration::from_secs(5),async{loop{if sqlx::query_scalar::<_,i64>("SELECT count(*) FROM pg_locks WHERE database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND NOT granted AND relation='legal_destructions'::regclass").fetch_one(pool.raw()).await.unwrap()>0{break;}tokio::task::yield_now().await;}}).await.unwrap();
 assert!(!reader.is_finished());writer.commit().await.unwrap();assert!(matches!(reader.await.unwrap(),Err(Error::Business(e))if e.code=="LEGAL_RETENTION_FROZEN"));
}).await;
}
