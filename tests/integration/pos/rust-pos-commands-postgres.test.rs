//! Native POS register-session, bill, void and refund commands on migrated
//! PostgreSQL. Cross-owner Finance/Inventory/PMS effects are recording ports;
//! these tests prove POS-owned persistence, CAS, idempotency and readback.
use erpx_server_pos::public::commands::{
    PgPosCommandRepository, PosActor, PosBankTransaction, PosCatalogProduct, PosCatalogSnapshot,
    PosCommand, PosConsumptionPart, PosFinancePort, PosFuture, PosHospitalityPort, PosInventoryPort,
    PosInvoiceRequest, PosJournalRequest, PosOperation, PosOrganizationPort, PosRefundPaymentRequest,
    PosReceiptRequest, PosResult, PosReturnPart, PosStockRow, execute_pos_command,
};
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, PoolLimits,
    TransactionOptions, connect_pool, load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, RecordId, TimestampMillis,
    UserId,
};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row, postgres::PgConnectOptions};
use std::{
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

struct Finance;
impl PosFinancePort for Finance {
    fn open_period<'a>(&'a mut self, _: &'a CompanyId, _: &'a str) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn post_journal<'a>(
        &'a mut self,
        request: &'a PosJournalRequest,
    ) -> PosFuture<'a, Option<String>> {
        let journal = request.source_key.split(':').next().map(|value| {
            if value == "cash" {
                "journal-cash".to_owned()
            } else {
                "journal-shift".to_owned()
            }
        });
        Box::pin(async move { Ok(journal) })
    }
    fn party_exists<'a>(&'a mut self, _: &'a CompanyId, _: &'a str) -> PosFuture<'a, bool> {
        Box::pin(async { Ok(true) })
    }
    fn record_receipt<'a>(&'a mut self, _: &'a PosReceiptRequest) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn record_invoice<'a>(&'a mut self, _: &'a PosInvoiceRequest) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn bank_transaction<'a>(
        &'a mut self,
        _: &'a CompanyId,
        id: &'a str,
    ) -> PosFuture<'a, Option<PosBankTransaction>> {
        let bank = BANK.with(|bank| bank.borrow().clone()).filter(|bank| bank.id == id);
        Box::pin(async move { Ok(bank) })
    }
    fn attach_bank_payment<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn record_refund_payment<'a>(&'a mut self, _: &'a PosRefundPaymentRequest) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}
thread_local! {
    static BANK: std::cell::RefCell<Option<PosBankTransaction>> = const { std::cell::RefCell::new(None) };
}
fn set_bank(bank: Option<PosBankTransaction>) {
    BANK.with(|value| *value.borrow_mut() = bank);
}
#[derive(Clone)]
struct Inventory;
impl PosInventoryPort for Inventory {
    fn queue_consumption<'a>(
        &'a mut self,
        parts: &'a [PosConsumptionPart],
    ) -> PosFuture<'a, i64> {
        Box::pin(async move { Ok(parts.iter().map(|part| part.expected_value_minor).sum()) })
    }
    fn queue_return<'a>(&'a mut self, _: &'a [PosReturnPart]) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}
struct Hospitality;
impl PosHospitalityPort for Hospitality {
    fn hotel_day_open<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> PosFuture<'a, bool> {
        Box::pin(async { Ok(true) })
    }
    fn room_charge_exists<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, bool> {
        Box::pin(async { Ok(false) })
    }
}
struct Organization;
impl PosOrganizationPort for Organization {
    fn issuer_name<'a>(&'a mut self, _: &'a CompanyId) -> PosFuture<'a, String> {
        Box::pin(async { Ok("Company".into()) })
    }
}

