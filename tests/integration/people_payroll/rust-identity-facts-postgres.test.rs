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

#[tokio::test]
async fn operational_workforce_reference_preserves_utc_dates_and_company_subject_scope() {
    use erpx_server_people_payroll::public::workforce_reference::inspect_workforce_reference;
    use erpx_server_shared::public::TimestampMillis;
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        sqlx::query("UPDATE hr_engagements SET start_date='2026-10-27',end_date='2026-10-27' WHERE id='engagement'").execute(pool.raw()).await.unwrap();
        let day:i64=sqlx::query_scalar("SELECT (extract(epoch FROM TIMESTAMPTZ '2026-10-27 00:00:00+00')*1000)::bigint").fetch_one(pool.raw()).await.unwrap();
        PgUnitOfWork::new(pool).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move{
            lock_people_authority_sources(c).await?;
            let company=CompanyId::new("company").unwrap();let user=UserId::new("user").unwrap();
            for(now,eligible)in[(day-1,false),(day,true),(day+86_399_999,true),(day+86_400_000,false)]{
                let source=inspect_workforce_reference(c,&company,&user,TimestampMillis::new(now)).await?;
                assert_eq!(source.is_eligible(),eligible);source.require_current(c,TimestampMillis::new(now)).await?;
            }
            assert!(!inspect_workforce_reference(c,&CompanyId::new("foreign").unwrap(),&user,TimestampMillis::new(day)).await?.is_eligible());
            assert!(!inspect_workforce_reference(c,&company,&UserId::new("foreign-user").unwrap(),TimestampMillis::new(day)).await?.is_eligible());
            Ok(())
        })).await.unwrap();
    }).await;
}
#[tokio::test]
async fn operational_workforce_reference_rechecks_eligibility_without_retaining_profile_or_placement()
 {
    use erpx_server_people_payroll::public::workforce_reference::inspect_workforce_reference;
    use erpx_server_shared::public::TimestampMillis;
    fixture::with_database(2,|pool|async move{
        setup(&pool).await;
        let now:i64=sqlx::query_scalar("SELECT (extract(epoch FROM TIMESTAMPTZ '2026-10-27 12:00:00+00')*1000)::bigint").fetch_one(pool.raw()).await.unwrap();
        PgUnitOfWork::new(pool).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move{
            lock_people_authority_sources(c).await?;
            let company=CompanyId::new("company").unwrap();let user=UserId::new("user").unwrap();
            let source=inspect_workforce_reference(c,&company,&user,TimestampMillis::new(now)).await?;assert!(source.is_eligible());
            // These fields do not affect B0's operational worker-reference
            // predicate, and are never selected into the native descriptor.
            sqlx::raw_sql("UPDATE hr_people SET legal_name='PRIVATE_CHANGED_NAME',phone='PRIVATE_CHANGED_PHONE',version=version+1 WHERE id='person';UPDATE hr_engagements SET branch_id='unrelated-placement',version=version+1 WHERE id='engagement'").execute(&mut *c).await?;
            source.require_current(c,TimestampMillis::new(now)).await?;
            sqlx::query("UPDATE hr_engagements SET end_date='2026-10-27',version=version+1 WHERE id='engagement'").execute(&mut *c).await?;
            source.require_current(c,TimestampMillis::new(now)).await?;
            assert!(source.require_current(c,TimestampMillis::new(now+43_200_000)).await.is_err());
            sqlx::query("UPDATE hr_engagements SET status='suspended',version=version+1 WHERE id='engagement'").execute(&mut *c).await?;
            assert!(source.require_current(c,TimestampMillis::new(now)).await.is_err());
            Ok(())
        })).await.unwrap();
    }).await;
}
#[tokio::test]
async fn operational_workforce_reference_cannot_cross_a_transaction_or_use_an_untracked_connection()
{
    use erpx_server_people_payroll::public::workforce_reference::inspect_workforce_reference;
    use erpx_server_shared::public::TimestampMillis;
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let company = CompanyId::new("company").unwrap();
        let user = UserId::new("user").unwrap();
        let now = TimestampMillis::new(1_800_000_000_000);
        let mut c = pool.raw().acquire().await.unwrap();
        assert!(
            inspect_workforce_reference(&mut c, &company, &user, now)
                .await
                .is_err()
        );
        drop(c);
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        lock_people_authority_sources(&mut tx).await.unwrap();
        let source = inspect_workforce_reference(&mut tx, &company, &user, now)
            .await
            .unwrap();
        assert!(source.is_eligible());
        tx.commit().await.unwrap();
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *other)
            .await
            .unwrap();
        assert!(source.require_current(&mut other, now).await.is_err());
        other.rollback().await.unwrap();
    })
    .await;
}
#[tokio::test]
async fn operational_workforce_reference_preserves_real_old_snapshot_tuple_serialization_failure() {
    use erpx_server_people_payroll::public::workforce_reference::inspect_workforce_reference;
    use erpx_server_shared::public::TimestampMillis;
    fixture::with_database(2,|pool|async move{
        setup(&pool).await;
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("SELECT count(*)FROM hr_engagements").fetch_one(&mut *tx).await.unwrap();
        sqlx::query("UPDATE hr_engagements SET status='suspended',version=version+1 WHERE id='engagement'").execute(pool.raw()).await.unwrap();
        let error=inspect_workforce_reference(&mut tx,&CompanyId::new("company").unwrap(),&UserId::new("user").unwrap(),TimestampMillis::new(1_800_000_000_000)).await;
        assert!(matches!(error,Err(PgTransactionError::Persistence(error)) if error.sql_state()==Some("40001")));
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn learning_person_witness_is_scoped_transaction_bound_and_rechecks_only_person_predicate() {
    use erpx_server_people_payroll::public::learning_person::{inspect_learning_person, lock_learning_person_source};
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        assert!(inspect_learning_person(&mut c, "company", "person").await.is_err());
        drop(c);
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        // A failed inspection takes a snapshot, so do not reuse it to test preparation.
        assert!(inspect_learning_person(&mut tx, "company", "person").await.is_err());
        tx.rollback().await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_learning_person_source(&mut tx).await.unwrap();
        let current = inspect_learning_person(&mut tx, "company", "person").await.unwrap();
        assert!(current.is_active_unmerged());
        assert!(!inspect_learning_person(&mut tx, "foreign", "person").await.unwrap().is_active_unmerged());
        assert!(!inspect_learning_person(&mut tx, "company", "missing").await.unwrap().is_active_unmerged());
        // No engagement is needed, and unrelated Person fields are irrelevant.
        sqlx::query("UPDATE hr_engagements SET status='suspended' WHERE id='engagement'").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE hr_people SET legal_name='Changed' WHERE id='person'").execute(&mut *tx).await.unwrap();
        current.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE hr_people SET status='inactive' WHERE id='person'").execute(&mut *tx).await.unwrap();
        assert!(current.require_current(&mut tx).await.is_err());
        sqlx::query("UPDATE hr_people SET status='active',merged_into_id='foreign-person' WHERE id='person'").execute(&mut *tx).await.unwrap();
        assert!(current.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM hr_people WHERE id='person'").fetch_one(pool.raw()).await.unwrap(), "active");
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        lock_learning_person_source(&mut other).await.unwrap();
        assert!(current.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn learning_person_source_lock_waits_in_both_orders_and_reads_committed_change() {
    use erpx_server_people_payroll::public::learning_person::{inspect_learning_person, lock_learning_person_source};
    use sqlx::Connection;
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let mut writer = pool.raw().begin().await.unwrap();
        let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        sqlx::query("UPDATE hr_people SET status='inactive' WHERE id='person'").execute(&mut *writer).await.unwrap();
        let reader_pool = pool.clone();
        let (reader_pid_send, reader_pid_recv) = tokio::sync::oneshot::channel();
        let reader = tokio::spawn(async move {
            let mut connection = reader_pool.raw().acquire().await.unwrap();
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *connection).await.unwrap();
            let mut tx = connection.begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            reader_pid_send.send(pid).unwrap();
            lock_learning_person_source(&mut tx).await.unwrap();
            let active = inspect_learning_person(&mut tx, "company", "person").await.unwrap().is_active_unmerged();
            tx.rollback().await.unwrap(); active
        });
        let reader_pid = reader_pid_recv.await.unwrap();
        let blocked = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let found: bool = sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(writer_pid).bind(reader_pid).fetch_one(pool.raw()).await.unwrap();
                if found { break; } tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await;
        assert!(blocked.is_ok()); writer.commit().await.unwrap(); assert!(!reader.await.unwrap());
        let mut reader = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *reader).await.unwrap();
        lock_learning_person_source(&mut reader).await.unwrap();
        let source = inspect_learning_person(&mut reader, "company", "person").await.unwrap();
        assert!(!source.is_active_unmerged());
        let reader_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *reader).await.unwrap();
        let writer_pool = pool.clone();
        let (writer_pid_send, writer_pid_recv) = tokio::sync::oneshot::channel();
        let writer = tokio::spawn(async move {
            let mut connection = writer_pool.raw().acquire().await.unwrap();
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *connection).await.unwrap();
            writer_pid_send.send(pid).unwrap();
            sqlx::query("UPDATE hr_people SET status='active' WHERE id='person'").execute(&mut *connection).await.unwrap()
        });
        let writer_pid = writer_pid_recv.await.unwrap();
        let blocked = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let found: bool = sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(reader_pid).bind(writer_pid).fetch_one(pool.raw()).await.unwrap();
                if found { break; } tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await;
        assert!(blocked.is_ok()); source.require_current(&mut reader).await.unwrap(); reader.rollback().await.unwrap();
        assert_eq!(writer.await.unwrap().rows_affected(), 1);
    }).await;
}

