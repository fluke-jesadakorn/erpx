//! Native Agent evidence audit reads over real migrated PostgreSQL. These are
//! owner read projections, not command, provider or browser evidence.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_platform_administration::public::agent_evidence::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::BusinessError;

const NOW: i64 = 1_800_000_000_000;

fn digest() -> String {
    "a".repeat(64)
}

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}

async fn serializable(pool: &DatabasePool) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

async fn catalog(pool: &DatabasePool, id: &str, company: &str) {
    sqlx::query(
        "INSERT INTO agent_catalog(id,company_id,name,purpose,status,created_by,reviewed_by,version,created_at,updated_at) \
         VALUES($1,$2,'Support agent','Answer from approved sources only','approved','maker','reviewer',1,1,1)",
    )
    .bind(id)
    .bind(company)
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn version(pool: &DatabasePool, id: &str, agent: &str) {
    sqlx::query(
        "INSERT INTO agent_versions(id,agent_id,version_number,prompt,allowed_sources_json,prohibited_actions_json,status,created_by,reviewed_by,version,created_at,updated_at) \
         VALUES($1,$2,1,'Read approved Support sources','[\"/support\"]','[\"approve\",\"sign\",\"pay\"]','approved','maker','reviewer',1,1,1)",
    )
    .bind(id)
    .bind(agent)
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn assignment(pool: &DatabasePool, id: &str, company: &str, version: &str, user: &str) {
    sqlx::query(
        "INSERT INTO agent_assignments(id,company_id,agent_version_id,pool_id,user_id,scope_path,status,created_by,revoked_by,version,created_at,updated_at) \
         VALUES($1,$2,$3,NULL,$4,'/','active','maker',NULL,1,1,1)",
    )
    .bind(id)
    .bind(company)
    .bind(version)
    .bind(user)
    .execute(pool.raw())
    .await
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn session(
    pool: &DatabasePool,
    id: &str,
    company: &str,
    user: &str,
    assignment: &str,
    status: &str,
    expires_at: i64,
    updated_at: i64,
) {
    sqlx::query(
        "INSERT INTO ai_private_sessions(id,company_id,user_id,assignment_id,scope_key,conversation_key,agent_assignment_id,created_at,last_used_at,generation) \
         VALUES($1,$2,$3,$4,'/','conversation-'||$1,$4,1,1,1)",
    )
    .bind(id)
    .bind(company)
    .bind(user)
    .bind(assignment)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_private_session_controls(session_id,status,expires_at,legal_hold,hold_reason,version,last_used_at,updated_at) \
         VALUES($1,$2,$3,FALSE,NULL,1,1,$4)",
    )
    .bind(id)
    .bind(status)
    .bind(expires_at)
    .bind(updated_at)
    .execute(pool.raw())
    .await
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn invocation(
    pool: &DatabasePool,
    id: &str,
    session: &str,
    company: &str,
    user: &str,
    outcome: &str,
    error: Option<&str>,
    created_at: i64,
) {
    sqlx::query(
        "INSERT INTO ai_invocation_evidence(id,session_id,turn_id,company_id,user_id,agent_version_id,question_digest,sources_digest,authority_snapshot_json,provider_snapshot_json,outcome,input_tokens,output_tokens,error,created_at,governance_snapshot_json) \
         VALUES($1,$2,$3,$4,$5,'version-1',$6,$6,'{}','{}',$7,10,20,$8,$9,'{}')",
    )
    .bind(id)
    .bind(session)
    .bind(format!("turn-{id}"))
    .bind(company)
    .bind(user)
    .bind(digest())
    .bind(outcome)
    .bind(error)
    .bind(created_at)
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed(pool: &DatabasePool) {
    migrate(pool).await;
    catalog(pool, "agent-1", "company").await;
    version(pool, "version-1", "agent-1").await;
    assignment(pool, "assignment-1", "company", "version-1", "claimant").await;
    catalog(pool, "agent-x", "other").await;
    version(pool, "version-x", "agent-x").await;
    assignment(pool, "assignment-x", "other", "version-x", "outsider").await;

    session(
        pool,
        "s1",
        "company",
        "claimant",
        "assignment-1",
        "active",
        NOW + 1_000,
        40,
    )
    .await;
    session(
        pool,
        "s2",
        "company",
        "claimant",
        "assignment-1",
        "active",
        NOW,
        30,
    )
    .await;
    session(
        pool,
        "s3",
        "company",
        "other",
        "assignment-1",
        "active",
        NOW + 1_000,
        20,
    )
    .await;
    session(
        pool,
        "s4",
        "other",
        "outsider",
        "assignment-x",
        "active",
        NOW + 1_000,
        10,
    )
    .await;

    invocation(
        pool,
        "i1",
        "s1",
        "company",
        "claimant",
        "succeeded",
        None,
        30,
    )
    .await;
    invocation(
        pool,
        "i2",
        "s3",
        "company",
        "other",
        "failed",
        Some("provider unavailable"),
        20,
    )
    .await;
    invocation(pool, "i3", "s4", "other", "outsider", "succeeded", None, 10).await;
}

fn claimant() -> AgentEvidenceActor {
    AgentEvidenceActor {
        user_id: "claimant".into(),
        company_id: "company".into(),
        permissions: vec!["records.read".into()],
        modules: vec!["agent".into()],
        expires_at: NOW + 60_000,
    }
}

fn manager() -> AgentEvidenceActor {
    AgentEvidenceActor {
        permissions: vec!["records.read".into(), "ai.manage".into()],
        ..claimant()
    }
}

async fn read_workspace(
    pool: &DatabasePool,
    actor: &AgentEvidenceActor,
) -> Result<AgentEvidenceWorkspace, PgTransactionError<BusinessError>> {
    let mut tx = serializable(pool).await;
    lock_agent_evidence_sources(&mut tx).await.unwrap();
    let mut repository = PgAgentEvidenceRead::new(&mut tx);
    let workspace = agent_evidence_workspace(actor, &mut repository, NOW).await;
    tx.rollback().await.unwrap();
    workspace
}

#[tokio::test]
async fn workspace_and_record_reads_are_company_scoped_private_and_effective() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        let workspace = read_workspace(&holding, &claimant()).await.unwrap();
        assert_eq!(
            workspace
                .invocations
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["i1", "i2"]
        );
        assert_eq!(
            workspace
                .sessions
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["s1", "s2"]
        );
        assert_eq!(workspace.sessions[0].effective_status, "active");
        assert_eq!(workspace.sessions[1].effective_status, "expired");
        assert_eq!(
            workspace.invocations[1].error.as_deref(),
            Some("provider unavailable")
        );

        let managed = read_workspace(&holding, &manager()).await.unwrap();
        assert_eq!(
            managed
                .sessions
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["s1", "s2", "s3"]
        );

        let mut tx = serializable(&holding).await;
        lock_agent_evidence_sources(&mut tx).await.unwrap();
        let mut repository = PgAgentEvidenceRead::new(&mut tx);
        let record = agent_evidence_record(
            &claimant(),
            AgentEvidenceKind::Invocation,
            "i2",
            &mut repository,
            NOW,
        )
        .await
        .unwrap();
        let AgentEvidenceRecord::Invocation { record } = record else {
            panic!("expected invocation record");
        };
        assert_eq!(record.user_id, "other");
        let error = agent_evidence_record(
            &claimant(),
            AgentEvidenceKind::Session,
            "s3",
            &mut repository,
            NOW,
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Business(error) => {
                assert_eq!(error.code, "AGENT_RECORD_NOT_FOUND");
            }
            error => panic!("expected an owner business error, got {error:?}"),
        }
        let error = agent_evidence_record(
            &claimant(),
            AgentEvidenceKind::Invocation,
            "i3",
            &mut repository,
            NOW,
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Business(error) => {
                assert_eq!(error.code, "AGENT_RECORD_NOT_FOUND");
            }
            error => panic!("expected an owner business error, got {error:?}"),
        }
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn reads_require_serializable_units_and_leave_sources_unchanged() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let before: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_invocation_evidence")
            .fetch_one(holding.raw())
            .await
            .unwrap();
        let sessions_before: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_private_sessions")
            .fetch_one(holding.raw())
            .await
            .unwrap();

        let mut connection = holding.raw().acquire().await.unwrap();
        let error = read_invocations(&mut connection, "company").await.unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "AGENT_EVIDENCE_TRANSACTION_REQUIRED"
        ));
        drop(connection);

        let mut tx = holding.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *tx)
            .await
            .unwrap();
        let error = read_invocations(&mut tx, "company").await.unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "AGENT_EVIDENCE_SERIALIZABLE_REQUIRED"
        ));
        tx.rollback().await.unwrap();

        let mut tx = serializable(&holding).await;
        lock_agent_evidence_sources(&mut tx).await.unwrap();
        {
            let mut repository = PgAgentEvidenceRead::new(&mut tx);
            let workspace = agent_evidence_workspace(&claimant(), &mut repository, NOW)
                .await
                .unwrap();
            assert_eq!(workspace.invocations.len(), 2);
        }
        sqlx::query(
            "INSERT INTO ai_invocation_evidence(id,session_id,turn_id,company_id,user_id,agent_version_id,question_digest,sources_digest,authority_snapshot_json,provider_snapshot_json,outcome,input_tokens,output_tokens,error,created_at,governance_snapshot_json) \
             VALUES('i4','s1','turn-i4','company','claimant','version-1',$1,$1,'{}','{}','succeeded',NULL,NULL,NULL,40,'{}')",
        )
        .bind(digest())
        .execute(&mut *tx)
        .await
        .unwrap();
        let mut repository = PgAgentEvidenceRead::new(&mut tx);
        let workspace = agent_evidence_workspace(&claimant(), &mut repository, NOW)
            .await
            .unwrap();
        assert_eq!(workspace.invocations.len(), 3);
        assert_eq!(workspace.invocations[0].id, "i4");
        tx.rollback().await.unwrap();

        let after: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_invocation_evidence")
            .fetch_one(holding.raw())
            .await
            .unwrap();
        let sessions_after: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_private_sessions")
            .fetch_one(holding.raw())
            .await
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(sessions_before, sessions_after);
    })
    .await;
}

