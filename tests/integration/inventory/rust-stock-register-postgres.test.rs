//! Actual Inventory stock-register reads over leased PostgreSQL. These cases
//! prove source scope, lot/count visibility and reconciliation only; they do
//! not prove IAM, Commercial purchase or an HTTP session.
#[path = "../../support/inventory/stock-register-facts.rs"]
mod facts;
#[path = "../../support/inventory/reservation-source-fence.rs"]
mod fence;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_inventory::public::stock_register::{
    lock_stock_register_sources, read_stock_register,
};
use erpx_server_runtime::public::postgres::{PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::BusinessError;

type Error = erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>;

#[tokio::test]
async fn register_reads_company_scope_lots_counts_and_reconciles_tracked_stock() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_register_sources(c).await?;
                    read_stock_register(c, "company", None).await
                })
            })
            .await
            .unwrap();
        assert_eq!(result.currency, "THB");
        assert_eq!(result.stock.len(), 3);
        assert_eq!(result.lots.len(), 3);
        assert_eq!(result.counts.len(), 3);
        let main = result
            .stock
            .iter()
            .find(|row| row.warehouse_id == "warehouse" && row.product_id == "product")
            .unwrap();
        assert_eq!(main.quantity_milli, 3_000);
        assert_eq!(main.reserved_milli, 500);
        assert_eq!(main.quarantined_milli, 1_000);
        assert_eq!(main.available_milli, 2_000);
        assert_eq!(main.tracking_mode, "lot");
        assert_eq!(main.version, 3);
        let untracked = result
            .stock
            .iter()
            .find(|row| row.product_id == "untracked")
            .unwrap();
        assert_eq!(untracked.available_milli, 7_000);
        let held = result.lots.iter().find(|lot| lot.id == "lot-b").unwrap();
        assert_eq!(held.quality_hold_milli, 1_000);
        assert_eq!(held.quality_status.as_deref(), Some("held"));
        assert_eq!(held.quantity_milli, 1_000);
        assert_eq!(held.version, 1);
        let initial = result
            .counts
            .iter()
            .find(|count| count.id == "count-initial-command")
            .unwrap();
        assert!(initial.initial_physical_source);
        assert_eq!(initial.status, "posted");
        assert_eq!(initial.version, 4);
        let draft = result
            .counts
            .iter()
            .find(|count| count.id == "count-draft")
            .unwrap();
        assert!(!draft.initial_physical_source);
        assert_eq!(draft.version, 2);
        assert_eq!(result.totals.tracked_quantity_milli, 7_000);
        assert_eq!(result.totals.tracked_value_minor, 7_000);
        assert_eq!(result.totals.lot_quantity_milli, 7_000);
        assert_eq!(result.totals.lot_value_minor, 7_000);
        assert!(result.totals.consistent);
    })
    .await;
}

#[tokio::test]
async fn register_preserves_branch_and_company_isolation() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        let scoped = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_register_sources(c).await?;
                    read_stock_register(c, "company", Some("branch-two")).await
                })
            })
            .await
            .unwrap();
        assert_eq!(scoped.stock.len(), 1);
        assert_eq!(scoped.stock[0].warehouse_id, "warehouse-two");
        assert!(
            scoped
                .lots
                .iter()
                .all(|lot| lot.warehouse_id == "warehouse-two")
        );
        assert_eq!(scoped.counts.len(), 1);
        assert_eq!(scoped.counts[0].id, "count-two");
        let foreign = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_register_sources(c).await?;
                    read_stock_register(c, "another-company", None).await
                })
            })
            .await
            .unwrap();
        assert!(foreign.stock.is_empty());
        assert!(foreign.lots.is_empty());
        assert!(foreign.counts.is_empty());
        assert_eq!(foreign.currency, "THB");
    })
    .await;
}

