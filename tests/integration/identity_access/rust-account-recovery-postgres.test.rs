#[path = "../../support/rust-postgres.rs"]
mod database;
#[path = "../../support/rust-account-credentials.rs"]
mod fixture;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_identity_access::public::*;
use std::sync::Arc;
use tower::ServiceExt;

async fn post(
    router: axum::Router,
    path: &str,
    body: serde_json::Value,
) -> axum::response::Response {
    router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("host", "account.example.test")
                .header("origin", "https://account.example.test")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
}
#[tokio::test]
async fn recovery_and_verification_requests_have_identical_public_responses_for_unknown_or_ineligible_accounts()
 {
    database::with_database(2, |pool| async move {
        let scenario =
            fixture::setup(&pool, Arc::new(UnavailableInvitationCredentialAuthority)).await;
        let router =
            account_credentials_router(scenario.auth.clone(), scenario.credentials.clone())
                .unwrap();
        for path in [
            "/api/auth/request-password-reset",
            "/api/auth/send-verification-email",
        ] {
            let mut bodies = vec![];
            for email in [
                "a@example.test",
                "missing@example.test",
                "unverified@example.test",
            ] {
                let response = post(router.clone(), path, serde_json::json!({"email":email})).await;
                assert_eq!(response.status(), StatusCode::OK);
                assert!(!response.headers().contains_key("set-cookie"));
                bodies.push(to_bytes(response.into_body(), 8192).await.unwrap());
            }
            assert!(bodies.iter().all(|body| body == &bodies[0]));
            assert_eq!(bodies[0], r#"{"status":true}"#);
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM native_account_proofs WHERE email='missing@example.test'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM native_account_mail_outbox")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            3
        );
    })
    .await;
}
#[tokio::test]
async fn verification_consumes_once_and_issues_a_current_device_bound_account_session() {
    database::with_database(2,|pool|async move{
  let scenario=fixture::setup(&pool,Arc::new(UnavailableInvitationCredentialAuthority)).await;assert!(scenario.auth.sign_in("unverified@example.test",fixture::PASSWORD.into(),&fixture::account::empty(),None).await.is_err());
  scenario.credentials.request(CredentialProofKind::EmailVerification,"unverified@example.test",Some("/onboarding")).await.unwrap();let mail=fixture::mail(&pool,&scenario,CredentialProofKind::EmailVerification,"unverified@example.test").await;let token=mail.token.expose().to_owned();let signed=scenario.credentials.verify_email(token.clone(),&fixture::account::empty(),None).await.unwrap();assert!(signed.result.session.user.email_verified);let proof=fixture::remember(signed);assert!(scenario.auth.session(&proof,RequestAudience::Account).await.unwrap().is_some());assert!(scenario.credentials.verify_email(token,&proof,None).await.is_err());
  assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM session").fetch_one(pool.raw()).await.unwrap(),1);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_account_credential_events WHERE action='email_verified'").fetch_one(pool.raw()).await.unwrap(),1);assert_eq!(sqlx::query_scalar::<_,String>("SELECT state FROM native_account_mail_outbox").fetch_one(pool.raw()).await.unwrap(),"cancelled");
 }).await;
}
#[tokio::test]
async fn password_reset_revokes_all_target_sessions_and_cannot_replay_or_consume_a_verification_proof()
 {
    database::with_database(2,|pool|async move{
  let scenario=fixture::setup(&pool,Arc::new(UnavailableInvitationCredentialAuthority)).await;let a=fixture::remember(scenario.auth.sign_in("a@example.test",fixture::PASSWORD.into(),&fixture::account::empty(),None).await.unwrap());let b=fixture::remember(scenario.auth.sign_in("b@example.test",fixture::PASSWORD.into(),&fixture::account::empty(),None).await.unwrap());
  scenario.credentials.request(CredentialProofKind::PasswordReset,"a@example.test",Some("/reset-password")).await.unwrap();let mail=fixture::mail(&pool,&scenario,CredentialProofKind::PasswordReset,"a@example.test").await;let token=mail.token.expose().to_owned();assert!(scenario.credentials.verify_email(token.clone(),&a,None).await.is_err());scenario.credentials.reset_password(token.clone(),fixture::NEW_PASSWORD.into()).await.unwrap();assert!(scenario.credentials.reset_password(token,fixture::PASSWORD.into()).await.is_err());assert!(scenario.auth.session(&a,RequestAudience::Account).await.unwrap().is_none());assert!(scenario.auth.session(&b,RequestAudience::Account).await.unwrap().is_some());
  assert!(scenario.auth.sign_in("a@example.test",fixture::PASSWORD.into(),&fixture::account::empty(),None).await.is_err());assert!(scenario.auth.sign_in("a@example.test",fixture::NEW_PASSWORD.into(),&fixture::account::empty(),None).await.is_ok());assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_account_credential_events WHERE action='password_reset'").fetch_one(pool.raw()).await.unwrap(),1);
 }).await;
}
#[tokio::test]
async fn supersession_expiry_and_principal_changes_fail_closed_before_password_mutation() {
    database::with_database(2,|pool|async move{
  let scenario=fixture::setup(&pool,Arc::new(UnavailableInvitationCredentialAuthority)).await;scenario.credentials.request(CredentialProofKind::PasswordReset,"a@example.test",None).await.unwrap();let first=fixture::mail(&pool,&scenario,CredentialProofKind::PasswordReset,"a@example.test").await;scenario.clock.advance(60_001);scenario.credentials.request(CredentialProofKind::PasswordReset,"a@example.test",None).await.unwrap();let second=fixture::mail(&pool,&scenario,CredentialProofKind::PasswordReset,"a@example.test").await;assert_ne!(first.token.expose(),second.token.expose());assert!(scenario.credentials.reset_password(first.token.expose().into(),fixture::NEW_PASSWORD.into()).await.is_err());
  sqlx::query("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES('suspended-a','fixture-a','Synthetic A','suspended',$1)").bind(fixture::account::NOW).execute(pool.raw()).await.unwrap();assert!(scenario.credentials.reset_password(second.token.expose().into(),fixture::NEW_PASSWORD.into()).await.is_err());sqlx::query("UPDATE principals SET status='active'").execute(pool.raw()).await.unwrap();scenario.clock.advance(3_600_000);assert!(scenario.credentials.reset_password(second.token.expose().into(),fixture::NEW_PASSWORD.into()).await.is_err());assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_account_credential_events WHERE action='password_reset'").fetch_one(pool.raw()).await.unwrap(),0);
 }).await;
}
#[tokio::test]
async fn a_failed_audit_insert_rolls_back_password_revocation_and_proof_consumption() {
    database::with_database(2,|pool|async move{
  let scenario=fixture::setup(&pool,Arc::new(UnavailableInvitationCredentialAuthority)).await;let account=fixture::remember(scenario.auth.sign_in("a@example.test",fixture::PASSWORD.into(),&fixture::account::empty(),None).await.unwrap());scenario.credentials.request(CredentialProofKind::PasswordReset,"a@example.test",None).await.unwrap();let mail=fixture::mail(&pool,&scenario,CredentialProofKind::PasswordReset,"a@example.test").await;
  sqlx::raw_sql("CREATE FUNCTION reject_reset_event() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='password_reset' THEN RAISE EXCEPTION 'synthetic reset audit unavailable'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_reset_event BEFORE INSERT ON native_account_credential_events FOR EACH ROW EXECUTE FUNCTION reject_reset_event();").execute(pool.raw()).await.unwrap();assert!(scenario.credentials.reset_password(mail.token.expose().into(),fixture::NEW_PASSWORD.into()).await.is_err());assert!(scenario.auth.session(&account,RequestAudience::Account).await.unwrap().is_some());assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_account_proofs WHERE consumed_at IS NULL").fetch_one(pool.raw()).await.unwrap(),1);
  sqlx::raw_sql("DROP TRIGGER reject_reset_event ON native_account_credential_events").execute(pool.raw()).await.unwrap();scenario.credentials.reset_password(mail.token.expose().into(),fixture::NEW_PASSWORD.into()).await.unwrap();assert!(scenario.auth.session(&account,RequestAudience::Account).await.unwrap().is_none());
 }).await;
}
#[tokio::test]
async fn current_password_change_rechecks_the_account_and_replaces_all_old_session_authority() {
    database::with_database(2, |pool| async move {
        let scenario =
            fixture::setup(&pool, Arc::new(UnavailableInvitationCredentialAuthority)).await;
        let proof = fixture::remember(
            scenario
                .auth
                .sign_in(
                    "a@example.test",
                    fixture::PASSWORD.into(),
                    &fixture::account::empty(),
                    None,
                )
                .await
                .unwrap(),
        );
        assert!(
            scenario
                .credentials
                .change_password(
                    &proof,
                    "wrong-synthetic-password".into(),
                    fixture::NEW_PASSWORD.into()
                )
                .await
                .is_err()
        );
        assert!(
            scenario
                .auth
                .session(&proof, RequestAudience::Account)
                .await
                .unwrap()
                .is_some()
        );
        scenario
            .credentials
            .change_password(
                &proof,
                fixture::PASSWORD.into(),
                fixture::NEW_PASSWORD.into(),
            )
            .await
            .unwrap();
        assert!(
            scenario
                .auth
                .session(&proof, RequestAudience::Account)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            scenario
                .credentials
                .change_password(
                    &proof,
                    fixture::NEW_PASSWORD.into(),
                    fixture::PASSWORD.into()
                )
                .await
                .is_err()
        );
        assert!(
            scenario
                .auth
                .sign_in(
                    "a@example.test",
                    fixture::NEW_PASSWORD.into(),
                    &fixture::account::empty(),
                    None
                )
                .await
                .is_ok()
        );
    })
    .await;
}
#[tokio::test]
async fn verification_link_has_exact_host_safe_redirect_private_cookies_and_no_token_referrer() {
    database::with_database(2, |pool| async move {
        let scenario =
            fixture::setup(&pool, Arc::new(UnavailableInvitationCredentialAuthority)).await;
        scenario
            .credentials
            .request(
                CredentialProofKind::EmailVerification,
                "unverified@example.test",
                Some("/onboarding"),
            )
            .await
            .unwrap();
        let mail = fixture::mail(
            &pool,
            &scenario,
            CredentialProofKind::EmailVerification,
            "unverified@example.test",
        )
        .await;
        let router =
            account_credentials_router(scenario.auth.clone(), scenario.credentials.clone())
                .unwrap();
        let path = format!(
            "/api/auth/verify-email?token={}&callbackURL=%2Fonboarding&revision=0e0",
            mail.token.expose()
        );
        let denied = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&path)
                    .header("host", "acme.example.test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::NOT_FOUND);
        let response = router
            .oneshot(
                Request::builder()
                    .uri(&path)
                    .header("host", "account.example.test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()["location"], "/onboarding");
        assert_eq!(response.headers()["referrer-policy"], "no-referrer");
        assert_eq!(response.headers().get_all("set-cookie").iter().count(), 3);
        assert!(
            to_bytes(response.into_body(), 8192)
                .await
                .unwrap()
                .is_empty()
        );
    })
    .await;
}
#[tokio::test]
async fn signup_without_a_current_invitation_owner_authority_creates_no_account_or_outbox() {
    database::with_database(2,|pool|async move{
  let scenario=fixture::setup(&pool,Arc::new(UnavailableInvitationCredentialAuthority)).await;
fixture::account::assert_error(scenario.credentials.sign_up("New Staff".into(),"new@example.test",fixture::PASSWORD.into(),None,None).await,"INVITATION_REQUIRED");let router=account_credentials_router(scenario.auth.clone(),scenario.credentials.clone()).unwrap();let response=post(router,"/api/auth/sign-up/email",serde_json::json!({"name":"New Synthetic Staff","email":"new@example.test","password":fixture::PASSWORD})).await;assert_eq!(response.status(),StatusCode::FORBIDDEN);
  let token=SystemAuthenticationSecrets.issue().unwrap();let digest=SystemAuthenticationSecrets.digest(SecretPurpose::Invitation,&token);sqlx::query("INSERT INTO account_invitations(id,token_hash,holding_id,company_id,person_id,email,name,role,scope_kind,created_by,assignment_id,expires_at,created_at) VALUES('invitation-new',$1,'h001','company','person','new@example.test','New Staff','{}','holding','admin','grant',$2,$3)").bind(digest.as_str()).bind(fixture::account::NOW+3600000).bind(fixture::account::NOW).execute(pool.raw()).await.unwrap();assert!(scenario.credentials.sign_up("New Staff".into(),"new@example.test",fixture::PASSWORD.into(),Some(token.expose().into()),None).await.is_err());assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM \"user\" WHERE email='new@example.test'").fetch_one(pool.raw()).await.unwrap(),0);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_account_mail_outbox").fetch_one(pool.raw()).await.unwrap(),0);
 }).await;
}

