#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_platform_administration::public::legal_release::*;
use erpx_server_pms::public::scheduled_legal::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::Connection;
type Error = PgTransactionError<BusinessError>;
const ARTIFACT: &str = "synthetic-native-release";
fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}
async fn setup(pool: &DatabasePool) {
    let migrations = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('company','company','synthetic-native-release','Synthetic Company',NULL,'company',NULL,'active');
        INSERT INTO company_business_acceptance(company_id,status,version,requested_by,reviewed_by,requested_at,reviewed_at,reason,updated_at) VALUES('company','accepted',2,'requester','reviewer',1,1,'Synthetic reviewed setup',1);
        INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','synthetic_native_legal_review','Synthetic Legal review',false,1,'active');
        INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','synthetic_native_legal_review',1,'[\"legal.manage\",\"organization.manage\"]','[]','[\"holding\"]','synthetic',0);
        INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('reviewer-grant','reviewer','synthetic_native_legal_review','[\"legal\"]','company',0,8000000000000000,1,'holding');
        INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,reason,changed_by,changed_at) VALUES('company','legal','pilot','Synthetic approved pilot','synthetic',1)")
        .execute(pool.raw()).await.unwrap();
    acceptance(pool, "release-1", 1, false).await;
}
async fn acceptance(pool: &DatabasePool, id: &str, created: i64, revoked: bool) {
    sqlx::query("INSERT INTO company_release_acceptances(id,company_id,scope_version,software_version,configuration_revision,setup_version,evidence_sha256,evidence_json,requested_by,reason,revoked_at,created_at,updated_at) SELECT $1,'company','legal-th-operations-v1',$2,revision,2,$3,'{}','requester','Synthetic release evidence',$4,$5,$5 FROM company_legal_release_revisions WHERE company_id='company'")
        .bind(id).bind(ARTIFACT).bind("a".repeat(64)).bind(if revoked{Some(created)}else{None}).bind(created).execute(pool.raw()).await.unwrap();
    let catalogue = current_legal_release_catalogue().unwrap();
    for gate in catalogue.criteria() {
        sqlx::query("INSERT INTO company_release_gate_reviews(company_id,release_id,criterion_id,state,reviewer_id,assignment_id,permission,reason,reviewed_at) VALUES('company',$1,$2,'passed','reviewer','reviewer-grant',$3,'Synthetic independent evidence',1)")
            .bind(id).bind(&gate.id).bind(&gate.permission).execute(pool.raw()).await.unwrap();
    }
}
async fn inspect(pool: &DatabasePool, expected: bool) -> ScheduledLegalInspection {
    PgUnitOfWork::new(pool.clone()).run::<ScheduledLegalInspection,BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move {
        lock_scheduled_legal_sources(c,ScheduledReleaseMode::Production).await?;
        let legacy:bool=sqlx::query_scalar(include_str!("../../../fixtures/platform_administration/rust-legal-release/predicate.v1.sql")).bind(company().as_str()).fetch_one(&mut *c).await?;
        let catalogue=current_legal_release_catalogue()?;
        let current=PgLegalReleaseReader::new(c,ARTIFACT,&catalogue,None)?.inspect(&company(),TimestampMillis::new(0)).await?;
        assert_eq!(current.accepted,legacy);assert_eq!(legacy,expected);assert!(!current.engineering_allowed);
        let scheduled=PgScheduledLegalReader::new(c,ScheduledReleaseMode::Production,ARTIFACT,&catalogue,None).inspect(&company(),TimestampMillis::new(0)).await?;
        assert_eq!(scheduled.allowed,expected);Ok(scheduled)
    })).await.unwrap()
}
#[tokio::test]
async fn legal_release_matches_actual_legacy_sql_for_current_gates_build_setup_and_latest_acceptance()
 {
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;let initial=inspect(&pool,true).await;
        for change in ["UPDATE company_release_gate_reviews SET state='not_applicable' WHERE criterion_id='legal_content'","UPDATE company_release_gate_reviews SET permission='organization.read' WHERE criterion_id='legal_content'","UPDATE company_release_gate_reviews SET reviewer_id='requester' WHERE criterion_id='legal_content'","UPDATE assignments SET revoked_at=1 WHERE id='reviewer-grant'"] {
            let mut transaction=pool.raw().begin().await.unwrap();sqlx::raw_sql(change).execute(&mut *transaction).await.unwrap();transaction.commit().await.unwrap();inspect(&pool,false).await;
            sqlx::raw_sql("UPDATE company_release_gate_reviews SET state='passed',permission='legal.manage',reviewer_id='reviewer' WHERE criterion_id='legal_content';UPDATE assignments SET revoked_at=NULL WHERE id='reviewer-grant'").execute(pool.raw()).await.unwrap();
        }
        assert_eq!(initial.revision,inspect(&pool,true).await.revision);
        sqlx::query("UPDATE company_business_acceptance SET status='revoked',version=version+1 WHERE company_id='company'").execute(pool.raw()).await.unwrap();inspect(&pool,false).await;
        // A fresh acceptance bound to the current setup/configuration is needed.
        sqlx::query("UPDATE company_business_acceptance SET status='accepted',version=2 WHERE company_id='company'").execute(pool.raw()).await.unwrap();
        inspect(&pool,false).await;
        acceptance(&pool,"release-2",2,false).await;inspect(&pool,true).await;
        acceptance(&pool,"release-3",3,true).await;inspect(&pool,false).await;
    }).await;
}
#[tokio::test]
async fn legal_pilot_is_not_a_release_bypass_and_read_evidence_is_transaction_bound() {
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        let mut connection=pool.raw().acquire().await.unwrap();
        let mut first_transaction=connection.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *first_transaction).await.unwrap();
        let catalogue=current_legal_release_catalogue().unwrap();
        let first=PgLegalReleaseReader::new(&mut first_transaction,ARTIFACT,&catalogue,None).unwrap().inspect(&company(),TimestampMillis::new(0)).await.unwrap();
        first_transaction.commit().await.unwrap();
        let mut second_transaction=connection.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *second_transaction).await.unwrap();
        assert!(matches!(PgLegalReleaseReader::new(&mut second_transaction,ARTIFACT,&catalogue,None).unwrap().revalidate(&company(),TimestampMillis::new(0),&first.witness).await,Err(Error::Business(ref e))if e.code=="LEGAL_RELEASE_CHANGED"));
        second_transaction.rollback().await.unwrap();drop(connection);
        PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
            let catalogue=current_legal_release_catalogue()?;
            assert!(!PgScheduledLegalReader::new(c,ScheduledReleaseMode::Production,"unbuilt",&catalogue,None).inspect(&company(),TimestampMillis::new(0)).await?.allowed);
            assert!(PgScheduledLegalReader::new(c,ScheduledReleaseMode::Development,"unbuilt",&catalogue,None).inspect(&company(),TimestampMillis::new(0)).await?.allowed);
            Ok(())
        })).await.unwrap();
        sqlx::query("UPDATE company_feature_rollouts SET rollout_state='off'").execute(pool.raw()).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
            let catalogue=current_legal_release_catalogue()?;
            assert!(!PgScheduledLegalReader::new(c,ScheduledReleaseMode::Production,ARTIFACT,&catalogue,None).inspect(&company(),TimestampMillis::new(0)).await?.allowed);Ok(())
        })).await.unwrap();
    }).await;
}

