//! Native Identity Access assignment workspace and withdrawal over migrated
//! PostgreSQL. These prove owner SQL, Company isolation, current IAM authority,
//! version/CAS, self/last-administrator guards, idempotency, rollback and
//! persisted readback. They are not a browser or Account/SSO journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_identity_access::public::assignment::{
    AssignmentActor, AssignmentRevocation, PgIdentityAssignments, assignment_workspace,
    revoke_assignment,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, RecordId, UserId,
};
use serde_json::Value;

const NOW: i64 = 500;

fn id(value: &str) -> RecordId {
    RecordId::new(value).unwrap()
}

fn actor(session: &str) -> AssignmentActor {
    AssignmentActor {
        user_id: UserId::new("actor").unwrap(),
        session_id: id(session),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: AssignmentId::new("assignment-actor").unwrap(),
        scope_kind: "holding".into(),
        scope_node_id: None,
        role: Some("it_admin".into()),
        role_revision: Some(99),
        assignment_version: Some(1),
        identity_version: Some(2),
        authority_revision: Some(9),
        permissions: vec!["iam.read".into(), "iam.manage".into()],
        person_user_ids: vec![UserId::new("actor").unwrap()],
        emergency_until: None,
        expires_at: 1_000_000,
    }
}

fn command(key: &str) -> CommandKey {
    CommandKey::new(key).unwrap()
}

fn input(version: i64) -> AssignmentRevocation {
    AssignmentRevocation {
        expected_version: version,
        reason: "  Reviewed withdrawal  ".into(),
    }
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','it_admin','IT administrator',true,99,'active'),\
         ('*','holding_owner','Holding owner',true,1,'active'),\
         ('*','employee','Employee',true,1,'active') \
         ON CONFLICT(company_id,id) DO UPDATE SET published_revision=excluded.published_revision,status='active';\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','it_admin',1,'[\"iam.read\"]','[]','[\"holding\"]','system',0),\
         ('*','it_admin',99,'[\"iam.read\",\"iam.manage\"]','[\"iam.read\",\"iam.manage\"]','[\"holding\"]','system',0),\
         ('*','holding_owner',1,'[\"iam.read\",\"iam.manage\"]','[\"iam.read\",\"iam.manage\"]','[\"holding\"]','system',0),\
         ('*','employee',1,'[\"iam.self\"]','[]','[\"holding\",\"branch\",\"department\"]','system',0) \
         ON CONFLICT(company_id,role_id,revision) DO NOTHING;\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) VALUES \
         ('company','actor','local:actor',NULL,'active','local',2),\
         ('company','target','local:target',NULL,'active','local',1),\
         ('company','owner','local:owner',NULL,'active','local',1),\
         ('company','foreign','local:foreign',NULL,'active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,version,scope_kind,scope_node_id,parent_id,source,source_id) VALUES \
         ('assignment-actor','actor','it_admin','[]','company',NULL,0,1000000,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('assignment-target','target','employee','[\"collaboration\"]','company',NULL,0,1000000,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('assignment-self','actor','employee','[]','company',NULL,0,1000000,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('assignment-owner','owner','holding_owner','[]','company',NULL,0,1000000,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('assignment-branch','target','employee','[]','company','branch-1',0,1000000,NULL,1,'branch','branch-1',NULL,'manual',NULL),\
         ('assignment-foreign','foreign','employee','[]','foreign',NULL,0,1000000,NULL,1,'holding',NULL,NULL,'manual',NULL);\
         INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES \
         ('session-1','actor','company','assignment-actor',1,9,1000000,0),\
         ('session-target','target','company','assignment-target',1,1,1000000,0),\
         ('session-self','actor','company','assignment-self',1,3,1000000,0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn workspace(pool: &DatabasePool) -> Value {
    let mut tx = pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await
        .unwrap();
    let mut repository = PgIdentityAssignments::new(&mut tx);
    let value = assignment_workspace(&mut repository, &actor("session-1"), NOW)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    value
}

async fn revoke(
    pool: &DatabasePool,
    session: &str,
    assignment: &str,
    key: &str,
    version: i64,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let mut tx = pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await
        .unwrap();
    let mut repository = PgIdentityAssignments::new(&mut tx);
    let result = revoke_assignment(
        &mut repository,
        &actor(session),
        &command(key),
        assignment,
        &input(version),
        NOW,
    )
    .await;
    match result {
        Ok(value) => {
            tx.commit().await.unwrap();
            Ok(value)
        }
        Err(error) => {
            tx.rollback().await.unwrap();
            Err(error)
        }
    }
}

async fn revoke_with_retry(
    pool: &DatabasePool,
    session: &str,
    assignment: &str,
    key: &str,
    version: i64,
) -> Result<Value, PgTransactionError<BusinessError>> {
    for _ in 0..4 {
        match revoke(pool, session, assignment, key, version).await {
            Err(PgTransactionError::Persistence(ref error))
                if error.sql_state() == Some("40001") =>
            {
                continue;
            }
            other => return other,
        }
    }
    revoke(pool, session, assignment, key, version).await
}

async fn effect_counts(pool: &DatabasePool, key: &str) -> (i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
         (SELECT count(*) FROM audit_events WHERE command_id=$1),\
         (SELECT count(*) FROM outbox_events WHERE command_id=$1),\
         (SELECT count(*) FROM iam_audit WHERE action='iam.assignment.revoke')",
    )
    .bind(key)
    .fetch_one(pool.raw())
    .await
    .unwrap()
}

