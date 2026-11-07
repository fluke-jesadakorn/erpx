#[allow(
    dead_code,
    reason = "shared policy fixture includes Account auth helpers exercised by its own suites"
)]
#[path = "../../support/rust-account.rs"]
mod fixture;
use erpx_server_identity_access::public::{account_mail::*, *};

#[test]
fn delivery_requires_an_unconsumed_current_credential_recipient_and_exact_proof_window() {
    for case in 0..10 {
        let mut facts = CurrentAccountMailProof {
            kind: CredentialProofKind::EmailVerification,
            created_at: 100,
            expires_at: 1000,
            consumed: false,
            identity_current: true,
            credential_current: true,
            recipient_current: true,
            email_verified: false,
        };
        match case {
            0 => {}
            1 => facts.consumed = true,
            2 => facts.identity_current = false,
            3 => facts.credential_current = false,
            4 => facts.recipient_current = false,
            5 => facts.email_verified = true,
            6 => facts.created_at = 201,
            7 => facts.expires_at = 200,
            8 => facts.expires_at = 3_600_101,
            9 => {
                facts.kind = CredentialProofKind::PasswordReset;
                facts.email_verified = true;
            }
            _ => unreachable!(),
        }
        assert_eq!(
            account_mail_proof_available(&facts, 200),
            matches!(case, 0 | 9),
            "source case {case}"
        );
    }
}

fn payload(kind: CredentialProofKind, path: &str) -> OpenedAccountMail {
    OpenedAccountMail {
        kind,
        account_origin: "https://account.example.test".into(),
        return_path: path.into(),
        token: OpaqueSecret::new("a".repeat(64)).unwrap(),
    }
}
#[test]
fn native_mail_matches_current_plain_text_template_and_better_auth_link_shapes() {
    // Current-source oracles: manage_erpx/domain/email.ts betaEmailBody,
    // installed Better Auth api/routes/{password,email-verification}.mjs,
    // and account-recovery's active central Account origin contract.
    let policy = AccountMailPolicy::new(fixture::policy()).unwrap();
    let message = policy
        .render(
            EmailAddress::new("staff@example.test").unwrap(),
            &payload(
                CredentialProofKind::EmailVerification,
                "/onboarding?tab=~!*'()&next=%2Fhome",
            ),
            "synthetic-outbox",
        )
        .unwrap();
    let url = format!(
        "https://account.example.test/api/auth/verify-email?token={}&callbackURL=%2Fonboarding%3Ftab%3D~!*'()%26next%3D%252Fhome",
        "a".repeat(64)
    );
    assert_eq!(message.subject(), "Verify your ERPX email");
    assert_eq!(
        message.body(),
        format!(
            "Verify your ERPX email\n\n{url}\n\nIf you did not request this, you can ignore this email."
        )
    );
    assert_eq!(message.delivery_key(), "erpx-account-mail/synthetic-outbox");
    let reset = policy
        .render(
            EmailAddress::new("staff@example.test").unwrap(),
            &payload(CredentialProofKind::PasswordReset, "/reset-password"),
            "reset-outbox",
        )
        .unwrap();
    assert_eq!(
        reset.body(),
        format!(
            "Reset your ERPX password\n\nhttps://account.example.test/api/auth/reset-password/{}?callbackURL=%2Freset-password\n\nIf you did not request this, you can ignore this email.",
            "a".repeat(64)
        )
    );
}
#[test]
fn mail_never_adopts_a_foreign_origin_or_a_secret_bearing_header() {
    let policy = AccountMailPolicy::new(fixture::policy()).unwrap();
    for path in [
        "//foreign.example.test",
        "/%2Fforeign.example.test",
        "/\\evil",
        "/%0D%0Ainjected",
    ] {
        assert!(
            policy
                .render(
                    EmailAddress::new("staff@example.test").unwrap(),
                    &payload(CredentialProofKind::PasswordReset, path),
                    "outbox"
                )
                .is_err()
        );
    }
    let mut value = payload(CredentialProofKind::EmailVerification, "/");
    value.account_origin = "https://foreign.example.test".into();
    assert!(
        policy
            .render(
                EmailAddress::new("staff@example.test").unwrap(),
                &value,
                "outbox"
            )
            .is_err()
    );
    let message = policy
        .render(
            EmailAddress::new("staff@example.test").unwrap(),
            &payload(CredentialProofKind::PasswordReset, "/reset-password"),
            "outbox",
        )
        .unwrap();
    let profile = AccountMailProviderProfile {
        kind: AccountMailProviderKind::Resend,
        profile_id: "provider\r\nsecret".into(),
        from: "ERPX <noreply@example.test>".into(),
    };
    assert!(profile.fingerprint(&message).is_err());
}
#[test]
fn provider_retry_matches_six_attempt_backoff_and_unknown_smtp_is_never_replayed() {
    assert_eq!(account_mail_next_attempt(1, 100), Some(60_100));
    assert_eq!(account_mail_next_attempt(5, 100), Some(960_100));
    assert_eq!(account_mail_next_attempt(6, 100), None);
    assert_eq!(account_mail_next_attempt(0, 100), None);
    assert_eq!(account_mail_next_attempt(1, i64::MAX), None);
    assert!(account_mail_can_retry_unknown(
        AccountMailProviderKind::Resend,
        100,
        3_600_100,
        200
    ));
    assert!(!account_mail_can_retry_unknown(
        AccountMailProviderKind::Resend,
        100,
        3_600_100,
        3_600_100
    ));
    assert!(!account_mail_can_retry_unknown(
        AccountMailProviderKind::Resend,
        100,
        i64::MAX,
        100 + RESEND_IDEMPOTENCY_WINDOW_MS
    ));
    assert!(!account_mail_can_retry_unknown(
        AccountMailProviderKind::Smtp,
        100,
        3_600_100,
        200
    ));
}
#[test]
fn idempotent_message_fingerprint_binds_sender_provider_recipient_and_exact_payload() {
    let policy = AccountMailPolicy::new(fixture::policy()).unwrap();
    let message = policy
        .render(
            EmailAddress::new("staff@example.test").unwrap(),
            &payload(CredentialProofKind::PasswordReset, "/reset-password"),
            "outbox",
        )
        .unwrap();
    let mut profile = AccountMailProviderProfile {
        kind: AccountMailProviderKind::Resend,
        profile_id: "synthetic-provider".into(),
        from: "ERPX <noreply@example.test>".into(),
    };
    let first = profile.fingerprint(&message).unwrap();
    assert_eq!(first.len(), 64);
    assert_eq!(first, profile.fingerprint(&message).unwrap());
    profile.kind = AccountMailProviderKind::Smtp;
    assert_ne!(first, profile.fingerprint(&message).unwrap());
    profile.kind = AccountMailProviderKind::Resend;
    profile.from = "Other <noreply@example.test>".into();
    assert_ne!(first, profile.fingerprint(&message).unwrap());
    let other = policy
        .render(
            EmailAddress::new("other@example.test").unwrap(),
            &payload(CredentialProofKind::PasswordReset, "/reset-password"),
            "outbox",
        )
        .unwrap();
    assert_ne!(
        profile.fingerprint(&message).unwrap(),
        profile.fingerprint(&other).unwrap()
    );
}
