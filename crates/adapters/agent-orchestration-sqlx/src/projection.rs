//! Disposable Agent read-model projectors. Authoritative normalized state is
//! loaded from the tenant write plane for each opaque outbox fact; read-side
//! rows never become a command fallback.

use async_trait::async_trait;
use erpx_agent_orchestration::PortError;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::tenant::TenantBoundPool;

const ASSIGNMENTS: &str = "agent_orchestration.assignments";
const POOLS: &str = "agent_orchestration.pools";
const POOL_ROUTING: &str = "agent_orchestration.pool_routing";
const PRIVATE_SESSIONS: &str = "agent_orchestration.private_sessions";
const PROVIDER_INVOCATIONS: &str = "agent_orchestration.provider_invocations";
const ACTION_CARDS: &str = "agent_orchestration.action_cards";
const GOVERNANCE: &str = "agent_orchestration.governance";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectionKind {
    Assignments,
    Pools,
    PoolRouting,
    PrivateSessions,
    ProviderInvocations,
    ActionCards,
    Governance,
}

#[derive(Debug, Clone)]
pub struct AgentReadProjector {
    source: TenantBoundPool,
    kind: ProjectionKind,
    event_types: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AgentProjectionSet {
    assignments: AgentReadProjector,
    pools: AgentReadProjector,
    pool_routing: AgentReadProjector,
    private_sessions: AgentReadProjector,
    provider_invocations: AgentReadProjector,
    action_cards: AgentReadProjector,
    governance: AgentReadProjector,
}

impl AgentProjectionSet {
    pub fn try_new(write_pool: PgPool, tenant_id: Uuid) -> Result<Self, PortError> {
        let source = TenantBoundPool::try_new(write_pool, tenant_id)?;
        let projector = |kind, event_type: &str| AgentReadProjector {
            source: source.clone(),
            kind,
            event_types: vec![event_type.to_owned()],
        };
        Ok(Self {
            assignments: projector(
                ProjectionKind::Assignments,
                "agent.assignment.state_notification.requested",
            ),
            pools: projector(ProjectionKind::Pools, "agent.pool.changed"),
            pool_routing: projector(ProjectionKind::PoolRouting, "agent.pool.routed"),
            private_sessions: projector(ProjectionKind::PrivateSessions, "agent.session.changed"),
            provider_invocations: projector(
                ProjectionKind::ProviderInvocations,
                "agent.provider_invocation.changed",
            ),
            action_cards: projector(
                ProjectionKind::ActionCards,
                "agent.assignment.state_notification.requested",
            ),
            governance: projector(ProjectionKind::Governance, "agent.governance.changed"),
        })
    }

    pub const fn assignments(&self) -> &AgentReadProjector {
        &self.assignments
    }

    pub const fn pools(&self) -> &AgentReadProjector {
        &self.pools
    }

    pub const fn pool_routing(&self) -> &AgentReadProjector {
        &self.pool_routing
    }

    pub const fn private_sessions(&self) -> &AgentReadProjector {
        &self.private_sessions
    }

    pub const fn provider_invocations(&self) -> &AgentReadProjector {
        &self.provider_invocations
    }

    pub const fn action_cards(&self) -> &AgentReadProjector {
        &self.action_cards
    }

    pub const fn governance(&self) -> &AgentReadProjector {
        &self.governance
    }

    pub fn all(&self) -> [&AgentReadProjector; 7] {
        [
            self.assignments(),
            self.pools(),
            self.pool_routing(),
            self.private_sessions(),
            self.provider_invocations(),
            self.action_cards(),
            self.governance(),
        ]
    }
}

#[async_trait]
impl Projector for AgentReadProjector {
    fn name(&self) -> &str {
        match self.kind {
            ProjectionKind::Assignments => ASSIGNMENTS,
            ProjectionKind::Pools => POOLS,
            ProjectionKind::PoolRouting => POOL_ROUTING,
            ProjectionKind::PrivateSessions => PRIVATE_SESSIONS,
            ProjectionKind::ProviderInvocations => PROVIDER_INVOCATIONS,
            ProjectionKind::ActionCards => ACTION_CARDS,
            ProjectionKind::Governance => GOVERNANCE,
        }
    }

    fn event_types(&self) -> &[String] {
        &self.event_types
    }

