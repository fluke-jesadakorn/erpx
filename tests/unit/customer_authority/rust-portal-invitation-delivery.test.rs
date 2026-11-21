//! Native Customer Portal invitation delivery rules and orchestration.
//!
//! Proves the retained token encoding byte-for-byte against a TypeScript
//! reference vector, the exact private link/body/idempotency key, bounded
//! retry and terminal states, the safe persisted delivery codes, and the
//! recovery/claim/cancel/sent/failed orchestration with a fake owner unit.
use erpx_server_customer_authority::public::portal_invitation_delivery::{
    EMAIL_DELIVERY_FAILED, EMAIL_IDEMPOTENCY_CONFLICT, EMAIL_NOT_CONFIGURED,
    EMAIL_SANDBOX_FORBIDDEN_IN_PRODUCTION, EMAIL_SENSITIVE_RECEIPT_INVALID, ENROLLMENT_PATH,
    InvitationClaims, InvitationJob, LEASE_MILLIS, MAX_ATTEMPTS, PUBLIC_ORIGIN_NOT_CONFIGURED,
    PortalEmail, PortalInvitationEmailPort, PortalInvitationUnit, SIGNING_SECRET_CHANGED,
    beta_email_body, deliver_one, invitation_token, invitation_token_hash,
    invitation_token_hash_matches, iso_millis, require_invitation_token, resolve_account_origin,
    safe_delivery_code,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::Mutex;

const SECRET: &str = "vector-secret-2026";

fn claims() -> InvitationClaims {
    InvitationClaims {
        invitation_id: "customer-portal-invitation-vector".into(),
        binding_id: "binding-vector".into(),
        holding_slug: "acme".into(),
        generation: 3,
        expires_at: 1_800_000_000_000,
        key_version: 1,
    }
}

fn job() -> InvitationJob {
    InvitationJob {
        id: "customer-portal-invitation-vector".into(),
        binding_id: "binding-vector".into(),
        company_id: "company".into(),
        holding_slug: "acme".into(),
        generation: 3,
        email: "customer@example.invalid".into(),
        token_hash: invitation_token_hash(&invitation_token(SECRET, &claims())),
        token_key_version: 1,
        token_expires_at: 1_800_000_000_000,
        attempts: 1,
        lease_token: "lease-1".into(),
    }
}

#[test]
fn signed_token_matches_the_retained_typescript_vector() {
    let token = invitation_token(SECRET, &claims());
    assert_eq!(
        token,
        "eyJpbnZpdGF0aW9uSWQiOiJjdXN0b21lci1wb3J0YWwtaW52aXRhdGlvbi12ZWN0b3IiLCJiaW5kaW5nSWQiOiJiaW5kaW5nLXZlY3RvciIsImhvbGRpbmdTbHVnIjoiYWNtZSIsImdlbmVyYXRpb24iOjMsImV4cGlyZXNBdCI6MTgwMDAwMDAwMDAwMCwia2V5VmVyc2lvbiI6MX0.RCdt1WqTzmwAJsjuihgBkgOpu3rFpuca_l6JfpYb_40"
    );
    assert_eq!(
        invitation_token_hash(&token),
        "92b5b1103894ad95d18707663ebf82c56b3629439b8766a653d50c4c69daddcc"
    );
    assert!(invitation_token_hash_matches(&token, &job().token_hash));
}

#[test]
fn a_changed_secret_or_digest_is_refused_before_sending() {
    assert_eq!(
        require_invitation_token(Some(SECRET), &claims(), &job().token_hash).unwrap(),
        invitation_token(SECRET, &claims())
    );
    let changed =
        require_invitation_token(Some("another-secret"), &claims(), &job().token_hash).unwrap_err();
    assert_eq!(changed.code, SIGNING_SECRET_CHANGED);
    let unavailable = require_invitation_token(None, &claims(), &job().token_hash).unwrap_err();
    assert_eq!(unavailable.code, "INVITATION_SIGNING_SECRET_UNAVAILABLE");
    assert!(!invitation_token_hash_matches("token", &"z".repeat(64)));
    assert!(!invitation_token_hash_matches("token", &"A".repeat(64)));
}

#[test]
fn private_link_body_and_idempotency_key_are_the_retained_message() {
    let job = job();
    let token = invitation_token(SECRET, &claims());
    let link = job.enrollment_url("https://account-dev-erpx.jesadakorn.com", &token);
    assert_eq!(
        link,
        format!("https://account-dev-erpx.jesadakorn.com{ENROLLMENT_PATH}#token={token}")
    );
    assert_eq!(
        job.delivery_key(),
        "customer-portal-invitation-customer-portal-invitation-vector"
    );
    let text = job.email_text(&link);
    assert_eq!(
        text,
        format!(
            "Use this private link to verify your account and accept the invitation for the approved customer document.\n\n{link}\n\nThe link expires at 2027-01-15T08:00:00.000Z. After verifying your email, reopen this invitation link and sign in to accept it."
        )
    );
    assert_eq!(
        beta_email_body(job.email_subject(), &text),
        format!(
            "Your ERPX Customer Portal invitation\n\n{text}\n\nIf you did not request this, you can ignore this email."
        )
    );
    assert!(link.contains("https://") && !link.contains("&token="));
}

#[test]
fn javascript_iso_timestamps_keep_millisecond_parity() {
    assert_eq!(iso_millis(0), "1970-01-01T00:00:00.000Z");
    assert_eq!(iso_millis(1_800_000_000_000), "2027-01-15T08:00:00.000Z");
    assert_eq!(iso_millis(-1), "1969-12-31T23:59:59.999Z");
    assert_eq!(iso_millis(1_700_000_000_123), "2023-11-14T22:13:20.123Z");
}

#[test]
fn retry_backoff_and_terminal_state_are_bounded() {
    let mut job = job();
    let expected = [
        (1, 60_000),
        (2, 120_000),
        (3, 240_000),
        (5, 960_000),
        (6, 1_920_000),
    ];
    for (attempts, delay) in expected {
        job.attempts = attempts;
        assert_eq!(
            job.next_attempt_at(1_000),
            1_000 + delay,
            "attempts {attempts}"
        );
    }
    job.attempts = 12;
    assert_eq!(job.next_attempt_at(1_000), 1_000 + 3_600_000);
    job.attempts = MAX_ATTEMPTS - 1;
    assert_eq!(job.terminal_state(), "queued");
    job.attempts = MAX_ATTEMPTS;
    assert_eq!(job.terminal_state(), "failed");
    assert_eq!(LEASE_MILLIS, 30_000);
}

#[test]
fn only_retained_safe_codes_are_persisted() {
    for code in [
        EMAIL_NOT_CONFIGURED,
        EMAIL_SANDBOX_FORBIDDEN_IN_PRODUCTION,
        EMAIL_SENSITIVE_RECEIPT_INVALID,
        EMAIL_IDEMPOTENCY_CONFLICT,
        PUBLIC_ORIGIN_NOT_CONFIGURED,
    ] {
        assert_eq!(safe_delivery_code(code), code);
    }
    assert_eq!(
        safe_delivery_code("Private message for customer@example.test"),
        EMAIL_DELIVERY_FAILED
    );
    assert_eq!(
        safe_delivery_code("EMAIL_SENSITIVE_ENCRYPTION_NOT_CONFIGURED"),
        EMAIL_DELIVERY_FAILED
    );
}

#[test]
fn account_origin_requires_the_complete_development_profile() {
    assert_eq!(
        resolve_account_origin(
            true,
            true,
            true,
            true,
            true,
            false,
            Some("http://legacy.local")
        )
        .as_deref(),
        Some("https://account-dev-erpx.jesadakorn.com")
    );
    assert_eq!(
        resolve_account_origin(true, true, true, true, true, true, None).as_deref(),
        Some("https://account.erpx.localhost:3443")
    );
    assert_eq!(
        resolve_account_origin(
            true,
            false,
            true,
            true,
            true,
            false,
            Some("https://legacy.local")
        )
        .as_deref(),
        Some("https://legacy.local")
    );
    assert_eq!(
        resolve_account_origin(false, false, false, false, false, false, Some(" ")).as_deref(),
        None
    );
}

#[derive(Clone, Copy)]
enum SendOutcome {
    Accepted,
    Failed(&'static str),
}

struct FakeEmail {
    outcome: SendOutcome,
    sent: Mutex<Vec<PortalEmail>>,
}

impl PortalInvitationEmailPort for FakeEmail {
    fn send<'a>(
        &'a self,
        email: PortalEmail,
    ) -> erpx_server_customer_authority::public::portal_invitation_delivery::PortalEmailFuture<'a, ()>
    {
        self.sent.lock().unwrap().push(email);
        let outcome = self.outcome;
        Box::pin(async move {
            match outcome {
                SendOutcome::Accepted => Ok(()),
                SendOutcome::Failed(code) => Err(BusinessError::new(
                    FailureKind::Unavailable,
                    code,
                    "declared delivery failure",
                )),
            }
        })
    }
}

#[derive(Default)]
struct FakeState {
    job: Option<InvitationJob>,
    current: bool,
    recovered: usize,
    cancelled: Vec<(String, String)>,
    sent: usize,
    failed: Vec<(String, String, i64)>,
}

struct FakeUnit {
    state: Mutex<FakeState>,
}

impl FakeUnit {
    fn new(job: Option<InvitationJob>, current: bool) -> Self {
        Self {
            state: Mutex::new(FakeState {
                job,
                current,
                ..FakeState::default()
            }),
        }
    }
}

impl PortalInvitationUnit for FakeUnit {
    type Error = BusinessError;

    fn recover<'a>(
        &'a self,
        _now: i64,
    ) -> erpx_server_customer_authority::public::portal_invitation_delivery::PortalDeliveryFuture<
        'a,
        (),
        Self::Error,
    >{
        Box::pin(async move {
            self.state.lock().unwrap().recovered += 1;
            Ok(())
        })
    }

    fn claim<'a>(
        &'a self,
        _lease_token: &'a str,
        _now: i64,
    ) -> erpx_server_customer_authority::public::portal_invitation_delivery::PortalDeliveryFuture<
        'a,
        Option<InvitationJob>,
        Self::Error,
    >{
        Box::pin(async move { Ok(self.state.lock().unwrap().job.clone()) })
    }

    fn current<'a>(
        &'a self,
        _job: &'a InvitationJob,
        _now: i64,
    ) -> erpx_server_customer_authority::public::portal_invitation_delivery::PortalDeliveryFuture<
        'a,
        bool,
        Self::Error,
    >{
        Box::pin(async move { Ok(self.state.lock().unwrap().current) })
    }

    fn cancel<'a>(
        &'a self,
        job: &'a InvitationJob,
        _now: i64,
        state: &'static str,
        reason: &'static str,
    ) -> erpx_server_customer_authority::public::portal_invitation_delivery::PortalDeliveryFuture<
        'a,
        (),
        Self::Error,
    >{
        Box::pin(async move {
            self.state
                .lock()
                .unwrap()
                .cancelled
                .push((job.id.clone(), format!("{state}:{reason}")));
            Ok(())
        })
    }

    fn sent<'a>(
        &'a self,
        _job: &'a InvitationJob,
        _now: i64,
    ) -> erpx_server_customer_authority::public::portal_invitation_delivery::PortalDeliveryFuture<
        'a,
        (),
        Self::Error,
    >{
        Box::pin(async move {
            self.state.lock().unwrap().sent += 1;
            Ok(())
        })
    }

    fn failed<'a>(
        &'a self,
        _job: &'a InvitationJob,
        _now: i64,
        state: &'static str,
        code: &'static str,
        next_attempt_at: i64,
    ) -> erpx_server_customer_authority::public::portal_invitation_delivery::PortalDeliveryFuture<
        'a,
        (),
        Self::Error,
    >{
        Box::pin(async move {
            self.state
                .lock()
                .unwrap()
                .failed
                .push((state.into(), code.into(), next_attempt_at));
            Ok(())
        })
    }
}