fn actor(user: &str) -> PosActor {
    PosActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: RecordId::new("branch").ok(),
        department_id: None,
        assignment_id: AssignmentId::new("assignment").unwrap(),
        permissions: vec!["pos.sell".into(), "pos.approve".into()],
        modules: vec!["pos".into()],
        expires_at: NOW.get() + 60_000,
    }
}
fn command(operation: PosOperation, key: &str, value: serde_json::Value) -> PosCommand {
    PosCommand::parse(operation, CommandKey::new(key).unwrap(), &value).unwrap()
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
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse','company','branch','WH','Warehouse','active',1);\
         INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tax_bps,status,version) VALUES \
         ('product','company','SKU','Product',10000,700,'active',1);\
         INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES \
         ('warehouse','product',100000,100000);\
         INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES \
         ('chart','company','Chart','THB','owner',0,0);\
         INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES \
         ('period','company','2027','2027-01-01','2027-12-31',0);\
         INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,memo,status,created_by,created_at,updated_at) VALUES \
         ('journal-shift','company','chart','period','J-SHIFT','2027-01-15','THB','pos','shift','POS-GL-SHIFT','posted','owner',0,0),\
         ('journal-cash','company','chart','period','J-CASH','2027-01-15','THB','pos','cash','POS-GL-CASH','posted','owner',0,0);\
         INSERT INTO pos_settings(company_id,enabled,currency,accounts_json,accounting_prepared_by,accounting_approved_by,version) VALUES \
         ('company',true,'THB','{\"chartId\":\"chart\",\"cash\":\"cash\",\"cashTransfer\":\"transfer\",\"cashDifference\":\"difference\",\"edc\":\"edc\",\"receivable\":\"receivable\",\"advance\":\"advance\",\"bank\":\"bank\",\"revenue\":\"revenue\",\"outputTax\":\"outputTax\",\"cogs\":\"cogs\",\"inventory\":\"inventory\",\"refund\":\"refund\"}','owner','finance',1);\
         INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active,version) VALUES \
         ('register','company','branch','warehouse','Retail','retail',true,1);\
         INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,state,version,business_date) VALUES \
         ('shift','register','company','branch','cashier',1800000000000,100000,'open',1,'2027-01-15');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

async fn run(
    pool: &DatabasePool,
    actor: PosActor,
    command: PosCommand,
) -> Result<PosResult, PgTransactionError<BusinessError>> {
    run_with_catalog(pool, actor, command, None).await
}

async fn run_with_catalog(
    pool: &DatabasePool,
    actor: PosActor,
    command: PosCommand,
    catalog: Option<PosCatalogSnapshot>,
) -> Result<PosResult, PgTransactionError<BusinessError>> {
    run_with_snapshots(pool, actor, command, catalog, None).await
}

async fn run_with_snapshots(
    pool: &DatabasePool,
    actor: PosActor,
    command: PosCommand,
    catalog: Option<PosCatalogSnapshot>,
    stock: Option<Vec<PosStockRow>>,
) -> Result<PosResult, PgTransactionError<BusinessError>> {
    run_with_all(pool, actor, command, catalog, stock, Inventory).await
}

async fn run_with_all(
    pool: &DatabasePool,
    actor: PosActor,
    command: PosCommand,
    catalog: Option<PosCatalogSnapshot>,
    stock: Option<Vec<PosStockRow>>,
    inventory: Inventory,
) -> Result<PosResult, PgTransactionError<BusinessError>> {
    PgUnitOfWork::new(pool.clone())
        .run::<PosResult, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let command = command.clone();
            let catalog = catalog.clone();
            let stock = stock.clone();
            let mut inventory = inventory.clone();
            Box::pin(async move {
                let mut repository = PgPosCommandRepository::new(connection);
                execute_pos_command(
                    &actor,
                    &command,
                    &mut repository,
                    catalog.as_ref(),
                    stock.as_deref(),
                    &mut Finance,
                    &mut inventory,
                    &mut Hospitality,
                    &mut Organization,
                    NOW,
                )
                .await
            })
        })
        .await
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
        let name = format!("erpx_dev_rust_pos_{token}");
        let lease = format!("erpx-rust-pos-test:{token}");
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

#[test]
fn shift_open_close_and_review_persist_exact_money_and_readback() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            sqlx::raw_sql("DELETE FROM pos_shifts")
                .execute(pool.raw())
                .await
                .unwrap();
            let opened = run(
                &pool,
                actor("cashier"),
                command(
                    PosOperation::ShiftOpen,
                    "shift-open-0001",
                    serde_json::json!({"registerId":"register","openingMinor":100000}),
                ),
            )
            .await
            .unwrap();
            let shift: (String, String, i64, i64, String) = sqlx::query_as(
                "SELECT state,business_date,version,opening_minor,opened_by FROM pos_shifts WHERE id=$1",
            )
            .bind(&opened.id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                shift,
                (
                    "open".into(),
                    "2027-01-15".into(),
                    1,
                    100000,
                    "cashier".into()
                )
            );
            let _ = &shift;

            let closed = run(
                &pool,
                actor("cashier"),
                command(
                    PosOperation::ShiftClose,
                    "shift-close-0001",
                    serde_json::json!({"id":opened.id,"expectedVersion":1,"countedMinor":103500,"denominations":{"1000":100,"50":70}}),
                ),
            )
            .await
            .unwrap();
            assert_eq!(closed.id, opened.id);
            assert_eq!(closed.version, Some(2));
            let money: (String, i64, i64, i64, String) = sqlx::query_as(
                "SELECT state,expected_minor,variance_minor,counted_minor,closed_by FROM pos_shifts WHERE id=$1",
            )
            .bind(&opened.id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                money,
                ("closed".into(), 100000, 3500, 103500, "cashier".into())
            );

            let reviewed = run(
                &pool,
                actor("manager"),
                command(
                    PosOperation::ShiftReview,
                    "shift-review-0001",
                    serde_json::json!({"id":opened.id,"expectedVersion":2,"reason":"Count reviewed"}),
                ),
            )
            .await
            .unwrap();
            assert_eq!(reviewed.id, opened.id);
            let status: (String, Option<String>, i64) =
                sqlx::query_as("SELECT state,reviewed_by,version FROM pos_shifts WHERE id=$1")
                    .bind(&opened.id)
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(status, ("reviewed".into(), Some("manager".into()), 3));
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM pos_audit WHERE company_id='company'"
                )
                .fetch_one(pool.raw())
                .await
                .unwrap(),
                3
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM pos_receipts WHERE company_id='company'"
                )
                .fetch_one(pool.raw())
                .await
                .unwrap(),
                3
            );
        })
    });
}

#[test]
fn shift_close_and_review_reject_stale_versions_without_writes() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let stale = business(
                run(
                    &pool,
                    actor("cashier"),
                    command(
                        PosOperation::ShiftClose,
                        "shift-close-0002",
                        serde_json::json!({"id":"shift","expectedVersion":9,"countedMinor":100000}),
                    ),
                )
                .await
                .unwrap_err(),
            );
            assert_eq!(stale.code, "POS_VERSION_CONFLICT");
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT version FROM pos_shifts WHERE id='shift'")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap(),
                1
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM pos_receipts WHERE company_id='company'"
                )
                .fetch_one(pool.raw())
                .await
                .unwrap(),
                0
            );
        })
    });
}

#[test]
fn cash_approval_posts_one_journal_and_bumps_the_shift_with_idempotent_retry() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            sqlx::raw_sql(
                "INSERT INTO pos_cash_movements(id,shift_id,amount_minor,reason,created_by,created_at) \
                 VALUES('movement','shift',-5000,'Petty cash','cashier',1800000000000)",
            )
            .execute(pool.raw())
            .await
            .unwrap();
            let input = || {
                command(
                    PosOperation::CashApprove,
                    "cash-approve-0001",
                    serde_json::json!({"id":"shift","expectedVersion":1,"movementId":"movement"}),
                )
            };
            let first = run(&pool, actor("manager"), input()).await.unwrap();
            assert_eq!(first.id, "shift");
            assert_eq!(
                sqlx::query_scalar::<_, Option<String>>(
                    "SELECT approved_by FROM pos_cash_movements WHERE id='movement'"
                )
                .fetch_one(pool.raw())
                .await
                .unwrap(),
                Some("manager".into())
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT version FROM pos_shifts WHERE id='shift'")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap(),
                2
            );
            let posting: (String, String) = sqlx::query_as(
                "SELECT company_id,journal_id FROM pos_postings WHERE source_key='cash:movement'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(posting, ("company".into(), "journal-cash".into()));

            let replay = run(&pool, actor("manager"), input()).await.unwrap();
            assert_eq!(replay.id, first.id);
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pos_cash_movements")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap(),
                1
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pos_postings")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap(),
                1
            );

            let independent = business(
                run(
                    &pool,
                    actor("cashier"),
                    command(
                        PosOperation::CashApprove,
                        "cash-approve-0002",
                        serde_json::json!({"id":"shift","expectedVersion":2,"movementId":"movement"}),
                    ),
                )
                .await
                .unwrap_err(),
            );
            assert_eq!(independent.code, "POS_INDEPENDENT_REVIEW");
        })
    });
}

