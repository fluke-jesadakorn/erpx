#[path = "../../support/rust-account.rs"]
mod fixture;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_identity_access::public::account_contracts::*;
use erpx_server_identity_access::public::*;
use std::sync::atomic::Ordering;
use tower::ServiceExt;

use std::sync::{Arc, atomic::AtomicUsize};
pub struct ProbePasswords {
    pub calls: AtomicUsize,
}
impl PasswordHashing for ProbePasswords {
    fn hash(&self, _: Password) -> AccountFuture<'_, PasswordHash> {
        Box::pin(async { panic!("hash was not expected") })
    }
    fn verify(&self, _: Password, expected: Option<PasswordHash>) -> AccountFuture<'_, bool> {
        assert!(expected.is_none());
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(false) })
    }
}
#[derive(Default)]
pub struct ProbeRepository {
    pub reservations: AtomicUsize,
    pub releases: AtomicUsize,
    pub selections: AtomicUsize,
}
impl AccountRepository for ProbeRepository {
    fn reserve_sign_in(&self, input: ReserveSignIn) -> AccountFuture<'_, SignInReservation> {
        self.reservations.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            Ok(SignInReservation {
                browser: input.candidate_browser,
                created_browser: true,
                nonce: input.nonce,
                credential: None,
            })
        })
    }
    fn complete_sign_in(&self, _: CompleteSignIn) -> AccountFuture<'_, CompletedSignIn> {
        Box::pin(async { panic!("unknown credentials must never complete") })
    }
    fn release_sign_in(&self, _: SignInReservation) -> AccountFuture<'_, ()> {
        self.releases.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }
    fn session(
        &self,
        _: DeviceProof,
        _: RequestAudience,
        _: i64,
    ) -> AccountFuture<'_, Option<AccountSession>> {
        Box::pin(async { Ok(None) })
    }
    fn devices(&self, _: DeviceProof, _: i64) -> AccountFuture<'_, DeviceAccounts> {
        Box::pin(async { Ok(DeviceAccounts::empty()) })
    }
    fn initialize_device(
        &self,
        _: DeviceProof,
        _: SecretDigest,
        _: i64,
    ) -> AccountFuture<'_, (DeviceAccounts, bool)> {
        Box::pin(async { panic!("initialization was not expected") })
    }
    fn select_device(&self, _: DeviceSelection) -> AccountFuture<'_, SelectedDevice> {
        self.selections.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { panic!("a missing saved proof must fail before selection") })
    }
    fn remove_device(&self, _: DeviceSelection) -> AccountFuture<'_, DeviceAccounts> {
        Box::pin(async { panic!("removal was not expected") })
    }
    fn sign_out(&self, _: DeviceProof, _: RequestAudience, _: i64) -> AccountFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
pub fn probe() -> (
    Arc<AccountAuthentication>,
    Arc<ProbeRepository>,
    Arc<ProbePasswords>,
) {
    let repository = Arc::new(ProbeRepository::default());
    let passwords = Arc::new(ProbePasswords {
        calls: AtomicUsize::new(0),
    });
    let service = AccountAuthentication::new(
        fixture::policy(),
        repository.clone(),
        passwords.clone(),
        Arc::new(SystemAuthenticationSecrets),
        Arc::new(fixture::FixedClock),
    )
    .unwrap();
    (Arc::new(service), repository, passwords)
}