#[tokio::test]
async fn benefit_person_retains_unordered_selection_and_full_people_source() {
    use erpx_server_people_payroll::public::benefit_person::{inspect_benefit_person, lock_benefit_people_sources};
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        sqlx::query("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES('engagement-two','person','company','second-branch','second-department','employee','salaried','active','2200-01-01','Other','THB','user',0,0)").execute(pool.raw()).await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        assert!(inspect_benefit_person(&mut tx,"company","user").await.is_err());
        tx.rollback().await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_benefit_people_sources(&mut tx).await.unwrap();
        let current = inspect_benefit_person(&mut tx,"company","user").await.unwrap();
        let selected = current.selected().unwrap();
        assert_eq!(selected.person_id, "person");
        assert!(["engagement", "engagement-two"].contains(&selected.engagement_id.as_str()));
        if selected.engagement_id == "engagement-two" { assert_eq!(selected.branch_id.as_deref(), Some("second-branch")); assert_eq!(selected.department_id.as_deref(), Some("second-department")); }
        else { assert_eq!(selected.branch_id, None); assert_eq!(selected.department_id, None); }
        assert!(inspect_benefit_person(&mut tx,"foreign","user").await.unwrap().selected().is_none());
        assert!(inspect_benefit_person(&mut tx,"company","missing").await.unwrap().selected().is_none());
        current.require_current(&mut tx).await.unwrap();
        // Any retained engagement image change matters, including an ended one.
        sqlx::query("UPDATE hr_engagements SET job_title='Not in retained image' WHERE id='ended'").execute(&mut *tx).await.unwrap();
        current.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE hr_engagements SET branch_id='changed' WHERE id='ended'").execute(&mut *tx).await.unwrap();
        assert!(current.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        lock_benefit_people_sources(&mut other).await.unwrap();
        assert!(current.require_current(&mut other).await.is_err());
        let source = inspect_benefit_person(&mut other,"company","user").await.unwrap();
        sqlx::query("UPDATE hr_people SET status='inactive' WHERE id='person'").execute(&mut *other).await.unwrap();
        assert!(source.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn benefit_person_prelock_waits_for_engagement_writer_and_reads_committed_change() {
    use erpx_server_people_payroll::public::benefit_person::{inspect_benefit_person, lock_benefit_people_sources};
    use sqlx::Connection;
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let mut writer = pool.raw().begin().await.unwrap();
        let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        sqlx::query("UPDATE hr_engagements SET status='suspended' WHERE id='engagement'").execute(&mut *writer).await.unwrap();
        let (send,recv) = tokio::sync::oneshot::channel(); let read_pool = pool.clone();
        let reader = tokio::spawn(async move {
            let mut connection=read_pool.raw().acquire().await.unwrap();
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *connection).await.unwrap();
            let mut tx=connection.begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            send.send(pid).unwrap(); lock_benefit_people_sources(&mut tx).await.unwrap();
            let none=inspect_benefit_person(&mut tx,"company","user").await.unwrap().selected().is_none();
            tx.rollback().await.unwrap(); none
        });
        let reader_pid=recv.await.unwrap();
        let blocked=tokio::time::timeout(Duration::from_secs(3),async { loop {
            let found:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(writer_pid).bind(reader_pid).fetch_one(pool.raw()).await.unwrap();
            if found {break;} tokio::time::sleep(Duration::from_millis(10)).await;
        }}).await;
        assert!(blocked.is_ok()); writer.commit().await.unwrap(); assert!(reader.await.unwrap());
    }).await;
}
