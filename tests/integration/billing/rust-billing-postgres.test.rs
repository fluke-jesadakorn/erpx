//! Native Billing workspace read and cost-review commands on migrated
//! PostgreSQL. These tests prove owner SQL, Holding isolation, exact account
//! CAS, command idempotency, audit/receipt persistence, rollback and committed
//! readback. They do not prove a Next/browser journey or composition mounting.
use erpx_server_billing::public::cost::{
    BillingCostCommand, CompleteCostReview, DeleteCost, PgBillingCostCommand, RecordCost,
    execute_cost_command,
};
use erpx_server_billing::public::workspace::{BillingActor, PgBillingWorkspace, billing_workspace};
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, PoolLimits,
    TransactionOptions, connect_pool, load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, HoldingId, RecordId, TimestampMillis, UserId,
};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row, postgres::PgConnectOptions};
use std::{str::FromStr, time::Duration};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const PERIOD_START: i64 = 1_778_000_000_000;
const PERIOD_END: i64 = 1_780_000_000_000;

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn target() -> HoldingId {
    HoldingId::new("h001").unwrap()
}

fn actor() -> BillingActor {
    BillingActor {
        user_id: UserId::new("admin").unwrap(),
        assignment_id: AssignmentId::new("assignment").unwrap(),
        holding_id: None,
        platform: true,
        role: "platform_admin".into(),
        expires_at: None,
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn record(key_value: &str, version: i64) -> BillingCostCommand {
    BillingCostCommand::Record(RecordCost {
        key: key(key_value),
        expected_version: version,
        reason: "Reviewed actual cost".into(),
        period_id: RecordId::new("period-10").unwrap(),
        cost_id: None,
        category: "hosting".into(),
        amount_satang: 12_500,
        service_start: PERIOD_START,
        service_end: PERIOD_END,
        note: "  Provider hosting  ".into(),
        simulated: false,
        evidence_id: Some(RecordId::new("evidence-1").unwrap()),
        original_invoice_id: None,
    })
}

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::Control)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::Control, &files)
        .await
        .unwrap();
}

async fn setup(pool: &DatabasePool) {
    migrate(pool).await;
    sqlx::raw_sql(
        "INSERT INTO holding_bindings\
         (holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES \
         ('h001','acme','Acme','write-h001','read-h001','files-h001','active'),\
         ('h002','northstar','Northstar','write-h002','read-h002','files-h002','active');\
         INSERT INTO billing_accounts(holding_id,version,customer_id,payment_mode,created_at) VALUES \
         ('h001',4,'customer-1','manual',100),\
         ('h002',1,'customer-2','manual',100);\
         INSERT INTO billing_enrollments(id,holding_id,plan_id,started_at) VALUES \
         ('enrollment-1','h001','standard',1770000000000),\
         ('enrollment-2','h002','standard',1770000000000);\
         INSERT INTO billing_periods\
         (id,holding_id,enrollment_id,month,start_at,end_at,plan_id,plan_revision,support_satang,costs_complete) VALUES \
         ('period-10','h001','enrollment-1','2026-10',1778000000000,1780000000000,'standard',1,50000,true),\
         ('period-foreign','h002','enrollment-2','2026-10',1778000000000,1780000000000,'standard',1,50000,true);\
         INSERT INTO billing_costs\
         (id,holding_id,period_id,category,amount_satang,service_start,service_end,note,simulated,evidence_id,created_at) VALUES \
         ('cost-1','h001','period-10','hosting',12500,1778000000000,1780000000000,'Provider hosting',false,'evidence-1',500),\
         ('cost-foreign','h002','period-foreign','hosting',9900,1778000000000,1780000000000,'Foreign hosting',false,NULL,500);\
         INSERT INTO billing_invoices\
         (id,holding_id,period_id,kind,status,total_satang,lines_json,payment_mode,created_at,approved_by,updated_at) VALUES \
         ('invoice-1','h001','period-10','main','open',62500,'[{\"description\":\"Hosting\",\"amount\":12500,\"start\":1778000000000,\"end\":1780000000000}]','manual',600,'admin',600),\
         ('invoice-foreign','h002','period-foreign','main','open',50000,'[]','manual',600,'admin',600);\
         INSERT INTO billing_audit(id,holding_id,actor,action,reason,details_json,created_at) VALUES \
         ('audit-1','h001','admin','cost','Reviewed actual cost','{}',600),\
         ('audit-foreign','h002','admin','cost','Foreign cost','{}',600);\
         INSERT INTO billing_jobs(id,holding_id,invoice_id,kind,next_at,created_at) VALUES \
         ('job-1','h001','invoice-1','manual',700,700),\
         ('job-foreign','h002','invoice-foreign','manual',700,700);\
         INSERT INTO beta_billing(holding_id,customer_id,status,last_event_at,updated_at) VALUES \
         ('h001','customer-1','active',600,600),\
         ('h002','customer-2','active',600,600);\
         INSERT INTO stripe_inbox(id,event_type,created_at,state,payload_json,last_error) VALUES \
         ('inbox-1','invoice.payment_failed',600,'pending','{\"data\":{\"object\":{\"customer\":\"customer-1\"}}}','provider retry'),\
         ('inbox-foreign','invoice.payment_failed',600,'pending','{\"data\":{\"object\":{\"customer\":\"customer-2\"}}}','foreign retry');\
         INSERT INTO billing_evidence\
         (id,holding_id,filename,mime,size,sha256,actor,created_at,object_key,detected_mime,source_state,scanned_at,scanner_definition_at) VALUES \
         ('evidence-1','h001','provider.pdf','application/pdf',3,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','admin',500,'h001/evidence-1','application/pdf','available',500,500),\
         ('evidence-foreign','h002','foreign.pdf','application/pdf',3,'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb','admin',500,'h002/evidence-foreign','application/pdf','available',500,500);",
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

async fn read(pool: &DatabasePool, actor: BillingActor) -> Result<(), BusinessError> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            Box::pin(async move {
                let mut repository = PgBillingWorkspace::new(connection);
                billing_workspace(&actor, &target(), &mut repository, NOW)
                    .await
                    .map(|_| ())
            })
        })
        .await
        .map_err(business)
}

