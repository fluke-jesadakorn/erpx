#[path = "../../support/rust-postgres.rs"]
mod database;
#[path = "../../support/rust-account.rs"]
mod fixture;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_identity_access::public::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use sqlx::Row;
use std::sync::Arc;
use tower::ServiceExt;

async fn setup(pool: &DatabasePool) -> Arc<AccountAuthentication> {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::Control,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::Control, &files)
        .await
        .unwrap();
    let passwords = Arc::new(Argon2Passwords::new(2).unwrap());
    let hash = passwords
        .hash(Password::new_credential("synthetic-fixture-password".into()).unwrap())
        .await
        .unwrap();
    for (id, email) in [
        ("fixture-a", "a@example.test"),
        ("fixture-b", "b@example.test"),
    ] {
        sqlx::query("INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\") VALUES($1,$1,$2,1,$3,$3)").bind(id).bind(email).bind(fixture::NOW).execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO account(id,\"accountId\",\"providerId\",issuer,\"userId\",password,\"createdAt\",\"updatedAt\") VALUES($1,$1,'credential','local:credential',$1,$2,$3,$3)").bind(id).bind(hash.as_str()).bind(fixture::NOW).execute(pool.raw()).await.unwrap();
    }
    Arc::new(
        AccountAuthentication::new(
            fixture::policy(),
            Arc::new(
                PostgresAccountRepository::from_control_database(
                    pool.clone(),
                    "development".into(),
                    true,
                )
                .unwrap(),
            ),
            passwords,
            Arc::new(SystemAuthenticationSecrets),
            Arc::new(fixture::FixedClock),
        )
        .unwrap(),
    )
}
fn remember(previous: Option<PresentedSecrets>, signed: SignedIn) -> PresentedSecrets {
    let mut proof = previous.unwrap_or_else(fixture::empty);
    if let Some(browser) = signed.browser_secret {
        proof.browser = Some(browser);
    }
    proof.active = Some(signed.session_secret.clone());
    proof.saved.push((signed.handle, signed.session_secret));
    proof
}
async fn child(pool: &DatabasePool, parent: &AccountSession, suffix: &str) -> OpaqueSecret {
    let secrets = SystemAuthenticationSecrets;
    let raw = secrets.issue().unwrap();
    let digest = secrets.digest(SecretPurpose::Session, &raw);
    let id = format!("fixture-child-{suffix}");
    sqlx::query("INSERT INTO session(id,\"expiresAt\",token,\"createdAt\",\"updatedAt\",\"userId\") VALUES($1,'2027-01-15T08:00:10Z',$2,'2027-01-15T08:00:00Z','2027-01-15T08:00:00Z',$3)").bind(&id).bind(digest.as_str()).bind(&parent.user.id).execute(pool.raw()).await.unwrap();
    // NOW is 2027-01-15T08:00:00Z. Both clocks and absolute windows are synthetic.
    sqlx::query("INSERT INTO sso_session_bindings(session_id,audience,environment,parent_session_id,authenticated_at,absolute_expires_at) VALUES($1,'holding:development:h001:acme.example.test','development',$2,$3,$4)").bind(id).bind(&parent.id).bind(fixture::NOW).bind(fixture::NOW+10_000).execute(pool.raw()).await.unwrap();
    raw
}
fn holding_proof(secret: OpaqueSecret) -> PresentedSecrets {
    PresentedSecrets {
        browser: None,
        active: Some(secret),
        saved: vec![],
    }
}
fn holding() -> RequestAudience {
    RequestAudience::Holding {
        audience: "holding:development:h001:acme.example.test".into(),
    }
}

