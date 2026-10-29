#[path = "../../support/rust-account-revocation.rs"]
mod support;
use erpx_server_identity_access::public::{account_revocation::*, *};
use std::{collections::BTreeMap, sync::Arc};

#[tokio::test]
async fn selecting_the_current_saved_account_preserves_revision_and_both_holding_contexts() {
    with_system(|system| async move {
        let a = login(&system, "a@example.test", None).await;
        let live = children(&system, &a, "fixture-a").await;
        let state = system.scenario.auth.devices(&a).await.unwrap();
        let current = state
            .accounts
            .iter()
            .find(|account| account.current)
            .unwrap();
        let result = system
            .scenario
            .auth
            .select(&a, current.handle.clone(), state.revision)
            .await
            .unwrap();
        assert_eq!(result.devices.revision, state.revision);
        for (pool, child) in system.holdings.iter().zip(&live) {
            assert!(!fenced(pool, child, "fixture-a").await);
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM active_work_contexts WHERE session_id=$1)",
            )
            .bind(&child.result.session_id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert!(exists);
        }
    })
    .await;
}
use support::*;

#[tokio::test]
async fn actual_sso_switch_retires_both_holdings_before_ack_and_preserves_an_independent_device() {
    with_system(|system| async move {
        let a = login(&system, "a@example.test", None).await;
        let retired = children(&system, &a, "fixture-a").await;
        let sibling = login(&system, "a@example.test", None).await;
        let alive = children(&system, &sibling, "fixture-a").await;
        let switched = login(&system, "b@example.test", Some(a)).await;
        assert_fenced(&system, &retired, "fixture-a").await;
        assert_eq!(
            system
                .scenario
                .auth
                .devices(&switched)
                .await
                .unwrap()
                .revision,
            2
        );
        for (pool, child) in system.holdings.iter().zip(&alive) {
            assert!(!fenced(pool, child, "fixture-a").await);
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM active_work_contexts WHERE session_id=$1")
                    .bind(&child.result.session_id)
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(count, 1);
        }
    })
    .await;
}

#[tokio::test]
async fn inactive_saved_account_removal_does_not_retire_new_current_children() {
    with_system(|system| async move {
        let a = login(&system, "a@example.test", None).await;
        let a_handle = system.scenario.auth.devices(&a).await.unwrap().accounts[0]
            .handle
            .clone();
        let mut b = login(&system, "b@example.test", Some(a)).await;
        let b_handle = system
            .scenario
            .auth
            .devices(&b)
            .await
            .unwrap()
            .accounts
            .iter()
            .find(|a| a.current)
            .unwrap()
            .handle
            .clone();
        let old = children(&system, &b, "fixture-b").await;
        let selected = system.scenario.auth.select(&b, a_handle, 2).await.unwrap();
        b.active = Some(selected.session_secret);
        let current = children(&system, &b, "fixture-a").await;
        let result = system.scenario.auth.remove(&b, b_handle, 3).await.unwrap();
        assert_eq!(result.revision, 4);
        assert_eq!(result.accounts.len(), 1);
        assert_fenced(&system, &old, "fixture-b").await;
        for (pool, child) in system.holdings.iter().zip(&current) {
            assert!(!fenced(pool, child, "fixture-a").await);
        }
    })
    .await;
}