#[test]
fn sale_create_and_unpaid_void_persist_state_cancel_fulfilments_and_release_reservations() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let created = run(
                &pool,
                actor("cashier"),
                command(
                    PosOperation::SaleCreate,
                    "sale-create-0001",
                    serde_json::json!({"shiftId":"shift","tableId":null,"guests":1,"partyId":null,"serviceMode":"retail","currency":"THB"}),
                ),
            )
            .await
            .unwrap();
            let sale: (String, String, i64, String) =
                sqlx::query_as("SELECT state,currency,version,number FROM pos_sales WHERE id=$1")
                    .bind(&created.id)
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(sale.0, "open");
            assert_eq!(sale.1, "THB");
            assert_eq!(sale.2, 1);
            assert!(sale.3.starts_with("POS-20270115-"));

            sqlx::raw_sql(AssertSqlSafe(format!(
                "INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,recipe_json,accounts_json,currency,business_date,created_at,state) \
                 VALUES('fulfilment','company','branch','warehouse','product',1000,'stock','Register','[]','{{}}','THB','2027-01-15',0,'draft');\
                 INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,subtotal_minor,tax_minor,modifiers_json) \
                 VALUES('line','{0}','fulfilment','product','Product',1000,10000,700,false,10000,700,'[]');\
                 INSERT INTO pos_reservations(fulfillment_id,warehouse_id,product_id,quantity_milli) \
                 VALUES('fulfilment','warehouse','product',1000);",
                created.id
            )))
            .execute(pool.raw())
            .await
            .unwrap();
            let cancelled = run(
                &pool,
                actor("cashier"),
                command(
                    PosOperation::SaleCancel,
                    "sale-cancel-0001",
                    serde_json::json!({"id":created.id,"expectedVersion":1,"attachmentSetIds":[]}),
                ),
            )
            .await
            .unwrap();
            assert_eq!(cancelled.version, Some(2));
            let state: (String, i64) =
                sqlx::query_as("SELECT state,version FROM pos_sales WHERE id=$1")
                    .bind(&created.id)
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(state, ("cancelled".into(), 2));
            assert_eq!(
                sqlx::query_scalar::<_, String>(
                    "SELECT state FROM pos_fulfillments WHERE id='fulfilment'"
                )
                .fetch_one(pool.raw())
                .await
                .unwrap(),
                "cancelled"
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pos_reservations")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap(),
                0
            );

            sqlx::raw_sql(
                "INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,created_by,created_at,updated_at,accounts_json) \
                 VALUES('paid','company','branch','register','shift','POS-PAID','THB','2027-01-15','cashier',0,0,'{}');\
                 INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,reference,state,created_by,created_at,updated_at) \
                 VALUES('payment','paid','company','cash',1000,1000,'','succeeded','cashier',0,0)",
            )
            .execute(pool.raw())
            .await
            .unwrap();
            let payment = business(
                run(
                    &pool,
                    actor("cashier"),
                    command(
                        PosOperation::SaleCancel,
                        "sale-cancel-0002",
                        serde_json::json!({"id":"paid","expectedVersion":1}),
                    ),
                )
                .await
                .unwrap_err(),
            );
            assert_eq!(payment.code, "POS_PAYMENT_STARTED");
        })
    });
}

#[test]
fn refund_request_and_reject_persist_line_tax_capacity_and_independent_review() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            sqlx::raw_sql(
                "INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,subtotal_minor,tax_minor,total_minor,created_by,created_at,updated_at,accounts_json,version) \
                 VALUES('sale','company','branch','register','shift','POS-1','THB','2027-01-15','completed',10000,700,10700,'cashier',0,0,'{}',2);\
                 INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,recipe_json,accounts_json,currency,business_date,created_at,state) \
                 VALUES('fulfilment','company','branch','warehouse','product',2000,'stock','Register','[]','{}','THB','2027-01-15',0,'delivered');\
                 INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,subtotal_minor,tax_minor,modifiers_json) \
                 VALUES('line','sale','fulfilment','product','Product',2000,10000,700,false,10000,700,'[]')",
            )
            .execute(pool.raw())
            .await
            .unwrap();
            let requested = run(
                &pool,
                actor("cashier"),
                command(
                    PosOperation::RefundRequest,
                    "refund-request-0001",
                    serde_json::json!({"id":"sale","expectedVersion":2,"reason":"Return one unit","lines":[{"lineId":"line","quantityMilli":1000,"restock":true}],"amountMinor":null,"payoutShiftId":null}),
                ),
            )
            .await
            .unwrap();
            let refund: (String, i64, i64, String, String) = sqlx::query_as(
                "SELECT state,amount_minor,tax_minor,requested_by,accounting_date FROM pos_refunds WHERE id=$1",
            )
            .bind(&requested.id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                refund,
                (
                    "requested".into(),
                    5350,
                    350,
                    "cashier".into(),
                    "2027-01-15".into()
                )
            );

            let rejected = run(
                &pool,
                actor("manager"),
                command(
                    PosOperation::RefundReject,
                    "refund-reject-0001",
                    serde_json::json!({"id":requested.id,"expectedVersion":1,"reason":"Evidence insufficient","attachmentSetIds":[]}),
                ),
            )
            .await
            .unwrap();
            assert_eq!(rejected.id, requested.id);
            let state: (String, Option<String>, i64) =
                sqlx::query_as("SELECT state,approved_by,version FROM pos_refunds WHERE id=$1")
                    .bind(&requested.id)
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(state, ("rejected".into(), Some("manager".into()), 2));

            let over = business(
                run(
                    &pool,
                    actor("cashier"),
                    command(
                        PosOperation::RefundRequest,
                        "refund-request-0002",
                        serde_json::json!({"id":"sale","expectedVersion":2,"reason":"Return too many","lines":[{"lineId":"line","quantityMilli":3000,"restock":true}],"amountMinor":null,"payoutShiftId":null}),
                    ),
                )
                .await
                .unwrap_err(),
            );
            assert_eq!(over.code, "POS_REFUND_QUANTITY");
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pos_refunds")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap(),
                1
            );

            let same = business(
                run(
                    &pool,
                    actor("cashier"),
                    command(
                        PosOperation::RefundReject,
                        "refund-reject-0002",
                        serde_json::json!({"id":requested.id,"expectedVersion":2,"reason":"Second review"}),
                    ),
                )
                .await
                .unwrap_err(),
            );
            assert_eq!(same.code, "POS_INDEPENDENT_REVIEW");
        })
    });
}

