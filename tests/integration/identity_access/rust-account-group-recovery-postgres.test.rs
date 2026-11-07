#[allow(
    dead_code,
    reason = "shared actual Account HTTP fixtures support multiple native suites"
)]
#[path = "../../support/rust-account-device-http.rs"]
mod http;
use axum::http::{StatusCode, header};
use erpx_server_identity_access::public::account_revocation::*;
use erpx_server_identity_access::public::{account_auth_router, *};
use http::revocation as fixture;
use http::*;
use serde_json::{Value, json};
use std::sync::Arc;

fn from_proof(proof: &PresentedSecrets) -> Jar {
    let mut jar = Jar::default();
    if let Some(secret) = &proof.browser {
        jar.0.insert(GROUP.into(), secret.expose().into());
    }
    if let Some(secret) = &proof.active {
        jar.0.insert(ACTIVE.into(), secret.expose().into());
    }
    for (handle, secret) in &proof.saved {
        jar.0
            .insert(format!("{SAVED}{handle}"), secret.expose().into());
    }
    jar
}
async fn parent_state(system: &fixture::System) -> Value {
    sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(s)||jsonb_build_object('binding',to_jsonb(b)) ORDER BY s.id),'[]'::jsonb) FROM session s JOIN sso_session_bindings b ON b.session_id=s.id WHERE b.audience='account'").fetch_one(system.control.raw()).await.unwrap()
}
async fn key_count(system: &fixture::System) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM native_account_device_group_keys")
        .fetch_one(system.control.raw())
        .await
        .unwrap()
}
async fn initialize(
    router: &axum::Router,
    jar: &mut Jar,
    revision: i64,
) -> (StatusCode, Value, axum::http::HeaderMap) {
    send(
        router,
        jar,
        "POST",
        &format!("{DEVICE}/initialize"),
        Some(ACME),
        Some(json!({"revision":revision})),
    )
    .await
}

#[tokio::test]
async fn lost_group_get_is_read_only_then_fresh_rotation_fences_both_holdings_without_extending_parents()
 {
    fixture::with_system(|system| async move {
        let proof = fixture::login(&system, "a@example.test", None).await;
        let retired = fixture::children(&system, &proof, "fixture-a").await;
        let before = parent_state(&system).await;
        let router = account_auth_router(system.scenario.auth.clone());
        let original = from_proof(&proof);
        let mut jar = original.clone();
        jar.0.remove(GROUP);
        let (status, body, headers) =
            send(&router, &mut jar, "GET", DEVICE, Some(ACME), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["initialized"], false);
        assert_eq!(body["data"]["revision"], 1);
        assert_eq!(body["data"]["accounts"].as_array().unwrap().len(), 1);
        assert!(!headers.contains_key(header::SET_COOKIE));
        assert_eq!(key_count(&system).await, 1);
        for (pool, child) in system.holdings.iter().zip(&retired) {
            assert!(!fixture::fenced(pool, child, "fixture-a").await);
        }
        let (status, body, headers) = initialize(&router, &mut jar, 1).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["initialized"], true);
        assert_eq!(body["data"]["revision"], 2);
        assert_ne!(jar.0[GROUP], original.0[GROUP]);
        assert_eq!(headers.get_all(header::SET_COOKIE).iter().count(), 1);
        no_credentials(&body, &jar);
        fixture::assert_fenced(&system, &retired, "fixture-a").await;
        assert_eq!(parent_state(&system).await, before);
        assert_eq!(key_count(&system).await, 2);
        assert!(
            system
                .scenario
                .auth
                .session(&jar.proof(), RequestAudience::Account)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            system
                .scenario
                .auth
                .session(&original.proof(), RequestAudience::Account)
                .await
                .unwrap()
                .is_none()
        );
        let id: String = sqlx::query_scalar("SELECT id FROM account_device_browsers")
            .fetch_one(system.control.raw())
            .await
            .unwrap();
        assert_ne!(jar.0[GROUP], id);
        let mut old_group_only = Jar::default();
        old_group_only
            .0
            .insert(GROUP.into(), original.0[GROUP].clone());
        let (status, body, headers) =
            send(&router, &mut old_group_only, "GET", DEVICE, None, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body["data"],
            json!({"initialized":false,"revision":0,"maximum":5,"accounts":[]})
        );
        assert!(!headers.contains_key(header::SET_COOKIE));
        let (status, _, headers) = initialize(&router, &mut old_group_only, 2).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(!headers.contains_key(header::SET_COOKIE));
        let mut stored_hash = Jar::default();
        stored_hash.0.insert(GROUP.into(), id);
        let (status, body, _) = send(&router, &mut stored_hash, "GET", DEVICE, None, None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["data"]["accounts"].as_array().unwrap().is_empty());
        assert_eq!(
            initialize(&router, &mut stored_hash, 2).await.0,
            StatusCode::CONFLICT
        );
    })
    .await;
}