#[tokio::test]
async fn second_holding_failure_keeps_first_fence_without_ack_then_retries_idempotently() {
    with_system(|system| async move {
        let a = login(&system, "a@example.test", None).await;
        let retired = children(&system, &a, "fixture-a").await;
        sqlx::query("ALTER TABLE active_work_contexts RENAME TO fixture_unavailable_contexts")
            .execute(system.holdings[1].raw())
            .await
            .unwrap();
        let result = system
            .scenario
            .auth
            .sign_in("b@example.test", fixture::PASSWORD.into(), &a, Some(1))
            .await;
        fixture::account::assert_error(result, "DEVICE_ACCOUNT_REENTRY_REQUIRED");
        assert!(fenced(&system.holdings[0], &retired[0], "fixture-a").await);
        assert_eq!(system.scenario.auth.devices(&a).await.unwrap().revision, 1);
        let live: i64 = sqlx::query_scalar("SELECT count(*) FROM session WHERE id=ANY($1::text[])")
            .bind(
                retired
                    .iter()
                    .map(|c| c.result.session_id.clone())
                    .collect::<Vec<_>>(),
            )
            .fetch_one(system.control.raw())
            .await
            .unwrap();
        assert_eq!(live, 2);
        sqlx::query("ALTER TABLE fixture_unavailable_contexts RENAME TO active_work_contexts")
            .execute(system.holdings[1].raw())
            .await
            .unwrap();
        let b = login(&system, "b@example.test", Some(a)).await;
        assert_eq!(system.scenario.auth.devices(&b).await.unwrap().revision, 2);
        assert_fenced(&system, &retired, "fixture-a").await;
    })
    .await;
}

#[tokio::test]
async fn deferred_control_serialization_failure_never_replays_a_committed_holding_callback() {
    with_system(|system|async move{
 let a=login(&system,"a@example.test",None).await;let retired=children(&system,&a,"fixture-a").await;
 sqlx::raw_sql("CREATE SEQUENCE fixture_control_attempts; CREATE FUNCTION fixture_control_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('fixture_control_attempts'); RAISE EXCEPTION 'Synthetic late Control serialization' USING ERRCODE='40001'; END $$; CREATE CONSTRAINT TRIGGER fixture_control_failure AFTER UPDATE ON account_device_browsers DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.revision=2) EXECUTE FUNCTION fixture_control_failure()").execute(system.control.raw()).await.unwrap();
 fixture::account::assert_error(system.scenario.auth.sign_in("b@example.test",fixture::PASSWORD.into(),&a,Some(1)).await,"ACCOUNT_PERSISTENCE_UNAVAILABLE");
 let attempts:i64=sqlx::query_scalar("SELECT last_value FROM fixture_control_attempts").fetch_one(system.control.raw()).await.unwrap();assert_eq!(attempts,1);assert_eq!(system.scenario.auth.devices(&a).await.unwrap().revision,1);assert_fenced(&system,&retired,"fixture-a").await;
}).await;
}

struct AdvanceAfterInvalidation {
    inner: Arc<dyn AccountWorkContextInvalidation>,
    clock: Arc<fixture::MutableClock>,
    millis: i64,
}
impl AccountWorkContextInvalidation for AdvanceAfterInvalidation {
    fn invalidate<'a>(
        &'a self,
        targets: &'a [AccountRevocationTarget],
        children: &'a [RetiredAccountChild],
    ) -> AccountFuture<'a, ()> {
        Box::pin(async move {
            self.inner.invalidate(targets, children).await?;
            self.clock.advance(self.millis);
            Ok(())
        })
    }
}
fn auth_with(
    system: &System,
    invalidation: Arc<dyn AccountWorkContextInvalidation>,
) -> AccountAuthentication {
    AccountAuthentication::new(
        fixture::account::policy(),
        Arc::new(
            PostgresAccountRepository::from_control_database(
                system.control.clone(),
                "development".into(),
                true,
                invalidation,
                system.scenario.clock.clone(),
                fixture::account::holding_registry(),
            )
            .unwrap(),
        ),
        Arc::new(Argon2Passwords::new(2).unwrap()),
        Arc::new(SystemAuthenticationSecrets),
        system.scenario.clock.clone(),
    )
    .unwrap()
}
#[tokio::test]
async fn reservation_expiring_after_real_holding_commits_cannot_ack_a_selection() {
    with_system(|system| async move {
        let a = login(&system, "a@example.test", None).await;
        let retired = children(&system, &a, "fixture-a").await;
        let auth = auth_with(
            &system,
            Arc::new(AdvanceAfterInvalidation {
                inner: invalidator(&system.holdings, system.scenario.clock.clone()),
                clock: system.scenario.clock.clone(),
                millis: 120_001,
            }),
        );
        fixture::account::assert_error(
            auth.sign_in("b@example.test", fixture::PASSWORD.into(), &a, Some(1))
                .await,
            "DEVICE_ACCOUNTS_CHANGED",
        );
        assert_eq!(system.scenario.auth.devices(&a).await.unwrap().revision, 1);
        assert_fenced(&system, &retired, "fixture-a").await;
        // The unchanged Account can recover using an ordinary fresh SSO transaction.
        let fresh = child(&system, &a, ACME).await;
        assert_ne!(fresh.result.session_id, retired[0].result.session_id);
    })
    .await;
}

