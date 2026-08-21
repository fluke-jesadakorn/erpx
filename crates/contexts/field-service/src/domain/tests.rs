use std::collections::BTreeSet;

use chrono::{NaiveDate, TimeZone, Utc};
use erpx_integration_contracts::{
    CurrencyCode, MoneyAmount, Quantity, RecordVersionRef, StableRecordRef, catalog::ContextId,
    reference::RecordTypeId,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuthorityScope, CommandId, CorrelationId, HumanDecisionProof,
    PrincipalId, RecordId, SessionId,
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
fn day(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 8, day).unwrap()
}
fn scope() -> AuthorityScope {
    AuthorityScope::Branch {
        holding_id: uuid(1),
        company_id: uuid(2),
        branch_id: uuid(3),
    }
}
fn command_id(value: u128) -> CommandId {
    CommandId::try_from_uuid(uuid(value)).unwrap()
}
fn human_decision(reference: String, at: chrono::DateTime<Utc>) -> HumanDecisionProof {
    HumanDecisionProof::try_new(
        ActorAttribution {
            principal_ref: uuid(8).to_string(),
            kind: ActorKind::HoldingUser,
            session_id: Some(SessionId::try_from_uuid(uuid(9)).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope(),
        reference,
        at,
    )
    .unwrap()
}
fn record(id: u128, owner: ContextId, kind: &str) -> StableRecordRef {
    StableRecordRef::try_new(
        RecordId::try_from_uuid(uuid(id)).unwrap(),
        owner,
        RecordTypeId::try_new(kind).unwrap(),
    )
    .unwrap()
}
fn pin(id: u128, owner: ContextId, kind: &str, version: u64) -> RecordVersionRef {
    RecordVersionRef::try_new(record(id, owner, kind), version).unwrap()
}
fn qty(value: i64) -> Quantity {
    Quantity::try_new(value, 3).unwrap()
}
fn setup() -> (ServiceAgreement, ServiceRequest) {
    let customer = pin(10, ContextId::CustomerAuthority, "customer.account", 2);
    let asset = pin(11, ContextId::Assets, "asset.asset", 4);
    let location = record(12, ContextId::Organization, "organization.branch");
    let agreement_id = ServiceAgreementId::try_from_uuid(uuid(13)).unwrap();
    let command = command_id(100);
    let agreement = ServiceAgreement::activate(
        agreement_id,
        scope(),
        customer.clone(),
        BTreeSet::from([asset.record().clone()]),
        BTreeSet::from([location.clone()]),
        BTreeSet::from(["repair".into()]),
        day(1),
        day(31),
        &human_decision(
            agreement_activate_decision_ref(command, agreement_id),
            at(1),
        ),
        command,
        at(1),
    )
    .unwrap();
    let request = ServiceRequest {
        id: ServiceRequestId::try_from_uuid(uuid(14)).unwrap(),
        version: 1,
        scope: scope(),
        customer,
        asset,
        location,
        service_code: "repair".into(),
        requested_at: at(1),
    };
    (agreement, request)
}

#[test]
fn agreement_activation_rejects_unbound_human_evidence() {
    let customer = pin(110, ContextId::CustomerAuthority, "customer.account", 2);
    let asset = pin(111, ContextId::Assets, "asset.asset", 4);
    let location = record(112, ContextId::Organization, "organization.branch");
    let agreement_id = ServiceAgreementId::try_from_uuid(uuid(113)).unwrap();
    let command = command_id(114);
    let result = ServiceAgreement::activate(
        agreement_id,
        scope(),
        customer,
        BTreeSet::from([asset.record().clone()]),
        BTreeSet::from([location]),
        BTreeSet::from(["repair".into()]),
        day(1),
        day(31),
        &human_decision("field-service.agreement.activate:wrong".into(), at(1)),
        command,
        at(1),
    );
    assert_eq!(result, Err(FieldServiceDomainError::HumanDecisionRequired));
}
fn scheduled() -> (ServiceAgreement, ServiceOrder) {
    let (agreement, request) = setup();
    let mut order = ServiceOrder::create(
        ServiceOrderId::try_from_uuid(uuid(20)).unwrap(),
        &request,
        &agreement,
        BTreeSet::from(["electrical".into()]),
        at(2),
    )
    .unwrap();
    order
        .schedule(
            Connectivity::Online,
            day(16),
            TechnicianQualification {
                technician: pin(21, ContextId::PeoplePayroll, "people.employee", 3),
                principal_id: PrincipalId::try_from_uuid(uuid(22)).unwrap(),
                skills: BTreeSet::from(["electrical".into()]),
                valid_through: day(30),
                evidence: pin(23, ContextId::Learning, "learning.completion", 2),
            },
            at(3),
        )
        .unwrap();
    order
        .reserve_part(
            Connectivity::Online,
            PartReservation {
                part: pin(24, ContextId::Inventory, "inventory.lot", 3),
                reserved: qty(2_000),
                issued: qty(0),
                inventory_reservation: pin(25, ContextId::Inventory, "inventory.reservation", 5),
            },
            at(4),
        )
        .unwrap();
    (agreement, order)
}

#[test]
fn mutations_are_online_only_and_skill_gate_is_fail_closed() {
    let (agreement, request) = setup();
    let mut order = ServiceOrder::create(
        ServiceOrderId::try_from_uuid(uuid(30)).unwrap(),
        &request,
        &agreement,
        BTreeSet::from(["electrical".into()]),
        at(2),
    )
    .unwrap();
    let qualification = TechnicianQualification {
        technician: pin(31, ContextId::PeoplePayroll, "people.employee", 3),
        principal_id: PrincipalId::try_from_uuid(uuid(32)).unwrap(),
        skills: BTreeSet::from(["mechanical".into()]),
        valid_through: day(30),
        evidence: pin(33, ContextId::Learning, "learning.completion", 2),
    };
    assert_eq!(
        order.schedule(Connectivity::Offline, day(16), qualification.clone(), at(3)),
        Err(FieldServiceDomainError::OnlineRequired)
    );
    assert_eq!(
        order.schedule(Connectivity::Online, day(16), qualification, at(3)),
        Err(FieldServiceDomainError::SkillOrScheduleDenied)
    );
}

#[test]
fn customer_and_asset_mismatch_require_explicit_exception() {
    let (agreement, mut order) = scheduled();
    order
        .dispatch(
            Connectivity::Online,
            DispatchId::try_from_uuid(uuid(40)).unwrap(),
            &agreement,
            at(5),
        )
        .unwrap();
    let wrong_customer = pin(41, ContextId::CustomerAuthority, "customer.account", 2);
    assert_eq!(
        order.start_visit(
            Connectivity::Online,
            VisitId::try_from_uuid(uuid(42)).unwrap(),
            &wrong_customer,
            &pin(11, ContextId::Assets, "asset.asset", 4),
            at(6)
        ),
        Err(FieldServiceDomainError::ExplicitMismatchRequired(
            ExceptionKind::CustomerMismatch
        ))
    );
    order
        .record_exception(
            Connectivity::Online,
            ServiceException {
                id: ExceptionId::try_from_uuid(uuid(43)).unwrap(),
                kind: ExceptionKind::CustomerMismatch,
                reason: "Customer identity did not match service request".into(),
                evidence: pin(44, ContextId::Collaboration, "collaboration.thread", 2),
                recorded_at: at(6),
            },
        )
        .unwrap();
    assert_eq!(order.status(), ServiceOrderStatus::Exception);
}

#[test]
fn parts_evidence_refusal_and_billing_handoff_are_explicit() {
    let (agreement, request) = setup();
    let (agreement2, mut order) = scheduled();
    assert_eq!(agreement.id(), agreement2.id());
    order
        .dispatch(
            Connectivity::Online,
            DispatchId::try_from_uuid(uuid(50)).unwrap(),
            &agreement,
            at(5),
        )
        .unwrap();
    order
        .start_visit(
            Connectivity::Online,
            VisitId::try_from_uuid(uuid(51)).unwrap(),
            &request.customer,
            &request.asset,
            at(6),
        )
        .unwrap();
    let part_id = request.asset.record().id().as_uuid();
    assert_eq!(
        order.issue_part(
            Connectivity::Online,
            PartFactId::try_from_uuid(uuid(52)).unwrap(),
            part_id,
            qty(1_000),
            at(7)
        ),
        Err(FieldServiceDomainError::InventoryGateDenied)
    );
    let part_record = pin(24, ContextId::Inventory, "inventory.lot", 3);
    assert!(
        order
            .issue_part(
                Connectivity::Online,
                PartFactId::try_from_uuid(uuid(53)).unwrap(),
                part_record.record().id().as_uuid(),
                qty(1_000),
                at(7)
            )
            .unwrap()
    );
    let signature = CustomerSignature::Refused {
        exception_id: ExceptionId::try_from_uuid(uuid(59)).unwrap(),
        reason: "Customer declined electronic signature".into(),
        witnessed_by: PrincipalId::try_from_uuid(uuid(54)).unwrap(),
        evidence: pin(55, ContextId::Collaboration, "collaboration.thread", 2),
    };
    order
        .complete_visit(
            Connectivity::Online,
            vec![pin(56, ContextId::Collaboration, "collaboration.thread", 3)],
            signature,
            at(8),
        )
        .unwrap();
    let handoff = order
        .billing_handoff(
            BillingHandoffId::try_from_uuid(uuid(57)).unwrap(),
            MoneyAmount::new(10_000, CurrencyCode::try_new("THB").unwrap()),
            CorrelationId::try_from_uuid(uuid(58)).unwrap(),
        )
        .unwrap();
    assert_eq!(handoff.service_order_version, order.version());
}