#[tokio::test]
async fn active_proof_has_group_priority_without_merging_independent_saved_accounts() {
    fixture::with_system(|system| async move {
        let a = fixture::login(&system, "a@example.test", None).await;
        let b = fixture::login(&system, "b@example.test", None).await;
        let a_children = fixture::children(&system, &a, "fixture-a").await;
        let b_children = fixture::children(&system, &b, "fixture-b").await;
        let router = account_auth_router(system.scenario.auth.clone());
        let mut jar = from_proof(&a);
        jar.0.remove(GROUP);
        for (handle, token) in &b.saved {
            jar.0
                .insert(format!("{SAVED}{handle}"), token.expose().into());
        }
        let (status, body, _) = send(&router, &mut jar, "GET", DEVICE, None, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["accounts"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["accounts"][0]["email"], "a@example.test");
        assert_eq!(initialize(&router, &mut jar, 1).await.0, StatusCode::OK);
        fixture::assert_fenced(&system, &a_children, "fixture-a").await;
        for (pool, child) in system.holdings.iter().zip(&b_children) {
            assert!(!fixture::fenced(pool, child, "fixture-b").await);
        }
        assert!(
            system
                .scenario
                .auth
                .session(&b, RequestAudience::Account)
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM account_device_browsers")
                .fetch_one(system.control.raw())
                .await
                .unwrap(),
            2
        );
    })
    .await;
}

#[tokio::test]
async fn multiple_saved_groups_without_current_active_proof_deny_discovery_and_mutation() {
    fixture::with_system(|system| async move {
        let a = fixture::login(&system, "a@example.test", None).await;
        let b = fixture::login(&system, "b@example.test", None).await;
        let before = parent_state(&system).await;
        let router = account_auth_router(system.scenario.auth.clone());
        let mut jar = from_proof(&a);
        jar.0.remove(GROUP);
        jar.0.remove(ACTIVE);
        for (handle, token) in &b.saved {
            jar.0
                .insert(format!("{SAVED}{handle}"), token.expose().into());
        }
        let (status, _, headers) = send(&router, &mut jar, "GET", DEVICE, None, None).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(!headers.contains_key(header::SET_COOKIE));
        let (status, _, headers) = initialize(&router, &mut jar, 1).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(!headers.contains_key(header::SET_COOKIE));
        assert_eq!(key_count(&system).await, 2);
        assert_eq!(parent_state(&system).await, before);
    })
    .await;
}

#[tokio::test]
async fn recovery_requires_original_revision_and_rejects_pending_nonce_and_expired_proofs() {
    fixture::with_system(|system| async move {
        let a = fixture::login(&system, "a@example.test", None).await;
        let router = account_auth_router(system.scenario.auth.clone());
        let mut jar = from_proof(&a);
        jar.0.remove(GROUP);
        for body in [
            json!({}),
            json!({"revision":0}),
            json!({"revision":-1}),
            json!({"revision":1,"authority":{}}),
        ] {
            let (status, _, headers) = send(
                &router,
                &mut jar,
                "POST",
                &format!("{DEVICE}/initialize"),
                Some(ACME),
                Some(body),
            )
            .await;
            assert!(status.is_client_error());
            assert!(!headers.contains_key(header::SET_COOKIE));
        }
        sqlx::query(
            "UPDATE account_device_browsers SET pending_nonce='synthetic-other',pending_until=$1",
        )
        .bind(fixture::fixture::account::NOW + 120000)
        .execute(system.control.raw())
        .await
        .unwrap();
        let (status, body, headers) = initialize(&router, &mut jar, 1).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "ACCOUNT_SIGN_IN_PENDING");
        assert!(!headers.contains_key(header::SET_COOKIE));
        assert_eq!(key_count(&system).await, 1);
        system.scenario.clock.advance(SESSION_LIFETIME_MS + 1);
        let (status, _, headers) = initialize(&router, &mut jar, 1).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(!headers.contains_key(header::SET_COOKIE));
        assert_eq!(key_count(&system).await, 1);
    })
    .await;
}

