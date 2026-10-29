#[path = "../../support/rust-postgres.rs"]
mod database;
#[allow(
    dead_code,
    reason = "mail suites use the credential queue/keyring/clock fixture; auth helpers have their own suites"
)]
#[path = "../../support/rust-account-credentials.rs"]
mod fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use erpx_server_identity_access::public::{account_mail::*, *};
use erpx_server_runtime::public::postgres::DatabasePool;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use tower::ServiceExt;

struct FakeProvider {
    profile: AccountMailProviderProfile,
    calls: AtomicUsize,
    deliveries: Mutex<BTreeMap<String, String>>,
    outcomes: Mutex<VecDeque<AccountMailSendOutcome>>,
}
impl FakeProvider {
    fn new(kind: AccountMailProviderKind, outcomes: Vec<AccountMailSendOutcome>) -> Self {
        Self {
            profile: AccountMailProviderProfile {
                kind,
                profile_id: "public-synthetic-fake".into(),
                from: "ERPX <noreply@example.test>".into(),
            },
            calls: AtomicUsize::new(0),
            deliveries: Mutex::new(BTreeMap::new()),
            outcomes: Mutex::new(outcomes.into()),
        }
    }
}
impl AccountMailProvider for FakeProvider {
    fn profile(&self) -> &AccountMailProviderProfile {
        &self.profile
    }
    fn send<'a>(&'a self, message: &'a AccountEmailMessage) -> AccountMailSendFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let fingerprint = self.profile.fingerprint(message).unwrap();
            let mut deliveries = self.deliveries.lock().unwrap();
            if let Some(prior) = deliveries.get(message.delivery_key()) {
                assert_eq!(
                    prior, &fingerprint,
                    "same provider key must keep the same payload"
                );
                return AccountMailSendOutcome::Accepted {
                    provider_id: "synthetic-provider-receipt".into(),
                };
            }
            let outcome = self.outcomes.lock().unwrap().pop_front().unwrap_or(
                AccountMailSendOutcome::Accepted {
                    provider_id: "synthetic-provider-receipt".into(),
                },
            );
            if matches!(outcome, AccountMailSendOutcome::Accepted { .. }) {
                deliveries.insert(message.delivery_key().to_owned(), fingerprint);
            }
            outcome
        })
    }
}
async fn setup(pool: &DatabasePool) -> fixture::Scenario {
    fixture::setup(pool, Arc::new(UnavailableInvitationCredentialAuthority)).await
}
async fn queue(scenario: &fixture::Scenario, kind: CredentialProofKind) {
    scenario
        .credentials
        .request(
            kind,
            if kind == CredentialProofKind::EmailVerification {
                "unverified@example.test"
            } else {
                "a@example.test"
            },
            Some(if kind == CredentialProofKind::PasswordReset {
                "/reset-password"
            } else {
                "/onboarding"
            }),
        )
        .await
        .unwrap();
}
fn worker(
    pool: &DatabasePool,
    scenario: &fixture::Scenario,
    provider: Arc<dyn AccountMailProvider>,
    opening: Arc<dyn AccountMailOpening>,
) -> AccountMailWorker {
    AccountMailWorker::new(
        Arc::new(PostgresAccountMail::new(
            pool.clone(),
            scenario.clock.clone(),
        )),
        opening,
        provider,
        Arc::new(SystemAuthenticationSecrets),
        AccountMailPolicy::new(fixture::account::policy()).unwrap(),
        scenario.clock.clone(),
    )
}
async fn state(pool: &DatabasePool) -> (String, i64, Option<String>, Option<String>) {
    sqlx::query_as("SELECT state,attempts,provider_id,failure_code FROM native_account_mail_outbox ORDER BY created_at DESC,id DESC LIMIT 1").fetch_one(pool.raw()).await.unwrap()
}
async fn claim(repo: &PostgresAccountMail, now: i64) -> AccountMailAttempt {
    match repo
        .claim(SystemAuthenticationSecrets.issue().unwrap(), now)
        .await
        .unwrap()
    {
        AccountMailClaim::Ready(job) => job,
        _ => panic!("expected a real queued mail job"),
    }
}
fn content(scenario: &fixture::Scenario, job: &AccountMailAttempt) -> AccountEmailMessage {
    let opened = scenario
        .keyring
        .open(&job.proof_id, &job.payload_ciphertext)
        .unwrap();
    AccountMailPolicy::new(fixture::account::policy())
        .unwrap()
        .render(job.email.clone(), &opened, &job.id)
        .unwrap()
}

