//! Native Agent governance over a leased PostgreSQL database: catalogue,
//! version approval, pool membership, assignment start/delegate/cancel,
//! private-session hold/expire, replay and retained evidence.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_agent::public::governance::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{AssignmentId, BusinessError, CompanyId, UserId};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandField, CommandJson, WorkAuthority,
};
use serde_json::{Value, json};

const NOW: i64 = 1_000;

fn command_actor(company: &str, user: &str, assignment: &str, role: &str) -> CommandActor {
    CommandActor {
        user_id: UserId::new(user).unwrap(),
        assignment_id: AssignmentId::new(assignment).unwrap(),
        role: role.into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: CommandField::Omitted,
        department_id: CommandField::Omitted,
        authority: CommandField::Value(WorkAuthority {
            session_id: "session".into(),
            revision: 1,
            assignment_version: 1,
        }),
        iam: CommandField::Value(
            CommandJson::parse(
                r#"{"roleRevision":1,"identityVersion":1,"permissions":[],"grantablePermissions":[],"principalId":"principal","personId":null,"personUserIds":[],"freshUntil":null}"#,
            )
            .unwrap(),
        ),
        assignment_scope_kind: "holding".into(),
        assignment_scope_node_id: None,
    }
}

fn actor(company: &str, user: &str, assignment: &str, role: &str) -> AgentActor {
    AgentActor::new(
        "h001",
        command_actor(company, user, assignment, role),
        vec!["agent".into()],
        vec!["ai.manage".into(), "records.read".into()],
    )
}

fn owner() -> AgentActor {
    actor("company", "owner", "assign-owner", "holding_owner")
}
fn reviewer() -> AgentActor {
    actor("company", "reviewer", "assign-reviewer", "holding_owner")
}
fn foreign() -> AgentActor {
    actor("foreign", "owner", "assign-foreign", "holding_owner")
}

fn code(error: AgentError) -> &'static str {
    match error {
        PgTransactionError::Business(error) => error.code,
        PgTransactionError::Persistence(error) => {
            panic!("unexpected persistence failure: {error}")
        }
    }
}

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,status) VALUES\
         ('company','company','acme','Acme',NULL,'company','active'),\
         ('foreign','company','foreign','Foreign',NULL,'foreign','active');\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES\
         ('assign-owner','owner','holding_owner','[\"agent\"]','company',0,9007199254740000,'holding'),\
         ('assign-reviewer','reviewer','holding_owner','[\"agent\"]','company',0,9007199254740000,'holding'),\
         ('assign-claimant','claimant','holding_owner','[\"agent\"]','company',0,9007199254740000,'holding'),\
         ('assign-foreign','owner','holding_owner','[\"agent\"]','foreign',0,9007199254740000,'holding');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn create(
    pool: &DatabasePool,
    actor: &AgentActor,
    raw: Value,
    key: &str,
) -> Result<Value, AgentError> {
    let actor = actor.clone();
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let raw = raw.clone();
            let key = key.clone();
            Box::pin(async move {
                let mut ports = PgAgentGovernance::new(connection, &actor, NOW);
                create_agent(&mut ports, &raw, &key).await
            })
        })
        .await
}

async fn create_version(
    pool: &DatabasePool,
    actor: &AgentActor,
    raw: Value,
    key: &str,
) -> Result<Value, AgentError> {
    let actor = actor.clone();
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let raw = raw.clone();
            let key = key.clone();
            Box::pin(async move {
                let mut ports = PgAgentGovernance::new(connection, &actor, NOW);
                create_agent_version(&mut ports, &raw, &key).await
            })
        })
        .await
}

async fn create_pool(
    pool: &DatabasePool,
    actor: &AgentActor,
    raw: Value,
    key: &str,
) -> Result<Value, AgentError> {
    let actor = actor.clone();
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let raw = raw.clone();
            let key = key.clone();
            Box::pin(async move {
                let mut ports = PgAgentGovernance::new(connection, &actor, NOW);
                create_agent_pool(&mut ports, &raw, &key).await
            })
        })
        .await
}