#[test]
fn email_password_and_session_windows_reject_ambiguous_or_unbounded_input() {
    assert_eq!(
        EmailAddress::new(" Alice@Example.Test ").unwrap().as_str(),
        "alice@example.test"
    );
    for email in [
        "a@@example.test",
        "a b@example.test",
        "a@",
        "@example.test",
        "a@localhost",
        "a..b@example.test",
        "a@example.12",
        "person@-invalid.test",
        "ไทย@example.test",
        "a\n@example.test",
    ] {
        assert!(EmailAddress::new(email).is_err());
    }
    assert!(Password::new_credential("short".into()).is_err());
    assert!(Password::new_credential("twelve-characters".into()).is_ok());
    assert!(Password::new_credential("🔐".repeat(6)).is_ok());
    assert!(Password::new_credential("🔐".repeat(5)).is_err());
    assert!(Password::sign_in("a".repeat(4097)).is_err());
    assert!(valid_session_window(100, 100 + SESSION_LIFETIME_MS, 101));
    for (a, e, n) in [
        (100, 100 + SESSION_LIFETIME_MS + 1, 101),
        (100, 200, 99),
        (100, 100, 100),
        (100, 200, 200),
        (i64::MIN, i64::MAX, 0),
    ] {
        assert!(!valid_session_window(a, e, n));
    }
}
#[test]
fn revision_contract_rejects_lossy_and_fractional_javascript_numbers() {
    for input in ["1.0", "1e0", "9007199254740991.0"] {
        let value: AccountRevision = serde_json::from_str(input).unwrap();
        assert_eq!(
            value.get(),
            if input.starts_with("9007") {
                9_007_199_254_740_991
            } else {
                1
            }
        );
    }
    for revision in [0, 1, 9_007_199_254_740_991] {
        let value = AccountRevision::new(revision).unwrap();
        assert_eq!(serde_json::to_string(&value).unwrap(), revision.to_string());
        assert_eq!(
            serde_json::from_str::<AccountRevision>(&revision.to_string())
                .unwrap()
                .get(),
            revision
        );
    }
    for input in [
        "-1",
        "9007199254740992",
        "9223372036854775807",
        "1.5",
        "null",
        "\"1\"",
    ] {
        assert!(
            serde_json::from_str::<AccountRevision>(input).is_err(),
            "{input}"
        );
    }
    let mut devices = DeviceAccounts::empty();
    devices.revision = i64::MAX;
    assert!(DeviceAccountsDto::try_from(devices).is_err());
}
#[test]
fn authority_is_exact_and_never_accepts_a_host_suffix_or_foreign_origin() {
    let p = fixture::policy();
    assert!(p.validate().is_ok());
    assert_eq!(
        p.audience("account.example.test"),
        Some(RequestAudience::Account)
    );
    assert!(matches!(
        p.audience("acme.example.test"),
        Some(RequestAudience::Holding { .. })
    ));
    for host in [
        "account.example.test.evil.test",
        "ACCOUNT.example.test",
        "account.example.test:443",
        "unknown.example.test",
    ] {
        assert!(p.audience(host).is_none());
    }
    assert!(!p.device_origin_allowed("https://acme.example.test.evil.test"));
    assert!(!p.device_origin_allowed("http://acme.example.test"));
    let mut invalid = p.clone();
    invalid.environment = "production".into();
    assert!(invalid.validate().is_err());
    invalid = p.clone();
    invalid.holdings[0].host = invalid.account_host.clone();
    assert!(invalid.validate().is_err());
}
#[test]
fn issued_secrets_are_random_and_domain_separated() {
    let source = SystemAuthenticationSecrets;
    let a = source.issue().unwrap();
    let b = source.issue().unwrap();
    assert_ne!(a.expose(), b.expose());
    assert_eq!(a.expose().len(), 64);
    let session = source.digest(SecretPurpose::Session, &a);
    let device = source.digest(SecretPurpose::DeviceBrowser, &a);
    let reservation = source.digest(SecretPurpose::Reservation, &a);
    assert_ne!(session, device);
    assert_ne!(session, reservation);
    assert_ne!(session.as_str(), a.expose());
    assert_eq!(source.identifier().unwrap().len(), 36);
}
#[tokio::test]
async fn unknown_credentials_do_bounded_password_work_and_release_the_nonce() {
    let (service, repository, passwords) = probe();
    fixture::assert_error(
        service
            .sign_in(
                "missing@example.test",
                "not-a-secret".into(),
                &fixture::empty(),
                None,
            )
            .await,
        "INVALID_EMAIL_OR_PASSWORD",
    );
    assert_eq!(repository.reservations.load(Ordering::SeqCst), 1);
    assert_eq!(passwords.calls.load(Ordering::SeqCst), 1);
    assert_eq!(repository.releases.load(Ordering::SeqCst), 1);
    fixture::assert_error(
        service
            .sign_in(
                "missing@example.test",
                "not-a-secret".into(),
                &fixture::empty(),
                Some(9_007_199_254_740_992),
            )
            .await,
        "INVALID_DEVICE_REVISION",
    );
    assert_eq!(repository.reservations.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn selecting_a_handle_without_its_own_cookie_cannot_commit_a_selection() {
    let (service, repository, _) = probe();
    assert!(
        service
            .select(
                &fixture::empty(),
                "00000000-0000-4000-8000-000000000001".into(),
                0
            )
            .await
            .is_err()
    );
    assert_eq!(repository.selections.load(Ordering::SeqCst), 0);
}
async fn request(
    router: axum::Router,
    path: &str,
    method: &str,
    host: &str,
    origin: Option<&str>,
    body: String,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .uri(path)
        .method(method)
        .header("host", host)
        .header("content-type", "application/json");
    if let Some(origin) = origin {
        builder = builder.header("origin", origin);
    }
    router
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap()
}
#[tokio::test]
async fn http_fences_run_before_password_or_database_work() {
    let (service, repository, passwords) = probe();
    let router = account_auth_router(service);
    for (host, origin, status) in [
        (
            "unknown.example.test",
            Some("https://account.example.test"),
            StatusCode::NOT_FOUND,
        ),
        (
            "account.example.test.evil.test",
            Some("https://account.example.test"),
            StatusCode::NOT_FOUND,
        ),
        (
            "acme.example.test",
            Some("https://acme.example.test"),
            StatusCode::NOT_FOUND,
        ),
        ("account.example.test", None, StatusCode::FORBIDDEN),
        (
            "account.example.test",
            Some("https://evil.test"),
            StatusCode::FORBIDDEN,
        ),
        (
            "account.example.test",
            Some("https://acme.example.test"),
            StatusCode::FORBIDDEN,
        ),
    ] {
        let response = request(
            router.clone(),
            "/api/auth/sign-in/email",
            "POST",
            host,
            origin,
            r#"{"email":"a@example.test","password":"fake"}"#.into(),
        )
        .await;
        assert_eq!(response.status(), status);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert!(!response.headers().contains_key("set-cookie"));
    }
    for body in [
        r#"{"email":"a@example.test","password":"fake","unknown":true}"#.into(),
        r#"{"email":"a@example.test","password":"fake","revision":9007199254740992}"#.into(),
        r#"{"email":"a@example.test","password":"fake","callbackURL":"//evil.test"}"#.into(),
        "a".repeat(8193),
    ] {
        assert_eq!(
            request(
                router.clone(),
                "/api/auth/sign-in/email",
                "POST",
                "account.example.test",
                Some("https://account.example.test"),
                body
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(repository.reservations.load(Ordering::SeqCst), 0);
    assert_eq!(passwords.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        request(
            router,
            "/api/auth/sign-in/email",
            "POST",
            "account.example.test",
            Some("https://account.example.test"),
            r#"{"email":"a@example.test","password":"fake"}"#.into()
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(passwords.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn device_cors_uses_exact_registered_origins_and_rejects_unapproved_headers() {
    let (service, _, _) = probe();
    let router = account_auth_router(service);
    let build = |origin: &str, headers: &str| {
        Request::builder()
            .uri("/api/v1/account/device-accounts/active")
            .method("OPTIONS")
            .header("host", "account.example.test")
            .header("origin", origin)
            .header("access-control-request-method", "POST")
            .header("access-control-request-headers", headers)
            .body(Body::empty())
            .unwrap()
    };
    let response = router
        .clone()
        .oneshot(build("https://acme.example.test", "content-type"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "https://acme.example.test"
    );
    assert_eq!(
        response.headers()["access-control-allow-credentials"],
        "true"
    );
    assert_eq!(
        router
            .clone()
            .oneshot(build("https://acme.example.test.evil.test", "content-type"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        router
            .oneshot(build("https://acme.example.test", "authorization"))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
}
#[tokio::test]
async fn session_json_never_returns_a_raw_or_hashed_cookie_proof() {
    let (service, _, _) = probe();
    let response = request(
        account_auth_router(service),
        "/api/auth/get-session",
        "GET",
        "account.example.test",
        None,
        String::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(to_bytes(response.into_body(), 8192).await.unwrap(), "null");
}
#[tokio::test]
async fn argon2_fresh_credentials_use_independent_salts_and_constant_parameters() {
    let passwords = Argon2Passwords::new(1).unwrap();
    let first = passwords
        .hash(Password::new_credential("synthetic-fixture-password".into()).unwrap())
        .await
        .unwrap();
    let second = passwords
        .hash(Password::new_credential("synthetic-fixture-password".into()).unwrap())
        .await
        .unwrap();
    assert_ne!(first.as_str(), second.as_str());
    assert!(
        first
            .as_str()
            .starts_with("$argon2id$v=19$m=65536,t=3,p=1$")
    );
    assert!(
        passwords
            .verify(
                Password::sign_in("synthetic-fixture-password".into()).unwrap(),
                Some(first.clone())
            )
            .await
            .unwrap()
    );
    assert!(
        !passwords
            .verify(
                Password::sign_in("wrong-fixture-password".into()).unwrap(),
                Some(first)
            )
            .await
            .unwrap()
    );
    assert!(
        !passwords
            .verify(
                Password::sign_in("synthetic-fixture-password".into()).unwrap(),
                None
            )
            .await
            .unwrap()
    );
}
