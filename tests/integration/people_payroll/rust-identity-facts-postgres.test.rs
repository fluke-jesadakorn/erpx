#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_people_payroll::public::identity_facts::{
    PgPeopleAuthorityReader, lock_people_authority_sources,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, UserId};
use std::{sync::Arc, time::Duration};

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,email,phone,country_code,created_by,created_at,updated_at) VALUES('person','user','company','PRIVATE_NAME_SENTINEL','PRIVATE_EMAIL_SENTINEL','PRIVATE_PHONE_SENTINEL','TH','user',0,0),('foreign-person','foreign-user','foreign','Foreign fixture',NULL,NULL,'TH','user',0,0);
        INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES('engagement','person','company','employee','salaried','active','2026-01-01','Fixture','THB','user',0,0),('ended','person','company','employee','salaried','ended','2025-01-01','Previous fixture','THB','user',0,0)")
        .execute(pool.raw()).await.unwrap();
}

async fn wait_for_source_lock(pool: &DatabasePool) -> bool {
    let observed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let waiting = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND usename=current_user AND backend_type='client backend' AND pid<>pg_backend_pid() AND wait_event_type='Lock')")
                .fetch_one(pool.raw()).await?;
            if waiting { return Ok::<(), sqlx::Error>(()); }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await;
    matches!(observed, Ok(Ok(())))
}

#[tokio::test]
async fn people_facts_are_company_scoped_minimal_and_current() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::query("UPDATE hr_engagements SET branch_id='branch',department_id='department',version=version+1 WHERE id='engagement'").execute(pool.raw()).await.unwrap();
        let mut outside = pool.raw().acquire().await.unwrap();
        assert!(PgPeopleAuthorityReader::new(&mut outside).person_for_user(&CompanyId::new("company").unwrap(), &UserId::new("user").unwrap()).await.is_err());
        drop(outside);
        let uow = PgUnitOfWork::new(pool.clone());
        uow.run::<(), BusinessError, _>(TransactionOptions::default(), |connection| Box::pin(async move {
            lock_people_authority_sources(&mut *connection).await?;
            let mut reader = PgPeopleAuthorityReader::new(connection);
            let company = CompanyId::new("company").unwrap(); let user = UserId::new("user").unwrap();
            let person = reader.person_for_user(&company, &user).await?.unwrap();
            assert_eq!(person.active_engagements[0].branch_id.as_ref().map(|v|v.as_str()),Some("branch")); assert_eq!(person.active_engagements[0].department_id.as_ref().map(|v|v.as_str()),Some("department"));
            assert_eq!(person.person_id.as_str(), "person"); assert_eq!(person.active_engagements.len(), 1); assert!(person.active_hr_subject());
            let debug = format!("{person:?}"); assert!(!debug.contains("PRIVATE_"));
            assert!(reader.person_subject(&CompanyId::new("foreign").unwrap(), &person.person_id).await?.is_none());
            assert!(reader.person_for_user(&CompanyId::new("foreign").unwrap(), &user).await?.is_none());
            assert_eq!(reader.person_subject(&company, &RecordId::new("person").unwrap()).await?.unwrap(), person);
            Ok(())
        })).await.unwrap();
        let rollback = uow.run::<(), BusinessError, _>(TransactionOptions::default(), |connection| Box::pin(async move {
            sqlx::query("UPDATE hr_engagements SET status='suspended',version=version+1 WHERE id='engagement'").execute(&mut *connection).await?;
            let person = PgPeopleAuthorityReader::new(connection).person_subject(&CompanyId::new("company").unwrap(), &RecordId::new("person").unwrap()).await?.unwrap();
            assert!(!person.active_hr_subject());
            Err(PgTransactionError::Business(BusinessError::invalid("OWNER_ABORT", "Rollback fixture")))
        })).await;
        assert!(rollback.is_err());
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM hr_engagements WHERE id='engagement'").fetch_one(pool.raw()).await.unwrap(), "active");
    }).await;
}

#[tokio::test]
async fn people_source_lock_blocks_changes_until_owner_commit() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let ready = Arc::new(tokio::sync::Notify::new()); let release = Arc::new(tokio::sync::Notify::new());
        let read_pool = pool.clone(); let read_ready = ready.clone(); let read_release = release.clone();
        let reader = tokio::spawn(async move {
            PgUnitOfWork::new(read_pool).run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let ready = read_ready.clone(); let release = read_release.clone();
                Box::pin(async move {
                    lock_people_authority_sources(&mut *connection).await?;
                    let facts = PgPeopleAuthorityReader::new(connection).person_subject(&CompanyId::new("company").unwrap(), &RecordId::new("person").unwrap()).await?.unwrap();
                    ready.notify_one(); release.notified().await; Ok(facts)
                })
            }).await.unwrap()
        });
        if tokio::time::timeout(Duration::from_secs(3), ready.notified()).await.is_err() {
            reader.abort(); let _ = reader.await;
            panic!("the owned reader did not reach its source snapshot");
        }
        let write_pool = pool.clone();
        let writer = tokio::spawn(async move { sqlx::query("UPDATE hr_engagements SET status='suspended',version=version+1 WHERE id='engagement'").execute(write_pool.raw()).await.unwrap() });
        let waiting = wait_for_source_lock(&pool).await;
        let writer_pending = !writer.is_finished();
        release.notify_one(); assert!(reader.await.unwrap().active_hr_subject()); writer.await.unwrap();
        assert!(waiting && writer_pending, "expected an actual owned PostgreSQL source-lock wait");
        PgUnitOfWork::new(pool.clone()).run::<(), BusinessError, _>(TransactionOptions::default(), |connection| Box::pin(async move {
            lock_people_authority_sources(&mut *connection).await?;
            let facts = PgPeopleAuthorityReader::new(connection).person_subject(&CompanyId::new("company").unwrap(), &RecordId::new("person").unwrap()).await?.unwrap();
            assert!(!facts.active_hr_subject()); Ok(())
        })).await.unwrap();
    }).await;
}

#[tokio::test]
async fn source_snapshot_observes_a_writer_committed_before_lock_acquisition() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let mut write = pool.raw().begin().await.unwrap();
        sqlx::query("UPDATE hr_people SET status='inactive',version=version+1 WHERE id='person'")
            .execute(&mut *write)
            .await
            .unwrap();
        let read_pool = pool.clone();
        let reader = tokio::spawn(async move {
            PgUnitOfWork::new(read_pool)
                .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                    Box::pin(async move {
                        lock_people_authority_sources(&mut *connection).await?;
                        PgPeopleAuthorityReader::new(connection)
                            .person_subject(
                                &CompanyId::new("company").unwrap(),
                                &RecordId::new("person").unwrap(),
                            )
                            .await
                    })
                })
                .await
                .unwrap()
                .unwrap()
        });
        let waiting = wait_for_source_lock(&pool).await;
        let reader_pending = !reader.is_finished();
        write.commit().await.unwrap();
        let facts = reader.await.unwrap();
        assert!(
            waiting && reader_pending,
            "expected an actual owned PostgreSQL source-lock wait"
        );
        assert_eq!(facts.status, "inactive");
        assert!(!facts.active_hr_subject());
    })
    .await;
}