// Explicit repository-test authority. The actual IAM/People adapter remains unmounted and pending.
struct ReviewedFixtureAuthority;
struct ReviewedFixtureLease(InvitationCredentialFacts);
impl InvitationCredentialLease for ReviewedFixtureLease {
    fn facts(&self) -> &InvitationCredentialFacts {
        &self.0
    }
}
impl InvitationCredentialAuthority for ReviewedFixtureAuthority {
    fn inspect<'a>(
        &'a self,
        query: &'a InvitationCredentialQuery,
    ) -> AccountFuture<'a, Box<dyn InvitationCredentialLease>> {
        Box::pin(async move {
            assert_eq!(query.holding_id, "h001");
            assert_eq!(query.company_id, "company");
            assert_eq!(query.person_id, "person");
            Ok(Box::new(ReviewedFixtureLease(InvitationCredentialFacts {
                invitation_id: query.invitation_id.clone(),
                holding_id: query.holding_id.clone(),
                company_id: query.company_id.clone(),
                person_id: query.person_id.clone(),
                email: query.email.clone(),
                expected_email: EmailAddress::new("new@example.test").unwrap(),
                now: query.now,
                expires_at: query.now + 3_600_000,
                pending: true,
                independently_reviewed: true,
                published_role_current: true,
                person_active: true,
                person_unlinked: true,
                authorizer_current: true,
                authorizer_can_manage: true,
                scope_within_parent: true,
                grant_expires_at: query.now + 86_400_000,
                directory_only: false,
                existing_user: false,
            })) as Box<dyn InvitationCredentialLease>)
        })
    }
}
#[tokio::test]
async fn reviewed_fixture_signup_atomically_creates_fresh_credentials_and_mail_without_granting_a_holding_role()
 {
    database::with_database(2,|pool|async move {
  let scenario=fixture::setup(&pool,Arc::new(ReviewedFixtureAuthority)).await;let token=SystemAuthenticationSecrets.issue().unwrap();let digest=SystemAuthenticationSecrets.digest(SecretPurpose::Invitation,&token);
  sqlx::query("INSERT INTO account_invitations(id,token_hash,holding_id,company_id,person_id,email,name,role,scope_kind,created_by,assignment_id,expires_at,created_at) VALUES('reviewed-invitation',$1,'h001','company','person','new@example.test','Reviewed Staff','{}','holding','admin','grant',$2,$3)").bind(digest.as_str()).bind(fixture::account::NOW+3_600_000).bind(fixture::account::NOW).execute(pool.raw()).await.unwrap();
  scenario.credentials.sign_up("Submitted display name".into(),"NEW@example.test",fixture::PASSWORD.into(),Some(token.expose().into()),Some("/onboarding")).await.unwrap();
  let row=sqlx::query("SELECT u.name,u.\"emailVerified\",a.password,a.issuer FROM \"user\" u JOIN account a ON a.\"userId\"=u.id WHERE u.email='new@example.test'").fetch_one(pool.raw()).await.unwrap();use sqlx::Row;assert_eq!(row.get::<String,_>("name"),"Reviewed Staff");assert_eq!(row.get::<i64,_>("emailVerified"),0);assert_eq!(row.get::<String,_>("issuer"),"local:credential");assert!(row.get::<String,_>("password").starts_with("$argon2id$v=19$m=65536,t=3,p=1$"));
  assert!(scenario.credentials.sign_up("Submitted display name".into(),"new@example.test",fixture::NEW_PASSWORD.into(),Some(token.expose().into()),None).await.is_err());
  assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_account_mail_outbox WHERE user_id='usr_invite_reviewed-invitation'").fetch_one(pool.raw()).await.unwrap(),1);
  let accepted:Option<i64>=sqlx::query_scalar("SELECT accepted_at FROM account_invitations WHERE id='reviewed-invitation'").fetch_one(pool.raw()).await.unwrap();assert!(accepted.is_none());assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM session WHERE \"userId\"='usr_invite_reviewed-invitation'").fetch_one(pool.raw()).await.unwrap(),0);
  let mail=fixture::mail(&pool,&scenario,CredentialProofKind::EmailVerification,"new@example.test").await;let verified=scenario.credentials.verify_email(mail.token.expose().into(),&fixture::account::empty(),None).await.unwrap();assert_eq!(verified.result.session.user.id,"usr_invite_reviewed-invitation");
 }).await;
}

