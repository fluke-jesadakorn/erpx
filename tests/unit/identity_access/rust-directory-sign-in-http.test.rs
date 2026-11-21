//! Native Directory sign-in router transport. A configured composition merges
//! `directory_account_router`; an unconfigured one leaves the path to the
//! router fallback. Neither case touches Account session storage for an unknown
//! connection or failed provider credential check.
#[path = "../../support/rust-account.rs"]
mod fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use erpx_server_identity_access::public::directory::{
    CompleteDirectorySnapshot, DirectoryAccountIssuance, DirectoryAttemptCounter,
    DirectoryAuthentication, DirectoryConnectionConfig, DirectoryConnectionDescriptor,
    DirectoryIssueFuture, DirectoryMode, DirectoryPassword, DirectoryProvider,
    DirectoryProviderRead, DirectoryRoutes, DirectorySynchronization, SynchronizationOutcome,
    VerifiedDirectorySubject, directory_account_router,
};
use erpx_server_identity_access::public::*;
use erpx_server_runtime::public::foundation_router;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tower::ServiceExt;

const CONNECTION: &str = "11111111-1111-4111-8111-111111111111";

#[derive(Default)]
struct UntouchedSessions(AtomicUsize);
impl UntouchedSessions {
    fn touch(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn calls(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}
impl AccountRepository for UntouchedSessions {
    fn reserve_sign_in(&self, _: ReserveSignIn) -> AccountFuture<'_, SignInReservation> {
        self.touch();
        Box::pin(async { panic!("Directory sign-in must not reserve an Account sign-in") })
    }
    fn complete_sign_in(&self, _: CompleteSignIn) -> AccountFuture<'_, CompletedSignIn> {
        self.touch();
        Box::pin(async { panic!("Directory sign-in must not complete an Account sign-in") })
    }
    fn release_sign_in(&self, _: SignInReservation) -> AccountFuture<'_, ()> {
        self.touch();
        Box::pin(async { panic!("Directory sign-in must not release an Account sign-in") })
    }
    fn session(
        &self,
        _: DeviceProof,
        _: RequestAudience,
        _: i64,
    ) -> AccountFuture<'_, Option<AccountSession>> {
        self.touch();
        Box::pin(async { panic!("Directory sign-in must not read an Account session") })
    }
    fn devices(&self, _: DeviceProof, _: i64) -> AccountFuture<'_, DeviceAccounts> {
        self.touch();
        Box::pin(async { panic!("Directory sign-in must not read Account devices") })
    }
    fn initialize_device(
        &self,
        _: DeviceProof,
        _: SecretDigest,
        _: Option<i64>,
        _: i64,
    ) -> AccountFuture<'_, (DeviceAccounts, bool)> {
        self.touch();
        Box::pin(async { panic!("Directory sign-in must not initialize an Account device") })
    }
    fn select_device(&self, _: DeviceSelection) -> AccountFuture<'_, SelectedDevice> {
        self.touch();
        Box::pin(async { panic!("Directory sign-in must not select an Account device") })
    }
    fn remove_device(&self, _: DeviceSelection) -> AccountFuture<'_, DeviceAccounts> {
        self.touch();
        Box::pin(async { panic!("Directory sign-in must not remove an Account device") })
    }
    fn sign_out(&self, _: DeviceProof, _: RequestAudience, _: i64) -> AccountFuture<'_, ()> {
        self.touch();
        Box::pin(async { panic!("Directory sign-in must not sign out an Account") })
    }
}
struct UnusedPasswords;
impl PasswordHashing for UnusedPasswords {
    fn hash(&self, _: Password) -> AccountFuture<'_, PasswordHash> {
        Box::pin(async { panic!("Directory sign-in must not hash a local password") })
    }
    fn verify(&self, _: Password, _: Option<PasswordHash>) -> AccountFuture<'_, bool> {
        Box::pin(async { panic!("Directory sign-in must not verify a local password") })
    }
}
struct UnknownConnection;
impl DirectoryRoutes for UnknownConnection {
    fn current<'a>(
        &'a self,
        _: &'a str,
    ) -> AccountFuture<'a, DirectoryConnectionDescriptor> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "DIRECTORY_SECRET_UNAVAILABLE",
                "The selected approved Directory credential profile is unavailable.",
            ))
        })
    }
}
struct DescribedConnection(DirectoryConnectionDescriptor);
impl DirectoryRoutes for DescribedConnection {
    fn current<'a>(
        &'a self,
        _: &'a str,
    ) -> AccountFuture<'a, DirectoryConnectionDescriptor> {
        let connection = self.0.clone();
        Box::pin(async move { Ok(connection) })
    }
}
struct RejectedProvider;
impl DirectoryProviderRead for RejectedProvider {
    fn verify<'a>(
        &'a self,
        _: &'a DirectoryConnectionDescriptor,
        _: &'a str,
        _: DirectoryPassword,
    ) -> AccountFuture<'a, VerifiedDirectorySubject> {
        Box::pin(async { panic!("the provider must reject before returning a subject") })
    }
    fn observe<'a>(
        &'a self,
        _: &'a DirectoryConnectionDescriptor,
        _: i64,
    ) -> AccountFuture<'a, CompleteDirectorySnapshot> {
        Box::pin(async { panic!("a rejected credential is never observed") })
    }
}
struct FailedProvider;
impl DirectoryProviderRead for FailedProvider {
    fn verify<'a>(
        &'a self,
        _: &'a DirectoryConnectionDescriptor,
        _: &'a str,
        _: DirectoryPassword,
    ) -> AccountFuture<'a, VerifiedDirectorySubject> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unauthenticated,
                "DIRECTORY_LOGIN_FAILED",
                "Directory sign-in failed.",
            ))
        })
    }
    fn observe<'a>(
        &'a self,
        _: &'a DirectoryConnectionDescriptor,
        _: i64,
    ) -> AccountFuture<'a, CompleteDirectorySnapshot> {
        Box::pin(async { panic!("a rejected credential is never observed") })
    }
}
struct CountingAttempts(AtomicUsize);
impl DirectoryAttemptCounter for CountingAttempts {
    fn attempt<'a>(&'a self, _: &'a str, _: &'a str) -> AccountFuture<'a, ()> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }
}
struct UnavailableAttempts;
impl DirectoryAttemptCounter for UnavailableAttempts {
    fn attempt<'a>(&'a self, _: &'a str, _: &'a str) -> AccountFuture<'a, ()> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "DIRECTORY_ATTEMPT_UNAVAILABLE",
                "The Directory attempt counter is unavailable.",
            ))
        })
    }
}
struct UnusedSynchronization;
impl DirectorySynchronization for UnusedSynchronization {
    fn apply<'a>(
        &'a self,
        _: &'a DirectoryConnectionDescriptor,
        _: CompleteDirectorySnapshot,
    ) -> AccountFuture<'a, SynchronizationOutcome> {
        Box::pin(async { panic!("no observation is applied") })
    }
    fn failed<'a>(
        &'a self,
        _: &'a DirectoryConnectionDescriptor,
        _: i64,
        _: &'static str,
    ) -> AccountFuture<'a, ()> {
        Box::pin(async { panic!("no observation failure is recorded") })
    }
}
struct UnusedIssuance;
impl DirectoryAccountIssuance for UnusedIssuance {
    fn issue<'a>(
        &'a self,
        _: VerifiedDirectorySubject,
        _: DirectoryConnectionDescriptor,
        _: &'a PresentedSecrets,
        _: Option<i64>,
    ) -> DirectoryIssueFuture<'a> {
        Box::pin(async { panic!("no Directory subject is issued") })
    }
}
fn account(repository: Arc<UntouchedSessions>) -> Arc<AccountAuthentication> {
    Arc::new(
        AccountAuthentication::new(
            fixture::policy(),
            repository,
            Arc::new(UnusedPasswords),
            Arc::new(SystemAuthenticationSecrets),
            Arc::new(fixture::FixedClock),
        )
        .unwrap(),
    )
}
fn directory(
    routes: Arc<dyn DirectoryRoutes>,
    provider: Arc<dyn DirectoryProviderRead>,
    attempts: Arc<dyn DirectoryAttemptCounter>,
) -> Arc<DirectoryAuthentication> {
    Arc::new(DirectoryAuthentication::new(
        routes,
        provider,
        Arc::new(UnusedSynchronization),
        attempts,
        Arc::new(UnusedIssuance),
        Arc::new(fixture::FixedClock),
    ))
}
fn descriptor() -> DirectoryConnectionDescriptor {
    DirectoryConnectionDescriptor {
        source: None,
        connection_id: CONNECTION.into(),
        holding_id: None,
        company_id: "platform".into(),
        provider: DirectoryProvider::ActiveDirectory,
        mode: DirectoryMode::Ldaps,
        version: 1,
        tested_version: Some(1),
        config_fingerprint: "config".into(),
        profile_fingerprint: "profile".into(),
        config: DirectoryConnectionConfig {
            mode: DirectoryMode::Ldaps,
            secret_profile: "fixture".into(),
            authority: "ldaps://directory.example.test".into(),
            ldap_url: Some("ldaps://directory.example.test".into()),
            base_dn: Some("dc=example,dc=test".into()),
            group_base_dn: None,
            user_filter: None,
            issuer: None,
            tenant_id: None,
            client_id: None,
        },
    }
}
fn sign_in() -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/auth/directory/sign-in")
        .header("host", "account.example.test")
        .header("origin", "https://account.example.test")
        .header("content-type", "application/json")
        .body(Body::from(format!(
            r#"{{"connectionId":"{CONNECTION}","username":"person","password":"credential"}}"#
        )))
        .unwrap()
}
fn router(
    account: Arc<AccountAuthentication>,
    directory: Arc<DirectoryAuthentication>,
) -> Router {
    foundation_router().merge(directory_account_router(account, directory))
}

