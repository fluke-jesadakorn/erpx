#[path = "../../support/rust-postgres.rs"]
mod database;
#[path = "../../support/rust-account-credentials.rs"]
mod fixture;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_identity_access::public::account_sso::*;
use erpx_server_identity_access::public::*;
use erpx_server_runtime::public::postgres::DatabasePool;
use std::sync::Arc;
use tower::ServiceExt;
use url::Url;
const ACME: &str = "holding:development:h001:acme.example.test";
const NORTHSTAR: &str = "holding:development:h002:northstar.example.test";
async fn setup(
    pool: &DatabasePool,
    authority: Arc<dyn InvitationCredentialAuthority>,
) -> fixture::Scenario {
    let scenario = fixture::setup(pool, authority).await;
    sqlx::query("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding) VALUES('h002','northstar','Synthetic Northstar','HOLDING_H002_DB','HOLDING_H002_READ_DB','HOLDING_H002_FILES')").execute(pool.raw()).await.unwrap();
    scenario
}
fn sso(pool: &DatabasePool, scenario: &fixture::Scenario) -> Arc<AccountSso> {
    let policy = SsoPolicy::new(fixture::account::policy()).unwrap();
    Arc::new(AccountSso::new(
        policy.clone(),
        Arc::new(PostgresAccountSso::from_control_database(
            pool.clone(),
            policy,
            scenario.clock.clone(),
            fixture::account::holding_registry(),
        )),
        Arc::new(SystemSsoCryptography),
        Arc::new(SystemAuthenticationSecrets),
        scenario.clock.clone(),
    ))
}
async fn login(scenario: &fixture::Scenario, email: &str) -> (AccountSession, PresentedSecrets) {
    let signed = scenario
        .auth
        .sign_in(
            email,
            fixture::PASSWORD.into(),
            &fixture::account::empty(),
            None,
        )
        .await
        .unwrap();
    let parent = signed.result.session.clone();
    (parent, fixture::remember(signed))
}
async fn flow(
    service: &AccountSso,
    proof: &PresentedSecrets,
    audience: &str,
) -> (StartedSso, AuthorizedSso) {
    let started = service
        .start(
            audience,
            Some("/bangkok/finance?company=acme&status=paid&status=open&code=old#private"),
        )
        .await
        .unwrap();
    let grant = service
        .authorize(
            started.transaction_id.clone(),
            &started.state,
            started.challenge.clone(),
            proof,
        )
        .await
        .unwrap()
        .unwrap();
    (started, grant)
}
async fn consume(
    service: &AccountSso,
    flow: &(StartedSso, AuthorizedSso),
    audience: &str,
) -> IssuedSso {
    service
        .consume(
            audience,
            &flow.1.code,
            &flow.0.state,
            flow.0.browser_cookie.clone(),
        )
        .await
        .unwrap()
}
fn proof(token: OpaqueSecret) -> PresentedSecrets {
    PresentedSecrets {
        browser: None,
        active: Some(token),
        saved: vec![],
    }
}
fn holding(audience: &str) -> RequestAudience {
    RequestAudience::Holding {
        audience: audience.into(),
    }
}