#[test]
fn merged_iam_member_alias_is_denied_as_self_review_for_shift_cash_and_refund() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            sqlx::raw_sql(
                "INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy) VALUES \
                 ('company','manager','principal-manager','person-shared','active','local'),\
                 ('company','manager-alias','principal-alias','person-shared','active','local'),\
                 ('company','manager-other','principal-other','person-other','active','local');\
                 UPDATE pos_shifts SET state='closed',counted_minor=100000,expected_minor=100000,\
                 variance_minor=0,opened_by='manager',closed_by='cashier',version=2 WHERE id='shift';\
                 INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,state,version,business_date) \
                 VALUES('shift-cash','register','company','branch','cashier',1800000000000,100000,'open',1,'2027-01-15');\
                 INSERT INTO pos_cash_movements(id,shift_id,amount_minor,reason,created_by,created_at) \
                 VALUES('movement-cash','shift-cash',1000,'Drawer top-up','manager',1800000000000);\
                 INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,subtotal_minor,tax_minor,total_minor,created_by,created_at,updated_at,accounts_json,version) \
                 VALUES('refund-sale','company','branch','register','shift-cash','POS-REF','THB','2027-01-15','completed',1000,0,1000,'cashier',0,0,'{}',2);\
                 INSERT INTO pos_refunds(id,sale_id,company_id,branch_id,reason,lines_json,amount_minor,state,requested_by,created_at,accounting_date,tax_minor) \
                 VALUES('refund-alias','refund-sale','company','branch','Return','[]',1000,'requested','manager',0,'2027-01-15',0)",
            )
            .execute(pool.raw())
            .await
            .unwrap();

            for (operation, value) in [
                (
                    PosOperation::ShiftReview,
                    serde_json::json!({"id":"shift","expectedVersion":2,"reason":"Count reviewed"}),
                ),
                (
                    PosOperation::CashApprove,
                    serde_json::json!({"id":"shift-cash","expectedVersion":1,"movementId":"movement-cash"}),
                ),
                (
                    PosOperation::RefundReject,
                    serde_json::json!({"id":"refund-alias","expectedVersion":1,"reason":"Evidence insufficient"}),
                ),
            ] {
                let error = business(
                    run(
                        &pool,
                        actor("manager-alias"),
                        command(operation, "alias-denied-0001", value),
                    )
                    .await
                    .unwrap_err(),
                );
                assert_eq!(error.code, "POS_INDEPENDENT_REVIEW", "{operation:?}");
            }
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT version FROM pos_shifts WHERE id='shift'")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap(),
                2
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT version FROM pos_shifts WHERE id='shift-cash'"
                )
                .fetch_one(pool.raw())
                .await
                .unwrap(),
                1
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT version FROM pos_refunds WHERE id='refund-alias'"
                )
                .fetch_one(pool.raw())
                .await
                .unwrap(),
                1
            );

            run(
                &pool,
                actor("manager-other"),
                command(
                    PosOperation::ShiftReview,
                    "distinct-review-0001",
                    serde_json::json!({"id":"shift","expectedVersion":2,"reason":"Count reviewed"}),
                ),
            )
            .await
            .unwrap();
            run(
                &pool,
                actor("manager-other"),
                command(
                    PosOperation::CashApprove,
                    "distinct-cash-0001",
                    serde_json::json!({"id":"shift-cash","expectedVersion":1,"movementId":"movement-cash"}),
                ),
            )
            .await
            .unwrap();
            run(
                &pool,
                actor("manager-other"),
                command(
                    PosOperation::RefundReject,
                    "distinct-refund-0001",
                    serde_json::json!({"id":"refund-alias","expectedVersion":1,"reason":"Evidence insufficient"}),
                ),
            )
            .await
            .unwrap();
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT version FROM pos_shifts WHERE id='shift'")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap(),
                3
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT version FROM pos_shifts WHERE id='shift-cash'"
                )
                .fetch_one(pool.raw())
                .await
                .unwrap(),
                2
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT version FROM pos_refunds WHERE id='refund-alias'"
                )
                .fetch_one(pool.raw())
                .await
                .unwrap(),
                2
            );
        })
    });
}

#[test]
fn non_serializable_transaction_fails_closed() {
    with_database(|pool| {
        Box::pin(async move {
            let mut connection = pool.raw().acquire().await.unwrap();
            sqlx::query("BEGIN ISOLATION LEVEL READ COMMITTED")
                .execute(&mut *connection)
                .await
                .unwrap();
            let mut repository = PgPosCommandRepository::new(&mut connection);
            let error = execute_pos_command(
                &actor("cashier"),
                &command(
                    PosOperation::ShiftOpen,
                    "shift-open-0002",
                    serde_json::json!({"registerId":"register","openingMinor":100000}),
                ),
                &mut repository,
                None,
                None,
                &mut Finance,
                &mut Inventory,
                &mut Hospitality,
                &mut Organization,
                NOW,
            )
            .await
            .unwrap_err();
            match error {
                PgTransactionError::Persistence(error) => {
                    assert_eq!(error.code, "POS_COMMAND_SERIALIZABLE_REQUIRED")
                }
                other => panic!("expected serializable guard, got {other:?}"),
            }
            sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .unwrap();
        })
    });
}

fn service_catalog() -> PosCatalogProduct {
    PosCatalogProduct {
        id: "service".into(),
        name: "Service".into(),
        price_minor: 10_000,
        tax_bps: 700,
        tax_inclusive: false,
        kind: "service".into(),
        station: "Shop".into(),
        recipe_json: "[]".into(),
        modifiers_json: "[]".into(),
        catalog_version: 1,
    }
}

