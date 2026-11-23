//! Native POS approval-queue (`posWork`) read statements on isolated PostgreSQL.
//!
//! The four `pos_read*` owner kinds retired from
//! `packages/server-pos/src/adapters/pos.ts` execute on the caller's
//! serializable transaction with fixed Company/branch scope and independence
//! filters. These tests prove the retired statements return exactly the rows
//! the retained `posWork` queue consumed: requested refunds, closed shifts and
//! unapproved cash movements excluding the viewer's own requests, plus jobs in
//! `exception` state. Company isolation, branch scope and the serializable
//! guard fail closed. The final test covers the Stripe by-id payment registry
//! statement: its `FOR UPDATE` lock is taken on the caller's unit and blocks a
//! second caller until the holder commits.
use erpx_server_pos::public::owner_writes_http::{
    POS_READPOSCASHMOVEMENTS2_242, POS_READPOSJOBS1_245, POS_READPOSREFUNDS3_230,
    POS_READPOSSHIFTS1_236, STRIPE_READPOSPAYMENTSBYID_527, owner_write_statement,
};
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, MigrationTrack, PgTransactionError, PoolLimits, connect_pool,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::CompanyId;
use serde_json::Value;
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row, postgres::PgConnectOptions};
use std::{
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

/// The runtime binds `?` positionally; convert to Postgres `$n` for direct
/// execution. None of the retired statements contains a literal `?`.
fn pg(sql: &str) -> String {
    let mut out = String::new();
    let mut chunks = sql.split('?');
    if let Some(first) = chunks.next() {
        out.push_str(first);
        let mut index = 0_usize;
        for chunk in chunks {
            index += 1;
            out.push_str(&format!("${index}"));
            out.push_str(chunk);
        }
    }
    out
}

fn bind_all<'q>(
    mut query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    values: &'q [Value],
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    for value in values {
        query = match value {
            Value::String(text) => query.bind(text.as_str()),
            Value::Null => query.bind(None::<&str>),
            Value::Bool(flag) => query.bind(*flag),
            Value::Number(number) => query.bind(number.as_i64()),
            other => panic!("unsupported owner read value: {other:?}"),
        };
    }
    query
}

async fn read_ids(
    connection: &mut PgConnection,
    company: &str,
    kind: &str,
    values: Vec<Value>,
) -> Vec<String> {
    let company = CompanyId::new(company).unwrap();
    let statement = owner_write_statement(connection, &company, kind, values)
        .await
        .unwrap();
    let mut rows = bind_all(
        sqlx::query(AssertSqlSafe(pg(&statement.sql))),
        &statement.values,
    )
        .fetch_all(&mut *connection)
        .await
        .unwrap();
    rows.sort_by(|left, right| {
        left.try_get::<String, _>("id")
            .unwrap()
            .cmp(&right.try_get::<String, _>("id").unwrap())
    });
    rows
        .into_iter()
        .map(|row| row.try_get::<String, _>("id").unwrap())
        .collect()
}

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
}

