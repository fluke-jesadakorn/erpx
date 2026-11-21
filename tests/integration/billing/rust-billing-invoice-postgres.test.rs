//! Native Billing invoice lifecycle commands on migrated PostgreSQL. These
//! tests prove owner SQL, Holding isolation, exact account CAS, derived money,
//! cost attachment/release, the durable provider handoff row, replay, audit,
//! rollback and committed readback. They do not prove a Next/browser journey,
//! composition mounting, Stripe issue/settlement or the jobs executor.
use erpx_server_billing::public::cost::CommandReceipt;
use erpx_server_billing::public::invoice::{
    BillingInvoiceCommand, CreateInvoice, FinalizeInvoice, UpdateInvoice, VoidInvoice,
    invoice_command,
};
use erpx_server_billing::public::workspace::BillingActor;
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, PoolLimits,
    TransactionOptions, connect_pool, load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, HoldingId, RecordId, TimestampMillis, UserId,
};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row, postgres::PgConnectOptions};
use std::{
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const DUE: i64 = 1_800_604_800_000;
const INVOICE_ID: &str = "billing-invoice-billing-invoice-key-0001";

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn target() -> HoldingId {
    HoldingId::new("h001").unwrap()
}

fn admin() -> BillingActor {
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

fn create(finalize: bool, version: i64) -> BillingInvoiceCommand {
    BillingInvoiceCommand::Create(CreateInvoice {
        key: key("billing-invoice-key-0001"),
        expected_version: version,
        reason: "Approve the reviewed October costs".into(),
        period_id: RecordId::new("period-10").unwrap(),
        finalize,
    })
}

fn update(version: i64) -> BillingInvoiceCommand {
    BillingInvoiceCommand::Update(UpdateInvoice {
        key: key("billing-invoice-key-0002"),
        expected_version: version,
        reason: "Refresh the reviewed draft".into(),
        invoice_id: RecordId::new(INVOICE_ID).unwrap(),
    })
}

fn finalize(version: i64) -> BillingInvoiceCommand {
    BillingInvoiceCommand::Finalize(FinalizeInvoice {
        key: key("billing-invoice-key-0003"),
        expected_version: version,
        reason: "Finalize the reviewed invoice".into(),
        invoice_id: RecordId::new(INVOICE_ID).unwrap(),
    })
}

fn void(version: i64, invoice: &str) -> BillingInvoiceCommand {
    BillingInvoiceCommand::Void(VoidInvoice {
        key: key(&format!("billing-invoice-key-{version:04}")),
        expected_version: version,
        reason: "Void the unpaid invoice".into(),
        invoice_id: RecordId::new(invoice).unwrap(),
    })
}

async fn run(
    pool: &DatabasePool,
    actor: &BillingActor,
    target: &HoldingId,
    command: &BillingInvoiceCommand,
) -> Result<CommandReceipt, PgTransactionError<BusinessError>> {
    let pool = pool.clone();
    let actor = actor.clone();
    let target = target.clone();
    let command = command.clone();
    PgUnitOfWork::new(pool)
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let target = target.clone();
            let command = command.clone();
            Box::pin(
                async move { invoice_command(connection, &actor, &target, &command, NOW).await },
            )
        })
        .await
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
         INSERT INTO billing_accounts(holding_id,version,payment_mode,created_at) VALUES \
         ('h001',4,'manual',100),\
         ('h002',1,'manual',100);\
         INSERT INTO billing_enrollments(id,holding_id,plan_id,started_at) VALUES \
         ('enrollment-1','h001','standard',1770000000000),\
         ('enrollment-2','h002','standard',1770000000000);\
         INSERT INTO billing_periods\
         (id,holding_id,enrollment_id,month,start_at,end_at,plan_id,plan_revision,support_satang,costs_complete,closed_at) VALUES \
         ('period-10','h001','enrollment-1','2026-10',1778000000000,1780000000000,'standard',1,50000,true,1780000000000),\
         ('period-foreign','h002','enrollment-2','2026-10',1778000000000,1780000000000,'standard',1,50000,true,1780000000000);\
         INSERT INTO billing_costs\
         (id,holding_id,period_id,category,amount_satang,service_start,service_end,note,simulated,created_at) VALUES \
         ('cost-1','h001','period-10','hosting',12500,1778000000000,1780000000000,'Provider hosting',false,500),\
         ('cost-2','h001','period-10','storage',5000,1778000000000,1780000000000,'Provider storage',false,500),\
         ('cost-foreign','h002','period-foreign','hosting',9900,1778000000000,1780000000000,'Foreign hosting',false,500);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn count(pool: &DatabasePool, sql: &str) -> i64 {
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
        let mut admin = PgConnection::connect_with(&admin_options)
            .await
            .expect("test administrator unavailable");
        let token = format!(
            "{}{}{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_DATABASE.fetch_add(1, Ordering::Relaxed)
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
            "COMMENT ON DATABASE \"{name}\" IS '{lease}'"
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

#[test]
fn create_finalize_persists_invoice_costs_outbox_receipt_and_replays() {
    with_database(|pool| async move {
        setup(&pool).await;
        let receipt = run(&pool, &admin(), &target(), &create(true, 4))
            .await
            .unwrap();
        assert_eq!(receipt.result_json, "{\"ok\":true}");
        let row = sqlx::query(
            "SELECT kind,status,total_satang,lines_json,due_at,approved_by FROM billing_invoices WHERE id=$1",
        )
        .bind(INVOICE_ID)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("kind"), "main");
        assert_eq!(row.get::<String, _>("status"), "queued");
        assert_eq!(row.get::<i64, _>("total_satang"), 67_500);
        assert_eq!(row.get::<Option<i64>, _>("due_at"), Some(DUE));
        assert_eq!(row.get::<String, _>("approved_by"), "admin");
        let lines: serde_json::Value =
            serde_json::from_str(&row.get::<String, _>("lines_json")).unwrap();
        assert_eq!(lines.as_array().unwrap().len(), 3);
        assert_eq!(lines[0]["amount"], 50_000);
        assert_eq!(
            count(&pool, "SELECT count(*) FROM billing_costs WHERE invoice_id='billing-invoice-billing-invoice-key-0001'").await,
            2
        );
        assert_eq!(
            count(&pool, "SELECT count(*) FROM billing_jobs WHERE id='issue-billing-invoice-billing-invoice-key-0001' AND kind='issue' AND state='pending'").await,
            1
        );
        assert_eq!(
            count(&pool, "SELECT count(*) FROM billing_audit WHERE action='create-invoice' AND reason='Approve the reviewed October costs'").await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT version::bigint FROM billing_accounts WHERE holding_id='h001'"
            )
            .await,
            5
        );
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM billing_receipts WHERE id='billing-invoice-key-0001'"
            )
            .await,
            1
        );

        let replay = run(&pool, &admin(), &target(), &create(true, 4))
            .await
            .unwrap();
        assert_eq!(replay, receipt);
        assert_eq!(
            count(&pool, "SELECT count(*) FROM billing_invoices").await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM billing_audit WHERE action='create-invoice'"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM billing_jobs WHERE kind='issue'"
            )
            .await,
            1
        );
    });
}

