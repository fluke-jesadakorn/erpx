#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_organization::public::{
    PgOrganizationReader, WorkspaceRouteQuery, read_company_context, resolve_workspace,
};
use erpx_server_runtime::public::postgres::{
    MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions, load_migrations,
    migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, HoldingId};
#[tokio::test]
async fn structural_routes_use_current_active_company_lineage_on_the_callers_transaction() {
    fixture::with_database(2,|pool|async move {
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let files=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES
         ('company','company','acme','Acme',NULL,'company',NULL,'active'),
         ('branch','branch','north','North','company','company','branch','active'),
         ('department','department','engineering','Engineering','branch','company','branch','active'),
         ('company-department','department','legal','Legal','company','company',NULL,'active'),
         ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active'),
         ('foreign-dept','department','wrong','Wrong','branch','foreign','branch','active')").execute(pool.raw()).await.unwrap();
        let uow=PgUnitOfWork::new(pool.clone());
        let context=uow.run::<_,BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move {
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *connection).await?;
            let mut port=PgOrganizationReader::new(HoldingId::new("h001").unwrap(),&mut *connection);
            let query=WorkspaceRouteQuery::new(Some("north".into()),Some("engineering".into()),Some("acme".into())).unwrap();
            let context=resolve_workspace(&mut port,&query).await?.unwrap();
            let second:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(connection).await?;assert_eq!(pid,second);Ok(context)
        })).await.unwrap();
        assert_eq!(context.holding_id().as_str(),"h001");assert_eq!(context.company().id.as_str(),"company");assert_eq!(context.branch().unwrap().id.as_str(),"branch");
        let denied=uow.run::<_,BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move {
            sqlx::query("UPDATE organization_nodes SET status='suspended' WHERE id='company'").execute(&mut *connection).await?;
            let mut port=PgOrganizationReader::new(HoldingId::new("h001").unwrap(),connection);
            read_company_context(&mut port,&CompanyId::new("company").unwrap()).await
        })).await;
        assert!(matches!(denied,Err(PgTransactionError::Business(_))));
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT status FROM organization_nodes WHERE id='company'").fetch_one(pool.raw()).await.unwrap(),"active");
        for (branch,department,company,exists) in [(None,Some("legal"),Some("acme"),true),(Some("north"),Some("wrong"),Some("acme"),false),(Some("north"),Some("engineering"),Some("foreign"),false)] {
            uow.run::<_,BusinessError,_>(TransactionOptions::default(),move|connection|Box::pin(async move {
                let mut port=PgOrganizationReader::new(HoldingId::new("h001").unwrap(),connection);
                let query=WorkspaceRouteQuery::new(branch.map(str::to_owned),department.map(str::to_owned),company.map(str::to_owned)).unwrap();
                assert_eq!(resolve_workspace(&mut port,&query).await?.is_some(),exists);Ok(())
            })).await.unwrap();
        }
    }).await;
}

#[tokio::test]
async fn organization_reader_preserves_sqlstate_for_the_outer_whole_transaction_retry() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    fixture::with_database(2,|pool|async move{
        sqlx::raw_sql("CREATE TABLE organization_nodes(id text PRIMARY KEY,kind text,slug text,display_name text,parent_id text,company_id text,branch_id text,status text,hierarchy_revision bigint); INSERT INTO organization_nodes VALUES('company','company','acme','Acme',NULL,'company',NULL,'active',1); CREATE SEQUENCE read_attempt; CREATE FUNCTION authority_probe() RETURNS boolean LANGUAGE plpgsql AS $$ BEGIN IF nextval('read_attempt')<2 THEN RAISE EXCEPTION 'retry authority snapshot' USING ERRCODE='40001'; END IF; RETURN true; END $$; ALTER TABLE organization_nodes ENABLE ROW LEVEL SECURITY; ALTER TABLE organization_nodes FORCE ROW LEVEL SECURITY; CREATE POLICY retry_probe ON organization_nodes USING(authority_probe());").execute(pool.raw()).await.unwrap();
        let attempts=Arc::new(AtomicUsize::new(0));let observed=attempts.clone();
        let context=PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),move|connection|{let observed=observed.clone();Box::pin(async move{
            observed.fetch_add(1,Ordering::SeqCst);let mut port=PgOrganizationReader::new(HoldingId::new("h001").unwrap(),connection);
            read_company_context(&mut port,&CompanyId::new("company").unwrap()).await
        })}).await.unwrap();
        assert_eq!(context.company().id.as_str(),"company");assert_eq!(attempts.load(Ordering::SeqCst),2);
    }).await;
}