#[tokio::test]
async fn encrypted_verification_and_reset_dispatch_once_and_only_real_receipts_mark_sent() {
    database::with_database(1,|pool|async move {
        let scenario=setup(&pool).await;let provider=Arc::new(FakeProvider::new(AccountMailProviderKind::Resend,vec![]));let worker=worker(&pool,&scenario,provider.clone(),scenario.keyring.clone());
        for kind in [CredentialProofKind::EmailVerification,CredentialProofKind::PasswordReset] {queue(&scenario,kind).await;assert!(worker.deliver_one().await.unwrap());assert_eq!(state(&pool).await.0,"sent");assert!(!worker.deliver_one().await.unwrap());}
        assert_eq!(provider.calls.load(Ordering::SeqCst),2);assert_eq!(provider.deliveries.lock().unwrap().len(),2);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_account_mail_events WHERE action='accepted'").fetch_one(pool.raw()).await.unwrap(),2);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_account_mail_outbox WHERE payload_ciphertext LIKE 'v1.%' AND message_fingerprint ~ '^[0-9a-f]{64}$' AND dispatch_lease_digest ~ '^[0-9a-f]{64}$'").fetch_one(pool.raw()).await.unwrap(),2);
    }).await;
}

#[tokio::test]
async fn consumed_expired_suspended_recipient_and_credential_changes_never_call_a_provider() {
    database::with_database(1,|pool|async move {
        let scenario=setup(&pool).await;let provider=Arc::new(FakeProvider::new(AccountMailProviderKind::Resend,vec![]));let worker=worker(&pool,&scenario,provider.clone(),scenario.keyring.clone());
        for case in 0..5 {
            scenario.clock.advance(61_000);queue(&scenario,CredentialProofKind::PasswordReset).await;
            match case {
                0=>{sqlx::query("UPDATE native_account_proofs SET consumed_at=created_at WHERE consumed_at IS NULL").execute(pool.raw()).await.unwrap();},
                1=>scenario.clock.advance(3_600_000),
                2=>{sqlx::query("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES('suspended-test','fixture-a','Synthetic','suspended',0)").execute(pool.raw()).await.unwrap();},
                3=>{sqlx::query("UPDATE \"user\" SET email='changed@example.test' WHERE id='fixture-a'").execute(pool.raw()).await.unwrap();},
                4=>{sqlx::query("UPDATE native_account_proofs SET credential_fingerprint=repeat('b',64) WHERE consumed_at IS NULL").execute(pool.raw()).await.unwrap();},_=>unreachable!(),
            }
            assert!(worker.deliver_one().await.unwrap());assert_eq!(state(&pool).await.0,"cancelled");assert_eq!(provider.calls.load(Ordering::SeqCst),0);
            if case==2 {sqlx::query("DELETE FROM principals WHERE id='suspended-test'").execute(pool.raw()).await.unwrap();}
            if case==3 {sqlx::query("UPDATE \"user\" SET email='a@example.test' WHERE id='fixture-a'").execute(pool.raw()).await.unwrap();}
        }
    }).await;
}

#[tokio::test]
async fn six_attempt_backoff_has_a_terminal_bound_and_no_seventh_provider_invocation() {
    database::with_database(1, |pool| async move {
        let scenario = setup(&pool).await;
        queue(&scenario, CredentialProofKind::PasswordReset).await;
        let provider = Arc::new(FakeProvider::new(
            AccountMailProviderKind::Resend,
            vec![AccountMailSendOutcome::Retryable(AccountMailFailure::ProviderRateLimited); 6],
        ));
        let worker = worker(&pool, &scenario, provider.clone(), scenario.keyring.clone());
        for attempt in 1..=6 {
            assert!(worker.deliver_one().await.unwrap());
            let row = state(&pool).await;
            assert_eq!(row.1, attempt);
            assert_eq!(row.0, if attempt < 6 { "queued" } else { "failed" });
            assert!(!worker.deliver_one().await.unwrap());
            if attempt < 6 {
                scenario.clock.advance(30_000 * (1_i64 << attempt));
            }
        }
        scenario.clock.advance(3_600_000);
        assert!(!worker.deliver_one().await.unwrap());
        assert_eq!(provider.calls.load(Ordering::SeqCst), 6);
    })
    .await;
}

