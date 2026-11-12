//! Native Commerce sales goods/mixed delivery on migrated PostgreSQL: the
//! composed fulfillment, Inventory stock issue, Finance delivery journal,
//! order-state advance, readback and exact replay on one serializable unit.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_commerce::public::order::CommerceOrderActor;
use erpx_server_commerce::public::receipt::{
    CommerceReceipt, GoodsValuation, GoodsValuationRequest,
};
use erpx_server_commerce::public::sales_delivery::{
    CommerceSalesDeliveryGoods, CommerceSalesDeliveryStock, PgCommerceSalesDelivery,
    PreparedSalesDelivery, SalesDeliveryAllocationCommand, SalesDeliveryCommand,
    SalesDeliveryDisposition, SalesDeliveryJournalRequest, SalesDeliveryLineCommand,
    SalesDeliveryMode, SalesDeliveryPlanAllocation, SalesDeliveryPlanLine,
    SalesDeliveryStockApplied, SalesDeliveryStockAppliedAllocation, SalesDeliveryStockAppliedLine,
    SalesDeliveryStockRequest, deliver_sales_order,
};
use erpx_server_finance::public::FinanceActor;
use erpx_server_finance::public::commerce_goods_journal::{
    CommerceGoodsJournalPosting, CommerceGoodsKind, CommerceGoodsValuation,
    CommerceGoodsValuationRequest, post_commerce_goods_journal, resolve_commerce_goods_valuation,
};
use erpx_server_inventory::public::stock_issue::{
    StockIssueAllocationInput, StockIssueAppliedReceipt, StockIssueInput, StockIssueLineInput,
    StockIssuePlan, StockIssueSource, apply_stock_issue, lock_stock_issue_sources,
    prepare_stock_issue, read_stock_issue_source, validate_stock_issue_plan,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, PgWorkFuture,
    TransactionOptions, load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    TimestampMillis, UserId,
};
use sqlx::PgConnection;
use std::sync::Arc;

const NOW: i64 = 1_800_000_000_000;

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(error) => panic!("unexpected persistence error: {error}"),
    }
}

fn code(error: PgTransactionError<BusinessError>) -> String {
    business(error).code.to_owned()
}

fn id(value: &str) -> RecordId {
    RecordId::new(value).unwrap()
}

#[derive(Clone, Copy)]
struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(NOW)
    }
}

/// Inventory public stock-issue seam bound exactly as composition would.
struct TestStock;

impl CommerceSalesDeliveryStock for TestStock {
    type Source = StockIssueSource;
    type Plan = StockIssuePlan;
    type Applied = StockIssueAppliedReceipt;