#[test]
fn draft_refresh_then_finalize_keeps_money_and_cost_attachment_exact() {
    with_database(|pool| async move {
        setup(&pool).await;
        run(&pool, &admin(), &target(), &create(false, 4))
            .await
            .unwrap();
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM billing_invoices WHERE status='draft' AND total_satang=67500"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM billing_costs WHERE invoice_id IS NOT NULL"
            )
            .await,
            0
        );
        sqlx::raw_sql(
            "INSERT INTO billing_costs\
             (id,holding_id,period_id,category,amount_satang,service_start,service_end,note,simulated,created_at) \
             VALUES ('cost-3','h001','period-10','provider_ai',7000,1778000000000,1780000000000,'Provider AI',true,600)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        run(&pool, &admin(), &target(), &update(5)).await.unwrap();
        assert_eq!(
            count(&pool, "SELECT total_satang FROM billing_invoices WHERE id='billing-invoice-billing-invoice-key-0001'").await,
            74_500
        );
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM billing_costs WHERE invoice_id IS NOT NULL"
            )
            .await,
            0
        );
        run(&pool, &admin(), &target(), &finalize(6)).await.unwrap();
        let row =
            sqlx::query("SELECT status,total_satang,due_at FROM billing_invoices WHERE id=$1")
                .bind(INVOICE_ID)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(row.get::<String, _>("status"), "queued");
        assert_eq!(row.get::<i64, _>("total_satang"), 74_500);
        assert_eq!(row.get::<Option<i64>, _>("due_at"), Some(DUE));
        assert_eq!(
            count(&pool, "SELECT count(*) FROM billing_costs WHERE invoice_id='billing-invoice-billing-invoice-key-0001'").await,
            3
        );
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM billing_jobs WHERE kind='issue'"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT version::bigint FROM billing_accounts WHERE holding_id='h001'"
            )
            .await,
            7
        );
    });
}

