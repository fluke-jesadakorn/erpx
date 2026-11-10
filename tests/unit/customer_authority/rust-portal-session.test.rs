//! Unit rules for the verified external portal customer proof.
#![allow(dead_code)]

#[path = "portal_session_cases.rs"]
mod cases;

use cases::{request, Enrollment, Session};
use erpx_server_customer_authority::public::portal_session::{
    resolve_portal_customer, session_required, PortalCustomerRequest, PortalSessionEvidence,
};

const NOW: i64 = 1_800_000_000_000;

fn evidence() -> PortalSessionEvidence {
    PortalSessionEvidence {
        session_id: "session-child".into(),
        user_id: "customer-user".into(),
        email: "Customer@Example.Test".into(),
        email_verified: true,
        authenticated_at: NOW - 1_000,
        expires_at: NOW + 60_000,
    }
}

fn verifier_ok() -> Session {
    Session {
        value: Some(evidence()),
        ..Session::default()
    }
}

fn enrollment_ok() -> Enrollment {
    Enrollment {
        value: Some(cases::proof("customer-user", "customer@example.test")),
        ..Enrollment::default()
    }
}

#[tokio::test]
async fn verified_session_and_enrollment_produce_normalized_holding_proof() {
    let mut verifier = verifier_ok();
    let mut enrollment = enrollment_ok();
    let proof = resolve_portal_customer(&mut verifier, &mut enrollment, &request(), NOW)
        .await
        .unwrap();
    assert_eq!(proof.user_id, "customer-user");
    assert_eq!(proof.email, "customer@example.test");
    assert_eq!(proof.holding_id.as_deref(), Some("h001"));
    assert_eq!(verifier.calls, 1);
    assert_eq!(enrollment.calls, 1);
}

#[tokio::test]
async fn absent_session_fails_closed_before_enrollment() {
    let mut verifier = Session::default();
    let mut enrollment = enrollment_ok();
    let error = resolve_portal_customer(&mut verifier, &mut enrollment, &request(), NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_SESSION_REQUIRED");
    assert_eq!(enrollment.calls, 0);
}

#[tokio::test]
async fn unverified_email_is_forbidden() {
    let mut verifier = Session {
        value: Some(PortalSessionEvidence {
            email_verified: false,
            ..evidence()
        }),
        ..Session::default()
    };
    let mut enrollment = enrollment_ok();
    let error = resolve_portal_customer(&mut verifier, &mut enrollment, &request(), NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_EMAIL_UNVERIFIED");
    assert_eq!(enrollment.calls, 0);
}

#[tokio::test]
async fn expired_or_revoked_session_is_unauthenticated() {
    for session in [
        PortalSessionEvidence {
            expires_at: NOW,
            ..evidence()
        },
        PortalSessionEvidence {
            expires_at: NOW - 1,
            ..evidence()
        },
    ] {
        let mut verifier = Session {
            value: Some(session),
            ..Session::default()
        };
        let mut enrollment = enrollment_ok();
        let error = resolve_portal_customer(&mut verifier, &mut enrollment, &request(), NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, "CUSTOMER_PORTAL_SESSION_REQUIRED");
        assert_eq!(enrollment.calls, 0);
    }
}

#[tokio::test]
async fn malformed_session_proof_is_unauthenticated() {
    for session in [
        PortalSessionEvidence {
            session_id: String::new(),
            ..evidence()
        },
        PortalSessionEvidence {
            user_id: String::new(),
            ..evidence()
        },
        PortalSessionEvidence {
            email: "not-an-email".into(),
            ..evidence()
        },
    ] {
        let mut verifier = Session {
            value: Some(session),
            ..Session::default()
        };
        let mut enrollment = enrollment_ok();
        let error = resolve_portal_customer(&mut verifier, &mut enrollment, &request(), NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, "CUSTOMER_PORTAL_SESSION_REQUIRED");
    }
}

#[tokio::test]
async fn missing_enrollment_does_not_reveal_tenant_exists() {
    let mut verifier = verifier_ok();
    let mut enrollment = Enrollment::default();
    let error = resolve_portal_customer(&mut verifier, &mut enrollment, &request(), NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_NOT_FOUND");
}

#[tokio::test]
async fn enrollment_mismatch_in_user_email_or_holding_is_not_found() {
    for wrong in [
        cases::proof("other-user", "customer@example.test"),
        cases::proof("customer-user", "other@example.test"),
        cases::proof_with_holding("customer-user", "customer@example.test", "h002"),
    ] {
        let mut verifier = verifier_ok();
        let mut enrollment = Enrollment {
            value: Some(wrong),
            ..Enrollment::default()
        };
        let error = resolve_portal_customer(&mut verifier, &mut enrollment, &request(), NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, "CUSTOMER_PORTAL_NOT_FOUND");
    }
}

#[tokio::test]
async fn direct_account_session_mode_is_a_valid_request() {
    let mut verifier = verifier_ok();
    let mut enrollment = enrollment_ok();
    let proof = resolve_portal_customer(
        &mut verifier,
        &mut enrollment,
        &PortalCustomerRequest {
            audience: None,
            ..request()
        },
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(proof.user_id, "customer-user");
    assert_eq!(proof.holding_id.as_deref(), Some("h001"));
}

#[tokio::test]
async fn malformed_request_facts_are_rejected_before_any_read() {
    let cases = [
        PortalCustomerRequest {
            token: "",
            ..request()
        },
        PortalCustomerRequest {
            token: "session token",
            ..request()
        },
        PortalCustomerRequest {
            audience: Some(""),
            ..request()
        },
        PortalCustomerRequest {
            environment: "Development",
            ..request()
        },
        PortalCustomerRequest {
            holding_id: "",
            ..request()
        },
        PortalCustomerRequest {
            holding_slug: "../h001",
            ..request()
        },
    ];
    for request in cases {
        let mut verifier = verifier_ok();
        let mut enrollment = enrollment_ok();
        let error = resolve_portal_customer(&mut verifier, &mut enrollment, &request, NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, "CUSTOMER_PORTAL_SESSION_INVALID");
        assert_eq!(verifier.calls, 0);
        assert_eq!(enrollment.calls, 0);
    }
}

#[tokio::test]
async fn verifier_business_failure_is_returned_unchanged() {
    let mut verifier = Session {
        error: Some(session_required()),
        ..Session::default()
    };
    let mut enrollment = enrollment_ok();
    let error = resolve_portal_customer(&mut verifier, &mut enrollment, &request(), NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_SESSION_REQUIRED");
    assert_eq!(enrollment.calls, 0);
}