#[tokio::test]
async fn concurrent_recovery_requests_allow_one_original_revision_and_no_old_secret_reissue() {
    fixture::with_system(|system| async move {
        let a = fixture::login(&system, "a@example.test", None).await;
        let retired = fixture::children(&system, &a, "fixture-a").await;
        let router = account_auth_router(system.scenario.auth.clone());
        let mut first = from_proof(&a);
        first.0.remove(GROUP);
        let mut second = first.clone();
        let (left, right) = tokio::join!(
            initialize(&router, &mut first, 1),
            initialize(&router, &mut second, 1)
        );
        let statuses = [left.0, right.0];
        assert_eq!(statuses.iter().filter(|s| **s == StatusCode::OK).count(), 1);
        assert_eq!(
            statuses
                .iter()
                .filter(|s| **s == StatusCode::CONFLICT)
                .count(),
            1
        );
        assert_eq!(key_count(&system).await, 2);
        fixture::assert_fenced(&system, &retired, "fixture-a").await;
        let loser = if left.0 == StatusCode::CONFLICT {
            left
        } else {
            right
        };
        assert!(!loser.2.contains_key(header::SET_COOKIE));
        assert!(loser.1.get("data").is_none());
    })
    .await;
}

#[tokio::test]
async fn lost_set_cookie_ack_requires_new_discovery_then_rotates_again_instead_of_replaying_secret()
{
    fixture::with_system(|system| async move {
        let a = fixture::login(&system, "a@example.test", None).await;
        let router = account_auth_router(system.scenario.auth.clone());
        let mut lost = from_proof(&a);
        lost.0.remove(GROUP);
        let mut delivered = lost.clone();
        assert_eq!(
            initialize(&router, &mut delivered, 1).await.0,
            StatusCode::OK
        );
        let first = delivered.0[GROUP].clone();
        let (status, _, headers) = initialize(&router, &mut lost, 1).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(!headers.contains_key(header::SET_COOKIE));
        assert_eq!(key_count(&system).await, 2);
        let (status, body, _) = send(&router, &mut lost, "GET", DEVICE, None, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["revision"], 2);
        assert_eq!(body["data"]["initialized"], false);
        assert_eq!(initialize(&router, &mut lost, 2).await.0, StatusCode::OK);
        assert_ne!(lost.0[GROUP], first);
        assert_eq!(key_count(&system).await, 3);
        assert!(
            system
                .scenario
                .auth
                .session(&delivered.proof(), RequestAudience::Account)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            system
                .scenario
                .auth
                .session(&lost.proof(), RequestAudience::Account)
                .await
                .unwrap()
                .is_some()
        );
    })
    .await;
}

#[tokio::test]
async fn second_holding_failure_never_acknowledges_or_rotates_and_retry_finishes_monotonic_fences()
{
    fixture::with_system(|system| async move {
        let a = fixture::login(&system, "a@example.test", None).await;
        let retired = fixture::children(&system, &a, "fixture-a").await;
        let router = account_auth_router(system.scenario.auth.clone());
        let mut jar = from_proof(&a);
        jar.0.remove(GROUP);
        let before = parent_state(&system).await;
        sqlx::query("ALTER TABLE active_work_contexts RENAME TO fixture_unavailable_contexts")
            .execute(system.holdings[1].raw())
            .await
            .unwrap();
        let (status, body, headers) = initialize(&router, &mut jar, 1).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(body.get("data").is_none());
        assert!(!headers.contains_key(header::SET_COOKIE));
        assert_eq!(key_count(&system).await, 1);
        assert!(fixture::fenced(&system.holdings[0], &retired[0], "fixture-a").await);
        assert_eq!(parent_state(&system).await, before);
        assert_eq!(system.scenario.auth.devices(&a).await.unwrap().revision, 1);
        sqlx::query("ALTER TABLE fixture_unavailable_contexts RENAME TO active_work_contexts")
            .execute(system.holdings[1].raw())
            .await
            .unwrap();
        assert_eq!(initialize(&router, &mut jar, 1).await.0, StatusCode::OK);
        fixture::assert_fenced(&system, &retired, "fixture-a").await;
    })
    .await;
}

