//! Native POS workspace/day reads on migrated PostgreSQL.
//! These tests prove owner SQL, Company/branch isolation, the retained tab
//! gates, bounded pages, serializable transaction guards, rollback and
//! persisted readback. They do not prove a Next/browser journey, the retained
//! history pagination/report/accounting reads, provider effects or composition
//! mounting.
use erpx_server_pos::public::read::{
    PgPosWorkspaceRead, PosReadActor, PosWorkspace, PosWorkspaceEntitlements, PosWorkspaceQuery,
    PosWorkspaceRepository, PosWorkspaceTab, pos_workspace,
};
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, PoolLimits,
    TransactionOptions, connect_pool, load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row, postgres::PgConnectOptions};
use std::{
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const BUSINESS_DATE: &str = "2027-01-15";

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn actor(company: &str, branch: Option<&str>) -> PosReadActor {
    PosReadActor {
        user_id: "cashier".into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: None,
        permissions: vec!["pos.read".into(), "pos.reports".into()],
        modules: vec!["pos".into()],
        expires_at: NOW.get() + 60_000,
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
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version) VALUES \
         ('product','company','P1','Coffee','each',50000,700,'active',1),\
         ('product-f','foreign','P1','Foreign coffee','each',50000,700,'active',1);\
         INSERT INTO pos_settings(company_id,enabled,currency,timezone,cashier_discount_bps,accounts_json,version) VALUES \
         ('company',true,'THB','Asia/Bangkok',500,'{}',3),\
         ('foreign',true,'USD','Asia/Bangkok',0,'{}',1);\
         INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active,version) VALUES \
         ('register','company','branch','warehouse','Retail','retail',true,1),\
         ('register-b','company','branch-b','warehouse-b','Retail B','retail',true,1),\
         ('register-f','foreign','foreign-branch','warehouse-f','Foreign','retail',true,1);\
         INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,state,version,business_date) VALUES \
         ('shift-1','register','company','branch','cashier',1700000000000,100000,'open',2,'2027-01-15'),\
         ('shift-closed','register','company','branch','cashier',1690000000000,50000,'closed',4,'2027-01-14'),\
         ('shift-b','register-b','company','branch-b','cashier',1700000000000,200000,'open',1,'2027-01-15'),\
         ('shift-f','register-f','foreign','foreign-branch','cashier',1700000000000,300000,'open',1,'2027-01-15');\
         INSERT INTO pos_cash_movements(id,shift_id,amount_minor,reason,created_by,approved_by,created_at) VALUES \
         ('movement-pending','shift-1',50000,'Opening float top-up','cashier',NULL,1700000100000),\
         ('movement-approved','shift-1',30000,'Approved payout','cashier','reviewer',1700000200000),\
         ('movement-b','shift-b',70000,'Branch B top-up','cashier',NULL,1700000300000),\
         ('movement-f','shift-f',90000,'Foreign top-up','cashier',NULL,1700000400000);\
         INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,recipe_json,accounts_json,currency,business_date,state,created_at) VALUES \
         ('fulfillment','company','branch','warehouse','product',2000,'stock','Kitchen','[]','{}','THB','2027-01-15','delivered',1700000500000),\
         ('fulfillment-f','foreign','foreign-branch','warehouse-f','product-f',2000,'stock','Kitchen','[]','{}','USD','2027-01-15','delivered',1700000500000);\
         INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,subtotal_minor,tax_minor,discount_minor,total_minor,created_by,created_at,updated_at,accounts_json,issuer_json,version) VALUES \
         ('sale-completed','company','branch','register','shift-1','POS-0001','THB','2027-01-15','completed',100000,7000,0,107000,'cashier',1700000600000,1700000600000,'{}','{}',4),\
         ('sale-open','company','branch','register','shift-1','POS-0002','THB','2027-01-15','open',20000,1400,0,21400,'cashier',1700000700000,1700000700000,'{}','{}',1),\
         ('sale-b','company','branch-b','register-b','shift-b','POS-0003','THB','2027-01-15','completed',30000,2100,0,32100,'cashier',1700000800000,1700000800000,'{}','{}',2),\
         ('sale-f','foreign','foreign-branch','register-f','shift-f','POS-0004','USD','2027-01-15','completed',40000,0,0,40000,'cashier',1700000900000,1700000900000,'{}','{}',1);\
         INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,subtotal_minor,tax_minor,modifiers_json,returned_milli,refunded_minor) VALUES \
         ('line','sale-completed','fulfillment','product','Coffee',2000,50000,700,false,0,100000,7000,'[]',0,0),\
         ('line-f','sale-f','fulfillment-f','product-f','Foreign coffee',2000,50000,0,false,0,40000,0,'[]',0,0);\
         INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,reference,state,created_by,created_at,updated_at,version,provider_received_at) VALUES \
         ('payment-cash','sale-completed','company','cash',107000,120000,'','succeeded','cashier',1700001000000,1700001000000,1,NULL),\
         ('payment-transfer','sale-completed','company','transfer',20000,20000,'REF-1','pending','cashier',1700001100000,1700001100000,1,1700001200000),\
         ('payment-f','sale-f','foreign','cash',40000,40000,'','succeeded','cashier',1700001000000,1700001000000,1,NULL);\
         INSERT INTO pos_day_closes(id,company_id,branch_id,business_date,currency,state,snapshot_json,source_digest,prepared_by,prepared_at,version,confirmed_by,confirmed_at) VALUES \
         ('day','company','branch','2027-01-15','THB','prepared','{}','digest','cashier',1700001300000,1,NULL,NULL),\
         ('day-old','company','branch','2027-01-14','THB','confirmed','{}','digest-old','cashier',1690000000000,2,'reviewer',1690000001000),\
         ('day-f','foreign','foreign-branch','2027-01-15','USD','prepared','{}','digest-f','cashier',1700001300000,1,NULL,NULL);",
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

