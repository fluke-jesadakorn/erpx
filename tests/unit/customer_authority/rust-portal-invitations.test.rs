//! Native Customer Portal invitation issue/resend policy.
//!
//! Proves the retained decision rules: version, permission, identity binding,
//! approved/expiring/order-state source, one active invitation, resend
//! precondition, bounded token expiry, generation and supersession evidence.
use erpx_server_customer_authority::public::access::AccessActor;
use erpx_server_customer_authority::public::portal_invitation_delivery::{
    InvitationClaims, invitation_token, invitation_token_hash,
};
use erpx_server_customer_authority::public::portal_invitations::{
    INVITATION_LIFETIME_MILLIS, InvitationAction, InvitationBinding, InvitationCommand, issue_plan,
};
use erpx_server_shared::public::{AssignmentId, CompanyId, HoldingId, RecordId, UserId};

const SECRET: &str = "vector-secret-2026";
const NOW: i64 = 1_800_000_000_000;

fn actor(user: &str, permissions: &[&str]) -> AccessActor {
    AccessActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        assignment_id: AssignmentId::new("assignment").unwrap(),
        role: Some("sales_manager".into()),
        authority_revision: Some(7),
        assignment_version: Some(3),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        expires_at: NOW + 60_000,
    }
}

fn binding() -> InvitationBinding {
    InvitationBinding {
        id: "binding-1".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        email: "Customer@Example.invalid".into(),
        status: "approved".into(),
        order_id: "order-1".into(),
        order_state: "confirmed".into(),
        expires_at: NOW + 30 * 86_400_000,
        version: 2,
        created_by: "creator".into(),
        identity_exists: false,
    }
}

fn command(action: InvitationAction, version: i64) -> InvitationCommand {
    InvitationCommand {
        action,
        expected_version: version,
        reason: "Issue the approved customer invitation.".into(),
    }
}

#[test]
fn invite_issues_a_bounded_generation_one_token() {
    let plan = issue_plan(
        &actor("creator", &["commerce.sales", "commerce.read"]),
        &binding(),
        None,
        &command(InvitationAction::Invite, 2),
        SECRET,
        "acme",
        NOW,
    )
    .unwrap();
    assert_eq!(plan.generation, 1);
    assert_eq!(plan.holding_slug, "acme");
    assert_eq!(plan.email, "customer@example.invalid");
    assert_eq!(plan.token_expires_at, NOW + INVITATION_LIFETIME_MILLIS);
    assert!(
        plan.invitation_id
            .starts_with("customer-portal-invitation-")
    );
    let claims = InvitationClaims {
        invitation_id: plan.invitation_id.clone(),
        binding_id: "binding-1".into(),
        holding_slug: "acme".into(),
        generation: 1,
        expires_at: plan.token_expires_at,
        key_version: 1,
    };
    assert_eq!(
        plan.token_hash,
        invitation_token_hash(&invitation_token(SECRET, &claims))
    );
    assert_eq!(plan.terminal, None);
}

