//! Native POS register-session, bill, void and refund commands on migrated
//! PostgreSQL. Cross-owner Finance/Inventory/PMS effects are recording ports;
//! these tests prove POS-owned persistence, CAS, idempotency and readback.
use erpx_server_pos::public::commands::{
    PgPosCommandRepository, PosActor, PosCatalogProduct, PosCatalogSnapshot, PosCommand,
    PosFinancePort, PosFuture, PosHospitalityPort, PosJournalRequest, PosOperation,
    PosOrganizationPort, PosResult, execute_pos_command,
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
         ('company',true,'THB','{\"chartId\":\"chart\",\"cash\":\"cash\",\"cashTransfer\":\"transfer\",\"cashDifference\":\"difference\"}','owner','finance',1);\
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
    PgUnitOfWork::new(pool.clone())
        .run::<PosResult, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let command = command.clone();
            let catalog = catalog.clone();
            Box::pin(async move {
                let mut repository = PgPosCommandRepository::new(connection);
                execute_pos_command(
                    &actor,
                    &command,
                    &mut repository,
                    catalog.as_ref(),
                    &mut Finance,
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
                &mut Finance,
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
