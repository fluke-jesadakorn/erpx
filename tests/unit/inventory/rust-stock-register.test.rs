//! Portable stock-register admission and reconciliation. These cases do not
//! prove a transaction, IAM or PostgreSQL source; they pin the owner's field
//! and arithmetic contract.
use erpx_server_inventory::public::stock_register::*;

fn stock(
    warehouse: &str,
    product: &str,
    tracking: &str,
    quantity: i64,
    value: i64,
) -> StockRegisterRow {
    StockRegisterRow {
        warehouse_id: warehouse.into(),
        product_id: product.into(),
        sku: format!("SKU-{product}"),
        name: "Product".into(),
        unit: "pcs".into(),
        tracking_mode: tracking.into(),
        warehouse_name: format!("Warehouse {warehouse}"),
        quantity_milli: quantity,
        reserved_milli: 0,
        quarantined_milli: 0,
        in_transit_milli: 0,
        value_minor: value,
        version: 1,
        available_milli: 0,
    }
}

fn lot(id: &str, tracking: &str, quantity: i64, value: i64) -> StockRegisterLot {
    StockRegisterLot {
        id: id.into(),
        warehouse_id: "w".into(),
        product_id: "p".into(),
        sku: "SKU-p".into(),
        tracking_mode: tracking.into(),
        warehouse_name: "Warehouse w".into(),
        lot_code: (tracking == "lot").then(|| format!("L-{id}")),
        serial_number: (tracking == "serial").then(|| format!("S-{id}")),
        manufactured_date: None,
        expiry_date: Some("2027-01-01".into()),
        status: "active".into(),
        quantity_milli: quantity,
        value_minor: value,
        version: 2,
        quality_hold_milli: 0,
        quality_status: None,
    }
}

fn count(id: &str, status: &str, version: i64, initial: bool) -> StockRegisterCount {
    StockRegisterCount {
        id: id.into(),
        number: format!("CNT-{id}"),
        status: status.into(),
        version,
        count_date: "2026-11-04".into(),
        reason: "Physical count".into(),
        warehouse_id: "w".into(),
        warehouse_name: "Warehouse w".into(),
        net_value_minor: 0,
        posted_at: (status == "posted").then_some(100),
        journal_id: (status == "posted").then(|| "journal-1".into()),
        initial_physical_source: initial,
        created_at: 1,
        updated_at: 2,
    }
}

#[test]
fn derives_available_from_reservation_and_quarantine() {
    let mut row = stock("w", "p", "none", 10_000, 1_000);
    row.reserved_milli = 2_000;
    row.quarantined_milli = 4_000;
    row.in_transit_milli = 1_500;
    let row = validate_row(row).unwrap();
    assert_eq!(row.available_milli, 6_000);
    assert_eq!(row.in_transit_milli, 1_500);
}

#[test]
fn reconciles_only_tracked_stock_to_lot_balances() {
    let register = validate_register(
        vec![
            stock("w", "p", "lot", 3_000, 3_000),
            stock("w", "u", "none", 7_000, 7_000),
        ],
        vec![lot("a", "lot", 2_000, 2_000), lot("b", "lot", 1_000, 1_000)],
        vec![count("c1", "posted", 4, true)],
        "THB".into(),
    )
    .unwrap();
    assert_eq!(register.totals.tracked_quantity_milli, 3_000);
    assert_eq!(register.totals.tracked_value_minor, 3_000);
    assert_eq!(register.totals.lot_quantity_milli, 3_000);
    assert_eq!(register.totals.lot_value_minor, 3_000);
    assert!(register.totals.consistent);
    assert!(register.counts[0].initial_physical_source);
    assert_eq!(register.counts[0].version, 4);
    assert_eq!(register.counts[0].journal_id.as_deref(), Some("journal-1"));
}

#[test]
fn a_lot_shortfall_breaks_reconciliation_without_hiding_rows() {
    let register = validate_register(
        vec![stock("w", "p", "lot", 3_000, 3_000)],
        vec![lot("a", "lot", 2_000, 2_000)],
        vec![],
        "THB".into(),
    )
    .unwrap();
    assert!(!register.totals.consistent);
    assert_eq!(register.stock.len(), 1);
    assert_eq!(register.lots.len(), 1);
}

