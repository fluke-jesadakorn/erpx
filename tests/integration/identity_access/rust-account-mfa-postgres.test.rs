#[allow(
    dead_code,
    reason = "shared actual MFA/security fixtures expose operations used by the sibling target"
)]
#[path = "../../support/rust-account-security.rs"]
mod support;
use erpx_server_identity_access::public::{account_security::*, *};
use erpx_server_shared::public::Clock;
use sqlx::Row;
use std::sync::Arc;

#[tokio::test]
async fn an_older_serializable_snapshot_cannot_keep_mfa_fresh_after_a_concurrent_disable_or_rotation()
 {
    fixture::with_system(|system|async move{
    use erpx_server_runtime::public::postgres::PgTransactionError;
    let proof=fixture::login(&system,"a@example.test",None).await;let s=services(&system);verified(&system,&s,&proof).await;
    for disable in [true,false]{
        sqlx::query("UPDATE iam_mfa SET enabled=true WHERE user_id='fixture-a'").execute(system.control.raw()).await.unwrap();
        let mut reader=system.control.raw().acquire().await.unwrap();sqlx::query("BEGIN ISOLATION LEVEL SERIALIZABLE").execute(&mut *reader).await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid() FROM iam_mfa WHERE user_id='fixture-a'").fetch_one(&mut *reader).await.unwrap();
        let mut writer=system.control.raw().acquire().await.unwrap();sqlx::query("BEGIN").execute(&mut *writer).await.unwrap();
        if disable{sqlx::query("UPDATE iam_mfa SET enabled=false WHERE user_id='fixture-a'").execute(&mut *writer).await.unwrap();}
        else{let envelope:String=sqlx::query_scalar("SELECT secret FROM iam_mfa WHERE user_id='fixture-a'").fetch_one(&mut *writer).await.unwrap();let rotated=s.keys.seal("fixture-a",&s.keys.open("fixture-a",&envelope).unwrap()).unwrap();sqlx::query("UPDATE iam_mfa SET secret=$1 WHERE user_id='fixture-a'").bind(rotated).execute(&mut *writer).await.unwrap();}
        let raw=proof.proof(&SystemAuthenticationSecrets);let keys=s.keys.clone();let now=system.scenario.clock.now().get();
        let task=tokio::spawn(async move{let result=require_fresh_mfa_in_transaction(&mut reader,&raw,&RequestAudience::Account,"development",keys,now).await;sqlx::query("ROLLBACK").execute(&mut *reader).await.unwrap();match result{Err(PgTransactionError::Persistence(error))=>error.sql_state().map(str::to_owned),_=>None}});
        tokio::time::timeout(std::time::Duration::from_secs(5),async{loop{let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND relation='iam_mfa'::regclass AND NOT granted)").bind(pid).fetch_one(system.control.raw()).await.unwrap();if waiting{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.expect("actual second backend blocks on the retained MFA source lock");
        sqlx::query("COMMIT").execute(&mut *writer).await.unwrap();drop(writer);assert_eq!(task.await.unwrap().as_deref(),Some("40001"));
    }
}).await;
}
use support::{fixture, *};

#[tokio::test]
async fn malformed_attempts_persist_and_exhaustion_resets_only_after_the_source_window() {
    fixture::with_system(|system| async move {
        let proof = fixture::login(&system, "a@example.test", None).await;
        let s = services(&system);
        s.service
            .enroll(&proof, RequestAudience::Account)
            .await
            .unwrap();
        for _ in 0..8 {
            assert_eq!(
                s.service
                    .verify(&proof, RequestAudience::Account, "１２３４５６".into())
                    .await
                    .err()
                    .unwrap()
                    .code,
                "MFA_INVALID"
            );
        }
        assert_eq!(
            s.service
                .verify(
                    &proof,
                    RequestAudience::Account,
                    code(&system, &s.keys, "fixture-a", 0).await
                )
                .await
                .err()
                .unwrap()
                .code,
            "IAM_RATE_LIMIT"
        );
        assert_eq!(enabled(&system).await, (false, -1, 0));
        let attempts: i32 = sqlx::query_scalar("SELECT max(attempts) FROM iam_throttles")
            .fetch_one(system.control.raw())
            .await
            .unwrap();
        assert_eq!(attempts, 9);
        system.scenario.clock.advance(300_000);
        assert_eq!(
            s.service
                .verify(&proof, RequestAudience::Account, "bad".into())
                .await
                .err()
                .unwrap()
                .code,
            "IAM_RATE_LIMIT"
        );
        system.scenario.clock.advance(1);
        s.service
            .verify(
                &proof,
                RequestAudience::Account,
                code(&system, &s.keys, "fixture-a", 0).await,
            )
            .await
            .unwrap();
    })
    .await;
}
#[tokio::test]
async fn one_global_counter_winner_and_exact_child_freshness_never_inherit_to_account() {
    fixture::with_system(|system| async move {
        let proof = fixture::login(&system, "a@example.test", None).await;
        let s = services(&system);
        s.service
            .enroll(&proof, RequestAudience::Account)
            .await
            .unwrap();
        let child = fixture::child(&system, &proof, fixture::ACME).await;
        let child_proof = PresentedSecrets {
            browser: None,
            active: Some(child.token),
            saved: vec![],
        };
        let audience = RequestAudience::Holding {
            audience: fixture::ACME.into(),
        };
        let c = code(&system, &s.keys, "fixture-a", 0).await;
        let (a, b) = tokio::join!(
            s.service.verify(&child_proof, audience.clone(), c.clone()),
            s.service.verify(&child_proof, audience.clone(), c)
        );
        assert_ne!(a.is_ok(), b.is_ok());
        assert_eq!(enabled(&system).await.2, 1);
        let rows = sqlx::query("SELECT session_id,user_id FROM iam_mfa_sessions")
            .fetch_all(system.control.raw())
            .await
            .unwrap();
        assert_eq!(
            rows[0].get::<String, _>("session_id"),
            child.result.session_id
        );
        let mut conn = system.control.raw().acquire().await.unwrap();
        sqlx::query("BEGIN").execute(&mut *conn).await.unwrap();
        let raw = child_proof.proof(&SystemAuthenticationSecrets);
        let fresh = require_fresh_mfa_in_transaction(
            &mut conn,
            &raw,
            &audience,
            "development",
            s.keys.clone(),
            system.scenario.clock.now().get(),
        )
        .await
        .unwrap();
        assert_eq!(fresh.session_id(), child.result.session_id);
        assert!(
            require_fresh_mfa_in_transaction(
                &mut conn,
                &proof.proof(&SystemAuthenticationSecrets),
                &RequestAudience::Account,
                "development",
                s.keys.clone(),
                system.scenario.clock.now().get()
            )
            .await
            .is_err()
        );
        fresh
            .require_current(&mut conn, system.scenario.clock.now().get())
            .await
            .unwrap();
        sqlx::query("COMMIT").execute(&mut *conn).await.unwrap();
        sqlx::query("BEGIN").execute(&mut *conn).await.unwrap();
        assert!(
            fresh
                .require_current(&mut conn, system.scenario.clock.now().get())
                .await
                .is_err()
        );
        sqlx::query("ROLLBACK").execute(&mut *conn).await.unwrap();
    })
    .await;
}
#[tokio::test]
async fn enrollment_throttle_and_unsupported_legacy_or_future_freshness_never_grant_authority() {
    fixture::with_system(|system| async move {
        let proof = fixture::login(&system, "a@example.test", None).await;
        let s = services(&system);
        for _ in 0..3 {
            s.service
                .enroll(&proof, RequestAudience::Account)
                .await
                .unwrap();
        }
        assert_eq!(
            s.service
                .enroll(&proof, RequestAudience::Account)
                .await
                .err()
                .unwrap()
                .code,
            "IAM_RATE_LIMIT"
        );
        let c = code(&system, &s.keys, "fixture-a", 0).await;
        s.service
            .verify(&proof, RequestAudience::Account, c)
            .await
            .unwrap();
        sqlx::query("UPDATE iam_mfa_sessions SET verified_at=$1")
            .bind(system.scenario.clock.now().get() + 1)
            .execute(system.control.raw())
            .await
            .unwrap();
        let mut connection = system.control.raw().acquire().await.unwrap();
        sqlx::query("BEGIN")
            .execute(&mut *connection)
            .await
            .unwrap();
        assert!(
            require_fresh_mfa_in_transaction(
                &mut connection,
                &proof.proof(&SystemAuthenticationSecrets),
                &RequestAudience::Account,
                "development",
                s.keys.clone(),
                system.scenario.clock.now().get()
            )
            .await
            .is_err()
        );
        sqlx::query("ROLLBACK")
            .execute(&mut *connection)
            .await
            .unwrap();
        drop(connection);
        sqlx::query("UPDATE iam_mfa SET secret='iv.tag.data'")
            .execute(system.control.raw())
            .await
            .unwrap();
        let before = enabled(&system).await;
        assert_eq!(
            s.service
                .verify(&proof, RequestAudience::Account, "123456".into())
                .await
                .err()
                .unwrap()
                .code,
            "ACCOUNT_MFA_KEY_UNAVAILABLE"
        );
        assert_eq!(enabled(&system).await, before);
    })
    .await;
}
#[tokio::test]
async fn enrollment_uses_original_authentication_age_not_a_freshly_issued_child() {
    fixture::with_system(|system| async move {
        let proof = fixture::login(&system, "a@example.test", None).await;
        system.scenario.clock.advance(300_001);
        let child = fixture::child(&system, &proof, fixture::ACME).await;
        let child_proof = PresentedSecrets {
            browser: None,
            active: Some(child.token),
            saved: vec![],
        };
        let s = services(&system);
        assert_eq!(
            s.service
                .enroll(
                    &child_proof,
                    RequestAudience::Holding {
                        audience: fixture::ACME.into()
                    }
                )
                .await
                .err()
                .unwrap()
                .code,
            "REAUTHENTICATION_REQUIRED"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM iam_mfa")
                .fetch_one(system.control.raw())
                .await
                .unwrap(),
            0
        );
    })
    .await;
}
#[tokio::test]
async fn successful_verification_reseals_current_key_and_late_failure_rolls_back_counter_and_freshness()
 {
    fixture::with_system(|system|async move{
    let proof=fixture::login(&system,"a@example.test",None).await;let old=services_with_keys(&system,Arc::new(AccountMfaKeyring::new("old".into(),vec![("old".into(),[8;32])]).unwrap()));old.service.enroll(&proof,RequestAudience::Account).await.unwrap();
    let current=services_with_keys(&system,Arc::new(AccountMfaKeyring::new("current".into(),vec![("old".into(),[8;32]),("current".into(),[9;32])]).unwrap()));
    sqlx::raw_sql("CREATE FUNCTION synthetic_mfa_rollback() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RAISE EXCEPTION 'synthetic rollback'; END$$; CREATE CONSTRAINT TRIGGER synthetic_mfa_rollback AFTER INSERT ON iam_mfa_sessions DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_mfa_rollback()") .execute(system.control.raw()).await.unwrap();
    let c=code(&system,&old.keys,"fixture-a",0).await;assert!(current.service.verify(&proof,RequestAudience::Account,c.clone()).await.is_err());assert_eq!(enabled(&system).await,(false,-1,0));
    sqlx::query("DROP TRIGGER synthetic_mfa_rollback ON iam_mfa_sessions").execute(system.control.raw()).await.unwrap();current.service.verify(&proof,RequestAudience::Account,c).await.unwrap();
    let envelope:String=sqlx::query_scalar("SELECT secret FROM iam_mfa WHERE user_id='fixture-a'").fetch_one(system.control.raw()).await.unwrap();assert!(envelope.starts_with("mfa1.current."));assert!(old.keys.open("fixture-a",&envelope).is_err());
}).await;
}
