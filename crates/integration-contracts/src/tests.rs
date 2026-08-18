use chrono::{TimeZone, Utc};
use erpx_kernel::{AggregateId, CorrelationId, EventId, TenantScope};
use serde_json::json;
use uuid::Uuid;

use super::*;

fn uuid(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn marketing_event() -> marketing::Event {
    marketing::Event::LeadQualified(marketing::QualifiedLeadFact {
        candidate: marketing::LeadCandidateRef::try_from_uuid(uuid(10)).unwrap(),
        campaign: marketing::CampaignRef::try_from_uuid(uuid(11)).unwrap(),
        audience_member: marketing::AudienceMemberRef::try_from_uuid(uuid(12)).unwrap(),
        existing_customer: None,
        source: ReferenceCode::try_new("webinar").unwrap(),
        qualification_score_basis_points: 8_500,
    })
}

fn envelope() -> IntegrationEventEnvelope<marketing::Event> {
    IntegrationEventEnvelope::try_new(
        EventId::try_from_uuid(uuid(1)).unwrap(),
        TenantScope::Company {
            holding_id: uuid(2),
            company_id: uuid(3),
        },
        AggregateKind::try_new("campaign_audience_member").unwrap(),
        AggregateId::try_from_uuid(uuid(4)).unwrap(),
        7,
        CorrelationId::try_from_uuid(uuid(5)).unwrap(),
        Some(EventId::try_from_uuid(uuid(6)).unwrap()),
        Utc.with_ymd_and_hms(2026, 8, 5, 10, 30, 0).unwrap(),
        marketing_event(),
    )
    .unwrap()
}

#[test]
fn typed_envelope_round_trips_without_erasing_its_owner() {
    let original = envelope();
    let encoded = serde_json::to_string(&original).unwrap();
    let decoded: IntegrationEventEnvelope<marketing::Event> =
        serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded, original);
    assert_eq!(decoded.owner(), BoundedContext::Marketing);
    assert_eq!(decoded.event_type(), "marketing.lead_qualified");
    assert_eq!(decoded.event_version(), 1);
    decoded.validate().unwrap();
}

#[test]
fn envelope_rejects_zero_versions_and_typed_metadata_mismatches() {
    let mut wire = serde_json::to_value(envelope()).unwrap();
    wire["aggregate_version"] = json!(0);
    let decoded: IntegrationEventEnvelope<marketing::Event> = serde_json::from_value(wire).unwrap();
    assert_eq!(
        decoded.validate(),
        Err(ContractValidationError::ZeroVersion("aggregate_version"))
    );

    let mut wire = serde_json::to_value(envelope()).unwrap();
    wire["owner"] = json!("crm");
    let decoded: IntegrationEventEnvelope<marketing::Event> = serde_json::from_value(wire).unwrap();
    assert_eq!(
        decoded.validate(),
        Err(ContractValidationError::OwnerMismatch)
    );

    let mut wire = serde_json::to_value(envelope()).unwrap();
    wire["event_type"] = json!("crm.lead_accepted");
    let decoded: IntegrationEventEnvelope<marketing::Event> = serde_json::from_value(wire).unwrap();
    assert_eq!(
        decoded.validate(),
        Err(ContractValidationError::EventTypeMismatch)
    );
}

#[test]
fn invalid_scope_payload_and_nil_owned_references_fail_closed() {
    let invalid_scope = IntegrationEventEnvelope::try_new(
        EventId::try_from_uuid(uuid(1)).unwrap(),
        TenantScope::Subtree {
            holding_id: uuid(2),
            company_ids: vec![],
            branch_ids: vec![],
        },
        AggregateKind::try_new("lead_candidate").unwrap(),
        AggregateId::try_from_uuid(uuid(4)).unwrap(),
        1,
        CorrelationId::try_from_uuid(uuid(5)).unwrap(),
        None,
        Utc.with_ymd_and_hms(2026, 8, 5, 10, 30, 0).unwrap(),
        marketing_event(),
    );
    assert_eq!(
        invalid_scope,
        Err(ContractValidationError::InvalidTenantScope)
    );

    let mut event = marketing_event();
    let marketing::Event::LeadQualified(fact) = &mut event else {
        unreachable!();
    };
    fact.qualification_score_basis_points = 10_001;
    assert_eq!(
        event.validate(),
        Err(ContractValidationError::OutOfRange(
            "qualification_score_basis_points"
        ))
    );

    let nil = "\"00000000-0000-0000-0000-000000000000\"";
    assert!(serde_json::from_str::<customer::CustomerRef>(nil).is_err());
    assert!(serde_json::from_str::<commercial::SubscriptionRef>(nil).is_err());
}