async fn start_assignment(
    pool: &DatabasePool,
    actor: &AgentActor,
    raw: Value,
    key: &str,
) -> Result<Value, AgentError> {
    let actor = actor.clone();
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let raw = raw.clone();
            let key = key.clone();
            Box::pin(async move {
                let mut ports = PgAgentGovernance::new(connection, &actor, NOW);
                start_agent_assignment(&mut ports, &raw, &key).await
            })
        })
        .await
}

async fn change_record(
    pool: &DatabasePool,
    actor: &AgentActor,
    kind: RecordKind,
    id: &str,
    raw: Value,
    key: &str,
) -> Result<Value, AgentError> {
    let actor = actor.clone();
    let id = id.to_owned();
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let raw = raw.clone();
            let id = id.clone();
            let key = key.clone();
            Box::pin(async move {
                let mut ports = PgAgentGovernance::new(connection, &actor, NOW);
                change_agent_record(&mut ports, kind, &id, &raw, &key).await
            })
        })
        .await
}

async fn delegate(
    pool: &DatabasePool,
    actor: &AgentActor,
    id: &str,
    raw: Value,
    key: &str,
) -> Result<Value, AgentError> {
    let actor = actor.clone();
    let id = id.to_owned();
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let raw = raw.clone();
            let id = id.clone();
            let key = key.clone();
            Box::pin(async move {
                let mut ports = PgAgentGovernance::new(connection, &actor, NOW);
                delegate_agent_assignment(&mut ports, &id, &raw, &key).await
            })
        })
        .await
}

async fn cancel(
    pool: &DatabasePool,
    actor: &AgentActor,
    id: &str,
    raw: Value,
    key: &str,
) -> Result<Value, AgentError> {
    let actor = actor.clone();
    let id = id.to_owned();
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let raw = raw.clone();
            let id = id.clone();
            let key = key.clone();
            Box::pin(async move {
                let mut ports = PgAgentGovernance::new(connection, &actor, NOW);
                cancel_agent_assignment(&mut ports, &id, &raw, &key).await
            })
        })
        .await
}

async fn retry(
    pool: &DatabasePool,
    actor: &AgentActor,
    id: &str,
    raw: Value,
    key: &str,
) -> Result<Value, AgentError> {
    let actor = actor.clone();
    let id = id.to_owned();
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let raw = raw.clone();
            let id = id.clone();
            let key = key.clone();
            Box::pin(async move {
                let mut ports = PgAgentGovernance::new(connection, &actor, NOW);
                retry_agent_assignment(&mut ports, &id, &raw, &key).await
            })
        })
        .await
}

async fn session(
    pool: &DatabasePool,
    actor: &AgentActor,
    id: &str,
    raw: Value,
    key: &str,
) -> Result<Value, AgentError> {
    let actor = actor.clone();
    let id = id.to_owned();
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let raw = raw.clone();
            let id = id.clone();
            let key = key.clone();
            Box::pin(async move {
                let mut ports = PgAgentGovernance::new(connection, &actor, NOW);
                change_private_session(&mut ports, &id, &raw, &key).await
            })
        })
        .await
}

async fn change_pool(
    pool: &DatabasePool,
    actor: &AgentActor,
    id: &str,
    raw: Value,
    key: &str,
) -> Result<Value, AgentError> {
    let actor = actor.clone();
    let id = id.to_owned();
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let raw = raw.clone();
            let id = id.clone();
            let key = key.clone();
            Box::pin(async move {
                let mut ports = PgAgentGovernance::new(connection, &actor, NOW);
                change_agent_pool(&mut ports, &id, &raw, &key).await
            })
        })
        .await
}