#[test]
fn sale_add_and_update_line_write_the_line_totals_and_versions() {
    with_database(|pool| async move {
            setup(&pool).await;
            sqlx::raw_sql(
                "INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tax_bps,status,version) VALUES('service','company','SVC','Service',10000,700,'active',1); INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,subtotal_minor,tax_minor,total_minor,created_by,created_at,updated_at,accounts_json,service_mode,version) VALUES('sale-add','company','branch','register','shift','POS-ADD','THB','2027-01-15','open',0,0,0,'cashier',0,0,'{}','retail',1)",
            )
            .execute(pool.raw())
            .await
            .unwrap();
            let snapshot = PosCatalogSnapshot {
                register_id: "register".into(),
                product: service_catalog(),
            };
            let add = command(
                PosOperation::SaleAdd,
                "sale-add-key-0001",
                serde_json::json!({"id":"sale-add","expectedVersion":1,"productId":"service","quantityMilli":2000,"modifierIds":[],"discountMinor":0,"note":"First line"}),
            );
            let result = run_with_catalog(&pool, actor("manager"), add, Some(snapshot)).await.unwrap();
            assert_eq!(result.id, "sale-add");
            assert_eq!(result.version, Some(2));
            let line: (String, i64, i64, i64) = sqlx::query_as(
                "SELECT product_id,quantity_milli,subtotal_minor,tax_minor FROM pos_lines WHERE sale_id='sale-add'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(line, ("service".into(), 2000, 20_000, 1_400));
            let sale: (i64, i64, i64, i64) = sqlx::query_as(
                "SELECT subtotal_minor,tax_minor,total_minor,version FROM pos_sales WHERE id='sale-add'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(sale, (20_000, 1_400, 21_400, 2));
            let line_id: String = sqlx::query_scalar(
                "SELECT id FROM pos_lines WHERE sale_id='sale-add'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            let update = command(
                PosOperation::SaleUpdateLine,
                "sale-update-key-0001",
                serde_json::json!({"id":"sale-add","expectedVersion":2,"lineId":line_id,"quantityMilli":3000,"discountMinor":0,"note":"Updated line"}),
            );
            let updated = run(&pool, actor("manager"), update).await.unwrap();
            assert_eq!(updated.id, "sale-add");
            assert_eq!(updated.version, Some(3));
            let updated_line: (i64, i64, i64) = sqlx::query_as(
                "SELECT quantity_milli,subtotal_minor,tax_minor FROM pos_lines WHERE sale_id='sale-add'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(updated_line, (3000, 30_000, 2_100));
            let updated_sale: (i64, i64, i64, i64) = sqlx::query_as(
                "SELECT subtotal_minor,tax_minor,total_minor,version FROM pos_sales WHERE id='sale-add'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(updated_sale, (30_000, 2_100, 32_100, 3));
    });
}

#[test]
fn sale_customer_move_table_and_remove_write_the_basket() {
    with_database(|pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('party','company','customer','P','Party','THB',0,0);\
             INSERT INTO pos_tables(id,register_id,name,seats) VALUES('table','register','T1',2);\
             INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,created_by,created_at,updated_at,accounts_json,service_mode,version) VALUES('sale-basket','company','branch','register','shift','POS-B','THB','2027-01-15','open','cashier',0,0,'{}','retail',1);\
             INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,recipe_json,accounts_json,currency,business_date,created_at,state) VALUES('basket-fulfilment','company','branch','warehouse','product',1000,'stock','Kitchen','[]','{}','THB','2027-01-15',0,'draft');\
             INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,subtotal_minor,tax_minor,modifiers_json) VALUES('basket-line','sale-basket','basket-fulfilment','product','Product',1000,1000,700,false,0,1000,70,'[]');"
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let customer = command(
            PosOperation::SaleCustomer,
            "sale-customer-key-1",
            serde_json::json!({"id":"sale-basket","expectedVersion":1,"partyId":"party"}),
        );
        let result = run(&pool, actor("manager"), customer).await.unwrap();
        assert_eq!(result.version, Some(2));
        assert_eq!(
            sqlx::query_scalar::<_, Option<String>>("SELECT party_id FROM pos_sales WHERE id='sale-basket'")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            Some("party".into())
        );
        let move_table = command(
            PosOperation::SaleMoveTable,
            "sale-table-key-1",
            serde_json::json!({"id":"sale-basket","expectedVersion":2,"tableId":"table"}),
        );
        let result = run(&pool, actor("manager"), move_table).await.unwrap();
        assert_eq!(result.version, Some(3));
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT table_id FROM pos_sales WHERE id='sale-basket'")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            "table"
        );
        let remove = command(
            PosOperation::SaleRemove,
            "sale-remove-key-1",
            serde_json::json!({"id":"sale-basket","expectedVersion":3,"lineId":"basket-line"}),
        );
        let result = run(&pool, actor("manager"), remove).await.unwrap();
        assert_eq!(result.version, Some(4));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pos_lines WHERE sale_id='sale-basket'")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
        let totals: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT subtotal_minor,tax_minor,total_minor,version FROM pos_sales WHERE id='sale-basket'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(totals, (0, 0, 0, 4));
    });
}

#[test]
fn sale_discount_allocates_the_bill_discount_and_refreshes_totals() {
    with_database(|pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,created_by,created_at,updated_at,accounts_json,service_mode,version) VALUES('sale-discount','company','branch','register','shift','POS-D','THB','2027-01-15','open','cashier',0,0,'{}','retail',1);\
             INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,recipe_json,accounts_json,currency,business_date,created_at,state) VALUES('discount-fulfilment','company','branch','warehouse','product',1000,'stock','Kitchen','[]','{}','THB','2027-01-15',0,'draft');\
             INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,subtotal_minor,tax_minor,modifiers_json) VALUES('discount-line','sale-discount','discount-fulfilment','product','Product',1000,1000,700,false,0,1000,70,'[]');"
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let discount = command(
            PosOperation::SaleDiscount,
            "sale-discount-key-1",
            serde_json::json!({"id":"sale-discount","expectedVersion":1,"amountMinor":100}),
        );
        let result = run(&pool, actor("manager"), discount).await.unwrap();
        assert_eq!(result.version, Some(2));
        let line: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT discount_minor,bill_discount_minor,subtotal_minor,tax_minor FROM pos_lines WHERE id='discount-line'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(line, (100, 100, 900, 63));
        let sale: (i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT discount_minor,subtotal_minor,tax_minor,total_minor,version FROM pos_sales WHERE id='sale-discount'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(sale, (100, 900, 63, 963, 2));
    });
}