#[test]
fn keeps_signed_scrap_count_values_in_the_register() {
    let mut scrap = count("scrap", "submitted", 1, false);
    scrap.net_value_minor = -300;
    let register = validate_register(vec![], vec![], vec![scrap], "THB".into()).unwrap();
    assert_eq!(register.counts[0].net_value_minor, -300);
}

#[test]
fn rejects_serial_lot_identity_mix_and_invalid_lifecycle() {
    let mut mixed = lot("a", "lot", 1_000, 1_000);
    mixed.serial_number = Some("S".into());
    assert_eq!(
        validate_lot(mixed).unwrap_err().code,
        "INVENTORY_STOCK_REGISTER_INVALID"
    );
    assert_eq!(
        validate_count(count("c", "approved", 1, false))
            .unwrap()
            .status,
        "approved"
    );
    assert_eq!(
        validate_count(count("c", "draft", 0, false))
            .unwrap_err()
            .code,
        "INVENTORY_STOCK_REGISTER_INVALID"
    );
    assert_eq!(
        validate_register(vec![], vec![], vec![], "thb".into())
            .unwrap_err()
            .code,
        "INVENTORY_STOCK_REGISTER_INVALID"
    );
}


mod transfer_landed_reorder {
    use erpx_server_inventory::public::landed_costs::{
        LandedCostAction, LandedCostActionInput, LandedCostCreateInput, LandedCostCreateSource,
        LandedCostRecord, LandedCostTransitionSource, ReceiptLine, ReceiptLotBalance, ReceiptRecord,
        ReceivingStock, money_minor, plan_create, plan_transition as plan_landed_transition,
    };
    use erpx_server_inventory::public::reorder_policies::{
        ReorderAction, ReorderActionInput, ReorderPolicyRecord, ReorderProduct, ReorderSaveInput,
        ReorderSaveSource, ReorderTransitionSource, ReorderWarehouse, plan_save as plan_reorder_save,
        plan_transition as plan_reorder_transition,
    };
    use erpx_server_inventory::public::transfers::{
        LotSnapshot, StockSnapshot, TransferAction, TransferActionInput, TransferLineFacts,
        TransferLineInput, TransferSaveInput, TransferSaveSource, TransferTransitionSource, Warehouse,
        plan_save, plan_transition, positive_quantity,
    };

    fn warehouse(id: &str, branch: &str, version: i64) -> Warehouse {
        Warehouse {
            id: id.into(),
            branch_id: branch.into(),
            version,
        }
    }

    fn stock(tracking: &str, quantity: i64, value: i64) -> StockSnapshot {
        StockSnapshot {
            product_id: "product".into(),
            name: "Product".into(),
            unit: "pcs".into(),
            tracking_mode: tracking.into(),
            quantity_milli: quantity,
            value_minor: value,
            reserved_milli: 0,
            quarantined_milli: 0,
            version: 3,
            product_version: 2,
        }
    }

    fn input(lines: Vec<TransferLineInput>) -> TransferSaveInput {
        TransferSaveInput {
            source_warehouse_id: "source".into(),
            target_warehouse_id: "target".into(),
            transfer_date: "2026-11-13".into(),
            delivery_mode: None,
            arrival_due_date: None,
            reason: "Move replenishment stock".into(),
            expected_version: None,
            lines,
        }
    }

    fn line(product: &str, quantity: &str) -> TransferLineInput {
        TransferLineInput {
            product_id: product.into(),
            quantity: quantity.into(),
            allocations: Vec::new(),
        }
    }

