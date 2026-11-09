//! Native Billing workspace read and cost-review commands on migrated
//! PostgreSQL. These tests prove owner SQL, Holding isolation, exact account
//! CAS, command idempotency, audit/receipt persistence, rollback and committed
//! readback. They do not prove a Next/browser journey or composition mounting.
use erpx_server_billing::public::cost::{
    BillingCostCommand, CompleteCostReview, DeleteCost, PgBillingCostCommand, RecordCost,
    execute_cost_command,
};
use erpx_server_billing::public::forecast_read::{
    billing_month, next_billing_month, read_seat_forecast_baseline, target_holding,
};
use erpx_server_billing::public::target_assignment::read_target_assignment;
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

async fn run_assign(
    pool: &DatabasePool,
    actor: &BillingActor,
    target: &HoldingId,
    command: &erpx_server_billing::public::target_command::AssignTarget,
    now: TimestampMillis,
) -> Result<erpx_server_billing::public::cost::CommandReceipt, PgTransactionError<BusinessError>> {
    let pool = pool.clone();
    let actor = actor.clone();
    let target = target.clone();
    let command = command.clone();
    PgUnitOfWork::new(pool)
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let target = target.clone();
            let command = command.clone();
            Box::pin(async move {
                erpx_server_billing::public::target_command::assign_target(
                    connection, &actor, &target, &command, now,
                )
                .await
            })
        })
        .await
}

async fn run_evidence(
    pool: &DatabasePool,
    actor: &BillingActor,
    target: &HoldingId,
    command: &erpx_server_billing::public::target_evidence_command::BillingTargetEvidenceCommand,
    now: TimestampMillis,
) -> Result<erpx_server_billing::public::cost::CommandReceipt, PgTransactionError<BusinessError>> {
    let pool = pool.clone();
    let actor = actor.clone();
    let target = target.clone();
    let command = command.clone();
    PgUnitOfWork::new(pool)
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let target = target.clone();
            let command = command.clone();
            Box::pin(async move {
                erpx_server_billing::public::target_evidence_command::execute_target_evidence(
                    connection, &actor, &target, &command, now,
                )
                .await
            })
        })
        .await
}

async fn run_provider(
    pool: &DatabasePool,
    actor: &BillingActor,
    target: &HoldingId,
    command: &erpx_server_billing::public::target_provider_command::BillingTargetProviderCommand,
    now: TimestampMillis,
) -> Result<erpx_server_billing::public::cost::CommandReceipt, PgTransactionError<BusinessError>> {
    let pool = pool.clone();
    let actor = actor.clone();
    let target = target.clone();
    let command = command.clone();
    PgUnitOfWork::new(pool)
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let target = target.clone();
            let command = command.clone();
            Box::pin(async move {
                erpx_server_billing::public::target_provider_command::execute_target_provider(
                    connection, &actor, &target, &command, now,
                )
                .await
            })
        })
        .await
}

async fn run_rate(
    pool: &DatabasePool,
    actor: &BillingActor,
    target: &HoldingId,
    command: &erpx_server_billing::public::target_rate_command::RateTarget,
    now: TimestampMillis,
) -> Result<erpx_server_billing::public::cost::CommandReceipt, PgTransactionError<BusinessError>> {
    let pool = pool.clone();
    let actor = actor.clone();
    let target = target.clone();
    let command = command.clone();
    PgUnitOfWork::new(pool)
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let target = target.clone();
            let command = command.clone();
            Box::pin(async move {
                erpx_server_billing::public::target_rate_command::execute_rate_target(
                    connection, &actor, &target, &command, now,
                )
                .await
            })
        })
        .await
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