fn signer() -> impl Fn(&InvitationClaims, &str) -> Result<String, BusinessError> {
    |claims, stored| require_invitation_token(Some(SECRET), claims, stored)
}

fn fast_job() -> InvitationJob {
    let mut job = job();
    job.token_expires_at = 10_000;
    job.token_hash = invitation_token_hash(&invitation_token(SECRET, &job.claims()));
    job
}

#[tokio::test]
async fn one_cycle_sends_the_private_link_and_commits_sent() {
    let unit = FakeUnit::new(Some(fast_job()), true);
    let email = FakeEmail {
        outcome: SendOutcome::Accepted,
        sent: Mutex::new(Vec::new()),
    };
    assert!(
        deliver_one(
            &unit,
            &email,
            &signer(),
            Some("https://account.example.test"),
            "lease",
            1_000
        )
        .await
        .unwrap()
    );
    let state = unit.state.lock().unwrap();
    assert_eq!(state.recovered, 1);
    assert_eq!(state.sent, 1);
    assert!(state.failed.is_empty() && state.cancelled.is_empty());
    let sent = email.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].delivery_key, fast_job().delivery_key());
    assert_eq!(sent[0].recipient, "customer@example.invalid");
    assert!(sent[0].text.contains("#token="));
    assert!(
        sent[0]
            .text
            .ends_with("If you did not request this, you can ignore this email.")
    );
}