#[tokio::test]
async fn fresh_sign_in_hashes_proofs_and_switching_invalidates_children_without_exposing_saved_tokens()
 {
    database::with_database(2, |pool| async move {
        let service = setup(&pool).await;
        let first = service
            .sign_in(
                "a@example.test",
                "synthetic-fixture-password".into(),
                &fixture::empty(),
                None,
            )
            .await
            .unwrap();
        let parent = first.result.session.clone();
        assert_eq!(first.result.devices.revision, 1);
        let first_handle = first.handle.clone();
        let proof = remember(None, first);
        let stored: String = sqlx::query_scalar("SELECT token FROM session WHERE id=$1")
            .bind(&parent.id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_ne!(stored, proof.active.as_ref().unwrap().expose());
        assert_eq!(
            stored,
            SystemAuthenticationSecrets
                .digest(SecretPurpose::Session, proof.active.as_ref().unwrap())
                .as_str()
        );
        let raw_child = child(&pool, &parent, "one").await;
        let child_proof = holding_proof(raw_child.clone());
        assert!(
            service
                .session(&child_proof, holding())
                .await
                .unwrap()
                .is_some()
        );
        let second = service
            .sign_in(
                "b@example.test",
                "synthetic-fixture-password".into(),
                &proof,
                Some(1),
            )
            .await
            .unwrap();
        let second_handle = second.handle.clone();
        let mut proof = remember(Some(proof), second);
        assert!(
            service
                .session(&child_proof, holding())
                .await
                .unwrap()
                .is_none()
        );
        let devices = service.devices(&proof).await.unwrap();
        assert_eq!(devices.revision, 2);
        assert_eq!(devices.accounts.len(), 2);
        let json = serde_json::to_string(
            &erpx_server_identity_access::public::account_contracts::DeviceAccountsDto::try_from(
                devices,
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!json.contains("token"));
        for (_, secret) in &proof.saved {
            assert!(!json.contains(secret.expose()));
        }
        fixture::assert_error(
            service.select(&proof, first_handle.clone(), 1).await,
            "DEVICE_ACCOUNTS_CHANGED",
        );
        assert_eq!(
            service
                .session(&proof, RequestAudience::Account)
                .await
                .unwrap()
                .unwrap()
                .user
                .id,
            "fixture-b"
        );
        let selected = service
            .select(&proof, first_handle.clone(), 2)
            .await
            .unwrap();
        proof.active = Some(selected.session_secret);
        assert_eq!(selected.devices.revision, 3);
        assert_eq!(
            service
                .session(&proof, RequestAudience::Account)
                .await
                .unwrap()
                .unwrap()
                .user
                .id,
            "fixture-a"
        );
        assert!(
            service
                .devices(&PresentedSecrets {
                    browser: proof.browser.clone(),
                    active: None,
                    saved: vec![]
                })
                .await
                .unwrap()
                .accounts
                .is_empty()
        );
        let removed = service.remove(&proof, second_handle, 3).await.unwrap();
        assert_eq!(removed.revision, 4);
        assert_eq!(removed.accounts.len(), 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM account_device_accounts")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            1
        );
        assert!(
            service
                .session(&child_proof, holding())
                .await
                .unwrap()
                .is_none()
        );
    })
    .await;
}
#[tokio::test]
async fn stale_parallel_device_commands_have_one_winner_on_a_single_connection_pool() {
    database::with_database(1, |pool| async move {
        let service = setup(&pool).await;
        let first = service
            .sign_in(
                "a@example.test",
                "synthetic-fixture-password".into(),
                &fixture::empty(),
                None,
            )
            .await
            .unwrap();
        let handle = first.handle.clone();
        let proof = remember(None, first);
        let (left, right) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            tokio::join!(
                service.select(&proof, handle.clone(), 1),
                service.select(&proof, handle.clone(), 1)
            )
        })
        .await
        .unwrap();
        assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
        let error = left.err().or_else(|| right.err()).unwrap();
        assert_eq!(error.code, "DEVICE_ACCOUNTS_CHANGED");
        assert_eq!(service.devices(&proof).await.unwrap().revision, 2);
    })
    .await;
}
#[tokio::test]
async fn host_bound_children_require_current_parent_selection_and_active_principal() {
    database::with_database(2,|pool|async move{
  let service=setup(&pool).await;let signed=service.sign_in("a@example.test","synthetic-fixture-password".into(),&fixture::empty(),None).await.unwrap();let parent=signed.result.session.clone();let proof=remember(None,signed);let child=holding_proof(child(&pool,&parent,"guard").await);
  assert!(service.session(&child,holding()).await.unwrap().is_some());assert!(service.session(&child,RequestAudience::Holding{audience:"holding:development:h002:foreign.example.test".into()}).await.unwrap().is_none());assert!(service.session(&child,RequestAudience::Account).await.unwrap().is_none());
  sqlx::query("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES('p-a','fixture-a','Synthetic A','suspended',$1)").bind(fixture::NOW).execute(pool.raw()).await.unwrap();assert!(service.session(&proof,RequestAudience::Account).await.unwrap().is_none());assert!(service.session(&child,holding()).await.unwrap().is_none());
  sqlx::query("UPDATE principals SET status='active' WHERE id='p-a'").execute(pool.raw()).await.unwrap();sqlx::query("UPDATE sso_session_bindings SET authenticated_at=$1 WHERE session_id=$2").bind(fixture::NOW+1).bind(&parent.id).execute(pool.raw()).await.unwrap();assert!(service.session(&proof,RequestAudience::Account).await.unwrap().is_none());
 }).await;
}
#[tokio::test]
async fn holding_signout_revokes_current_family_and_preserves_other_saved_accounts() {
    database::with_database(2, |pool| async move {
        let service = setup(&pool).await;
        let a = service
            .sign_in(
                "a@example.test",
                "synthetic-fixture-password".into(),
                &fixture::empty(),
                None,
            )
            .await
            .unwrap();
        let handle = a.handle.clone();
        let proof = remember(None, a);
        let b = service
            .sign_in(
                "b@example.test",
                "synthetic-fixture-password".into(),
                &proof,
                Some(1),
            )
            .await
            .unwrap();
        let parent = b.result.session.clone();
        let mut proof = remember(Some(proof), b);
        let child = holding_proof(child(&pool, &parent, "signout").await);
        service.sign_out(&child, holding()).await.unwrap();
        assert!(service.session(&child, holding()).await.unwrap().is_none());
        assert!(
            service
                .session(&proof, RequestAudience::Account)
                .await
                .unwrap()
                .is_none()
        );
        service
            .sign_out(&proof, RequestAudience::Account)
            .await
            .unwrap();
        assert!(
            service
                .session(&proof, RequestAudience::Account)
                .await
                .unwrap()
                .is_none()
        );
        let devices = service.devices(&proof).await.unwrap();
        assert_eq!(devices.accounts.len(), 1);
        assert_eq!(devices.accounts[0].handle, handle);
        let selected = service
            .select(&proof, handle, devices.revision)
            .await
            .unwrap();
        proof.active = Some(selected.session_secret);
        assert_eq!(
            service
                .session(&proof, RequestAudience::Account)
                .await
                .unwrap()
                .unwrap()
                .user
                .id,
            "fixture-a"
        );
    })
    .await;
}
#[tokio::test]
async fn wrong_password_and_unknown_user_leave_no_session_or_pending_reservation() {
    database::with_database(2, |pool| async move {
        let service = setup(&pool).await;
        for email in ["a@example.test", "missing@example.test"] {
            fixture::assert_error(
                service
                    .sign_in(
                        email,
                        "wrong-synthetic-password".into(),
                        &fixture::empty(),
                        None,
                    )
                    .await,
                "INVALID_EMAIL_OR_PASSWORD",
            );
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM session")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM account_device_browsers WHERE pending_nonce IS NOT NULL"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
        );
        let row = sqlx::query("SELECT count(*) count FROM \"rateLimit\" WHERE count=1")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(row.get::<i64, _>("count"), 2);
    })
    .await;
}