#[tokio::test]
async fn an_unconfigured_router_does_not_serve_the_directory_sign_in_path() {
    let response = foundation_router().oneshot(sign_in()).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_unknown_connection_is_rejected_without_touching_account_sessions() {
    let sessions = Arc::new(UntouchedSessions::default());
    let attempts = Arc::new(CountingAttempts(AtomicUsize::new(0)));
    let app = router(
        account(sessions.clone()),
        directory(Arc::new(UnknownConnection), Arc::new(RejectedProvider), attempts.clone()),
    );
    let response = app.oneshot(sign_in()).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(attempts.0.load(Ordering::SeqCst), 1);
    assert_eq!(sessions.calls(), 0);
}

#[tokio::test]
async fn an_unavailable_attempt_store_is_service_unavailable_without_account_sessions() {
    let sessions = Arc::new(UntouchedSessions::default());
    let app = router(
        account(sessions.clone()),
        directory(
            Arc::new(UnknownConnection),
            Arc::new(RejectedProvider),
            Arc::new(UnavailableAttempts),
        ),
    );
    let response = app.oneshot(sign_in()).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(sessions.calls(), 0);
}

#[tokio::test]
async fn a_failed_provider_credential_check_is_unauthorized_without_account_sessions() {
    let sessions = Arc::new(UntouchedSessions::default());
    let attempts = Arc::new(CountingAttempts(AtomicUsize::new(0)));
    let app = router(
        account(sessions.clone()),
        directory(
            Arc::new(DescribedConnection(descriptor())),
            Arc::new(FailedProvider),
            attempts.clone(),
        ),
    );
    let response = app.oneshot(sign_in()).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(attempts.0.load(Ordering::SeqCst), 1);
    assert_eq!(sessions.calls(), 0);
}

#[tokio::test]
async fn a_non_account_host_is_not_found() {
    let sessions = Arc::new(UntouchedSessions::default());
    let attempts = Arc::new(CountingAttempts(AtomicUsize::new(0)));
    let app = router(
        account(sessions.clone()),
        directory(Arc::new(UnknownConnection), Arc::new(RejectedProvider), attempts),
    );
    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/directory/sign-in")
        .header("host", "acme.example.test")
        .header("origin", "https://account.example.test")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(sessions.calls(), 0);
}