async fn read(
    pool: &DatabasePool,
    actor: PosReadActor,
    query: PosWorkspaceQuery,
) -> Result<PosWorkspace, PgTransactionError<BusinessError>> {
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let query = query.clone();
            Box::pin(async move {
                let mut repository = PgPosWorkspaceRead::new(connection);
                pos_workspace(
                    &actor,
                    &query,
                    &PosWorkspaceEntitlements::default(),
                    &mut repository,
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

#[test]
fn reads_the_current_branch_workspace_without_writing_owner_rows() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let workspace = read(
                &pool,
                actor("company", Some("branch")),
                PosWorkspaceQuery::default(),
            )
            .await
            .unwrap();
            assert_eq!(workspace.business_date, BUSINESS_DATE);
            assert_eq!(
                workspace
                    .registers
                    .iter()
                    .map(|row| row.id.as_str())
                    .collect::<Vec<_>>(),
                vec!["register"]
            );
            assert_eq!(workspace.registers[0].warehouse_name, "Warehouse");
            let settings = workspace.settings.as_ref().unwrap();
            assert_eq!(settings.currency, "THB");
            assert_eq!(settings.cashier_discount_bps, 500);
            assert_eq!(settings.version, 3);

            assert_eq!(
                workspace
                    .shifts
                    .iter()
                    .map(|row| row.id.as_str())
                    .collect::<Vec<_>>(),
                vec!["shift-1", "shift-closed"]
            );
            let open = &workspace.shifts[0];
            assert_eq!(open.state, "open");
            // 100000 opening + 107000 succeeded cash payment + 30000 approved
            // movement; the pending 50000 movement and the pending transfer are
            // excluded exactly like the retained `readPosPayments1` aggregate.
            assert_eq!(open.cash_expected_minor, 237_000);
            assert_eq!(open.register_name, "Retail");
            assert_eq!(workspace.shifts[1].state, "closed");

            assert_eq!(
                workspace
                    .cash_movements
                    .iter()
                    .map(|row| row.id.as_str())
                    .collect::<Vec<_>>(),
                vec!["movement-approved", "movement-pending"]
            );
            assert_eq!(workspace.cash_summary.pending_count, 1);
            assert_eq!(workspace.cash_summary.pending_minor, 50_000);
            assert_eq!(workspace.cash_summary.approved_count, 1);
            assert_eq!(workspace.cash_summary.approved_minor, 30_000);

            assert_eq!(workspace.day.state, "prepared");
            assert_eq!(
                workspace.day.close.as_ref().map(|close| close.id.as_str()),
                Some("day")
            );
            assert_eq!(
                workspace
                    .sales
                    .iter()
                    .map(|row| row.id.as_str())
                    .collect::<Vec<_>>(),
                vec!["sale-open", "sale-completed"]
            );
            assert_eq!(workspace.sales[1].number, "POS-0001");
            assert_eq!(workspace.sales[1].total_minor, 107_000);
            assert!(workspace.sale.is_none());
            assert!(workspace.lines.is_empty());
            assert!(workspace.payments.is_empty());
            assert_eq!(workspace.permissions.get("pos.read"), Some(&true));
            assert_eq!(workspace.permissions.get("finance.write"), Some(&false));

            assert_eq!(count(&pool, "pos_sales", "company_id='company'").await, 3);
            assert_eq!(
                count(&pool, "pos_cash_movements", "id LIKE 'movement%'").await,
                4
            );
            assert_eq!(
                count(&pool, "pos_day_closes", "company_id='company'").await,
                2
            );
        })
    });
}