#[tokio::test]
async fn workspace_reads_only_company_assignments_with_active_flags_and_roles() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let data = workspace(&pool).await;
        let rows = data["assignments"].as_array().unwrap();
        assert_eq!(rows.len(), 5);
        assert!(rows.iter().all(|row| row["company_id"] == "company"));
        assert!(!rows.iter().any(|row| row["id"] == "assignment-foreign"));
        let target = rows
            .iter()
            .find(|row| row["id"] == "assignment-target")
            .unwrap();
        assert_eq!(target["active"], true);
        assert_eq!(target["identity_status"], "active");
        assert_eq!(target["login_policy"], "local");
        let branch = rows
            .iter()
            .find(|row| row["id"] == "assignment-branch")
            .unwrap();
        assert_eq!(branch["scope_kind"], "branch");
        assert_eq!(branch["scope_node_id"], "branch-1");
        let roles = data["roles"].as_array().unwrap();
        assert!(roles.iter().any(|role| role["id"] == "employee"));
        assert_eq!(data["evaluatedAt"], NOW);
        assert_eq!(data["currentAssignmentId"], "assignment-actor");
        assert_eq!(data["currentUserId"], "actor");
        assert_eq!(data["scopeKind"], "holding");
    })
    .await;
}

#[tokio::test]
async fn withdrawal_persists_version_revocation_receipt_audit_and_outbox_readback() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let value = revoke(
            &pool,
            "session-1",
            "assignment-target",
            "iam-revoke-key-1",
            1,
        )
        .await
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({"id": "assignment-target", "status": "revoked", "version": 2})
        );
        let row: (Option<i64>, i64) = sqlx::query_as(
            "SELECT revoked_at,version FROM assignments WHERE id='assignment-target'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row, (Some(NOW), 2));
        let context: (Option<String>, Option<i64>, i64) = sqlx::query_as(
            "SELECT assignment_id,assignment_version,revision FROM active_work_contexts \
             WHERE session_id='session-target'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(context, (None, None, 2));
        assert_eq!(effect_counts(&pool, "iam-revoke-key-1").await, (1, 1, 1, 1));
        let authority: String = sqlx::query_scalar(
            "SELECT authority_json FROM audit_events WHERE command_id='iam-revoke-key-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let authority: Value = serde_json::from_str(&authority).unwrap();
        assert_eq!(authority["assignmentId"], "assignment-actor");
        assert_eq!(authority["companyId"], "company");
        let evidence: Value = sqlx::query_scalar(
            "SELECT evidence FROM iam_audit WHERE action='iam.assignment.revoke' \
             AND target_id='assignment-target'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(evidence["input"]["reason"], "Reviewed withdrawal");
        assert_eq!(evidence["result"]["version"], 2);
        let replay = revoke(
            &pool,
            "session-1",
            "assignment-target",
            "iam-revoke-key-1",
            1,
        )
        .await
        .unwrap();
        assert_eq!(replay["version"], 2);
        assert_eq!(effect_counts(&pool, "iam-revoke-key-1").await, (1, 1, 1, 1));
    })
    .await;
}

#[tokio::test]
async fn withdrawal_rejects_self_same_person_foreign_scope_stale_and_revoked_sources() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        assert_eq!(
            business(
                revoke(
                    &pool,
                    "session-1",
                    "assignment-self",
                    "iam-revoke-self-1",
                    1
                )
                .await
                .unwrap_err()
            )
            .code,
            "SELF_ACCESS_CHANGE"
        );
        assert_eq!(
            business(
                revoke(
                    &pool,
                    "session-1",
                    "assignment-foreign",
                    "iam-revoke-foreign-1",
                    1
                )
                .await
                .unwrap_err()
            )
            .code,
            "ASSIGNMENT_NOT_FOUND"
        );
        assert_eq!(
            business(
                revoke(
                    &pool,
                    "session-1",
                    "assignment-target",
                    "iam-revoke-stale-1",
                    9
                )
                .await
                .unwrap_err()
            )
            .code,
            "ASSIGNMENT_CHANGED"
        );
        sqlx::query(
            "UPDATE iam_members SET person_id='person-shared' WHERE user_id IN ('actor','target')",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            business(
                revoke(
                    &pool,
                    "session-1",
                    "assignment-target",
                    "iam-revoke-person-1",
                    1
                )
                .await
                .unwrap_err()
            )
            .code,
            "SELF_ACCESS_CHANGE"
        );
        sqlx::query("UPDATE iam_members SET person_id=NULL WHERE user_id IN ('actor','target')")
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query("UPDATE assignments SET revoked_at=100,version=2 WHERE id='assignment-target'")
            .execute(pool.raw())
            .await
            .unwrap();
        assert_eq!(
            business(
                revoke(
                    &pool,
                    "session-1",
                    "assignment-target",
                    "iam-revoke-revoked-1",
                    2
                )
                .await
                .unwrap_err()
            )
            .code,
            "ASSIGNMENT_CHANGED"
        );
        assert_eq!(
            effect_counts(&pool, "iam-revoke-person-1").await,
            (0, 0, 0, 0)
        );
    })
    .await;
}

