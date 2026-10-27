#[allow(
    dead_code,
    reason = "shared actual Account/SSO/MFA and private profile scenarios"
)]
#[path = "../../support/rust-directory-source-admin.rs"]
mod fixture;
use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
use erpx_server_identity_access::public::{directory::*, *};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use erpx_server_shared::public::{Clock, CommandKey, RecordId};
use erpx_service::directory_source_administration::*;
use fixture::*;
use std::{sync::Arc, time::Duration};
fn denied<T>(result: Result<T, PgTransactionError<BusinessError>>, code: &str) {
    match result {
        Err(PgTransactionError::Business(error)) => assert_eq!(error.code, code),
        Err(_) => panic!("expected business denial {code}"),
        Ok(_) => panic!("expected denial {code}"),
    }
}
async fn proposed(p: &Prepared, key: &str) -> DirectorySourceReviewResult {
    p.service()
        .preview(
            HOST,
            p.proof(false),
            p.workspace(false),
            p.preview(key, None, "synthetic_provenance_v1"),
        )
        .await
        .unwrap()
}
async fn waiting(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    table: &str,
    advisory: bool,
) {
    tokio::time::timeout(Duration::from_secs(5),async{
    loop{let yes:bool=if advisory{sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE NOT granted AND locktype='advisory' AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(pool.raw()).await.unwrap()}else{sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE NOT granted AND relation=$1::regclass AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").bind(table).fetch_one(pool.raw()).await.unwrap()};if yes{break;}tokio::task::yield_now().await;}
}).await.expect("actual independent backend must wait on the owned source fence");
}

#[tokio::test]
async fn actual_account_child_mfa_and_independent_current_review_apply_one_source_audit_and_replay_without_effects()
 {
    account::with_system(|system| async move {
        let p = prepare(system, true).await;
        let service = p.service();
        let sessions: i64 = sqlx::query_scalar("SELECT count(*)FROM session")
            .fetch_one(p.system.control.raw())
            .await
            .unwrap();
        let proposal = proposed(&p, "native-source-preview-one").await;
        assert_eq!(proposal.status, "pending");
        assert_eq!(counts(&p).await, (0, 1, 1, 1));
        let first = service
            .approve_and_apply(
                HOST,
                p.proof(true),
                p.workspace(true),
                p.decision(&proposal, "native-source-apply-one"),
            )
            .await
            .unwrap();
        assert_eq!(first.status, "approved");
        assert_eq!(first.version, 2);
        assert_eq!(first.source_revision, 1);
        assert_eq!(counts(&p).await, (1, 1, 2, 3));
        let replay = service
            .approve_and_apply(
                HOST,
                p.proof(true),
                p.workspace(true),
                p.decision(&proposal, "native-source-apply-one"),
            )
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(replay).unwrap(),
            serde_json::to_value(first).unwrap()
        );
        assert_eq!(counts(&p).await, (1, 1, 2, 3));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*)FROM session")
                .fetch_one(p.system.control.raw())
                .await
                .unwrap(),
            sessions
        );
        let payload: String = sqlx::query_scalar("SELECT payload::text FROM iam_reviews")
            .fetch_one(p.system.holdings[0].raw())
            .await
            .unwrap();
        assert!(!payload.contains(ldap::SERVICE_PASSWORD));
        assert!(!payload.contains("bindPassword"));
        assert!(payload.contains("profileTag"));
        assert_eq!(p.server.stats.lock().unwrap().connections, 0);
        p.system.scenario.clock.advance(300000);
        denied(
            service
                .approve_and_apply(
                    HOST,
                    p.proof(true),
                    p.workspace(true),
                    p.decision(&proposal, "native-source-apply-one"),
                )
                .await,
            "FRESH_MFA_REQUIRED",
        );
        assert_eq!(counts(&p).await, (1, 1, 2, 3));
        p.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn proposer_cannot_approve_own_review_and_unrelated_idempotency_input_cannot_replace_it() {
    account::with_system(|system| async move {
        let p = prepare(system, true).await;
        let proposal = proposed(&p, "native-independent-source-preview").await;
        denied(
            p.service()
                .approve_and_apply(
                    HOST,
                    p.proof(false),
                    p.workspace(false),
                    p.decision(&proposal, "native-self-source-apply"),
                )
                .await,
            "INDEPENDENT_REVIEW_REQUIRED",
        );
        assert_eq!(counts(&p).await, (0, 1, 1, 1));
        let mut different = p.preview(
            "native-independent-source-preview",
            None,
            "synthetic_provenance_v1",
        );
        different.reason = "Different requested source intent".into();
        denied(
            p.service()
                .preview(HOST, p.proof(false), p.workspace(false), different)
                .await,
            "IDEMPOTENCY_CONFLICT",
        );
        assert_eq!(counts(&p).await, (0, 1, 1, 1));
        p.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn a_verified_parent_never_supplies_its_child_mfa_and_missing_child_proof_creates_no_decision()
 {
    account::with_system(|system| async move {
        let p = prepare(system, false).await;
        let proposal = proposed(&p, "native-child-mfa-preview").await;
        // Give the actual parent its own fresh counter; the child remains unverified.
        p.system.scenario.clock.advance(30000);
        let s = security::services_with_keys(&p.system, p.keys.clone());
        let code = security::code(&p.system, &p.keys, "fixture-b", 0).await;
        s.service
            .verify(&p.central_b, RequestAudience::Account, code)
            .await
            .unwrap();
        denied(
            p.service()
                .approve_and_apply(
                    HOST,
                    p.proof(true),
                    p.workspace(true),
                    p.decision(&proposal, "native-child-mfa-apply"),
                )
                .await,
            "FRESH_MFA_REQUIRED",
        );
        assert_eq!(counts(&p).await, (0, 1, 1, 1));
        p.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn withdrawing_actual_published_proposer_permission_invalidates_the_saved_proposal() {
    account::with_system(|system|async move{
    let p=prepare(system,true).await;let proposal=proposed(&p,"native-source-policy-preview").await;
    sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_source_admin',2,'[\"iam.review\"]','[]','[\"holding\"]','synthetic-fixture',1);UPDATE iam_roles SET published_revision=2 WHERE id='custom_source_admin'").execute(p.system.holdings[0].raw()).await.unwrap();
    denied(p.service().approve_and_apply(HOST,p.proof(true),p.workspace(true),p.decision(&proposal,"native-source-policy-apply")).await,"PERMISSION_REQUIRED");assert_eq!(counts(&p).await,(0,1,1,1));p.server.finish().await;
 }).await;
}
#[tokio::test]
async fn a_changed_private_profile_cannot_be_applied_under_the_old_exact_reviewed_tag() {
    account::with_system(|system| async move {
        let p = prepare(system, true).await;
        let proposal = proposed(&p, "native-source-profile-preview").await;
        ldap::write_profile(
            p.profile.directory.path(),
            Some(&ldap::material().ca),
            "ldap.fixture.test",
            true,
        );
        p.profile.profiles.reload().await.unwrap();
        denied(
            p.service()
                .approve_and_apply(
                    HOST,
                    p.proof(true),
                    p.workspace(true),
                    p.decision(&proposal, "native-source-profile-apply"),
                )
                .await,
            "DIRECTORY_SOURCE_REVIEW_CHANGED",
        );
        assert_eq!(counts(&p).await, (0, 1, 1, 1));
        assert_eq!(p.server.stats.lock().unwrap().connections, 0);
        p.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn actual_parent_retirement_fences_the_proposer_and_the_other_account_cannot_apply_stale_authority()
 {
    account::with_system(|system|async move{
    let p=prepare(system,true).await;let proposal=proposed(&p,"native-source-retirement-preview").await;
    p.system.scenario.auth.sign_out(&p.central_a,RequestAudience::Account).await.unwrap();
    denied(p.service().approve_and_apply(HOST,p.proof(true),p.workspace(true),p.decision(&proposal,"native-source-retirement-apply")).await,"SESSION_REVOKED");
    assert!(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM iam_revoked_sessions WHERE session_id=$1 AND user_id='fixture-a')AND NOT EXISTS(SELECT 1 FROM active_work_contexts WHERE session_id=$1)").bind(&p.id_a).fetch_one(p.system.holdings[0].raw()).await.unwrap());assert_eq!(counts(&p).await,(0,1,1,1));p.server.finish().await;
 }).await;
}
#[tokio::test]
async fn source_wait_resamples_exact_session_mfa_and_rolls_back_source_review_receipt_and_audit() {
    account::with_system(|system|async move{
    let p=prepare(system,true).await;let proposal=proposed(&p,"native-source-expiry-preview").await;let pool=&p.system.holdings[0];
    sqlx::raw_sql("CREATE FUNCTION synthetic_admin_wait()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN PERFORM pg_advisory_xact_lock(197431);RETURN NEW;END$$;CREATE TRIGGER synthetic_admin_wait BEFORE INSERT ON iam_directory_source_selections FOR EACH ROW EXECUTE FUNCTION synthetic_admin_wait()").execute(pool.raw()).await.unwrap();
    let mut blocker=pool.raw().begin().await.unwrap();sqlx::query("SELECT pg_advisory_xact_lock(197431)").execute(&mut*blocker).await.unwrap();
    let service=p.service();let proof=p.proof(true);let workspace=p.workspace(true);let input=p.decision(&proposal,"native-source-expiry-apply");let task=tokio::spawn(async move{service.approve_and_apply(HOST,proof,workspace,input).await});
    waiting(pool,"",true).await;p.system.scenario.clock.advance(300000);blocker.commit().await.unwrap();
    denied(task.await.unwrap(),"DIRECTORY_SOURCE_REVIEW_CHANGED");assert_eq!(counts(&p).await,(0,1,1,1));p.server.finish().await;
 }).await;
}
#[tokio::test]
async fn exact_control_session_and_holding_source_are_retained_until_approval_commit_before_actual_revocation()
 {
    account::with_system(|system|async move{
    let p=prepare(system,true).await;let proposal=proposed(&p,"native-source-control-race-preview").await;let pool=&p.system.holdings[0];
    sqlx::raw_sql("CREATE FUNCTION synthetic_admin_retention()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN PERFORM pg_advisory_xact_lock(197432);RETURN NEW;END$$;CREATE TRIGGER synthetic_admin_retention BEFORE INSERT ON iam_directory_source_selections FOR EACH ROW EXECUTE FUNCTION synthetic_admin_retention()").execute(pool.raw()).await.unwrap();
    let mut blocker=pool.raw().begin().await.unwrap();sqlx::query("SELECT pg_advisory_xact_lock(197432)").execute(&mut*blocker).await.unwrap();
    let service=p.service();let proof=p.proof(true);let workspace=p.workspace(true);let input=p.decision(&proposal,"native-source-control-race-apply");let apply=tokio::spawn(async move{service.approve_and_apply(HOST,proof,workspace,input).await});waiting(pool,"",true).await;
    let auth=p.system.scenario.auth.clone();let central=PresentedSecrets{browser:p.central_b.browser.clone(),active:p.central_b.active.clone(),saved:p.central_b.saved.clone()};let retire=tokio::spawn(async move{auth.sign_out(&central,RequestAudience::Account).await});waiting(&p.system.control,"session",false).await;
    blocker.commit().await.unwrap();assert_eq!(apply.await.unwrap().unwrap().source_revision,1);retire.await.unwrap().unwrap();
    denied(p.service().approve_and_apply(HOST,p.proof(true),p.workspace(true),p.decision(&proposal,"native-source-control-race-apply")).await,"UNAUTHENTICATED");assert_eq!(counts(&p).await,(1,1,2,3));p.server.finish().await;
 }).await;
}

#[tokio::test]
async fn explicit_independent_key_rotation_and_process_restart_never_auto_approve_local_key_material()
 {
    account::with_system(|system| async move {
        let p = prepare(system, true).await;
        let proposal = proposed(&p, "native-source-key-initial-preview").await;
        p.service()
            .approve_and_apply(
                HOST,
                p.proof(true),
                p.workspace(true),
                p.decision(&proposal, "native-source-key-initial-apply"),
            )
            .await
            .unwrap();
        let keys = Arc::new(
            provenance::ProvenanceKeyRing::from_keys(
                "development".into(),
                vec![
                    ("synthetic_provenance_v1".into(), [0x31; 32]),
                    ("synthetic_provenance_v2".into(), [0x52; 32]),
                ],
            )
            .unwrap(),
        );
        let profiles = Arc::new(
            DirectoryProfileStore::load(
                p.profile.directory.path().join("profiles.json"),
                keys.clone(),
            )
            .await
            .unwrap(),
        );
        let next = rebuilt(&p, profiles);
        let service = DirectorySourceAdministration::new(
            next,
            p.authorizer.clone(),
            p.keys.clone(),
            p.system.scenario.clock.clone(),
        );
        assert_eq!(counts(&p).await, (1, 1, 2, 3));
        let rotation = service
            .preview(
                HOST,
                p.proof(false),
                p.workspace(false),
                p.preview(
                    "native-source-key-rotation-preview",
                    Some(1),
                    "synthetic_provenance_v2",
                ),
            )
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT revision FROM iam_directory_source_selections")
                .fetch_one(p.system.holdings[0].raw())
                .await
                .unwrap(),
            1
        );
        service
            .approve_and_apply(
                HOST,
                p.proof(true),
                p.workspace(true),
                p.decision(&rotation, "native-source-key-rotation-apply"),
            )
            .await
            .unwrap();
        let restarted = rebuilt(
            &p,
            Arc::new(
                DirectoryProfileStore::load(p.profile.directory.path().join("profiles.json"), keys)
                    .await
                    .unwrap(),
            ),
        );
        let restart = DirectorySourceAdministration::new(
            restarted,
            p.authorizer.clone(),
            p.keys.clone(),
            p.system.scenario.clock.clone(),
        );
        assert_eq!(
            restart
                .approve_and_apply(
                    HOST,
                    p.proof(true),
                    p.workspace(true),
                    p.decision(&rotation, "native-source-key-rotation-apply")
                )
                .await
                .unwrap()
                .source_revision,
            2
        );
        assert_eq!(counts(&p).await, (1, 2, 4, 6));
        denied(
            p.service()
                .preview(
                    HOST,
                    p.proof(false),
                    p.workspace(false),
                    p.preview(
                        "native-source-key-missing-preview",
                        Some(2),
                        "synthetic_provenance_v2",
                    ),
                )
                .await,
            "DIRECTORY_SOURCE_CONFIGURATION",
        );
        assert_eq!(counts(&p).await, (1, 2, 4, 6));
        assert_eq!(p.server.stats.lock().unwrap().connections, 0);
        p.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn a_deferred_approval_receipt_trigger_cannot_rewrite_the_reviewed_source_binding_and_commit_any_effect()
 {
    account::with_system(|system|async move{
    let p=prepare(system,true).await;let proposal=proposed(&p,"native-source-tamper-preview").await;
    sqlx::raw_sql("CREATE FUNCTION synthetic_source_review_tamper()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.result->>'status'='approved' THEN UPDATE iam_reviews SET payload=jsonb_set(payload,'{binding,keyId}','\"forged-key\"'::jsonb) WHERE id=NEW.result->>'id';END IF;RETURN NEW;END$$;CREATE CONSTRAINT TRIGGER synthetic_source_review_tamper AFTER INSERT ON iam_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_source_review_tamper()").execute(p.system.holdings[0].raw()).await.unwrap();
    denied(p.service().approve_and_apply(HOST,p.proof(true),p.workspace(true),p.decision(&proposal,"native-source-tamper-apply")).await,"DIRECTORY_SOURCE_REVIEW_CHANGED");assert_eq!(counts(&p).await,(0,1,1,1));p.server.finish().await;
 }).await;
}

#[tokio::test]
async fn a_normal_iam_context_cannot_call_the_review_store_without_the_same_control_mfa_route_and_holding_witness()
 {
    account::with_system(|system| async move {
        let p = prepare(system, true).await;
        let input = p.preview(
            "native-source-no-admin-proof",
            None,
            "synthetic_provenance_v1",
        );
        let change = p
            .directory
            .prepare_source_change(input.request)
            .await
            .unwrap();
        let clock = p.system.scenario.clock.clone();
        let result = p
            .authorizer
            .execute_with_sources(
                HOST,
                p.proof(false),
                p.workspace(false),
                WorkAuthorizationRequest {
                    permission: "iam.directory.manage".into(),
                    module: None,
                    destination: "iam/connections".into(),
                    api: true,
                },
                DirectorySourceReviewPreparation,
                move |c, current| {
                    let change = change.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        DirectorySourceReviewStore::propose(
                            c,
                            current,
                            &change,
                            &CommandKey::new("native-no-witness-key")?,
                            "Actual generic context must not stand in for MFA proof",
                            clock.now().get(),
                        )
                        .await
                    })
                },
            )
            .await;
        denied(result, "DIRECTORY_SOURCE_ADMIN_PROOF_REQUIRED");
        assert_eq!(counts(&p).await, (0, 0, 0, 0));
        p.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn replay_cannot_replace_the_original_review_connection_with_another_current_same_company_connection()
 {
    account::with_system(|system|async move{
    let p=prepare(system,true).await;let proposal=proposed(&p,"native-source-replay-target-preview").await;let decision=p.decision(&proposal,"native-source-replay-target-apply");p.service().approve_and_apply(HOST,p.proof(true),p.workspace(true),decision.clone()).await.unwrap();
    sqlx::query("INSERT INTO iam_connection_routes(connection_id,holding_id,company_id)VALUES('second-current-connection','h001','company')").execute(p.system.control.raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_connections(id,company_id,name,provider,config,status,version,tested_version,created_by,created_at)VALUES('second-current-connection','company','Another actual connection','univention',$1,'active',1,1,'fixture-a',0)").bind(serde_json::to_value(&p.profile.descriptor.config).unwrap()).execute(p.system.holdings[0].raw()).await.unwrap();
    let mut replaced=decision;replaced.connection_id=RecordId::new("second-current-connection").unwrap();
    denied(p.service().approve_and_apply(HOST,p.proof(true),p.workspace(true),replaced).await,"DIRECTORY_SOURCE_ADMIN_SCOPE_REQUIRED");assert_eq!(counts(&p).await,(1,1,2,3));assert_eq!(p.server.stats.lock().unwrap().connections,0);p.server.finish().await;
 }).await;
}