#[tokio::test]
async fn real_cross_holding_children_preserve_parent_time_expiry_hashing_and_exact_audience() {
    database::with_database(2, |pool| async move {
        let scenario = setup(&pool, Arc::new(UnavailableInvitationCredentialAuthority)).await;
        let service = sso(&pool, &scenario);
        let (parent, account) = login(&scenario, "a@example.test").await;
        let issued = consume(&service, &flow(&service, &account, ACME).await, ACME).await;
        let second = consume(
            &service,
            &flow(&service, &account, NORTHSTAR).await,
            NORTHSTAR,
        )
        .await;
        assert_eq!(
            issued.result.return_path.as_str(),
            "/bangkok/finance?company=acme&status=paid&status=open"
        );
        let stored: String = sqlx::query_scalar("SELECT token FROM session WHERE id=$1")
            .bind(&issued.result.session_id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_ne!(stored, issued.token.expose());
        assert_eq!(
            stored,
            SystemAuthenticationSecrets
                .digest(SecretPurpose::Session, &issued.token)
                .as_str()
        );
        let child = proof(issued.token);
        let current = scenario
            .auth
            .session(&child, holding(ACME))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(current.authenticated_at, parent.authenticated_at);
        assert_eq!(current.expires_at, parent.expires_at);
        assert_eq!(current.user.id, parent.user.id);
        assert!(
            scenario
                .auth
                .session(&child, holding(NORTHSTAR))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            scenario
                .auth
                .session(&child, RequestAudience::Account)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            scenario
                .auth
                .session(&proof(second.token), holding(NORTHSTAR))
                .await
                .unwrap()
                .is_some()
        );
    })
    .await;
}

#[tokio::test]
async fn malformed_state_browser_pkce_and_foreign_grant_leave_the_valid_exchange_unconsumed() {
    database::with_database(2, |pool| async move {
        let scenario = setup(&pool, Arc::new(UnavailableInvitationCredentialAuthority)).await;
        let service = sso(&pool, &scenario);
        let (_, account) = login(&scenario, "a@example.test").await;
        let f = flow(&service, &account, ACME).await;
        let wrong = SystemSsoCryptography.issue().unwrap();
        fixture::account::assert_error(
            service
                .consume(ACME, &f.1.code, &wrong, f.0.browser_cookie.clone())
                .await,
            "INVALID_SSO_TRANSACTION",
        );
        assert!(
            service
                .consume(NORTHSTAR, &f.1.code, &f.0.state, f.0.browser_cookie.clone())
                .await
                .is_err()
        );
        let mut browser = f.0.browser_cookie.clone();
        browser.browser = wrong.clone();
        assert!(
            service
                .consume(ACME, &f.1.code, &f.0.state, browser)
                .await
                .is_err()
        );
        let mut browser = f.0.browser_cookie.clone();
        browser.verifier = wrong;
        assert!(
            service
                .consume(ACME, &f.1.code, &f.0.state, browser)
                .await
                .is_err()
        );
        let other = flow(&service, &account, ACME).await;
        assert!(
            service
                .consume(ACME, &other.1.code, &f.0.state, f.0.browser_cookie.clone())
                .await
                .is_err()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM sso_authorization_grants WHERE consumed_at IS NOT NULL"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
        );
        let issued = consume(&service, &f, ACME).await;
        assert!(
            scenario
                .auth
                .session(&proof(issued.token), holding(ACME))
                .await
                .unwrap()
                .is_some()
        );
    })
    .await;
}

#[tokio::test]
async fn concurrent_callbacks_commit_one_child_and_one_pair_of_consumption_marks_on_one_connection()
{
    database::with_database(1, |pool| async move {
        let scenario = setup(&pool, Arc::new(UnavailableInvitationCredentialAuthority)).await;
        let service = sso(&pool, &scenario);
        let (parent, account) = login(&scenario, "a@example.test").await;
        let f = flow(&service, &account, ACME).await;
        let (left, right) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            tokio::join!(
                service.consume(ACME, &f.1.code, &f.0.state, f.0.browser_cookie.clone()),
                service.consume(ACME, &f.1.code, &f.0.state, f.0.browser_cookie.clone())
            )
        })
        .await
        .unwrap();
        assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM sso_session_bindings WHERE parent_session_id=$1"
            )
            .bind(parent.id)
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM sso_browser_transactions WHERE consumed_at IS NOT NULL"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM sso_authorization_grants WHERE consumed_at IS NOT NULL"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
    })
    .await;
}