    fn lock<'a>(&'a mut self, c: &'a mut PgConnection) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move { lock_stock_issue_sources(c).await })
    }

    fn read<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        company: &'a str,
        branch: Option<&'a str>,
        department: Option<&'a str>,
        order_id: &'a str,
    ) -> PgWorkFuture<'a, Self::Source, BusinessError> {
        Box::pin(
            async move { read_stock_issue_source(c, company, branch, department, order_id).await },
        )
    }

    fn prepare<'a>(
        &'a mut self,
        _c: &'a mut PgConnection,
        source: &'a Self::Source,
        request: &'a SalesDeliveryStockRequest,
    ) -> PgWorkFuture<'a, PreparedSalesDelivery<Self::Plan>, BusinessError> {
        Box::pin(async move {
            let input = StockIssueInput {
                expected_order_version: request.expected_order_version,
                date: request.date.clone(),
                lines: request
                    .lines
                    .iter()
                    .map(|line| StockIssueLineInput {
                        line_id: line.line_id.clone(),
                        quantity_milli: line.quantity_milli,
                        allocations: line
                            .allocations
                            .iter()
                            .map(|allocation| StockIssueAllocationInput {
                                lot_id: allocation.lot_id.clone(),
                                quantity_milli: allocation.quantity_milli,
                            })
                            .collect(),
                    })
                    .collect(),
            };
            let plan = prepare_stock_issue(source, &input)?;
            let lines = plan
                .lines
                .iter()
                .map(|line| SalesDeliveryPlanLine {
                    line_id: line.line_id.clone(),
                    product_id: line.product_id.clone(),
                    tracking_mode: line.tracking_mode.clone(),
                    ordered_milli: line.ordered_milli,
                    quantity_milli: line.quantity_milli,
                    stock_value_minor: line.stock_value_minor,
                    expected_fulfilled_milli: line.expected_fulfilled_milli,
                    stock_version: line.expected_stock_version,
                    allocations: line
                        .allocations
                        .iter()
                        .map(|allocation| SalesDeliveryPlanAllocation {
                            lot_id: allocation.lot_id.clone(),
                            lot_code: allocation.lot_code.clone(),
                            serial_number: allocation.serial_number.clone(),
                            quantity_milli: allocation.quantity_milli,
                            value_minor: allocation.value_minor,
                        })
                        .collect(),
                })
                .collect();
            Ok(PreparedSalesDelivery {
                order_version: plan.order_version,
                warehouse_id: plan.warehouse_id.clone(),
                branch_id: plan.branch_id.clone(),
                total_value_minor: plan.total_value_minor,
                plan,
                lines,
            })
        })
    }

    fn apply<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a Self::Plan,
        fulfillment_id: &'a str,
        command_key: &'a str,
        actor: &'a str,
        now: i64,
    ) -> PgWorkFuture<'a, Self::Applied, BusinessError> {
        Box::pin(async move {
            apply_stock_issue(c, plan, fulfillment_id, command_key, actor, now).await
        })
    }

    fn applied_facts(&self, applied: &Self::Applied) -> SalesDeliveryStockApplied {
        SalesDeliveryStockApplied {
            fulfillment_id: applied.fulfillment_id.clone(),
            replayed: applied.replayed,
            lines: applied
                .lines
                .iter()
                .map(|line| SalesDeliveryStockAppliedLine {
                    line_id: line.line_id.clone(),
                    stock_version: line.stock_version,
                    quantity_milli: line.quantity_milli,
                    value_minor: line.value_minor,
                    allocations: line
                        .allocations
                        .iter()
                        .map(|allocation| SalesDeliveryStockAppliedAllocation {
                            lot_id: allocation.lot_id.clone(),
                            quantity_milli: allocation.quantity_milli,
                            value_minor: allocation.value_minor,
                            balance_version: allocation.balance_version,
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn require_applied<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a Self::Plan,
        applied: &'a Self::Applied,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            validate_stock_issue_plan(plan)?;
            if applied.fulfillment_id.is_empty() || applied.replayed {
                return Err(erpx_server_shared::public::BusinessError::new(
                    FailureKind::Conflict,
                    "STOCK_DELIVERY_EVIDENCE_INVALID",
                    "The Inventory delivery evidence is unavailable.",
                )
                .into());
            }
            for line in &applied.lines {
                let row: Option<(i64, i64)> = sqlx::query_as(
                    "SELECT quantity_milli,value_minor FROM commerce_stock_moves \
                     WHERE id=$1 AND fulfillment_id=$2",
                )
                .bind(&line.stock_move_id)
                .bind(&applied.fulfillment_id)
                .fetch_optional(&mut *c)
                .await?;
                if row != Some((-line.quantity_milli, -line.value_minor)) {
                    return Err(erpx_server_shared::public::BusinessError::new(
                        FailureKind::Conflict,
                        "STOCK_DELIVERY_EVIDENCE_INVALID",
                        "The Inventory delivery evidence changed.",
                    )
                    .into());
                }
            }
            Ok(())
        })
    }
}

/// Finance public goods-journal seam bound exactly as composition would.
struct TestGoods {
    clock: Arc<FixedClock>,
    resolved: Option<CommerceGoodsValuation>,
}

impl TestGoods {
    fn new() -> Self {
        Self {
            clock: Arc::new(FixedClock),
            resolved: None,
        }
    }
}

fn finance_actor(request: &GoodsValuationRequest) -> FinanceActor {
    FinanceActor {
        user_id: UserId::new(&request.user_id).unwrap(),
        holding_id: HoldingId::new(&request.holding_id).unwrap(),
        company_id: CompanyId::new(&request.company_id).unwrap(),
        assignment_id: AssignmentId::new(&request.assignment_id).unwrap(),
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "GOODS_FINANCE_VALUATION_REQUIRED",
        "The Finance goods valuation is unavailable.",
    )
}

impl CommerceSalesDeliveryGoods for TestGoods {
    fn lock<'a>(&'a mut self, _c: &'a mut PgConnection) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move { Ok(()) })
    }

    fn resolve<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        request: &'a GoodsValuationRequest,
    ) -> PgWorkFuture<'a, GoodsValuation, BusinessError> {
        Box::pin(async move {
            let resolved = resolve_commerce_goods_valuation(
                c,
                &CommerceGoodsValuationRequest {
                    actor: finance_actor(request),
                    chart_id: request.chart_id.clone(),
                    business_date: request.business_date.clone(),
                    document_currency: request.document_currency.clone(),
                },
            )
            .await?;
            let valuation = GoodsValuation {
                stock_currency: resolved.stock_currency().to_owned(),
                fx_rate_micros: resolved.fx_rate_micros(),
                source_rate_id: resolved.fx().source_rate_id.clone(),
                target_rate_id: resolved.fx().target_rate_id.clone(),
                used_latest_fallback: resolved.fx().used_latest_fallback,
            };
            self.resolved = Some(resolved);
            Ok(valuation)
        })
    }

    fn post<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        request: &'a SalesDeliveryJournalRequest,
        valuation: &'a GoodsValuation,
    ) -> PgWorkFuture<'a, String, BusinessError> {
        Box::pin(async move {
            if self.resolved.as_ref().is_none_or(|resolved| {
                resolved.stock_currency() != valuation.stock_currency
                    || resolved.fx_rate_micros() != valuation.fx_rate_micros
            }) {
                return Err(unavailable().into());
            }
            let actor = finance_actor(&GoodsValuationRequest {
                company_id: request.company_id.clone(),
                user_id: request.user_id.clone(),
                holding_id: request.holding_id.clone(),
                assignment_id: request.assignment_id.clone(),
                chart_id: request.chart_id.clone(),
                business_date: request.business_date.clone(),
                document_currency: request.currency.clone(),
            });
            Ok(post_commerce_goods_journal(
                c,
                self.clock.as_ref(),
                &CommerceGoodsJournalPosting {
                    journal_id: request.journal_id.clone(),
                    actor,
                    chart_id: request.chart_id.clone(),
                    business_date: request.business_date.clone(),
                    fulfillment_id: request.fulfillment_id.clone(),
                    memo: request.memo.clone(),
                    inventory_account_id: request.inventory_account_id.clone(),
                    kind: CommerceGoodsKind::Delivery {
                        cogs_account_id: request.cogs_account_id.clone(),
                    },
                    document_currency: request.currency.clone(),
                    document_subtotal_minor: request.amount_minor,
                    stock_value_minor: request.amount_minor,
                    fx_rate_micros: Some(request.fx_rate_micros),
                },
            )
            .await?)
        })
    }
}