    #[test]
    fn transfer_save_rejects_same_warehouse_and_duplicate_products() {
        let mut same = input(vec![line("product", "1")]);
        same.target_warehouse_id = "source".into();
        let source = TransferSaveSource {
            current: None,
            source: warehouse("source", "branch", 1),
            target: warehouse("source", "branch", 1),
            lines: vec![TransferLineFacts {
                stock: stock("none", 3_000, 1_000),
                lots: Vec::new(),
            }],
        };
        assert_eq!(
            plan_save(&source, &same, "actor", "company", "transfer-key", "TR-1")
                .unwrap_err()
                .code,
            "SAME_WAREHOUSE"
        );
        let duplicate = input(vec![line("product", "1"), line("product", "1")]);
        let source = TransferSaveSource {
            current: None,
            source: warehouse("source", "branch", 1),
            target: warehouse("target", "branch", 1),
            lines: vec![
                TransferLineFacts {
                    stock: stock("none", 3_000, 1_000),
                    lots: Vec::new(),
                };
                2
            ],
        };
        assert_eq!(
            plan_save(&source, &duplicate, "actor", "company", "transfer-key", "TR-1")
                .unwrap_err()
                .code,
            "DUPLICATE_PRODUCT"
        );
    }

    #[test]
    fn transfer_save_values_untracked_stock_by_source_share() {
        let source = TransferSaveSource {
            current: None,
            source: warehouse("source", "branch", 7),
            target: warehouse("target", "other", 9),
            lines: vec![TransferLineFacts {
                stock: stock("none", 3_000, 1_000),
                lots: Vec::new(),
            }],
        };
        let plan = plan_save(
            &source,
            &input(vec![line("product", "1.5")]),
            "actor",
            "company",
            "transfer-key",
            "TR-1",
        )
        .unwrap();
        assert_eq!(plan.version, 1);
        assert_eq!(plan.total_value_minor, 500);
        assert_eq!(plan.lines[0].quantity_milli, 1_500);
    }

    #[test]
    fn transfer_save_requires_full_lot_allocation_and_rejects_holds() {
        let lots = vec![
            LotSnapshot {
                lot_id: "lot-a".into(),
                quantity_milli: 1_000,
                value_minor: 400,
                balance_version: 1,
                quality_held: false,
            },
            LotSnapshot {
                lot_id: "lot-b".into(),
                quantity_milli: 1_000,
                value_minor: 600,
                balance_version: 1,
                quality_held: true,
            },
        ];
        let source = TransferSaveSource {
            current: None,
            source: warehouse("source", "branch", 1),
            target: warehouse("target", "branch", 1),
            lines: vec![TransferLineFacts {
                stock: stock("lot", 2_000, 1_000),
                lots: lots.clone(),
            }],
        };
        let mut partial = line("product", "2");
        partial.allocations = vec![erpx_server_inventory::public::transfers::TransferLotAllocationInput {
            lot_id: "lot-a".into(),
            quantity: "1".into(),
        }];
        assert_eq!(
            plan_save(&source, &input(vec![partial]), "actor", "company", "t", "TR-1")
                .unwrap_err()
                .code,
            "LOT_ALLOCATION_REQUIRED"
        );
        let mut held = line("product", "2");
        held.allocations = vec![
            erpx_server_inventory::public::transfers::TransferLotAllocationInput {
                lot_id: "lot-a".into(),
                quantity: "1".into(),
            },
            erpx_server_inventory::public::transfers::TransferLotAllocationInput {
                lot_id: "lot-b".into(),
                quantity: "1".into(),
            },
        ];
        assert_eq!(
            plan_save(&source, &input(vec![held]), "actor", "company", "t", "TR-1")
                .unwrap_err()
                .code,
            "LOT_UNAVAILABLE"
        );
    }

