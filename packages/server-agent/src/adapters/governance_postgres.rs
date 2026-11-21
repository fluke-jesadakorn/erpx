//! PostgreSQL Agent governance commands. Every method runs on the caller's
//! serializable Holding transaction and keeps the retained guard, receipt,
//! audit and outbox evidence in that same unit.
use crate::application::governance::{
    AgentActor, AgentGovernancePorts, AgentMutation, AgentMutationAction,
};
use crate::domain::governance::{AssignmentState, PolicyCandidate, RecordKind, RecordState};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::commands::{CommandEventScope, CommandJson, command_fingerprint};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

pub type AgentError = PgTransactionError<BusinessError>;

fn conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONFLICT",
        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
    )
}

fn authority_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "AGENT_AUTHORITY_CHANGED",
        "Your current Agent authority is unavailable.",
    )
}

fn invalid_action() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "AGENT_ACTION_INVALID",
        "Choose an available lifecycle action for this record.",
    )
}

/// Current IAM, module and Company scope authority. The retained TypeScript
/// owner checked `ai.manage`/`records.read` plus the `agent` module; the same
/// exact source is re-read here on the caller's transaction.
pub(crate) async fn authorize(
    connection: &mut PgConnection,
    actor: &AgentActor,
    now: i64,
    write: bool,
) -> Result<(), AgentError> {
    let permission = if write { "ai.manage" } else { "records.read" };
    actor.require_manage(write)?;
    if write {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND kind='company' AND status='active')",
        )
        .bind(actor.company_id())
        .fetch_one(&mut *connection)
        .await?;
        if !exists {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "AGENT_MANAGEMENT_FORBIDDEN",
                "An active Company scope is required for Agent governance.",
            )
            .into());
        }
    }
    let current: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM assignments g WHERE g.id=$1 AND g.user_id=$2 AND g.role=$3 \
         AND g.company_id=$4 AND iam_assignment_active(g.id,$5) \
         AND iam_principal_active(g.user_id,g.company_id,$5) \
         AND iam_role_allows(g.role,g.company_id,$6))",
    )
    .bind(actor.assignment_id())
    .bind(actor.user_id())
    .bind(actor.command.role.as_str())
    .bind(actor.company_id())
    .bind(now)
    .bind(permission)
    .fetch_one(&mut *connection)
    .await?;
    if !current {
        return Err(authority_changed().into());
    }
    Ok(())
}

/// Constraint failures are the retained guard outcome; serialization and
/// deadlock states stay persistence failures so the owning unit can retry.
fn statement_error(error: sqlx::Error) -> AgentError {
    match error
        .as_database_error()
        .and_then(|error| error.code())
        .as_deref()
    {
        Some("23502" | "23503" | "23505" | "23514") => conflict().into(),
        _ => PgTransactionError::Persistence(PersistenceError::from(error)),
    }
}

pub struct PgAgentGovernance<'c> {
    connection: &'c mut PgConnection,
    actor: &'c AgentActor,
    now: i64,
}

impl<'c> PgAgentGovernance<'c> {
    pub fn new(connection: &'c mut PgConnection, actor: &'c AgentActor, now: i64) -> Self {
        Self {
            connection,
            actor,
            now,
        }
    }