#[test]
fn resend_supersedes_only_a_live_previous_invitation() {
    let previous = erpx_server_customer_authority::public::portal_invitations::ActiveInvitation {
        id: "customer-portal-invitation-previous".into(),
        generation: 3,
        state: "sent".into(),
        token_expires_at: NOW + 1_000,
    };
    let plan = issue_plan(
        &actor("creator", &["commerce.sales"]),
        &binding(),
        Some(&previous),
        &command(InvitationAction::Resend, 2),
        SECRET,
        "acme",
        NOW,
    )
    .unwrap();
    assert_eq!(plan.generation, 4);
    assert_eq!(plan.terminal, Some("superseded"));
    assert_eq!(
        plan.previous.map(|value| value.id),
        Some(previous.id.clone())
    );

    let mut expired = previous.clone();
    expired.token_expires_at = NOW - 1;
    let error = issue_plan(
        &actor("creator", &["commerce.sales"]),
        &binding(),
        Some(&expired),
        &command(InvitationAction::Resend, 2),
        SECRET,
        "acme",
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_INVITATION_NOT_ACTIVE");

    let error = issue_plan(
        &actor("creator", &["commerce.sales"]),
        &binding(),
        None,
        &command(InvitationAction::Resend, 2),
        SECRET,
        "acme",
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_INVITATION_NOT_ACTIVE");
}

#[test]
fn invite_refuses_a_live_previous_invitation() {
    let previous = erpx_server_customer_authority::public::portal_invitations::ActiveInvitation {
        id: "customer-portal-invitation-previous".into(),
        generation: 1,
        state: "queued".into(),
        token_expires_at: NOW + 1_000,
    };
    let error = issue_plan(
        &actor("creator", &["commerce.sales"]),
        &binding(),
        Some(&previous),
        &command(InvitationAction::Invite, 2),
        SECRET,
        "acme",
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_INVITATION_ACTIVE");

    // An elapsed token no longer blocks a fresh invite and is marked expired.
    let mut expired = previous;
    expired.token_expires_at = NOW - 1;
    let plan = issue_plan(
        &actor("creator", &["commerce.sales"]),
        &binding(),
        Some(&expired),
        &command(InvitationAction::Invite, 2),
        SECRET,
        "acme",
        NOW,
    )
    .unwrap();
    assert_eq!(plan.terminal, Some("expired"));
}

#[test]
fn authority_and_source_preconditions_are_exact() {
    let mut stale = binding();
    stale.version = 3;
    assert_eq!(
        issue_plan(
            &actor("creator", &["commerce.sales"]),
            &stale,
            None,
            &command(InvitationAction::Invite, 2),
            SECRET,
            "acme",
            NOW
        )
        .unwrap_err()
        .code,
        "CUSTOMER_ACCESS_CHANGED"
    );

    // Only the original requester or a sales approver may invite.
    assert_eq!(
        issue_plan(
            &actor("someone-else", &["commerce.sales"]),
            &binding(),
            None,
            &command(InvitationAction::Invite, 2),
            SECRET,
            "acme",
            NOW
        )
        .unwrap_err()
        .code,
        "CUSTOMER_ACCESS_PERMISSION_REQUIRED"
    );
    assert!(
        issue_plan(
            &actor("manager", &["commerce.sales", "commerce.approve"]),
            &binding(),
            None,
            &command(InvitationAction::Invite, 2),
            SECRET,
            "acme",
            NOW
        )
        .is_ok()
    );

    let mut bound = binding();
    bound.identity_exists = true;
    assert_eq!(
        issue_plan(
            &actor("creator", &["commerce.sales"]),
            &bound,
            None,
            &command(InvitationAction::Invite, 2),
            SECRET,
            "acme",
            NOW
        )
        .unwrap_err()
        .code,
        "CUSTOMER_PORTAL_IDENTITY_ALREADY_BOUND"
    );

    let mut accepted =
        erpx_server_customer_authority::public::portal_invitations::ActiveInvitation {
            id: "customer-portal-invitation-accepted".into(),
            generation: 1,
            state: "accepted".into(),
            token_expires_at: NOW + 1_000,
        };
    assert_eq!(
        issue_plan(
            &actor("creator", &["commerce.sales"]),
            &binding(),
            Some(&accepted),
            &command(InvitationAction::Invite, 2),
            SECRET,
            "acme",
            NOW
        )
        .unwrap_err()
        .code,
        "CUSTOMER_PORTAL_IDENTITY_ALREADY_BOUND"
    );
    accepted.state = "sent".into();

    for (status, state, expires_at) in [
        ("pending", "confirmed", NOW + 1_000),
        ("approved", "draft", NOW + 1_000),
        ("approved", "confirmed", NOW - 1),
    ] {
        let mut source = binding();
        source.status = status.into();
        source.order_state = state.into();
        source.expires_at = expires_at;
        assert_eq!(
            issue_plan(
                &actor("creator", &["commerce.sales"]),
                &source,
                None,
                &command(InvitationAction::Invite, 2),
                SECRET,
                "acme",
                NOW
            )
            .unwrap_err()
            .code,
            "CUSTOMER_PORTAL_BINDING_UNAVAILABLE"
        );
    }
}

#[test]
fn token_expiry_never_exceeds_the_binding_expiry() {
    let mut short = binding();
    short.expires_at = NOW + 60_000;
    let plan = issue_plan(
        &actor("creator", &["commerce.sales"]),
        &short,
        None,
        &command(InvitationAction::Invite, 2),
        SECRET,
        "acme",
        NOW,
    )
    .unwrap();
    assert_eq!(plan.token_expires_at, NOW + 60_000);
}

#[test]
fn strict_command_parsing_rejects_unknown_or_inverted_input() {
    use serde_json::json;
    assert!(
        InvitationCommand::parse(&json!({
            "action": "invite", "expectedVersion": 2, "reason": "Issue the approved invitation."
        }))
        .is_ok()
    );
    for invalid in [
        json!({"action": "invite", "expectedVersion": 0, "reason": "Issue the approved invitation."}),
        json!({"action": "invite", "expectedVersion": 2, "reason": "  "}),
        json!({"action": "invite", "expectedVersion": 2, "reason": "Issue the approved invitation.", "extra": true}),
        json!({"action": "other", "expectedVersion": 2, "reason": "Issue the approved invitation."}),
    ] {
        assert_eq!(
            InvitationCommand::parse(&invalid).unwrap_err().code,
            "CUSTOMER_ACCESS_REQUEST_INVALID"
        );
    }
}
