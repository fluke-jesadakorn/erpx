//! Native POS cash-movement command on migrated PostgreSQL.
//! These tests prove owner SQL, Company/branch isolation, exact CAS, command
//! idempotency, POS audit persistence, rollback and committed readback. They do
//! not prove a Next/browser journey, Finance/Inventory effects or composition
//! mounting.
use erpx_server_pos::public::cash_movement::{
    CashMovementActor, CashMovementReceipt, CreateCashMovement, PgCashMovementCommand,
    create_cash_movement, valid_uuid,
};
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, PoolLimits,
    TransactionOptions, connect_pool, load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    TimestampMillis, UserId,
};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row, postgres::PgConnectOptions};
use std::{str::FromStr, time::Duration};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn actor(company: &str, branch: Option<&str>) -> CashMovementActor {
    CashMovementActor {
        user_id: UserId::new("owner").unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: None,
        assignment_id: AssignmentId::new("assignment").unwrap(),
        permissions: vec!["pos.sell".into()],
        modules: vec!["pos".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn command(key: &str, shift: &str, version: i64, amount: i64) -> CreateCashMovement {
    CreateCashMovement {
        key: CommandKey::new(key).unwrap(),
        shift_id: RecordId::new(shift).unwrap(),
        expected_version: version,
        amount_minor: amount,
        reason: "  Opening float top-up  ".into(),
    }
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
         ('branch-b','branch','branch-b','Branch B',NULL,'company',NULL,'active',1),\
         ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active',1),\
         ('foreign-branch','branch','foreign-branch','Foreign branch',NULL,'foreign',NULL,'active',1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse','company','branch','WH','Warehouse','active',1),\
         ('warehouse-b','company','branch-b','WHB','Warehouse B','active',1),\
         ('warehouse-f','foreign','foreign-branch','WHF','Foreign warehouse','active',1);\
         INSERT INTO pos_settings(company_id,enabled,currency,accounts_json,version) VALUES \
         ('company',true,'THB','{}',1),('foreign',true,'USD','{}',1);\
         INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active,version) VALUES \
         ('register','company','branch','warehouse','Retail','retail',true,1),\
         ('register-b','company','branch-b','warehouse-b','Retail B','retail',true,1),\
         ('register-f','foreign','foreign-branch','warehouse-f','Foreign','retail',true,1);\
         INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,state,version,business_date) VALUES \
         ('shift-1','register','company','branch','owner',1800000000000,100000,'open',1,'2026-11-04'),\
         ('shift-b','register-b','company','branch-b','owner',1800000000000,200000,'open',1,'2026-11-04'),\
         ('shift-f','register-f','foreign','foreign-branch','owner',1800000000000,300000,'open',1,'2026-11-04');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn count(pool: &DatabasePool, table: &str, filter: &str) -> i64 {
    let sql = format!("SELECT count(*) FROM {table} WHERE {filter}");
    sqlx::query_scalar(AssertSqlSafe(sql))
        .fetch_one(pool.raw())
        .await
        .unwrap()
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
        let token = format!(
            "{}{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let name = format!("erpx_dev_rust_{token}");
        let lease = format!("erpx-rust-test:{token}");
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

async fn run(
    pool: &DatabasePool,
    actor: CashMovementActor,
    command: CreateCashMovement,
) -> Result<CashMovementReceipt, PgTransactionError<BusinessError>> {
    PgUnitOfWork::new(pool.clone())
        .run::<CashMovementReceipt, BusinessError, _>(
            TransactionOptions::default(),
            move |connection| {
                let actor = actor.clone();
                let command = command.clone();
                Box::pin(async move {
                    let mut repository = PgCashMovementCommand::new(connection);
                    create_cash_movement(&actor, &command, &mut repository, NOW).await
                })
            },
        )
        .await
}

#[test]
fn creates_movement_bumps_shift_and_persists_receipt_and_audit() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let receipt = run(
                &pool,
                actor("company", Some("branch")),
                command("cash-command-0001", "shift-1", 1, 50_000),
            )
            .await
            .unwrap();
            assert!(valid_uuid(&receipt.id));

            let movement = sqlx::query(
                "SELECT id,shift_id,amount_minor,reason,created_by,approved_by,created_at \
                 FROM pos_cash_movements WHERE id=$1",
            )
            .bind(&receipt.id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(movement.get::<String, _>("shift_id"), "shift-1");
            assert_eq!(movement.get::<i64, _>("amount_minor"), 50_000);
            assert_eq!(movement.get::<String, _>("reason"), "Opening float top-up");
            assert_eq!(movement.get::<String, _>("created_by"), "owner");
            assert_eq!(movement.get::<Option<String>, _>("approved_by"), None);
            assert_eq!(movement.get::<i64, _>("created_at"), NOW.get());

            let shift = sqlx::query("SELECT state,version FROM pos_shifts WHERE id='shift-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
            assert_eq!(shift.get::<String, _>("state"), "open");
            assert_eq!(shift.get::<i64, _>("version"), 2);

            let receipt_row = sqlx::query(
                "SELECT company_id,request_hash,result_json,created_at FROM pos_receipts WHERE id=$1",
            )
            .bind("cash-command-0001")
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(receipt_row.get::<String, _>("company_id"), "company");
            let hash = receipt_row.get::<String, _>("request_hash");
            assert_eq!(hash.len(), 64);
            assert!(hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
            assert_eq!(
                receipt_row.get::<String, _>("result_json"),
                format!("{{\"id\":\"{}\"}}", receipt.id)
            );
            assert_eq!(receipt_row.get::<i64, _>("created_at"), NOW.get());

            let audit = sqlx::query(
                "SELECT company_id,branch_id,actor_id,operation,record_id,details_json FROM pos_audit WHERE record_id=$1",
            )
            .bind(&receipt.id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(audit.get::<String, _>("company_id"), "company");
            assert_eq!(audit.get::<String, _>("branch_id"), "branch");
            assert_eq!(audit.get::<String, _>("actor_id"), "owner");
            assert_eq!(audit.get::<String, _>("operation"), "cash.create");
            let details: serde_json::Value =
                serde_json::from_str(&audit.get::<String, _>("details_json")).unwrap();
            assert_eq!(details["input"]["id"], "shift-1");
            assert_eq!(details["input"]["expectedVersion"], 1);
            assert_eq!(details["input"]["amountMinor"], 50_000);
            assert_eq!(details["input"]["reason"], "Opening float top-up");
            assert_eq!(details["result"]["id"], receipt.id);
            assert_eq!(details["assignmentId"], "assignment");

            assert_eq!(
                count(&pool, "pos_cash_movements", "shift_id='shift-1'").await,
                1
            );
            assert_eq!(count(&pool, "pos_audit", "company_id='company'").await, 1);
        })
    });
}

#[test]
fn replay_returns_the_original_receipt_without_duplicate_rows() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let command = || command("cash-command-0001", "shift-1", 1, 50_000);
            let first = run(&pool, actor("company", Some("branch")), command())
                .await
                .unwrap();
            for _ in 0..2 {
                let replay = run(&pool, actor("company", Some("branch")), command())
                    .await
                    .unwrap();
                assert_eq!(replay.id, first.id);
            }
            assert_eq!(
                count(&pool, "pos_cash_movements", "shift_id='shift-1'").await,
                1
            );
            assert_eq!(
                count(&pool, "pos_receipts", "id='cash-command-0001'").await,
                1
            );
            assert_eq!(count(&pool, "pos_audit", "company_id='company'").await, 1);
            let version: i64 =
                sqlx::query_scalar("SELECT version FROM pos_shifts WHERE id='shift-1'")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(version, 2);
        })
    });
}

