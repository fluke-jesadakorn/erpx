use erpx_server_identity_access::public::account_sso::*;
use erpx_server_identity_access::public::{AccountSession, AccountUser, SESSION_LIFETIME_MS};
#[path = "../../support/rust-account.rs"]
#[allow(
    dead_code,
    reason = "This pure protocol suite uses the shared policy fixture only."
)]
mod fixture;
fn digest() -> SsoDigest {
    SystemSsoCryptography.digest("public-synthetic-value")
}
fn browser(now: i64) -> BrowserSsoTransaction {
    let policy = SsoPolicy::new(fixture::policy()).unwrap();
    BrowserSsoTransaction {
        id: SystemSsoCryptography.transaction_id().unwrap(),
        browser_hash: digest(),
        state_hash: digest(),
        challenge: digest(),
        audience: "holding:development:h001:acme.example.test".into(),
        environment: "development".into(),
        callback: policy
            .callback("holding:development:h001:acme.example.test")
            .unwrap(),
        return_path: SsoReturnPath::sanitize(Some("/finance")),
        created_at: now,
        expires_at: now + SSO_BROWSER_LIFETIME_MS,
        consumed_at: None,
    }
}
#[test]
fn pkce_matches_the_independent_rfc7636_appendix_b_s256_vector() {
    // Public test vector, not a credential: https://www.rfc-editor.org/rfc/rfc7636#appendix-B
    assert_eq!(
        SystemSsoCryptography
            .digest("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")
            .as_str(),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}
#[test]
fn generated_sso_proofs_preserve_the_existing_43_character_login_continuation_shape() {
    let source = SystemSsoCryptography;
    let left = source.issue().unwrap();
    let right = source.issue().unwrap();
    assert_eq!(left.expose().len(), 43);
    assert_ne!(left.expose(), right.expose());
    let cookie = SsoBrowserCookie {
        transaction_id: source.transaction_id().unwrap(),
        browser: left,
        verifier: right,
    };
    let parsed = SsoBrowserCookie::parse(&cookie.cookie_value()).unwrap();
    assert_eq!(parsed.transaction_id, cookie.transaction_id);
    assert_eq!(parsed.browser.expose(), cookie.browser.expose());
    for invalid in ["", "one.two", "one.two.three.four", "../cookie"] {
        assert!(SsoBrowserCookie::parse(invalid).is_err());
    }
    assert!(SsoSecret::new("a".repeat(44)).is_err());
}
#[test]
fn return_paths_retain_company_and_repeated_filters_without_auth_credentials_or_fragments() {
    for (input, expected) in [
        (
            "/bangkok/finance?company=acme&status=paid&status=open&code=old&state=old&token=old&session_token=old&_rsc=old#private",
            "/bangkok/finance?company=acme&status=paid&status=open",
        ),
        (
            "/finance/../people?search=one%20two",
            "/people?search=one+two",
        ),
        (
            "/launch?next=%2Fbangkok%2Ffinance%3Fcompany%3Dacme",
            "/launch?next=%2Fbangkok%2Ffinance%3Fcompany%3Dacme",
        ),
    ] {
        assert_eq!(SsoReturnPath::sanitize(Some(input)).as_str(), expected);
    }
    for input in [
        "//evil.test",
        "https://evil.test",
        "/%2f%2fevil.test",
        "/%5cevil.test",
        "/sso/start",
        "/api/auth/sign-in/email",
        "/login",
        "/product-entry",
        "/finance/../sso/callback",
        "/%ff",
        "/%",
        "/a b",
    ] {
        assert_eq!(
            SsoReturnPath::sanitize(Some(input)).as_str(),
            "/",
            "{input}"
        );
    }
}
#[test]
fn browser_and_grant_lifetimes_have_exact_boundaries_and_no_future_metadata() {
    let policy = SsoPolicy::new(fixture::policy()).unwrap();
    let mut transaction = browser(1000);
    assert!(transaction.require_current(&policy, 1000).is_ok());
    assert!(transaction.require_current(&policy, 999).is_err());
    assert!(transaction.require_current(&policy, 301000).is_err());
    transaction.expires_at += 1;
    assert!(transaction.require_current(&policy, 1000).is_err());
    transaction = browser(1000);
    transaction.callback = "https://evil.test/sso/callback".into();
    assert!(transaction.require_current(&policy, 1000).is_err());
    transaction = browser(1000);
    transaction.environment = "production".into();
    assert!(transaction.require_current(&policy, 1000).is_err());
    let transaction = browser(1000);
    let grant = SsoAuthorizationGrant {
        code_hash: digest(),
        transaction_id: transaction.id.clone(),
        parent_session_id: "parent".into(),
        audience: transaction.audience.clone(),
        environment: transaction.environment.clone(),
        callback: transaction.callback.clone(),
        state_hash: transaction.state_hash.clone(),
        challenge: transaction.challenge.clone(),
        created_at: 1000,
        expires_at: 61000,
        consumed_at: None,
    };
    assert!(grant.require_current(&transaction, &policy, 60999).is_ok());
    assert!(grant.require_current(&transaction, &policy, 61000).is_err());
}
#[test]
fn grant_expiry_never_extends_the_browser_parent_or_original_authentication_window() {
    let transaction = browser(1000);
    let mut parent = AccountSession {
        id: "parent".into(),
        user: AccountUser {
            id: "user".into(),
            name: "Synthetic User".into(),
            email: "user@example.test".into(),
            email_verified: true,
            image: None,
        },
        authenticated_at: 0,
        expires_at: 5000,
    };
    assert_eq!(grant_expiry(1000, &transaction, &parent).unwrap(), 5000);
    parent.expires_at = SESSION_LIFETIME_MS + 1000;
    assert_eq!(grant_expiry(1000, &transaction, &parent).unwrap(), 61000);
    assert!(grant_expiry(SESSION_LIFETIME_MS, &transaction, &parent).is_err());
    assert!(grant_expiry(i64::MAX, &transaction, &parent).is_err());
}