#[test]
fn sale_merge_moves_the_lines_and_marks_the_source() {
    with_database(|pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,created_by,created_at,updated_at,accounts_json,service_mode,version) VALUES\
             ('sale-merge-a','company','branch','register','shift','POS-MA','THB','2027-01-15','open','cashier',0,0,'{}','retail',1),\
             ('sale-merge-b','company','branch','register','shift','POS-MB','THB','2027-01-15','open','cashier',0,0,'{}','retail',1);\
             INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,recipe_json,accounts_json,currency,business_date,created_at,state) VALUES\
             ('merge-fa','company','branch','warehouse','product',1000,'stock','Kitchen','[]','{}','THB','2027-01-15',0,'draft'),\
             ('merge-fb','company','branch','warehouse','product',1000,'stock','Kitchen','[]','{}','THB','2027-01-15',0,'draft');\
             INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,subtotal_minor,tax_minor,modifiers_json) VALUES\
             ('merge-la','sale-merge-a','merge-fa','product','Product',1000,1000,700,false,0,1000,70,'[]'),\
             ('merge-lb','sale-merge-b','merge-fb','product','Product',1000,1000,700,false,0,1000,70,'[]');"
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let merge = command(
            PosOperation::SaleMerge,
            "sale-merge-key-1",
            serde_json::json!({"id":"sale-merge-a","expectedVersion":1,"targetId":"sale-merge-b","targetVersion":1}),
        );
        let result = run(&pool, actor("manager"), merge).await.unwrap();
        assert_eq!(result.id, "sale-merge-a");
        assert_eq!(result.version, Some(2));
        let source: (String, i64) = sqlx::query_as(
            "SELECT state,version FROM pos_sales WHERE id='sale-merge-a'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(source, ("merged".into(), 2));
        let target: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT subtotal_minor,tax_minor,total_minor,version FROM pos_sales WHERE id='sale-merge-b'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(target, (2000, 140, 2140, 2));
        let lines: i64 = sqlx::query_scalar("SELECT count(*) FROM pos_lines WHERE sale_id='sale-merge-b'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(lines, 2);
        let moved: i64 = sqlx::query_scalar("SELECT count(*) FROM pos_lines WHERE sale_id='sale-merge-a'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(moved, 0);
    });
}

#[test]
fn sale_split_creates_the_target_and_halves_the_fulfillment() {
    with_database(|pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,created_by,created_at,updated_at,accounts_json,service_mode,version,issuer_json) VALUES\
             ('sale-split','company','branch','register','shift','POS-S','THB','2027-01-15','open','cashier',0,0,'{}','retail',1,'{}');\
             INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,recipe_json,accounts_json,currency,business_date,created_at,state) VALUES\
             ('split-fulfilment','company','branch','warehouse','product',2000,'stock','Kitchen','[]','{}','THB','2027-01-15',0,'draft');\
             INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,bill_discount_minor,subtotal_minor,tax_minor,modifiers_json) VALUES\
             ('split-line','sale-split','split-fulfilment','product','Product',2000,1000,700,false,31,21,2000,140,'[]');"
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let split = command(
            PosOperation::SaleSplit,
            "sale-split-key-1",
            serde_json::json!({"id":"sale-split","expectedVersion":1,"lineId":"split-line","quantityMilli":1000}),
        );
        let result = run(&pool, actor("manager"), split).await.unwrap();
        let target = result.id.clone();
        assert_ne!(target, "sale-split");
        let source: (i64, i64, i64) = sqlx::query_as(
            "SELECT subtotal_minor,tax_minor,version FROM pos_sales WHERE id='sale-split'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(source, (1000, 70, 2));
        let source_line: (i64, i64, i64) = sqlx::query_as(
            "SELECT quantity_milli,subtotal_minor,tax_minor FROM pos_lines WHERE id='split-line'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(source_line, (1000, 1000, 70));
        let target_sale: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT subtotal_minor,tax_minor,total_minor,version FROM pos_sales WHERE id=$1",
        )
        .bind(&target)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(target_sale, (1000, 70, 1070, 2));
        let target_line: (i64, i64, i64) = sqlx::query_as(
            "SELECT quantity_milli,subtotal_minor,tax_minor FROM pos_lines WHERE sale_id=$1",
        )
        .bind(&target)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(target_line, (1000, 1000, 70));
        let discounts: (i64, i64) = sqlx::query_as(
            "SELECT COALESCE(SUM(discount_minor),0)::bigint,COALESCE(SUM(bill_discount_minor),0)::bigint \
             FROM pos_lines WHERE product_id='product' AND sale_id IN ('sale-split',$1)",
        )
        .bind(&target)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(discounts, (31, 21));
        let fulfilments: Vec<i64> = sqlx::query_scalar(
            "SELECT quantity_milli FROM pos_fulfillments WHERE product_id='product' AND company_id='company' ORDER BY quantity_milli",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(fulfilments, vec![1000, 1000]);
    });
}

#[test]
fn payment_create_reserves_settles_cash_and_marks_the_sale_pending() {
    with_database(|pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,subtotal_minor,tax_minor,discount_minor,total_minor,created_by,created_at,updated_at,accounts_json,service_mode,version) VALUES('sale-pay','company','branch','register','shift','POS-P','THB','2027-01-15','open',9300,700,0,10000,'cashier',0,0,'{}','retail',1);\
             INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,recipe_json,accounts_json,currency,business_date,created_at,state) VALUES('pay-fulfilment','company','branch','warehouse','product',1000,'service','Shop','[]','{}','THB','2027-01-15',0,'draft');\
             INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,subtotal_minor,tax_minor,modifiers_json) VALUES('pay-line','sale-pay','pay-fulfilment','product','Service',1000,10000,700,false,0,9300,700,'[]');"
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let payment = command(
            PosOperation::PaymentCreate,
            "payment-create-key-1",
            serde_json::json!({"id":"sale-pay","expectedVersion":1,"method":"cash","amountMinor":10000,"tenderedMinor":10000,"reference":""}),
        );
        let result = run(&pool, actor("cashier"), payment).await.unwrap();
        assert_eq!(result.version, Some(2));
        let payment_row: (String, i64, i64, String, String) = sqlx::query_as(
            "SELECT state,amount_minor,tendered_minor,method,created_by FROM pos_payments WHERE sale_id='sale-pay'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(payment_row, ("succeeded".into(), 10000, 10000, "cash".into(), "cashier".into()));
        let sale: (String, i64) = sqlx::query_as(
            "SELECT state,version FROM pos_sales WHERE id='sale-pay'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(sale, ("payment_pending".into(), 2));
    });
}