fn seller() -> CommerceOrderActor {
    CommerceOrderActor {
        user_id: UserId::new("seller").unwrap(),
        person_user_ids: vec![],
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(id("branch")),
        department_id: None,
        assignment_id: AssignmentId::new("assignment-seller").unwrap(),
        role: Some("warehouse_manager".into()),
        authority_revision: Some(1),
        assignment_version: Some(1),
        permissions: vec!["commerce.read".into(), "inventory.prepare".into()],
        modules: vec!["inventory".into()],
        expires_at: 2_000_000_000_000,
    }
}

fn owner() -> CommerceOrderActor {
    CommerceOrderActor {
        user_id: UserId::new("owner").unwrap(),
        person_user_ids: vec![],
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(id("branch")),
        department_id: None,
        assignment_id: AssignmentId::new("assignment-owner").unwrap(),
        role: Some("holding_owner".into()),
        authority_revision: Some(1),
        assignment_version: Some(1),
        permissions: vec![
            "commerce.read".into(),
            "commerce.sales".into(),
            "inventory.prepare".into(),
        ],
        modules: vec!["sales".into()],
        expires_at: 2_000_000_000_000,
    }
}

fn lot_allocation(lot: &str, quantity: i64) -> SalesDeliveryAllocationCommand {
    SalesDeliveryAllocationCommand {
        lot_id: Some(lot.into()),
        lot_code: None,
        serial_number: None,
        quantity_milli: quantity,
        disposition: SalesDeliveryDisposition::Available,
        quarantine_reason: None,
        quarantine_due_date: None,
    }
}