#[tokio::test]
async fn withdrawal_protects_last_recovery_and_last_company_administrator() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::query("UPDATE iam_members SET login_policy='emergency' WHERE user_id='owner'")
            .execute(pool.raw())
            .await
            .unwrap();
        assert_eq!(
            business(
                revoke(
                    &pool,
                    "session-1",
                    "assignment-owner",
                    "iam-revoke-recovery-1",
                    1
                )
                .await
                .unwrap_err()
            )
            .code,
            "LAST_RECOVERY_ADMINISTRATOR"
        );
        sqlx::query("UPDATE iam_members SET login_policy='local' WHERE user_id='owner'")
            .execute(pool.raw())
            .await
            .unwrap();
        assert_eq!(
            business(
                revoke(
                    &pool,
                    "session-1",
                    "assignment-owner",
                    "iam-revoke-admin-1",
                    1
                )
                .await
                .unwrap_err()
            )
            .code,
            "LAST_ADMINISTRATOR"
        );
        assert_eq!(
            effect_counts(&pool, "iam-revoke-recovery-1").await,
            (0, 0, 0, 0)
        );
        assert_eq!(
            effect_counts(&pool, "iam-revoke-admin-1").await,
            (0, 0, 0, 0)
        );
    })
    .await;
}

#[tokio::test]
async fn concurrent_same_key_withdrawals_commit_once_and_replay_the_same_receipt() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let first_pool = pool.clone();
        let second_pool = pool.clone();
        let (first, second) = tokio::join!(
            revoke_with_retry(
                &first_pool,
                "session-1",
                "assignment-target",
                "iam-revoke-concurrent-1",
                1
            ),
            revoke_with_retry(
                &second_pool,
                "session-1",
                "assignment-target",
                "iam-revoke-concurrent-1",
                1
            )
        );
        assert_eq!(first.unwrap()["version"], 2);
        assert_eq!(second.unwrap()["version"], 2);
        let row: (Option<i64>, i64) = sqlx::query_as(
            "SELECT revoked_at,version FROM assignments WHERE id='assignment-target'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row, (Some(NOW), 2));
        assert_eq!(
            effect_counts(&pool, "iam-revoke-concurrent-1").await,
            (1, 1, 1, 1)
        );
    })
    .await;
}

