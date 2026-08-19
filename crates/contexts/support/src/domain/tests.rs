use chrono::{Duration, TimeZone, Utc};
use erpx_integration_contracts::{
    DisplayName, ReferenceCode, customer, identity_access, organization,
};
use erpx_kernel::AuthorityScope;
use uuid::Uuid;

use super::*;
use crate::ports::{SupportCapabilityView, SupportTicketView, TicketReadAudience};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}
fn scope() -> AuthorityScope {
    AuthorityScope::Company {
        holding_id: id(1),
        company_id: id(2),
    }
}
fn customer_fact(status: customer::CustomerStatus) -> customer::CustomerProfileFact {
    customer::CustomerProfileFact {
        customer: customer::CustomerRef::try_from_uuid(id(10)).unwrap(),
        account: customer::CustomerAccountRef::try_from_uuid(id(11)).unwrap(),
        customer_code: ReferenceCode::try_new("CUS-10").unwrap(),
        display_name: DisplayName::try_new("Acme").unwrap(),
        owning_company: organization::CompanyRef::try_from_uuid(id(2)).unwrap(),
        status,
    }
}
fn ticket() -> SupportTicket {
    SupportTicket::open(
        SupportTicketId::try_from_uuid(id(20)).unwrap(),
        scope(),
        customer_fact(customer::CustomerStatus::Active),
        "SUP-20".into(),
        "identity".into(),
        "high".into(),
        "cannot authenticate".into(),
        at(),
    )
    .unwrap()
}
fn access_requested_ticket() -> SupportTicket {
    let mut value = ticket();
    value.take_uncommitted_events();
    value.transition(TicketStatus::Triaged, at()).unwrap();
    value
        .transition(TicketStatus::AccessRequested, at())
        .unwrap();
    value
}
fn principal(value: u128) -> identity_access::PrincipalRef {
    identity_access::PrincipalRef::try_from_uuid(id(value)).unwrap()
}
fn approved_session(mode: SupportMode) -> SupportSession {
    let represented = (mode == SupportMode::Impersonation).then(|| principal(31));
    let mut value = SupportSession::request(
        SessionId::try_from_uuid(id(30)).unwrap(),
        &access_requested_ticket(),
        principal(32),
        represented,
        mode,
        vec!["holding.read".into(), "holding.write".into()],
        scope(),
        "support".into(),
        "customer approved investigation".into(),
        Duration::minutes(30),
        at(),
    )
    .unwrap();
    value
        .approve(
            principal(33).as_uuid().to_string(),
            at() + Duration::minutes(1),
        )
        .unwrap();
    value
}

#[test]
fn ticket_requires_eligible_customer_and_owning_company_scope() {
    assert!(matches!(
        SupportTicket::open(
            SupportTicketId::try_from_uuid(id(1)).unwrap(),
            scope(),
            customer_fact(customer::CustomerStatus::Blocked),
            "SUP-1".into(),
            "access".into(),
            "high".into(),
            "x".into(),
            at()
        ),
        Err(SupportDomainError::CustomerUnavailable)
    ));
    let foreign = AuthorityScope::Company {
        holding_id: id(1),
        company_id: id(99),
    };
    assert!(matches!(
        SupportTicket::open(
            SupportTicketId::try_from_uuid(id(1)).unwrap(),
            foreign,
            customer_fact(customer::CustomerStatus::Active),
            "SUP-1".into(),
            "access".into(),
            "high".into(),
            "x".into(),
            at()
        ),
        Err(SupportDomainError::InvalidScope)
    ));
}

#[test]
fn ticket_lifecycle_is_ordered_and_closed_is_terminal() {
    let mut value = ticket();
    assert!(matches!(
        value.transition(TicketStatus::Resolved, at()),
        Err(SupportDomainError::InvalidTicketTransition)
    ));
    value.transition(TicketStatus::Triaged, at()).unwrap();
    value.transition(TicketStatus::InProgress, at()).unwrap();
    value.transition(TicketStatus::Resolved, at()).unwrap();
    value
        .transition(TicketStatus::CustomerConfirmed, at())
        .unwrap();
    value.transition(TicketStatus::Closed, at()).unwrap();
    assert!(matches!(
        value.add_message(
            TicketMessage::new(
                MessageId::try_from_uuid(id(70)).unwrap(),
                "agent".into(),
                MessageAudience::Internal,
                "secret note".into(),
                at()
            )
            .unwrap(),
            at()
        ),
        Err(SupportDomainError::ClosedTicket)
    ));
}

