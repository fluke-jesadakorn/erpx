//! Owner effect test with explicit synthetic source; this does not grant IAM.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::service_count_flow_notification::*;
use erpx_server_runtime::public::postgres::{
    MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions, load_migrations,
    migrate_database,
};
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
struct SyntheticSource(CountFlowNotificationFacts);
impl PgCountFlowNotificationSource for SyntheticSource {
    fn facts(&self) -> &CountFlowNotificationFacts {
        &self.0
    }
    async fn require_before(&self, _: &mut PgConnection) -> Result<(), Error> {
        Ok(())
    }
    async fn require_after(&self, _: &mut PgConnection) -> Result<(), Error> {
        Ok(())
    }
}
#[tokio::test]
async fn notification_planned_image_is_retained_and_existing_unique_conflict_is_preserved() {
    fixture::with_database(2,|pool|async move{
 let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let m=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&m).await.unwrap();
 for id in ["notice-a","notice-b"] {PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move{lock_count_flow_notifications(c).await?;let source=SyntheticSource(CountFlowNotificationFacts{id:id.into(),company_id:"co".into(),recipient:"reviewer".into(),count_id:"count".into(),version:2,message:"Review".into(),created_at:1});let p=prepare_count_flow_notification(c,&source).await?;let retained=p.persist(c).await?;retained.require_current(c).await?;Ok(())})).await.unwrap();}
 let rows:(i64,String)=sqlx::query_as("SELECT count(*),min(id) FROM staff_notifications WHERE record_type='stock_count' AND record_id='count'").fetch_one(pool.raw()).await.unwrap();assert_eq!(rows,(1,"notice-a".into()));
}).await
}