#[test]
fn company_wide_read_excludes_foreign_company() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let workspace = read(&pool, actor("company", None), PosWorkspaceQuery::default())
                .await
                .unwrap();
            assert_eq!(
                workspace
                    .registers
                    .iter()
                    .map(|row| row.id.as_str())
                    .collect::<Vec<_>>(),
                vec!["register", "register-b"]
            );
            assert!(
                workspace
                    .sales
                    .iter()
                    .all(|row| row.company_id == "company")
            );
            assert_eq!(workspace.sales.len(), 3);
            assert_eq!(workspace.shifts.len(), 3);
            // A Company-wide context has no single branch day close.
            assert_eq!(workspace.day.state, "open");
            assert!(workspace.day.close.is_none());

            let foreign = read(
                &pool,
                actor("foreign", Some("foreign-branch")),
                PosWorkspaceQuery::default(),
            )
            .await
            .unwrap();
            assert_eq!(foreign.registers.len(), 1);
            assert_eq!(foreign.registers[0].id, "register-f");
            assert_eq!(foreign.settings.as_ref().unwrap().currency, "USD");
            assert_eq!(
                foreign
                    .sales
                    .iter()
                    .map(|row| row.id.as_str())
                    .collect::<Vec<_>>(),
                vec!["sale-f"]
            );
            assert_eq!(foreign.shifts[0].id, "shift-f");
            assert_eq!(foreign.day.state, "prepared");
        })
    });
}

#[test]
fn retained_tab_gates_keep_the_same_lists() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let shifts = read(
                &pool,
                actor("company", Some("branch")),
                PosWorkspaceQuery {
                    tab: Some(PosWorkspaceTab::Shifts),
                    ..PosWorkspaceQuery::default()
                },
            )
            .await
            .unwrap();
            assert!(shifts.sales.is_empty());
            assert_eq!(shifts.cash_movements.len(), 2);
            assert_eq!(shifts.shifts.len(), 2);

            let sale = read(
                &pool,
                actor("company", Some("branch")),
                PosWorkspaceQuery {
                    tab: Some(PosWorkspaceTab::Sale),
                    ..PosWorkspaceQuery::default()
                },
            )
            .await
            .unwrap();
            assert_eq!(sale.sales.len(), 2);
            assert!(sale.cash_movements.is_empty());
            assert_eq!(sale.cash_summary.pending_count, 0);

            let reports = read(
                &pool,
                actor("company", Some("branch")),
                PosWorkspaceQuery {
                    tab: Some(PosWorkspaceTab::Reports),
                    ..PosWorkspaceQuery::default()
                },
            )
            .await
            .unwrap();
            assert!(reports.sales.is_empty());
            assert!(reports.shifts.is_empty());
            assert!(reports.cash_movements.is_empty());
            assert_eq!(reports.registers.len(), 1);
            assert_eq!(reports.day.state, "prepared");
        })
    });
}

#[test]
fn selected_sale_detail_is_scoped_and_bounded() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let detail = read(
                &pool,
                actor("company", Some("branch")),
                PosWorkspaceQuery {
                    sale_id: Some("sale-completed".into()),
                    ..PosWorkspaceQuery::default()
                },
            )
            .await
            .unwrap();
            assert_eq!(detail.sale.as_ref().unwrap().id, "sale-completed");
            assert_eq!(detail.lines.len(), 1);
            assert_eq!(detail.lines[0].fulfillment_state, "delivered");
            assert_eq!(detail.lines[0].quantity_milli, 2_000);
            assert_eq!(
                detail
                    .payments
                    .iter()
                    .map(|row| (row.method.as_str(), row.state.as_str()))
                    .collect::<Vec<_>>(),
                vec![("cash", "succeeded"), ("transfer", "pending")]
            );
            assert!(!detail.payments[0].provider_confirmed);
            assert!(detail.payments[1].provider_confirmed);

            for (company, branch, sale) in [
                ("company", Some("branch"), "sale-b"),
                ("company", Some("branch"), "missing"),
                ("foreign", Some("foreign-branch"), "sale-completed"),
            ] {
                let error = business(
                    read(
                        &pool,
                        actor(company, branch),
                        PosWorkspaceQuery {
                            sale_id: Some(sale.into()),
                            ..PosWorkspaceQuery::default()
                        },
                    )
                    .await
                    .unwrap_err(),
                );
                assert_eq!(error.kind, FailureKind::NotFound);
                assert_eq!(error.code, "POS_NOT_FOUND");
            }

            let page = read(
                &pool,
                actor("company", Some("branch")),
                PosWorkspaceQuery {
                    limit: 1,
                    ..PosWorkspaceQuery::default()
                },
            )
            .await
            .unwrap();
            assert_eq!(page.sales.len(), 1);
            assert_eq!(page.sales[0].id, "sale-open");

            let searched = read(
                &pool,
                actor("company", Some("branch")),
                PosWorkspaceQuery {
                    search: "POS-0001".into(),
                    ..PosWorkspaceQuery::default()
                },
            )
            .await
            .unwrap();
            assert_eq!(
                searched
                    .sales
                    .iter()
                    .map(|row| row.id.as_str())
                    .collect::<Vec<_>>(),
                vec!["sale-completed"]
            );
        })
    });
}