#[test]
fn workspace_reads_only_the_current_holding_and_fails_closed_on_foreign_or_missing_actor_scope() {
    with_database(|pool| async move {
        setup(&pool).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let mut repository = PgBillingWorkspace::new(connection);
                    let workspace = billing_workspace(&actor(), &target(), &mut repository, NOW)
                        .await
                        .unwrap();
                    assert_eq!(workspace.account.as_ref().unwrap().holding_id, "h001");
                    assert_eq!(workspace.periods.len(), 1);
                    assert_eq!(workspace.periods[0].id, "period-10");
                    assert_eq!(workspace.costs.len(), 1);
                    assert_eq!(workspace.costs[0].id, "cost-1");
                    assert_eq!(workspace.invoices.len(), 1);
                    assert_eq!(workspace.audit[0].id, "audit-1");
                    assert_eq!(workspace.jobs[0].id, "job-1");
                    assert_eq!(workspace.legacy.as_ref().unwrap().subscription_id, None);
                    Ok(())
                })
            })
            .await
            .unwrap();

        let mut owner = actor();
        owner.platform = false;
        owner.role = "holding_owner".into();
        owner.holding_id = Some(target());
        assert!(read(&pool, owner).await.is_ok());

        let mut foreign = actor();
        foreign.platform = false;
        foreign.role = "holding_owner".into();
        foreign.holding_id = Some(HoldingId::new("h002").unwrap());
        assert_eq!(read(&pool, foreign).await.unwrap_err().code, "FORBIDDEN");
    });
}

#[test]
fn record_replay_complete_and_delete_persist_cost_audit_receipt_and_versions() {
    with_database(|pool| async move {
        setup(&pool).await;
        let command = record("billing-command-0001", 4);
        let receipt = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                let command = command.clone();
                Box::pin(async move {
                    let mut repository = PgBillingCostCommand::new(connection);
                    execute_cost_command(&actor(), &target(), &command, &mut repository, NOW).await
                })
            })
            .await
            .unwrap();
        assert_eq!(receipt.result_json, "{\"ok\":true}");
        assert_eq!(
            count(&pool, "billing_accounts", "holding_id='h001' AND version=5").await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "billing_costs",
                "id='billing-cost-billing-command-0001' AND holding_id='h001' AND note='Provider hosting'"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "billing_periods",
                "id='period-10' AND costs_complete=FALSE"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "billing_audit",
                "action='cost' AND holding_id='h001'"
            )
            .await,
            2
        );
        assert_eq!(
            count(&pool, "billing_receipts", "id='billing-command-0001'").await,
            1
        );

        let replay = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                let command = command.clone();
                Box::pin(async move {
                    let mut repository = PgBillingCostCommand::new(connection);
                    execute_cost_command(&actor(), &target(), &command, &mut repository, NOW).await
                })
            })
            .await
            .unwrap();
        assert_eq!(replay.result_json, receipt.result_json);
        assert_eq!(
            count(
                &pool,
                "billing_audit",
                "action='cost' AND holding_id='h001'"
            )
            .await,
            2
        );
        assert_eq!(count(&pool, "billing_costs", "holding_id='h001'").await, 2);

        let mut changed = record("billing-command-0001", 5);
        if let BillingCostCommand::Record(record) = &mut changed {
            record.note = "Changed after replay".into();
        }
        let error = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                let changed = changed.clone();
                Box::pin(async move {
                    let mut repository = PgBillingCostCommand::new(connection);
                    execute_cost_command(&actor(), &target(), &changed, &mut repository, NOW).await
                })
            })
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "KEY_CONFLICT");

        let complete = BillingCostCommand::Complete(CompleteCostReview {
            key: key("billing-complete-0001"),
            expected_version: 5,
            reason: "Reviewed the whole month".into(),
            period_id: RecordId::new("period-10").unwrap(),
            complete: true,
        });
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                let complete = complete.clone();
                Box::pin(async move {
                    let mut repository = PgBillingCostCommand::new(connection);
                    execute_cost_command(&actor(), &target(), &complete, &mut repository, NOW).await
                })
            })
            .await
            .unwrap();
        assert_eq!(
            count(
                &pool,
                "billing_periods",
                "id='period-10' AND costs_complete=TRUE"
            )
            .await,
            1
        );
        assert_eq!(
            count(&pool, "billing_accounts", "holding_id='h001' AND version=6").await,
            1
        );

        let delete = BillingCostCommand::Delete(DeleteCost {
            key: key("billing-delete-0001"),
            expected_version: 6,
            reason: "Remove incorrect cost".into(),
            cost_id: RecordId::new("cost-1").unwrap(),
        });
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                let delete = delete.clone();
                Box::pin(async move {
                    let mut repository = PgBillingCostCommand::new(connection);
                    execute_cost_command(&actor(), &target(), &delete, &mut repository, NOW).await
                })
            })
            .await
            .unwrap();
        assert_eq!(count(&pool, "billing_costs", "id='cost-1'").await, 0);
        assert_eq!(
            count(
                &pool,
                "billing_periods",
                "id='period-10' AND costs_complete=FALSE"
            )
            .await,
            1
        );
        assert_eq!(
            count(&pool, "billing_accounts", "holding_id='h001' AND version=7").await,
            1
        );
    });
}