#[test]
fn composed_read_inputs_keep_their_source_owner_explicit() {
    let fact = customer::CustomerProfileFact {
        customer: customer::CustomerRef::try_from_uuid(uuid(20)).unwrap(),
        account: customer::CustomerAccountRef::try_from_uuid(uuid(21)).unwrap(),
        customer_code: ReferenceCode::try_new("C-100").unwrap(),
        display_name: DisplayName::try_new("Acme").unwrap(),
        owning_company: organization::CompanyRef::try_from_uuid(uuid(22)).unwrap(),
        status: customer::CustomerStatus::Active,
    };
    let source = operations::SourceFact::Customer(fact);
    let wire = serde_json::to_value(&source).unwrap();

    assert_eq!(source.source_context(), BoundedContext::Customer);
    assert_eq!(wire["source"], json!("customer"));

    let mut ambiguous_wire = wire;
    ambiguous_wire["source"] = json!("commercial");
    assert!(serde_json::from_value::<operations::SourceFact>(ambiguous_wire).is_err());
}

#[test]
fn bounded_scalar_deserialization_cannot_bypass_validation() {
    assert!(serde_json::from_str::<ReferenceCode>("\"\"").is_err());
    assert!(serde_json::from_str::<CurrencyCode>("\"thb\"").is_err());
    assert!(serde_json::from_str::<Quantity>(r#"{"units":1,"scale":10}"#).is_err());
}

fn dimensions() -> DimensionSet {
    let mut value =
        DimensionSet::for_legal_entity(organization::CompanyRef::try_from_uuid(uuid(40)).unwrap());
    value.cost_center = Some(CostCenterRef::try_from_uuid(uuid(41)).unwrap());
    value.campaign = Some(marketing::CampaignRef::try_from_uuid(uuid(42)).unwrap());
    value
}

#[test]
fn billing_eligibility_is_not_a_receivable_and_carries_accounting_dimensions() {
    let currency = CurrencyCode::try_new("THB").unwrap();
    let event = sales::Event::BillingEligible(sales::BillingEligibilityFact {
        billing_event: sales::BillingEventRef::try_from_uuid(uuid(43)).unwrap(),
        fulfillment: sales::FulfillmentRef::try_from_uuid(uuid(44)).unwrap(),
        arrangement: sales::RevenueArrangementRef::try_from_uuid(uuid(45)).unwrap(),
        sales_order: sales::SalesOrderRef::try_from_uuid(uuid(46)).unwrap(),
        customer: customer::CustomerRef::try_from_uuid(uuid(47)).unwrap(),
        order_code: ReferenceCode::try_new("SO-43").unwrap(),
        billable_amount: MoneyAmount::new(10_000, currency),
        tax_amount: MoneyAmount::new(700, currency),
        eligible_on: chrono::NaiveDate::from_ymd_opt(2026, 8, 10).unwrap(),
        dimensions: dimensions(),
    });

    event.validate().unwrap();
    assert_eq!(event.event_type(), "sales.billing_eligible");
    let encoded = serde_json::to_string(&event).unwrap();
    assert!(!encoded.contains("receivable"));
    assert!(encoded.contains("cost_center"));
    assert!(encoded.contains("campaign"));
}

#[test]
fn quote_and_finance_approval_requests_pin_subject_versions_and_typed_gates() {
    let requested_at = Utc.with_ymd_and_hms(2026, 8, 10, 8, 0, 0).unwrap();
    let quote = sales::QuoteRef::try_from_uuid(uuid(70)).unwrap();
    let request = operations::ApprovalSessionRequestFact {
        session: operations::ApprovalSessionRef::try_from_uuid(uuid(71)).unwrap(),
        subject: RecordRef::try_new(BoundedContext::Sales, "quote", quote.as_uuid(), 4).unwrap(),
        policy_version: 2,
        required_gates: vec![
            operations::ApprovalGateKind::SalesCommercial,
            operations::ApprovalGateKind::FinanceAccountingPlan,
        ],
        requested_by: ReferenceCode::try_new("sales-maker").unwrap(),
        requested_at,
    };
    sales::Event::QuoteApprovalRequested(sales::QuoteApprovalRequestedFact {
        quote,
        approval: request,
    })
    .validate()
    .unwrap();

    let plan = finance::AccountingPlanRef::try_from_uuid(uuid(72)).unwrap();
    let finance_request = operations::ApprovalSessionRequestFact {
        session: operations::ApprovalSessionRef::try_from_uuid(uuid(73)).unwrap(),
        subject: RecordRef::try_new(
            BoundedContext::Finance,
            "accounting_plan",
            plan.as_uuid(),
            3,
        )
        .unwrap(),
        policy_version: 2,
        required_gates: vec![operations::ApprovalGateKind::FinanceAccountingPlan],
        requested_by: ReferenceCode::try_new("finance-preparer").unwrap(),
        requested_at,
    };
    finance::Event::AccountingPlanApprovalRequested(finance::AccountingPlanApprovalRequestedFact {
        plan,
        quote,
        approval: finance_request,
    })
    .validate()
    .unwrap();
}

#[test]
fn attribution_revision_must_reconcile_exactly_to_its_finance_source() {
    let currency = CurrencyCode::try_new("THB").unwrap();
    let source =
        RecordRef::try_new(BoundedContext::Finance, "recognized_revenue", uuid(50), 2).unwrap();
    let mut event =
        marketing::Event::AttributionRevisionPublished(marketing::AttributionRevisionFact {
            revision: marketing::AttributionRevisionRef::try_from_uuid(uuid(51)).unwrap(),
            basis: marketing::AttributionBasis::Recognized,
            revenue_source: source,
            source_total: MoneyAmount::new(101, currency),
            allocations: vec![
                marketing::AttributedCampaignAmount {
                    campaign: marketing::CampaignRef::try_from_uuid(uuid(52)).unwrap(),
                    amount: MoneyAmount::new(51, currency),
                },
                marketing::AttributedCampaignAmount {
                    campaign: marketing::CampaignRef::try_from_uuid(uuid(53)).unwrap(),
                    amount: MoneyAmount::new(50, currency),
                },
            ],
            published_at: Utc.with_ymd_and_hms(2026, 8, 10, 8, 0, 0).unwrap(),
        });
    event.validate().unwrap();

    let marketing::Event::AttributionRevisionPublished(fact) = &mut event else {
        unreachable!();
    };
    fact.allocations[1].amount = MoneyAmount::new(49, currency);
    assert_eq!(
        event.validate(),
        Err(ContractValidationError::OutOfRange(
            "attribution.allocated_total"
        ))
    );
}

#[test]
fn support_incident_fact_is_secret_free_and_operations_keeps_support_ownership() {
    let source = operations::SourceFact::SupportIncident(support::SupportIncidentFact {
        incident: support::SupportIncidentRef::try_from_uuid(uuid(30)).unwrap(),
        incident_code: ReferenceCode::try_new("INC-30").unwrap(),
        status: support::SupportIncidentStatus::Monitoring,
        severity: ReferenceCode::try_new("sev-2").unwrap(),
        affected_services: vec![ReferenceCode::try_new("billing").unwrap()],
        affected_customer_accounts: vec![
            customer::CustomerAccountRef::try_from_uuid(uuid(31)).unwrap(),
        ],
        updated_at: Utc.with_ymd_and_hms(2026, 8, 5, 11, 0, 0).unwrap(),
    });

    let wire = serde_json::to_value(&source).unwrap();
    let encoded = wire.to_string();
    assert_eq!(source.source_context(), BoundedContext::Support);
    assert_eq!(wire["source"], json!("support_incident"));
    for forbidden in [
        "capability",
        "credential",
        "secret",
        "session",
        "timeline",
        "approval",
    ] {
        assert!(!encoded.contains(forbidden), "leaked {forbidden}");
    }
}