#[test]
fn idempotency_conflict_and_stale_cas_leave_no_new_rows() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let first = run(
                &pool,
                actor("company", Some("branch")),
                command("cash-command-0001", "shift-1", 1, 50_000),
            )
            .await
            .unwrap();

            let conflict = business(
                run(
                    &pool,
                    actor("company", Some("branch")),
                    command("cash-command-0001", "shift-1", 1, 60_000),
                )
                .await
                .unwrap_err(),
            );
            assert_eq!(conflict.kind, FailureKind::Conflict);
            assert_eq!(conflict.code, "IDEMPOTENCY_CONFLICT");

            let stale = business(
                run(
                    &pool,
                    actor("company", Some("branch")),
                    command("cash-command-0002", "shift-1", 1, 60_000),
                )
                .await
                .unwrap_err(),
            );
            assert_eq!(stale.kind, FailureKind::Conflict);
            assert_eq!(stale.code, "POS_VERSION_CONFLICT");
            assert_eq!(
                count(&pool, "pos_cash_movements", "shift_id='shift-1'").await,
                1
            );
            assert_eq!(
                count(&pool, "pos_receipts", "company_id='company'").await,
                1
            );
            assert_eq!(count(&pool, "pos_audit", "company_id='company'").await, 1);
            let version: i64 =
                sqlx::query_scalar("SELECT version FROM pos_shifts WHERE id='shift-1'")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(version, 2);

            let next = run(
                &pool,
                actor("company", Some("branch")),
                command("cash-command-0002", "shift-1", 2, 60_000),
            )
            .await
            .unwrap();
            assert_ne!(next.id, first.id);
            assert_eq!(
                count(&pool, "pos_cash_movements", "shift_id='shift-1'").await,
                2
            );
            let version: i64 =
                sqlx::query_scalar("SELECT version FROM pos_shifts WHERE id='shift-1'")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(version, 3);
        })
    });
}