#[tokio::test]
async fn exact_grant_and_browser_expiry_boundaries_reject_without_refreshing_parent_authentication()
{
    database::with_database(2, |pool| async move {
        let scenario = setup(&pool, Arc::new(UnavailableInvitationCredentialAuthority)).await;
        let service = sso(&pool, &scenario);
        let (_, account) = login(&scenario, "a@example.test").await;
        let f = flow(&service, &account, ACME).await;
        scenario.clock.advance(60_000);
        assert!(
            service
                .consume(ACME, &f.1.code, &f.0.state, f.0.browser_cookie.clone())
                .await
                .is_err()
        );
        let started = service.start(ACME, None).await.unwrap();
        scenario.clock.advance(300_000);
        assert!(
            service
                .authorize(
                    started.transaction_id,
                    &started.state,
                    started.challenge,
                    &account
                )
                .await
                .is_err()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM sso_session_bindings WHERE parent_session_id IS NOT NULL"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
        );
    })
    .await;
}

#[tokio::test]
async fn binding_failure_rolls_back_the_child_and_both_one_time_marks_then_allows_a_real_retry() {
    database::with_database(2,|pool|async move{
 let scenario=setup(&pool,Arc::new(UnavailableInvitationCredentialAuthority)).await;let service=sso(&pool,&scenario);let(_,account)=login(&scenario,"a@example.test").await;let f=flow(&service,&account,ACME).await;
 sqlx::raw_sql("CREATE FUNCTION reject_child_binding() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.parent_session_id IS NOT NULL THEN RAISE EXCEPTION 'synthetic SSO binding failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_child_binding BEFORE INSERT ON sso_session_bindings FOR EACH ROW EXECUTE FUNCTION reject_child_binding();").execute(pool.raw()).await.unwrap();assert!(service.consume(ACME,&f.1.code,&f.0.state,f.0.browser_cookie.clone()).await.is_err());assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM session").fetch_one(pool.raw()).await.unwrap(),1);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM sso_browser_transactions WHERE consumed_at IS NOT NULL").fetch_one(pool.raw()).await.unwrap(),0);sqlx::raw_sql("DROP TRIGGER reject_child_binding ON sso_session_bindings").execute(pool.raw()).await.unwrap();let issued=consume(&service,&f,ACME).await;assert!(scenario.auth.session(&proof(issued.token),holding(ACME)).await.unwrap().is_some());
}).await;
}

#[tokio::test]
async fn serialization_retry_replays_only_the_rolled_back_control_unit_and_mints_one_child() {
    database::with_database(2,|pool|async move{
 let scenario=setup(&pool,Arc::new(UnavailableInvitationCredentialAuthority)).await;let service=sso(&pool,&scenario);let(_,account)=login(&scenario,"a@example.test").await;let f=flow(&service,&account,ACME).await;
 sqlx::raw_sql("CREATE SEQUENCE sso_attempts; CREATE FUNCTION retry_child_binding() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.parent_session_id IS NOT NULL AND nextval('sso_attempts')=1 THEN RAISE EXCEPTION 'synthetic serialization retry' USING ERRCODE='40001'; END IF; RETURN NEW; END $$; CREATE TRIGGER retry_child_binding BEFORE INSERT ON sso_session_bindings FOR EACH ROW EXECUTE FUNCTION retry_child_binding();").execute(pool.raw()).await.unwrap();let issued=consume(&service,&f,ACME).await;assert!(scenario.auth.session(&proof(issued.token),holding(ACME)).await.unwrap().is_some());assert_eq!(sqlx::query_scalar::<_,i64>("SELECT last_value FROM sso_attempts").fetch_one(pool.raw()).await.unwrap(),2);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM sso_session_bindings WHERE parent_session_id IS NOT NULL").fetch_one(pool.raw()).await.unwrap(),1);
}).await;
}