#[tokio::test]
async fn unknown_smtp_is_terminal_while_resend_retry_reuses_identical_message_and_key() {
    for kind in [
        AccountMailProviderKind::Smtp,
        AccountMailProviderKind::Resend,
    ] {
        database::with_database(1, move |pool| async move {
            let scenario = setup(&pool).await;
            queue(&scenario, CredentialProofKind::PasswordReset).await;
            let provider = Arc::new(FakeProvider::new(
                kind,
                vec![AccountMailSendOutcome::Uncertain(
                    AccountMailFailure::DeliveryUncertain,
                )],
            ));
            let worker = worker(&pool, &scenario, provider.clone(), scenario.keyring.clone());
            assert!(worker.deliver_one().await.unwrap());
            assert_eq!(
                state(&pool).await.0,
                if kind == AccountMailProviderKind::Smtp {
                    "failed"
                } else {
                    "queued"
                }
            );
            scenario.clock.advance(60_000);
            assert_eq!(
                worker.deliver_one().await.unwrap(),
                kind == AccountMailProviderKind::Resend
            );
            assert_eq!(
                provider.calls.load(Ordering::SeqCst),
                if kind == AccountMailProviderKind::Smtp {
                    1
                } else {
                    2
                }
            );
        })
        .await;
    }
}

#[tokio::test]
async fn lease_recovery_distinguishes_unstarted_claim_from_uncertain_provider_intent() {
    for mode in 0..3 {
        database::with_database(1, move |pool| async move {
            let scenario = setup(&pool).await;
            queue(&scenario, CredentialProofKind::PasswordReset).await;
            let repo = PostgresAccountMail::new(pool.clone(), scenario.clock.clone());
            let job = claim(&repo, fixture::account::NOW).await;
            let kind = if mode == 1 {
                AccountMailProviderKind::Smtp
            } else {
                AccountMailProviderKind::Resend
            };
            let provider = Arc::new(FakeProvider::new(kind, vec![]));
            if mode != 0 {
                let message = content(&scenario, &job);
                assert!(
                    repo.begin_dispatch(
                        &job,
                        provider.profile(),
                        &provider.profile().fingerprint(&message).unwrap(),
                        fixture::account::NOW
                    )
                    .await
                    .unwrap()
                );
            }
            scenario.clock.advance(ACCOUNT_MAIL_LEASE_MS);
            let worker = worker(&pool, &scenario, provider.clone(), scenario.keyring.clone());
            let processed = worker.deliver_one().await.unwrap();
            assert_eq!(processed, mode != 1);
            assert_eq!(
                provider.calls.load(Ordering::SeqCst),
                if mode == 1 { 0 } else { 1 }
            );
            assert_eq!(
                state(&pool).await.0,
                if mode == 1 { "failed" } else { "sent" }
            );
            repo.finish(
                &job,
                AccountMailDispatch::Completed(AccountMailSendOutcome::Accepted {
                    provider_id: "forged-stale".into(),
                }),
                fixture::account::NOW,
            )
            .await
            .unwrap();
            assert_ne!(state(&pool).await.2.as_deref(), Some("forged-stale"));
        })
        .await;
    }
}

#[tokio::test]
async fn retained_key_rotation_reads_the_old_envelope_and_missing_keys_never_send() {
    for retain in [true, false] {
        database::with_database(1, move |pool| async move {
            let scenario = setup(&pool).await;
            queue(&scenario, CredentialProofKind::PasswordReset).await;
            let provider = Arc::new(FakeProvider::new(AccountMailProviderKind::Resend, vec![]));
            let mut keys = vec![("new-synthetic-key".into(), [9; 32])];
            if retain {
                keys.push(("synthetic-test-only".into(), [7; 32]));
            }
            let keyring =
                Arc::new(AccountMailKeyring::new("new-synthetic-key".into(), keys).unwrap());
            let worker = worker(&pool, &scenario, provider.clone(), keyring);
            assert!(worker.deliver_one().await.unwrap());
            assert_eq!(
                provider.calls.load(Ordering::SeqCst),
                if retain { 1 } else { 0 }
            );
            assert_eq!(state(&pool).await.0, if retain { "sent" } else { "failed" });
        })
        .await;
    }
}