#[test]
fn customer_ticket_view_cannot_encode_internal_notes() {
    let mut value = ticket();
    value
        .add_message(
            TicketMessage::new(
                MessageId::try_from_uuid(id(71)).unwrap(),
                "customer".into(),
                MessageAudience::CustomerVisible,
                "public reply".into(),
                at(),
            )
            .unwrap(),
            at(),
        )
        .unwrap();
    value
        .add_message(
            TicketMessage::new(
                MessageId::try_from_uuid(id(72)).unwrap(),
                "agent".into(),
                MessageAudience::Internal,
                "private diagnosis".into(),
                at(),
            )
            .unwrap(),
            at(),
        )
        .unwrap();
    let view = SupportTicketView::from_aggregate(&value, TicketReadAudience::Customer);
    assert_eq!(view.customer_messages.len(), 1);
    assert!(view.internal_messages.is_none());
    let json = serde_json::to_string(&view).unwrap();
    assert!(json.contains("public reply"));
    assert!(!json.contains("private diagnosis"));
}

#[test]
fn impersonation_requires_a_represented_principal_and_distinct_approver() {
    let ticket = access_requested_ticket();
    let result = SupportSession::request(
        SessionId::try_from_uuid(id(30)).unwrap(),
        &ticket,
        principal(32),
        None,
        SupportMode::Impersonation,
        vec!["holding.read".into()],
        scope(),
        "support".into(),
        "reason".into(),
        Duration::minutes(30),
        at(),
    );
    assert!(matches!(
        result,
        Err(SupportDomainError::InvalidRepresentedPrincipal)
    ));
    let mut session = SupportSession::request(
        SessionId::try_from_uuid(id(30)).unwrap(),
        &ticket,
        principal(32),
        Some(principal(31)),
        SupportMode::Impersonation,
        vec!["holding.read".into()],
        scope(),
        "support".into(),
        "reason".into(),
        Duration::minutes(30),
        at(),
    )
    .unwrap();
    assert!(matches!(
        session.approve(principal(32).as_uuid().to_string(), at()),
        Err(SupportDomainError::ApprovalSeparation)
    ));
}

#[test]
fn high_risk_capability_requires_current_mfa_sod_and_contained_scope() {
    let session = approved_session(SupportMode::HoldingMutation);
    let base = CapabilityEvidence {
        device_ref: "device-1".into(),
        mfa_verified_at: Some(at()),
        segregation_of_duties_passed: true,
        authority_scope: scope(),
    };
    assert!(
        SupportCapability::issue(
            CapabilityId::try_from_uuid(id(40)).unwrap(),
            &session,
            base.clone(),
            [7; 32],
            at() + Duration::minutes(2)
        )
        .is_ok()
    );
    let mut stale = base.clone();
    stale.mfa_verified_at = Some(at() - Duration::minutes(4));
    assert!(matches!(
        SupportCapability::issue(
            CapabilityId::try_from_uuid(id(41)).unwrap(),
            &session,
            stale,
            [7; 32],
            at() + Duration::minutes(2)
        ),
        Err(SupportDomainError::MfaRequired)
    ));
    let mut failed_sod = base;
    failed_sod.segregation_of_duties_passed = false;
    assert!(matches!(
        SupportCapability::issue(
            CapabilityId::try_from_uuid(id(42)).unwrap(),
            &session,
            failed_sod,
            [7; 32],
            at() + Duration::minutes(2)
        ),
        Err(SupportDomainError::SegregationOfDutiesDenied)
    ));
}

