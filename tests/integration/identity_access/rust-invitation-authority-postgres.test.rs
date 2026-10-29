#[path = "../../support/rust-postgres.rs"]
mod database;
#[path = "../../support/rust-account-invitation.rs"]
mod fixture;
use erpx_server_runtime::public::postgres::DatabasePool;
use fixture::{effects, setup, signup};
use std::sync::Arc;

#[tokio::test]
async fn real_reviewed_invitation_uses_original_authority_and_creates_only_pending_credential_effects()
 {
    database::with_database(1, |control| async move { database::with_database(1, |holding| async move {
        let (scenario, token) = setup(&control, &holding).await;
        signup(&scenario, &token).await.unwrap();
        assert_eq!(effects(&control).await, 5); // user, claim, mail, signup audit and mail audit
        let row: (String, String, Option<i64>) = sqlx::query_as("SELECT status,accepted_user_id,accepted_at FROM account_invitations WHERE id='invitation'").fetch_one(control.raw()).await.unwrap();
        assert_eq!(row, ("accepted".into(), "usr_invite_invitation".into(), None));
        let (name, verified): (String, i64) = sqlx::query_as("SELECT name,\"emailVerified\"::bigint FROM \"user\" WHERE id='usr_invite_invitation'").fetch_one(control.raw()).await.unwrap();
        assert_eq!((name, verified), ("Synthetic Invitee".into(), 0));
        let hash: String = sqlx::query_scalar("SELECT password FROM account WHERE \"userId\"='usr_invite_invitation'").fetch_one(control.raw()).await.unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM assignments WHERE user_id='usr_invite_invitation'").fetch_one(holding.raw()).await.unwrap(), 0);
        assert_eq!(sqlx::query_scalar::<_, Option<String>>("SELECT user_id FROM hr_people WHERE id='person'").fetch_one(holding.raw()).await.unwrap(), None);
        assert!(signup(&scenario, &token).await.is_err());
        assert_eq!(effects(&control).await, 5);
    }).await }).await;
}

#[tokio::test]
async fn current_iam_review_role_people_and_scope_denials_leave_control_untouched() {
    database::with_database(1, |control| async move { database::with_database(1, |holding| async move {
        let (scenario, token) = setup(&control, &holding).await;
        for (mutate, restore) in [
            ("UPDATE assignments SET revoked_at=1 WHERE id='parent'", "UPDATE assignments SET revoked_at=NULL WHERE id='parent'"),
            ("UPDATE iam_roles SET status='archived' WHERE id='native_inviter'", "UPDATE iam_roles SET status='active' WHERE id='native_inviter'"),
            ("INSERT INTO iam_role_revisions SELECT company_id,role_id,2,'[]'::jsonb,grantable_json,scopes_json,created_by,created_at FROM iam_role_revisions WHERE role_id='native_inviter' AND revision=1; UPDATE iam_roles SET published_revision=2 WHERE id='native_inviter'", "UPDATE iam_roles SET published_revision=1 WHERE id='native_inviter'"),
            ("INSERT INTO iam_role_revisions SELECT company_id,role_id,3,permissions_json,'[]'::jsonb,scopes_json,created_by,created_at FROM iam_role_revisions WHERE role_id='native_inviter' AND revision=1; UPDATE iam_roles SET published_revision=3 WHERE id='native_inviter'", "UPDATE iam_roles SET published_revision=1 WHERE id='native_inviter'"),
            ("UPDATE iam_reviews SET status='rejected' WHERE id='review'", "UPDATE iam_reviews SET status='approved' WHERE id='review'"),
            ("UPDATE iam_reviews SET reviewed_by='fixture-a' WHERE id='review'", "UPDATE iam_reviews SET reviewed_by='fixture-b' WHERE id='review'"),
            ("UPDATE iam_reviews SET payload=jsonb_set(payload,'{expectedRoleRevision}','999999') WHERE id='review'", "UPDATE iam_reviews SET payload=jsonb_set(payload,'{expectedRoleRevision}',to_jsonb((SELECT published_revision FROM iam_roles WHERE company_id='*' AND id='finance_preparer'))) WHERE id='review'"),
            ("UPDATE iam_reviews SET payload=jsonb_set(payload,'{parentId}','\"foreign\"') WHERE id='review'", "UPDATE iam_reviews SET payload=jsonb_set(payload,'{parentId}','\"parent\"') WHERE id='review'"),
            ("UPDATE hr_people SET status='inactive' WHERE id='person'", "UPDATE hr_people SET status='active' WHERE id='person'"),
            ("UPDATE hr_people SET user_id='different-user' WHERE id='person'", "UPDATE hr_people SET user_id=NULL WHERE id='person'"),
            ("UPDATE hr_engagements SET status='suspended' WHERE id='engagement'", "UPDATE hr_engagements SET status='active' WHERE id='engagement'"),
            ("UPDATE hr_engagements SET department_id='foreign' WHERE id='engagement'", "UPDATE hr_engagements SET department_id='department' WHERE id='engagement'"),
            ("UPDATE organization_nodes SET status='suspended' WHERE id='branch'", "UPDATE organization_nodes SET status='active' WHERE id='branch'"),
            ("UPDATE assignments SET scope_kind='department',scope_node_id='foreign' WHERE id='parent'", "UPDATE assignments SET scope_kind='holding',scope_node_id=NULL WHERE id='parent'"),
        ] {
            sqlx::raw_sql(mutate).execute(holding.raw()).await.unwrap();
            assert!(signup(&scenario, &token).await.is_err(), "current owner denial: {mutate}");
            assert_eq!(effects(&control).await, 0);
            sqlx::raw_sql(restore).execute(holding.raw()).await.unwrap();
        }
        sqlx::query("UPDATE holding_bindings SET status='suspended'").execute(control.raw()).await.unwrap();
        assert!(signup(&scenario, &token).await.is_err()); assert_eq!(effects(&control).await, 0);
        sqlx::query("UPDATE holding_bindings SET status='active'").execute(control.raw()).await.unwrap();
        signup(&scenario, &token).await.unwrap();
    }).await }).await;
}