#[test]
fn read_requires_a_serializable_caller_transaction() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let mut connection = pool.raw().acquire().await.unwrap();
            let error = PgPosWorkspaceRead::new(&mut connection)
                .workspace(
                    &actor("company", Some("branch")),
                    &PosWorkspaceQuery::default(),
                    BUSINESS_DATE,
                )
                .await
                .unwrap_err();
            match error {
                PgTransactionError::Persistence(error) => {
                    assert_eq!(error.code, "POS_WORKSPACE_TRANSACTION_REQUIRED")
                }
                other => panic!("expected a transaction guard, got {other:?}"),
            }
            drop(connection);

            let mut connection = pool.raw().acquire().await.unwrap();
            let mut transaction = connection
                .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
                .await
                .unwrap();
            let error = PgPosWorkspaceRead::new(&mut transaction)
                .workspace(
                    &actor("company", Some("branch")),
                    &PosWorkspaceQuery::default(),
                    BUSINESS_DATE,
                )
                .await
                .unwrap_err();
            match error {
                PgTransactionError::Persistence(error) => {
                    assert_eq!(error.code, "POS_WORKSPACE_SERIALIZABLE_REQUIRED")
                }
                other => panic!("expected a serializable guard, got {other:?}"),
            }
            transaction.rollback().await.unwrap();
        })
    });
}

#[test]
fn policy_denial_and_rollback_leave_only_persisted_source_rows() {
    with_database(|pool| {
        Box::pin(async move {
            setup(&pool).await;
            let mut denied = actor("company", Some("branch"));
            denied.permissions.clear();
            match read(&pool, denied, PosWorkspaceQuery::default()).await {
                Err(PgTransactionError::Business(error)) => {
                    assert_eq!(error.kind, FailureKind::Forbidden);
                    assert_eq!(error.code, "PERMISSION_REQUIRED");
                }
                other => panic!("expected the policy failure, got {other:?}"),
            }

            let result = PgUnitOfWork::new(pool.clone())
                .run::<(), BusinessError, _>(TransactionOptions::default(), |connection| {
                    Box::pin(async move {
                        sqlx::raw_sql(
                            "INSERT INTO pos_cash_movements(id,shift_id,amount_minor,reason,created_by,approved_by,created_at) \
                             VALUES('movement-uncommitted','shift-1',1000,'Uncommitted','cashier',NULL,1700001400000);",
                        )
                        .execute(&mut *connection)
                        .await?;
                        let mut repository = PgPosWorkspaceRead::new(connection);
                        let current = pos_workspace(
                            &actor("company", Some("branch")),
                            &PosWorkspaceQuery::default(),
                            &PosWorkspaceEntitlements::default(),
                            &mut repository,
                            NOW,
                        )
                        .await?;
                        assert!(
                            current
                                .cash_movements
                                .iter()
                                .any(|row| row.id == "movement-uncommitted")
                        );
                        Err(BusinessError::new(
                            FailureKind::Unavailable,
                            "POS_TEST_LATE_FAILURE",
                            "Late failure after the owner read.",
                        )
                        .into())
                    })
                })
                .await;
            match result {
                Err(PgTransactionError::Business(error)) => {
                    assert_eq!(error.code, "POS_TEST_LATE_FAILURE")
                }
                other => panic!("expected the controlled late failure, got {other:?}"),
            }

            assert_eq!(
                count(&pool, "pos_cash_movements", "id='movement-uncommitted'").await,
                0
            );
            let readback = read(
                &pool,
                actor("company", Some("branch")),
                PosWorkspaceQuery::default(),
            )
            .await
            .unwrap();
            assert_eq!(readback.cash_movements.len(), 2);
            assert_eq!(readback.sales.len(), 2);
            assert_eq!(readback.cash_summary.approved_minor, 30_000);
        })
    });
}
