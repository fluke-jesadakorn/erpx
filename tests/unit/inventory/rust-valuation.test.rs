//! Independent retained Node oracle plus conservation and safe-admission cases.
//! Planning ports are synthetic: these tests do not prove a transaction or IAM.
use erpx_server_inventory::public::{consumption::*, *};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use serde_json::{Value, json};
fn corpus() -> Value {
    serde_json::from_str(include_str!(
        "../../../fixtures/inventory/rust-valuation/conformance.v1.json"
    ))
    .unwrap()
}
fn integer(value: &Value, key: &str) -> i64 {
    value[key].as_i64().unwrap()
}
fn error_json(error: BusinessError) -> Value {
    let status = match error.kind {
        FailureKind::InvalidInput => 422,
        FailureKind::Conflict => 409,
        _ => panic!("unexpected planning error kind"),
    };
    json!({"error":{"status":status,"code":error.code,"message":error.message}})
}
fn outcome(result: Result<Value, BusinessError>) -> Value {
    match result {
        Ok(value) => json!({"value":value}),
        Err(error) => error_json(error),
    }
}
#[test]
fn retained_sources_identify_the_real_b0_oracle() {
    let data = corpus();
    assert_eq!(data["version"], 1);
    assert_eq!(data["node"], "v26.11.1");
    assert_eq!(
        data["sourceCommit"],
        "a5ed8737a2f05eff1ee9841fda4a094b3926d732"
    );
    assert_eq!(data["sources"].as_array().unwrap().len(), 7);
    for row in data["sources"].as_array().unwrap() {
        let digest = row["sha256"].as_str().unwrap();
        assert_eq!(digest.len(), 64);
        assert!(digest.bytes().all(|b| b.is_ascii_hexdigit()));
    }
}
#[test]
fn all_landed_cost_allocations_match_the_actual_node_function() {
    for row in corpus()["allocations"].as_array().unwrap() {
        let weights: Vec<i64> = row["weights"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect();
        let actual =
            outcome(allocate_landed_cost(integer(row, "total"), &weights).map(|v| json!(v)));
        assert_eq!(actual, row["expected"], "{row}");
    }
}
#[test]
fn transit_preserves_pending_reservations_and_cumulative_value_rounding() {
    for row in corpus()["transit"].as_array().unwrap() {
        let l = &row["line"];
        let line = TransitValuation {
            quantity_milli: integer(l, "quantity_milli"),
            received_milli: integer(l, "received_milli"),
            lost_milli: integer(l, "lost_milli"),
            pending_loss_milli: integer(l, "pending_loss_milli"),
            value_minor: integer(l, "value_minor"),
            settled_value_minor: integer(l, "settled_value_minor"),
        };
        let actual =
            outcome(transit_quantity_value(&line, integer(row, "quantity")).map(|v| json!(v)));
        assert_eq!(actual, row["expected"], "{row}");
    }
}
#[test]
fn dispatch_and_reorder_semantics_match_validated_b0_inputs() {
    let data = corpus();
    for row in data["dates"].as_array().unwrap() {
        let actual = outcome(
            require_dispatch_date(
                row["document"]["status"].as_str().unwrap(),
                row["document"]["dispatch_date"].as_str().unwrap(),
                row["date"].as_str().unwrap(),
            )
            .map(|()| json!(true)),
        );
        assert_eq!(actual, row["expected"], "{row}");
    }
    for row in data["reorder"].as_array().unwrap() {
        let actual = outcome(
            reorder_point_milli(row["input"].as_str().unwrap()).map(|v| json!(v.to_string())),
        );
        assert_eq!(actual, row["expected"], "{row}");
    }
}
fn mode(value: &Value) -> TrackingMode {
    match value.as_str().unwrap() {
        "none" => TrackingMode::None,
        "lot" => TrackingMode::Lot,
        "serial" => TrackingMode::Serial,
        _ => panic!("invalid oracle mode"),
    }
}
fn stock(value: &Value) -> StockValuation {
    StockValuation {
        quantity_milli: integer(value, "quantity_milli"),
        value_minor: integer(value, "value_minor"),
        version: integer(value, "version"),
        tracking_mode: mode(&value["tracking_mode"]),
    }
}
fn allocation(value: &Value) -> LotAllocation {
    LotAllocation {
        lot_id: value["lotId"].as_str().unwrap().into(),
        quantity_milli: integer(value, "quantityMilli"),
        balance_version: integer(value, "balanceVersion"),
    }
}
fn rid(value: &str) -> RecordId {
    RecordId::new(value).unwrap()
}
struct Ports {
    row: Value,
    queries: Vec<String>,
}
impl ConsumptionPlanningPorts for Ports {
    type Error = BusinessError;
    async fn stock(
        &mut self,
        selection: &ConsumptionSelection,
    ) -> Result<Option<StockValuation>, BusinessError> {
        assert_eq!(selection.company_id.as_str(), "company");
        assert_eq!(selection.branch_id.as_str(), "branch");
        assert_eq!(selection.warehouse_id.as_str(), "warehouse");
        assert_eq!(selection.product_id.as_str(), "product");
        self.queries.push("stock".into());
        Ok((!self.row["stock"].is_null()).then(|| stock(&self.row["stock"])))
    }
    async fn posting_config(
        &mut self,
        company: &CompanyId,
    ) -> Result<Option<ConsumptionPostingConfig>, BusinessError> {
        assert_eq!(company.as_str(), "company");
        self.queries.push("config".into());
        let v = &self.row["config"];
        Ok((!v.is_null()).then(|| ConsumptionPostingConfig {
            chart_id: rid(v["chart_id"].as_str().unwrap()),
            currency: v["currency"].as_str().unwrap().into(),
            base_currency: v["base_currency"].as_str().unwrap().into(),
            inventory_account_id: rid(v["inventory_account_id"].as_str().unwrap()),
            expense_account_id: rid(v["cogs_account_id"].as_str().unwrap()),
        }))
    }
    async fn posting_periods(
        &mut self,
        _: &CompanyId,
        date: &str,
    ) -> Result<Vec<RecordId>, BusinessError> {
        assert_eq!(date, self.row["input"]["entryDate"].as_str().unwrap());
        self.queries.push("periods".into());
        Ok(self.row["periods"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| rid(v["id"].as_str().unwrap()))
            .collect())
    }
    async fn lot(
        &mut self,
        _: &ConsumptionSelection,
        allocation: &LotAllocation,
    ) -> Result<Option<LotValuation>, BusinessError> {
        self.queries.push(format!("lot:{}", allocation.lot_id));
        let v = &self.row["lots"][&allocation.lot_id];
        Ok((!v.is_null()).then(|| LotValuation {
            quantity_milli: integer(v, "quantity_milli"),
            value_minor: integer(v, "value_minor"),
            version: integer(v, "version"),
            tracking_mode: mode(&v["tracking_mode"]),
            expiry_date: v["expiry_date"].as_str().map(str::to_owned),
            active: v["status"] == "active",
        }))
    }
}
fn selection(row: &Value) -> ConsumptionSelection {
    let input = &row["input"];
    ConsumptionSelection {
        company_id: CompanyId::new("company").unwrap(),
        branch_id: rid("branch"),
        warehouse_id: rid("warehouse"),
        product_id: rid("product"),
        entry_date: input["entryDate"].as_str().unwrap().into(),
        quantity_milli: integer(input, "quantityMilli"),
        allocations: input["allocations"]
            .as_array()
            .unwrap()
            .iter()
            .map(allocation)
            .collect(),
    }
}
#[tokio::test]
async fn actual_consumption_application_oracle_matches_value_lot_order_and_query_failure_order() {
    for row in corpus()["consumption"].as_array().unwrap() {
        let mut ports = Ports {
            row: row.clone(),
            queries: vec![],
        };
        let result = prepare_consumption(&mut ports, selection(row)).await;
        let actual=outcome(result.map(|p|json!({"valueMinor":p.valuation.value_minor,"currency":p.config.currency,"lots":p.valuation.lots.iter().map(|l|json!({"lotId":l.allocation.lot_id,"quantityMilli":l.allocation.quantity_milli,"balanceVersion":l.allocation.balance_version,"value":l.value_minor})).collect::<Vec<_>>()})));
        assert_eq!(actual, row["expected"], "{}", row["id"]);
        assert_eq!(json!(ports.queries), row["queries"], "{}", row["id"]);
    }
}
#[test]
fn ordered_cumulative_allocation_conserves_every_unit_at_rounding_boundaries() {
    for total in 0..200 {
        for weights in [
            vec![1, 1, 1],
            vec![0, 1, 0, 2],
            vec![3, 5, 7, 11],
            vec![999, 1],
        ] {
            let parts = allocate_landed_cost(total, &weights).unwrap();
            assert_eq!(parts.len(), weights.len());
            assert_eq!(parts.iter().sum::<i64>(), total);
            assert!(parts.iter().all(|v| *v >= 0));
            for (part, weight) in parts.iter().zip(&weights) {
                if *weight == 0 {
                    assert_eq!(*part, 0);
                }
            }
        }
    }
    assert_eq!(allocate_landed_cost(1, &[1, 1, 1]).unwrap(), vec![0, 1, 0]);
    assert_eq!(allocate_landed_cost(2, &[1, 1, 1]).unwrap(), vec![1, 0, 1]);
}
#[test]
fn unsupported_numeric_domains_and_unvalidated_quantity_text_fail_closed() {
    assert!(allocate_landed_cost(1, &[MAX_SAFE_UNITS, 1]).is_err());
    assert!(allocate_landed_cost(1, &[-1, 2]).is_err());
    assert!(scaled_inventory_value(i64::MAX, 1, 1).is_err());
    assert!(scaled_inventory_value(1, 1, 0).is_err());
    for text in [
        "",
        "-1",
        "+1",
        " 1 ",
        "1.",
        ".1",
        "1.0001",
        "1.2.3",
        "1000000000000",
        "๑",
        "1e2",
    ] {
        assert!(reorder_point_milli(text).is_err(), "{text}");
    }
}
#[tokio::test]
async fn invalid_dates_and_nonpositive_consumption_cannot_prepare_a_plan() {
    let row = corpus()["consumption"][0].clone();
    for date in [
        "2026-02-29",
        "2026-04-31",
        "2026-00-01",
        "2026-01-00",
        "2026-01-01extra",
        "bad",
    ] {
        let mut ports = Ports {
            row: row.clone(),
            queries: vec![],
        };
        let mut input = selection(&row);
        input.entry_date = date.into();
        assert_eq!(
            prepare_consumption(&mut ports, input)
                .await
                .unwrap_err()
                .code,
            "INVALID_CONSUMPTION_DATE"
        );
        assert!(ports.queries.is_empty());
    }
    for quantity in [0, -1, MAX_SAFE_UNITS + 1] {
        let mut ports = Ports {
            row: row.clone(),
            queries: vec![],
        };
        let mut input = selection(&row);
        input.quantity_milli = quantity;
        assert!(prepare_consumption(&mut ports, input).await.is_err());
        assert_eq!(ports.queries, vec!["stock"]);
    }
}

// Portable planner evidence only: it does not exercise PostgreSQL or authority.
impl FieldServiceConsumptionPlanningPorts for Ports {
    type Error = BusinessError;
    async fn stock(
        &mut self,
        _: &FieldServiceConsumptionSelection,
    ) -> Result<Option<StockValuation>, BusinessError> {
        self.queries.push("stock".into());
        Ok((!self.row["stock"].is_null()).then(|| stock(&self.row["stock"])))
    }
    async fn posting_config(
        &mut self,
        company: &CompanyId,
    ) -> Result<Option<ConsumptionPostingConfig>, BusinessError> {
        ConsumptionPlanningPorts::posting_config(self, company).await
    }
    async fn posting_periods(
        &mut self,
        company: &CompanyId,
        date: &str,
    ) -> Result<Vec<RecordId>, BusinessError> {
        ConsumptionPlanningPorts::posting_periods(self, company, date).await
    }
    async fn lot(
        &mut self,
        _: &FieldServiceConsumptionSelection,
        allocation: &LotAllocation,
    ) -> Result<Option<LotValuation>, BusinessError> {
        let row = self.row.clone();
        ConsumptionPlanningPorts::lot(self, &selection(&row), allocation).await
    }
}

#[tokio::test]
async fn field_service_raw_lot_identity_preserves_planning_checks_and_legacy_bounds() {
    let original = corpus()["consumption"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "lot-partial")
        .unwrap()
        .clone();
    for id in [
        format!("raw-{}", "x".repeat(220)),
        "raw-\u{1}-control".to_owned(),
    ] {
        let mut row = original.clone();
        let lot = row["lots"]["lot-a"].clone();
        row["lots"] = json!({id.clone():lot});
        row["input"]["allocations"][0]["lotId"] = json!(id);
        let typed = selection(&row);
        let raw = FieldServiceConsumptionSelection {
            company_id: typed.company_id.clone(),
            branch_id: typed.branch_id.as_str().into(),
            warehouse_id: typed.warehouse_id.as_str().into(),
            product_id: typed.product_id.as_str().into(),
            entry_date: typed.entry_date.clone(),
            quantity_milli: typed.quantity_milli,
            lot_identity_utf16: typed
                .allocations
                .iter()
                .map(|a| a.lot_id.encode_utf16().collect())
                .collect(),
            allocations: typed.allocations.clone(),
        };
        let mut ports = Ports {
            row: row.clone(),
            queries: vec![],
        };
        let mut observed = false;
        let result = prepare_field_service_consumption_observed(&mut ports, raw.clone(), || {
            observed = true;
        })
        .await
        .unwrap();
        assert!(observed);
        assert_eq!(result.0.valuation.value_minor, 31);
        assert_eq!(
            ports.queries,
            vec!["stock", "config", "periods", &format!("lot:{id}")]
        );
        let mut legacy = Ports {
            row: row.clone(),
            queries: vec![],
        };
        assert_eq!(
            prepare_consumption(&mut legacy, typed)
                .await
                .unwrap_err()
                .code,
            "PART_LOT_ALLOCATION_REQUIRED"
        );
        assert_eq!(legacy.queries, vec!["stock", "config", "periods"]);
        for case in ["duplicate", "sum", "untracked"] {
            let mut selection = raw.clone();
            let mut fixture = row.clone();
            match case {
                "duplicate" => {
                    selection.allocations.push(selection.allocations[0].clone());
                    selection
                        .lot_identity_utf16
                        .push(selection.lot_identity_utf16[0].clone());
                }
                "sum" => selection.allocations[0].quantity_milli -= 1,
                _ => fixture["stock"]["tracking_mode"] = json!("none"),
            };
            let mut ports = Ports {
                row: fixture,
                queries: vec![],
            };
            let mut observed = false;
            let err = prepare_field_service_consumption_observed(&mut ports, selection, || {
                observed = true;
            })
            .await
            .unwrap_err();
            assert_eq!(
                err.code,
                if case == "untracked" {
                    "PART_LOT_NOT_ALLOWED"
                } else {
                    "PART_LOT_ALLOCATION_REQUIRED"
                }
            );
            assert!(observed);
            assert_eq!(ports.queries, vec!["stock", "config", "periods"]);
        }
    }
}

#[tokio::test]
async fn field_service_utf16_lot_slots_preserve_original_identity_and_storage_binding() {
    let mut row = corpus()["consumption"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "lot-partial")
        .unwrap()
        .clone();
    let lot = row["lots"]["lot-a"].clone();
    row["lots"] = json!({"�":lot});
    let typed = selection(&row);
    let allocation = LotAllocation {
        lot_id: "�".into(),
        quantity_milli: 1500,
        balance_version: 3,
    };
    let raw = FieldServiceConsumptionSelection {
        company_id: typed.company_id,
        branch_id: typed.branch_id.as_str().into(),
        warehouse_id: "warehouse".into(),
        product_id: "product".into(),
        entry_date: typed.entry_date,
        quantity_milli: 3000,
        allocations: vec![allocation.clone(), allocation],
        lot_identity_utf16: vec![vec![0xd800], vec![0xd801]],
    };
    for (ids, success) in [
        (vec![vec![0xd800], vec![0xd801]], true),
        (vec![vec![0xd800], vec![0xfffd]], true),
        (vec![vec![0xd800], vec![0xd800]], false),
        (vec![vec![0xd800]], false),
        (vec![vec![0xd800], vec![0x61]], false),
    ] {
        let mut input = raw.clone();
        input.lot_identity_utf16 = ids;
        let mut ports = Ports {
            row: row.clone(),
            queries: vec![],
        };
        let mut observed = false;
        let result = prepare_field_service_consumption_observed(&mut ports, input, || {
            observed = true;
        })
        .await;
        assert!(observed);
        if success {
            assert!(result.is_ok());
            assert_eq!(
                ports.queries,
                vec!["stock", "config", "periods", "lot:�", "lot:�"]
            );
        } else {
            assert_eq!(result.unwrap_err().code, "PART_LOT_ALLOCATION_REQUIRED");
            assert_eq!(ports.queries, vec!["stock", "config", "periods"]);
        }
    }
}

#[test]
fn service_detail_available_stock_accounts_for_reservations_and_holds() {
    use erpx_server_inventory::public::field_service_detail::service_available_milli;
    assert_eq!(service_available_milli(5_000, 1_000, false, 1_500), 2_500);
    assert_eq!(service_available_milli(5_000, 1_000, true, 0), 0);
    assert_eq!(service_available_milli(5_000, 0, false, 8_000), 0);
    assert_eq!(service_available_milli(5_000, 8_000, false, 0), 0);
}