#[tokio::test]
async fn emergency_membership_requires_a_live_bounded_session() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::query("UPDATE iam_members SET login_policy='emergency' WHERE user_id='actor'")
            .execute(pool.raw())
            .await
            .unwrap();
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let mut repository = PgIdentityAssignments::new(&mut tx);
        assert!(matches!(
            assignment_workspace(&mut repository, &actor("session-1"), NOW)
                .await
                .unwrap_err(),
            PgTransactionError::Business(ref error) if error.code == "IAM_ACCESS_CHANGED"
        ));
        tx.rollback().await.unwrap();
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let mut repository = PgIdentityAssignments::new(&mut tx);
        let emergency = AssignmentActor {
            emergency_until: Some(NOW + 1),
            ..actor("session-1")
        };
        assert!(
            assignment_workspace(&mut repository, &emergency, NOW)
                .await
                .is_ok()
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn late_outbox_failure_rolls_back_every_effect_and_exact_retry_persists_once() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "CREATE FUNCTION synthetic_assignment_outbox() RETURNS trigger LANGUAGE plpgsql AS \
             $$BEGIN RAISE EXCEPTION 'synthetic assignment outbox failure'; END$$;\
             CREATE TRIGGER synthetic_assignment_outbox BEFORE INSERT ON outbox_events \
             FOR EACH ROW EXECUTE FUNCTION synthetic_assignment_outbox();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let error = revoke(&pool, "session-1", "assignment-target", "iam-revoke-rollback-1", 1)
            .await
            .unwrap_err();
        assert!(matches!(error, PgTransactionError::Persistence(_)));
        let row: (Option<i64>, i64) =
            sqlx::query_as("SELECT revoked_at,version FROM assignments WHERE id='assignment-target'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(row, (None, 1));
        assert_eq!(
            effect_counts(&pool, "iam-revoke-rollback-1").await,
            (0, 0, 0, 0)
        );
        let context: (Option<String>, i64) = sqlx::query_as(
            "SELECT assignment_id,revision FROM active_work_contexts WHERE session_id='session-target'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(context, (Some("assignment-target".into()), 1));
        sqlx::raw_sql(
            "DROP TRIGGER synthetic_assignment_outbox ON outbox_events;\
             DROP FUNCTION synthetic_assignment_outbox();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let value = revoke(&pool, "session-1", "assignment-target", "iam-revoke-rollback-1", 1)
            .await
            .unwrap();
        assert_eq!(value["version"], 2);
        assert_eq!(
            effect_counts(&pool, "iam-revoke-rollback-1").await,
            (1, 1, 1, 1)
        );
    })
    .await;
}

#[tokio::test]
async fn transaction_and_serializable_guards_fail_closed_for_read_and_write() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut plain = pool.raw().acquire().await.unwrap();
        let mut repository = PgIdentityAssignments::new(&mut plain);
        let error = assignment_workspace(&mut repository, &actor("session-1"), NOW)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref error)
                if error.code == "IAM_ASSIGNMENT_TRANSACTION_REQUIRED"
        ));
        let mut read_committed = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
            .await
            .unwrap();
        let mut repository = PgIdentityAssignments::new(&mut read_committed);
        let error = assignment_workspace(&mut repository, &actor("session-1"), NOW)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref error)
                if error.code == "IAM_ASSIGNMENT_SERIALIZABLE_REQUIRED"
        ));
        read_committed.rollback().await.unwrap();
        let mut revoked = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        sqlx::query("INSERT INTO iam_revoked_sessions VALUES('session-1','company','actor',$1)")
            .bind(NOW)
            .execute(&mut *revoked)
            .await
            .unwrap();
        let mut repository = PgIdentityAssignments::new(&mut revoked);
        assert!(matches!(
            assignment_workspace(&mut repository, &actor("session-1"), NOW)
                .await
                .unwrap_err(),
            PgTransactionError::Business(ref error) if error.code == "IAM_ACCESS_CHANGED"
        ));
        revoked.rollback().await.unwrap();
    })
    .await;
}