#[tokio::test]
async fn parent_expiry_reduction_is_reflected_and_a_child_cannot_refresh_authentication_time() {
    database::with_database(2,|pool|async move{
 let scenario=setup(&pool,Arc::new(UnavailableInvitationCredentialAuthority)).await;let service=sso(&pool,&scenario);let(parent,account)=login(&scenario,"a@example.test").await;let issued=consume(&service,&flow(&service,&account,ACME).await,ACME).await;let id=issued.result.session_id.clone();let child=proof(issued.token);
 sqlx::query("UPDATE session SET \"expiresAt\"='2027-01-15T08:00:01Z' WHERE id=$1").bind(&parent.id).execute(pool.raw()).await.unwrap();assert_eq!(scenario.auth.session(&child,holding(ACME)).await.unwrap().unwrap().expires_at,fixture::account::NOW+1000);
 sqlx::query("UPDATE sso_session_bindings SET authenticated_at=authenticated_at+1 WHERE session_id=$1").bind(&id).execute(pool.raw()).await.unwrap();assert!(scenario.auth.session(&child,holding(ACME)).await.unwrap().is_none());
}).await;
}

#[tokio::test]
async fn a_pending_grant_cannot_escape_account_switch_or_current_principal_suspension() {
    database::with_database(2,|pool|async move{
 let scenario=setup(&pool,Arc::new(UnavailableInvitationCredentialAuthority)).await;let service=sso(&pool,&scenario);let(_,account)=login(&scenario,"a@example.test").await;let f=flow(&service,&account,ACME).await;
 let b=scenario.auth.sign_in("b@example.test",fixture::PASSWORD.into(),&account,Some(1)).await.unwrap();assert!(service.consume(ACME,&f.1.code,&f.0.state,f.0.browser_cookie.clone()).await.is_err());
 let mut current=account;current.saved.push((b.handle,b.session_secret.clone()));current.active=Some(b.session_secret);let f=flow(&service,&current,ACME).await;sqlx::query("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES('sso-suspended','fixture-b','Synthetic B','suspended',$1)").bind(fixture::account::NOW).execute(pool.raw()).await.unwrap();assert!(service.consume(ACME,&f.1.code,&f.0.state,f.0.browser_cookie).await.is_err());assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM sso_authorization_grants WHERE consumed_at IS NOT NULL").fetch_one(pool.raw()).await.unwrap(),0);
}).await;
}

#[tokio::test]
async fn whole_family_holding_signout_preserves_an_independent_device_and_other_saved_account() {
    database::with_database(2, |pool| async move {
        let scenario = setup(&pool, Arc::new(UnavailableInvitationCredentialAuthority)).await;
        let service = sso(&pool, &scenario);
        let (_, first) = login(&scenario, "b@example.test").await;
        let a = scenario
            .auth
            .sign_in("a@example.test", fixture::PASSWORD.into(), &first, Some(1))
            .await
            .unwrap();
        let parent = a.result.session.clone();
        let mut account = first;
        account.saved.push((a.handle, a.session_secret.clone()));
        account.active = Some(a.session_secret);
        let (_, independent) = login(&scenario, "a@example.test").await;
        let acme = consume(&service, &flow(&service, &account, ACME).await, ACME).await;
        let northstar = consume(
            &service,
            &flow(&service, &account, NORTHSTAR).await,
            NORTHSTAR,
        )
        .await;
        let other = consume(&service, &flow(&service, &independent, ACME).await, ACME).await;
        let pending = flow(&service, &account, ACME).await;
        let acme = proof(acme.token);
        let northstar = proof(northstar.token);
        scenario.auth.sign_out(&acme, holding(ACME)).await.unwrap();
        assert!(
            scenario
                .auth
                .session(&acme, holding(ACME))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            scenario
                .auth
                .session(&northstar, holding(NORTHSTAR))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            scenario
                .auth
                .session(&account, RequestAudience::Account)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            scenario
                .auth
                .session(&proof(other.token), holding(ACME))
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            scenario
                .auth
                .session(&independent, RequestAudience::Account)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            service
                .consume(
                    ACME,
                    &pending.1.code,
                    &pending.0.state,
                    pending.0.browser_cookie
                )
                .await
                .is_err()
        );
        let saved = scenario.auth.devices(&account).await.unwrap();
        assert_eq!(saved.accounts.len(), 1);
        assert_eq!(saved.accounts[0].email, "b@example.test");
        assert!(!saved.accounts[0].current);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM session WHERE id=$1")
                .bind(parent.id)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
    })
    .await;
}