    #[test]
    fn transfer_transition_requires_independence_and_current_version() {
        let source = TransferTransitionSource {
            transfer: erpx_server_inventory::public::transfers::TransferRecord {
                id: "transfer".into(),
                company_id: "company".into(),
                source_warehouse_id: "source".into(),
                target_warehouse_id: "target".into(),
                source_branch_id: "source-branch".into(),
                target_branch_id: "target-branch".into(),
                source_warehouse_version: 1,
                target_warehouse_version: 1,
                status: "submitted".into(),
                version: 2,
                created_by: "preparer".into(),
                prepared_by: "preparer".into(),
                delivery_mode: "direct".into(),
                arrival_due_date: None,
                transfer_date: "2026-11-13".into(),
            },
            lines: Vec::new(),
            allocations: Vec::new(),
            source_warehouse: warehouse("source", "source-branch", 1),
            target_warehouse: warehouse("target", "target-branch", 1),
            source_stock: Vec::new(),
            target_stock: Vec::new(),
            target_lots: Vec::new(),
            periods: vec!["period".into()],
            transit: None,
        };
        let action = TransferActionInput {
            expected_version: 2,
            reason: Some("Independent receipt review".into()),
            attachment_set_ids: Vec::new(),
        };
        assert_eq!(
            plan_transition(&source, &action, TransferAction::Post, "preparer")
                .unwrap_err()
                .code,
            "TRANSFER_ACTION_FORBIDDEN"
        );
        let stale = TransferActionInput {
            expected_version: 1,
            ..action
        };
        assert_eq!(
            plan_transition(&source, &stale, TransferAction::Post, "reviewer")
                .unwrap_err()
                .code,
            "TRANSFER_VERSION_CHANGED"
        );
    }

    #[test]
    fn transfer_positive_quantity_matches_three_decimal_contract() {
        assert_eq!(positive_quantity("1.5").unwrap(), 1_500);
        assert_eq!(positive_quantity("0").unwrap_err().code, "TRANSFER_QUANTITY_INVALID");
        assert_eq!(positive_quantity("1.2345").unwrap_err().code, "TRANSFER_QUANTITY_INVALID");
    }

    fn receipt(lines: Vec<ReceiptLine>, lots: Vec<Vec<ReceiptLotBalance>>) -> LandedCostCreateSource {
        LandedCostCreateSource {
            receipt: ReceiptRecord {
                id: "receipt".into(),
                number: "GR-1".into(),
                fulfillment_date: "2026-11-01".into(),
                created_at: 1,
                order_number: "PO-1".into(),
                branch_id: "branch".into(),
                warehouse_id: "warehouse".into(),
                order_kind: "purchase".into(),
                journal_status: "posted".into(),
                currency: "THB".into(),
                chart_id: "chart".into(),
                inventory_account_id: "inventory".into(),
                version: 1,
            },
            lines,
            stock: vec![
                ReceivingStock {
                    quantity_milli: 2_000,
                    value_minor: 2_000,
                    version: 1,
                };
                lots.len()
            ],
            lots,
            credit_account_exists: true,
        }
    }

    fn receipt_line(id: &str, product: &str) -> ReceiptLine {
        ReceiptLine {
            id: id.into(),
            product_id: product.into(),
            quantity_milli: 1_000,
            value_minor: 1_000,
            tracking_mode: "none".into(),
        }
    }

    fn landed_input(amount: &str) -> LandedCostCreateInput {
        LandedCostCreateInput {
            fulfillment_id: "receipt".into(),
            cost_date: "2026-11-02".into(),
            description: "Ocean freight and duty".into(),
            allocation_method: "quantity".into(),
            amount: amount.into(),
            credit_account_id: "freight".into(),
        }
    }

    #[test]
    fn landed_cost_allocates_remainder_and_rejects_pre_receipt_dates() {
        let source = receipt(
            vec![
                receipt_line("line-a", "product-a"),
                receipt_line("line-b", "product-b"),
            ],
            vec![Vec::new(), Vec::new()],
        );
        let plan = plan_create(
            &source,
            &landed_input("1.00"),
            "company",
            "actor",
            "landed-key",
            "LC-1",
        )
        .unwrap();
        assert_eq!(plan.amount_minor, 100);
        assert_eq!(
            plan.lines
                .iter()
                .map(|line| line.allocated_minor)
                .sum::<i64>(),
            100
        );
        let mut early = landed_input("1.00");
        early.cost_date = "2026-10-31".into();
        assert_eq!(
            plan_create(&source, &early, "company", "actor", "landed-key", "LC-1")
                .unwrap_err()
                .code,
            "LANDED_COST_DATE_INVALID"
        );
        assert_eq!(money_minor("0.00").unwrap_err().code, "LANDED_COST_AMOUNT_REQUIRED");
    }