#[test]
fn company_branch_and_scope_isolation_are_enforced() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            for actor in [
                actor("foreign", Some("foreign-branch")),
                actor("company", Some("branch-b")),
                actor("company", Some("unknown-branch")),
            ] {
                let error = business(
                    run(
                        &pool,
                        actor,
                        command("cash-command-0001", "shift-1", 1, 50_000),
                    )
                    .await
                    .unwrap_err(),
                );
                assert_eq!(error.kind, FailureKind::NotFound);
                assert_eq!(error.code, "POS_NOT_FOUND");
            }
            assert_eq!(
                count(&pool, "pos_cash_movements", "shift_id='shift-1'").await,
                0
            );

            let company_wide = run(
                &pool,
                actor("company", None),
                command("cash-command-0003", "shift-b", 1, 70_000),
            )
            .await
            .unwrap();
            assert!(valid_uuid(&company_wide.id));
            let foreign = run(
                &pool,
                actor("foreign", Some("foreign-branch")),
                command("cash-command-0004", "shift-f", 1, 80_000),
            )
            .await
            .unwrap();
            assert!(valid_uuid(&foreign.id));
            assert_eq!(
                count(&pool, "pos_cash_movements", "shift_id='shift-1'").await,
                0
            );
            assert_eq!(
                count(&pool, "pos_cash_movements", "shift_id='shift-b'").await,
                1
            );
            assert_eq!(
                count(&pool, "pos_cash_movements", "shift_id='shift-f'").await,
                1
            );
        })
    });
}

#[test]
fn day_close_and_missing_setup_fail_before_any_write() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            sqlx::raw_sql("DELETE FROM pos_settings WHERE company_id='company'")
                .execute(pool.raw())
                .await
                .unwrap();
            let error = business(
                run(
                    &pool,
                    actor("company", Some("branch-b")),
                    command("cash-command-0002", "shift-b", 1, 50_000),
                )
                .await
                .unwrap_err(),
            );
            assert_eq!(error.kind, FailureKind::Conflict);
            assert_eq!(error.code, "POS_SETUP_REQUIRED");
            assert_eq!(
                count(&pool, "pos_cash_movements", "shift_id='shift-b'").await,
                0
            );

            sqlx::raw_sql(
                "INSERT INTO pos_settings(company_id,enabled,currency,accounts_json,version) \
                 VALUES('company',true,'THB','{}',1);\
                 INSERT INTO pos_day_closes(id,company_id,branch_id,business_date,currency,state,snapshot_json,source_digest,prepared_by,prepared_at,version) \
                 VALUES('day-1','company','branch','2026-11-04','THB','prepared','{}','digest','owner',1800000000000,1)",
            )
            .execute(pool.raw())
            .await
            .unwrap();
            let error = business(
                run(
                    &pool,
                    actor("company", Some("branch")),
                    command("cash-command-0001", "shift-1", 1, 50_000),
                )
                .await
                .unwrap_err(),
            );
            assert_eq!(error.code, "POS_DAY_CLOSED");
            assert_eq!(
                count(&pool, "pos_cash_movements", "shift_id='shift-1'").await,
                0
            );
            assert_eq!(
                count(&pool, "pos_receipts", "company_id='company'").await,
                0
            );
            assert_eq!(count(&pool, "pos_audit", "company_id='company'").await, 0);
            let version: i64 =
                sqlx::query_scalar("SELECT version FROM pos_shifts WHERE id='shift-1'")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(version, 1);
        })
    });
}