#[test]
fn holding_isolation_never_crosses_the_owner_scope() {
    with_database(|pool| async move {
        setup(&pool).await;
        let foreign_period = BillingInvoiceCommand::Create(CreateInvoice {
            key: key("billing-invoice-key-0005"),
            expected_version: 4,
            reason: "Approve another Holding period".into(),
            period_id: RecordId::new("period-foreign").unwrap(),
            finalize: true,
        });
        let error = business(
            run(&pool, &admin(), &target(), &foreign_period)
                .await
                .unwrap_err(),
        );
        assert_eq!(error.code, "PERIOD_REQUIRED");
        assert_eq!(
            count(&pool, "SELECT count(*) FROM billing_invoices").await,
            0
        );

        let error = business(
            run(&pool, &admin(), &target(), &void(4, "invoice-foreign"))
                .await
                .unwrap_err(),
        );
        assert_eq!(error.code, "INVOICE_NOT_FOUND");
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM billing_costs WHERE invoice_id IS NOT NULL"
            )
            .await,
            0
        );
    });
}

#[test]
fn void_releases_costs_writes_the_outbox_and_is_terminal() {
    with_database(|pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO billing_invoices\
             (id,holding_id,period_id,kind,status,total_satang,lines_json,payment_mode,stripe_id,\
              due_at,payment_state,created_at,approved_by,updated_at) VALUES \
             ('invoice-issued','h001','period-10','main','open',12500,'[]','manual','in_test',\
              1790000000000,'unpaid',600,'admin',600);\
             UPDATE billing_costs SET invoice_id='invoice-issued' WHERE id='cost-1';",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        run(&pool, &admin(), &target(), &void(4, "invoice-issued"))
            .await
            .unwrap();
        let row = sqlx::query("SELECT status FROM billing_invoices WHERE id='invoice-issued'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("status"), "void");
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM billing_costs WHERE id='cost-1' AND invoice_id IS NULL"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM billing_jobs WHERE id='void-invoice-issued' AND kind='void'"
            )
            .await,
            1
        );

        let error = business(
            run(&pool, &admin(), &target(), &void(5, "invoice-issued"))
                .await
                .unwrap_err(),
        );
        assert_eq!(error.code, "VOID_UNAVAILABLE");
        assert_eq!(
            count(&pool, "SELECT count(*) FROM billing_jobs WHERE kind='void'").await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT version::bigint FROM billing_accounts WHERE holding_id='h001'"
            )
            .await,
            5
        );
    });
}

#[test]
fn stale_version_and_paid_invoice_fail_without_writes() {
    with_database(|pool| async move {
        setup(&pool).await;
        let error = business(
            run(&pool, &admin(), &target(), &create(true, 3))
                .await
                .unwrap_err(),
        );
        assert_eq!(error.code, "VERSION_CONFLICT");
        assert_eq!(
            count(&pool, "SELECT count(*) FROM billing_invoices").await,
            0
        );

        sqlx::raw_sql(
            "INSERT INTO billing_invoices\
             (id,holding_id,period_id,kind,status,total_satang,lines_json,payment_mode,stripe_id,\
              due_at,payment_state,created_at,approved_by,updated_at) VALUES \
             ('invoice-paid','h001','period-10','main','paid',12500,'[]','manual','in_paid',\
              1790000000000,'paid',600,'admin',600);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let error = business(
            run(&pool, &admin(), &target(), &void(4, "invoice-paid"))
                .await
                .unwrap_err(),
        );
        assert_eq!(error.code, "VOID_UNAVAILABLE");
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM billing_invoices WHERE id='invoice-paid' AND status='paid'"
            )
            .await,
            1
        );
        assert_eq!(count(&pool, "SELECT count(*) FROM billing_jobs").await, 0);
    });
}

