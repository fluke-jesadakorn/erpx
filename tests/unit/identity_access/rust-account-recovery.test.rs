use erpx_server_identity_access::public::*;

#[test]
fn invitation_scope_matches_current_grant_rules_without_missing_node_wildcards() {
    for (parent, node, scope, branch, department, expected) in [
        ("holding", None, "holding", None, None, true),
        (
            "holding",
            None,
            "department",
            None,
            Some("company-department"),
            true,
        ),
        (
            "branch",
            Some("north"),
            "department",
            Some("north"),
            Some("payroll"),
            true,
        ),
        (
            "branch",
            Some("south"),
            "department",
            Some("north"),
            Some("payroll"),
            false,
        ),
        ("branch", None, "branch", None, None, false),
        (
            "department",
            Some("payroll"),
            "department",
            Some("north"),
            Some("payroll"),
            true,
        ),
        (
            "department",
            Some("payroll"),
            "branch",
            Some("north"),
            None,
            false,
        ),
        ("branch", Some("north"), "holding", None, None, false),
        ("holding", None, "holding", Some("north"), None, false),
        ("holding", None, "platform", None, None, false),
    ] {
        assert_eq!(
            invitation_scope_covers(parent, node, scope, branch, department),
            expected
        );
    }
}

#[test]
fn staff_invitation_payload_never_accepts_directory_or_existing_account_credentials() {
    let grant = StaffInvitationGrant::parse(r#"{"role":"custom_staff","expires":1800000000000.0}"#)
        .unwrap();
    assert_eq!(grant.role, "custom_staff");
    assert_eq!(grant.expires, 1800000000000);
    let it_admin =
        StaffInvitationGrant::parse(r#"{"role":"it_admin","expires":1800000000000}"#).unwrap();
    assert!(it_admin.supports_scope("holding"));
    assert!(!it_admin.supports_scope("branch"));
    assert!(!it_admin.supports_scope("department"));
    assert!(!grant.supports_scope("platform"));
    for payload in [
        r#"{"role":"custom_staff","expires":1800000000000,"direct":true}"#,
        r#"{"role":"custom_staff","expires":1800000000000,"existingUserId":"someone"}"#,
        r#"{"role":"","expires":1800000000000}"#,
        r#"{"role":"staff","expires":9007199254740992}"#,
        r#"{"role":"staff","expires":12.5}"#,
        r#"{"role":"staff","expires":"1800000000000"}"#,
    ] {
        assert!(StaffInvitationGrant::parse(payload).is_err());
    }
}

fn invitation() -> InvitationCredentialFacts {
    InvitationCredentialFacts {
        invitation_id: "synthetic-invitation".into(),
        holding_id: "holding".into(),
        company_id: "company".into(),
        person_id: "person".into(),
        email: EmailAddress::new("Staff@Example.Test").unwrap(),
        expected_email: EmailAddress::new("staff@example.test").unwrap(),
        now: 100,
        expires_at: 200,
        pending: true,
        independently_reviewed: true,
        published_role_current: true,
        person_active: true,
        person_unlinked: true,
        authorizer_current: true,
        authorizer_can_manage: true,
        scope_within_parent: true,
        grant_expires_at: 300,
        directory_only: false,
        existing_user: false,
    }
}

#[test]
fn current_reviewed_staff_invitation_allows_only_the_exact_fresh_local_identity() {
    assert!(validate_invitation_credential(&invitation()).is_ok());
    for (name, change) in [
        ("expired invitation", (0)),
        ("expired grant", 1),
        ("revoked invitation", 2),
        ("missing independent review", 3),
        ("unpublished role", 4),
        ("inactive person", 5),
        ("already linked person", 6),
        ("revoked authorizer", 7),
        ("scope exceeds parent", 8),
        ("Directory identity", 9),
        ("existing account", 10),
        ("wrong verified address", 11),
        ("authorizer lacks IAM management", 12),
    ] {
        let mut facts = invitation();
        match change {
            0 => facts.expires_at = 100,
            1 => facts.grant_expires_at = 100,
            2 => facts.pending = false,
            3 => facts.independently_reviewed = false,
            4 => facts.published_role_current = false,
            5 => facts.person_active = false,
            6 => facts.person_unlinked = false,
            7 => facts.authorizer_current = false,
            8 => facts.scope_within_parent = false,
            9 => facts.directory_only = true,
            10 => facts.existing_user = true,
            11 => facts.expected_email = EmailAddress::new("other@example.test").unwrap(),
            12 => facts.authorizer_can_manage = false,
            _ => unreachable!(),
        };
        assert!(validate_invitation_credential(&facts).is_err(), "{name}");
    }
}
#[test]
fn recovery_return_paths_cannot_escape_account_after_percent_decoding() {
    for path in [
        "/",
        "/onboarding",
        "/reset-password",
        "/sso/authorize?transaction=synthetic",
        "/my-profile?tab=security",
    ] {
        assert_eq!(account_return_path(Some(path)).unwrap(), path);
    }
    for path in [
        "https://evil.test/",
        "//evil.test",
        "/%2f%2fevil.test",
        "/%5cevil.test",
        "/%0d%0aLocation:evil",
        "/\\evil.test",
        "/%",
        "/%zz",
        "/\n",
        "/a b",
    ] {
        assert!(account_return_path(Some(path)).is_err(), "{path}");
    }
    assert!(account_return_path(Some(&format!("/{}", "a".repeat(4096)))).is_err());
}
#[test]
fn mail_tokens_have_distinct_reset_verification_and_session_domains() {
    let source = SystemAuthenticationSecrets;
    let token = source.issue().unwrap();
    let session = source.digest(SecretPurpose::Session, &token);
    let reset = source.digest(SecretPurpose::PasswordReset, &token);
    let verification = source.digest(SecretPurpose::EmailVerification, &token);
    let invitation = source.digest(SecretPurpose::Invitation, &token);
    assert_ne!(session, reset);
    assert_ne!(reset, verification);
    assert_ne!(verification, invitation);
}
#[test]
fn private_mail_payloads_use_authenticated_random_nonce_encryption() {
    let keyring = AccountMailKeyring::new(
        "synthetic-key-v1".into(),
        vec![("synthetic-key-v1".into(), [7; 32])],
    )
    .unwrap();
    let token = SystemAuthenticationSecrets.issue().unwrap();
    let first = keyring
        .seal(
            "synthetic-proof",
            CredentialProofKind::PasswordReset,
            "https://account.example.test",
            "/reset-password",
            &token,
        )
        .unwrap();
    let second = keyring
        .seal(
            "synthetic-proof",
            CredentialProofKind::PasswordReset,
            "https://account.example.test",
            "/reset-password",
            &token,
        )
        .unwrap();
    assert_ne!(first, second);
    assert!(!first.contains(token.expose()));
    assert!(!first.contains("reset-password"));
    assert!(keyring.open("different-proof", &first).is_err());
    let opened = keyring.open("synthetic-proof", &first).unwrap();
    assert_eq!(opened.token.expose(), token.expose());
    assert_eq!(opened.return_path, "/reset-password");
    assert_eq!(opened.account_origin, "https://account.example.test");
    assert!(opened.kind == CredentialProofKind::PasswordReset);
    let mut tampered = first.into_bytes();
    let index = tampered.len() - 1;
    tampered[index] = if tampered[index] == b'0' { b'1' } else { b'0' };
    assert!(
        keyring
            .open("synthetic-proof", std::str::from_utf8(&tampered).unwrap())
            .is_err()
    );
}
#[test]
fn mail_key_rotation_can_read_retained_keys_and_rejects_unknown_or_wrong_keys() {
    let old = AccountMailKeyring::new("old".into(), vec![("old".into(), [7; 32])]).unwrap();
    let token = SystemAuthenticationSecrets.issue().unwrap();
    let payload = old
        .seal(
            "synthetic-proof",
            CredentialProofKind::EmailVerification,
            "https://account.example.test",
            "/",
            &token,
        )
        .unwrap();
    let rotated = AccountMailKeyring::new(
        "new".into(),
        vec![("old".into(), [7; 32]), ("new".into(), [9; 32])],
    )
    .unwrap();
    assert_eq!(
        rotated
            .open("synthetic-proof", &payload)
            .unwrap()
            .token
            .expose(),
        token.expose()
    );
    let missing = AccountMailKeyring::new("new".into(), vec![("new".into(), [9; 32])]).unwrap();
    assert!(missing.open("synthetic-proof", &payload).is_err());
    let wrong = AccountMailKeyring::new("old".into(), vec![("old".into(), [8; 32])]).unwrap();
    assert!(wrong.open("synthetic-proof", &payload).is_err());
    assert!(AccountMailKeyring::new("zero".into(), vec![("zero".into(), [0; 32])]).is_err());
    assert!(AccountMailKeyring::new("missing".into(), vec![("old".into(), [7; 32])]).is_err());
}