#[tokio::test]
async fn register_includes_field_and_manufacturing_reservations() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,status,version,created_by,created_at,updated_at)\
             VALUES('field-ticket','company','branch','reserve-order','reserve-party','Field request','Issue','maker','2026-11-10','open',1,'maker',0,0);\
             INSERT INTO field_service_agreements(id,company_id,branch_id,customer_id,title,start_date,end_date,currency,visit_rate_minor,status,created_by,version,created_at,updated_at)\
             VALUES('field-agreement','company','branch','reserve-party','Field agreement','2026-01-01','2027-01-01','THB',0,'draft','maker',1,0,0);\
             INSERT INTO field_service_orders(id,company_id,branch_id,ticket_id,ticket_version,agreement_id,agreement_version,issue,site,technician_id,status,version,created_by,created_at,updated_at)\
             VALUES('field-order','company','branch','field-ticket',1,'field-agreement',1,'Issue','Site','maker','open',1,'maker',0,0);\
             INSERT INTO field_service_part_reservations(id,order_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,reason,created_at)\
             VALUES('field-reservation','field-order','warehouse','untracked',250,0,0,9007199254740991,'maker','Field reserve',0);\
             INSERT INTO manufacturing_specifications(id,company_id,name,mode,output_product_id,batch_quantity_milli,expected_yield_bps,status,created_by,version,created_at,updated_at)\
             VALUES('mfg-spec','company','Specification','bom','untracked',1000,10000,'approved','maker',1,0,0);\
             INSERT INTO planning_scenarios(id,company_id,branch_id,warehouse_id,specification_id,specification_version,demand_quantity_milli,due_date,capacity_minutes,shortages_json,status,created_by,version,created_at,updated_at)\
             VALUES('mfg-scenario','company','branch','warehouse','mfg-spec',1,1000,'2026-11-30',0,'[]','calculated','maker',1,0,0);\
             INSERT INTO planning_recommendations(id,scenario_id,scenario_version,recommended_quantity_milli,required_minutes,feasible_date,status,created_by,version,created_at,updated_at)\
             VALUES('mfg-recommendation','mfg-scenario',1,1000,0,'2026-11-30','proposed','maker',1,0,0);\
             INSERT INTO manufacturing_orders(id,company_id,branch_id,warehouse_id,recommendation_id,specification_id,specification_version,planned_quantity_milli,status,created_by,version,created_at,updated_at)\
             VALUES('mfg-order','company','branch','warehouse','mfg-recommendation','mfg-spec',1,1000,'reserved','maker',1,0,0);\
             INSERT INTO manufacturing_material_reservations(id,order_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,created_at)\
             VALUES('mfg-reservation','mfg-order','warehouse','untracked',250,0,0,9007199254740991,'maker',0);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let register = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_register_sources(c).await?;
                    read_stock_register(c, "company", None).await
                })
            })
            .await
            .unwrap();
        let row = register
            .stock
            .iter()
            .find(|row| row.warehouse_id == "warehouse" && row.product_id == "untracked")
            .unwrap();
        assert_eq!(row.quantity_milli, 7_000);
        assert_eq!(row.reserved_milli, 500);
        assert_eq!(row.available_milli, 6_500);
    })
    .await;
}

#[tokio::test]
async fn register_fences_every_reservation_source_behind_the_view() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        let mut reader = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *reader)
            .await
            .unwrap();
        lock_stock_register_sources(&mut reader).await.unwrap();
        fence::assert_reservation_source_writes_wait(&pool).await;
        reader.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn register_requires_a_serializable_owner_transaction() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        let result = read_stock_register(&mut c, "company", None).await;
        assert!(matches!(
            result,
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(_))
        ));
    })
    .await;
}

#[tokio::test]
async fn register_rolls_back_with_the_enclosing_unit() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        let result: Result<(), Error> = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_register_sources(c).await?;
                    let register = read_stock_register(c, "company", None).await?;
                    assert_eq!(register.stock.len(), 3);
                    sqlx::query("UPDATE commerce_stock SET quantity_milli=quantity_milli+1 WHERE warehouse_id='warehouse' AND product_id='product'")
                        .execute(&mut *c)
                        .await?;
                    Err(BusinessError::new(
                        erpx_server_shared::public::FailureKind::Conflict,
                        "FIXTURE_ABORT",
                        "Rollback after verifying the register read.",
                    )
                    .into())
                })
            })
            .await;
        assert!(matches!(
            result,
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(_))
        ));
        let quantity: i64 = sqlx::query_scalar(
            "SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(quantity, 3_000);
    })
    .await;
}