#[tokio::test]
async fn a_cancelled_proof_keeps_cancellation_even_when_the_original_provider_receipt_arrives() {
    database::with_database(1,|pool|async move {
        let scenario=setup(&pool).await;queue(&scenario,CredentialProofKind::PasswordReset).await;let repo=PostgresAccountMail::new(pool.clone(),scenario.clock.clone());let job=claim(&repo,fixture::account::NOW).await;
        let provider=Arc::new(FakeProvider::new(AccountMailProviderKind::Resend,vec![]));let message=content(&scenario,&job);repo.begin_dispatch(&job,provider.profile(),&provider.profile().fingerprint(&message).unwrap(),fixture::account::NOW).await.unwrap();
        let result=repo.dispatch_current(&job,message,provider.clone()).await.unwrap();
        sqlx::query("UPDATE native_account_mail_outbox SET state='cancelled',lease_token=NULL,lease_until=NULL").execute(pool.raw()).await.unwrap();sqlx::query("UPDATE native_account_proofs SET consumed_at=created_at").execute(pool.raw()).await.unwrap();
        let mut forged=job.clone();forged.lease=SystemAuthenticationSecrets.issue().unwrap();repo.finish(&forged,AccountMailDispatch::Completed(AccountMailSendOutcome::Accepted{provider_id:"forged-receipt".into()}),fixture::account::NOW).await.unwrap();assert_eq!(state(&pool).await.2,None);
        repo.finish(&job,result,fixture::account::NOW).await.unwrap();let row=state(&pool).await;assert_eq!(row.0,"cancelled");assert_eq!(row.2.as_deref(),Some("synthetic-provider-receipt"));assert_eq!(provider.calls.load(Ordering::SeqCst),1);
    }).await;
}