#[test]
fn sale_complete_consumes_stock_and_marks_the_bill_completed() {
    with_database(|pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,subtotal_minor,tax_minor,discount_minor,total_minor,created_by,created_at,updated_at,accounts_json,service_mode,version) VALUES('sale-complete','company','branch','register','shift','POS-C','THB','2027-01-15','payment_pending',10000,0,0,10000,'cashier',0,0,'{}','retail',1);\
             INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,state,created_by,created_at,updated_at) VALUES('pay','sale-complete','company','cash',10000,10000,'succeeded','cashier',0,0);\
             INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,recipe_json,accounts_json,currency,business_date,created_at,state,reserved) VALUES('complete-fulfilment','company','branch','warehouse','product',1000,'stock','Shop','[{\"productId\":\"product\",\"quantityMilli\":1000}]','{}','THB','2027-01-15',0,'draft',true);\
             INSERT INTO pos_reservations(fulfillment_id,warehouse_id,product_id,quantity_milli) VALUES('complete-fulfilment','warehouse','product',1000);\
             INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,subtotal_minor,tax_minor,modifiers_json) VALUES('complete-line','sale-complete','complete-fulfilment','product','Product',1000,10000,0,false,0,10000,0,'[]');"
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let complete = command(
            PosOperation::SaleComplete,
            "sale-complete-key-1",
            serde_json::json!({"id":"sale-complete","expectedVersion":1}),
        );
        let stock = vec![PosStockRow {
            warehouse_id: "warehouse".into(),
            product_id: "product".into(),
            quantity_milli: 100000,
            reserved_milli: 0,
            value_minor: 100000,
        }];
        let result = run_with_snapshots(&pool, actor("manager"), complete, None, Some(stock))
            .await
            .unwrap();
        assert_eq!(result.id, "sale-complete");
        assert_eq!(result.version, Some(2));
        let sale: (String, i64) = sqlx::query_as(
            "SELECT state,version FROM pos_sales WHERE id='sale-complete'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(sale, ("completed".into(), 2));
        let fulfillment: (String, i64) = sqlx::query_as(
            "SELECT state,cost_minor FROM pos_fulfillments WHERE id='complete-fulfilment'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(fulfillment, ("delivered".into(), 1000));
        let reservations: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pos_reservations WHERE fulfillment_id='complete-fulfilment'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(reservations, 0);
        let moves: (i64, i64, String) = sqlx::query_as(
            "SELECT quantity_milli,value_minor,source_number FROM pos_stock_moves WHERE fulfillment_id='complete-fulfilment'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(moves.0, -1000);
        assert_eq!(moves.1, -1000);
        assert_eq!(moves.2, "POS-complete-fulfilment");
    });
}