    async fn require_transaction(connection: &mut PgConnection) -> Result<(), AgentError> {
        if !connection.is_in_transaction() {
            return Err(PersistenceError::new("AGENT_COMMAND_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }

    async fn guard(&mut self, sql: &'static str, binds: &[&str]) -> Result<(), AgentError> {
        let mut query = sqlx::query_scalar::<_, bool>(sql);
        for value in binds {
            query = query.bind(*value);
        }
        let found = query.fetch_one(&mut *self.connection).await?;
        if found {
            Ok(())
        } else {
            Err(conflict().into())
        }
    }

    async fn guard_approved_version(
        &mut self,
        version_id: &str,
        company: &str,
    ) -> Result<(), AgentError> {
        self.guard(
            "SELECT EXISTS(SELECT 1 FROM agent_versions v JOIN agent_catalog a ON a.id=v.agent_id \
             WHERE v.id=$1 AND v.status='approved' AND a.status='approved' AND a.company_id=$2)",
            &[version_id, company],
        )
        .await
    }

    async fn guard_active_pool(&mut self, pool_id: &str, company: &str) -> Result<(), AgentError> {
        self.guard(
            "SELECT EXISTS(SELECT 1 FROM agent_pools WHERE id=$1 AND company_id=$2 AND status='active')",
            &[pool_id, company],
        )
        .await
    }

    async fn guard_eligible_member(
        &mut self,
        company: &str,
        user_id: &str,
    ) -> Result<(), AgentError> {
        let found: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM assignments WHERE company_id=$1 AND user_id=$2 \
             AND iam_assignment_active(id,$3) AND iam_principal_active(user_id,company_id,$3))",
        )
        .bind(company)
        .bind(user_id)
        .bind(self.now)
        .fetch_one(&mut *self.connection)
        .await?;
        if found {
            Ok(())
        } else {
            Err(conflict().into())
        }
    }

    async fn guard_binding(
        &mut self,
        agent_version_id: &str,
        pool_id: Option<&str>,
        user_id: Option<&str>,
        company: &str,
    ) -> Result<(), AgentError> {
        self.guard_approved_version(agent_version_id, company)
            .await?;
        match (pool_id, user_id) {
            (Some(pool), None) => self.guard_active_pool(pool, company).await,
            (None, Some(user)) => self.guard_eligible_member(company, user).await,
            _ => Err(invalid_action().into()),
        }
    }

    async fn guard_session(
        &mut self,
        session_id: &str,
        expected_version: i64,
        manage: bool,
    ) -> Result<(String, bool), AgentError> {
        let row = sqlx::query(
            "SELECT c.status,c.version,c.legal_hold FROM ai_private_session_controls c \
             JOIN ai_private_sessions s ON s.id=c.session_id \
             WHERE c.session_id=$1 AND s.company_id=$2 AND (s.user_id=$3 OR $4) FOR UPDATE OF c",
        )
        .bind(session_id)
        .bind(self.actor.company_id())
        .bind(self.actor.user_id())
        .bind(manage)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Err(conflict().into());
        };
        let version: i64 = row.try_get("version")?;
        if version != expected_version {
            return Err(conflict().into());
        }
        Ok((row.try_get("status")?, row.try_get("legal_hold")?))
    }

    async fn insert_receipt(
        &mut self,
        mutation: &AgentMutation,
        result: &Value,
    ) -> Result<(), AgentError> {
        let digest = command_fingerprint(
            &self.actor.command,
            mutation.common.operation,
            &mutation.common.record_id,
            &mutation.common.input,
        )?;
        sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(&mutation.common.key)
        .bind(mutation.common.operation)
        .bind(self.actor.user_id())
        .bind(digest)
        .bind(&mutation.common.record_id)
        .bind(result.to_string())
        .bind(mutation.common.now)
        .bind(self.actor.command.receipt_authority()?.as_str())
        .execute(&mut *self.connection)
        .await
        .map_err(statement_error)?;
        Ok(())
    }
}

impl AgentGovernancePorts for PgAgentGovernance<'_> {
    type Error = AgentError;

    fn actor(&self) -> &AgentActor {
        self.actor
    }

    fn now(&self) -> i64 {
        self.now
    }

    async fn authorize(&mut self, write: bool) -> Result<(), Self::Error> {
        authorize(self.connection, self.actor, self.now, write).await
    }