async fn setup(pool: &DatabasePool) {
    migrate(pool).await;
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1),\
         ('other','branch','other','Other',NULL,'company',NULL,'active',1),\
         ('other-co','company','other-co','Other company',NULL,'other',NULL,'active',1),\
         ('obranch','branch','obranch','Other branch',NULL,'other',NULL,'active',1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse','company','branch','WH','Warehouse','active',1),\
         ('warehouse-other','company','other','WHO','Other','active',1),\
         ('warehouse-oc','other','obranch','WOC','Other company','active',1);\
         INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active,version) VALUES \
         ('register','company','branch','warehouse','Retail','retail',true,1),\
         ('register-other','company','other','warehouse-other','Other','retail',true,1),\
         ('register-oc','other','obranch','warehouse-oc','Other company','retail',true,1);\
         INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,state,version) VALUES \
         ('shift-closed-alice','register','company','branch','alice',1,0,'closed',1),\
         ('shift-closed-bob','register','company','branch','bob',1,0,'closed',1),\
         ('shift-open-alice','register','company','branch','alice',1,0,'open',1),\
         ('shift-other','register-other','company','other','alice',1,0,'closed',1),\
         ('shift-oc','register-oc','other','obranch','alice',1,0,'closed',1);\
         INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,created_by,created_at,updated_at,version,accounts_json) VALUES \
         ('sale-1','company','branch','register','shift-closed-alice','S-1','THB','2027-01-15','alice',1,1,1,'{}'),\
         ('sale-oc','other','obranch','register-oc','shift-oc','S-OC','THB','2027-01-15','alice',1,1,1,'{}');\
         INSERT INTO pos_refunds(id,sale_id,company_id,branch_id,reason,lines_json,amount_minor,state,requested_by,created_at,accounting_date) VALUES \
         ('refund-alice','sale-1','company','branch','review','[]',100,'requested','alice',1,'2027-01-15'),\
         ('refund-bob','sale-1','company','branch','review','[]',100,'requested','bob',1,'2027-01-15'),\
         ('refund-approved','sale-1','company','branch','review','[]',100,'approved','alice',1,'2027-01-15'),\
         ('refund-branch-other','sale-1','company','other','review','[]',100,'requested','alice',1,'2027-01-15'),\
         ('refund-oc','sale-oc','other','obranch','review','[]',100,'requested','alice',1,'2027-01-15');\
         INSERT INTO pos_cash_movements(id,shift_id,amount_minor,reason,created_by,approved_by,created_at) VALUES \
         ('move-alice','shift-closed-alice',100,'float','alice',NULL,1),\
         ('move-bob','shift-closed-alice',100,'float','bob',NULL,1),\
         ('move-approved','shift-closed-alice',100,'float','carol','manager',1),\
         ('move-other','shift-other',100,'float','alice',NULL,1);\
         INSERT INTO pos_jobs(id,company_id,kind,record_id,state,created_at) VALUES \
         ('job-exc','company','checkout','r1','exception',1),\
         ('job-pending','company','checkout','r2','pending',1),\
         ('job-exc-oc','other','checkout','r3','exception',1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn with_database<F, Fut>(test: F)
where
    F: FnOnce(DatabasePool) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    sqlx::test_block_on(async move {
        let admin_url = std::env::var("ERPX_TEST_ADMIN_URL")
            .expect("ERPX_TEST_ADMIN_URL required for isolated native Postgres tests");
        let admin_options = PgConnectOptions::from_str(&admin_url).expect("invalid test admin URL");
        assert_eq!(admin_options.get_host(), "127.0.0.1");
        assert_eq!(admin_options.get_port(), 5432);
        assert_eq!(admin_options.get_database(), Some("postgres"));
        assert_eq!(admin_options.get_username(), "erpx_test_admin");
        let mut admin = PgConnection::connect_with(&admin_options)
            .await
            .expect("test administrator unavailable");
        static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);
        let token = format!(
            "{}{}{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_DATABASE.fetch_add(1, Ordering::SeqCst)
        );
        let name = format!("erpx_dev_rust_pos_work_{token}");
        let lease = format!("erpx-rust-pos-work-test:{token}");
        sqlx::raw_sql(AssertSqlSafe(format!(
            "CREATE DATABASE \"{name}\" TEMPLATE template0"
        )))
        .execute(&mut admin)
        .await
        .unwrap();
        sqlx::raw_sql(AssertSqlSafe(format!(
            "REVOKE CONNECT ON DATABASE \"{name}\" FROM PUBLIC; COMMENT ON DATABASE \"{name}\" IS '{lease}'"
        )))
        .execute(&mut admin)
        .await
        .unwrap();
        let config = DatabaseConfig::from_options(admin_options.clone().database(&name)).unwrap();
        let pool = connect_pool(
            &config,
            PoolLimits {
                max_connections: 2,
                acquire_timeout: Duration::from_secs(5),
            },
        )
        .await
        .unwrap();
        test(pool.clone()).await;
        pool.close().await;
        let ownership = sqlx::query(
            "SELECT pg_get_userbyid(datdba)::text owner,shobj_description(oid,'pg_database') lease FROM pg_database WHERE datname=$1",
        )
        .bind(&name)
        .fetch_one(&mut admin)
        .await
        .unwrap();
        assert_eq!(ownership.get::<String, _>("owner"), "erpx_test_admin");
        assert_eq!(ownership.get::<String, _>("lease"), lease);
        for attempt in 0..30 {
            match sqlx::raw_sql(AssertSqlSafe(format!("DROP DATABASE \"{name}\"")))
                .execute(&mut admin)
                .await
            {
                Ok(_) => break,
                Err(error)
                    if error.as_database_error().and_then(|e| e.code()).as_deref()
                        == Some("55006")
                        && attempt < 29 =>
                {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Err(error) => panic!(
                    "owned test database cleanup failed (SQLSTATE {:?})",
                    error.as_database_error().and_then(|e| e.code())
                ),
            }
        }
    });
}