async fn credential_revocation(system: System, reset: bool) {
    let a = login(&system, "a@example.test", None).await;
    let retired = children(&system, &a, "fixture-a").await;
    let sibling = login(&system, "a@example.test", None).await;
    let sibling_children = children(&system, &sibling, "fixture-a").await;
    let other = login(&system, "b@example.test", None).await;
    let untouched = children(&system, &other, "fixture-b").await;
    if reset {
        system
            .scenario
            .credentials
            .request(CredentialProofKind::PasswordReset, "a@example.test", None)
            .await
            .unwrap();
        let mail = fixture::mail(
            &system.control,
            &system.scenario,
            CredentialProofKind::PasswordReset,
            "a@example.test",
        )
        .await;
        system
            .scenario
            .credentials
            .reset_password(mail.token.expose().into(), fixture::NEW_PASSWORD.into())
            .await
            .unwrap();
    } else {
        system
            .scenario
            .credentials
            .change_password(&a, fixture::PASSWORD.into(), fixture::NEW_PASSWORD.into())
            .await
            .unwrap();
    }
    assert_fenced(&system, &retired, "fixture-a").await;
    assert_fenced(&system, &sibling_children, "fixture-a").await;
    for (pool, child) in system.holdings.iter().zip(&untouched) {
        assert!(!fenced(pool, child, "fixture-b").await);
    }
    assert!(
        system
            .scenario
            .auth
            .session(&a, RequestAudience::Account)
            .await
            .unwrap()
            .is_none()
    );
}
#[tokio::test]
async fn reset_password_fences_all_own_devices_in_both_holdings() {
    with_system(|system| credential_revocation(system, true)).await;
}
#[tokio::test]
async fn current_password_change_fences_all_own_devices_in_both_holdings() {
    with_system(|system| credential_revocation(system, false)).await;
}

#[tokio::test]
async fn email_verification_selection_uses_the_same_mandatory_holding_fences() {
    with_system(|system| async move {
        let a = login(&system, "a@example.test", None).await;
        let retired = children(&system, &a, "fixture-a").await;
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
        let mail = fixture::mail(
            &system.control,
            &system.scenario,
            CredentialProofKind::EmailVerification,
            "unverified@example.test",
        )
        .await;
        let verified = system
            .scenario
            .credentials
            .verify_email(mail.token.expose().into(), &a, Some(1))
            .await
            .unwrap();
        assert_eq!(verified.result.devices.revision, 2);
        assert_eq!(verified.result.session.user.id, "fixture-unverified");
        assert_fenced(&system, &retired, "fixture-a").await;
    })
    .await;
}

#[tokio::test]
async fn holding_signout_fences_the_whole_parent_family_and_preserves_an_independent_device() {
    with_system(|system| async move {
        let a = login(&system, "a@example.test", None).await;
        let retired = children(&system, &a, "fixture-a").await;
        let sibling = login(&system, "a@example.test", None).await;
        let alive = children(&system, &sibling, "fixture-a").await;
        system
            .scenario
            .auth
            .sign_out(
                &PresentedSecrets {
                    browser: None,
                    active: Some(retired[0].token.clone()),
                    saved: vec![],
                },
                RequestAudience::Holding {
                    audience: ACME.into(),
                },
            )
            .await
            .unwrap();
        assert_fenced(&system, &retired, "fixture-a").await;
        assert!(
            system
                .scenario
                .auth
                .session(&a, RequestAudience::Account)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            system
                .scenario
                .auth
                .session(&sibling, RequestAudience::Account)
                .await
                .unwrap()
                .is_some()
        );
        for (pool, child) in system.holdings.iter().zip(&alive) {
            assert!(!fenced(pool, child, "fixture-a").await);
        }
    })
    .await;
}

