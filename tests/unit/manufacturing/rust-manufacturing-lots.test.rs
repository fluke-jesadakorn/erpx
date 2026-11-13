//! Native Manufacturing production-lot posting policy and use cases.
//! Domain/application only: no PostgreSQL, HTTP or browser.
use erpx_server_manufacturing::public::lots::{
    Genealogy, LotPosting, ManufacturingLotRepository, MaterialAllocation, MaterialIssueRow,
    MaterialLotFacts, MaterialLotPlan, MaterialLotRequest, OutputAllocation, OutputLotPlan,
    OutputLotRequest, TrackingMode, plan_material_lots, plan_output_lots, post_material_lots,
    post_output_lots, scaled_amount,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;
use std::pin::Pin;

type Fut<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Default)]
struct Fake {
    facts: Vec<Option<MaterialLotFacts>>,
    issues: Vec<MaterialIssueRow>,
    calls: Vec<String>,
}

impl ManufacturingLotRepository for Fake {
    type Error = BusinessError;

    fn material_lot<'a>(
        &'a mut self,
        _company: &'a str,
        _product: &'a str,
        _warehouse: &'a str,
        lot: &'a str,
    ) -> Fut<'a, Option<MaterialLotFacts>> {
        self.calls.push(format!("read:{lot}"));
        let index = self.calls.iter().filter(|call| call.starts_with("read:")).count() - 1;
        let facts = self.facts.get(index).cloned().flatten();
        Box::pin(async move { Ok(facts) })
    }

    fn material_lot_issues<'a>(&'a mut self, _order: &'a str) -> Fut<'a, Vec<MaterialIssueRow>> {
        self.calls.push("issues".into());
        let issues = self.issues.clone();
        Box::pin(async move { Ok(issues) })
    }

    fn commit_material_lots<'a>(
        &'a mut self,
        plan: &'a MaterialLotPlan,
        _posting: &'a LotPosting<'a>,
    ) -> Fut<'a, ()> {
        self.calls.push(format!("material:{}", plan.value_minor));
        Box::pin(async move { Ok(()) })
    }

    fn commit_output_lots<'a>(
        &'a mut self,
        plan: &'a OutputLotPlan,
        _posting: &'a LotPosting<'a>,
    ) -> Fut<'a, ()> {
        self.calls.push(format!(
            "output:{}:{}",
            plan.lots.len(),
            plan.genealogy.len()
        ));
        Box::pin(async move { Ok(()) })
    }
}

fn posting<'a>(product: &'a str, key: &'a str) -> LotPosting<'a> {
    LotPosting {
        key,
        order_id: "order",
        order_version: 3,
        warehouse_id: "warehouse",
        product_id: product,
        entry_date: "2026-10-01",
        company_id: "company",
        user_id: "maker",
        now: 42,
    }
}

fn facts(quantity: i64, value: i64, version: i64) -> MaterialLotFacts {
    MaterialLotFacts {
        status: "active".into(),
        held: false,
        balance_version: version,
        quantity_milli: quantity,
        value_minor: value,
        expiry_date: None,
    }
}

fn allocation(lot: &str, quantity: i64, version: i64) -> MaterialAllocation {
    MaterialAllocation {
        product_id: "material".into(),
        lot_id: lot.into(),
        quantity_milli: quantity,
        balance_version: version,
    }
}

#[tokio::test]
async fn untracked_material_resolves_to_no_lot_work() {
    let mut fake = Fake::default();
    let result = post_material_lots(
        &mut fake,
        posting("material", "key"),
        MaterialLotRequest {
            tracking_mode: TrackingMode::None,
            required_milli: 1_000,
            allocations: vec![],
        },
    )
    .await
    .unwrap();
    assert!(result.is_none());
    assert!(fake.calls.is_empty());
}