#[tokio::test]
async fn agent_governance_persists_catalogue_pool_assignment_and_session_evidence() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;

        let agent = create(
            &pool,
            &owner(),
            json!({"name": "Support agent", "purpose": "Answer support questions"}),
            "agent-key",
        )
        .await
        .unwrap();
        assert_eq!(agent, json!({"id": "agent-agent-key", "status": "draft", "version": 1}));
        let replay = create(
            &pool,
            &owner(),
            json!({"name": "Support agent", "purpose": "Answer support questions"}),
            "agent-key",
        )
        .await
        .unwrap();
        assert_eq!(replay, agent);
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id='agent-key'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let audits: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_events WHERE command_id='agent-key'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let outbox: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM outbox_events WHERE command_id='agent-key'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!((receipts, audits, outbox), (1, 1, 1));

        let approved = change_record(
            &pool,
            &reviewer(),
            RecordKind::Agent,
            "agent-agent-key",
            json!({"action": "approve", "expectedVersion": 1, "reason": "Reviewed catalogue entry"}),
            "agent-approve",
        )
        .await
        .unwrap();
        assert_eq!(approved["status"], "approved");
        assert_eq!(approved["version"], 2);
        assert_eq!(
            code(change_record(
                &pool,
                &reviewer(),
                RecordKind::Agent,
                "agent-agent-key",
                json!({"action": "approve", "expectedVersion": 1, "reason": "Reviewed catalogue entry"}),
                "agent-approve-stale",
            )
            .await
            .unwrap_err()),
            "CONFLICT"
        );
        assert_eq!(
            code(create(
                &pool,
                &reviewer(),
                json!({"name": "Support agent", "purpose": "Answer support questions"}),
                "agent-key",
            )
            .await
            .unwrap_err()),
            "IDEMPOTENCY_CONFLICT"
        );

        let version = create_version(
            &pool,
            &owner(),
            json!({
                "agentId": "agent-agent-key",
                "prompt": "Answer support questions with citations",
                "allowedSources": ["/support", "/records"],
            }),
            "version-key",
        )
        .await
        .unwrap();
        assert_eq!(version["versionNumber"], 1);
        assert_eq!(version["status"], "draft");
        change_record(
            &pool,
            &reviewer(),
            RecordKind::Version,
            "agent-version-version-key",
            json!({"action": "approve", "expectedVersion": 1, "reason": "Reviewed prompt"}),
            "version-approve",
        )
        .await
        .unwrap();

        let pool_row = create_pool(
            &pool,
            &owner(),
            json!({"name": "Support routing", "userIds": ["claimant"]}),
            "pool-key",
        )
        .await
        .unwrap();
        assert_eq!(pool_row, json!({"id": "agent-pool-pool-key", "status": "active", "version": 1}));
        let changed = change_pool(
            &pool,
            &owner(),
            "agent-pool-pool-key",
            json!({
                "action": "replace-members",
                "expectedVersion": 1,
                "userIds": ["reviewer"],
                "reason": "Reviewed routing change",
            }),
            "pool-change",
        )
        .await
        .unwrap();
        assert_eq!(changed, json!({"id": "agent-pool-pool-key", "status": "active", "version": 2}));
        let revoked: Option<i64> = sqlx::query_scalar(
            "SELECT revoked_at FROM agent_pool_members WHERE pool_id='agent-pool-pool-key' AND user_id='claimant'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(revoked.is_some());

        let assignment = start_assignment(
            &pool,
            &owner(),
            json!({
                "agentVersionId": "agent-version-version-key",
                "userId": "claimant",
                "scopePath": "/support",
            }),
            "assignment-key",
        )
        .await
        .unwrap();
        assert_eq!(
            assignment,
            json!({"id": "agent-assignment-assignment-key", "status": "active", "version": 1})
        );
        let delegated = delegate(
            &pool,
            &owner(),
            "agent-assignment-assignment-key",
            json!({
                "expectedVersion": 1,
                "poolId": "agent-pool-pool-key",
                "scopePath": "/support",
                "reason": "Delegate the support scope to the reviewed pool",
            }),
            "delegate-key",
        )
        .await
        .unwrap();
        assert_eq!(delegated["previousId"], "agent-assignment-assignment-key");
        assert_eq!(delegated["id"], "agent-assignment-delegate-key");
        let prior_status: String = sqlx::query_scalar(
            "SELECT status FROM agent_assignments WHERE id='agent-assignment-assignment-key'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(prior_status, "revoked");

        let cancelled = cancel(
            &pool,
            &owner(),
            "agent-assignment-delegate-key",
            json!({"expectedVersion": 1, "reason": "End the delegated scope"}),
            "cancel-key",
        )
        .await
        .unwrap();
        assert_eq!(cancelled, json!({"id": "agent-assignment-delegate-key", "status": "revoked", "version": 2}));
        let retried = retry(
            &pool,
            &owner(),
            "agent-assignment-delegate-key",
            json!({"expectedVersion": 2, "reason": "Retry the reviewed scope"}),
            "retry-key",
        )
        .await
        .unwrap();
        assert_eq!(retried["previousId"], "agent-assignment-delegate-key");
        assert_eq!(retried["id"], "agent-assignment-retry-key");
        assert_eq!(
            code(cancel(
                &pool,
                &owner(),
                "agent-assignment-delegate-key",
                json!({"expectedVersion": 2, "reason": "End the delegated scope"}),
                "cancel-again",
            )
            .await
            .unwrap_err()),
            "CONFLICT"
        );
        let cancelled_retry = cancel(
            &pool,
            &owner(),
            "agent-assignment-retry-key",
            json!({"expectedVersion": 1, "reason": "End the retried scope"}),
            "cancel-retry",
        )
        .await
        .unwrap();
        assert_eq!(cancelled_retry["version"], 2);

        sqlx::raw_sql(
            "INSERT INTO ai_private_sessions(id,company_id,user_id,assignment_id,scope_key,conversation_key,agent_assignment_id,created_at,last_used_at,generation) \
             VALUES('session-1','company','claimant','assign-claimant','/support','conversation-1',NULL,1000,1000,1);\
             INSERT INTO ai_private_session_controls(session_id,expires_at,last_used_at,updated_at) \
             VALUES('session-1',9007199254740000,1000,1000);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let held = session(
            &pool,
            &reviewer(),
            "session-1",
            json!({"action": "hold", "expectedVersion": 1, "reason": "Legal retention review"}),
            "session-hold",
        )
        .await
        .unwrap();
        assert_eq!(held, json!({"id": "session-1", "version": 2, "action": "hold"}));
        let released = session(
            &pool,
            &reviewer(),
            "session-1",
            json!({"action": "release-hold", "expectedVersion": 2, "reason": "Review complete"}),
            "session-release",
        )
        .await
        .unwrap();
        assert_eq!(released["version"], 3);
        let expired = session(
            &pool,
            &reviewer(),
            "session-1",
            json!({"action": "expire", "expectedVersion": 3, "reason": "Retention elapsed"}),
            "session-expire",
        )
        .await
        .unwrap();
        assert_eq!(expired["version"], 4);
        let event_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ai_private_session_events WHERE session_id='session-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(event_count, 3);

        assert_eq!(
            code(change_record(
                &pool,
                &foreign(),
                RecordKind::Agent,
                "agent-agent-key",
                json!({"action": "retire", "expectedVersion": 2, "reason": "Foreign attempt"}),
                "foreign-change",
            )
            .await
            .unwrap_err()),
            "AGENT_RECORD_NOT_FOUND"
        );
        let foreign_agent = create(
            &pool,
            &foreign(),
            json!({"name": "Foreign agent", "purpose": "Answer foreign support questions"}),
            "foreign-agent",
        )
        .await
        .unwrap();
        assert_eq!(foreign_agent["id"], "agent-foreign-agent");
    })
    .await;
}