#[test]
fn negative_commands_fail_before_writing_and_a_late_audit_failure_rolls_back() {
    with_database(|pool| async move {
        setup(&pool).await;
        let stale = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                let command = record("billing-stale-0001", 3);
                Box::pin(async move {
                    let mut repository = PgBillingCostCommand::new(connection);
                    execute_cost_command(&actor(), &target(), &command, &mut repository, NOW).await
                })
            })
            .await
            .unwrap_err();
        assert_eq!(business(stale).code, "VERSION_CONFLICT");
        assert_eq!(count(&pool, "billing_costs", "holding_id='h001'").await, 1);

        let mut missing_evidence = record("billing-evidence-0001", 4);
        if let BillingCostCommand::Record(record) = &mut missing_evidence {
            record.evidence_id = None;
        }
        let error = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                let command = missing_evidence.clone();
                Box::pin(async move {
                    let mut repository = PgBillingCostCommand::new(connection);
                    execute_cost_command(&actor(), &target(), &command, &mut repository, NOW).await
                })
            })
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "EVIDENCE_REQUIRED");

        let mut foreign_evidence = record("billing-foreign-evidence-0001", 4);
        if let BillingCostCommand::Record(record) = &mut foreign_evidence {
            record.evidence_id = Some(RecordId::new("evidence-foreign").unwrap());
        }
        let error = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                let command = foreign_evidence.clone();
                Box::pin(async move {
                    let mut repository = PgBillingCostCommand::new(connection);
                    execute_cost_command(&actor(), &target(), &command, &mut repository, NOW).await
                })
            })
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "INVALID_EVIDENCE");

        let rollback_key = "billing-rollback-0001";
        sqlx::query(
            "INSERT INTO billing_audit(id,holding_id,actor,action,reason,details_json,created_at) \
             VALUES($1,'h001','admin','cost','Preexisting collision','{}',700)",
        )
        .bind(format!("billing-audit-{rollback_key}"))
        .execute(pool.raw())
        .await
        .unwrap();
        let error = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                let command = record(rollback_key, 4);
                Box::pin(async move {
                    let mut repository = PgBillingCostCommand::new(connection);
                    execute_cost_command(&actor(), &target(), &command, &mut repository, NOW).await
                })
            })
            .await
            .unwrap_err();
        assert!(matches!(error, PgTransactionError::Persistence(_)));
        assert_eq!(
            count(
                &pool,
                "billing_costs",
                &format!("id='billing-cost-{rollback_key}'")
            )
            .await,
            0
        );
        assert_eq!(
            count(&pool, "billing_accounts", "holding_id='h001' AND version=4").await,
            1
        );
        assert_eq!(
            count(&pool, "billing_receipts", &format!("id='{rollback_key}'")).await,
            0
        );
        assert_eq!(
            count(
                &pool,
                "billing_periods",
                "id='period-10' AND costs_complete=TRUE"
            )
            .await,
            1
        );
    });
}

#[test]
fn transaction_and_serializable_guards_are_required_by_the_adapters() {
    with_database(|pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut repository = PgBillingWorkspace::new(&mut connection);
        let error = billing_workspace(&actor(), &target(), &mut repository, NOW)
            .await
            .unwrap_err();
        assert!(matches!(error, PgTransactionError::Persistence(_)));

        let mut transaction = connection.begin().await.unwrap();
        let mut repository = PgBillingWorkspace::new(&mut transaction);
        let error = billing_workspace(&actor(), &target(), &mut repository, NOW)
            .await
            .unwrap_err();
        assert!(matches!(error, PgTransactionError::Persistence(_)));
        transaction.rollback().await.unwrap();
        drop(connection);
    });
}