#[tokio::test]
async fn password_reset_and_callback_races_cannot_retain_a_revoked_parent_child() {
    database::with_database(1,|pool|async move{
 let scenario=setup(&pool,Arc::new(UnavailableInvitationCredentialAuthority)).await;let service=sso(&pool,&scenario);let(parent,account)=login(&scenario,"a@example.test").await;let f=flow(&service,&account,ACME).await;let(consumed,signed_out)=tokio::join!(service.consume(ACME,&f.1.code,&f.0.state,f.0.browser_cookie.clone()),scenario.auth.sign_out(&account,RequestAudience::Account));signed_out.unwrap();if let Ok(issued)=consumed{assert!(scenario.auth.session(&proof(issued.token),holding(ACME)).await.unwrap().is_none());}assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM session WHERE id=$1 OR id IN(SELECT session_id FROM sso_session_bindings WHERE parent_session_id=$1)").bind(parent.id).fetch_one(pool.raw()).await.unwrap(),0);
 let(_,account)=login(&scenario,"a@example.test").await;let f=flow(&service,&account,ACME).await;scenario.credentials.request(CredentialProofKind::PasswordReset,"a@example.test",None).await.unwrap();let reset=fixture::mail(&pool,&scenario,CredentialProofKind::PasswordReset,"a@example.test").await;scenario.credentials.reset_password(reset.token.expose().into(),fixture::NEW_PASSWORD.into()).await.unwrap();assert!(service.consume(ACME,&f.1.code,&f.0.state,f.0.browser_cookie).await.is_err());
}).await;
}

async fn request(router: Router, host: &str, path: &str, cookie: Option<&str>) -> Response {
    let mut request = Request::builder().uri(path).header("host", host);
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    router
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}
type Response = axum::response::Response;
#[tokio::test]
async fn full_http_sso_preserves_login_continuation_and_never_forwards_an_account_token() {
    database::with_database(2, |pool| async move {
        let scenario = setup(&pool, Arc::new(UnavailableInvitationCredentialAuthority)).await;
        let service = sso(&pool, &scenario);
        let router = account_auth_router(scenario.auth.clone()).merge(account_sso_router(service));
        let (_, account) = login(&scenario, "a@example.test").await;
        let account_cookie = format!(
            "__Host-erpx-device={}; __Secure-erpx-account.session_token={}",
            account.browser.as_ref().unwrap().expose(),
            account.active.as_ref().unwrap().expose()
        );
        let start = request(
            router.clone(),
            "acme.example.test",
            "/sso/start?next=%2Ffinance%3Fcompany%3Dacme%26status%3Dpaid",
            None,
        )
        .await;
        assert_eq!(start.status(), StatusCode::SEE_OTHER);
        assert_eq!(start.headers()["cache-control"], "private, no-store");
        let browser = start.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        assert!(
            !start.headers()["set-cookie"]
                .to_str()
                .unwrap()
                .contains("Domain=")
        );
        let authorize = Url::parse(start.headers()["location"].to_str().unwrap()).unwrap();
        assert_eq!(authorize.host_str(), Some("account.example.test"));
        for (_, value) in authorize.query_pairs() {
            assert!(!value.contains(account.active.as_ref().unwrap().expose()));
        }
        let path = format!("{}?{}", authorize.path(), authorize.query().unwrap());
        let anonymous = request(router.clone(), "account.example.test", &path, None).await;
        let login = Url::parse(anonymous.headers()["location"].to_str().unwrap()).unwrap();
        assert_eq!(login.path(), "/login");
        assert_eq!(
            login
                .query_pairs()
                .find(|(key, _)| key == "state")
                .unwrap()
                .1
                .len(),
            43
        );
        let authorized = request(
            router.clone(),
            "account.example.test",
            &path,
            Some(&account_cookie),
        )
        .await;
        let callback = Url::parse(authorized.headers()["location"].to_str().unwrap()).unwrap();
        assert_eq!(callback.host_str(), Some("acme.example.test"));
        assert_eq!(callback.path(), "/sso/callback");
        assert!(!authorized.headers().contains_key("set-cookie"));
        let callback_path = format!("{}?{}", callback.path(), callback.query().unwrap());
        let completed = request(
            router.clone(),
            "acme.example.test",
            &callback_path,
            Some(&browser),
        )
        .await;
        assert_eq!(completed.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            completed.headers()["location"],
            "https://acme.example.test/finance?company=acme&status=paid"
        );
        assert_eq!(completed.headers()["referrer-policy"], "no-referrer");
        let cookies = completed
            .headers()
            .get_all("set-cookie")
            .iter()
            .map(|value| value.to_str().unwrap())
            .collect::<Vec<_>>();
        let child = cookies
            .iter()
            .find(|value| value.starts_with("__Host-erpx-product="))
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        assert!(
            cookies
                .iter()
                .all(|value| value.contains("Secure; HttpOnly; SameSite=Lax")
                    && !value.contains("Domain="))
        );
        assert!(
            cookies
                .iter()
                .any(|value| value.starts_with("__Host-erpx-sso=;") && value.contains("Max-Age=0"))
        );
        let session = request(
            router.clone(),
            "acme.example.test",
            "/api/auth/get-session",
            Some(&child),
        )
        .await;
        assert_eq!(session.status(), StatusCode::OK);
        let value: serde_json::Value =
            serde_json::from_slice(&to_bytes(session.into_body(), 8192).await.unwrap()).unwrap();
        assert_eq!(value["user"]["id"], "fixture-a");
        let replay = request(router, "acme.example.test", &callback_path, Some(&browser)).await;
        assert_eq!(replay.status(), StatusCode::SEE_OTHER);
        let recovery = replay.headers()["location"].to_str().unwrap();
        assert!(recovery.contains("reason=sso_expired"));
        assert!(!recovery.contains("code="));
        assert!(!recovery.contains("state="));
    })
    .await;
}