fn scoped(company: &str, branch: Option<&str>, user: &str) -> Vec<Value> {
    vec![
        Value::String(company.to_owned()),
        branch.map_or(Value::Null, |value| Value::String(value.to_owned())),
        branch.map_or(Value::Null, |value| Value::String(value.to_owned())),
        Value::String(user.to_owned()),
    ]
}

#[test]
fn approval_queue_reads_return_only_reviewable_rows() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let mut connection = pool.raw().acquire().await.unwrap();
            let mut transaction = connection
                .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
                .await
                .unwrap();
            // Requested refunds excluding the viewer's own request and the
            // approved/other-branch/other-company rows.
            assert_eq!(
                read_ids(
                    &mut transaction,
                    "company",
                    POS_READPOSREFUNDS3_230,
                    scoped("company", Some("branch"), "bob"),
                )
                .await,
                vec!["refund-alice".to_owned()]
            );
            // Closed shifts excluding the viewer's own close and open rows.
            assert_eq!(
                read_ids(
                    &mut transaction,
                    "company",
                    POS_READPOSSHIFTS1_236,
                    scoped("company", Some("branch"), "bob"),
                )
                .await,
                vec!["shift-closed-alice".to_owned()]
            );
            // Unapproved cash movements excluding the viewer's own entry and
            // the approved row.
            assert_eq!(
                read_ids(
                    &mut transaction,
                    "company",
                    POS_READPOSCASHMOVEMENTS2_242,
                    scoped("company", Some("branch"), "bob"),
                )
                .await,
                vec!["move-alice".to_owned()]
            );
            // Payment jobs in exception state only.
            assert_eq!(
                read_ids(
                    &mut transaction,
                    "company",
                    POS_READPOSJOBS1_245,
                    vec![Value::String("company".to_owned())],
                )
                .await,
                vec!["job-exc".to_owned()]
            );
            transaction.rollback().await.unwrap();
        })
    });
}

#[test]
fn approval_queue_reads_respect_company_branch_scope() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let mut connection = pool.raw().acquire().await.unwrap();
            let mut transaction = connection
                .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
                .await
                .unwrap();
            // A Company-wide work context (NULL branch) sees every branch.
            assert_eq!(
                read_ids(
                    &mut transaction,
                    "company",
                    POS_READPOSREFUNDS3_230,
                    scoped("company", None, "bob"),
                )
                .await,
                vec!["refund-alice".to_owned(), "refund-branch-other".to_owned()]
            );
            assert_eq!(
                read_ids(
                    &mut transaction,
                    "company",
                    POS_READPOSSHIFTS1_236,
                    scoped("company", None, "bob"),
                )
                .await,
                vec!["shift-closed-alice".to_owned(), "shift-other".to_owned()]
            );
            assert_eq!(
                read_ids(
                    &mut transaction,
                    "company",
                    POS_READPOSCASHMOVEMENTS2_242,
                    scoped("company", None, "bob"),
                )
                .await,
                vec!["move-alice".to_owned(), "move-other".to_owned()]
            );
            // Another Company never leaks rows.
            assert_eq!(
                read_ids(
                    &mut transaction,
                    "other",
                    POS_READPOSREFUNDS3_230,
                    scoped("other", Some("obranch"), "bob"),
                )
                .await,
                vec!["refund-oc".to_owned()]
            );
            assert_eq!(
                read_ids(
                    &mut transaction,
                    "other",
                    POS_READPOSJOBS1_245,
                    vec![Value::String("other".to_owned())],
                )
                .await,
                vec!["job-exc-oc".to_owned()]
            );
            transaction.rollback().await.unwrap();
        })
    });
}