    async fn replay(
        &mut self,
        key: &str,
        operation: &str,
        record_id: &str,
        input: &CommandJson,
    ) -> Result<Option<Value>, Self::Error> {
        let row = sqlx::query(
            "SELECT operation,request_hash,result_json FROM command_receipts WHERE command_id=$1",
        )
        .bind(key)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let saved_operation: String = row.try_get("operation")?;
        let saved_hash: String = row.try_get("request_hash")?;
        let digest = command_fingerprint(&self.actor.command, operation, record_id, input)?;
        if saved_operation != operation || saved_hash != digest {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            )
            .into());
        }
        let result: String = row.try_get("result_json")?;
        let value = serde_json::from_str::<Value>(&result).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "AGENT_COMMAND_RESULT_UNAVAILABLE",
                "The retained Agent command result is unavailable.",
            )
        })?;
        Ok(Some(value))
    }

    async fn next_version_number(&mut self, agent_id: &str) -> Result<Option<i64>, Self::Error> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM agent_catalog WHERE id=$1 AND company_id=$2)",
        )
        .bind(agent_id)
        .bind(self.actor.company_id())
        .fetch_one(&mut *self.connection)
        .await?;
        if !exists {
            return Ok(None);
        }
        let current: i64 = sqlx::query_scalar(
            "SELECT COALESCE(max(v.version_number),0)::bigint FROM agent_versions v \
             JOIN agent_catalog a ON a.id=v.agent_id WHERE v.agent_id=$1 AND a.company_id=$2",
        )
        .bind(agent_id)
        .bind(self.actor.company_id())
        .fetch_one(&mut *self.connection)
        .await?;
        Ok(Some(current))
    }

    async fn record(
        &mut self,
        kind: RecordKind,
        id: &str,
    ) -> Result<Option<RecordState>, Self::Error> {
        let row = match kind {
            RecordKind::Agent => sqlx::query(
                "SELECT status,version,created_by FROM agent_catalog WHERE id=$1 AND company_id=$2",
            )
            .bind(id)
            .bind(self.actor.company_id())
            .fetch_optional(&mut *self.connection)
            .await?,
            RecordKind::Version => sqlx::query(
                "SELECT v.status,v.version,v.created_by FROM agent_versions v \
                 JOIN agent_catalog a ON a.id=v.agent_id WHERE v.id=$1 AND a.company_id=$2",
            )
            .bind(id)
            .bind(self.actor.company_id())
            .fetch_optional(&mut *self.connection)
            .await?,
            RecordKind::Assignment => sqlx::query(
                "SELECT status,version,created_by FROM agent_assignments WHERE id=$1 AND company_id=$2",
            )
            .bind(id)
            .bind(self.actor.company_id())
            .fetch_optional(&mut *self.connection)
            .await?,
        };
        match row {
            Some(row) => Ok(Some(RecordState {
                status: row.try_get("status")?,
                version: row.try_get("version")?,
                created_by: row.try_get("created_by")?,
            })),
            None => Ok(None),
        }
    }

    async fn assignment(&mut self, id: &str) -> Result<Option<AssignmentState>, Self::Error> {
        let row = sqlx::query(
            "SELECT status,version,agent_version_id,pool_id,user_id,scope_path \
             FROM agent_assignments WHERE id=$1 AND company_id=$2",
        )
        .bind(id)
        .bind(self.actor.company_id())
        .fetch_optional(&mut *self.connection)
        .await?;
        match row {
            Some(row) => Ok(Some(AssignmentState {
                status: row.try_get("status")?,
                version: row.try_get("version")?,
                agent_version_id: row.try_get("agent_version_id")?,
                pool_id: row.try_get("pool_id")?,
                user_id: row.try_get("user_id")?,
                scope_path: row.try_get("scope_path")?,
            })),
            None => Ok(None),
        }
    }

    async fn policy_candidates(&mut self) -> Result<Vec<PolicyCandidate>, Self::Error> {
        let rows = sqlx::query(
            "SELECT x.id,x.version assignment_version,x.scope_path,x.pool_id,x.agent_version_id,\
             v.version version_revision,v.prompt,v.allowed_sources_json,v.status version_status,\
             c.version catalog_version,c.status catalog_status,p.version pool_version,p.status pool_status \
             FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id \
             JOIN agent_catalog c ON c.id=v.agent_id LEFT JOIN agent_pools p ON p.id=x.pool_id \
             WHERE x.company_id=$1 AND c.company_id=x.company_id AND x.status='active' \
             AND (x.user_id=$2 OR EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=x.pool_id AND m.user_id=$2 AND m.revoked_at IS NULL)) \
             ORDER BY length(x.scope_path) DESC,x.created_at DESC,x.id",
        )
        .bind(self.actor.company_id())
        .bind(self.actor.user_id())
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                let allowed_sources_json: String = row.try_get("allowed_sources_json")?;
                let allowed_sources = serde_json::from_str::<Vec<String>>(&allowed_sources_json)
                    .map_err(|_| {
                        BusinessError::new(
                            FailureKind::Unavailable,
                            "AGENT_SOURCES_UNAVAILABLE",
                            "The approved Agent sources are unavailable.",
                        )
                    })?;
                Ok(PolicyCandidate {
                    assignment_id: row.try_get("id")?,
                    assignment_version: row.try_get("assignment_version")?,
                    scope_path: row.try_get("scope_path")?,
                    pool_id: row.try_get("pool_id")?,
                    agent_version_id: row.try_get("agent_version_id")?,
                    agent_version_revision: row.try_get("version_revision")?,
                    prompt: row.try_get("prompt")?,
                    allowed_sources,
                    agent_version_status: row.try_get("version_status")?,
                    catalog_version: row.try_get("catalog_version")?,
                    catalog_status: row.try_get("catalog_status")?,
                    pool_version: row.try_get("pool_version")?,
                    pool_status: row.try_get("pool_status")?,
                })
            })
            .collect()
    }

    async fn active_private_session(
        &mut self,
        session_id: &str,
        assignment_id: Option<&str>,
    ) -> Result<bool, Self::Error> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM ai_private_session_controls c \
             JOIN ai_private_sessions s ON s.id=c.session_id \
             WHERE s.id=$1 AND s.company_id=$2 AND s.user_id=$3 \
             AND s.assignment_id IS NOT DISTINCT FROM $4 AND c.status='active' \
             AND c.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)",
        )
        .bind(session_id)
        .bind(self.actor.company_id())
        .bind(self.actor.user_id())
        .bind(assignment_id)
        .fetch_one(&mut *self.connection)
        .await?)
    }

    async fn commit(
        &mut self,
        mutation: &AgentMutation,
        result: &Value,
    ) -> Result<(), Self::Error> {
        Self::require_transaction(&mut *self.connection).await?;
        sqlx::query(
            "SELECT id FROM organization_nodes WHERE id=$1 AND kind='company' AND status='active' FOR UPDATE",
        )
        .bind(self.actor.company_id())
        .execute(&mut *self.connection)
        .await?;
        self.insert_receipt(mutation, result).await?;
        let company = self.actor.company_id().to_owned();
        let now = mutation.common.now;
        let key = mutation.common.key.clone();
        let user = self.actor.user_id().to_owned();
        match &mutation.action {
            AgentMutationAction::CreateAgent { name, purpose } => {
                sqlx::query(
                    "INSERT INTO agent_catalog(id,company_id,name,purpose,status,created_by,version,created_at,updated_at) \
                     VALUES($1,$2,$3,$4,'draft',$5,1,$6,$6)",
                )
                .bind(&mutation.common.record_id)
                .bind(&company)
                .bind(name)
                .bind(purpose)
                .bind(&user)
                .bind(now)
                .execute(&mut *self.connection)
                .await
                .map_err(statement_error)?;
            }
            AgentMutationAction::CreateVersion {
                agent_id,
                number,
                prompt,
                allowed_sources_json,
            } => {
                self.guard_approved_version_for_agent(agent_id, &company)
                    .await?;
                sqlx::query(
                    "INSERT INTO agent_versions(id,agent_id,version_number,prompt,allowed_sources_json,prohibited_actions_json,status,created_by,version,created_at,updated_at) \
                     VALUES($1,$2,$3,$4,$5,'[\"approve\",\"sign\",\"pay\"]','draft',$6,1,$7,$7)",
                )
                .bind(&mutation.common.record_id)
                .bind(agent_id)
                .bind(number)
                .bind(prompt)
                .bind(allowed_sources_json)
                .bind(&user)
                .bind(now)
                .execute(&mut *self.connection)
                .await
                .map_err(statement_error)?;
            }
            AgentMutationAction::CreatePool {
                name,
                user_ids,
                member_ids,
            } => {
                sqlx::query(
                    "INSERT INTO agent_pools(id,company_id,name,status,created_by,version,created_at,updated_at) \
                     VALUES($1,$2,$3,'active',$4,1,$5,$5)",
                )
                .bind(&mutation.common.record_id)
                .bind(&company)
                .bind(name)
                .bind(&user)
                .bind(now)
                .execute(&mut *self.connection)
                .await
                .map_err(statement_error)?;
                for (user_id, member_id) in user_ids.iter().zip(member_ids) {
                    self.guard_eligible_member(&company, user_id).await?;
                    sqlx::query(
                        "INSERT INTO agent_pool_members(id,pool_id,user_id,created_by,created_at) \
                         VALUES($1,$2,$3,$4,$5)",
                    )
                    .bind(member_id)
                    .bind(&mutation.common.record_id)
                    .bind(user_id)
                    .bind(&user)
                    .bind(now)
                    .execute(&mut *self.connection)
                    .await
                    .map_err(statement_error)?;
                }
                let mut members = user_ids.clone();
                members.sort();
                let members_json = serde_json::to_string(&members).map_err(|_| conflict())?;
                sqlx::query(
                    "INSERT INTO agent_pool_revisions(id,pool_id,version,members_json,reason,created_by,created_at) \
                     VALUES($1,$2,1,$3,'Initial eligible routing members',$4,$5)",
                )
                .bind(format!("agent-pool-revision-{key}"))
                .bind(&mutation.common.record_id)
                .bind(members_json)
                .bind(&user)
                .bind(now)
                .execute(&mut *self.connection)
                .await
                .map_err(statement_error)?;
            }
            AgentMutationAction::ChangePool {
                action,
                expected_version,
                user_ids,
                member_ids,
                reason,
            } => {
                let row = sqlx::query(
                    "SELECT status,version FROM agent_pools WHERE id=$1 AND company_id=$2 FOR UPDATE",
                )
                .bind(&mutation.common.record_id)
                .bind(&company)
                .fetch_optional(&mut *self.connection)
                .await?;
                let Some(row) = row else {
                    return Err(conflict().into());
                };
                let status: String = row.try_get("status")?;
                let version: i64 = row.try_get("version")?;
                if status != "active" || version != *expected_version {
                    return Err(conflict().into());
                }
                if *action == crate::domain::governance::PoolAction::ReplaceMembers {
                    for user_id in user_ids {
                        self.guard_eligible_member(&company, user_id).await?;
                    }
                    let mut members = user_ids.clone();
                    members.sort();
                    sqlx::query(
                        "UPDATE agent_pool_members SET revoked_at=$1,version=version+1 \
                         WHERE pool_id=$2 AND revoked_at IS NULL AND NOT (user_id = ANY($3))",
                    )
                    .bind(now)
                    .bind(&mutation.common.record_id)
                    .bind(&members)
                    .execute(&mut *self.connection)
                    .await
                    .map_err(statement_error)?;
                    for (user_id, member_id) in user_ids.iter().zip(member_ids) {
                        sqlx::query(
                            "INSERT INTO agent_pool_members(id,pool_id,user_id,created_by,created_at) \
                             VALUES($1,$2,$3,$4,$5) ON CONFLICT(pool_id,user_id) DO UPDATE \
                             SET revoked_at=NULL,version=agent_pool_members.version+1 \
                             WHERE agent_pool_members.revoked_at IS NOT NULL",
                        )
                        .bind(member_id)
                        .bind(&mutation.common.record_id)
                        .bind(user_id)
                        .bind(&user)
                        .bind(now)
                        .execute(&mut *self.connection)
                        .await
                        .map_err(statement_error)?;
                    }
                    let members_json = serde_json::to_string(&members).map_err(|_| conflict())?;
                    sqlx::query(
                        "INSERT INTO agent_pool_revisions(id,pool_id,version,members_json,reason,created_by,created_at) \
                         VALUES($1,$2,$3,$4,$5,$6,$7)",
                    )
                    .bind(format!("agent-pool-revision-{key}"))
                    .bind(&mutation.common.record_id)
                    .bind(expected_version + 1)
                    .bind(members_json)
                    .bind(reason)
                    .bind(&user)
                    .bind(now)
                    .execute(&mut *self.connection)
                    .await
                    .map_err(statement_error)?;
                }
                sqlx::query(
                    "UPDATE agent_pools SET status=$1,version=version+1,updated_at=$2 WHERE id=$3",
                )
                .bind(
                    if *action == crate::domain::governance::PoolAction::Retire {
                        "retired"
                    } else {
                        "active"
                    },
                )
                .bind(now)
                .bind(&mutation.common.record_id)
                .execute(&mut *self.connection)
                .await
                .map_err(statement_error)?;
            }
            AgentMutationAction::CreateAssignment { binding } => {
                self.guard_binding(
                    &binding.agent_version_id,
                    binding.pool_id.as_deref(),
                    binding.user_id.as_deref(),
                    &company,
                )
                .await?;
                self.insert_assignment(&mutation.common.record_id, binding, &company)
                    .await?;
            }
            AgentMutationAction::DelegateAssignment {
                prior_id,
                prior_version,
                binding,
                ..
            } => {
                self.revoke_assignment(prior_id, *prior_version, &user, now)
                    .await?;
                self.guard_binding(
                    &binding.agent_version_id,
                    binding.pool_id.as_deref(),
                    binding.user_id.as_deref(),
                    &company,
                )
                .await?;
                self.insert_assignment(&mutation.common.record_id, binding, &company)
                    .await?;
            }
            AgentMutationAction::RetryAssignment {
                prior_id,
                prior_version,
                prior_status,
                binding,
                ..
            } => {
                if prior_status == "active" {
                    self.revoke_assignment(prior_id, *prior_version, &user, now)
                        .await?;
                } else if !self.prior_revoked(prior_id, *prior_version).await? {
                    return Err(conflict().into());
                }
                self.guard_binding(
                    &binding.agent_version_id,
                    binding.pool_id.as_deref(),
                    binding.user_id.as_deref(),
                    &company,
                )
                .await?;
                self.insert_assignment(&mutation.common.record_id, binding, &company)
                    .await?;
            }
            AgentMutationAction::CancelAssignment {
                expected_version, ..
            } => {
                self.revoke_assignment(&mutation.common.record_id, *expected_version, &user, now)
                    .await?;
            }
            AgentMutationAction::Lifecycle {
                kind,
                expected_version,
                expected_status,
                status,
            } => match kind {
                RecordKind::Agent => {
                    let changed = sqlx::query(
                            "UPDATE agent_catalog SET status=$1,reviewed_by=$2,version=version+1,updated_at=$3 \
                             WHERE id=$4 AND company_id=$5 AND version=$6 AND status=$7",
                        )
                        .bind(status)
                        .bind(&user)
                        .bind(now)
                        .bind(&mutation.common.record_id)
                        .bind(&company)
                        .bind(expected_version)
                        .bind(expected_status)
                        .execute(&mut *self.connection)
                        .await
                        .map_err(statement_error)?;
                    if changed.rows_affected() != 1 {
                        return Err(conflict().into());
                    }
                }
                RecordKind::Version => {
                    let changed = sqlx::query(
                            "UPDATE agent_versions v SET status=$1,reviewed_by=$2,version=v.version+1,updated_at=$3 \
                             FROM agent_catalog a WHERE v.id=$4 AND a.id=v.agent_id AND a.company_id=$5 \
                             AND v.version=$6 AND v.status=$7",
                        )
                        .bind(status)
                        .bind(&user)
                        .bind(now)
                        .bind(&mutation.common.record_id)
                        .bind(&company)
                        .bind(expected_version)
                        .bind(expected_status)
                        .execute(&mut *self.connection)
                        .await
                        .map_err(statement_error)?;
                    if changed.rows_affected() != 1 {
                        return Err(conflict().into());
                    }
                }
                RecordKind::Assignment => {
                    self.revoke_assignment(
                        &mutation.common.record_id,
                        *expected_version,
                        &user,
                        now,
                    )
                    .await?;
                }
            },
            AgentMutationAction::Session {
                action,
                expected_version,
                reason,
                manage,
            } => {
                let (status, legal_hold) = self
                    .guard_session(&mutation.common.record_id, *expected_version, *manage)
                    .await?;
                match action {
                    crate::domain::governance::PrivateSessionAction::Expire => {
                        if status != "active" {
                            return Err(conflict().into());
                        }
                        sqlx::query(
                            "UPDATE ai_private_session_controls SET status='expired',version=version+1,updated_at=$1 \
                             WHERE session_id=$2",
                        )
                        .bind(now)
                        .bind(&mutation.common.record_id)
                        .execute(&mut *self.connection)
                        .await
                        .map_err(statement_error)?;
                    }
                    crate::domain::governance::PrivateSessionAction::Hold => {
                        if legal_hold {
                            return Err(conflict().into());
                        }
                        sqlx::query(
                            "UPDATE ai_private_session_controls SET legal_hold=TRUE,hold_reason=$1,version=version+1,updated_at=$2 \
                             WHERE session_id=$3",
                        )
                        .bind(reason)
                        .bind(now)
                        .bind(&mutation.common.record_id)
                        .execute(&mut *self.connection)
                        .await
                        .map_err(statement_error)?;
                    }
                    crate::domain::governance::PrivateSessionAction::ReleaseHold => {
                        if !legal_hold {
                            return Err(conflict().into());
                        }
                        sqlx::query(
                            "UPDATE ai_private_session_controls SET legal_hold=FALSE,hold_reason=NULL,version=version+1,updated_at=$1 \
                             WHERE session_id=$2",
                        )
                        .bind(now)
                        .bind(&mutation.common.record_id)
                        .execute(&mut *self.connection)
                        .await
                        .map_err(statement_error)?;
                    }
                }
                sqlx::query(
                    "INSERT INTO ai_private_session_events(id,session_id,version,action,actor_id,reason,created_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7)",
                )
                .bind(format!(
                    "ai-private-session-event-{key}-{}",
                    action.as_str()
                ))
                .bind(&mutation.common.record_id)
                .bind(expected_version + 1)
                .bind(action.as_str())
                .bind(&user)
                .bind(reason)
                .bind(now)
                .execute(&mut *self.connection)
                .await
                .map_err(statement_error)?;
            }
        }
        insert_audit_and_outbox(&mut *self.connection, self.actor, mutation, result, now).await
    }
}