#[tokio::test]
async fn unregistered_or_incorrect_endpoint_hosts_and_tampered_callbacks_cannot_mint_a_holding_session()
 {
    database::with_database(2, |pool| async move {
        let scenario = setup(&pool, Arc::new(UnavailableInvitationCredentialAuthority)).await;
        let service = sso(&pool, &scenario);
        let router = account_sso_router(service.clone());
        for (host, path) in [
            ("acme.example.test.evil.test", "/sso/start"),
            ("account.example.test", "/sso/start"),
            ("acme.example.test", "/sso/authorize"),
            ("account.example.test", "/sso/callback"),
            ("unknown.example.test", "/sso/start"),
        ] {
            let response = request(router.clone(), host, path, None).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert!(!response.headers().contains_key("set-cookie"));
        }
        let (_, account) = login(&scenario, "a@example.test").await;
        let f = flow(&service, &account, ACME).await;
        sqlx::query(
            "UPDATE sso_authorization_grants SET callback='https://evil.test/sso/callback'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        assert!(
            service
                .consume(ACME, &f.1.code, &f.0.state, f.0.browser_cookie)
                .await
                .is_err()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM sso_session_bindings WHERE parent_session_id IS NOT NULL"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
        );
    })
    .await;
}

#[tokio::test]
async fn observing_a_legacy_unbound_parent_never_upgrades_it_into_an_issuer_session() {
    database::with_database(2, |pool| async move {
        let scenario = setup(&pool, Arc::new(UnavailableInvitationCredentialAuthority)).await;
        let service = sso(&pool, &scenario);
        let (parent, account) = login(&scenario, "a@example.test").await;
        sqlx::query("DELETE FROM sso_session_bindings WHERE session_id=$1")
            .bind(&parent.id)
            .execute(pool.raw())
            .await
            .unwrap();
        let started = service.start(ACME, Some("/finance")).await.unwrap();
        assert!(
            service
                .authorize(
                    started.transaction_id,
                    &started.state,
                    started.challenge,
                    &account
                )
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM sso_authorization_grants")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM sso_session_bindings WHERE session_id=$1"
            )
            .bind(parent.id)
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
        );
    })
    .await;
}

