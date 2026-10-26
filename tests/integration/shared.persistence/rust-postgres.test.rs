#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_runtime::public::postgres::{
    PgTransactionError, PgUnitOfWork, SessionLock, TransactionOptions,
};
use fixture::with_database;
use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[tokio::test]
async fn serializable_unit_of_work_rolls_back_receipt_audit_outbox_and_owner_write() {
    with_database(2,|pool|async move {
        sqlx::raw_sql("CREATE TABLE owner_write(id int PRIMARY KEY); CREATE TABLE receipt(id int); CREATE TABLE audit(id int); CREATE TABLE outbox(id int)").execute(pool.raw()).await.unwrap();
        let uow=PgUnitOfWork::new(pool.clone());
        let result=uow.run(TransactionOptions::default(),|connection|Box::pin(async move {
            sqlx::raw_sql("INSERT INTO owner_write VALUES(1); INSERT INTO receipt VALUES(1); INSERT INTO audit VALUES(1); INSERT INTO outbox VALUES(1)").execute(connection).await?;
            Err::<(),_>(PgTransactionError::Business("authority_revoked"))
        })).await;
        assert!(matches!(result,Err(PgTransactionError::Business("authority_revoked"))));
        let count:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM owner_write)+(SELECT count(*) FROM receipt)+(SELECT count(*) FROM audit)+(SELECT count(*) FROM outbox)").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(count,0);
        let isolation:String=uow.run::<_,Infallible,_>(TransactionOptions::default(),|connection|Box::pin(async move {Ok(sqlx::query_scalar("SHOW transaction_isolation").fetch_one(connection).await?)})).await.unwrap();
        assert_eq!(isolation,"serializable");
    }).await;
}
#[tokio::test]
async fn serialization_retry_replays_the_complete_callback_with_no_partial_writes() {
    with_database(2,|pool|async move {
        sqlx::raw_sql("CREATE SEQUENCE attempt; CREATE TABLE replayed(id int); CREATE FUNCTION serialization_probe() RETURNS void LANGUAGE plpgsql AS $$ BEGIN IF nextval('attempt') < 3 THEN RAISE EXCEPTION 'retry' USING ERRCODE='40001'; END IF; END $$;").execute(pool.raw()).await.unwrap();
        let attempts=Arc::new(AtomicUsize::new(0));let observed=attempts.clone();
        PgUnitOfWork::new(pool.clone()).run::<_,Infallible,_>(TransactionOptions::default(),move|connection|{let observed=observed.clone();Box::pin(async move {
            observed.fetch_add(1,Ordering::SeqCst);
            sqlx::query("INSERT INTO replayed VALUES(1)").execute(&mut *connection).await?;
            sqlx::query("SELECT serialization_probe()").execute(connection).await?; Ok(())
        })}).await.unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst),3);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM replayed").fetch_one(pool.raw()).await.unwrap(),1);
    }).await;
}
#[tokio::test]
async fn keyed_waiters_do_not_exhaust_the_pool_and_snapshot_starts_after_the_session_lock() {
    with_database(2, |pool| async move {
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let options = TransactionOptions {
            session_lock: Some(SessionLock {
                namespace: "test".into(),
                key: "same-owner".into(),
            }),
            prefer_indexed_reads: true,
        };
        let uow = PgUnitOfWork::new(pool.clone());
        let first = {
            let uow = uow.clone();
            let entered = entered.clone();
            let release = release.clone();
            let options = options.clone();
            tokio::spawn(async move {
                uow.run::<_, Infallible, _>(options, move |conn| {
                    let entered = entered.clone();
                    let release = release.clone();
                    Box::pin(async move {
                        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                            .fetch_one(&mut *conn)
                            .await?;
                        entered.notify_one();
                        release.notified().await;
                        let second: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                            .fetch_one(conn)
                            .await?;
                        assert_eq!(pid, second);
                        Ok(())
                    })
                })
                .await
                .unwrap()
            })
        };
        entered.notified().await;
        let mut waiters = Vec::new();
        for _ in 0..8 {
            let uow = uow.clone();
            let options = options.clone();
            waiters.push(tokio::spawn(async move {
                uow.run::<_, Infallible, _>(options, |conn| {
                    Box::pin(async move {
                        sqlx::query("SELECT 1").execute(conn).await?;
                        Ok(())
                    })
                })
                .await
                .unwrap()
            }));
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(
            tokio::time::timeout(
                Duration::from_millis(500),
                sqlx::query_scalar::<_, i32>("SELECT 42").fetch_one(pool.raw())
            )
            .await
            .unwrap()
            .unwrap(),
            42
        );
        release.notify_one();
        first.await.unwrap();
        for waiter in waiters {
            waiter.await.unwrap();
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM pg_locks WHERE locktype='advisory' AND pid=pg_backend_pid()"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
        );
    })
    .await;
}
#[tokio::test]
async fn cancelling_a_session_locked_transaction_discards_and_releases_its_backend() {
    with_database(1, |pool| async move {
        let entered = Arc::new(tokio::sync::Notify::new());
        let task = {
            let uow = PgUnitOfWork::new(pool.clone());
            let entered = entered.clone();
            tokio::spawn(async move {
                uow.run::<_, Infallible, _>(
                    TransactionOptions {
                        session_lock: Some(SessionLock {
                            namespace: "cancel".into(),
                            key: "owner".into(),
                        }),
                        prefer_indexed_reads: false,
                    },
                    move |_| {
                        let entered = entered.clone();
                        Box::pin(async move {
                            entered.notify_one();
                            std::future::pending::<Result<(), PgTransactionError<Infallible>>>()
                                .await
                        })
                    },
                )
                .await
            })
        };
        entered.notified().await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let mut connection = pool.raw().acquire().await.unwrap();
        assert!(
            sqlx::query_scalar::<_, bool>(
                "SELECT pg_try_advisory_lock(hashtext('cancel'),hashtext('owner'))"
            )
            .fetch_one(&mut *connection)
            .await
            .unwrap()
        );
        assert!(
            sqlx::query_scalar::<_, bool>(
                "SELECT pg_advisory_unlock(hashtext('cancel'),hashtext('owner'))"
            )
            .fetch_one(&mut *connection)
            .await
            .unwrap()
        );
    })
    .await;
}

#[tokio::test]
async fn retry_budget_is_four_callbacks_and_business_failures_are_never_retried() {
    with_database(2,|pool|async move {
        sqlx::raw_sql("CREATE FUNCTION deadlock_probe() RETURNS void LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'retry' USING ERRCODE='40P01'; END $$;").execute(pool.raw()).await.unwrap();
        let attempts=Arc::new(AtomicUsize::new(0));let observed=attempts.clone();
        let uow=PgUnitOfWork::new(pool.clone());
        let failed=uow.run::<_,Infallible,_>(TransactionOptions::default(),move|conn|{let observed=observed.clone();Box::pin(async move {observed.fetch_add(1,Ordering::SeqCst);sqlx::query("SELECT deadlock_probe()").execute(conn).await?;Ok(())})}).await;
        assert!(matches!(failed,Err(PgTransactionError::Persistence(ref error)) if error.sql_state()==Some("40P01")));
        assert_eq!(attempts.load(Ordering::SeqCst),4);
        let attempts=Arc::new(AtomicUsize::new(0));let observed=attempts.clone();
        let failed=uow.run(TransactionOptions::default(),move|_|{let observed=observed.clone();Box::pin(async move {observed.fetch_add(1,Ordering::SeqCst);Err::<(),_>(PgTransactionError::Business("authority_revoked"))})}).await;
        assert!(matches!(failed,Err(PgTransactionError::Business("authority_revoked"))));assert_eq!(attempts.load(Ordering::SeqCst),1);
    }).await;
}

#[tokio::test]
async fn request_authorization_lease_never_replays_a_callback_after_a_holding_commit() {
    use erpx_server_runtime::public::postgres::PgRequestLease;
    with_database(2,|control|async move {
        sqlx::raw_sql("CREATE TABLE current_account(id int PRIMARY KEY,active boolean); INSERT INTO current_account VALUES(1,true); CREATE FUNCTION retry_after_commit() RETURNS void LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'must not replay' USING ERRCODE='40001'; END $$;").execute(control.raw()).await.unwrap();
        with_database(2,move|holding|async move {
            sqlx::query("CREATE TABLE durable_owner_write(id int PRIMARY KEY)").execute(holding.raw()).await.unwrap();
            let attempts=Arc::new(AtomicUsize::new(0));let observed=attempts.clone();
            let holding_uow=PgUnitOfWork::new(holding.clone());
            let result=PgRequestLease::new(control.clone()).run::<_,Infallible,_>(move|connection|Box::pin(async move {
                observed.fetch_add(1,Ordering::SeqCst);
                assert_eq!(sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *connection).await?,"read committed");
                sqlx::query("LOCK TABLE current_account IN SHARE MODE").execute(&mut *connection).await?;
                holding_uow.run::<_,Infallible,_>(TransactionOptions::default(),|holding_conn|Box::pin(async move {sqlx::query("INSERT INTO durable_owner_write VALUES(1)").execute(holding_conn).await?;Ok(())})).await?;
                sqlx::query("SELECT retry_after_commit()").execute(connection).await?;
                Ok(())
            })).await;
            assert!(matches!(result,Err(PgTransactionError::Persistence(ref error)) if error.sql_state()==Some("40001")));
            assert_eq!(attempts.load(Ordering::SeqCst),1);
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM durable_owner_write").fetch_one(holding.raw()).await.unwrap(),1);
        }).await;
    }).await;
}