#[tokio::test]
async fn deferred_control_failure_does_not_retry_the_callback_or_ack_a_rotated_key() {
    fixture::with_system(|system|async move{
        let a=fixture::login(&system,"a@example.test",None).await;let retired=fixture::children(&system,&a,"fixture-a").await;let router=account_auth_router(system.scenario.auth.clone());let mut jar=from_proof(&a);jar.0.remove(GROUP);
        sqlx::raw_sql("CREATE SEQUENCE fixture_recovery_attempts; CREATE FUNCTION fixture_recovery_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('fixture_recovery_attempts'); RAISE EXCEPTION 'Synthetic late Control serialization' USING ERRCODE='40001'; END $$; CREATE CONSTRAINT TRIGGER fixture_recovery_failure AFTER INSERT ON native_account_device_group_keys DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION fixture_recovery_failure()").execute(system.control.raw()).await.unwrap();
        let(status,body,headers)=initialize(&router,&mut jar,1).await;assert_eq!(status,StatusCode::SERVICE_UNAVAILABLE);assert!(body.get("data").is_none());assert!(!headers.contains_key(header::SET_COOKIE));assert_eq!(key_count(&system).await,1);assert_eq!(system.scenario.auth.devices(&a).await.unwrap().revision,1);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT last_value FROM fixture_recovery_attempts").fetch_one(system.control.raw()).await.unwrap(),1);fixture::assert_fenced(&system,&retired,"fixture-a").await;
    }).await;
}

#[tokio::test]
async fn failed_password_does_not_rotate_or_retire_work_and_successful_signin_recovers_with_original_nonce_cas()
 {
    fixture::with_system(|system|async move{
        let a=fixture::login(&system,"a@example.test",None).await;let retired=fixture::children(&system,&a,"fixture-a").await;let router=account_auth_router(system.scenario.auth.clone());let mut jar=from_proof(&a);jar.0.remove(GROUP);
        let(status,_,headers)=send(&router,&mut jar,"POST",&format!("{DEVICE}/sign-in"),Some(ACME),Some(json!({"email":"b@example.test","password":"synthetic-wrong-password","revision":1}))).await;assert_eq!(status,StatusCode::UNAUTHORIZED);assert!(!headers.contains_key(header::SET_COOKIE));assert_eq!(key_count(&system).await,1);assert_eq!(system.scenario.auth.devices(&a).await.unwrap().revision,1);
        for(pool,child)in system.holdings.iter().zip(&retired){assert!(!fixture::fenced(pool,child,"fixture-a").await);}
        let state=sign_in(&router,&mut jar,"b@example.test",Some(1),ACME).await;assert_eq!(state["revision"],2);assert_eq!(key_count(&system).await,2);fixture::assert_fenced(&system,&retired,"fixture-a").await;
    }).await;
}

#[tokio::test]
async fn successful_verification_uses_the_same_recovery_group_and_fences_before_new_cookie_ack() {
    fixture::with_system(|system| async move {
        let a = fixture::login(&system, "a@example.test", None).await;
        let retired = fixture::children(&system, &a, "fixture-a").await;
        system
            .scenario
            .credentials
            .request(
                CredentialProofKind::EmailVerification,
                "unverified@example.test",
                None,
            )
            .await
            .unwrap();
        let mail = fixture::fixture::mail(
            &system.control,
            &system.scenario,
            CredentialProofKind::EmailVerification,
            "unverified@example.test",
        )
        .await;
        let mut proof = a;
        proof.browser = None;
        let verified = system
            .scenario
            .credentials
            .verify_email(mail.token.expose().into(), &proof, Some(1))
            .await
            .unwrap();
        assert!(verified.browser_secret.is_some());
        assert_eq!(verified.result.devices.revision, 2);
        assert_eq!(verified.result.session.user.id, "fixture-unverified");
        assert_eq!(key_count(&system).await, 2);
        fixture::assert_fenced(&system, &retired, "fixture-a").await;
    })
    .await;
}