impl PgAgentGovernance<'_> {
    async fn guard_approved_version_for_agent(
        &mut self,
        agent_id: &str,
        company: &str,
    ) -> Result<(), AgentError> {
        self.guard(
            "SELECT EXISTS(SELECT 1 FROM agent_catalog WHERE id=$1 AND company_id=$2 AND status='approved')",
            &[agent_id, company],
        )
        .await
    }

    async fn insert_assignment(
        &mut self,
        id: &str,
        binding: &crate::application::governance::AssignmentBinding,
        company: &str,
    ) -> Result<(), AgentError> {
        sqlx::query(
            "INSERT INTO agent_assignments(id,company_id,agent_version_id,pool_id,user_id,scope_path,status,created_by,version,created_at,updated_at) \
             VALUES($1,$2,$3,$4,$5,$6,'active',$7,1,$8,$8)",
        )
        .bind(id)
        .bind(company)
        .bind(&binding.agent_version_id)
        .bind(binding.pool_id.as_deref())
        .bind(binding.user_id.as_deref())
        .bind(&binding.scope_path)
        .bind(self.actor.user_id())
        .bind(self.now)
        .execute(&mut *self.connection)
        .await
        .map_err(statement_error)?;
        Ok(())
    }

    async fn prior_revoked(&mut self, id: &str, expected_version: i64) -> Result<bool, AgentError> {
        let row = sqlx::query(
            "SELECT status,version FROM agent_assignments WHERE id=$1 AND company_id=$2 FOR UPDATE",
        )
        .bind(id)
        .bind(self.actor.company_id())
        .fetch_optional(&mut *self.connection)
        .await?;
        Ok(row.is_some_and(|row| {
            row.try_get::<String, _>("status")
                .is_ok_and(|status| status == "revoked")
                && row
                    .try_get::<i64, _>("version")
                    .is_ok_and(|version| version == expected_version)
        }))
    }

    async fn revoke_assignment(
        &mut self,
        id: &str,
        expected_version: i64,
        user: &str,
        now: i64,
    ) -> Result<(), AgentError> {
        let changed = sqlx::query(
            "UPDATE agent_assignments SET status='revoked',revoked_by=$1,version=version+1,updated_at=$2 \
             WHERE id=$3 AND company_id=$4 AND version=$5 AND status='active'",
        )
        .bind(user)
        .bind(now)
        .bind(id)
        .bind(self.actor.company_id())
        .bind(expected_version)
        .execute(&mut *self.connection)
        .await
        .map_err(statement_error)?;
        if changed.rows_affected() != 1 {
            return Err(conflict().into());
        }
        Ok(())
    }
}