#[tokio::test]
async fn grant_expiry_during_a_real_session_lock_wait_rejects_the_callback_before_snapshot() {
    database::with_database(2, |pool| async move {
        let scenario = setup(&pool, Arc::new(UnavailableInvitationCredentialAuthority)).await;
        let service = sso(&pool, &scenario);
        let (_, account) = login(&scenario, "a@example.test").await;
        let f = flow(&service, &account, ACME).await;
        let mut blocker = pool.raw().begin().await.unwrap();
        sqlx::query("LOCK TABLE session IN SHARE MODE").execute(&mut *blocker).await.unwrap();
        let worker_service = service.clone();
        let task = tokio::spawn(async move { worker_service.consume(ACME, &f.1.code, &f.0.state, f.0.browser_cookie).await });
        let mut waiting = false;
        for _ in 0..200 {
            waiting = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE relation='session'::regclass AND NOT granted AND mode='ExclusiveLock' AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(&mut *blocker).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        if !waiting { task.abort(); blocker.rollback().await.unwrap(); panic!("callback did not reach the real session lock"); }
        scenario.clock.advance(60_000);
        blocker.commit().await.unwrap();
        fixture::account::assert_error(task.await.unwrap(), "INVALID_SSO_TRANSACTION");
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM sso_session_bindings WHERE parent_session_id IS NOT NULL").fetch_one(pool.raw()).await.unwrap(), 0);
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM sso_authorization_grants WHERE consumed_at IS NOT NULL").fetch_one(pool.raw()).await.unwrap(), 0);
    }).await;
}

#[tokio::test]
async fn grant_expiry_during_child_binding_rolls_back_before_the_control_commit() {
    database::with_database(2, |pool| async move {
        let scenario = setup(&pool, Arc::new(UnavailableInvitationCredentialAuthority)).await;
        let service = sso(&pool, &scenario);
        let (_, account) = login(&scenario, "a@example.test").await;
        let f = flow(&service, &account, ACME).await;
        sqlx::raw_sql("CREATE FUNCTION hold_child_binding() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.parent_session_id IS NOT NULL THEN PERFORM pg_advisory_xact_lock(7636001); END IF; RETURN NEW; END $$; CREATE TRIGGER hold_child_binding BEFORE INSERT ON sso_session_bindings FOR EACH ROW EXECUTE FUNCTION hold_child_binding();").execute(pool.raw()).await.unwrap();
        let mut blocker = pool.raw().acquire().await.unwrap();
        blocker.close_on_drop();
        sqlx::query("SELECT pg_advisory_lock(7636001)").execute(&mut *blocker).await.unwrap();
        let worker_service = service.clone();
        let task = tokio::spawn(async move { worker_service.consume(ACME, &f.1.code, &f.0.state, f.0.browser_cookie).await });
        let mut waiting = false;
        for _ in 0..200 {
            waiting = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND objid=7636001 AND NOT granted AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(&mut *blocker).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        if !waiting { task.abort(); panic!("callback did not reach the staged binding"); }
        scenario.clock.advance(60_000);
        assert!(sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock(7636001)").fetch_one(&mut *blocker).await.unwrap());
        fixture::account::assert_error(task.await.unwrap(), "INVALID_SSO_TRANSACTION");
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM session").fetch_one(pool.raw()).await.unwrap(), 1);
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM sso_browser_transactions WHERE consumed_at IS NOT NULL").fetch_one(pool.raw()).await.unwrap(), 0);
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM sso_authorization_grants WHERE consumed_at IS NOT NULL").fetch_one(pool.raw()).await.unwrap(), 0);
    }).await;
}