#[test]
fn period_gates_and_negative_net_total_are_owner_enforced() {
    with_database(|pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("UPDATE billing_periods SET closed_at=NULL WHERE id='period-10'")
            .execute(pool.raw())
            .await
            .unwrap();
        let error = business(
            run(&pool, &admin(), &target(), &create(true, 4))
                .await
                .unwrap_err(),
        );
        assert_eq!(error.code, "MONTH_NOT_CLOSED");

        sqlx::raw_sql(
            "UPDATE billing_periods SET closed_at=1780000000000,costs_complete=false \
             WHERE id='period-10'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let error = business(
            run(&pool, &admin(), &target(), &create(true, 4))
                .await
                .unwrap_err(),
        );
        assert_eq!(error.code, "COSTS_INCOMPLETE");

        sqlx::raw_sql(
            "UPDATE billing_periods SET costs_complete=true WHERE id='period-10';\
             INSERT INTO billing_costs\
             (id,holding_id,period_id,category,amount_satang,service_start,service_end,note,simulated,created_at) \
             VALUES ('cost-credit','h001','period-10','adjustment',-100000000,1778000000000,1780000000000,'Reviewed credit',false,700);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let error = business(
            run(&pool, &admin(), &target(), &create(true, 4))
                .await
                .unwrap_err(),
        );
        assert_eq!(error.code, "CREDIT_REVIEW_REQUIRED");
        assert_eq!(
            count(&pool, "SELECT count(*) FROM billing_invoices").await,
            0
        );
    });
}

#[test]
fn a_changed_cost_after_draft_rolls_finalize_back() {
    with_database(|pool| async move {
        setup(&pool).await;
        run(&pool, &admin(), &target(), &create(false, 4))
            .await
            .unwrap();
        sqlx::raw_sql(
            "INSERT INTO billing_invoices\
             (id,holding_id,period_id,kind,status,total_satang,lines_json,payment_mode,\
              created_at,approved_by,updated_at) VALUES \
             ('invoice-other','h001','period-10','adjustment','draft',0,'[]','manual',\
              650,'admin',650);\
             UPDATE billing_costs SET invoice_id='invoice-other' WHERE id='cost-1';",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let error = business(
            run(&pool, &admin(), &target(), &finalize(5))
                .await
                .unwrap_err(),
        );
        assert_eq!(error.code, "INVOICE_SOURCE_CHANGED");
        assert_eq!(
            count(&pool, "SELECT count(*) FROM billing_invoices WHERE id='billing-invoice-billing-invoice-key-0001' AND status='draft'").await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM billing_jobs WHERE kind='issue'"
            )
            .await,
            0
        );
        assert_eq!(
            count(
                &pool,
                "SELECT version::bigint FROM billing_accounts WHERE holding_id='h001'"
            )
            .await,
            5
        );
    });
}

#[test]
fn same_key_with_a_different_request_conflicts_without_writes() {
    with_database(|pool| async move {
        setup(&pool).await;
        let first = run(&pool, &admin(), &target(), &create(true, 4))
            .await
            .unwrap();
        assert_eq!(first.result_json, "{\"ok\":true}");
        let error = business(
            run(&pool, &admin(), &target(), &create(false, 5))
                .await
                .unwrap_err(),
        );
        assert_eq!(error.code, "KEY_CONFLICT");
        assert_eq!(
            count(&pool, "SELECT count(*) FROM billing_invoices").await,
            1
        );
        assert_eq!(count(&pool, "SELECT count(*) FROM billing_audit").await, 1);
    });
}