#[tokio::test]
async fn concurrent_workers_on_one_connection_only_invoke_the_provider_once() {
    database::with_database(1, |pool| async move {
        let scenario = setup(&pool).await;
        queue(&scenario, CredentialProofKind::PasswordReset).await;
        let provider = Arc::new(FakeProvider::new(AccountMailProviderKind::Resend, vec![]));
        let worker = Arc::new(worker(
            &pool,
            &scenario,
            provider.clone(),
            scenario.keyring.clone(),
        ));
        let (a, b) = tokio::join!(worker.deliver_one(), worker.deliver_one());
        assert_ne!(a.unwrap(), b.unwrap());
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert_eq!(state(&pool).await.0, "sent");
    })
    .await;
}
struct BlockingProvider {
    profile: AccountMailProviderProfile,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
impl AccountMailProvider for BlockingProvider {
    fn profile(&self) -> &AccountMailProviderProfile {
        &self.profile
    }
    fn send<'a>(&'a self, _: &'a AccountEmailMessage) -> AccountMailSendFuture<'a> {
        Box::pin(async move {
            self.entered.notify_one();
            self.release.notified().await;
            AccountMailSendOutcome::Accepted {
                provider_id: "synthetic-retained-proof-receipt".into(),
            }
        })
    }
}
async fn wait_locked(pool: &DatabasePool, event: &str) -> bool {
    tokio::time::timeout(std::time::Duration::from_secs(4),async {loop {let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND wait_event_type='Lock' AND wait_event=$1)").bind(event).fetch_one(pool.raw()).await.unwrap();if waiting{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.is_ok()
}
#[tokio::test]
async fn actual_provider_invocation_retains_current_recipient_and_credential_source_locks() {
    database::with_database(3, |pool| async move {
        let scenario = setup(&pool).await;
        queue(&scenario, CredentialProofKind::PasswordReset).await;
        let provider = Arc::new(BlockingProvider {
            profile: AccountMailProviderProfile {
                kind: AccountMailProviderKind::Resend,
                profile_id: "synthetic-blocking".into(),
                from: "ERPX <noreply@example.test>".into(),
            },
            entered: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
        });
        let worker = worker(&pool, &scenario, provider.clone(), scenario.keyring.clone());
        let task = tokio::spawn(async move { worker.deliver_one().await });
        if tokio::time::timeout(
            std::time::Duration::from_secs(4),
            provider.entered.notified(),
        )
        .await
        .is_err()
        {
            task.abort();
            let _ = task.await;
            panic!("worker did not enter its explicit fake provider");
        }
        let writer_pool = pool.clone();
        let writer = tokio::spawn(async move {
            sqlx::query("UPDATE \"user\" SET email='changed@example.test' WHERE id='fixture-a'")
                .execute(writer_pool.raw())
                .await
                .unwrap()
        });
        let observed = wait_locked(&pool, "relation").await;
        let pending = !writer.is_finished();
        provider.release.notify_one();
        assert!(task.await.unwrap().unwrap());
        writer.await.unwrap();
        assert!(
            observed && pending,
            "recipient/credential changes must wait for the active send proof lease"
        );
        assert_eq!(state(&pool).await.0, "sent");
    })
    .await;
}
#[tokio::test]
async fn failed_ack_never_replays_smtp_and_resend_recovery_keeps_one_idempotent_delivery() {
    for kind in [
        AccountMailProviderKind::Smtp,
        AccountMailProviderKind::Resend,
    ] {
        database::with_database(1,move|pool|async move {
        let scenario=setup(&pool).await;queue(&scenario,CredentialProofKind::PasswordReset).await;let provider=Arc::new(FakeProvider::new(kind,vec![]));let worker=worker(&pool,&scenario,provider.clone(),scenario.keyring.clone());
        sqlx::raw_sql("CREATE SEQUENCE mail_ack_attempt; CREATE FUNCTION fail_mail_ack() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='accepted' AND nextval('mail_ack_attempt')=1 THEN RAISE EXCEPTION 'synthetic receipt failure' USING ERRCODE='23514'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_mail_ack BEFORE INSERT ON native_account_mail_events FOR EACH ROW EXECUTE FUNCTION fail_mail_ack()").execute(pool.raw()).await.unwrap();
        assert!(worker.deliver_one().await.is_err());assert_eq!(state(&pool).await.0,"running");assert_eq!(provider.calls.load(Ordering::SeqCst),1);scenario.clock.advance(ACCOUNT_MAIL_LEASE_MS);
        assert_eq!(worker.deliver_one().await.unwrap(),kind==AccountMailProviderKind::Resend);assert_eq!(state(&pool).await.0,if kind==AccountMailProviderKind::Resend{"sent"}else{"failed"});assert_eq!(provider.deliveries.lock().unwrap().len(),1);assert_eq!(provider.calls.load(Ordering::SeqCst),if kind==AccountMailProviderKind::Resend{2}else{1});
    }).await;
    }
}

#[tokio::test]
async fn generated_reset_mail_url_reaches_the_native_form_callback_without_consuming_the_proof() {
    database::with_database(1, |pool| async move {
        let scenario = setup(&pool).await;
        scenario
            .credentials
            .request(CredentialProofKind::PasswordReset, "a@example.test", None)
            .await
            .unwrap();
        let opened = fixture::mail(
            &pool,
            &scenario,
            CredentialProofKind::PasswordReset,
            "a@example.test",
        )
        .await;
        assert_eq!(opened.return_path, "/reset-password");
        let message = AccountMailPolicy::new(fixture::account::policy())
            .unwrap()
            .render(
                EmailAddress::new("a@example.test").unwrap(),
                &opened,
                "synthetic-reset",
            )
            .unwrap();
        let link = url::Url::parse(message.body().lines().nth(2).unwrap()).unwrap();
        let router =
            account_credentials_router(scenario.auth.clone(), scenario.credentials.clone())
                .unwrap();
        let request = || {
            Request::builder()
                .uri(format!("{}?{}", link.path(), link.query().unwrap()))
                .header("host", "account.example.test")
                .body(Body::empty())
                .unwrap()
        };
        let response = router.clone().oneshot(request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert!(response.headers().get("set-cookie").is_none());
        assert_eq!(response.headers()["referrer-policy"], "no-referrer");
        let target = url::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
        assert_eq!(
            target.origin().ascii_serialization(),
            "https://account.example.test"
        );
        assert_eq!(target.path(), "/reset-password");
        assert!(
            target
                .query_pairs()
                .any(|(key, value)| key == "token" && value == opened.token.expose())
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM native_account_proofs WHERE consumed_at IS NULL"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        scenario
            .credentials
            .reset_password(opened.token.expose().into(), fixture::NEW_PASSWORD.into())
            .await
            .unwrap();
        let response = router.clone().oneshot(request()).await.unwrap();
        let target = url::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
        assert!(
            target
                .query_pairs()
                .any(|(key, value)| key == "error" && value == "INVALID_TOKEN")
        );
        assert!(!target.query_pairs().any(|(key, _)| key == "token"));
        for (host, path, expected) in [
            (
                "unknown.example.test",
                format!("{}?callbackURL=%2Freset-password", link.path()),
                StatusCode::NOT_FOUND,
            ),
            (
                "account.example.test",
                format!("{}?callbackURL=%2F%2Fforeign.example.test", link.path()),
                StatusCode::BAD_REQUEST,
            ),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header("host", host)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
    })
    .await;
}

#[tokio::test]
async fn reset_and_verification_recheck_expiry_after_real_proof_source_lock_waits() {
    for kind in [
        CredentialProofKind::PasswordReset,
        CredentialProofKind::EmailVerification,
    ] {
        database::with_database(3,move|pool|async move {
        let scenario=setup(&pool).await;queue(&scenario,kind).await;let email=if kind==CredentialProofKind::PasswordReset{"a@example.test"}else{"unverified@example.test"};let opened=fixture::mail(&pool,&scenario,kind,email).await;scenario.clock.advance(CREDENTIAL_PROOF_LIFETIME_MS-1000);
        let mut blocker=pool.raw().begin().await.unwrap();sqlx::query("LOCK TABLE native_account_proofs IN SHARE MODE").execute(&mut *blocker).await.unwrap();let credentials=scenario.credentials.clone();let token=opened.token.expose().to_owned();
        let task=tokio::spawn(async move {match kind {CredentialProofKind::PasswordReset=>credentials.reset_password(token,fixture::NEW_PASSWORD.into()).await,CredentialProofKind::EmailVerification=>credentials.verify_email(token,&fixture::account::empty(),None).await.map(|_|())}});
        let observed=wait_locked(&pool,"relation").await;scenario.clock.advance(1000);blocker.commit().await.unwrap();assert!(task.await.unwrap().is_err());assert!(observed,"expected actual proof source lock wait");
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_account_proofs WHERE consumed_at IS NULL").fetch_one(pool.raw()).await.unwrap(),1);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_account_credential_events WHERE action IN ('password_reset','email_verified')").fetch_one(pool.raw()).await.unwrap(),0);
    }).await;
    }
}

#[tokio::test]
async fn proof_expiry_during_the_final_audit_rolls_back_reset_or_verification_effects() {
    for kind in [
        CredentialProofKind::PasswordReset,
        CredentialProofKind::EmailVerification,
    ] {
        database::with_database(3,move|pool|async move {
        let scenario=setup(&pool).await;queue(&scenario,kind).await;let email=if kind==CredentialProofKind::PasswordReset{"a@example.test"}else{"unverified@example.test"};let opened=fixture::mail(&pool,&scenario,kind,email).await;
        sqlx::raw_sql("CREATE FUNCTION pause_credential_completion() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action IN ('password_reset','email_verified') THEN PERFORM pg_advisory_xact_lock(927341,29); END IF; RETURN NEW; END $$; CREATE TRIGGER pause_credential_completion BEFORE INSERT ON native_account_credential_events FOR EACH ROW EXECUTE FUNCTION pause_credential_completion()").execute(pool.raw()).await.unwrap();
        let mut blocker=pool.raw().acquire().await.unwrap();blocker.close_on_drop();sqlx::query("SELECT pg_advisory_lock(927341,29)").execute(&mut *blocker).await.unwrap();let credentials=scenario.credentials.clone();let token=opened.token.expose().to_owned();
        let task=tokio::spawn(async move {match kind {CredentialProofKind::PasswordReset=>credentials.reset_password(token,fixture::NEW_PASSWORD.into()).await,CredentialProofKind::EmailVerification=>credentials.verify_email(token,&fixture::account::empty(),None).await.map(|_|())}});
        let observed=wait_locked(&pool,"advisory").await;scenario.clock.advance(CREDENTIAL_PROOF_LIFETIME_MS);sqlx::query("SELECT pg_advisory_unlock(927341,29)").execute(&mut *blocker).await.unwrap();assert!(task.await.unwrap().is_err());assert!(observed,"expected actual staged credential audit wait");
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_account_proofs WHERE consumed_at IS NULL").fetch_one(pool.raw()).await.unwrap(),1);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_account_credential_events WHERE action IN ('password_reset','email_verified')").fetch_one(pool.raw()).await.unwrap(),0);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM session").fetch_one(pool.raw()).await.unwrap(),0);
    }).await;
    }
}