#[test]
fn approval_queue_reads_fail_closed() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let mut connection = pool.raw().acquire().await.unwrap();
            let mut transaction = connection
                .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
                .await
                .unwrap();
            // Unknown kinds never produce a statement.
            let company = CompanyId::new("company").unwrap();
            let error = owner_write_statement(&mut transaction, &company, "not_a_pos_read", vec![])
                .await
                .unwrap_err();
            match error {
                PgTransactionError::Business(error) => {
                    assert_eq!(error.code, "POS_OWNER_WRITE_INVALID")
                }
                other => panic!("expected invalid-kind denial, got {other:?}"),
            }
            // Arity is derived from owner SQL: three values cannot bind the
            // four placeholders of the refund queue read.
            let error = owner_write_statement(
                &mut transaction,
                &company,
                POS_READPOSREFUNDS3_230,
                vec![
                    Value::String("company".to_owned()),
                    Value::Null,
                    Value::Null,
                ],
            )
            .await
            .unwrap_err();
            match error {
                PgTransactionError::Business(error) => {
                    assert_eq!(error.code, "POS_OWNER_WRITE_INVALID")
                }
                other => panic!("expected arity denial, got {other:?}"),
            }
            transaction.rollback().await.unwrap();
            // The caller transaction must be serializable.
            let mut plain = pool.raw().acquire().await.unwrap();
            let mut read_committed = plain
                .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
                .await
                .unwrap();
            let error = owner_write_statement(
                &mut read_committed,
                &company,
                POS_READPOSJOBS1_245,
                vec![Value::String("company".to_owned())],
            )
            .await
            .unwrap_err();
            match error {
                PgTransactionError::Persistence(error) => {
                    assert_eq!(error.code, "POS_OWNER_WRITE_SERIALIZABLE_REQUIRED")
                }
                other => panic!("expected serializable guard, got {other:?}"),
            }
            read_committed.rollback().await.unwrap();
        })
    });
}

#[test]
fn stripe_payment_by_id_read_preserves_for_update_lock() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            sqlx::raw_sql(AssertSqlSafe(
                "INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,state,created_by,created_at,updated_at) VALUES \
                 ('payment-lock','sale-1','company','cash',100,100,'pending','alice',1,1)",
            ))
            .execute(pool.raw())
            .await
            .unwrap();
            let company = CompanyId::new("company").unwrap();
            let values = vec![Value::String("payment-lock".to_owned())];

            // Caller A holds the registry statement's row lock on its own
            // serializable unit, exactly like the retired TypeScript reads.
            let mut holder = pool.raw().acquire().await.unwrap();
            let mut holder_tx = holder
                .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
                .await
                .unwrap();
            let statement = owner_write_statement(
                &mut holder_tx,
                &company,
                STRIPE_READPOSPAYMENTSBYID_527,
                values.clone(),
            )
            .await
            .unwrap();
            assert!(statement.sql.ends_with(" FOR UPDATE"));
            let locked = bind_all(
                sqlx::query(AssertSqlSafe(pg(&statement.sql))),
                &statement.values,
            )
            .fetch_one(&mut *holder_tx)
            .await
            .unwrap();
            assert_eq!(locked.try_get::<String, _>("id").unwrap(), "payment-lock");

            // Caller B must block on the same row. A short lock_timeout makes
            // the contention observable without an in-flight cancelled query.
            let mut contender = pool.raw().acquire().await.unwrap();
            let mut contender_tx = contender
                .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
                .await
                .unwrap();
            sqlx::raw_sql(AssertSqlSafe("SET LOCAL lock_timeout='300ms'"))
                .execute(&mut *contender_tx)
                .await
                .unwrap();
            let statement = owner_write_statement(
                &mut contender_tx,
                &company,
                STRIPE_READPOSPAYMENTSBYID_527,
                values.clone(),
            )
            .await
            .unwrap();
            let blocked = bind_all(
                sqlx::query(AssertSqlSafe(pg(&statement.sql))),
                &statement.values,
            )
            .fetch_one(&mut *contender_tx)
            .await;
            let error = blocked.expect_err("FOR UPDATE must block the second caller");
            assert_eq!(
                error.as_database_error().and_then(|e| e.code()).as_deref(),
                Some("55P03"),
                "contention must be a lock timeout, got {error:?}"
            );
            contender_tx.rollback().await.unwrap();
            drop(contender);

            // Once the holder commits, the same registry statement succeeds.
            holder_tx.commit().await.unwrap();
            drop(holder);
            let mut after = pool.raw().acquire().await.unwrap();
            let mut after_tx = after
                .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
                .await
                .unwrap();
            let statement = owner_write_statement(
                &mut after_tx,
                &company,
                STRIPE_READPOSPAYMENTSBYID_527,
                values,
            )
            .await
            .unwrap();
            let row = bind_all(
                sqlx::query(AssertSqlSafe(pg(&statement.sql))),
                &statement.values,
            )
            .fetch_one(&mut *after_tx)
            .await
            .unwrap();
            assert_eq!(row.try_get::<String, _>("id").unwrap(), "payment-lock");
            after_tx.rollback().await.unwrap();
        })
    });
}