async fn wait_for_lock(pool: &DatabasePool, event: Option<&str>) -> bool {
    tokio::time::timeout(std::time::Duration::from_secs(4), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND wait_event_type='Lock' AND ($1::text IS NULL OR wait_event=$1))")
                .bind(event).fetch_one(pool.raw()).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.is_ok()
}

#[tokio::test]
async fn invitation_expiring_while_waiting_for_holding_policy_locks_never_creates_credentials() {
    database::with_database(1, |control| async move {
        database::with_database(3, |holding| async move {
            let (scenario, token) = setup(&control, &holding).await;
            let mut writer = holding.raw().begin().await.unwrap();
            sqlx::query("UPDATE hr_people SET version=version+1 WHERE id='person'")
                .execute(&mut *writer)
                .await
                .unwrap();
            let scenario = Arc::new(scenario);
            let task_scenario = scenario.clone();
            let task = tokio::spawn(async move { signup(&task_scenario, &token).await });
            let observed = wait_for_lock(&holding, Some("relation")).await;
            scenario.clock.advance(3_600_000);
            writer.commit().await.unwrap();
            assert!(task.await.unwrap().is_err());
            assert!(observed, "expected actual Holding policy lock wait");
            assert_eq!(effects(&control).await, 0);
        })
        .await
    })
    .await;
}

#[tokio::test]
async fn holding_policy_lease_survives_staged_control_writes_and_final_expiry_rolls_everything_back()
 {
    database::with_database(3, |control| async move { database::with_database(3, |holding| async move {
        let (scenario, token) = setup(&control, &holding).await;
        sqlx::raw_sql("CREATE FUNCTION invitation_pause() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='invitation_credential_created' THEN PERFORM pg_advisory_xact_lock(927341,21); END IF; RETURN NEW; END $$; CREATE TRIGGER invitation_pause BEFORE INSERT ON native_account_credential_events FOR EACH ROW EXECUTE FUNCTION invitation_pause()").execute(control.raw()).await.unwrap();
        let mut blocker = control.raw().acquire().await.unwrap(); blocker.close_on_drop();
        sqlx::query("SELECT pg_advisory_lock(927341,21)").execute(&mut *blocker).await.unwrap();
        let scenario = Arc::new(scenario); let task_scenario = scenario.clone();
        let task = tokio::spawn(async move { signup(&task_scenario, &token).await });
        if !wait_for_lock(&control, Some("advisory")).await {
            task.abort(); let _=task.await;
            sqlx::query("SELECT pg_advisory_unlock(927341,21)").execute(&mut *blocker).await.unwrap();
            panic!("signup did not reach staged Control audit lock wait");
        }
        let writer_pool = holding.clone();
        let writer = tokio::spawn(async move { sqlx::query("UPDATE hr_people SET status='inactive',version=version+1 WHERE id='person'").execute(writer_pool.raw()).await.unwrap() });
        let observed = wait_for_lock(&holding, Some("relation")).await;
        let pending = !writer.is_finished();
        scenario.clock.advance(3_600_000);
        sqlx::query("SELECT pg_advisory_unlock(927341,21)").execute(&mut *blocker).await.unwrap();
        assert!(task.await.unwrap().is_err()); writer.await.unwrap();
        assert!(observed && pending, "Holding source lease must remain held through Control commit");
        assert_eq!(effects(&control).await, 0);
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status FROM account_invitations WHERE id='invitation'").fetch_one(control.raw()).await.unwrap(), "pending");
    }).await }).await;
}

#[tokio::test]
async fn control_serialization_retry_reacquires_real_holding_authority_without_replaying_effects() {
    database::with_database(1, |control| async move { database::with_database(1, |holding| async move {
        let (scenario, token) = setup(&control, &holding).await;
        sqlx::raw_sql("CREATE SEQUENCE invitation_attempt; CREATE FUNCTION invitation_retry() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='invitation_credential_created' AND nextval('invitation_attempt')=1 THEN RAISE EXCEPTION 'synthetic retry' USING ERRCODE='40001'; END IF; RETURN NEW; END $$; CREATE TRIGGER invitation_retry BEFORE INSERT ON native_account_credential_events FOR EACH ROW EXECUTE FUNCTION invitation_retry()").execute(control.raw()).await.unwrap();
        signup(&scenario, &token).await.unwrap();
        assert_eq!(effects(&control).await, 5);
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT last_value FROM invitation_attempt").fetch_one(control.raw()).await.unwrap(), 2);
        assert!(signup(&scenario, &token).await.is_err());
        assert_eq!(effects(&control).await, 5);
    }).await }).await;
}