    async fn apply(
        &self,
        read: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        if context.projection != self.name()
            || event.event_type != self.event_types[0]
            || event.aggregate_id.as_uuid().is_nil()
        {
            return Err(failure("agent projection fact does not match projector"));
        }
        match self.kind {
            ProjectionKind::Assignments => self.apply_assignment(read, context, event).await,
            ProjectionKind::Pools => self.apply_pool(read, context, event).await,
            ProjectionKind::PoolRouting => self.apply_pool_routing(read, context, event).await,
            ProjectionKind::PrivateSessions => self.apply_session(read, context, event).await,
            ProjectionKind::ProviderInvocations => {
                self.apply_provider_invocation(read, context, event).await
            }
            ProjectionKind::ActionCards => self.apply_action_cards(read, context, event).await,
            ProjectionKind::Governance => self.apply_governance(read, context, event).await,
        }
    }

    async fn reset(
        &self,
        read: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        if context.projection != self.name() {
            return Err(failure("agent projection reset targets another projector"));
        }
        let table = match self.kind {
            ProjectionKind::Assignments => "assignment_projection",
            ProjectionKind::Pools => "pool_directory_projection",
            ProjectionKind::PoolRouting => "pool_routing_projection",
            ProjectionKind::PrivateSessions => "private_session_projection",
            ProjectionKind::ProviderInvocations => "provider_invocation_projection",
            ProjectionKind::ActionCards => "action_card_projection",
            ProjectionKind::Governance => "governance_projection",
        };
        let statement = format!(
            "DELETE FROM erpx_agent_read.{table} WHERE projection_name=$1 AND generation=$2"
        );
        sqlx::query(&statement)
            .bind(context.projection)
            .bind(context.generation)
            .execute(&mut **read)
            .await
            .map_err(database_failure)?;
        Ok(())
    }
}

impl AgentReadProjector {
    async fn apply_assignment(
        &self,
        read: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let mut source = self.source.begin().await.map_err(port_failure)?;
        let row = sqlx::query(
            "SELECT a.scope_key,a.tenant_scope,a.root_assignment_id,a.parent_assignment_id,
                    a.root_initiating_principal_ref,d.agent_principal_ref,
                    a.agent_definition_id,a.agent_version_id,a.provider_ref,a.model_ref,
                    a.policy_digest,a.assignment_status,a.attempt,a.depth,a.queue_priority,
                    r.estimated_units,r.estimated_cost_micros,a.actual_units,
                    a.actual_cost_micros,a.failure_code,a.created_at,a.updated_at,
                    a.aggregate_version
             FROM erpx_agent_write.agent_assignments a
             JOIN erpx_agent_write.agent_definitions d
               ON d.tenant_id=a.tenant_id AND d.agent_definition_id=a.agent_definition_id
             JOIN erpx_agent_write.agent_budget_reservations r
               ON r.tenant_id=a.tenant_id AND r.reservation_id=a.budget_reservation_id
             WHERE a.tenant_id=$1 AND a.assignment_id=$2",
        )
        .bind(self.source.tenant_id())
        .bind(event.aggregate_id.as_uuid())
        .fetch_optional(&mut *source)
        .await
        .map_err(database_failure)?
        .ok_or_else(|| failure("agent assignment source is unavailable"))?;
        source.commit().await.map_err(database_failure)?;
        let source_version = ensure_version(&row, "aggregate_version", event.aggregate_version)?;
        sqlx::query(
            "INSERT INTO erpx_agent_read.assignment_projection
                (tenant_id,projection_name,generation,partition_key,assignment_id,
                 source_aggregate_version,source_global_sequence,scope_key,tenant_scope,
                 root_assignment_id,parent_assignment_id,initiating_principal_ref,
                 agent_principal_ref,agent_definition_id,agent_version_id,provider_ref,model_ref,
                 policy_digest,assignment_status,attempt,depth,queue_priority,estimated_units,
                 estimated_cost_micros,actual_units,actual_cost_micros,failure_code,created_at,
                 updated_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
                     $18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,now())
             ON CONFLICT (tenant_id,projection_name,generation,partition_key,assignment_id)
             DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
                 source_global_sequence=EXCLUDED.source_global_sequence,
                 assignment_status=EXCLUDED.assignment_status,attempt=EXCLUDED.attempt,
                 actual_units=EXCLUDED.actual_units,actual_cost_micros=EXCLUDED.actual_cost_micros,
                 failure_code=EXCLUDED.failure_code,updated_at=EXCLUDED.updated_at,projected_at=now()
             WHERE erpx_agent_read.assignment_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(self.source.tenant_id())
        .bind(context.projection)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(event.aggregate_id.as_uuid())
        .bind(to_i64(source_version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(value::<String>(&row, "scope_key")?)
        .bind(value::<serde_json::Value>(&row, "tenant_scope")?)
        .bind(value::<Uuid>(&row, "root_assignment_id")?)
        .bind(value::<Option<Uuid>>(&row, "parent_assignment_id")?)
        .bind(value::<String>(&row, "root_initiating_principal_ref")?)
        .bind(value::<String>(&row, "agent_principal_ref")?)
        .bind(value::<Uuid>(&row, "agent_definition_id")?)
        .bind(value::<Uuid>(&row, "agent_version_id")?)
        .bind(value::<String>(&row, "provider_ref")?)
        .bind(value::<String>(&row, "model_ref")?)
        .bind(value::<String>(&row, "policy_digest")?)
        .bind(value::<String>(&row, "assignment_status")?)
        .bind(value::<i32>(&row, "attempt")?)
        .bind(value::<i32>(&row, "depth")?)
        .bind(value::<i32>(&row, "queue_priority")?)
        .bind(value::<i64>(&row, "estimated_units")?)
        .bind(value::<i64>(&row, "estimated_cost_micros")?)
        .bind(value::<Option<i64>>(&row, "actual_units")?)
        .bind(value::<Option<i64>>(&row, "actual_cost_micros")?)
        .bind(value::<Option<String>>(&row, "failure_code")?)
        .bind(value::<chrono::DateTime<chrono::Utc>>(&row, "created_at")?)
        .bind(value::<chrono::DateTime<chrono::Utc>>(&row, "updated_at")?)
        .execute(&mut **read)
        .await
        .map_err(database_failure)?;
        Ok(())
    }

    async fn apply_pool(
        &self,
        read: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let mut source = self.source.begin().await.map_err(port_failure)?;
        let row = sqlx::query(
            "SELECT p.scope_key,p.pool_name,p.pool_version,p.enabled,p.updated_at,
                    array_agg(m.agent_version_id ORDER BY m.agent_version_id) AS members
             FROM erpx_agent_write.agent_pools p
             JOIN erpx_agent_write.agent_pool_members m
               ON m.tenant_id=p.tenant_id AND m.agent_pool_id=p.agent_pool_id
             WHERE p.tenant_id=$1 AND p.agent_pool_id=$2
             GROUP BY p.scope_key,p.pool_name,p.pool_version,p.enabled,p.updated_at",
        )
        .bind(self.source.tenant_id())
        .bind(event.aggregate_id.as_uuid())
        .fetch_optional(&mut *source)
        .await
        .map_err(database_failure)?
        .ok_or_else(|| failure("agent pool source is unavailable"))?;
        source.commit().await.map_err(database_failure)?;
        let source_version = ensure_version(&row, "pool_version", event.aggregate_version)?;
        sqlx::query(
            "INSERT INTO erpx_agent_read.pool_directory_projection
                (tenant_id,projection_name,generation,partition_key,agent_pool_id,
                 source_aggregate_version,source_global_sequence,scope_key,pool_name,
                 member_version_ids,enabled,updated_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,now())
             ON CONFLICT (tenant_id,projection_name,generation,partition_key,agent_pool_id)
             DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
                 source_global_sequence=EXCLUDED.source_global_sequence,
                 member_version_ids=EXCLUDED.member_version_ids,enabled=EXCLUDED.enabled,
                 updated_at=EXCLUDED.updated_at,projected_at=now()
             WHERE erpx_agent_read.pool_directory_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(self.source.tenant_id())
        .bind(context.projection)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(event.aggregate_id.as_uuid())
        .bind(to_i64(source_version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(value::<String>(&row, "scope_key")?)
        .bind(value::<String>(&row, "pool_name")?)
        .bind(value::<Vec<Uuid>>(&row, "members")?)
        .bind(value::<bool>(&row, "enabled")?)
        .bind(value::<chrono::DateTime<chrono::Utc>>(&row, "updated_at")?)
        .execute(&mut **read)
        .await
        .map_err(database_failure)?;
        Ok(())
    }

    async fn apply_pool_routing(
        &self,
        read: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let mut source = self.source.begin().await.map_err(port_failure)?;
        let row = sqlx::query(
            "SELECT p.scope_key,r.requested_scope,r.agent_pool_id,r.root_action_ref,r.capability,
                    r.selected_agent_version_ids,r.compare_requested,r.decision_digest,r.decided_at
             FROM erpx_agent_write.agent_pool_routing_decisions r
             JOIN erpx_agent_write.agent_pools p
               ON p.tenant_id=r.tenant_id AND p.agent_pool_id=r.agent_pool_id
             WHERE r.tenant_id=$1 AND r.routing_decision_id=$2",
        )
        .bind(self.source.tenant_id())
        .bind(event.aggregate_id.as_uuid())
        .fetch_optional(&mut *source)
        .await
        .map_err(database_failure)?
        .ok_or_else(|| failure("agent routing source is unavailable"))?;
        source.commit().await.map_err(database_failure)?;
        sqlx::query(
            "INSERT INTO erpx_agent_read.pool_routing_projection
                (tenant_id,projection_name,generation,partition_key,routing_decision_id,
                 source_aggregate_version,source_global_sequence,scope_key,requested_scope,
                 agent_pool_id,root_action_ref,capability,selected_agent_version_ids,
                 compare_requested,decision_digest,decided_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,now())
             ON CONFLICT (tenant_id,projection_name,generation,partition_key,routing_decision_id)
             DO NOTHING",
        )
        .bind(self.source.tenant_id())
        .bind(context.projection)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(event.aggregate_id.as_uuid())
        .bind(to_i64(event.aggregate_version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(value::<String>(&row, "scope_key")?)
        .bind(value::<serde_json::Value>(&row, "requested_scope")?)
        .bind(value::<Uuid>(&row, "agent_pool_id")?)
        .bind(value::<String>(&row, "root_action_ref")?)
        .bind(value::<String>(&row, "capability")?)
        .bind(value::<Vec<Uuid>>(&row, "selected_agent_version_ids")?)
        .bind(value::<bool>(&row, "compare_requested")?)
        .bind(value::<String>(&row, "decision_digest")?)
        .bind(value::<chrono::DateTime<chrono::Utc>>(&row, "decided_at")?)
        .execute(&mut **read)
        .await
        .map_err(database_failure)?;
        Ok(())
    }

    async fn apply_session(
        &self,
        read: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let mut source = self.source.begin().await.map_err(port_failure)?;
        let row = sqlx::query(
            "SELECT s.scope_key,s.owner_human_ref,s.record_id,s.expires_at,s.legal_hold,
                    s.expired_at,s.session_version,s.updated_at,COUNT(m.session_message_id)::int AS messages,
                    COUNT(m.published_at)::int AS published
             FROM erpx_agent_write.agent_sessions s
             LEFT JOIN erpx_agent_write.agent_session_messages m
               ON m.tenant_id=s.tenant_id AND m.agent_session_id=s.agent_session_id
             WHERE s.tenant_id=$1 AND s.agent_session_id=$2
             GROUP BY s.scope_key,s.owner_human_ref,s.record_id,s.expires_at,s.legal_hold,
                      s.expired_at,s.session_version,s.updated_at",
        )
        .bind(self.source.tenant_id())
        .bind(event.aggregate_id.as_uuid())
        .fetch_optional(&mut *source)
        .await
        .map_err(database_failure)?
        .ok_or_else(|| failure("agent session source is unavailable"))?;
        source.commit().await.map_err(database_failure)?;
        let source_version = ensure_version(&row, "session_version", event.aggregate_version)?;
        sqlx::query(
            "INSERT INTO erpx_agent_read.private_session_projection
                (tenant_id,projection_name,generation,partition_key,agent_session_id,
                 source_aggregate_version,source_global_sequence,scope_key,owner_human_ref,
                 record_id,expires_at,legal_hold,expired_at,message_count,published_message_count,
                 updated_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,now())
             ON CONFLICT (tenant_id,projection_name,generation,partition_key,agent_session_id)
             DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
                 source_global_sequence=EXCLUDED.source_global_sequence,
                 expires_at=EXCLUDED.expires_at,legal_hold=EXCLUDED.legal_hold,
                 expired_at=EXCLUDED.expired_at,message_count=EXCLUDED.message_count,
                 published_message_count=EXCLUDED.published_message_count,
                 updated_at=EXCLUDED.updated_at,projected_at=now()
             WHERE erpx_agent_read.private_session_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(self.source.tenant_id())
        .bind(context.projection)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(event.aggregate_id.as_uuid())
        .bind(to_i64(source_version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(value::<String>(&row, "scope_key")?)
        .bind(value::<String>(&row, "owner_human_ref")?)
        .bind(value::<Option<Uuid>>(&row, "record_id")?)
        .bind(value::<chrono::DateTime<chrono::Utc>>(&row, "expires_at")?)
        .bind(value::<bool>(&row, "legal_hold")?)
        .bind(value::<Option<chrono::DateTime<chrono::Utc>>>(
            &row,
            "expired_at",
        )?)
        .bind(value::<i32>(&row, "messages")?)
        .bind(value::<i32>(&row, "published")?)
        .bind(value::<chrono::DateTime<chrono::Utc>>(&row, "updated_at")?)
        .execute(&mut **read)
        .await
        .map_err(database_failure)?;
        Ok(())
    }

    async fn apply_provider_invocation(
        &self,
        read: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let mut source = self.source.begin().await.map_err(port_failure)?;
        let row = sqlx::query(
            "SELECT a.scope_key,a.tenant_scope,e.assignment_id,e.attempt,e.provider_ref,e.model_ref,
                    e.policy_version,e.policy_digest,e.payload_digest,e.output_digest,
                    e.provider_units,e.provider_cost_micros,e.outcome,e.invoked_at,e.completed_at,
                    e.evidence_version
             FROM erpx_agent_write.provider_invocation_evidence e
             JOIN erpx_agent_write.agent_assignments a
               ON a.tenant_id=e.tenant_id AND a.assignment_id=e.assignment_id
             WHERE e.tenant_id=$1 AND e.invocation_id=$2",
        )
        .bind(self.source.tenant_id())
        .bind(event.aggregate_id.as_uuid())
        .fetch_optional(&mut *source)
        .await
        .map_err(database_failure)?
        .ok_or_else(|| failure("agent provider invocation source is unavailable"))?;
        source.commit().await.map_err(database_failure)?;
        let source_version = ensure_version(&row, "evidence_version", event.aggregate_version)?;
        sqlx::query(
            "INSERT INTO erpx_agent_read.provider_invocation_projection
                (tenant_id,projection_name,generation,partition_key,invocation_id,
                 source_aggregate_version,source_global_sequence,scope_key,tenant_scope,
                 assignment_id,attempt,provider_ref,model_ref,policy_version,policy_digest,
                 payload_digest,output_digest,provider_units,provider_cost_micros,outcome,
                 invoked_at,completed_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
                     $18,$19,$20,$21,$22,now())
             ON CONFLICT (tenant_id,projection_name,generation,partition_key,invocation_id)
             DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
                 source_global_sequence=EXCLUDED.source_global_sequence,
                 output_digest=EXCLUDED.output_digest,provider_units=EXCLUDED.provider_units,
                 provider_cost_micros=EXCLUDED.provider_cost_micros,outcome=EXCLUDED.outcome,
                 completed_at=EXCLUDED.completed_at,projected_at=now()
             WHERE erpx_agent_read.provider_invocation_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(self.source.tenant_id())
        .bind(context.projection)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(event.aggregate_id.as_uuid())
        .bind(to_i64(source_version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(value::<String>(&row, "scope_key")?)
        .bind(value::<serde_json::Value>(&row, "tenant_scope")?)
        .bind(value::<Uuid>(&row, "assignment_id")?)
        .bind(value::<i32>(&row, "attempt")?)
        .bind(value::<String>(&row, "provider_ref")?)
        .bind(value::<String>(&row, "model_ref")?)
        .bind(value::<i64>(&row, "policy_version")?)
        .bind(value::<String>(&row, "policy_digest")?)
        .bind(value::<String>(&row, "payload_digest")?)
        .bind(value::<Option<String>>(&row, "output_digest")?)
        .bind(value::<Option<i64>>(&row, "provider_units")?)
        .bind(value::<Option<i64>>(&row, "provider_cost_micros")?)
        .bind(value::<Option<String>>(&row, "outcome")?)
        .bind(value::<chrono::DateTime<chrono::Utc>>(&row, "invoked_at")?)
        .bind(value::<Option<chrono::DateTime<chrono::Utc>>>(
            &row,
            "completed_at",
        )?)
        .execute(&mut **read)
        .await
        .map_err(database_failure)?;
        Ok(())
    }

    async fn apply_action_cards(
        &self,
        read: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let mut source = self.source.begin().await.map_err(port_failure)?;
        let assignment_version: i64 = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_agent_write.agent_assignments
             WHERE tenant_id=$1 AND assignment_id=$2",
        )
        .bind(self.source.tenant_id())
        .bind(event.aggregate_id.as_uuid())
        .fetch_optional(&mut *source)
        .await
        .map_err(database_failure)?
        .ok_or_else(|| failure("agent action-card source is unavailable"))?;
        let source_version = u64::try_from(assignment_version)
            .map_err(|_| failure("agent action-card source version is invalid"))?;
        if source_version < event.aggregate_version {
            return Err(failure("agent action-card source version is stale"));
        }
        let rows = sqlx::query(
            "SELECT a.scope_key,c.action_card_id,c.agent_principal_ref,
                    c.owner_context,c.command_type,c.action_status,c.created_at,c.resolved_at
             FROM erpx_agent_write.agent_assignments a
             JOIN erpx_agent_write.agent_action_cards c
               ON c.tenant_id=a.tenant_id AND c.assignment_id=a.assignment_id
             WHERE a.tenant_id=$1 AND a.assignment_id=$2 ORDER BY c.action_card_id",
        )
        .bind(self.source.tenant_id())
        .bind(event.aggregate_id.as_uuid())
        .fetch_all(&mut *source)
        .await
        .map_err(database_failure)?;
        for row in rows {
            sqlx::query(
                "INSERT INTO erpx_agent_read.action_card_projection
                    (tenant_id,projection_name,generation,partition_key,action_card_id,
                     source_aggregate_version,source_global_sequence,scope_key,assignment_id,
                     agent_principal_ref,owner_context,command_type,action_status,created_at,
                     resolved_at,projected_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,now())
                 ON CONFLICT (tenant_id,projection_name,generation,partition_key,action_card_id)
                 DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
                     source_global_sequence=EXCLUDED.source_global_sequence,
                     action_status=EXCLUDED.action_status,resolved_at=EXCLUDED.resolved_at,
                     projected_at=now()
                 WHERE erpx_agent_read.action_card_projection.source_aggregate_version
                       < EXCLUDED.source_aggregate_version",
            )
            .bind(self.source.tenant_id())
            .bind(context.projection)
            .bind(context.generation)
            .bind(&event.partition)
            .bind(value::<Uuid>(&row, "action_card_id")?)
            .bind(to_i64(source_version)?)
            .bind(to_i64(event.global_sequence)?)
            .bind(value::<String>(&row, "scope_key")?)
            .bind(event.aggregate_id.as_uuid())
            .bind(value::<String>(&row, "agent_principal_ref")?)
            .bind(value::<String>(&row, "owner_context")?)
            .bind(value::<String>(&row, "command_type")?)
            .bind(value::<String>(&row, "action_status")?)
            .bind(value::<chrono::DateTime<chrono::Utc>>(&row, "created_at")?)
            .bind(value::<Option<chrono::DateTime<chrono::Utc>>>(
                &row,
                "resolved_at",
            )?)
            .execute(&mut **read)
            .await
            .map_err(database_failure)?;
        }
        source.commit().await.map_err(database_failure)?;
        Ok(())
    }

    async fn apply_governance(
        &self,
        read: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let mut source = self.source.begin().await.map_err(port_failure)?;
        let row = sqlx::query(
            "SELECT scope_key,tenant_scope,record_kind,subject_record_id,policy_digest,
                    lifecycle_state,aggregate_version,updated_at
             FROM erpx_agent_write.agent_governance_records
             WHERE tenant_id=$1 AND record_id=$2",
        )
        .bind(self.source.tenant_id())
        .bind(event.aggregate_id.as_uuid())
        .fetch_optional(&mut *source)
        .await
        .map_err(database_failure)?
        .ok_or_else(|| failure("agent governance source is unavailable"))?;
        source.commit().await.map_err(database_failure)?;
        let source_version = ensure_version(&row, "aggregate_version", event.aggregate_version)?;
        if value::<String>(&row, "record_kind")? != event.aggregate_type {
            return Err(failure("agent governance kind does not match event"));
        }
        sqlx::query(
            "INSERT INTO erpx_agent_read.governance_projection
                (tenant_id,projection_name,generation,partition_key,record_id,
                 source_aggregate_version,source_global_sequence,scope_key,tenant_scope,
                 record_kind,subject_record_id,policy_digest,lifecycle_state,updated_at,
                 projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,now())
             ON CONFLICT (tenant_id,projection_name,generation,partition_key,record_id)
             DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
                 source_global_sequence=EXCLUDED.source_global_sequence,
                 policy_digest=EXCLUDED.policy_digest,lifecycle_state=EXCLUDED.lifecycle_state,
                 updated_at=EXCLUDED.updated_at,projected_at=now()
             WHERE erpx_agent_read.governance_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(self.source.tenant_id())
        .bind(context.projection)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(event.aggregate_id.as_uuid())
        .bind(to_i64(source_version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(value::<String>(&row, "scope_key")?)
        .bind(value::<serde_json::Value>(&row, "tenant_scope")?)
        .bind(value::<String>(&row, "record_kind")?)
        .bind(value::<Uuid>(&row, "subject_record_id")?)
        .bind(value::<String>(&row, "policy_digest")?)
        .bind(value::<String>(&row, "lifecycle_state")?)
        .bind(value::<chrono::DateTime<chrono::Utc>>(&row, "updated_at")?)
        .execute(&mut **read)
        .await
        .map_err(database_failure)?;
        Ok(())
    }
}

fn ensure_version(
    row: &sqlx::postgres::PgRow,
    column: &str,
    expected: u64,
) -> Result<u64, ProjectionFailure> {
    let actual: i64 = row.try_get(column).map_err(database_failure)?;
    let actual =
        u64::try_from(actual).map_err(|_| failure("agent projection source version is invalid"))?;
    if actual < expected {
        return Err(failure("agent projection source version is stale"));
    }
    Ok(actual)
}

fn value<T>(row: &sqlx::postgres::PgRow, column: &str) -> Result<T, ProjectionFailure>
where
    for<'r> T: sqlx::Decode<'r, Postgres> + sqlx::Type<Postgres>,
{
    row.try_get(column).map_err(database_failure)
}

fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| failure("agent projection sequence exceeds storage range"))
}

fn database_failure(_: sqlx::Error) -> ProjectionFailure {
    failure("agent projection persistence failed")
}

fn port_failure(_: PortError) -> ProjectionFailure {
    failure("agent projection source is unavailable")
}

fn failure(message: &'static str) -> ProjectionFailure {
    ProjectionFailure::new(message)
}

#[cfg(test)]
mod tests {
    use sqlx::postgres::PgPoolOptions;

    use super::*;

    #[tokio::test]
    async fn projection_set_is_tenant_bound_and_covers_durable_agent_facts() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://erpx@127.0.0.1/erpx_agent_projection_contract")
            .expect("lazy pool");
        assert!(matches!(
            AgentProjectionSet::try_new(pool.clone(), Uuid::nil()),
            Err(PortError::Denied)
        ));
        let set = AgentProjectionSet::try_new(pool, Uuid::from_u128(711)).expect("projectors");
        let names = set
            .all()
            .into_iter()
            .map(Projector::name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                ASSIGNMENTS,
                POOLS,
                POOL_ROUTING,
                PRIVATE_SESSIONS,
                PROVIDER_INVOCATIONS,
                ACTION_CARDS,
                GOVERNANCE,
            ]
        );
        assert!(
            set.all()
                .into_iter()
                .all(|projector| projector.event_types().len() == 1)
        );
    }

    #[test]
    fn projection_source_never_reads_full_aggregate_or_plaintext_material() {
        let source = include_str!("projection.rs");
        assert!(!source.contains(concat!("aggregate", "_state")));
        assert!(!source.contains(concat!("assignment", "_snapshots")));
        assert!(!source.contains(concat!("definition", "_snapshots")));
        assert!(!source.contains(concat!("encrypted", "_prompt_ref")));
        assert!(!source.contains(concat!("encrypted", "_output_ref")));
        assert!(!source.contains(concat!("encrypted", "_payload_ref")));
        assert!(source.contains("source_aggregate_version"));
        assert!(source.contains("ON CONFLICT"));
    }
}