fn goods_command(key: &str, version: i64) -> SalesDeliveryCommand {
    SalesDeliveryCommand {
        key: CommandKey::new(key).unwrap(),
        order_id: id("sales-1"),
        mode: SalesDeliveryMode::Goods,
        expected_version: version,
        date: "2026-11-05".into(),
        reason: None,
        lines: vec![
            SalesDeliveryLineCommand {
                line_id: "line-stock".into(),
                quantity_milli: 2_000,
                allocations: vec![],
            },
            SalesDeliveryLineCommand {
                line_id: "line-lot".into(),
                quantity_milli: 1_000,
                allocations: vec![lot_allocation("lot-1", 1_000)],
            },
        ],
    }
}

fn mixed_command(key: &str) -> SalesDeliveryCommand {
    SalesDeliveryCommand {
        key: CommandKey::new(key).unwrap(),
        order_id: id("sales-1"),
        mode: SalesDeliveryMode::Mixed,
        expected_version: 3,
        date: "2026-11-06".into(),
        reason: Some("Full delivery signed by the customer".into()),
        lines: vec![
            SalesDeliveryLineCommand {
                line_id: "line-stock".into(),
                quantity_milli: 5_000,
                allocations: vec![],
            },
            SalesDeliveryLineCommand {
                line_id: "line-lot".into(),
                quantity_milli: 2_000,
                allocations: vec![lot_allocation("lot-1", 2_000)],
            },
            SalesDeliveryLineCommand {
                line_id: "line-svc".into(),
                quantity_milli: 1_000,
                allocations: vec![],
            },
        ],
    }
}