#[test]
fn payment_verify_transfer_matches_bank_evidence_and_settles() {
    with_database(|pool| async move {
        setup(&pool).await;
        set_bank(Some(PosBankTransaction {
            id: "bank".into(),
            version: 1,
            match_status: "unmatched".into(),
            bank_account: "bank".into(),
            currency: "THB".into(),
            amount_minor: 10000,
            external_id: "REF-1".into(),
            transaction_date: "2027-01-15".into(),
        }));
        sqlx::raw_sql(
            "INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,subtotal_minor,tax_minor,discount_minor,total_minor,created_by,created_at,updated_at,accounts_json,service_mode,version) VALUES('sale-transfer','company','branch','register','shift','POS-T','THB','2027-01-15','payment_pending',10000,0,0,10000,'cashier',0,0,'{}','retail',1);\
             INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,reference,state,created_by,created_at,updated_at,version) VALUES('transfer-payment','sale-transfer','company','transfer',10000,10000,'REF-1','pending','cashier',0,0,1);"
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let verify = command(
            PosOperation::PaymentVerifyTransfer,
            "payment-verify-key-1",
            serde_json::json!({"id":"transfer-payment","expectedVersion":1,"bankTransactionId":"bank","bankVersion":1}),
        );
        let result = run(&pool, actor("manager"), verify).await.unwrap();
        assert_eq!(result.id, "transfer-payment");
        assert_eq!(result.version, Some(2));
        let payment: (String, i64, Option<String>) = sqlx::query_as(
            "SELECT state,version,finance_payment_id FROM pos_payments WHERE id='transfer-payment'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(payment.0, "succeeded");
        assert_eq!(payment.1, 2);
        assert!(payment.2.is_none(), "the fixture Finance fake does not attach");
        let sale: (String, i64) = sqlx::query_as(
            "SELECT state,version FROM pos_sales WHERE id='sale-transfer'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(sale, ("payment_pending".into(), 2));
    });
}

#[test]
fn payment_cancel_transfer_cancels_and_reopens_or_keeps_the_bill_pending() {
    with_database(|pool| async move {
        setup(&pool).await;
        set_bank(None);
        sqlx::raw_sql(
            "INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,subtotal_minor,tax_minor,discount_minor,total_minor,created_by,created_at,updated_at,accounts_json,service_mode,version) VALUES('sale-cancel-transfer','company','branch','register','shift','POS-TC','THB','2027-01-15','payment_pending',10000,0,0,10000,'cashier',0,0,'{}','retail',1);\
             INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,reference,state,created_by,created_at,updated_at,version) VALUES('cancel-payment','sale-cancel-transfer','company','transfer',10000,10000,'REF-2','pending','cashier',0,0,1);"
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let cancel = command(
            PosOperation::PaymentCancelTransfer,
            "payment-cancel-key-1",
            serde_json::json!({"id":"cancel-payment","expectedVersion":1}),
        );
        let result = run(&pool, actor("manager"), cancel).await.unwrap();
        assert_eq!(result.version, Some(2));
        let payment: (String, i64) = sqlx::query_as(
            "SELECT state,version FROM pos_payments WHERE id='cancel-payment'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(payment, ("cancelled".into(), 2));
        let sale: (String, i64) = sqlx::query_as(
            "SELECT state,version FROM pos_sales WHERE id='sale-cancel-transfer'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(sale, ("open".into(), 2));
    });
}

#[test]
fn refund_approve_returns_stock_and_settles_the_cash_refund() {
    with_database(|pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,subtotal_minor,tax_minor,discount_minor,total_minor,created_by,created_at,updated_at,accounts_json,service_mode,version) VALUES('sale-refund','company','branch','register','shift','POS-R','THB','2027-01-15','completed',10000,0,0,10000,'cashier',0,0,'{}','retail',5);\
             INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,state,created_by,created_at,updated_at) VALUES('refund-payment','sale-refund','company','cash',10000,10000,'succeeded','cashier',0,0);\
             INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,recipe_json,accounts_json,currency,business_date,created_at,state,cost_minor) VALUES('refund-fulfilment','company','branch','warehouse','product',1000,'stock','Shop','[]','{}','THB','2027-01-15',0,'delivered',1000);\
             INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,subtotal_minor,tax_minor,modifiers_json) VALUES('refund-line','sale-refund','refund-fulfilment','product','Product',1000,10000,0,false,0,10000,0,'[]');\
             INSERT INTO pos_refunds(id,sale_id,company_id,branch_id,payout_shift_id,kind,reason,lines_json,amount_minor,tax_minor,state,requested_by,created_at,accounting_date,version) VALUES('refund-request','sale-refund','company','branch','shift','sale','Customer return','[{\"lineId\":\"refund-line\",\"quantityMilli\":1000,\"restock\":true,\"subtotalMinor\":10000,\"taxMinor\":0,\"previousReturned\":0}]',10000,0,'requested','cashier',0,'2027-01-15',1);"
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let approve = command(
            PosOperation::RefundApprove,
            "refund-approve-key-1",
            serde_json::json!({"id":"refund-request","expectedVersion":1,"reason":"Return accepted"}),
        );
        let result = run(&pool, actor("manager"), approve).await.unwrap();
        assert_eq!(result.id, "refund-request");
        assert_eq!(result.version, Some(2));
        let refund: (String, String, i64) = sqlx::query_as(
            "SELECT state,approved_by,version FROM pos_refunds WHERE id='refund-request'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(refund, ("completed".into(), "manager".into(), 2));
        let line: (i64, i64, i64) = sqlx::query_as(
            "SELECT returned_milli,refunded_minor,returned_cost_minor FROM pos_lines WHERE id='refund-line'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(line, (1000, 10000, 1000));
        let payment: i64 = sqlx::query_scalar(
            "SELECT refunded_minor FROM pos_payments WHERE id='refund-payment'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(payment, 10000);
        let refund_payment: (i64, String) = sqlx::query_as(
            "SELECT amount_minor,state FROM pos_refund_payments WHERE refund_id='refund-request'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(refund_payment, (10000, "succeeded".into()));
        let stock_move: (String, i64, i64) = sqlx::query_as(
            "SELECT kind,quantity_milli,value_minor FROM pos_stock_moves WHERE fulfillment_id='refund-fulfilment' AND source_id='refund-request'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(stock_move, ("return".into(), 1000, 1000));
    });
}

#[test]
fn settings_accounting_table_and_discount_commands_persist_and_reject_stale_versions() {
    with_database(|pool| async move {
        setup(&pool).await;
        let mut company = actor("manager");
        company.branch_id = None;
        company.permissions = vec![
            "pos.sell".into(),
            "pos.approve".into(),
            "pos.configure".into(),
            "pos.accounting.approve".into(),
        ];
        let approved = run(
            &pool,
            company.clone(),
            command(
                PosOperation::AccountingApprove,
                "pos-accounting-approve-1",
                serde_json::json!({"expectedVersion":1}),
            ),
        )
        .await
        .unwrap();
        assert_eq!(approved.version, Some(2));
        let settings: (Option<String>, i64) = sqlx::query_as(
            "SELECT accounting_approved_by,version FROM pos_settings WHERE company_id='company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(settings, (Some("manager".into()), 2));

        let stale = run(
            &pool,
            company.clone(),
            command(
                PosOperation::AccountingApprove,
                "pos-accounting-approve-2",
                serde_json::json!({"expectedVersion":1}),
            ),
        )
        .await
        .unwrap_err();
        assert_eq!(business(stale).code, "POS_VERSION_CONFLICT");

        let operations = run(
            &pool,
            company.clone(),
            command(
                PosOperation::SettingsOperations,
                "pos-settings-operations-1",
                serde_json::json!({"expectedVersion":2,"enabled":false,"cashierDiscountBps":250}),
            ),
        )
        .await
        .unwrap();
        assert_eq!(operations.version, Some(3));
        let settings: (bool, i64, i64) = sqlx::query_as(
            "SELECT enabled,cashier_discount_bps,version FROM pos_settings WHERE company_id='company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(settings, (false, 250, 3));

        let table = run(
            &pool,
            actor("manager"),
            command(
                PosOperation::TableCreate,
                "pos-table-create-1",
                serde_json::json!({"registerId":"register","name":"Table 1","seats":4}),
            ),
        )
        .await
        .unwrap();
        let saved: (String, i64) = sqlx::query_as(
            "SELECT name,seats FROM pos_tables WHERE id=$1",
        )
        .bind(&table.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(saved, ("Table 1".into(), 4));
        let duplicate = run(
            &pool,
            actor("manager"),
            command(
                PosOperation::TableCreate,
                "pos-table-create-2",
                serde_json::json!({"registerId":"register","name":"Table 1","seats":2}),
            ),
        )
        .await
        .unwrap_err();
        assert_eq!(business(duplicate).code, "POS_TABLE_NAME_EXISTS");

        sqlx::raw_sql(
            "INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,created_by,created_at,updated_at,accounts_json,service_mode,version,discount_requested_by) VALUES('sale-approve','company','branch','register','shift','POS-A','THB','2027-01-15','open','cashier',0,0,'{}','retail',1,'requester');",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let discount = run(
            &pool,
            actor("manager"),
            command(
                PosOperation::SaleApproveDiscount,
                "pos-sale-approve-discount-1",
                serde_json::json!({"id":"sale-approve","expectedVersion":1,"reason":"Manager approved"}),
            ),
        )
        .await
        .unwrap();
        assert_eq!(discount.id, "sale-approve");
        let sale: (Option<String>, Option<String>, i64) = sqlx::query_as(
            "SELECT discount_approved_by,discount_reason,version FROM pos_sales WHERE id='sale-approve'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            sale,
            (Some("manager".into()), Some("Manager approved".into()), 2)
        );
    });
}