struct AdvanceAfterInvalidation {
    inner: Arc<dyn AccountWorkContextInvalidation>,
    clock: Arc<fixture::fixture::MutableClock>,
}
impl AccountWorkContextInvalidation for AdvanceAfterInvalidation {
    fn invalidate<'a>(
        &'a self,
        targets: &'a [AccountRevocationTarget],
        children: &'a [RetiredAccountChild],
    ) -> AccountFuture<'a, ()> {
        Box::pin(async move {
            self.inner.invalidate(targets, children).await?;
            self.clock.advance(SESSION_LIFETIME_MS + 1);
            Ok(())
        })
    }
}
#[tokio::test]
async fn parent_expiry_after_real_holding_commits_denies_recovery_and_keeps_old_key_without_ack() {
    fixture::with_system(|system| async move {
        let a = fixture::login(&system, "a@example.test", None).await;
        let retired = fixture::children(&system, &a, "fixture-a").await;
        let invalidation = Arc::new(AdvanceAfterInvalidation {
            inner: fixture::invalidator(&system.holdings, system.scenario.clock.clone()),
            clock: system.scenario.clock.clone(),
        });
        let auth = Arc::new(
            AccountAuthentication::new(
                fixture::fixture::account::policy(),
                Arc::new(
                    PostgresAccountRepository::from_control_database(
                        system.control.clone(),
                        "development".into(),
                        true,
                        invalidation,
                        system.scenario.clock.clone(),
                        fixture::fixture::account::holding_registry(),
                    )
                    .unwrap(),
                ),
                Arc::new(Argon2Passwords::new(2).unwrap()),
                Arc::new(SystemAuthenticationSecrets),
                system.scenario.clock.clone(),
            )
            .unwrap(),
        );
        let router = account_auth_router(auth);
        let mut jar = from_proof(&a);
        jar.0.remove(GROUP);
        let (status, _, headers) = initialize(&router, &mut jar, 1).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(!headers.contains_key(header::SET_COOKIE));
        assert_eq!(key_count(&system).await, 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT revision FROM account_device_browsers")
                .fetch_one(system.control.raw())
                .await
                .unwrap(),
            1
        );
        fixture::assert_fenced(&system, &retired, "fixture-a").await;
    })
    .await;
}

#[tokio::test]
async fn key_migration_backfills_only_original_native_digest_ids_and_is_idempotent() {
    use erpx_server_runtime::public::postgres::{
        MigrationTrack, load_migrations, migrate_database,
    };
    fixture::database::with_database(2,|pool|async move{
        let files=load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),MigrationTrack::Control).await.unwrap();
        let before=files.iter().filter(|file|file.name()<"0043_").cloned().collect::<Vec<_>>();
        migrate_database(&pool,MigrationTrack::Control,&before).await.unwrap();
        let native="a".repeat(64);sqlx::query("INSERT INTO account_device_browsers(id,revision,updated_at) VALUES($1,7,123),('b0ee2b0e-1234-4567-8888-123456789012',2,123)").bind(&native).execute(pool.raw()).await.unwrap();
        assert_eq!(migrate_database(&pool,MigrationTrack::Control,&files).await.unwrap().applied,1);
        let rows:Vec<(String,String)>=sqlx::query_as("SELECT cookie_digest,browser_id FROM native_account_device_group_keys").fetch_all(pool.raw()).await.unwrap();assert_eq!(rows,vec![(native.clone(),native)]);
        assert_eq!(migrate_database(&pool,MigrationTrack::Control,&files).await.unwrap().applied,0);
    }).await;
}