async fn insert_audit_and_outbox(
    connection: &mut PgConnection,
    actor: &AgentActor,
    mutation: &AgentMutation,
    result: &Value,
    now: i64,
) -> Result<(), AgentError> {
    let operation = mutation.common.operation;
    let record_type = operation.split('.').take(2).collect::<Vec<_>>().join("_");
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,(SELECT authority_json FROM command_receipts WHERE command_id=$1))",
    )
    .bind(&mutation.common.key)
    .bind(actor.user_id())
    .bind(actor.assignment_id())
    .bind(operation)
    .bind(&record_type)
    .bind(&mutation.common.record_id)
    .bind(mutation.common.version)
    .bind(now)
    .execute(&mut *connection)
    .await
    .map_err(statement_error)?;
    let state = result
        .get("state")
        .and_then(Value::as_str)
        .or_else(|| result.get("status").and_then(Value::as_str));
    let scope = CommandEventScope::from(&actor.command);
    let payload = actor.command.outbox_payload(
        &mutation.common.record_id,
        mutation.common.version,
        state,
        &scope,
    )?;
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(&mutation.common.key)
    .bind(operation)
    .bind(&record_type)
    .bind(&mutation.common.record_id)
    .bind(mutation.common.version)
    .bind(payload.as_str())
    .bind(now)
    .execute(&mut *connection)
    .await
    .map_err(statement_error)?;
    Ok(())
}