#[tokio::test]
async fn absent_or_changed_trusted_writer_bindings_fail_before_any_holding_mutation() {
    with_system(|system| async move {
        let a=login(&system,"a@example.test",None).await;
        let retired=children(&system,&a,"fixture-a").await;
        let absent=Arc::new(PostgresAccountWorkContextInvalidation::new(BTreeMap::new(),system.scenario.clock.clone()).unwrap());
        fixture::account::assert_error(auth_with(&system,absent).sign_in("b@example.test",fixture::PASSWORD.into(),&a,Some(1)).await,"DEVICE_ACCOUNT_REENTRY_REQUIRED");
        sqlx::query("UPDATE holding_bindings SET write_binding='HOLDING_CHANGED_DB' WHERE holding_id='h002'").execute(system.control.raw()).await.unwrap();
        fixture::account::assert_error(system.scenario.auth.sign_in("b@example.test",fixture::PASSWORD.into(),&a,Some(1)).await,"DEVICE_ACCOUNT_REENTRY_REQUIRED");
        assert_eq!(system.scenario.auth.devices(&a).await.unwrap().revision,1);
        for (pool,child) in system.holdings.iter().zip(&retired){assert!(!fenced(pool,child,"fixture-a").await);let count:i64=sqlx::query_scalar("SELECT count(*) FROM active_work_contexts WHERE session_id=$1").bind(&child.result.session_id).fetch_one(pool.raw()).await.unwrap();assert_eq!(count,1);}
    }).await;
}

#[tokio::test]
async fn reset_proof_expiry_after_real_holding_commits_rolls_back_control_without_resurrection() {
    with_system(|system| async move {
        let a=login(&system,"a@example.test",None).await;
        let retired=children(&system,&a,"fixture-a").await;
        system.scenario.credentials.request(CredentialProofKind::PasswordReset,"a@example.test",None).await.unwrap();
        let mail=fixture::mail(&system.control,&system.scenario,CredentialProofKind::PasswordReset,"a@example.test").await;
        let delayed=Arc::new(AdvanceAfterInvalidation{inner:invalidator(&system.holdings,system.scenario.clock.clone()),clock:system.scenario.clock.clone(),millis:CREDENTIAL_PROOF_LIFETIME_MS+1});
        let credentials=AccountCredentials::new(fixture::account::policy(),Arc::new(PostgresAccountCredentials::from_control_database(system.control.clone(),"development".into(),Arc::new(UnavailableInvitationCredentialAuthority),delayed,system.scenario.clock.clone(),
fixture::account::holding_registry(),
).unwrap()),Arc::new(Argon2Passwords::new(2).unwrap()),Arc::new(SystemAuthenticationSecrets),system.scenario.keyring.clone(),system.scenario.clock.clone()).unwrap();
        fixture::account::assert_error(credentials.reset_password(mail.token.expose().into(),fixture::NEW_PASSWORD.into()).await,"ACCOUNT_PROOF_UNAVAILABLE");
        assert_fenced(&system,&retired,"fixture-a").await;
        assert_eq!(system.scenario.auth.devices(&a).await.unwrap().revision,1);
        let remains:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM native_account_proofs WHERE kind='password_reset' AND consumed_at IS NULL)").fetch_one(system.control.raw()).await.unwrap();assert!(remains);
        // Current old credential remains usable; no staged reset was committed.
        system.scenario.auth.sign_in("a@example.test",fixture::PASSWORD.into(),&fixture::account::empty(),None).await.unwrap();
    }).await;
}
