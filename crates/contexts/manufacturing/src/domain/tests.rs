use chrono::{TimeZone, Utc};
use erpx_integration_contracts::{
    CurrencyCode, MoneyAmount, Quantity, StableRecordRef, catalog::ContextId,
    reference::RecordTypeId,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuthorityScope, CommandId, CorrelationId, HumanDecisionProof,
    RecordId, SessionId,
};
use uuid::Uuid;

use super::*;

fn uuid(value: u128) -> Uuid {
    let mut bytes = value.to_be_bytes();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
fn at(hour: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 15, hour, 0, 0)
        .single()
        .unwrap()
}
fn scope() -> AuthorityScope {
    AuthorityScope::Branch {
        holding_id: uuid(1),
        company_id: uuid(2),
        branch_id: uuid(3),
    }
}
fn record(id: u128, owner: ContextId, kind: &str) -> StableRecordRef {
    StableRecordRef::try_new(
        RecordId::try_from_uuid(uuid(id)).unwrap(),
        owner,
        RecordTypeId::try_new(kind).unwrap(),
    )
    .unwrap()
}
fn quantity(units: i64) -> Quantity {
    Quantity::try_new(units, 3).unwrap()
}
fn human_decision(reference: String, decided_at: chrono::DateTime<Utc>) -> HumanDecisionProof {
    HumanDecisionProof::try_new(
        ActorAttribution {
            principal_ref: uuid(90).to_string(),
            kind: ActorKind::HoldingUser,
            session_id: Some(SessionId::try_from_uuid(uuid(91)).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope(),
        reference,
        decided_at,
    )
    .unwrap()
}
fn specification(mode: ManufacturingMode) -> ManufacturingSpecification {
    let role = if mode == ManufacturingMode::Discrete {
        MaterialRole::Component
    } else {
        MaterialRole::ProcessInput
    };
    let mut value = ManufacturingSpecification::draft(
        SpecificationId::try_from_uuid(uuid(10)).unwrap(),
        scope(),
        mode,
        record(11, ContextId::Inventory, "inventory.lot"),
        quantity(1_000),
        vec![MaterialLine {
            line: 10,
            item: record(12, ContextId::Inventory, "inventory.lot"),
            quantity: quantity(2_000),
            role,
            expected_scrap_basis_points: 100,
        }],
        RoutingId::try_from_uuid(uuid(13)).unwrap(),
        vec![RoutingStep {
            sequence: 10,
            operation_code: "MIX".into(),
            work_center: record(14, ContextId::Assets, "asset.asset"),
            setup_seconds: 60,
            run_seconds_per_output: 1,
        }],
        at(1),
    )
    .unwrap();
    let command_id = CommandId::try_from_uuid(uuid(101)).unwrap();
    let decision = human_decision(
        specification_publish_decision_ref(
            value.reference().specification_id,
            value.reference().version,
            command_id,
        ),
        at(2),
    );
    value.release(command_id, &decision).unwrap();
    value
}

#[test]
fn discrete_order_enforces_reserve_issue_and_idempotent_facts() {
    let spec = specification(ManufacturingMode::Discrete);
    let mut order = ProductionOrder::draft(
        ProductionOrderId::try_from_uuid(uuid(20)).unwrap(),
        scope(),
        &spec,
        quantity(1_000),
        CorrelationId::try_from_uuid(uuid(19)).unwrap(),
        at(3),
    )
    .unwrap();
    let command_id = CommandId::try_from_uuid(uuid(102)).unwrap();
    let decision = human_decision(
        production_order_release_decision_ref(order.id(), order.version(), command_id),
        at(4),
    );
    order.release(command_id, &decision).unwrap();
    assert_eq!(
        order.start(at(5)),
        Err(ManufacturingDomainError::MaterialNotReserved)
    );
    order.reserve_material(10, quantity(2_000), at(5)).unwrap();
    order.start(at(6)).unwrap();
    let fact = MaterialFactId::try_from_uuid(uuid(21)).unwrap();
    assert!(
        order
            .issue_material(fact, 10, quantity(2_000), at(7))
            .unwrap()
    );
    assert!(
        !order
            .issue_material(fact, 10, quantity(2_000), at(7))
            .unwrap()
    );
}

#[test]
fn process_order_tracks_genealogy_quality_and_close_reconciliation() {
    let spec = specification(ManufacturingMode::Process);
    let mut order = ProductionOrder::draft(
        ProductionOrderId::try_from_uuid(uuid(30)).unwrap(),
        scope(),
        &spec,
        quantity(1_000),
        CorrelationId::try_from_uuid(uuid(29)).unwrap(),
        at(3),
    )
    .unwrap();
    let command_id = CommandId::try_from_uuid(uuid(103)).unwrap();
    let decision = human_decision(
        production_order_release_decision_ref(order.id(), order.version(), command_id),
        at(4),
    );
    order.release(command_id, &decision).unwrap();
    order.reserve_material(10, quantity(2_000), at(5)).unwrap();
    order.start(at(6)).unwrap();
    order
        .issue_material(
            MaterialFactId::try_from_uuid(uuid(31)).unwrap(),
            10,
            quantity(2_000),
            at(7),
        )
        .unwrap();
    let result_id = ResultFactId::try_from_uuid(uuid(32)).unwrap();
    order
        .report_result(ProductionResult {
            id: result_id,
            item: record(11, ContextId::Inventory, "inventory.lot"),
            lot_or_serial: "FG-LOT-1".into(),
            good: quantity(950),
            scrap: quantity(50),
            is_co_product: false,
            source_lots: vec!["RM-LOT-1".into()],
            at: at(8),
        })
        .unwrap();
    order
        .report_labor(LaborResult {
            id: LaborFactId::try_from_uuid(uuid(33)).unwrap(),
            routing_sequence: 10,
            labor_seconds: 600,
            machine_seconds: 700,
            at: at(8),
        })
        .unwrap();
    order
        .apply_disposition(
            QualityDisposition {
                id: DispositionId::try_from_uuid(uuid(34)).unwrap(),
                result_id,
                quantity: quantity(950),
                disposition: Disposition::Accepted,
                authority_record: record(35, ContextId::Quality, "quality.inspection"),
                source_version: 2,
            },
            at(9),
        )
        .unwrap();
    order
        .record_cost(
            ManufacturingCostFact {
                id: CostFactId::try_from_uuid(uuid(36)).unwrap(),
                amount: MoneyAmount::new(50_000, CurrencyCode::try_new("THB").unwrap()),
                category: "conversion".into(),
                owner_record: record(37, ContextId::Finance, "finance.journal"),
                owner_version: 1,
                reconciled: false,
            },
            at(9),
        )
        .unwrap();
    order.complete(at(10)).unwrap();
    let close_command = CommandId::try_from_uuid(uuid(104)).unwrap();
    let close_decision = human_decision(
        production_order_close_decision_ref(order.id(), order.version(), close_command),
        at(11),
    );
    assert_eq!(
        order.close(close_command, &close_decision),
        Err(ManufacturingDomainError::IncompleteReconciliation)
    );
}

#[test]
fn fixed_precision_and_mode_specific_roles_are_enforced() {
    let mut spec = specification(ManufacturingMode::Discrete);
    let command_id = CommandId::try_from_uuid(uuid(105)).unwrap();
    let decision = human_decision(
        specification_publish_decision_ref(
            spec.reference().specification_id,
            spec.reference().version,
            command_id,
        ),
        at(3),
    );
    assert_eq!(
        spec.release(command_id, &decision),
        Err(ManufacturingDomainError::InvalidTransition)
    );
    let bad = ManufacturingSpecification::draft(
        SpecificationId::try_from_uuid(uuid(40)).unwrap(),
        scope(),
        ManufacturingMode::Discrete,
        record(41, ContextId::Inventory, "inventory.lot"),
        quantity(1_000),
        vec![MaterialLine {
            line: 10,
            item: record(42, ContextId::Inventory, "inventory.lot"),
            quantity: quantity(1_000),
            role: MaterialRole::ProcessInput,
            expected_scrap_basis_points: 0,
        }],
        RoutingId::try_from_uuid(uuid(43)).unwrap(),
        vec![RoutingStep {
            sequence: 10,
            operation_code: "CUT".into(),
            work_center: record(44, ContextId::Assets, "asset.asset"),
            setup_seconds: 0,
            run_seconds_per_output: 1,
        }],
        at(1),
    );
    assert_eq!(bad, Err(ManufacturingDomainError::InvalidMaterialRole));
}