#[tokio::test]
async fn successful_http_sign_in_uses_only_secure_host_cookies_and_closed_public_json() {
    database::with_database(2,|pool|async move{
  let service=setup(&pool).await;let router=account_auth_router(service);
  let response=router.clone().oneshot(Request::builder().method("POST").uri("/api/auth/sign-in/email").header("host","account.example.test").header("origin","https://account.example.test").header("content-type","application/json").body(Body::from(r#"{"email":"a@example.test","password":"synthetic-fixture-password","rememberMe":false}"#)).unwrap()).await.unwrap();
  assert_eq!(response.status(),StatusCode::OK);let cookies=response.headers().get_all("set-cookie").iter().map(|value|value.to_str().unwrap().to_owned()).collect::<Vec<_>>();assert_eq!(cookies.len(),3);
  for cookie in &cookies{assert!(cookie.contains("; Secure; HttpOnly; SameSite=Lax"));assert!(!cookie.contains("Domain="));if cookie.starts_with("__Secure-erpx-account.session_token="){assert!(!cookie.contains("Max-Age="));}}
  let jar=cookies.iter().map(|cookie|cookie.split(';').next().unwrap()).collect::<Vec<_>>().join("; ");let body=to_bytes(response.into_body(),8192).await.unwrap();let json:serde_json::Value=serde_json::from_slice(&body).unwrap();assert_eq!(json["user"]["id"],"fixture-a");assert_eq!(json.as_object().unwrap().len(),1);assert!(!String::from_utf8(body.to_vec()).unwrap().contains("token"));
  for path in["/api/auth/get-session","/api/v1/account/device-accounts"]{let response=router.clone().oneshot(Request::builder().uri(path).header("host","account.example.test").header("cookie",&jar).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let body=to_bytes(response.into_body(),8192).await.unwrap();let value:serde_json::Value=serde_json::from_slice(&body).unwrap();assert!(!value.is_null());for cookie in &cookies{let secret=cookie.split(';').next().unwrap().split_once('=').unwrap().1;assert!(!String::from_utf8_lossy(&body).contains(secret));}}
  let duplicate=format!("{jar}; {}",cookies.iter().find(|cookie|cookie.starts_with("__Host-erpx-device=")).unwrap().split(';').next().unwrap());let response=router.oneshot(Request::builder().uri("/api/auth/get-session").header("host","account.example.test").header("cookie",duplicate).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST);
 }).await;
}

#[tokio::test]
async fn failed_selection_rolls_back_family_deletion_and_the_browser_revision() {
    database::with_database(2, |pool| async move {
        let service = setup(&pool).await;
        let signed = service.sign_in("a@example.test", "synthetic-fixture-password".into(), &fixture::empty(), None).await.unwrap();
        let parent = signed.result.session.clone();
        let handle = signed.handle.clone();
        let proof = remember(None, signed);
        let child = holding_proof(child(&pool, &parent, "rollback").await);
        sqlx::raw_sql("CREATE FUNCTION reject_device_selection() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic device update failure'; END $$; CREATE TRIGGER reject_device_selection BEFORE UPDATE ON account_device_browsers FOR EACH ROW EXECUTE FUNCTION reject_device_selection();").execute(pool.raw()).await.unwrap();
        fixture::assert_error(service.select(&proof, handle, 1).await, "ACCOUNT_PERSISTENCE_UNAVAILABLE");
        assert_eq!(service.devices(&proof).await.unwrap().revision, 1);
        assert!(service.session(&child, holding()).await.unwrap().is_some());
        assert_eq!(service.session(&proof, RequestAudience::Account).await.unwrap().unwrap().id, parent.id);
    }).await;
}

#[tokio::test]
async fn removing_an_inactive_account_preserves_the_active_holding_child_and_revokes_only_the_removed_family()
 {
    database::with_database(2, |pool| async move {
        let service = setup(&pool).await;
        let a = service
            .sign_in(
                "a@example.test",
                "synthetic-fixture-password".into(),
                &fixture::empty(),
                None,
            )
            .await
            .unwrap();
        let parent_a = a.result.session.clone();
        let handle_a = a.handle.clone();
        let proof = remember(None, a);
        let b = service
            .sign_in(
                "b@example.test",
                "synthetic-fixture-password".into(),
                &proof,
                Some(1),
            )
            .await
            .unwrap();
        let parent_b = b.result.session.clone();
        let handle_b = b.handle.clone();
        let mut proof = remember(Some(proof), b);
        let selected = service.select(&proof, handle_a.clone(), 2).await.unwrap();
        proof.active = Some(selected.session_secret);
        assert_eq!(selected.devices.revision, 3);
        // Issue the current Holding child after switching back: the switch must not explain its deletion.
        let active_child = holding_proof(child(&pool, &parent_a, "active-survivor").await);
        // An inactive parent's child cannot authenticate, but any retained family row still needs cleanup.
        let removed_child = holding_proof(child(&pool, &parent_b, "inactive-removed").await);
        let active_session = service
            .session(&active_child, holding())
            .await
            .unwrap()
            .unwrap();
        assert!(
            service
                .session(&removed_child, holding())
                .await
                .unwrap()
                .is_none()
        );

        fixture::assert_error(
            service.remove(&proof, handle_b.clone(), 2).await,
            "DEVICE_ACCOUNTS_CHANGED",
        );
        let without_removed_proof = PresentedSecrets {
            browser: proof.browser.clone(),
            active: proof.active.clone(),
            saved: proof
                .saved
                .iter()
                .filter(|(handle, _)| handle != &handle_b)
                .cloned()
                .collect(),
        };
        fixture::assert_error(
            service
                .remove(&without_removed_proof, handle_b.clone(), 3)
                .await,
            "INVALID_EMAIL_OR_PASSWORD",
        );
        let wrong_browser = PresentedSecrets {
            browser: Some(SystemAuthenticationSecrets.issue().unwrap()),
            active: proof.active.clone(),
            saved: proof.saved.clone(),
        };
        fixture::assert_error(
            service.remove(&wrong_browser, handle_b.clone(), 3).await,
            "INVALID_EMAIL_OR_PASSWORD",
        );
        let repository = PostgresAccountRepository::from_control_database(
            pool.clone(),
            "development".into(),
            true,
        )
        .unwrap();
        let nonce = SystemAuthenticationSecrets.issue().unwrap();
        let candidate = SystemAuthenticationSecrets.issue().unwrap();
        let reservation = repository
            .reserve_sign_in(ReserveSignIn {
                proof: proof.proof(&SystemAuthenticationSecrets),
                candidate_browser: SystemAuthenticationSecrets
                    .digest(SecretPurpose::DeviceBrowser, &candidate),
                nonce: SystemAuthenticationSecrets.digest(SecretPurpose::Reservation, &nonce),
                email: EmailAddress::new("a@example.test").unwrap(),
                expected_revision: Some(3),
                now: fixture::NOW,
            })
            .await
            .unwrap();
        fixture::assert_error(
            service.remove(&proof, handle_b.clone(), 3).await,
            "ACCOUNT_SIGN_IN_PENDING",
        );
        repository.release_sign_in(reservation).await.unwrap();
        assert_eq!(service.devices(&proof).await.unwrap().revision, 3);
        assert_eq!(
            service.session(&active_child, holding()).await.unwrap(),
            Some(active_session.clone())
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM session WHERE id=$1 OR id='fixture-child-inactive-removed'"
            )
            .bind(&parent_b.id)
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            2
        );

        let removed = service.remove(&proof, handle_b.clone(), 3).await.unwrap();
        assert_eq!(removed.revision, 4);
        assert_eq!(removed.accounts.len(), 1);
        assert_eq!(removed.accounts[0].handle, handle_a);
        assert!(removed.accounts[0].current);
        assert_eq!(
            service.session(&active_child, holding()).await.unwrap(),
            Some(active_session)
        );
        assert_eq!(
            service
                .session(&proof, RequestAudience::Account)
                .await
                .unwrap()
                .unwrap()
                .id,
            parent_a.id
        );
        assert!(
            service
                .session(&removed_child, holding())
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM session WHERE id=$1 OR id='fixture-child-inactive-removed'"
            )
            .bind(&parent_b.id)
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM account_device_accounts WHERE session_id=$1"
            )
            .bind(&parent_b.id)
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
        );
        fixture::assert_error(
            service.select(&proof, handle_b, 4).await,
            "INVALID_EMAIL_OR_PASSWORD",
        );
        assert_eq!(service.devices(&proof).await.unwrap().revision, 4);
    })
    .await;
}