mod transfer_landed_reorder_pg {
    //! Native command effects over the leased Holding database. These cases
    //! replace the retired TypeScript transfer/landed/reorder writer oracles:
    //! current-version guards, stock/lot valuation, immutable evidence and
    //! independent reorder approval are checked against actual owner rows.
    use super::fixture;
    use erpx_server_inventory::public::landed_costs::{
        LandedCostAction, LandedCostActionInput, LandedCostCreateInput, apply_landed_cost_create,
        apply_landed_cost_transition, landed_cost_credit_account, lock_landed_cost_sources,
        plan_create, plan_transition as plan_landed_transition, read_landed_cost_create_source,
        read_landed_cost_transition_source,
    };
    use erpx_server_inventory::public::reorder_policies::{
        ReorderAction, ReorderActionInput, ReorderSaveInput, apply_reorder_save,
        apply_reorder_transition, lock_reorder_sources, plan_save as plan_reorder_save,
        plan_transition as plan_reorder_transition, read_reorder_save_source,
        read_reorder_transition_source,
    };
    use erpx_server_inventory::public::transfers::{
        TransferAction, TransferActionInput, TransferLineInput, TransferLotAllocationInput,
        TransferSaveInput, apply_transfer_save, apply_transfer_transition, lock_transfer_sources,
        plan_save, plan_transition, read_transfer_save_source, read_transfer_transition_source,
    };
    use erpx_server_runtime::public::postgres::{PgUnitOfWork, TransactionOptions};
    use erpx_server_shared::public::BusinessError;
    use sqlx::Row;

    type Error = erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>;

    fn transfer_input(product: &str, quantity: &str) -> TransferSaveInput {
        TransferSaveInput {
            source_warehouse_id: "warehouse".into(),
            target_warehouse_id: "warehouse-two".into(),
            transfer_date: "2026-11-13".into(),
            delivery_mode: None,
            arrival_due_date: None,
            reason: "Move replenishment stock".into(),
            expected_version: None,
            lines: vec![TransferLineInput {
                product_id: product.into(),
                quantity: quantity.into(),
                allocations: Vec::new(),
            }],
        }
    }