#[tokio::test]
async fn one_saved_proof_recovers_only_its_group_without_authorizing_or_selecting_unproven_current_account()
 {
    fixture::with_system(|system| async move {
        let a = fixture::login(&system, "a@example.test", None).await;
        let a_saved = a.saved[0].clone();
        let b = fixture::login(&system, "b@example.test", Some(a)).await;
        let b_children = fixture::children(&system, &b, "fixture-b").await;
        let before = parent_state(&system).await;
        let router = account_auth_router(system.scenario.auth.clone());
        let mut jar = Jar::default();
        jar.0
            .insert(format!("{SAVED}{}", a_saved.0), a_saved.1.expose().into());
        let (status, body, _) = send(&router, &mut jar, "GET", DEVICE, None, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["revision"], 2);
        assert_eq!(body["data"]["accounts"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["accounts"][0]["email"], "a@example.test");
        assert_eq!(body["data"]["accounts"][0]["current"], false);
        let (status, body, _) = initialize(&router, &mut jar, 2).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["revision"], 3);
        assert_eq!(body["data"]["accounts"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["accounts"][0]["current"], false);
        assert!(!jar.0.contains_key(ACTIVE));
        assert_eq!(parent_state(&system).await, before);
        fixture::assert_fenced(&system, &b_children, "fixture-b").await;
        assert!(
            system
                .scenario
                .auth
                .session(&jar.proof(), RequestAudience::Account)
                .await
                .unwrap()
                .is_none()
        );
        let b_handle = b.saved.last().unwrap().0.clone();
        let (status, _, headers) = send(
            &router,
            &mut jar,
            "POST",
            &format!("{DEVICE}/active"),
            Some(ACME),
            Some(json!({"handle":b_handle,"revision":3})),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(!headers.contains_key(header::SET_COOKIE));
        let (status, body, _) = send(
            &router,
            &mut jar,
            "POST",
            &format!("{DEVICE}/active"),
            Some(ACME),
            Some(json!({"handle":a_saved.0,"revision":3})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["revision"], 4);
        assert_eq!(
            system
                .scenario
                .auth
                .session(&jar.proof(), RequestAudience::Account)
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
async fn revoked_parent_proofs_cannot_recover_or_resurrect_a_group() {
    fixture::with_system(|system| async move {
        let a = fixture::login(&system, "a@example.test", None).await;
        let router = account_auth_router(system.scenario.auth.clone());
        let mut lost = from_proof(&a);
        lost.0.remove(GROUP);
        system
            .scenario
            .auth
            .sign_out(&a, RequestAudience::Account)
            .await
            .unwrap();
        let before = key_count(&system).await;
        let (status, body, _) = send(&router, &mut lost, "GET", DEVICE, None, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body["data"],
            json!({"initialized":false,"revision":0,"maximum":5,"accounts":[]})
        );
        let (status, _, headers) = initialize(&router, &mut lost, 1).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(!headers.contains_key(header::SET_COOKIE));
        assert_eq!(key_count(&system).await, before);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM session")
                .fetch_one(system.control.raw())
                .await
                .unwrap(),
            0
        );
    })
    .await;
}

#[tokio::test]
async fn expired_recovery_reservation_cannot_complete_or_release_a_newer_original_group_nonce() {
    fixture::with_system(|system| async move {
        let a = fixture::login(&system, "a@example.test", None).await;
        let retired = fixture::children(&system, &a, "fixture-a").await;
        let mut proof = a;
        proof.browser = None;
        let secrets = SystemAuthenticationSecrets;
        let repository = PostgresAccountRepository::from_control_database(
            system.control.clone(),
            "development".into(),
            true,
            fixture::invalidator(&system.holdings, system.scenario.clock.clone()),
            system.scenario.clock.clone(),
            fixture::fixture::account::holding_registry(),
        )
        .unwrap();
        let make = |nonce: OpaqueSecret, browser: OpaqueSecret, now: i64| ReserveSignIn {
            proof: proof.proof(&secrets),
            candidate_browser: secrets.digest(SecretPurpose::DeviceBrowser, &browser),
            nonce: secrets.digest(SecretPurpose::Reservation, &nonce),
            email: EmailAddress::new("b@example.test").unwrap(),
            expected_revision: Some(1),
            now,
        };
        let first = repository
            .reserve_sign_in(make(
                secrets.issue().unwrap(),
                secrets.issue().unwrap(),
                fixture::fixture::account::NOW,
            ))
            .await
            .unwrap();
        system.scenario.clock.advance(120001);
        let second = repository
            .reserve_sign_in(make(
                secrets.issue().unwrap(),
                secrets.issue().unwrap(),
                fixture::fixture::account::NOW + 120001,
            ))
            .await
            .unwrap();
        let result = repository
            .complete_sign_in(CompleteSignIn {
                proof: proof.proof(&secrets),
                reservation: first.clone(),
                credential: first.credential.clone().unwrap(),
                session_id: secrets.identifier().unwrap(),
                session_digest: secrets.digest(SecretPurpose::Session, &secrets.issue().unwrap()),
                handle: secrets.identifier().unwrap(),
                now: fixture::fixture::account::NOW + 120001,
            })
            .await;
        fixture::fixture::account::assert_error(result, "DEVICE_ACCOUNTS_CHANGED");
        repository.release_sign_in(first).await.unwrap();
        let pending: String =
            sqlx::query_scalar("SELECT pending_nonce FROM account_device_browsers")
                .fetch_one(system.control.raw())
                .await
                .unwrap();
        assert_eq!(pending, second.nonce.as_str());
        assert_eq!(key_count(&system).await, 1);
        for (pool, child) in system.holdings.iter().zip(&retired) {
            assert!(!fixture::fenced(pool, child, "fixture-a").await);
        }
        let router = account_auth_router(system.scenario.auth.clone());
        let mut jar = from_proof(&proof);
        let (status, body, headers) = initialize(&router, &mut jar, 1).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "ACCOUNT_SIGN_IN_PENDING");
        assert!(!headers.contains_key(header::SET_COOKIE));
        repository.release_sign_in(second).await.unwrap();
        assert_eq!(initialize(&router, &mut jar, 1).await.0, StatusCode::OK);
        fixture::assert_fenced(&system, &retired, "fixture-a").await;
    })
    .await;
}

async fn ambiguous_fixture(system: &fixture::System) -> (Jar, PresentedSecrets, PresentedSecrets) {
    let a = fixture::login(system, "a@example.test", None).await;
    let b = fixture::login(system, "b@example.test", None).await;
    let mut jar = from_proof(&a);
    jar.0.remove(GROUP);
    jar.0.remove(ACTIVE);
    for (handle, token) in &b.saved {
        jar.0
            .insert(format!("{SAVED}{handle}"), token.expose().into());
    }
    (jar, a, b)
}
fn old_parents_unchanged(before: &Value, after: &Value) {
    for old in before.as_array().unwrap() {
        assert!(after.as_array().unwrap().contains(old));
    }
}
#[tokio::test]
async fn ambiguous_ordinary_signin_fails_normally_for_wrong_password_then_enrolls_only_freshly_authenticated_account()
 {
    fixture::with_system(|system| async move {
        let (mut jar, a, b) = ambiguous_fixture(&system).await;
        let a_children = fixture::children(&system, &a, "fixture-a").await;
        let b_children = fixture::children(&system, &b, "fixture-b").await;
        let before = parent_state(&system).await;
        let router = account_auth_router(system.scenario.auth.clone());
        let mut failures = Vec::new();
        for email in ["b@example.test", "unknown@example.test"] {
            let (status, body, headers) = send(
                &router,
                &mut jar,
                "POST",
                "/api/auth/sign-in/email",
                Some(ACCOUNT),
                Some(json!({"email":email,"password":"synthetic-wrong-password"})),
            )
            .await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
            assert!(!headers.contains_key(header::SET_COOKIE));
            failures.push(body);
            assert_eq!(key_count(&system).await, 2);
            assert_eq!(parent_state(&system).await, before);
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM account_device_browsers WHERE pending_nonce IS NOT NULL"
                )
                .fetch_one(system.control.raw())
                .await
                .unwrap(),
                0
            );
        }
        assert_eq!(failures[0], failures[1]);
        system.scenario.clock.advance(123456);
        let (status, body, headers) = send(
            &router,
            &mut jar,
            "POST",
            "/api/auth/sign-in/email",
            Some(ACCOUNT),
            Some(json!({"email":"b@example.test","password":fixture::fixture::PASSWORD})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["user"]["email"], "b@example.test");
        assert_eq!(
            headers
                .get_all(header::SET_COOKIE)
                .iter()
                .filter(|value| !value.to_str().unwrap().contains("Max-Age=0"))
                .count(),
            3
        );
        // The ordinary handler expires old saved-cookie names outside the new
        // display group. Their original server-side families remain untouched.
        for old in [&a, &b] {
            for (handle, _) in &old.saved {
                assert!(headers.get_all(header::SET_COOKIE).iter().any(|value| {
                    let cookie = value.to_str().unwrap();
                    cookie.starts_with(&format!("{SAVED}{handle}=;"))
                        && cookie.split(';').any(|part| part.trim() == "Max-Age=0")
                }));
            }
        }
        no_credentials(&body, &jar);
        assert_eq!(key_count(&system).await, 3);
        let after = parent_state(&system).await;
        assert_eq!(after.as_array().unwrap().len(), 3);
        old_parents_unchanged(&before, &after);
        let session = system
            .scenario
            .auth
            .session(&jar.proof(), RequestAudience::Account)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            session.authenticated_at,
            fixture::fixture::account::NOW + 123456
        );
        assert_eq!(
            session.expires_at,
            session.authenticated_at + SESSION_LIFETIME_MS
        );
        let state = system.scenario.auth.devices(&jar.proof()).await.unwrap();
        assert_eq!(state.revision, 1);
        assert_eq!(state.accounts.len(), 1);
        assert_eq!(state.accounts[0].email, "b@example.test");
        // Even replaying the independently valid old saved proofs cannot cross
        // into this freshly authenticated group's membership boundary.
        for old in [&a, &b] {
            for (handle, secret) in &old.saved {
                jar.0
                    .insert(format!("{SAVED}{handle}"), secret.expose().into());
            }
        }
        for old in [&a, &b] {
            for (handle, _) in &old.saved {
                let (status, _, headers) = send(
                    &router,
                    &mut jar,
                    "POST",
                    &format!("{DEVICE}/active"),
                    Some(ACME),
                    Some(json!({"handle":handle,"revision":1})),
                )
                .await;
                assert_eq!(status, StatusCode::UNAUTHORIZED);
                assert!(!headers.contains_key(header::SET_COOKIE));
            }
        }
        for (children, user) in [(&a_children, "fixture-a"), (&b_children, "fixture-b")] {
            for (pool, child) in system.holdings.iter().zip(children) {
                assert!(!fixture::fenced(pool, child, user).await);
            }
        }
        assert!(
            system
                .scenario
                .auth
                .session(&a, RequestAudience::Account)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            system
                .scenario
                .auth
                .session(&b, RequestAudience::Account)
                .await
                .unwrap()
                .is_some()
        );
    })
    .await;
}

#[tokio::test]
async fn fresh_credential_isolation_requires_initial_revision_and_cannot_be_requested_with_a_client_bypass_flag()
 {
    fixture::with_system(|system|async move{
        let(mut jar,_,_)=ambiguous_fixture(&system).await;let before=parent_state(&system).await;let router=account_auth_router(system.scenario.auth.clone());
        let(status,_,headers)=send(&router,&mut jar,"POST","/api/auth/sign-in/email",Some(ACCOUNT),Some(json!({"email":"b@example.test","password":fixture::fixture::PASSWORD,"revision":1}))).await;
        assert_eq!(status,StatusCode::CONFLICT);assert!(!headers.contains_key(header::SET_COOKIE));assert_eq!(key_count(&system).await,2);assert_eq!(parent_state(&system).await,before);
        for extra in ["ignoreProof","freshGroup","enrollment"] {let mut input=json!({"email":"b@example.test","password":fixture::fixture::PASSWORD,"revision":0});input[extra]=json!(true);let(status,_,headers)=send(&router,&mut jar,"POST","/api/auth/sign-in/email",Some(ACCOUNT),Some(input)).await;assert!(status.is_client_error());assert!(!headers.contains_key(header::SET_COOKIE));}
        assert_eq!(key_count(&system).await,2);
        let state=sign_in(&router,&mut jar,"b@example.test",Some(0),ACME).await;assert_eq!(state["revision"],1);assert_eq!(state["accounts"].as_array().unwrap().len(),1);assert_eq!(key_count(&system).await,3);old_parents_unchanged(&before,&parent_state(&system).await);
    }).await;
}

#[tokio::test]
async fn ambiguous_saved_proofs_need_a_valid_verification_before_fresh_isolated_enrollment() {
    fixture::with_system(|system| async move {
        let (jar, a, b) = ambiguous_fixture(&system).await;
        let a_children = fixture::children(&system, &a, "fixture-a").await;
        let b_children = fixture::children(&system, &b, "fixture-b").await;
        let before = parent_state(&system).await;
        let proof = jar.proof();
        assert!(
            system
                .scenario
                .credentials
                .verify_email("a".repeat(64), &proof, None)
                .await
                .is_err()
        );
        assert_eq!(key_count(&system).await, 2);
        assert_eq!(parent_state(&system).await, before);
        system
            .scenario
            .credentials
            .request(
                CredentialProofKind::EmailVerification,
                "unverified@example.test",
                None,
            )
            .await
            .unwrap();
        let mail = fixture::fixture::mail(
            &system.control,
            &system.scenario,
            CredentialProofKind::EmailVerification,
            "unverified@example.test",
        )
        .await;
        assert!(
            system
                .scenario
                .credentials
                .verify_email(mail.token.expose().into(), &proof, Some(1))
                .await
                .is_err()
        );
        assert_eq!(key_count(&system).await, 2);
        assert_eq!(parent_state(&system).await, before);
        let verified = system
            .scenario
            .credentials
            .verify_email(mail.token.expose().into(), &proof, None)
            .await
            .unwrap();
        assert!(verified.browser_secret.is_some());
        assert_eq!(verified.result.session.user.id, "fixture-unverified");
        assert_eq!(verified.result.devices.revision, 1);
        assert_eq!(verified.result.devices.accounts.len(), 1);
        assert_eq!(key_count(&system).await, 3);
        old_parents_unchanged(&before, &parent_state(&system).await);
        for (children, user) in [(&a_children, "fixture-a"), (&b_children, "fixture-b")] {
            for (pool, child) in system.holdings.iter().zip(children) {
                assert!(!fixture::fenced(pool, child, user).await);
            }
        }
    })
    .await;
}