#[tokio::test]
async fn stale_device_revision_does_not_consume_the_email_proof_or_verify_the_user() {
    database::with_database(2, |pool| async move {
        let scenario =
            fixture::setup(&pool, Arc::new(UnavailableInvitationCredentialAuthority)).await;
        let proof = fixture::remember(
            scenario
                .auth
                .sign_in(
                    "a@example.test",
                    fixture::PASSWORD.into(),
                    &fixture::account::empty(),
                    None,
                )
                .await
                .unwrap(),
        );
        scenario
            .credentials
            .request(
                CredentialProofKind::EmailVerification,
                "unverified@example.test",
                None,
            )
            .await
            .unwrap();
        let mail = fixture::mail(
            &pool,
            &scenario,
            CredentialProofKind::EmailVerification,
            "unverified@example.test",
        )
        .await;
        fixture::account::assert_error(
            scenario
                .credentials
                .verify_email(mail.token.expose().into(), &proof, Some(0))
                .await,
            "DEVICE_ACCOUNTS_CHANGED",
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT \"emailVerified\" FROM \"user\" WHERE id='fixture-unverified'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
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
        assert_eq!(scenario.auth.devices(&proof).await.unwrap().revision, 1);
        let verified = scenario
            .credentials
            .verify_email(mail.token.expose().into(), &proof, Some(1))
            .await
            .unwrap();
        assert_eq!(verified.result.devices.revision, 2);
        assert_eq!(verified.result.devices.accounts.len(), 2);
    })
    .await;
}