fn run(
    pool: &DatabasePool,
    actor: CommerceOrderActor,
    command: SalesDeliveryCommand,
) -> impl std::future::Future<Output = Result<CommerceReceipt, PgTransactionError<BusinessError>>> {
    let pool = pool.clone();
    async move {
        PgUnitOfWork::new(pool)
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = actor.clone();
                let command = command.clone();
                Box::pin(async move {
                    let mut repository = PgCommerceSalesDelivery::new();
                    let mut stock = TestStock;
                    let mut goods = TestGoods::new();
                    deliver_sales_order(
                        &actor,
                        &command,
                        &mut repository,
                        &mut stock,
                        &mut goods,
                        connection,
                        TimestampMillis::new(NOW),
                    )
                    .await
                })
            })
            .await
    }
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch','company','company','branch','active',1);\
         INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
         ('company','seller','local:seller','active','local',1),\
         ('company','owner','local:owner','active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id,source) VALUES \
         ('assignment-seller','seller','warehouse_manager','[]','company','branch',0,9000000000000,NULL,'branch','branch','manual'),\
         ('assignment-owner','owner','holding_owner','[]','company','branch',0,9000000000000,NULL,'holding','company','manual');\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) VALUES \
         ('party-1','company','customer','C1','Customer','THB',30,0,'active',1,1,1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse-1','company','branch','WH1','Main warehouse','active',1);\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version,tracking_mode,item_type) VALUES \
         ('product-stock','company','STK-1','Stock item','each',1000,0,'active',1,'none','stock'),\
         ('product-lot','company','STK-L','Lot item','each',500,0,'active',1,'lot','stock'),\
         ('product-svc','company','SVC-1','Service','each',2000,700,'active',1,'none','service');\
         INSERT INTO commerce_orders\
         (id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,\
         order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES \
         ('sales-1','company','branch',NULL,'sales','SO-1','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','confirmed',8000,140,3,'creator',0,0);\
         INSERT INTO commerce_order_lines\
         (id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,\
         subtotal_minor,tax_minor,ordinal,item_type) VALUES \
         ('line-stock','sales-1','product-stock','Stock item',5000,0,1000,0,5000,0,0,'stock'),\
         ('line-lot','sales-1','product-lot','Lot item',2000,0,500,0,1000,0,1,'stock'),\
         ('line-svc','sales-1','product-svc','Service',1000,0,2000,700,2000,140,2,'service');\
         INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES \
         ('warehouse-1','product-stock',5000,1000),\
         ('warehouse-1','product-lot',5000,1000);\
         INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at) VALUES \
         ('lot-1','company','product-lot','lot','L1','creator',0),\
         ('lot-2','company','product-lot','lot','L2','creator',0);\
         INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) VALUES \
         ('lot-1','warehouse-1',3000,600),\
         ('lot-2','warehouse-1',2000,400);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::raw_sql(
        "INSERT INTO finance_charts(id,company_id,name,base_currency,status,created_by,created_at,updated_at) \
         VALUES('chart-1','company','Company chart','THB','active','creator',0,0);\
         INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,status,created_at,updated_at) VALUES \
         ('coa-1','chart-1','company','1000','Receivable','asset','THB','active',0,0),\
         ('coa-2','chart-1','company','2000','Payable','liability','THB','active',0,0),\
         ('coa-3','chart-1','company','4000','Revenue','revenue','THB','active',0,0),\
         ('coa-4','chart-1','company','1200','Inventory','asset','THB','active',0,0),\
         ('coa-5','chart-1','company','2100','Clearing','liability','THB','active',0,0),\
         ('coa-6','chart-1','company','5000','Cost of sales','expense','THB','active',0,0),\
         ('coa-7','chart-1','company','1300','Input tax','asset','THB','active',0,0),\
         ('coa-8','chart-1','company','2200','Output tax','liability','THB','active',0,0),\
         ('coa-9','chart-1','company','1000-C','Cash','asset','THB','active',0,0);\
         INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,\
         revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,\
         output_tax_account_id,cash_account_id) VALUES('company','chart-1','THB','coa-1','coa-2','coa-3',\
         'coa-4','coa-5','coa-6','coa-7','coa-8','coa-9');\
         INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,close_version,created_at) VALUES \
         ('period-2026-11','company','November 2026','2026-11-01','2026-11-30','open',0,0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn goods_delivery_posts_stock_journal_order_state_and_exact_replay() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let receipt = run(&pool, seller(), goods_command("delivery-goods-one", 3))
            .await
            .unwrap();
        assert_eq!(receipt.order_id, "sales-1");
        assert_eq!(receipt.fulfillment_id, "delivery-delivery-goods-one");
        assert_eq!(receipt.state, "partially_fulfilled");
        assert_eq!(receipt.version, 4);
        assert_eq!(receipt.total_value_minor, 600);
        assert_eq!(
            receipt
                .lines
                .iter()
                .map(|line| (line.line_id.as_str(), line.quantity_milli, line.value_minor))
                .collect::<Vec<_>>(),
            vec![("line-stock", 2_000, 400), ("line-lot", 1_000, 200)]
        );

        let journal: (String, Option<i64>, String, String, String) = sqlx::query_as(
            "SELECT currency,fx_rate_micros,status,source_type,source_id \
             FROM finance_journals WHERE id='journal-delivery-goods-one' AND company_id='company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            journal,
            (
                "THB".into(),
                Some(1_000_000),
                "draft".into(),
                "delivery".into(),
                "delivery-delivery-goods-one".into()
            )
        );
        let lines: Vec<(String, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor \
             FROM finance_journal_lines WHERE journal_id='journal-delivery-goods-one' ORDER BY ordinal",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            lines,
            vec![
                ("coa-6".into(), 600, 0, 600, 0),
                ("coa-4".into(), 0, 600, 0, 600),
            ]
        );

        let link: String = sqlx::query_scalar(
            "SELECT journal_id FROM commerce_fulfillments WHERE id='delivery-delivery-goods-one'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(link, "journal-delivery-goods-one");
        let valuation: (String, String, String, String, i64, i64, i64, String) = sqlx::query_as(
            "SELECT document_currency,journal_currency,stock_currency,chart_id,fx_rate_micros,\
             document_subtotal_minor,stock_value_minor,valuation_date \
             FROM commerce_goods_valuations WHERE fulfillment_id='delivery-delivery-goods-one'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            valuation,
            (
                "THB".into(),
                "THB".into(),
                "THB".into(),
                "chart-1".into(),
                1_000_000,
                2_500,
                600,
                "2026-11-05".into()
            )
        );

        let stock: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT product_id,quantity_milli,value_minor FROM commerce_stock \
             WHERE warehouse_id='warehouse-1' ORDER BY product_id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            stock,
            vec![
                ("product-lot".into(), 4_000, 800),
                ("product-stock".into(), 3_000, 600),
            ]
        );
        let lot: (i64, i64) = sqlx::query_as(
            "SELECT quantity_milli,value_minor FROM inventory_lot_balances \
             WHERE lot_id='lot-1' AND warehouse_id='warehouse-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(lot, (2_000, 400));
        let allocations: Vec<(String, i64, i64, String)> = sqlx::query_as(
            "SELECT lot_id,quantity_milli,value_minor,disposition \
             FROM inventory_fulfillment_lot_allocations \
             WHERE fulfillment_line_id='delivery-delivery-goods-one-1'",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(allocations, vec![("lot-1".into(), 1_000, 200, "available".into())]);

        let moves: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT id,quantity_milli,value_minor FROM commerce_stock_moves \
             WHERE fulfillment_id='delivery-delivery-goods-one' ORDER BY id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            moves,
            vec![
                ("delivery-delivery-goods-one-0".into(), -2_000, -400),
                ("delivery-delivery-goods-one-1".into(), -1_000, -200),
            ]
        );
        let order: (String, i64) =
            sqlx::query_as("SELECT state,version FROM commerce_orders WHERE id='sales-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(order, ("partially_fulfilled".into(), 4));
        let fulfilled: Vec<(String, i64)> = sqlx::query_as(
            "SELECT id,fulfilled_milli FROM commerce_order_lines WHERE order_id='sales-1' ORDER BY ordinal",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            fulfilled,
            vec![
                ("line-stock".into(), 2_000),
                ("line-lot".into(), 1_000),
                ("line-svc".into(), 0),
            ]
        );
        let effects: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id='delivery-goods-one'),\
             (SELECT count(*) FROM audit_events WHERE command_id='delivery-goods-one'),\
             (SELECT count(*) FROM outbox_events WHERE command_id='delivery-goods-one')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(effects, (1, 1, 1));
        let result_json: String = sqlx::query_scalar(
            "SELECT result_json FROM command_receipts WHERE command_id='delivery-goods-one'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&result_json).unwrap(),
            serde_json::json!({
                "orderId": "sales-1",
                "fulfillmentId": "delivery-delivery-goods-one",
                "state": "partially_fulfilled",
                "version": 4,
                "totalValueMinor": 600,
                "lines": [
                    {"lineId": "line-stock", "quantityMilli": 2000, "valueMinor": 400},
                    {"lineId": "line-lot", "quantityMilli": 1000, "valueMinor": 200},
                ],
            })
        );

        let replay = run(&pool, seller(), goods_command("delivery-goods-one", 3))
            .await
            .unwrap();
        assert_eq!(replay, receipt);
        let counts: (i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM commerce_fulfillments WHERE order_id='sales-1'),\
             (SELECT count(*) FROM finance_journals WHERE source_type='delivery'),\
             (SELECT count(*) FROM commerce_goods_valuations),\
             (SELECT count(*) FROM command_receipts WHERE command_id='delivery-goods-one'),\
             (SELECT quantity_milli FROM commerce_stock WHERE product_id='product-stock' AND warehouse_id='warehouse-1')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1, 1, 3_000));

        let mut changed = goods_command("delivery-goods-one", 3);
        changed.lines[0].quantity_milli = 1_000;
        assert_eq!(
            code(run(&pool, seller(), changed).await.unwrap_err()),
            "IDEMPOTENCY_CONFLICT"
        );
        let quantity: i64 = sqlx::query_scalar(
            "SELECT quantity_milli FROM commerce_stock WHERE product_id='product-stock' AND warehouse_id='warehouse-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(quantity, 3_000);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn mixed_delivery_completes_service_lines_without_stock_moves() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let receipt = run(&pool, owner(), mixed_command("delivery-mixed-one"))
            .await
            .unwrap();
        assert_eq!(receipt.state, "fulfilled");
        assert_eq!(receipt.version, 4);
        assert_eq!(receipt.total_value_minor, 1_400);
        assert_eq!(
            receipt
                .lines
                .iter()
                .map(|line| (line.line_id.as_str(), line.value_minor))
                .collect::<Vec<_>>(),
            vec![("line-stock", 1_000), ("line-lot", 400), ("line-svc", 0)]
        );
        let fulfillment: (String, String, String) = sqlx::query_as(
            "SELECT completion_kind,completion_note,journal_id FROM commerce_fulfillments \
             WHERE id='delivery-delivery-mixed-one'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            fulfillment,
            (
                "mixed".into(),
                "Full delivery signed by the customer".into(),
                "journal-delivery-mixed-one".into()
            )
        );
        let line_currencies: Vec<(String, String, i64, i64, i64)> = sqlx::query_as(
            "SELECT id,stock_currency,value_minor,subtotal_minor,tax_minor \
             FROM commerce_fulfillment_lines WHERE fulfillment_id='delivery-delivery-mixed-one' \
             ORDER BY id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            line_currencies,
            vec![
                (
                    "delivery-delivery-mixed-one-0".into(),
                    "THB".into(),
                    1_000,
                    5_000,
                    0
                ),
                (
                    "delivery-delivery-mixed-one-1".into(),
                    "THB".into(),
                    400,
                    1_000,
                    0
                ),
                (
                    "delivery-delivery-mixed-one-2".into(),
                    "THB".into(),
                    0,
                    2_000,
                    140
                ),
            ]
        );
        let moves: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT id,quantity_milli,value_minor FROM commerce_stock_moves \
             WHERE fulfillment_id='delivery-delivery-mixed-one' ORDER BY id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            moves,
            vec![
                ("delivery-delivery-mixed-one-0".into(), -5_000, -1_000),
                ("delivery-delivery-mixed-one-1".into(), -2_000, -400),
            ]
        );
        let valuation: (i64, i64) = sqlx::query_as(
            "SELECT document_subtotal_minor,stock_value_minor FROM commerce_goods_valuations \
             WHERE fulfillment_id='delivery-delivery-mixed-one'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(valuation, (8_000, 1_400));
        let journal: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines \
             WHERE journal_id='journal-delivery-mixed-one' ORDER BY ordinal",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            journal,
            vec![("coa-6".into(), 1_400, 0), ("coa-4".into(), 0, 1_400)]
        );
        let order: (String, i64) =
            sqlx::query_as("SELECT state,version FROM commerce_orders WHERE id='sales-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(order, ("fulfilled".into(), 4));
        let fulfilled: Vec<(String, i64)> = sqlx::query_as(
            "SELECT id,fulfilled_milli FROM commerce_order_lines WHERE order_id='sales-1' ORDER BY ordinal",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            fulfilled,
            vec![
                ("line-stock".into(), 5_000),
                ("line-lot".into(), 2_000),
                ("line-svc".into(), 1_000),
            ]
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn delivery_requires_current_iam_scope_and_version() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;

        let mut no_inventory = seller();
        no_inventory.permissions = vec!["commerce.read".into()];
        assert_eq!(
            code(run(&pool, no_inventory, goods_command("delivery-denied-inventory", 3))
                .await
                .unwrap_err()),
            "COMMERCE_DELIVERY_PERMISSION_REQUIRED"
        );
        let mut department = seller();
        department.department_id = Some(id("department-1"));
        assert_eq!(
            code(run(&pool, department, goods_command("delivery-denied-department", 3))
                .await
                .unwrap_err()),
            "COMMERCE_DELIVERY_PERMISSION_REQUIRED"
        );
        // A mixed delivery additionally requires the Sales preparation
        // permission, which the warehouse manager role does not carry.
        assert_eq!(
            code(run(&pool, seller(), mixed_command("delivery-denied-mixed"))
                .await
                .unwrap_err()),
            "COMMERCE_DELIVERY_PERMISSION_REQUIRED"
        );
        // Stale expected version fails closed before any stock or Finance write.
        assert_eq!(
            code(run(&pool, seller(), goods_command("delivery-stale", 2))
                .await
                .unwrap_err()),
            "COMMERCE_ORDER_CHANGED"
        );
        let effects: (i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM commerce_fulfillments),\
             (SELECT count(*) FROM commerce_stock_moves),\
             (SELECT count(*) FROM finance_journals),\
             (SELECT count(*) FROM command_receipts),\
             (SELECT quantity_milli FROM commerce_stock WHERE product_id='product-stock' AND warehouse_id='warehouse-1')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(effects, (0, 0, 0, 0, 5_000));
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM commerce_orders WHERE id='sales-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "confirmed"
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn quarantined_or_unallocated_lot_delivery_rolls_back_the_unit() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        // A quality hold on the lot makes the whole delivery fail before any
        // fulfillment or stock row is written.
        sqlx::query(
            "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,\
             reason,due_date,created_by,created_at,updated_at,quantity_scope,lot_id,\
             held_quantity_milli) VALUES('hold-1','company','branch','warehouse-1','product-lot',\
             'Hold','2026-12-31','maker',0,0,'partial','lot-1',500)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let error = run(&pool, seller(), goods_command("delivery-held-lot", 3))
            .await
            .unwrap_err();
        assert_eq!(code(error), "LOT_UNAVAILABLE");
        let effects: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM commerce_fulfillments),\
             (SELECT count(*) FROM commerce_stock_moves),\
             (SELECT count(*) FROM finance_journals),\
             (SELECT count(*) FROM command_receipts)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(effects, (0, 0, 0, 0));
        let stock: i64 = sqlx::query_scalar(
            "SELECT value_minor FROM commerce_stock WHERE product_id='product-lot' AND warehouse_id='warehouse-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(stock, 1_000);
    })
    .await;
}