#[test]
fn forecast_baseline_reads_control_projection_and_matches_typescript() {
    with_database(|pool| async move {
        migrate(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES ('h001','acme','Acme','write-h001','read-h001','files-h001','active');             INSERT INTO billing_target_prices(id,effective_month,base_monthly_satang,included_original_bytes,storage_satang_per_gb_month,ai_markup_percent,created_at,created_by,seat_terms_status,included_seats,seat_monthly_satang,storage_allowance_mode,rating_enabled) VALUES             ('forecast-current-2026-10','2026-10',0,20000000000,500,10,0,'fixture','defined',0,49900,'per_paid_seat_time',TRUE),             ('forecast-future-2026-12','2026-12',0,20000000000,500,10,0,'fixture','defined',0,59900,'per_paid_seat_time',TRUE);             INSERT INTO billing_usage_snapshots(holding_id,month,snapshot_id,refreshed_at,source_count,storage_coverage,active_seat_count,active_seat_observed_at) VALUES ('h001','2026-09','snap-1',1234,2,'partial',282,1200);             INSERT INTO billing_usage_meters(holding_id,month,source_key,company_id,company_name,meter_kind,funding_source,pricing_status,quantity,active_at_period_end,snapshot_id) VALUES             ('h001','2026-09',repeat('a',64),'company','Acme','storage','erpx','unpriced',2636112,TRUE,'snap-1'),             ('h001','2026-09',repeat('b',64),'company','Acme','ai','erpx','unpriced',999,TRUE,'snap-1');",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let now = 1_789_430_400_000_i64;
        let holding = target_holding("h001").unwrap();
        let mut c = pool.raw().acquire().await.unwrap();
        let baseline = read_seat_forecast_baseline(&mut c, &holding, now)
            .await
            .unwrap();
        assert_eq!(baseline.forecast_month, "2026-10");
        assert_eq!(baseline.source_month, "2026-09");
        assert_eq!(
            baseline.price.as_ref().unwrap().id,
            "forecast-current-2026-10"
        );
        assert!(baseline.price_effective_for_month);
        assert_eq!(baseline.seats.count, Some(282));
        assert_eq!(baseline.seats.observed_at, Some(1200));
        assert_eq!(baseline.storage.original_bytes, Some(2_636_112));
        assert_eq!(baseline.storage.observed_at, Some(1234));
        assert_eq!(baseline.storage.coverage, "partial");
        // Actual TypeScript application with the same projection rows.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../");
        let application = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/server-billing/src/application/forecast.ts");
        let script = r#"const {readSeatForecastBaseline}=await import(process.argv[1]);
const rows={current:{rows:[{id:'forecast-current-2026-10',effective_month:'2026-10',base_monthly_satang:'0',included_original_bytes:'20000000000',storage_satang_per_gb_month:'500',ai_markup_percent:10,seat_terms_status:'defined',included_seats:0,seat_monthly_satang:'49900',storage_allowance_mode:'per_paid_seat_time',rating_enabled:true}],rowCount:1},future:{rows:[],rowCount:0},snapshot:{rows:[{snapshot_id:'snap-1',refreshed_at:'1234',source_count:2,storage_coverage:'partial',active_seat_count:282,active_seat_observed_at:'1200'}],rowCount:1},totals:{rows:[{row_count:'2',active_original_bytes:'2636112'}],rowCount:1}};
const repository={currentPrice:async()=>rows.current,futurePrice:async()=>rows.future,usageSnapshot:async()=>rows.snapshot,retainedStorageTotals:async()=>rows.totals};
process.stdout.write(JSON.stringify(await readSeatForecastBaseline(repository,'h001',Number(process.argv[2]))));"#;
        let output =
            std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
                .current_dir(root)
                .args(["--conditions=react-server", "-e", script])
                .arg(application)
                .arg(now.to_string())
                .output()
                .unwrap();
        assert!(
            output.status.success(),
            "actual TS forecast oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let ts: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(baseline.to_wire().unwrap(), ts);
        // A source-count mismatch makes storage unavailable but keeps seats.
        sqlx::query("UPDATE billing_usage_snapshots SET source_count=3 WHERE holding_id='h001' AND month='2026-09'").execute(pool.raw()).await.unwrap();
        let mismatch = read_seat_forecast_baseline(&mut c, &holding, now)
            .await
            .unwrap();
        assert_eq!(mismatch.seats.count, Some(282));
        assert_eq!(mismatch.storage.original_bytes, None);
        assert_eq!(mismatch.storage.coverage, "unavailable");
    });
}

#[test]
fn forecast_months_match_javascript_bangkok_boundaries() {
    assert_eq!(billing_month(1_789_430_400_000).unwrap(), "2026-09");
    assert_eq!(next_billing_month(1_789_430_400_000).unwrap(), "2026-10");
    assert_eq!(billing_month(1_798_675_200_000).unwrap(), "2026-12");
    assert_eq!(next_billing_month(1_798_675_200_000).unwrap(), "2027-01");
    assert_eq!(billing_month(1_798_783_200_000).unwrap(), "2027-01");
    assert_eq!(target_holding("h003").unwrap_err().code, "HOLDING_REQUIRED");
    assert_eq!(target_holding("h001").unwrap().as_str(), "h001");
}

#[test]
fn forecast_future_price_null_seats_and_invalid_source_fail_closed() {
    with_database(|pool| async move {
        migrate(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES ('h001','acme','Acme','write-h001','read-h001','files-h001','active');\
             INSERT INTO billing_usage_snapshots(holding_id,month,snapshot_id,refreshed_at,source_count,storage_coverage,active_seat_count,active_seat_observed_at) VALUES ('h001','2026-01','snap-null',1234,0,'partial',NULL,NULL);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let holding = target_holding("h001").unwrap();
        let now = 1_768_435_200_000_i64;
        let mut c = pool.raw().acquire().await.unwrap();
        let baseline = read_seat_forecast_baseline(&mut c, &holding, now)
            .await
            .unwrap();
        assert_eq!(baseline.forecast_month, "2026-02");
        assert_eq!(
            baseline.price.as_ref().unwrap().id,
            "holding-usage-2026-12-v3"
        );
        assert!(!baseline.price_effective_for_month);
        assert_eq!(baseline.seats.count, None);
        assert_eq!(baseline.seats.observed_at, None);
        assert_eq!(baseline.storage.original_bytes, Some(0));
        sqlx::query(
            "INSERT INTO billing_target_prices(id,effective_month,base_monthly_satang,included_original_bytes,storage_satang_per_gb_month,ai_markup_percent,created_at,created_by,seat_terms_status,included_seats,seat_monthly_satang,storage_allowance_mode,rating_enabled) VALUES ('forecast-aaa-unsafe','2026-03',0,9007199254740993,0,10,0,'fixture','defined',0,49900,'per_paid_seat_time',TRUE)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let error = read_seat_forecast_baseline(&mut c, &holding, now)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "BILLING_SOURCE_INVALID");
    });
}

#[test]
fn target_assignment_and_price_read_matches_actual_typescript_mapping() {
    with_database(|pool| async move {
        migrate(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES ('h001','acme','Acme','write-h001','read-h001','files-h001','active');\
             INSERT INTO billing_target_prices(id,effective_month,base_monthly_satang,included_original_bytes,storage_satang_per_gb_month,ai_markup_percent,created_at,created_by,seat_terms_status,included_seats,seat_monthly_satang,storage_allowance_mode,rating_enabled) VALUES ('target-assign-price','2026-10',0,20000000000,500,10,0,'fixture','defined',0,49900,'per_paid_seat_time',TRUE);\
             INSERT INTO billing_target_assignments(holding_id,effective_month,price_id,mode,service_started_at,assigned_by,reason,created_at) VALUES ('h001','2026-10','target-assign-price','sandbox_invoice',1790812800000,'fixture','fixture',0);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let holding = target_holding("h001").unwrap();
        let mut c = pool.raw().acquire().await.unwrap();
        let read = read_target_assignment(&mut c, &holding, "2026-10")
            .await
            .unwrap();
        let assignment = read.assignment.as_ref().unwrap();
        assert_eq!(assignment.effective_month, "2026-10");
        assert_eq!(assignment.mode, "sandbox_invoice");
        assert_eq!(assignment.price_id, "target-assign-price");
        assert_eq!(assignment.service_started_at, 1_790_812_800_000);
        let price = read.price.as_ref().unwrap();
        assert_eq!(price.id, "target-assign-price");
        assert_eq!(price.included_seats, Some(0));
        assert_eq!(price.seat_monthly_satang, Some(49_900));
        assert_eq!(price.storage_allowance_mode, "per_paid_seat_time");
        // Actual TypeScript target application mapping with the same rows.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../");
        let application = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/server-billing/src/application/target.ts");
        let script = r#"const {calculateTargetBillingPeriod}=await import(process.argv[1]);
const assignment={holding_id:'h001',effective_month:'2026-10',price_id:'target-assign-price',mode:'sandbox_invoice',service_started_at:'1790812800000'};
const price={id:'target-assign-price',effective_month:'2026-10',base_monthly_satang:'0',included_original_bytes:'20000000000',storage_satang_per_gb_month:'500',ai_markup_percent:10,seat_terms_status:'defined',included_seats:0,seat_monthly_satang:'49900',storage_allowance_mode:'per_paid_seat_time',rating_enabled:true};
const empty={rows:[],rowCount:0};
const repo=new Proxy({assignment:async()=>({rows:[assignment],rowCount:1}),assignedPrice:async()=>({rows:[price],rowCount:1})},{get:(target,key)=>key in target?target[key]:async()=>empty});
const result=await calculateTargetBillingPeriod(repo,'h001','2026-10',1792022400000);
process.stdout.write(JSON.stringify({assignment:result.assignment,price:result.price,state:result.state,missing:result.completeness.missing}));"#;
        let output =
            std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
                .current_dir(root)
                .args(["--conditions=react-server", "-e", script])
                .arg(application)
                .output()
                .unwrap();
        assert!(
            output.status.success(),
            "actual TS target mapping oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let ts: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            ts["assignment"]["effectiveMonth"],
            assignment.effective_month
        );
        assert_eq!(ts["assignment"]["mode"], assignment.mode);
        assert_eq!(ts["assignment"]["priceId"], assignment.price_id);
        assert_eq!(
            ts["assignment"]["serviceStartedAt"],
            assignment.service_started_at
        );
        assert_eq!(ts["price"]["id"], price.id);
        assert_eq!(ts["price"]["includedSeats"], 0);
        assert_eq!(ts["price"]["seatMonthlySatang"], 49_900);
        assert_eq!(ts["price"]["storageAllowanceMode"], "per_paid_seat_time");
        assert_eq!(ts["state"], "held");
        assert!(
            ts["missing"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == "meter_snapshot_missing")
        );
    });
}

#[test]
fn target_rollout_readiness_reads_control_evidence_and_isolates_holdings() {
    with_database(|pool| async move {
        migrate(&pool).await;
        sqlx::raw_sql(
            r#"INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES
                 ('h001','acme','Acme','write-h001','read-h001','files-h001','active'),
                 ('h002','northstar','Northstar','write-h002','read-h002','files-h002','active');
               INSERT INTO billing_accounts(holding_id,version,customer_id,payment_mode,created_at) VALUES
                 ('h001',1,'customer-1','manual',100),
                 ('h002',1,'customer-2','manual',100);
               INSERT INTO billing_target_prices
                 (id,effective_month,base_monthly_satang,included_original_bytes,storage_satang_per_gb_month,ai_markup_percent,created_at,created_by,seat_terms_status,included_seats,seat_monthly_satang,storage_allowance_mode,rating_enabled) VALUES
                 ('price-sep','2026-09',0,10000000000,500,10,0,'fixture','defined',0,49900,'per_paid_seat_time',TRUE),
                 ('price-oct','2026-10',0,20000000000,500,10,0,'fixture','defined',0,49900,'per_paid_seat_time',TRUE);
               INSERT INTO billing_target_assignments
                 (holding_id,effective_month,price_id,mode,service_started_at,assigned_by,reason,created_at) VALUES
                 ('h001','2026-10','price-oct','shadow',1788195600000,'fixture','fixture',0),
                 ('h002','2026-10','price-oct','shadow',1788195600000,'fixture','fixture',0);
               INSERT INTO billing_usage_snapshots
                 (holding_id,month,snapshot_id,refreshed_at,source_count,ai_coverage,storage_coverage,unreconciled_originals) VALUES
                 ('h001','2026-08','h1-aug',1234,1,'measured','measured',0),
                 ('h001','2026-09','h1-sep',1234,1,'measured','measured',0),
                 ('h002','2026-08','h2-aug',1234,1,'measured','measured',0),
                 ('h002','2026-09','h2-sep',1234,1,'measured','measured',0);
               INSERT INTO billing_evidence
                 (id,holding_id,filename,mime,size,sha256,actor,created_at,object_key,detected_mime,source_state,scanned_at,scanner_definition_at) VALUES
                 ('evidence-h1','h001','cost.pdf','application/pdf',10,repeat('a',64),'fixture',100,'h001/evidence-h1','application/pdf','available',100,100),
                 ('evidence-h2','h002','cost.pdf','application/pdf',10,repeat('b',64),'fixture',100,'h002/evidence-h2','application/pdf','available',100,100);
               INSERT INTO billing_target_rated_periods
                 (holding_id,month,price_id,assignment_month,mode,meter_snapshot_id,service_start,service_end,state,missing_json,lines_json,total_satang,rated_at) VALUES
                 ('h001','2026-08','price-oct','2026-10','shadow','h1-aug',1788195600000,1790787600000,'ready','[]',
                  $j1$[{"kind":"ai","sourceKey":"ai-1","fundingSource":"erpx","amountSatang":22000,"verifiedProviderCostSatang":20000},{"kind":"ai","sourceKey":"ai-2","fundingSource":"erpx","amountSatang":0,"verifiedProviderCostSatang":null}]$j1$,1000000,100),
                 ('h001','2026-09','price-oct','2026-10','shadow','h1-sep',1788195600000,1790787600000,'ready','[]',
                  $j2$[{"kind":"ai","sourceKey":"ai-1","fundingSource":"erpx","amountSatang":22000,"verifiedProviderCostSatang":20000}]$j2$,1000000,100),
                 ('h002','2026-08','price-oct','2026-10','shadow','h2-aug',1788195600000,1790787600000,'ready','[]',
                  $j3$[{"kind":"ai","sourceKey":"ai-2","fundingSource":"erpx","amountSatang":999,"verifiedProviderCostSatang":777}]$j3$,9999999,100),
                 ('h002','2026-09','price-oct','2026-10','shadow','h2-sep',1788195600000,1790787600000,'ready','[]',
                  $j4$[{"kind":"ai","sourceKey":"ai-2","fundingSource":"erpx","amountSatang":999,"verifiedProviderCostSatang":777}]$j4$,9999999,100);
               INSERT INTO billing_target_operating_costs
                 (holding_id,month,category,amount_satang,evidence_id,verified_by,verified_at) VALUES
                 ('h001','2026-08','infrastructure',25000,'evidence-h1','fixture',100),
                 ('h001','2026-08','backup',25000,'evidence-h1','fixture',100),
                 ('h001','2026-08','stripe',25000,'evidence-h1','fixture',100),
                 ('h001','2026-08','support',25000,'evidence-h1','fixture',100),
                 ('h001','2026-09','infrastructure',25000,'evidence-h1','fixture',100),
                 ('h001','2026-09','backup',25000,'evidence-h1','fixture',100),
                 ('h001','2026-09','stripe',25000,'evidence-h1','fixture',100),
                 ('h001','2026-09','support',25000,'evidence-h1','fixture',100),
                 ('h002','2026-08','infrastructure',1000000,'evidence-h2','fixture',100),
                 ('h002','2026-08','backup',1000000,'evidence-h2','fixture',100),
                 ('h002','2026-08','stripe',1000000,'evidence-h2','fixture',100),
                 ('h002','2026-08','support',1000000,'evidence-h2','fixture',100),
                 ('h002','2026-09','infrastructure',1000000,'evidence-h2','fixture',100),
                 ('h002','2026-09','backup',1000000,'evidence-h2','fixture',100),
                 ('h002','2026-09','stripe',1000000,'evidence-h2','fixture',100),
                 ('h002','2026-09','support',1000000,'evidence-h2','fixture',100);
               INSERT INTO billing_target_capacity_reviews
                 (holding_id,effective_month,projected_original_bytes,primary_available_bytes,backup_available_bytes,reserved_bytes,company_limit_bytes,evidence_id,approved_by,reason,approved_at) VALUES
                 ('h001','2026-10',10000000000,30000000000,30000000000,1000000000,25000000000,'evidence-h1','fixture','fixture',100),
                 ('h002','2026-10',99000000000,200000000000,200000000000,1000000000,200000000000,'evidence-h2','fixture','fixture',100);"#,
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let now = TimestampMillis::new(1_790_787_600_000);
        let mut c = pool.raw().acquire().await.unwrap();
        let h001 = HoldingId::new("h001").unwrap();
        let read = erpx_server_billing::public::target_rollout::read_target_rollout_readiness(
            &mut c,
            &h001,
            "2026-10",
            8_000,
            now,
            20_000_000_000,
        )
        .await
        .unwrap();
        assert!(read.ready, "{:?}", read.blockers);
        assert!(read.blockers.is_empty());
        assert_eq!(
            read.shadow_months,
            ["2026-08".to_owned(), "2026-09".to_owned()]
        );
        assert_eq!(read.margins.len(), 2);
        assert_eq!(read.margins[0].rated_revenue_satang, Some(1_000_000));
        assert_eq!(
            read.margins[0].verified_provider_ai_cost_satang,
            Some(20_000)
        );
        assert_eq!(
            read.margins[0].verified_operating_cost_satang,
            Some(100_000)
        );
        assert_eq!(read.margins[0].gross_margin_bps, Some(8_800));
        assert!(read.margins[0].missing_costs.is_empty());
        assert!(read.capacity.reviewed);
        assert_eq!(read.capacity.projected_original_bytes, Some(10_000_000_000));
        assert_eq!(read.capacity.company_limit_bytes, Some(25_000_000_000));

        let h002 = HoldingId::new("h002").unwrap();
        let foreign = erpx_server_billing::public::target_rollout::read_target_rollout_readiness(
            &mut c,
            &h002,
            "2026-10",
            8_000,
            now,
            20_000_000_000,
        )
        .await
        .unwrap();
        assert!(!foreign.ready);
        assert_eq!(
            foreign.blockers,
            [
                "margin_below_threshold:2026-08",
                "margin_below_threshold:2026-09"
            ]
        );
        assert_eq!(foreign.margins[0].rated_revenue_satang, Some(9_999_999));
        assert_eq!(
            foreign.margins[0].verified_provider_ai_cost_satang,
            Some(777)
        );
        assert_eq!(
            foreign.margins[0].verified_operating_cost_satang,
            Some(4_000_000)
        );
        assert_eq!(foreign.margins[0].gross_margin_bps, Some(5_999));
        assert_eq!(
            foreign.capacity.projected_original_bytes,
            Some(99_000_000_000)
        );
    });
}

#[test]
fn target_billing_period_reads_control_evidence_and_isolates_holdings() {
    with_database(|pool| async move {
        migrate(&pool).await;
        sqlx::raw_sql(
            r#"INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES
                 ('h001','acme','Acme','write-h001','read-h001','files-h001','active'),
                 ('h002','northstar','Northstar','write-h002','read-h002','files-h002','active');
               INSERT INTO billing_accounts(holding_id,version,customer_id,payment_mode,created_at) VALUES
                 ('h001',1,'customer-1','manual',100),
                 ('h002',1,'customer-2','manual',100);
               INSERT INTO billing_target_prices
                 (id,effective_month,base_monthly_satang,included_original_bytes,storage_satang_per_gb_month,ai_markup_percent,created_at,created_by,seat_terms_status,included_seats,seat_monthly_satang,storage_allowance_mode,rating_enabled) VALUES
                 ('price-1','2026-10',149000,20000000000,500,10,0,'fixture','defined',0,49900,'per_paid_seat_time',TRUE);
               INSERT INTO billing_target_assignments
                 (holding_id,effective_month,price_id,mode,service_started_at,assigned_by,reason,created_at) VALUES
                 ('h001','2026-10','price-1','sandbox_invoice',1790787600000,'fixture','fixture',0),
                 ('h002','2026-10','price-1','sandbox_invoice',1790787600000,'fixture','fixture',0);
               INSERT INTO billing_target_sandbox_holdings(holding_id,designated_by,reason,designated_at) VALUES
                 ('h001','fixture','fixture',0);
               INSERT INTO billing_usage_snapshots
                 (holding_id,month,snapshot_id,refreshed_at,source_count,ai_coverage,storage_coverage,unreconciled_originals,seat_coverage,seat_source_count,active_seat_count,active_seat_observed_at) VALUES
                 ('h001','2026-10','snap-1',1790000000000,3,'measured','measured',0,'measured',2,3,1790000000000),
                 ('h001','2026-11','snap-now',1793595600000,0,'measured','measured',0,'measured',0,3,1793595600000),
                 ('h002','2026-10','snap-h2',1790000000000,0,'measured','measured',0,'measured',0,NULL,NULL);
               INSERT INTO billing_evidence
                 (id,holding_id,filename,mime,size,sha256,actor,created_at,object_key,detected_mime,source_state,scanned_at,scanner_definition_at) VALUES
                 ('evidence-h1','h001','cost.pdf','application/pdf',10,repeat('e',64),'fixture',100,'h001/evidence-h1','application/pdf','available',100,100),
                 ('evidence-h2','h002','cost.pdf','application/pdf',10,repeat('f',64),'fixture',100,'h002/evidence-h2','application/pdf','available',100,100);
               INSERT INTO billing_usage_meters
                 (holding_id,month,source_key,company_id,company_name,branch_id,branch_name,meter_kind,funding_source,pricing_status,quantity,input_tokens,output_tokens,average_daily_bytes,active_at_period_end,snapshot_id,request_status,gateway_status,provider,model,units_json,cost_status,verified_cost_satang,cost_evidence_key,cost_reconciled_at,active_from,active_until,byte_milliseconds) VALUES
                 ('h001','2026-10',repeat('a',64),'company-1','Acme',NULL,NULL,'ai','erpx','unpriced',1,100,50,0,FALSE,'snap-1','succeeded','succeeded','openai','gpt','{"input":100}','reconciled',1000,NULL,100,NULL,NULL,NULL),
                 ('h001','2026-10',repeat('b',64),'company-1','Acme',NULL,NULL,'ai','byok','unpriced',1,80,20,0,FALSE,'snap-1','succeeded','succeeded',NULL,NULL,NULL,'not_applicable',NULL,NULL,NULL,NULL,NULL,NULL),
                 ('h001','2026-10',repeat('c',64),'company-1','Acme',NULL,NULL,'storage','erpx','unpriced',20000000000,0,0,0,TRUE,'snap-1',NULL,NULL,NULL,NULL,NULL,'not_applicable',NULL,NULL,NULL,1790787600000,1793466000000,864000000000000);
               INSERT INTO billing_seat_usage(holding_id,month,user_key,intervals_json,snapshot_id) VALUES
                 ('h001','2026-10',repeat('1',64),'[[1790787600000,1791651600000]]','snap-1'),
                 ('h001','2026-10',repeat('2',64),'[[1791224400000,1792088400000]]','snap-1');
               INSERT INTO billing_target_operating_costs
                 (holding_id,month,category,amount_satang,evidence_id,verified_by,verified_at) VALUES
                 ('h001','2026-10','infrastructure',10000,'evidence-h1','fixture',100),
                 ('h001','2026-10','backup',10000,'evidence-h1','fixture',100),
                 ('h001','2026-10','stripe',10000,'evidence-h1','fixture',100),
                 ('h001','2026-10','support',10000,'evidence-h1','fixture',100),
                 ('h002','2026-10','infrastructure',20000,'evidence-h2','fixture',100),
                 ('h002','2026-10','backup',20000,'evidence-h2','fixture',100),
                 ('h002','2026-10','stripe',20000,'evidence-h2','fixture',100),
                 ('h002','2026-10','support',20000,'evidence-h2','fixture',100);
               INSERT INTO billing_target_provider_costs
                 (holding_id,month,source_key,provider,statement_id,original_currency,original_minor,fx_numerator,fx_denominator,provider_cost_satang,evidence_id,verified_by,verified_at) VALUES
                 ('h001','2026-10',repeat('a',64),'openai','statement-1','USD',1000,1,1,1000,'evidence-h1','fixture',100);
               INSERT INTO billing_target_statements
                 (holding_id,month,provider,statement_id,evidence_id,provider_total_minor,original_currency,reconciled_total_minor,verified_by,verified_at) VALUES
                 ('h001','2026-10','openai','statement-1','evidence-h1',1000,'USD',1000,'fixture',100);
               INSERT INTO billing_target_extras
                 (id,holding_id,month,kind,description,amount_satang,service_start,service_end,evidence_id,approved_by,approval_reason,approved_at) VALUES
                 ('extra-1','h001','2026-10','setup','Onboarding',5000,1790787600000,1793466000000,'evidence-h1','fixture','fixture',100);"#,
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let now = TimestampMillis::new(1_793_595_600_000);
        let mut c = pool.raw().acquire().await.unwrap();
        let h001 = HoldingId::new("h001").unwrap();
        let read = erpx_server_billing::public::target::read_target_billing_period(
            &mut c, &h001, "2026-10", now,
        )
        .await
        .unwrap();
        assert_eq!(read.state, "ready", "{:?}", read.completeness.missing);
        assert!(read.completeness.ready);
        assert!(read.completeness.missing.is_empty());
        assert!(read.approval.eligible, "{:?}", read.approval.blockers);
        assert!(read.approval.blockers.is_empty());
        let assignment = read.assignment.as_ref().unwrap();
        assert_eq!(assignment.price_id, "price-1");
        assert_eq!(assignment.mode, "sandbox_invoice");
        assert_eq!(
            read.price.as_ref().unwrap().included_original_bytes,
            20_000_000_000
        );
        let measured = &read.measured_evidence;
        assert_eq!(measured.snapshot.as_ref().unwrap().id, "snap-1");
        assert_eq!(measured.ai.requests, Some(2));
        assert_eq!(measured.ai.platform_succeeded, 1);
        assert_eq!(measured.ai.platform_verified, 1);
        assert_eq!(measured.ai.byok, 1);
        assert_eq!(measured.ai.verified_provider_cost_satang, Some(1000));
        assert_eq!(measured.ai.total_provider_cost_satang, Some(1000));
        assert_eq!(measured.storage.original_count, Some(1));
        assert_eq!(
            measured.storage.byte_milliseconds.as_deref(),
            Some("864000000000000")
        );
        assert_eq!(measured.seats.distinct_users, Some(2));
        assert_eq!(measured.seats.coverage, "measured");
        assert_eq!(measured.seats.active_now.status, "measured");
        assert_eq!(measured.seats.active_now.count, Some(3));
        assert_eq!(measured.operating_costs.total_verified_satang, Some(40000));
        let kinds = read
            .lines
            .iter()
            .map(|line| line.kind.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                "base", "storage", "seat", "seat", "seat", "storage", "ai", "ai", "extra"
            ]
        );
        let ai_lines = read
            .lines
            .iter()
            .filter(|line| line.kind == "ai")
            .collect::<Vec<_>>();
        assert_eq!(ai_lines[0].amount_satang, Some(1100));
        assert_eq!(ai_lines[1].amount_satang, Some(0));
        assert_eq!(ai_lines[0].reconciliation_status, "verified");
        assert_eq!(ai_lines[1].reconciliation_status, "not_required");
        assert_eq!(read.lines.last().unwrap().amount_satang, Some(5000));
        assert_eq!(read.statement_reconciliation.len(), 1);
        assert_eq!(read.statement_reconciliation[0].status, "verified");
        assert_eq!(read.sources.len(), 6);
        assert_eq!(
            read.total_satang,
            Some(
                read.lines
                    .iter()
                    .map(|line| line.amount_satang.unwrap_or(0))
                    .sum::<i64>()
            )
        );
        assert!(read.total_satang.unwrap() > 149_000);

        let h002 = HoldingId::new("h002").unwrap();
        let foreign = erpx_server_billing::public::target::read_target_billing_period(
            &mut c, &h002, "2026-10", now,
        )
        .await
        .unwrap();
        assert_eq!(foreign.state, "ready");
        assert!(foreign.completeness.missing.is_empty());
        assert_eq!(foreign.measured_evidence.ai.requests, Some(0));
        assert_eq!(
            foreign.measured_evidence.ai.total_provider_cost_satang,
            Some(0)
        );
        assert_eq!(foreign.measured_evidence.storage.original_count, Some(0));
        assert_eq!(foreign.measured_evidence.seats.distinct_users, Some(0));
        assert_eq!(
            foreign
                .measured_evidence
                .operating_costs
                .total_verified_satang,
            Some(80000)
        );
        assert_eq!(
            foreign.approval.blockers,
            ["sandbox_designation_or_rollout_approval_missing"]
        );
        assert!(!foreign.approval.eligible);
        assert!(foreign.statement_reconciliation.is_empty());
        assert_eq!(
            foreign
                .lines
                .iter()
                .filter(|line| line.kind == "ai")
                .count(),
            0
        );
        assert_eq!(foreign.sources.len(), 0);
    });
}

#[test]
fn target_assignment_command_is_immutable_replayable_and_owner_scoped() {
    with_database(|pool| async move {
        migrate(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES \
             ('h001','acme','Acme','write-h001','read-h001','files-h001','active'),\
             ('h002','northstar','Northstar','write-h002','read-h002','files-h002','active');\
             INSERT INTO billing_accounts(holding_id,version,customer_id,payment_mode,created_at) VALUES \
             ('h001',3,'customer-1','manual',100),\
             ('h002',1,'customer-2','manual',100);\
             INSERT INTO billing_target_prices\
             (id,effective_month,base_monthly_satang,included_original_bytes,storage_satang_per_gb_month,ai_markup_percent,created_at,created_by,seat_terms_status,included_seats,seat_monthly_satang,storage_allowance_mode,rating_enabled) VALUES \
             ('price-current','2026-12',0,20000000000,500,10,0,'fixture','defined',0,49900,'per_paid_seat_time',TRUE),\
             ('price-sandbox','2027-02',0,20000000000,500,10,0,'fixture','defined',0,49900,'per_paid_seat_time',TRUE),\
             ('price-future','2027-03',0,20000000000,500,10,0,'fixture','defined',0,49900,'per_paid_seat_time',TRUE);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        use erpx_server_billing::public::target_command::AssignTarget;
        let now = TimestampMillis::new(1_799_989_200_000);
        let owner = |holding: &str| BillingActor {
            user_id: UserId::new("owner").unwrap(),
            assignment_id: AssignmentId::new("assignment").unwrap(),
            holding_id: Some(HoldingId::new(holding).unwrap()),
            platform: false,
            role: "holding_owner".into(),
            expires_at: None,
        };
        let command =
            |command_key: &str, version: i64, month: &str, mode: &str, price: Option<&str>| {
                AssignTarget {
                    key: key(command_key),
                    expected_version: version,
                    reason: "Reviewed target assignment".into(),
                    target_month: month.into(),
                    target_mode: mode.into(),
                    price_id: price.map(|id| RecordId::new(id).unwrap()),
                }
            };
        let h001 = HoldingId::new("h001").unwrap();
        let first = run_assign(
            &pool,
            &owner("h001"),
            &h001,
            &command("target:assign:shadow-1", 3, "2026-12", "shadow", None),
            now,
        )
        .await
        .unwrap();
        assert_eq!(first.result_json, "{\"ok\":true}");
        let assignment: (String, String, i64) = sqlx::query_as(
            "SELECT price_id,mode,service_started_at FROM billing_target_assignments \
             WHERE holding_id='h001' AND effective_month='2026-12'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            assignment,
            ("price-current".into(), "shadow".into(), 1_796_058_000_000)
        );
        let version: i32 =
            sqlx::query_scalar("SELECT version FROM billing_accounts WHERE holding_id='h001'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(version, 4);
        let replay = run_assign(
            &pool,
            &owner("h001"),
            &h001,
            &command("target:assign:shadow-1", 3, "2026-12", "shadow", None),
            now,
        )
        .await
        .unwrap();
        assert_eq!(replay, first);
        let conflict = run_assign(
            &pool,
            &owner("h001"),
            &h001,
            &command("target:assign:shadow-1", 3, "2026-12", "shadow", None),
            now,
        )
        .await;
        assert!(
            conflict.is_ok(),
            "replay with the same identity stays stable"
        );
        let changed = run_assign(
            &pool,
            &owner("h001"),
            &h001,
            &AssignTarget {
                reason: "A different reviewed request".into(),
                ..command("target:assign:shadow-1", 3, "2026-12", "shadow", None)
            },
            now,
        )
        .await
        .err()
        .expect("a different identity for the same key conflicts");
        assert_eq!(business(changed).code, "KEY_CONFLICT");

        let duplicate = run_assign(
            &pool,
            &owner("h001"),
            &h001,
            &command("target:assign:shadow-2", 4, "2026-12", "shadow", None),
            now,
        )
        .await
        .err()
        .expect("the month assignment is immutable");
        assert_eq!(business(duplicate).code, "TARGET_ASSIGNMENT_EXISTS");
        let backdate = run_assign(
            &pool,
            &owner("h001"),
            &h001,
            &command(
                "target:assign:backdate",
                4,
                "2026-12",
                "sandbox_invoice",
                None,
            ),
            now,
        )
        .await
        .err()
        .expect("sandbox invoice assignments cannot be backdated");
        assert_eq!(business(backdate).code, "TARGET_BACKDATE_FORBIDDEN");
        let not_ready = run_assign(
            &pool,
            &owner("h001"),
            &h001,
            &command(
                "target:assign:sandbox-1",
                4,
                "2027-02",
                "sandbox_invoice",
                None,
            ),
            now,
        )
        .await
        .err()
        .expect("sandbox assignment needs designation or rollout evidence");
        assert_eq!(business(not_ready).code, "TARGET_ROLLOUT_NOT_READY");
        sqlx::query(
            "INSERT INTO billing_target_sandbox_holdings(holding_id,designated_by,reason,designated_at) \
             VALUES ('h001','fixture','fixture',1)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        run_assign(
            &pool,
            &owner("h001"),
            &h001,
            &command(
                "target:assign:sandbox-2",
                4,
                "2027-02",
                "sandbox_invoice",
                None,
            ),
            now,
        )
        .await
        .unwrap();
        let sandbox: (String, String, i64) = sqlx::query_as(
            "SELECT price_id,mode,service_started_at FROM billing_target_assignments \
             WHERE holding_id='h001' AND effective_month='2027-02'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            sandbox,
            (
                "price-sandbox".into(),
                "sandbox_invoice".into(),
                1_801_414_800_000
            )
        );

        let h002 = HoldingId::new("h002").unwrap();
        let h002_not_ready = run_assign(
            &pool,
            &owner("h002"),
            &h002,
            &command(
                "target:assign:h2-sandbox",
                1,
                "2027-03",
                "sandbox_invoice",
                Some("price-future"),
            ),
            now,
        )
        .await
        .err()
        .expect("h002 has no designation or rollout yet");
        assert_eq!(business(h002_not_ready).code, "TARGET_ROLLOUT_NOT_READY");
        sqlx::query(
            "INSERT INTO billing_target_rollout_approvals\
             (holding_id,effective_month,shadow_month_1,shadow_month_2,minimum_margin_bps,approved_by,reason,approved_at) \
             VALUES ('h002','2027-02','2026-12','2027-01',8000,'fixture','fixture',1)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        run_assign(
            &pool,
            &owner("h002"),
            &h002,
            &command(
                "target:assign:h2-sandbox",
                1,
                "2027-03",
                "sandbox_invoice",
                Some("price-future"),
            ),
            now,
        )
        .await
        .unwrap();
        let h002_version: i32 =
            sqlx::query_scalar("SELECT version FROM billing_accounts WHERE holding_id='h002'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(h002_version, 2);
        let cross_holding = run_assign(
            &pool,
            &owner("h001"),
            &h002,
            &command(
                "target:assign:cross",
                2,
                "2027-03",
                "shadow",
                Some("price-future"),
            ),
            now,
        )
        .await
        .err()
        .expect("a Holding owner cannot assign another Holding");
        assert_eq!(business(cross_holding).code, "FORBIDDEN");
        let platform = BillingActor {
            platform: true,
            role: "platform_admin".into(),
            holding_id: None,
            ..owner("h001")
        };
        let platform_denied = run_assign(
            &pool,
            &platform,
            &h001,
            &command(
                "target:assign:platform",
                6,
                "2027-03",
                "shadow",
                Some("price-future"),
            ),
            now,
        )
        .await
        .err()
        .expect("target assignment is a Holding-owner command");
        assert_eq!(business(platform_denied).code, "FORBIDDEN");
        let assignments: i64 =
            sqlx::query_scalar("SELECT count(*) FROM billing_target_assignments")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(assignments, 3);
        let audit: i64 = sqlx::query_scalar("SELECT count(*) FROM billing_audit")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(audit, 3);
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM billing_receipts")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(receipts, 3);
    });
}

#[test]
fn target_evidence_commands_are_immutable_month_gated_and_owner_scoped() {
    with_database(|pool| async move {
        migrate(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES \
             ('h001','acme','Acme','write-h001','read-h001','files-h001','active'),\
             ('h002','northstar','Northstar','write-h002','read-h002','files-h002','active');\
             INSERT INTO billing_accounts(holding_id,version,customer_id,payment_mode,created_at) VALUES \
             ('h001',2,'customer-1','manual',100),\
             ('h002',1,'customer-2','manual',100);\
             INSERT INTO billing_evidence\
             (id,holding_id,filename,mime,size,sha256,actor,created_at,object_key,detected_mime,source_state,scanned_at,scanner_definition_at) VALUES \
             ('evidence-h1','h001','cost.pdf','application/pdf',10,repeat('a',64),'fixture',100,'h001/evidence-h1','application/pdf','available',100,100),\
             ('evidence-h2','h002','cost.pdf','application/pdf',10,repeat('b',64),'fixture',100,'h002/evidence-h2','application/pdf','available',100,100),\
             ('evidence-bad','h001','bad.pdf','application/pdf',10,repeat('c',64),'fixture',100,'h001/evidence-bad','application/pdf','quarantined',NULL,NULL);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        use erpx_server_billing::public::target_evidence_command::{
            BillingTargetEvidenceCommand, RecordOperatingCost, ReviewTargetCapacity,
        };
        let now = TimestampMillis::new(1_802_667_600_000);
        let owner = |holding: &str| BillingActor {
            user_id: UserId::new("owner").unwrap(),
            assignment_id: AssignmentId::new("assignment").unwrap(),
            holding_id: Some(HoldingId::new(holding).unwrap()),
            platform: false,
            role: "holding_owner".into(),
            expires_at: None,
        };
        let operating = |key_value: &str, version: i64, month: &str, evidence: &str| {
            BillingTargetEvidenceCommand::OperatingCost(RecordOperatingCost {
                key: key(key_value),
                expected_version: version,
                reason: "Reviewed actual operating cost".into(),
                target_month: month.into(),
                category: "infrastructure".into(),
                amount_satang: 12_500,
                evidence_id: RecordId::new(evidence).unwrap(),
            })
        };
        let capacity = |key_value: &str, version: i64, month: &str| {
            BillingTargetEvidenceCommand::Capacity(ReviewTargetCapacity {
                key: key(key_value),
                expected_version: version,
                reason: "Reviewed primary and backup capacity".into(),
                target_month: month.into(),
                projected_original_bytes: 10_000_000_000,
                primary_available_bytes: 30_000_000_000,
                backup_available_bytes: 30_000_000_000,
                reserved_bytes: 1_000_000_000,
                company_limit_bytes: 25_000_000_000,
                evidence_id: RecordId::new("evidence-h1").unwrap(),
            })
        };
        let h001 = HoldingId::new("h001").unwrap();
        let first = run_evidence(
            &pool,
            &owner("h001"),
            &h001,
            &operating("target:operating:1", 2, "2027-01", "evidence-h1"),
            now,
        )
        .await
        .unwrap();
        assert_eq!(first.result_json, "{\"ok\":true}");
        let row: (String, i64, String) = sqlx::query_as(
            "SELECT category,amount_satang,verified_by FROM billing_target_operating_costs \
             WHERE holding_id='h001' AND month='2027-01'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row, ("infrastructure".into(), 12_500, "owner".into()));
        let version: i32 =
            sqlx::query_scalar("SELECT version FROM billing_accounts WHERE holding_id='h001'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(version, 3);
        let replay = run_evidence(
            &pool,
            &owner("h001"),
            &h001,
            &operating("target:operating:1", 2, "2027-01", "evidence-h1"),
            now,
        )
        .await
        .unwrap();
        assert_eq!(replay, first);
        let duplicate = run_evidence(
            &pool,
            &owner("h001"),
            &h001,
            &operating("target:operating:2", 3, "2027-01", "evidence-h1"),
            now,
        )
        .await
        .err()
        .expect("operating cost evidence is immutable");
        assert_eq!(business(duplicate).code, "OPERATING_COST_ALREADY_VERIFIED");
        let open_month = run_evidence(
            &pool,
            &owner("h001"),
            &h001,
            &operating("target:operating:open", 3, "2027-03", "evidence-h1"),
            now,
        )
        .await
        .err()
        .expect("operating costs are recorded only after month end");
        assert_eq!(business(open_month).code, "MONTH_NOT_CLOSED");
        let bad_evidence = run_evidence(
            &pool,
            &owner("h001"),
            &h001,
            &operating("target:operating:bad", 3, "2027-01", "evidence-bad"),
            now,
        )
        .await
        .err()
        .expect("quarantined evidence is not available");
        assert_eq!(business(bad_evidence).code, "EVIDENCE_REQUIRED");
        let cross_holding = run_evidence(
            &pool,
            &owner("h002"),
            &HoldingId::new("h002").unwrap(),
            &operating("target:operating:cross", 1, "2027-01", "evidence-h1"),
            now,
        )
        .await
        .err()
        .expect("h001 evidence is not visible to h002");
        assert_eq!(business(cross_holding).code, "EVIDENCE_REQUIRED");

        let capacity_ok = run_evidence(
            &pool,
            &owner("h001"),
            &h001,
            &capacity("target:capacity:1", 3, "2027-04"),
            now,
        )
        .await
        .unwrap();
        assert_eq!(capacity_ok.result_json, "{\"ok\":true}");
        let reviewed: (i64, String) = sqlx::query_as(
            "SELECT projected_original_bytes,approved_by FROM billing_target_capacity_reviews \
             WHERE holding_id='h001' AND effective_month='2027-04'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(reviewed, (10_000_000_000, "owner".into()));
        let duplicate_capacity = run_evidence(
            &pool,
            &owner("h001"),
            &h001,
            &capacity("target:capacity:2", 4, "2027-04"),
            now,
        )
        .await
        .err()
        .expect("capacity evidence is immutable");
        assert_eq!(
            business(duplicate_capacity).code,
            "CAPACITY_ALREADY_REVIEWED"
        );
        let current_month = run_evidence(
            &pool,
            &owner("h001"),
            &h001,
            &capacity("target:capacity:current", 4, "2027-02"),
            now,
        )
        .await
        .err()
        .expect("capacity review must precede the effective month");
        assert_eq!(business(current_month).code, "FUTURE_MONTH_REQUIRED");
        let platform = BillingActor {
            platform: true,
            role: "platform_admin".into(),
            holding_id: None,
            ..owner("h001")
        };
        let platform_denied = run_evidence(
            &pool,
            &platform,
            &h001,
            &capacity("target:capacity:platform", 4, "2027-05"),
            now,
        )
        .await
        .err()
        .expect("target evidence is a Holding-owner command");
        assert_eq!(business(platform_denied).code, "FORBIDDEN");
        let stale = run_evidence(
            &pool,
            &owner("h001"),
            &h001,
            &capacity("target:capacity:stale", 3, "2027-05"),
            now,
        )
        .await
        .err()
        .expect("stale account version conflicts");
        assert_eq!(business(stale).code, "VERSION_CONFLICT");
        let audit: i64 = sqlx::query_scalar("SELECT count(*) FROM billing_audit")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(audit, 2);
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM billing_receipts")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(receipts, 2);
    });
}

#[test]
fn target_provider_cost_and_statement_commands_verify_fx_evidence_and_totals() {
    with_database(|pool| async move {
        migrate(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES \
             ('h001','acme','Acme','write-h001','read-h001','files-h001','active'),\
             ('h002','northstar','Northstar','write-h002','read-h002','files-h002','active');\
             INSERT INTO billing_accounts(holding_id,version,customer_id,payment_mode,created_at) VALUES \
             ('h001',2,'customer-1','manual',100),\
             ('h002',1,'customer-2','manual',100);\
             INSERT INTO billing_evidence\
             (id,holding_id,filename,mime,size,sha256,actor,created_at,object_key,detected_mime,source_state,scanned_at,scanner_definition_at) VALUES \
             ('evidence-h1','h001','cost.pdf','application/pdf',10,repeat('a',64),'fixture',100,'h001/evidence-h1','application/pdf','available',100,100),\
             ('evidence-bad','h001','bad.pdf','application/pdf',10,repeat('b',64),'fixture',100,'h001/evidence-bad','application/pdf','quarantined',NULL,NULL);\
             INSERT INTO billing_usage_snapshots\
             (holding_id,month,snapshot_id,refreshed_at,source_count,ai_coverage,storage_coverage,unreconciled_originals,seat_coverage,seat_source_count) VALUES \
             ('h001','2027-01','snap-1',100,1,'measured','measured',0,'unavailable',0);\
             INSERT INTO billing_usage_meters\
             (holding_id,month,source_key,company_id,company_name,meter_kind,funding_source,pricing_status,quantity,input_tokens,output_tokens,active_at_period_end,snapshot_id,request_status,gateway_status,provider,model,units_json,cost_status,verified_cost_satang,cost_reconciled_at) VALUES \
             ('h001','2027-01',repeat('a',64),'company-1','Acme','ai','erpx','unpriced',1,100,50,FALSE,'snap-1','succeeded','succeeded','openai','gpt','{\"input\":100}','reconciled',1000,100);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        use erpx_server_billing::public::target_provider_command::{
            BillingTargetProviderCommand, VerifyProviderCost, VerifyStatement,
        };
        let now = TimestampMillis::new(1_802_667_600_000);
        let owner = |holding: &str| BillingActor {
            user_id: UserId::new("owner").unwrap(),
            assignment_id: AssignmentId::new("assignment").unwrap(),
            holding_id: Some(HoldingId::new(holding).unwrap()),
            platform: false,
            role: "holding_owner".into(),
            expires_at: None,
        };
        let provider_cost = |key_value: &str,
                             version: i64,
                             source: &str,
                             provider: &str,
                             statement: &str,
                             currency: &str,
                             original: i64,
                             fx_num: i64,
                             cost: i64,
                             evidence: &str| {
            BillingTargetProviderCommand::ProviderCost(VerifyProviderCost {
                key: key(key_value),
                expected_version: version,
                reason: "Reviewed provider invoice and FX".into(),
                target_month: "2027-01".into(),
                source_key: source.into(),
                provider: provider.into(),
                statement_id: statement.into(),
                original_currency: currency.into(),
                original_minor: original,
                fx_numerator: fx_num,
                fx_denominator: 1,
                provider_cost_satang: cost,
                evidence_id: RecordId::new(evidence).unwrap(),
            })
        };
        let statement = |key_value: &str,
                         version: i64,
                         statement: &str,
                         currency: &str,
                         total: i64,
                         evidence: &str| {
            BillingTargetProviderCommand::Statement(VerifyStatement {
                key: key(key_value),
                expected_version: version,
                reason: "Reviewed provider statement".into(),
                target_month: "2027-01".into(),
                provider: "openai".into(),
                statement_id: statement.into(),
                original_currency: currency.into(),
                provider_total_minor: total,
                evidence_id: RecordId::new(evidence).unwrap(),
            })
        };
        let source = "a".repeat(64);
        let h001 = HoldingId::new("h001").unwrap();
        let first = run_provider(
            &pool,
            &owner("h001"),
            &h001,
            &provider_cost(
                "target:provider:1",
                2,
                &source,
                "openai",
                "stmt-1",
                "USD",
                1000,
                1,
                1000,
                "evidence-h1",
            ),
            now,
        )
        .await
        .unwrap();
        assert_eq!(first.result_json, "{\"ok\":true}");
        let row: (String, i64, i64, String) = sqlx::query_as(
            "SELECT statement_id,original_minor,provider_cost_satang,verified_by \
             FROM billing_target_provider_costs WHERE holding_id='h001' AND month='2027-01'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row, ("stmt-1".into(), 1000, 1000, "owner".into()));
        let version: i32 =
            sqlx::query_scalar("SELECT version FROM billing_accounts WHERE holding_id='h001'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(version, 3);
        let unverified = run_provider(
            &pool,
            &owner("h001"),
            &h001,
            &provider_cost(
                "target:provider:unverified",
                3,
                &source,
                "other",
                "stmt-1",
                "USD",
                1000,
                1,
                1000,
                "evidence-h1",
            ),
            now,
        )
        .await
        .err()
        .expect("provider must match the reconciled meter");
        assert_eq!(business(unverified).code, "PROVIDER_COST_UNVERIFIED");
        let fx = run_provider(
            &pool,
            &owner("h001"),
            &h001,
            &provider_cost(
                "target:provider:fx",
                3,
                &source,
                "openai",
                "stmt-1",
                "USD",
                1000,
                2,
                1000,
                "evidence-h1",
            ),
            now,
        )
        .await
        .err()
        .expect("FX mismatch is rejected");
        assert_eq!(business(fx).code, "FX_MISMATCH");
        let bad_evidence = run_provider(
            &pool,
            &owner("h001"),
            &h001,
            &provider_cost(
                "target:provider:evidence",
                3,
                &source,
                "openai",
                "stmt-1",
                "USD",
                1000,
                1,
                1000,
                "evidence-bad",
            ),
            now,
        )
        .await
        .err()
        .expect("quarantined evidence is not available");
        assert_eq!(business(bad_evidence).code, "EVIDENCE_REQUIRED");
        let duplicate = run_provider(
            &pool,
            &owner("h001"),
            &h001,
            &provider_cost(
                "target:provider:dup",
                3,
                &source,
                "openai",
                "stmt-1",
                "USD",
                1000,
                1,
                1000,
                "evidence-h1",
            ),
            now,
        )
        .await
        .err()
        .expect("verified provider cost is immutable");
        assert_eq!(business(duplicate).code, "PROVIDER_COST_ALREADY_VERIFIED");
        let cross = run_provider(
            &pool,
            &owner("h002"),
            &HoldingId::new("h002").unwrap(),
            &provider_cost(
                "target:provider:cross",
                1,
                &source,
                "openai",
                "stmt-1",
                "USD",
                1000,
                1,
                1000,
                "evidence-h1",
            ),
            now,
        )
        .await
        .err()
        .expect("h002 cannot use h001 meter");
        assert_eq!(business(cross).code, "PROVIDER_COST_UNVERIFIED");

        let statement_ok = run_provider(
            &pool,
            &owner("h001"),
            &h001,
            &statement(
                "target:statement:1",
                3,
                "stmt-1",
                "USD",
                1000,
                "evidence-h1",
            ),
            now,
        )
        .await
        .unwrap();
        assert_eq!(statement_ok.result_json, "{\"ok\":true}");
        let statement_row: (i64, i64) = sqlx::query_as(
            "SELECT provider_total_minor,reconciled_total_minor FROM billing_target_statements \
             WHERE holding_id='h001' AND month='2027-01' AND provider='openai' AND statement_id='stmt-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(statement_row, (1000, 1000));
        let statement_dup = run_provider(
            &pool,
            &owner("h001"),
            &h001,
            &statement(
                "target:statement:dup",
                4,
                "stmt-1",
                "USD",
                1000,
                "evidence-h1",
            ),
            now,
        )
        .await
        .err()
        .expect("statement verification is immutable");
        assert_eq!(business(statement_dup).code, "STATEMENT_ALREADY_VERIFIED");
        let statement_mismatch = run_provider(
            &pool,
            &owner("h001"),
            &h001,
            &statement(
                "target:statement:mismatch",
                4,
                "stmt-2",
                "USD",
                1000,
                "evidence-h1",
            ),
            now,
        )
        .await
        .err()
        .expect("statement total must match source costs");
        assert_eq!(business(statement_mismatch).code, "STATEMENT_MISMATCH");
        let currency_mismatch = run_provider(
            &pool,
            &owner("h001"),
            &h001,
            &statement(
                "target:statement:currency",
                4,
                "stmt-1",
                "EUR",
                1000,
                "evidence-h1",
            ),
            now,
        )
        .await
        .err()
        .expect("statement currency must match source costs");
        assert_eq!(business(currency_mismatch).code, "STATEMENT_MISMATCH");
        let platform = BillingActor {
            platform: true,
            role: "platform_admin".into(),
            holding_id: None,
            ..owner("h001")
        };
        let platform_denied = run_provider(
            &pool,
            &platform,
            &h001,
            &statement(
                "target:statement:platform",
                4,
                "stmt-1",
                "USD",
                1000,
                "evidence-h1",
            ),
            now,
        )
        .await
        .err()
        .expect("provider evidence is a Holding-owner command");
        assert_eq!(business(platform_denied).code, "FORBIDDEN");
        let stale = run_provider(
            &pool,
            &owner("h001"),
            &h001,
            &statement(
                "target:statement:stale",
                3,
                "stmt-1",
                "USD",
                1000,
                "evidence-h1",
            ),
            now,
        )
        .await
        .err()
        .expect("stale account version conflicts");
        assert_eq!(business(stale).code, "VERSION_CONFLICT");
        let audit: i64 = sqlx::query_scalar("SELECT count(*) FROM billing_audit")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(audit, 2);
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM billing_receipts")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(receipts, 2);
    });
}

#[test]
fn rate_target_appends_runs_and_replaces_only_unapproved_periods() {
    with_database(|pool| async move {
        migrate(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES \
             ('h001','acme','Acme','write-h001','read-h001','files-h001','active'),\
             ('h002','northstar','Northstar','write-h002','read-h002','files-h002','active');\
             INSERT INTO billing_accounts(holding_id,version,customer_id,payment_mode,created_at) VALUES \
             ('h001',2,'customer-1','manual',100),\
             ('h002',1,'customer-2','manual',100);\
             INSERT INTO billing_target_prices\
             (id,effective_month,base_monthly_satang,included_original_bytes,storage_satang_per_gb_month,ai_markup_percent,created_at,created_by,seat_terms_status,included_seats,seat_monthly_satang,storage_allowance_mode,rating_enabled) VALUES \
             ('price-1','2026-12',149000,20000000000,500,10,0,'fixture','defined',0,49900,'per_paid_seat_time',TRUE);\
             INSERT INTO billing_target_assignments\
             (holding_id,effective_month,price_id,mode,service_started_at,assigned_by,reason,created_at) VALUES \
             ('h001','2027-01','price-1','shadow',1798736400000,'fixture','fixture',0),\
             ('h001','2027-03','price-1','shadow',1803834000000,'fixture','fixture',0);\
             INSERT INTO billing_usage_snapshots\
             (holding_id,month,snapshot_id,refreshed_at,source_count,ai_coverage,storage_coverage,unreconciled_originals,seat_coverage,seat_source_count) VALUES \
             ('h001','2027-01','snap-1',100,0,'measured','measured',0,'measured',0);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        use erpx_server_billing::public::target_rate_command::RateTarget;
        let now = TimestampMillis::new(1_802_667_600_000);
        let owner = |holding: &str| BillingActor {
            user_id: UserId::new("owner").unwrap(),
            assignment_id: AssignmentId::new("assignment").unwrap(),
            holding_id: Some(HoldingId::new(holding).unwrap()),
            platform: false,
            role: "holding_owner".into(),
            expires_at: None,
        };
        let command = |key_value: &str, version: i64, month: &str| RateTarget {
            key: key(key_value),
            expected_version: version,
            reason: "Rated from reviewed target evidence".into(),
            target_month: month.into(),
        };
        let h001 = HoldingId::new("h001").unwrap();
        let first = run_rate(
            &pool,
            &owner("h001"),
            &h001,
            &command("target:rate:1", 2, "2027-01"),
            now,
        )
        .await
        .unwrap();
        assert_eq!(first.result_json, "{\"ok\":true}");
        let run: (String, String, Option<i64>, String) = sqlx::query_as(
            "SELECT state,meter_snapshot_id,total_satang,actor FROM billing_target_rating_runs \
             WHERE holding_id='h001' AND month='2027-01'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            run,
            (
                "ready".into(),
                "snap-1".into(),
                Some(149_000),
                "owner".into()
            )
        );
        let period: (String, Option<i64>, String) = sqlx::query_as(
            "SELECT state,total_satang,price_id FROM billing_target_rated_periods \
             WHERE holding_id='h001' AND month='2027-01'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(period, ("ready".into(), Some(149_000), "price-1".into()));
        let version: i32 =
            sqlx::query_scalar("SELECT version FROM billing_accounts WHERE holding_id='h001'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(version, 3);
        let replay = run_rate(
            &pool,
            &owner("h001"),
            &h001,
            &command("target:rate:1", 2, "2027-01"),
            now,
        )
        .await
        .unwrap();
        assert_eq!(replay, first);
        let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM billing_target_rating_runs")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(runs, 1);

        let held = run_rate(
            &pool,
            &owner("h001"),
            &h001,
            &command("target:rate:held", 3, "2027-03"),
            now,
        )
        .await
        .unwrap();
        assert_eq!(held.result_json, "{\"ok\":true}");
        let held_period: (String, Option<i64>) = sqlx::query_as(
            "SELECT state,total_satang FROM billing_target_rated_periods \
             WHERE holding_id='h001' AND month='2027-03'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(held_period, ("held".into(), None));
        let missing: String = sqlx::query_scalar(
            "SELECT missing_json FROM billing_target_rating_runs \
             WHERE holding_id='h001' AND month='2027-03'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(missing.contains("month_not_closed"));
        assert!(missing.contains("meter_snapshot_missing"));

        let platform = BillingActor {
            platform: true,
            role: "platform_admin".into(),
            holding_id: None,
            ..owner("h001")
        };
        let denied = run_rate(
            &pool,
            &platform,
            &h001,
            &command("target:rate:platform", 4, "2027-01"),
            now,
        )
        .await
        .err()
        .expect("rate-target is a Holding-owner command");
        assert_eq!(business(denied).code, "FORBIDDEN");
        let stale = run_rate(
            &pool,
            &owner("h001"),
            &h001,
            &command("target:rate:stale", 3, "2027-01"),
            now,
        )
        .await
        .err()
        .expect("stale account version conflicts");
        assert_eq!(business(stale).code, "VERSION_CONFLICT");
        let cross = run_rate(
            &pool,
            &owner("h002"),
            &HoldingId::new("h002").unwrap(),
            &command("target:rate:cross", 1, "2027-01"),
            now,
        )
        .await
        .err()
        .expect("h002 has no assignment");
        assert_eq!(business(cross).code, "TARGET_ASSIGNMENT_REQUIRED");
        let audit: i64 = sqlx::query_scalar("SELECT count(*) FROM billing_audit")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(audit, 2);
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM billing_receipts")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(receipts, 2);
    });
}