#[test]
fn rollback_clears_owner_rows_and_committed_readback_survives() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let rollback = PgUnitOfWork::new(pool.clone())
                .run::<(), BusinessError, _>(TransactionOptions::default(), |connection| {
                    Box::pin(async move {
                        let current = actor("company", Some("branch"));
                        let command = command("cash-command-0001", "shift-1", 1, 50_000);
                        let mut repository = PgCashMovementCommand::new(connection);
                        let receipt =
                            create_cash_movement(&current, &command, &mut repository, NOW).await?;
                        assert!(valid_uuid(&receipt.id));
                        Err(PgTransactionError::Business(BusinessError::new(
                            FailureKind::Conflict,
                            "ROLLBACK_TEST",
                            "Force rollback after a complete owner write.",
                        )))
                    })
                })
                .await
                .unwrap_err();
            assert_eq!(business(rollback).code, "ROLLBACK_TEST");
            assert_eq!(
                count(&pool, "pos_cash_movements", "shift_id='shift-1'").await,
                0
            );
            assert_eq!(
                count(&pool, "pos_receipts", "company_id='company'").await,
                0
            );
            assert_eq!(count(&pool, "pos_audit", "company_id='company'").await, 0);
            let version: i64 =
                sqlx::query_scalar("SELECT version FROM pos_shifts WHERE id='shift-1'")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(version, 1);

            let receipt = run(
                &pool,
                actor("company", Some("branch")),
                command("cash-command-0001", "shift-1", 1, 50_000),
            )
            .await
            .unwrap();
            let expected_id = receipt.id.clone();
            let readback = PgUnitOfWork::new(pool.clone())
                .run::<(String, i64, String), BusinessError, _>(
                    TransactionOptions::default(),
                    move |connection| {
                        let id = receipt.id.clone();
                        Box::pin(async move {
                            let mut repository = PgCashMovementCommand::new(connection);
                            let value = erpx_server_pos::public::cash_movement::CashMovementRepository::readback(
                                &mut repository,
                                &id,
                                "shift-1",
                            )
                            .await?
                            .ok_or_else(|| {
                                PgTransactionError::Business(BusinessError::new(
                                    FailureKind::Unavailable,
                                    "READBACK_MISSING",
                                    "Missing readback.",
                                ))
                            })?;
                            Ok((value.id, value.shift_version, value.reason))
                        })
                    },
                )
                .await
                .unwrap();
            assert_eq!(readback, (expected_id, 2, "Opening float top-up".into()));
        })
    });
}

#[test]
fn a_non_serializable_or_missing_transaction_fails_closed() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let mut connection = pool.raw().acquire().await.unwrap();
            sqlx::query("BEGIN ISOLATION LEVEL READ COMMITTED")
                .execute(&mut *connection)
                .await
                .unwrap();
            let error = {
                let current = actor("company", Some("branch"));
                let command = command("cash-command-0001", "shift-1", 1, 50_000);
                let mut repository = PgCashMovementCommand::new(&mut connection);
                create_cash_movement(&current, &command, &mut repository, NOW)
                    .await
                    .unwrap_err()
            };
            match error {
                PgTransactionError::Persistence(error) => {
                    assert_eq!(error.code, "POS_CASH_SERIALIZABLE_REQUIRED")
                }
                other => panic!("expected serializable guard, got {other:?}"),
            }
            sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .unwrap();
            drop(connection);
            assert_eq!(
                count(&pool, "pos_cash_movements", "shift_id='shift-1'").await,
                0
            );

            let error = {
                let mut connection = pool.raw().acquire().await.unwrap();
                let current = actor("company", Some("branch"));
                let command = command("cash-command-0001", "shift-1", 1, 50_000);
                let mut repository = PgCashMovementCommand::new(&mut connection);
                create_cash_movement(&current, &command, &mut repository, NOW)
                    .await
                    .unwrap_err()
            };
            match error {
                PgTransactionError::Persistence(error) => {
                    assert_eq!(error.code, "POS_CASH_SERIALIZABLE_REQUIRED")
                }
                other => panic!("expected transaction guard, got {other:?}"),
            }
        })
    });
}