    #[tokio::test]
    async fn transfer_draft_submit_and_direct_post_move_stock_once() {
        fixture::with_database(2, |pool| async move {
            super::facts::setup(&pool).await;
            sqlx::query("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,created_at)VALUES('period-transfer','company','FY26','2026-01-01','2026-12-31','open',0)").execute(pool.raw()).await.unwrap();
            let pool_for_save = pool.clone();
            PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                    Box::pin(async move {
                        lock_transfer_sources(c).await?;
                        let input = transfer_input("untracked", "2");
                        let source =
                            read_transfer_save_source(c, "company", None, None, &input).await?;
                        let plan = plan_save(
                            &source,
                            &input,
                            "maker",
                            "company",
                            "transfer-command-1",
                            "TR-COMMAND1",
                        )?;
                        apply_transfer_save(c, &plan, "key-save", "maker", 100, false).await?;
                        Ok(())
                    })
                })
                .await
                .unwrap();
            sqlx::raw_sql("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json)VALUES('key-post','inventory.transfer.post','reviewer','hash','transfer-command-1','{}',0,'{\"companyId\":\"company\"}')").execute(pool.raw()).await.unwrap();
            for (action, actor, version, at) in [
                (TransferAction::Submit, "maker", 1_i64, 101_i64),
                (TransferAction::Post, "reviewer", 2, 102),
            ] {
                let pool = pool_for_save.clone();
                PgUnitOfWork::new(pool_for_save.clone())
                    .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                        Box::pin(async move {
                            lock_transfer_sources(c).await?;
                            let input = TransferActionInput {
                                expected_version: version,
                                reason: Some("Independent receipt review".into()),
                                attachment_set_ids: Vec::new(),
                            };
                            let source = read_transfer_transition_source(
                                c,
                                "company",
                                None,
                                "transfer-command-1",
                                action,
                            )
                            .await?;
                            let plan = plan_transition(&source, &input, action, actor)?;
                            apply_transfer_transition(
                                c,
                                &plan,
                                &format!("key-{}", action.as_str()),
                                actor,
                                at,
                            )
                            .await?;
                            Ok(())
                        })
                    })
                    .await
                    .unwrap();
                let _ = pool;
            }
            let source: (i64, i64) = sqlx::query_as(
                "SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='untracked'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(source, (5_000, 5_000));
            let target: (i64, i64) = sqlx::query_as(
                "SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id='warehouse-two' AND product_id='untracked'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(target, (2_000, 2_000));
            let status: (String, i64) = sqlx::query_as(
                "SELECT status,version FROM inventory_transfers WHERE id='transfer-command-1'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(status, ("posted".into(), 3));
            let moves: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM inventory_transfer_moves WHERE transfer_id='transfer-command-1'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(moves, 2);
            let decisions: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM inventory_transfer_decisions WHERE transfer_id='transfer-command-1'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(decisions, 1);
        })
        .await;
    }

    #[tokio::test]
    async fn transfer_transit_post_creates_native_transit_without_moving_stock() {
        fixture::with_database(2, |pool| async move {
            super::facts::setup(&pool).await;
            PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                    Box::pin(async move {
                        lock_transfer_sources(c).await?;
                        let mut input = transfer_input("product", "1");
                        input.delivery_mode = Some("transit".into());
                        input.arrival_due_date = Some("2026-11-20".into());
                        input.lines[0].allocations = vec![TransferLotAllocationInput {
                            lot_id: "lot-a".into(),
                            quantity: "1".into(),
                        }];
                        let source =
                            read_transfer_save_source(c, "company", None, None, &input).await?;
                        let plan = plan_save(
                            &source,
                            &input,
                            "maker",
                            "company",
                            "transfer-transit",
                            "TR-TRANSIT",
                        )?;
                        apply_transfer_save(c, &plan, "key-save", "maker", 100, false).await?;
                        Ok(())
                    })
                })
                .await
                .unwrap();
            sqlx::raw_sql("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json)VALUES('key-post','inventory.transfer.post','reviewer','hash','transfer-transit','{}',0,'{\"companyId\":\"company\"}')").execute(pool.raw()).await.unwrap();
            for (action, actor, version) in [
                (TransferAction::Submit, "maker", 1_i64),
                (TransferAction::Post, "reviewer", 2),
            ] {
                PgUnitOfWork::new(pool.clone())
                    .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                        Box::pin(async move {
                            lock_transfer_sources(c).await?;
                            let action_input = TransferActionInput {
                                expected_version: version,
                                reason: Some("Approve carrier transport".into()),
                                attachment_set_ids: Vec::new(),
                            };
                            let source = read_transfer_transition_source(
                                c,
                                "company",
                                None,
                                "transfer-transit",
                                action,
                            )
                            .await?;
                            let plan = plan_transition(&source, &action_input, action, actor)?;
                            apply_transfer_transition(
                                c,
                                &plan,
                                &format!("key-{}", action.as_str()),
                                actor,
                                101,
                            )
                            .await?;
                            Ok(())
                        })
                    })
                    .await
                    .unwrap();
            }
            let transit: (String, String, i64) = sqlx::query_as(
                "SELECT id,status,version FROM inventory_transit WHERE transfer_id='transfer-transit'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(transit, ("transit-transfer-transit".into(), "awaiting_dispatch".into(), 1));
            let lots: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM inventory_transit_lots WHERE id='transit-transfer-transit-0-0'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(lots, 1);
            let source: (i64, i64) = sqlx::query_as(
                "SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(source, (3_000, 3_000));
        })
        .await;
    }

    async fn seed_purchase_receipt(pool: &erpx_server_runtime::public::postgres::DatabasePool) {
        let mut c = pool.raw().begin().await.unwrap();
        sqlx::raw_sql(
            "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('supplier','company','supplier','SUP-1','Supplier','THB',0,0);\
             INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,created_at)VALUES('period-landed','company','FY26','2026-01-01','2026-12-31','open',0);\
             INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,memo,status,version,created_by,created_at,updated_at)VALUES\
               ('receipt-journal','company','chart','period-landed','JV-1','2026-11-01','THB','purchase_receipt','Receipt', 'posted',1,'maker',0,0),\
               ('landed-journal','company','chart','period-landed','JV-2','2026-11-02','THB','landed_cost','Landed cost','posted',1,'approver',0,0);\
             INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at)VALUES\
               ('purchase-order','company','branch','purchase','PO-1','supplier','warehouse','THB','2026-11-01','2026-11-02','confirmed',1000,0,1,'maker',0,0);\
             INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal)VALUES\
               ('purchase-line','purchase-order','product','Received product',1000,1000,1000,0,1000,0,1);\
             INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,journal_id,version)VALUES\
               ('receipt','purchase-order','GR-1','2026-11-01','maker',0,'receipt-journal',1);\
             INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor)VALUES\
               ('receipt-line','receipt','purchase-line',1000,1000,1000,0);\
             INSERT INTO inventory_fulfillment_lot_allocations(id,fulfillment_line_id,lot_id,quantity_milli,value_minor,disposition)VALUES\
               ('receipt-lot','receipt-line','lot-a',1000,1000,'available');",
        )
        .execute(&mut *c)
        .await
        .unwrap();
        c.commit().await.unwrap();
    }

    #[tokio::test]
    async fn landed_cost_create_approve_and_post_revalue_stock_and_lots() {
        fixture::with_database(2, |pool| async move {
            super::facts::setup(&pool).await;
            seed_purchase_receipt(&pool).await;
            PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                    Box::pin(async move {
                        lock_landed_cost_sources(c).await?;
                        let source =
                            read_landed_cost_create_source(c, "company", None, "receipt").await?;
                        let credit = landed_cost_credit_account(
                            c,
                            "company",
                            &source.receipt.chart_id,
                            "variance-account",
                        )
                        .await?;
                        let source =
                            erpx_server_inventory::public::landed_costs::LandedCostCreateSource {
                                credit_account_exists: credit,
                                ..source
                            };
                        let input = LandedCostCreateInput {
                            fulfillment_id: "receipt".into(),
                            cost_date: "2026-11-02".into(),
                            description: "Ocean freight and duty".into(),
                            allocation_method: "receipt_value".into(),
                            amount: "3.00".into(),
                            credit_account_id: "variance-account".into(),
                        };
                        let plan = plan_create(
                            &source,
                            &input,
                            "company",
                            "preparer",
                            "landed-command",
                            "LC-COMMAND",
                        )?;
                        apply_landed_cost_create(c, &plan, "key-create", "preparer", 100).await?;
                        Ok(())
                    })
                })
                .await
                .unwrap();
            for (action, version, journal) in [
                (LandedCostAction::Approve, 1_i64, None),
                (LandedCostAction::Post, 2, Some("landed-journal")),
            ] {
                PgUnitOfWork::new(pool.clone())
                    .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                        Box::pin(async move {
                            lock_landed_cost_sources(c).await?;
                            let source =
                                read_landed_cost_transition_source(c, "company", None, "landed-command")
                                    .await?;
                            let period = if source.periods.len() == 1 {
                                Some(source.periods[0].clone())
                            } else {
                                None
                            };
                            let input = LandedCostActionInput {
                                expected_version: version,
                                reason: "Independent freight review".into(),
                            };
                            let plan = plan_landed_transition(
                                &source,
                                &input,
                                action,
                                "approver",
                                None,
                                period,
                            )?;
                            apply_landed_cost_transition(
                                c,
                                &plan,
                                &format!("key-{}", action.as_str()),
                                "approver",
                                101,
                                journal,
                            )
                            .await?;
                            Ok(())
                        })
                    })
                    .await
                    .unwrap();
            }
            let stock: (i64, i64) = sqlx::query_as(
                "SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(stock, (3_000, 3_300));
            let lot: i64 = sqlx::query_scalar(
                "SELECT value_minor FROM inventory_lot_balances WHERE lot_id='lot-a' AND warehouse_id='warehouse'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(lot, 2_300);
            let moves: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(value_minor)::bigint,0) FROM inventory_landed_cost_moves WHERE landed_cost_id='landed-command'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(moves, 300);
            let cost: (String, i64, Option<String>) = sqlx::query_as(
                "SELECT status,version,journal_id FROM inventory_landed_costs WHERE id='landed-command'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(cost, ("posted".into(), 3, Some("landed-journal".into())));
            let events: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM inventory_landed_cost_events WHERE landed_cost_id='landed-command'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(events, 3);
            let immutable = sqlx::query(
                "UPDATE inventory_landed_cost_lots SET allocated_minor=0 WHERE landed_cost_line_id='landed-command-0'",
            )
            .execute(pool.raw())
            .await;
            assert!(immutable.is_err());
        })
        .await;
    }

    #[tokio::test]
    async fn reorder_proposal_requires_independent_approval_and_retirement() {
        fixture::with_database(2, |pool| async move {
            super::facts::setup(&pool).await;
            PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                    Box::pin(async move {
                        lock_reorder_sources(c).await?;
                        let input = ReorderSaveInput {
                            warehouse_id: "warehouse".into(),
                            product_id: "product".into(),
                            reorder_point: "3".into(),
                            reason: "Replenishment threshold".into(),
                            expected_version: None,
                        };
                        let source = read_reorder_save_source(
                            c,
                            "company",
                            None,
                            "reorder-command",
                            "warehouse",
                            "product",
                        )
                        .await?;
                        let plan =
                            plan_reorder_save(&source, &input, "company", "reorder-command")?;
                        apply_reorder_save(c, &plan, "key-propose", "preparer", 100).await?;
                        Ok(())
                    })
                })
                .await
                .unwrap();
            for (action, actor, reason, version) in [
                (ReorderAction::Approve, "approver", "Independent threshold review", 2_i64),
                (ReorderAction::Retire, "second-approver", "Retire the active threshold", 3),
            ] {
                PgUnitOfWork::new(pool.clone())
                    .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                        Box::pin(async move {
                            lock_reorder_sources(c).await?;
                            let source =
                                read_reorder_transition_source(c, "company", None, "reorder-command")
                                    .await?;
                            let action_input = ReorderActionInput {
                                action,
                                expected_version: version - 1,
                                reason: reason.into(),
                            };
                            let plan = plan_reorder_transition(
                                &source,
                                &action_input,
                                actor,
                                None,
                                true,
                                true,
                            )?;
                            apply_reorder_transition(
                                c,
                                &plan,
                                &format!("key-{}", action.as_str()),
                                actor,
                                101,
                            )
                            .await?;
                            Ok(())
                        })
                    })
                    .await
                    .unwrap();
            }
            let policy: (Option<i64>, Option<i64>, Option<i64>, i64) = sqlx::query_as(
                "SELECT current_reorder_point_milli,proposed_reorder_point_milli,retired_at,version FROM inventory_reorder_policies WHERE id='reorder-command'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(policy, (Some(3_000), None, Some(101), 3));
            let events: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM inventory_reorder_policy_events WHERE policy_id='reorder-command'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(events, 3);
            let self_review = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                    Box::pin(async move {
                        lock_reorder_sources(c).await?;
                        let source =
                            read_reorder_transition_source(c, "company", None, "reorder-command")
                                .await?;
                        plan_reorder_transition(
                            &source,
                            &ReorderActionInput {
                                action: ReorderAction::Approve,
                                expected_version: 3,
                                reason: "Self review".into(),
                            },
                            "approver",
                            None,
                            true,
                            true,
                        )
                        .map_err(Into::into)
                    })
                })
                .await;
            let self_review = match self_review.unwrap_err() { erpx_server_runtime::public::postgres::PgTransactionError::Business(error) => error, other => panic!("unexpected self-review error: {other:?}") }; assert_eq!(self_review.code, "REORDER_PROPOSAL_MISSING");
            let row = sqlx::query("SELECT current_reorder_point_milli,version FROM inventory_reorder_policies WHERE id='reorder-command'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
            assert_eq!(row.get::<i64, _>("version"), 3);
        })
        .await;
    }
}