    #[test]
    fn landed_cost_review_requires_independent_company_finance() {
        let source = LandedCostTransitionSource {
            cost: LandedCostRecord {
                id: "landed".into(),
                company_id: "company".into(),
                branch_id: "branch".into(),
                fulfillment_id: "receipt".into(),
                number: "LC-1".into(),
                cost_date: "2026-11-02".into(),
                description: "Ocean freight and duty".into(),
                allocation_method: "quantity".into(),
                amount_minor: 100,
                currency: "THB".into(),
                chart_id: "chart".into(),
                credit_account_id: "freight".into(),
                inventory_account_id: "inventory".into(),
                warehouse_id: "warehouse".into(),
                status: "submitted".into(),
                version: 1,
                created_by: "preparer".into(),
                journal_id: None,
            },
            lines: Vec::new(),
            lots: Vec::new(),
            periods: vec!["period".into()],
        };
        let input = LandedCostActionInput {
            expected_version: 1,
            reason: "Independent review of freight allocation".into(),
        };
        assert_eq!(
            plan_landed_transition(
                &source,
                &input,
                LandedCostAction::Approve,
                "preparer",
                None,
                None
            )
            .unwrap_err()
            .code,
            "INDEPENDENT_FINANCE_REQUIRED"
        );
        assert_eq!(
            plan_landed_transition(
                &source,
                &input,
                LandedCostAction::Approve,
                "reviewer",
                Some("branch"),
                None
            )
            .unwrap_err()
            .code,
            "INDEPENDENT_FINANCE_REQUIRED"
        );
    }

    fn reorder_source(policy: Option<ReorderPolicyRecord>) -> ReorderSaveSource {
        ReorderSaveSource {
            policy,
            warehouse: Some(ReorderWarehouse {
                id: "warehouse".into(),
                branch_id: "branch".into(),
                version: 1,
            }),
            product: Some(ReorderProduct {
                id: "product".into(),
                version: 1,
            }),
        }
    }

    #[test]
    fn reorder_save_requires_editable_proposal_and_current_version() {
        let source = reorder_source(None);
        let input = ReorderSaveInput {
            warehouse_id: "warehouse".into(),
            product_id: "product".into(),
            reorder_point: "1.5".into(),
            reason: "Replenishment threshold".into(),
            expected_version: Some(1),
        };
        assert_eq!(
            plan_reorder_save(&source, &input, "company", "reorder-key")
                .unwrap_err()
                .code,
            "REORDER_POLICY_CHANGED"
        );
        let mut plan_input = input;
        plan_input.expected_version = None;
        let plan = plan_reorder_save(&source, &plan_input, "company", "reorder-key").unwrap();
        assert_eq!(plan.point_milli, 1_500);
        assert_eq!(plan.version, 1);
    }

    #[test]
    fn reorder_review_requires_independent_company_approver() {
        let source = ReorderTransitionSource {
            policy: ReorderPolicyRecord {
                id: "reorder".into(),
                company_id: "company".into(),
                warehouse_id: "warehouse".into(),
                product_id: "product".into(),
                branch_id: "branch".into(),
                current_reorder_point_milli: None,
                proposed_reorder_point_milli: Some(1_500),
                requested_by: "preparer".into(),
                approved_by: None,
                retired_at: None,
                version: 2,
            },
        };
        let input = ReorderActionInput {
            action: ReorderAction::Approve,
            expected_version: 2,
            reason: "Independent threshold review".into(),
        };
        assert_eq!(
            plan_reorder_transition(&source, &input, "preparer", None, true, true)
                .unwrap_err()
                .code,
            "INDEPENDENT_APPROVER_REQUIRED"
        );
        assert_eq!(
            plan_reorder_transition(&source, &input, "approver", Some("branch"), true, true)
                .unwrap_err()
                .code,
            "INVENTORY_APPROVAL_FORBIDDEN"
        );
    }
}