#[test]
fn support_access_never_expands_or_narrows_the_customer_approved_scope() {
    let ticket = access_requested_ticket();
    let branch = AuthorityScope::Branch {
        holding_id: id(1),
        company_id: id(2),
        branch_id: id(3),
    };
    assert!(matches!(
        SupportSession::request(
            SessionId::try_from_uuid(id(35)).unwrap(),
            &ticket,
            principal(32),
            None,
            SupportMode::DiagnosticRead,
            vec!["holding.read".into()],
            branch.clone(),
            "support".into(),
            "customer approved investigation".into(),
            Duration::minutes(30),
            at(),
        ),
        Err(SupportDomainError::InvalidSession)
    ));

    let session = approved_session(SupportMode::DiagnosticRead);
    let evidence = CapabilityEvidence {
        device_ref: "device-1".into(),
        mfa_verified_at: None,
        segregation_of_duties_passed: true,
        authority_scope: scope(),
    };
    let capability = SupportCapability::issue(
        CapabilityId::try_from_uuid(id(45)).unwrap(),
        &session,
        evidence.clone(),
        [4; 32],
        at() + Duration::minutes(2),
    )
    .unwrap();
    assert!(matches!(
        capability.authorize(
            &session,
            "holding.read",
            &branch,
            &evidence,
            at() + Duration::minutes(3),
        ),
        Err(SupportDomainError::InvalidCapabilityBinding)
    ));
}

#[test]
fn revoked_or_expired_capability_never_authorizes() {
    let session = approved_session(SupportMode::DiagnosticRead);
    let evidence = CapabilityEvidence {
        device_ref: "device-1".into(),
        mfa_verified_at: None,
        segregation_of_duties_passed: true,
        authority_scope: scope(),
    };
    let mut capability = SupportCapability::issue(
        CapabilityId::try_from_uuid(id(40)).unwrap(),
        &session,
        evidence.clone(),
        [9; 32],
        at() + Duration::minutes(2),
    )
    .unwrap();
    capability
        .authorize(
            &session,
            "holding.read",
            &scope(),
            &evidence,
            at() + Duration::minutes(3),
        )
        .unwrap();
    capability
        .revoke(
            CapabilityRevocationReason::CustomerRequested,
            at() + Duration::minutes(4),
        )
        .unwrap();
    assert!(matches!(
        capability.authorize(
            &session,
            "holding.read",
            &scope(),
            &evidence,
            at() + Duration::minutes(4)
        ),
        Err(SupportDomainError::CapabilityInactive)
    ));
    assert!(matches!(
        capability.authorize(
            &session,
            "holding.read",
            &scope(),
            &evidence,
            session.expires_at()
        ),
        Err(SupportDomainError::CapabilityInactive)
    ));
}

#[test]
fn capability_debug_and_view_serialization_never_expose_secret_or_hash() {
    let session = approved_session(SupportMode::DiagnosticRead);
    let evidence = CapabilityEvidence {
        device_ref: "device-1".into(),
        mfa_verified_at: None,
        segregation_of_duties_passed: true,
        authority_scope: scope(),
    };
    let capability = SupportCapability::issue(
        CapabilityId::try_from_uuid(id(40)).unwrap(),
        &session,
        evidence,
        [171; 32],
        at() + Duration::minutes(2),
    )
    .unwrap();
    let debug = format!("{capability:?}");
    assert!(debug.contains("REDACTED"));
    assert!(!debug.contains("171"));
    let view = SupportCapabilityView::from(&capability);
    let json = serde_json::to_string(&view).unwrap();
    assert!(!json.contains("secret"));
    assert!(!json.contains("hash"));
    assert!(!json.contains("credential"));
    assert!(!json.contains("171"));
}

#[test]
fn incident_audiences_are_explicit_and_postmortem_is_terminal() {
    let mut incident = Incident::open(
        IncidentId::try_from_uuid(id(50)).unwrap(),
        scope(),
        "INC-50".into(),
        "critical".into(),
        "commander".into(),
        vec!["api".into()],
        vec![customer::CustomerAccountRef::try_from_uuid(id(11)).unwrap()],
        at(),
    )
    .unwrap();
    incident
        .broadcast(
            "commander".into(),
            BroadcastAudience::CustomerVisible,
            "investigating".into(),
            at(),
        )
        .unwrap();
    incident
        .transition(IncidentStatus::Identified, "commander".into(), at())
        .unwrap();
    incident
        .transition(IncidentStatus::Monitoring, "commander".into(), at())
        .unwrap();
    incident
        .transition(IncidentStatus::Resolved, "commander".into(), at())
        .unwrap();
    incident
        .transition(IncidentStatus::PostmortemComplete, "commander".into(), at())
        .unwrap();
    assert!(matches!(
        incident.broadcast(
            "commander".into(),
            BroadcastAudience::Internal,
            "hidden".into(),
            at()
        ),
        Err(SupportDomainError::IncidentTerminal)
    ));
}