#[tokio::test]
async fn scheduled_source_lease_observes_inserted_latest_revocation_before_its_first_snapshot() {
    use std::time::Duration;
    fixture::with_database(3,|pool|async move {
        setup(&pool).await;
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::query("INSERT INTO company_release_acceptances(id,company_id,scope_version,software_version,configuration_revision,setup_version,evidence_sha256,evidence_json,requested_by,reason,revoked_at,created_at,updated_at) SELECT 'release-new-revoked','company','legal-th-operations-v1',$1,revision,2,$2,'{}','requester','Synthetic revoked newer acceptance',2,2,2 FROM company_legal_release_revisions WHERE company_id='company'")
            .bind(ARTIFACT).bind("b".repeat(64)).execute(&mut *writer).await.unwrap();
        let fresh_pool=pool.clone();
        let fresh=tokio::spawn(async move {
            PgUnitOfWork::new(fresh_pool).run::<bool,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
                lock_scheduled_legal_sources(c,ScheduledReleaseMode::Production).await?;
                let catalogue=current_legal_release_catalogue()?;
                let result=PgScheduledLegalReader::new(c,ScheduledReleaseMode::Production,ARTIFACT,&catalogue,None).inspect(&company(),TimestampMillis::new(0)).await?;
                let legacy:bool=sqlx::query_scalar(include_str!("../../../fixtures/platform_administration/rust-legal-release/predicate.v1.sql")).bind(company().as_str()).fetch_one(c).await?;
                assert_eq!(result.allowed,legacy);Ok(result.allowed)
            })).await
        });
        let blocked=tokio::time::timeout(Duration::from_secs(5),async {
            loop{let waiting=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND usename=current_user AND pid<>pg_backend_pid() AND backend_type='client backend' AND wait_event_type='Lock')").fetch_one(pool.raw()).await.unwrap();if waiting{break;}tokio::time::sleep(Duration::from_millis(10)).await;}
        }).await.is_ok();
        writer.commit().await.unwrap();let allowed=fresh.await.unwrap().unwrap();assert!(blocked);assert!(!allowed);
    }).await;
}