#[tokio::test]
async fn an_empty_queue_finishes_without_delivery() {
    let unit = FakeUnit::new(None, true);
    let email = FakeEmail {
        outcome: SendOutcome::Accepted,
        sent: Mutex::new(Vec::new()),
    };
    assert!(
        !deliver_one(
            &unit,
            &email,
            &signer(),
            Some("https://account.example.test"),
            "lease",
            1_000
        )
        .await
        .unwrap()
    );
    assert_eq!(unit.state.lock().unwrap().recovered, 1);
    assert!(email.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_expired_token_is_cancelled_before_any_send() {
    let mut job = fast_job();
    job.token_expires_at = 999;
    let unit = FakeUnit::new(Some(job), true);
    let email = FakeEmail {
        outcome: SendOutcome::Accepted,
        sent: Mutex::new(Vec::new()),
    };
    assert!(
        deliver_one(
            &unit,
            &email,
            &signer(),
            Some("https://account.example.test"),
            "lease",
            1_000
        )
        .await
        .unwrap()
    );
    assert_eq!(
        unit.state.lock().unwrap().cancelled,
        vec![(
            "customer-portal-invitation-vector".to_owned(),
            "expired:INVITATION_EXPIRED".to_owned()
        )]
    );
    assert!(email.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_changed_source_is_revoked_before_any_send() {
    let unit = FakeUnit::new(Some(fast_job()), false);
    let email = FakeEmail {
        outcome: SendOutcome::Accepted,
        sent: Mutex::new(Vec::new()),
    };
    assert!(
        deliver_one(
            &unit,
            &email,
            &signer(),
            Some("https://account.example.test"),
            "lease",
            1_000
        )
        .await
        .unwrap()
    );
    assert_eq!(
        unit.state.lock().unwrap().cancelled,
        vec![(
            "customer-portal-invitation-vector".to_owned(),
            "revoked:SOURCE_UNAVAILABLE".to_owned()
        )]
    );
    assert!(email.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn provider_failures_records_the_safe_code_and_next_attempt() {
    let unit = FakeUnit::new(Some(fast_job()), true);
    let email = FakeEmail {
        outcome: SendOutcome::Failed(EMAIL_SENSITIVE_RECEIPT_INVALID),
        sent: Mutex::new(Vec::new()),
    };
    assert!(
        deliver_one(
            &unit,
            &email,
            &signer(),
            Some("https://account.example.test"),
            "lease",
            1_000
        )
        .await
        .unwrap()
    );
    let state = unit.state.lock().unwrap();
    assert!(state.cancelled.is_empty());
    assert_eq!(
        state.failed,
        vec![(
            "queued".to_owned(),
            EMAIL_SENSITIVE_RECEIPT_INVALID.to_owned(),
            1_000 + 60_000
        )]
    );
}

#[tokio::test]
async fn a_missing_account_origin_fails_closed_with_the_retained_code() {
    let unit = FakeUnit::new(Some(fast_job()), true);
    let email = FakeEmail {
        outcome: SendOutcome::Accepted,
        sent: Mutex::new(Vec::new()),
    };
    assert!(
        deliver_one(&unit, &email, &signer(), None, "lease", 1_000)
            .await
            .unwrap()
    );
    assert_eq!(
        unit.state.lock().unwrap().failed,
        vec![(
            "queued".to_owned(),
            PUBLIC_ORIGIN_NOT_CONFIGURED.to_owned(),
            61_000
        )]
    );
    assert!(email.sent.lock().unwrap().is_empty());
}
