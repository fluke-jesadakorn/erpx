//! Pure customer ticket-portal rule checks. No database or TS writer is used.
use erpx_server_support::public::ticket_portal::{
    BindingStatus, DecisionAction, PortalActor, PortalBindingRow, PortalCustomer, PortalPlan,
    PortalResponse, PortalRights, PortalTicket, binding_view, parse_acknowledgement,
    parse_decision, parse_prepare, parse_response, plan_acknowledgement, plan_decision,
    plan_prepare, plan_response, validate_command_key,
};
use serde_json::json;

fn actor(user: &str, manage: bool) -> PortalActor {
    PortalActor {
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        user_id: user.into(),
        assignment_id: "assignment-1".into(),
        support_manage: manage,
    }
}

fn ticket() -> PortalTicket {
    PortalTicket {
        id: "ticket-1".into(),
        version: 4,
        title: "Printer offline".into(),
        status: "open".into(),
        source_kind: "general".into(),
    }
}

fn binding(status: BindingStatus, version: i64) -> PortalBindingRow {
    PortalBindingRow {
        id: "support-portal-command-1".into(),
        company_id: "company".into(),
        ticket_id: "ticket-1".into(),
        ticket_version: 4,
        customer_name: "Customer".into(),
        email: "customer@example.test".into(),
        subject: "Printer update".into(),
        summary: "We are reviewing the printer issue.".into(),
        expires_at: 5_000,
        status,
        version,
        prepared_by: "preparer".into(),
    }
}

#[test]
fn prepare_binds_current_ticket_and_future_expiry() {
    let input = parse_prepare(&json!({
        "ticketId":"ticket-1","ticketVersion":4,"customerName":" Customer ",
        "email":"Customer@Example.Test","subject":"Printer update",
        "summary":"We are reviewing the printer issue.","expiresAt":6000,
        "reason":"Customer asked for an update"
    }))
    .unwrap();
    assert_eq!(input.email, "customer@example.test");
    let plan = plan_prepare(
        &actor("manager", true),
        &input,
        &ticket(),
        "command-0001",
        5_000,
    )
    .unwrap();
    match plan {
        PortalPlan::Prepare {
            binding,
            event,
            result,
        } => {
            assert_eq!(binding.status, BindingStatus::Draft);
            assert_eq!(binding.version, 1);
            assert_eq!(event.action, "prepare");
            assert_eq!(result.id, "support-portal-command-0001");
        }
        _ => panic!("expected prepare plan"),
    }
}

#[test]
fn decisions_preserve_independent_review_and_cas() {
    let approve = parse_decision(&json!({
        "expectedVersion":1,"action":"approve","reason":"Reviewed the summary"
    }))
    .unwrap();
    assert_eq!(approve.action, DecisionAction::Approve);
    let plan = plan_decision(
        &actor("reviewer", true),
        &binding(BindingStatus::Draft, 1),
        &ticket(),
        &approve,
        "decision-0001",
        1_000,
    )
    .unwrap();
    assert!(matches!(
        plan,
        PortalPlan::Decide {
            status: BindingStatus::Approved,
            version: 2,
            ..
        }
    ));
    assert!(matches!(
        plan_decision(
            &actor("preparer", true),
            &binding(BindingStatus::Draft, 1),
            &ticket(),
            &approve,
            "decision-0002",
            1_000
        ),
        Err(error) if error.code == "INDEPENDENT_SUPPORT_PORTAL_REVIEW_REQUIRED"
    ));
    assert!(matches!(
        plan_decision(
            &actor("reviewer", true),
            &binding(BindingStatus::Draft, 1),
            &ticket(),
            &approve,
            "decision-0003",
            6_000
        ),
        Err(error) if error.code == "SUPPORT_PORTAL_BINDING_EXPIRED"
    ));
}

#[test]
fn response_and_acknowledgement_keep_exact_versions() {
    let response = parse_response(&json!({
        "expectedBindingVersion":2,"expectedResponseVersion":0,
        "message":"The printer is repaired.","reason":"Engineer confirmed repair"
    }))
    .unwrap();
    let plan = plan_response(
        &actor("reviewer", true),
        &binding(BindingStatus::Approved, 2),
        0,
        &response,
        "response-0001",
        1_000,
    )
    .unwrap();
    assert!(matches!(
        plan,
        PortalPlan::Respond { ref result, .. }
            if result.version == 1 && result.status == "published"
    ));
    let customer = PortalCustomer {
        user_id: "customer-user".into(),
        email: "Customer@Example.Test".into(),
        holding_id: Some("h001".into()),
    };
    let published = PortalResponse {
        id: "support-portal-response-response-0001".into(),
        version: 1,
        message: "The printer is repaired.".into(),
        published_at: 1_000,
    };
    let ack = parse_acknowledgement(&json!({
        "expectedBindingVersion":2,
        "responseId":"support-portal-response-response-0001",
        "responseVersion":1,"note":"Thank you"
    }))
    .unwrap();
    assert!(matches!(
        plan_acknowledgement(
            &customer,
            &binding(BindingStatus::Approved, 2),
            &published,
            &ack,
            "ack-00000001",
            1_100
        ),
        Ok(PortalPlan::Acknowledge { ref result, .. }) if result.status == "acknowledged"
    ));
}

#[test]
fn command_key_and_presentation_flags_fail_closed() {
    assert!(validate_command_key("short").is_err());
    assert!(validate_command_key("support:command-0001").is_ok());
    let view = binding_view(
        &actor("reviewer", false),
        &binding(BindingStatus::Approved, 2),
        &PortalRights {
            new_work: true,
            settle_existing: true,
        },
        None,
        None,
        1_000,
    );
    assert!(!view.can_revoke && !view.can_respond);
}