#[tokio::test]
async fn untracked_material_rejects_supplied_allocations() {
    let mut fake = Fake::default();
    let error = post_material_lots(
        &mut fake,
        posting("material", "key"),
        MaterialLotRequest {
            tracking_mode: TrackingMode::None,
            required_milli: 1_000,
            allocations: vec![allocation("lot", 1_000, 1)],
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "LOT_ALLOCATION_UNEXPECTED");
    assert!(fake.calls.is_empty());
}

#[tokio::test]
async fn missing_full_allocation_stops_before_any_lot_effect() {
    let mut fake = Fake::default();
    let error = post_material_lots(
        &mut fake,
        posting("material", "key"),
        MaterialLotRequest {
            tracking_mode: TrackingMode::Lot,
            required_milli: 1_000,
            allocations: vec![],
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "MANUFACTURING_LOT_ALLOCATION_REQUIRED");
    assert!(fake.calls.is_empty());
}

#[tokio::test]
async fn duplicate_material_lot_is_rejected() {
    let mut fake = Fake {
        facts: vec![Some(facts(2_000, 200, 1)), Some(facts(2_000, 200, 1))],
        ..Fake::default()
    };
    let error = post_material_lots(
        &mut fake,
        posting("material", "key"),
        MaterialLotRequest {
            tracking_mode: TrackingMode::Lot,
            required_milli: 2_000,
            allocations: vec![allocation("lot", 1_000, 1), allocation("lot", 1_000, 1)],
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "LOT_ALLOCATION_DUPLICATE");
    assert!(fake.calls.is_empty());
}

#[test]
fn material_lot_guard_covers_state_version_quantity_expiry_and_holds() {
    let entry = "2026-10-01";
    let base = allocation("lot", 1_000, 4);
    let ok = plan_material_lots(
        TrackingMode::Lot,
        1_000,
        entry,
        &[base.clone()],
        &[Some(facts(1_000, 100, 4))],
    )
    .unwrap()
    .unwrap();
    assert_eq!(ok.value_minor, 100);
    for (case, fact) in [
        ("held", MaterialLotFacts { held: true, ..facts(1_000, 100, 4) }),
        ("status", MaterialLotFacts { status: "depleted".into(), ..facts(1_000, 100, 4) }),
        ("version", facts(1_000, 100, 5)),
        ("quantity", facts(999, 100, 4)),
        (
            "expiry",
            MaterialLotFacts { expiry_date: Some("2026-09-30".into()), ..facts(1_000, 100, 4) },
        ),
    ] {
        let error = plan_material_lots(
            TrackingMode::Lot,
            1_000,
            entry,
            &[base.clone()],
            &[Some(fact)],
        )
        .unwrap_err();
        assert_eq!(error.code, "MANUFACTURING_LOT_UNAVAILABLE", "{case}");
    }
    assert_eq!(
        plan_material_lots(TrackingMode::Lot, 1_000, entry, &[base], &[None])
            .unwrap_err()
            .code,
        "MANUFACTURING_LOT_UNAVAILABLE"
    );
}

#[test]
fn material_value_is_proportional_and_rounds_half_up() {
    let plan = plan_material_lots(
        TrackingMode::Lot,
        1_500,
        "2026-10-01",
        &[allocation("lot", 1_500, 2)],
        &[Some(facts(4_000, 1_001, 2))],
    )
    .unwrap()
    .unwrap();
    // (1001*1500 + 2000) / 4000 = 375
    assert_eq!(plan.value_minor, 375);
    assert_eq!(scaled_amount(1_001, 1_500, 4_000), Some(375));
    assert_eq!(scaled_amount(5, 1, 2), Some(3));
}

#[test]
fn material_serial_must_be_one_unit() {
    let error = plan_material_lots(
        TrackingMode::Serial,
        2_000,
        "2026-10-01",
        &[allocation("lot", 2_000, 1)],
        &[Some(facts(2_000, 200, 1))],
    )
    .unwrap_err();
    assert_eq!(error.code, "SERIAL_QUANTITY_INVALID");
}

#[test]
fn output_allocations_require_full_quantity_and_unique_identity() {
    let input = erpx_server_manufacturing::public::lots::OutputLotInput {
        key: "key",
        order_id: "order",
        order_version: 4,
        tracking_mode: TrackingMode::Lot,
        output_milli: 1_800,
        total_minor: 3_200,
        entry_date: "2026-10-02",
    };
    let short = plan_output_lots(
        &input,
        &[OutputAllocation {
            quantity_milli: 1_000,
            lot_code: Some("A".into()),
            serial_number: None,
            expiry_date: None,
        }],
        &[],
    )
    .unwrap_err();
    assert_eq!(short.code, "MANUFACTURING_OUTPUT_LOTS_REQUIRED");
    let duplicate = plan_output_lots(
        &input,
        &[
            OutputAllocation {
                quantity_milli: 900,
                lot_code: Some("A".into()),
                serial_number: None,
                expiry_date: None,
            },
            OutputAllocation {
                quantity_milli: 900,
                lot_code: Some("A".into()),
                serial_number: None,
                expiry_date: None,
            },
        ],
        &[],
    )
    .unwrap_err();
    assert_eq!(duplicate.code, "MANUFACTURING_OUTPUT_IDENTITY");
}

#[test]
fn output_rejects_expired_and_non_unit_serials() {
    let entry = "2026-10-02";
    let lot_input = erpx_server_manufacturing::public::lots::OutputLotInput {
        key: "key",
        order_id: "order",
        order_version: 4,
        tracking_mode: TrackingMode::Lot,
        output_milli: 1_000,
        total_minor: 1_000,
        entry_date: entry,
    };
    let expired = plan_output_lots(
        &lot_input,
        &[OutputAllocation {
            quantity_milli: 1_000,
            lot_code: Some("A".into()),
            serial_number: None,
            expiry_date: Some("2026-10-01".into()),
        }],
        &[],
    )
    .unwrap_err();
    assert_eq!(expired.code, "MANUFACTURING_OUTPUT_EXPIRY");
    let serial_input = erpx_server_manufacturing::public::lots::OutputLotInput {
        tracking_mode: TrackingMode::Serial,
        output_milli: 2_000,
        total_minor: 2_000,
        ..lot_input
    };
    let serial = plan_output_lots(
        &serial_input,
        &[OutputAllocation {
            quantity_milli: 2_000,
            lot_code: None,
            serial_number: Some("S1".into()),
            expiry_date: None,
        }],
        &[],
    )
    .unwrap_err();
    assert_eq!(serial.code, "SERIAL_QUANTITY_INVALID");
}

#[test]
fn output_value_and_genealogy_split_across_allocations() {
    let input = erpx_server_manufacturing::public::lots::OutputLotInput {
        key: "key",
        order_id: "order",
        order_version: 4,
        tracking_mode: TrackingMode::Lot,
        output_milli: 1_800,
        total_minor: 3_200,
        entry_date: "2026-10-02",
    };
    let allocations = [
        OutputAllocation {
            quantity_milli: 1_000,
            lot_code: Some("A".into()),
            serial_number: None,
            expiry_date: None,
        },
        OutputAllocation {
            quantity_milli: 800,
            lot_code: Some("B".into()),
            serial_number: None,
            expiry_date: Some("2027-01-01".into()),
        },
    ];
    let issues = [
        MaterialIssueRow { id: "issue-a".into(), quantity_milli: 1_000 },
        MaterialIssueRow { id: "issue-b".into(), quantity_milli: 800 },
    ];
    let plan = plan_output_lots(&input, &allocations, &issues).unwrap().unwrap();
    assert_eq!(
        plan.lots.iter().map(|lot| lot.value_minor).collect::<Vec<_>>(),
        [1_778, 1_422]
    );
    assert_eq!(plan.lots.iter().map(|lot| lot.value_minor).sum::<i64>(), 3_200);
    assert_eq!(plan.lots[0].lot_id, "mfg-output-lot-key-0");
    assert_eq!(plan.lots[1].output_id, "mfg-output-key-1");
    assert_eq!(
        plan.genealogy,
        [
            Genealogy {
                id: "mfg-lot-genealogy-key-0-0".into(),
                issue_id: "issue-a".into(),
                output_id: "mfg-output-key-0".into(),
                input_quantity_milli: 556,
            },
            Genealogy {
                id: "mfg-lot-genealogy-key-0-1".into(),
                issue_id: "issue-b".into(),
                output_id: "mfg-output-key-0".into(),
                input_quantity_milli: 444,
            },
            Genealogy {
                id: "mfg-lot-genealogy-key-1-0".into(),
                issue_id: "issue-a".into(),
                output_id: "mfg-output-key-1".into(),
                input_quantity_milli: 444,
            },
            Genealogy {
                id: "mfg-lot-genealogy-key-1-1".into(),
                issue_id: "issue-b".into(),
                output_id: "mfg-output-key-1".into(),
                input_quantity_milli: 356,
            },
        ]
    );
}

#[tokio::test]
async fn output_posting_reads_issue_evidence_then_commits() {
    let mut fake = Fake {
        issues: vec![MaterialIssueRow { id: "issue-a".into(), quantity_milli: 1_000 }],
        ..Fake::default()
    };
    let result = post_output_lots(
        &mut fake,
        posting("output", "key"),
        OutputLotRequest {
            tracking_mode: TrackingMode::Lot,
            output_milli: 1_000,
            total_minor: 2_000,
            allocations: vec![OutputAllocation {
                quantity_milli: 1_000,
                lot_code: Some("A".into()),
                serial_number: None,
                expiry_date: None,
            }],
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result.lots, 1);
    assert_eq!(result.genealogy_rows, 1);
    assert_eq!(fake.calls, ["issues", "output:1:1"]);
}

#[tokio::test]
async fn untracked_output_writes_no_lot_or_genealogy() {
    let mut fake = Fake::default();
    let result = post_output_lots(
        &mut fake,
        posting("output", "key"),
        OutputLotRequest {
            tracking_mode: TrackingMode::None,
            output_milli: 1_000,
            total_minor: 1_000,
            allocations: vec![],
        },
    )
    .await
    .unwrap();
    assert!(result.is_none());
    assert!(fake.calls.is_empty());
}

#[tokio::test]
async fn material_posting_filters_other_products_and_commits_one_plan() {
    let mut fake = Fake {
        facts: vec![Some(facts(1_000, 500, 1))],
        ..Fake::default()
    };
    let result = post_material_lots(
        &mut fake,
        posting("material", "key"),
        MaterialLotRequest {
            tracking_mode: TrackingMode::Lot,
            required_milli: 1_000,
            allocations: vec![
                MaterialAllocation { product_id: "other".into(), lot_id: "other-lot".into(), quantity_milli: 5, balance_version: 1 },
                allocation("lot", 1_000, 1),
            ],
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result.value_minor, 500);
    assert_eq!(result.issued_lots, 1);
    assert_eq!(fake.calls, ["read:lot", "material:500"]);
}