#[tokio::test]
async fn policy_failure_and_committed_readback_follow_the_actor_unit() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let mut denied = claimant();
        denied.permissions.clear();
        match read_workspace(&holding, &denied).await.unwrap_err() {
            PgTransactionError::Business(error) => {
                assert_eq!(error.code, "PERMISSION_REQUIRED");
            }
            error => panic!("expected an owner business error, got {error:?}"),
        }

        let mut tx = serializable(&holding).await;
        lock_agent_evidence_sources(&mut tx).await.unwrap();
        sqlx::query(
            "INSERT INTO ai_invocation_evidence(id,session_id,turn_id,company_id,user_id,agent_version_id,question_digest,sources_digest,authority_snapshot_json,provider_snapshot_json,outcome,input_tokens,output_tokens,error,created_at,governance_snapshot_json) \
             VALUES('i5','s2','turn-i5','company','claimant',NULL,$1,$1,'{}','{}','failed',NULL,NULL,'bounded failure',50,'{}')",
        )
        .bind(digest())
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let workspace = read_workspace(&holding, &claimant()).await.unwrap();
        assert_eq!(workspace.invocations.len(), 3);
        assert_eq!(workspace.invocations[0].id, "i5");
        assert_eq!(workspace.invocations[0].outcome, "failed");
        assert_eq!(
            workspace.invocations[0].error.as_deref(),
            Some("bounded failure")
        );
    })
    .await;
}
