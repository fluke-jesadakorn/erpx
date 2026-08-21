#![doc = "PostgreSQL CQRS adapters for the Planning bounded context."]
#![forbid(unsafe_code)]

mod projection;
pub use projection::{PlanningProjector, PlanningRunSnapshot};

mod directory;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs_sqlx::ReadPool;
use erpx_planning::{
    AuthorizedPlanningAccess, CommitReceipt, PlanningAggregate, PlanningCommit,
    PlanningGovernedRecord, PlanningPlan, PlanningReadRepository, PlanningRecommendation,
    PlanningRecordProjection, PlanningRun, PlanningRunProjection, PlanningScenario,
    PlanningWriteRepository, PortError, RecommendationId, RunId, ScenarioId,
};
use sqlx::{PgPool, Postgres, Row, Transaction};

pub const EVENT_VERSION: u32 = 1;
pub const PROJECTION_NAME: &str = "planning.runs";
pub const SNAPSHOT_EVENT_TYPE: &str = "planning.run.snapshot.v1";
pub const PLAN_SNAPSHOT_EVENT_TYPE: &str = "planning.plan.snapshot.v1";
pub const GOVERNED_RECORD_SNAPSHOT_EVENT_TYPE: &str = "planning.governed-record.snapshot.v1";

#[derive(Debug, Clone)]
pub struct SqlxPlanningWriteRepository {
    pool: PgPool,
}
impl SqlxPlanningWriteRepository {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PlanningWriteRepository for SqlxPlanningWriteRepository {
    async fn load_scenario(
        &self,
        authority: &AuthorizedPlanningAccess,
        id: ScenarioId,
    ) -> Result<Option<PlanningScenario>, PortError> {
        load_aggregate(&self.pool, authority, "scenario", id.as_uuid()).await
    }
    async fn load_run(
        &self,
        authority: &AuthorizedPlanningAccess,
        id: RunId,
    ) -> Result<Option<PlanningRun>, PortError> {
        load_aggregate(&self.pool, authority, "run", id.as_uuid()).await
    }
    async fn load_recommendation(
        &self,
        authority: &AuthorizedPlanningAccess,
        id: RecommendationId,
    ) -> Result<Option<PlanningRecommendation>, PortError> {
        load_aggregate(&self.pool, authority, "recommendation", id.as_uuid()).await
    }
    async fn load_plan(
        &self,
        authority: &AuthorizedPlanningAccess,
        id: uuid::Uuid,
    ) -> Result<Option<PlanningPlan>, PortError> {
        load_plan_any_kind(&self.pool, authority, id).await
    }
    async fn commit(&self, request: PlanningCommit) -> Result<CommitReceipt, PortError> {
        request.identity.validate()?;
        if request.aggregates.is_empty()
            || !request
                .authority
                .actor
                .authority_scope
                .contains(&request.authority.target_scope)
        {
            return Err(PortError::Denied);
        }
        let holding_id = request.authority.actor.holding_id;
        let keys: Vec<String> = request
            .aggregates
            .iter()
            .map(PlanningAggregate::key)
            .collect();
        let versions = request
            .aggregates
            .iter()
            .map(|value| i64::try_from(value.version()).map_err(|_| PortError::InvalidState))
            .collect::<Result<Vec<_>, _>>()?;
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        set_holding(&mut tx, holding_id.as_uuid()).await?;
        if let Some(row) = sqlx::query("SELECT command_id,request_digest,aggregate_keys,aggregate_versions,committed_at FROM erpx_planning_write.command_receipts WHERE holding_id=$1 AND idempotency_key=$2").bind(holding_id.as_uuid()).bind(&request.identity.idempotency_key).fetch_optional(&mut *tx).await.map_err(map_error)? {
            let command_id: uuid::Uuid = row.try_get("command_id").map_err(|_| PortError::Internal)?;
            let digest: String = row.try_get("request_digest").map_err(|_| PortError::Internal)?;
            let stored_keys: Vec<String> = row.try_get("aggregate_keys").map_err(|_| PortError::Internal)?;
            let stored_versions: Vec<i64> = row.try_get("aggregate_versions").map_err(|_| PortError::Internal)?;
            if command_id != request.identity.command_id.as_uuid() || digest != request.identity.request_digest || stored_keys != keys || stored_versions != versions { return Err(PortError::Conflict); }
            let committed_at = row.try_get("committed_at").map_err(|_| PortError::Internal)?; tx.commit().await.map_err(map_error)?;
            let aggregate_versions = stored_versions
                .into_iter()
                .map(|version| u64::try_from(version).map_err(|_| PortError::Internal))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(CommitReceipt { aggregate_keys: stored_keys, aggregate_versions, committed_at, replayed: true });
        }
        // This command can lock several aggregates before emitting several
        // shared outbox rows. Take the same commit-order lock up front so a
        // reversed aggregate batch cannot form row-lock -> outbox-lock and
        // outbox-lock -> row-lock edges across two transactions.
        sqlx::query("SELECT erpx_cqrs_write.lock_outbox_commit_order()")
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;
        for (index, aggregate) in request.aggregates.iter().enumerate() {
            let (kind, id, document) = aggregate_parts(aggregate)?;
            let projection_document = document.clone();
            let key = aggregate.key();
            let version = aggregate.version();
            let current = sqlx::query_scalar::<_, i64>("SELECT aggregate_version FROM erpx_planning_write.aggregate_versions WHERE holding_id=$1 AND aggregate_key=$2 FOR UPDATE").bind(holding_id.as_uuid()).bind(&key).fetch_optional(&mut *tx).await.map_err(map_error)?;
            let expected = if index == 0 {
                request.expected_version
            } else {
                None
            };
            match (expected, current) {
                (None, None) if version == 1 => {}
                (Some(expected), Some(actual))
                    if i64::try_from(expected).ok() == Some(actual)
                        && version == expected.checked_add(1).ok_or(PortError::Conflict)? => {}
                _ => return Err(PortError::Conflict),
            }
            let scope = serde_json::to_value(&request.authority.target_scope)
                .map_err(|_| PortError::InvalidState)?;
            sqlx::query("SELECT erpx_planning_write.persist_normalized($1,$2,$3,$4,$5,$6,$7)")
                .bind(holding_id.as_uuid())
                .bind(&key)
                .bind(kind)
                .bind(id)
                .bind(scope)
                .bind(i64::try_from(version).map_err(|_| PortError::InvalidState)?)
                .bind(document)
                .execute(&mut *tx)
                .await
                .map_err(map_error)?;
            sqlx::query("INSERT INTO erpx_planning_write.outbox (holding_id,aggregate_key,aggregate_version,event_type,event_version,correlation_id,payload) VALUES ($1,$2,$3,$4,$5,$6,$7)")
                .bind(holding_id.as_uuid()).bind(&key).bind(i64::try_from(version).map_err(|_| PortError::InvalidState)?).bind(&request.event_type).bind(i32::try_from(EVENT_VERSION).map_err(|_| PortError::InvalidState)?).bind(request.correlation_id.as_uuid()).bind(&request.event_payload).execute(&mut *tx).await.map_err(map_error)?;
            if kind == "run" {
                let actor = serde_json::json!({"principal_ref":format!("principal:{}",request.authority.actor.principal_id.as_uuid()),"kind":"holding_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
                let event_id =
                    uuid::Uuid::new_v5(&request.identity.command_id.as_uuid(), id.as_bytes());
                sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES ($1,$2,'planning',$3,$4,$5,1,'run',$6,$7,$8,$9,$10,NULL,$11,clock_timestamp(),clock_timestamp())")
                    .bind(holding_id.as_uuid()).bind(event_id).bind(format!("planning.run:{}",id.simple())).bind(request.identity.command_id.as_uuid()).bind(SNAPSHOT_EVENT_TYPE).bind(id).bind(i64::try_from(version).map_err(|_|PortError::InvalidState)?).bind(serde_json::to_value(&request.authority.target_scope).map_err(|_|PortError::InvalidState)?).bind(actor).bind(request.correlation_id.as_uuid()).bind(serde_json::json!({"run":projection_document})).execute(&mut *tx).await.map_err(map_error)?;
            } else if matches!(kind, "demand_plan" | "supply_plan" | "capacity_plan") {
                let actor = serde_json::json!({"principal_ref":format!("principal:{}",request.authority.actor.principal_id.as_uuid()),"kind":"holding_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
                let event_id = uuid::Uuid::new_v5(
                    &request.identity.command_id.as_uuid(),
                    format!("plan:{id}").as_bytes(),
                );
                sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events(holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at)VALUES($1,$2,'planning',$3,$4,$5,1,$6,$7,$8,$9,$10,$11,NULL,$12,clock_timestamp(),clock_timestamp())")
                    .bind(holding_id.as_uuid()).bind(event_id).bind(format!("planning.plan:{}",id.simple())).bind(request.identity.command_id.as_uuid()).bind(PLAN_SNAPSHOT_EVENT_TYPE).bind(kind).bind(id).bind(i64::try_from(version).map_err(|_|PortError::InvalidState)?).bind(serde_json::to_value(&request.authority.target_scope).map_err(|_|PortError::InvalidState)?).bind(actor).bind(request.correlation_id.as_uuid()).bind(serde_json::json!({"plan":projection_document})).execute(&mut *tx).await.map_err(map_error)?;
            } else if matches!(kind, "scenario" | "recommendation") {
                let actor = serde_json::json!({"principal_ref":format!("principal:{}",request.authority.actor.principal_id.as_uuid()),"kind":"holding_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
                let event_id = uuid::Uuid::new_v5(
                    &request.identity.command_id.as_uuid(),
                    format!("governed:{kind}:{id}").as_bytes(),
                );
                sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events(holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES($1,$2,'planning',$3,$4,$5,1,$6,$7,$8,$9,$10,$11,NULL,$12,clock_timestamp(),clock_timestamp())")
                    .bind(holding_id.as_uuid()).bind(event_id).bind(format!("planning.record:{}",id.simple())).bind(request.identity.command_id.as_uuid()).bind(GOVERNED_RECORD_SNAPSHOT_EVENT_TYPE).bind(kind).bind(id).bind(i64::try_from(version).map_err(|_|PortError::InvalidState)?).bind(serde_json::to_value(&request.authority.target_scope).map_err(|_|PortError::InvalidState)?).bind(actor).bind(request.correlation_id.as_uuid()).bind(serde_json::json!({"record_kind":kind,"record":projection_document})).execute(&mut *tx).await.map_err(map_error)?;
            }
            if let Some(draft) =
                directory::planning_directory_draft(aggregate, &request.authority.target_scope)?
            {
                let actor = serde_json::json!({"principal_ref":format!("principal:{}",request.authority.actor.principal_id.as_uuid()),"kind":"holding_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
                directory::append_directory_draft(
                    &mut tx,
                    holding_id,
                    request.identity.command_id,
                    request.correlation_id,
                    actor,
                    &draft,
                )
                .await?;
            }
        }
        sqlx::query("INSERT INTO erpx_planning_write.audit_log (holding_id,command_id,actor_principal_id,action,authority_scope,aggregate_keys,aggregate_versions,correlation_id,evidence_digest) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(holding_id.as_uuid()).bind(request.identity.command_id.as_uuid()).bind(request.authority.actor.principal_id.as_uuid())
            .bind(&request.event_type).bind(serde_json::to_value(&request.authority.target_scope).map_err(|_|PortError::InvalidState)?)
            .bind(&keys).bind(&versions).bind(request.correlation_id.as_uuid()).bind(&request.identity.request_digest)
            .execute(&mut *tx).await.map_err(map_error)?;
        let committed_at: DateTime<Utc> = sqlx::query_scalar("INSERT INTO erpx_planning_write.command_receipts (holding_id,idempotency_key,command_id,request_digest,aggregate_keys,aggregate_versions) VALUES ($1,$2,$3,$4,$5,$6) RETURNING committed_at").bind(holding_id.as_uuid()).bind(&request.identity.idempotency_key).bind(request.identity.command_id.as_uuid()).bind(&request.identity.request_digest).bind(&keys).bind(&versions).fetch_one(&mut *tx).await.map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        Ok(CommitReceipt {
            aggregate_keys: keys,
            aggregate_versions: versions
                .into_iter()
                .map(|version| u64::try_from(version).map_err(|_| PortError::Internal))
                .collect::<Result<Vec<_>, _>>()?,
            committed_at,
            replayed: false,
        })
    }
}

fn aggregate_parts(
    value: &PlanningAggregate,
) -> Result<(&'static str, uuid::Uuid, serde_json::Value), PortError> {
    match value {
        PlanningAggregate::Scenario(value) => Ok((
            "scenario",
            value.id().as_uuid(),
            serde_json::to_value(value).map_err(|_| PortError::InvalidState)?,
        )),
        PlanningAggregate::Run(value) => Ok((
            "run",
            value.id.as_uuid(),
            serde_json::to_value(value).map_err(|_| PortError::InvalidState)?,
        )),
        PlanningAggregate::Recommendation(value) => Ok((
            "recommendation",
            value.id.as_uuid(),
            serde_json::to_value(value).map_err(|_| PortError::InvalidState)?,
        )),
        PlanningAggregate::Plan(value) => Ok((
            value.kind().token(),
            value.id(),
            serde_json::to_value(value).map_err(|_| PortError::InvalidState)?,
        )),
    }
}

async fn load_plan_any_kind<T: serde::de::DeserializeOwned>(
    pool: &PgPool,
    authority: &AuthorizedPlanningAccess,
    id: uuid::Uuid,
) -> Result<Option<T>, PortError> {
    if !authority
        .actor
        .authority_scope
        .contains(&authority.target_scope)
    {
        return Err(PortError::Denied);
    }
    let mut tx = pool.begin().await.map_err(map_error)?;
    set_holding(&mut tx, authority.actor.holding_id.as_uuid()).await?;
    let row=sqlx::query("SELECT authority_scope,aggregate_document FROM erpx_planning_write.aggregate_rehydration WHERE holding_id=$1 AND aggregate_kind IN ('demand_plan','supply_plan','capacity_plan') AND aggregate_id=$2")
        .bind(authority.actor.holding_id.as_uuid()).bind(id).fetch_optional(&mut *tx).await.map_err(map_error)?;
    tx.commit().await.map_err(map_error)?;
    let Some(row) = row else { return Ok(None) };
    let scope: erpx_kernel::AuthorityScope = serde_json::from_value(
        row.try_get("authority_scope")
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    if scope != authority.target_scope {
        return Err(PortError::Internal);
    }
    serde_json::from_value(
        row.try_get("aggregate_document")
            .map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}
async fn load_aggregate<T: serde::de::DeserializeOwned>(
    pool: &PgPool,
    authority: &AuthorizedPlanningAccess,
    kind: &str,
    id: uuid::Uuid,
) -> Result<Option<T>, PortError> {
    if !authority
        .actor
        .authority_scope
        .contains(&authority.target_scope)
    {
        return Err(PortError::Denied);
    }
    let mut tx = pool.begin().await.map_err(map_error)?;
    set_holding(&mut tx, authority.actor.holding_id.as_uuid()).await?;
    let row = sqlx::query("SELECT authority_scope,aggregate_document FROM erpx_planning_write.aggregate_rehydration WHERE holding_id=$1 AND aggregate_kind=$2 AND aggregate_id=$3").bind(authority.actor.holding_id.as_uuid()).bind(kind).bind(id).fetch_optional(&mut *tx).await.map_err(map_error)?;
    tx.commit().await.map_err(map_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let scope: erpx_kernel::AuthorityScope = serde_json::from_value(
        row.try_get("authority_scope")
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    if scope != authority.target_scope {
        return Err(PortError::Internal);
    }
    serde_json::from_value(
        row.try_get("aggregate_document")
            .map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

#[derive(Debug, Clone)]
pub struct SqlxPlanningReadRepository {
    pool: ReadPool,
    holding_id: erpx_kernel::HoldingId,
}
impl SqlxPlanningReadRepository {
    pub fn new(pool: PgPool, holding_id: erpx_kernel::HoldingId) -> Result<Self, PortError> {
        let pool = ReadPool::for_holding(pool, holding_id.as_uuid())
            .map_err(|_| PortError::Unavailable)?;
        Ok(Self { pool, holding_id })
    }
}
#[async_trait]
impl PlanningReadRepository for SqlxPlanningReadRepository {
    async fn get_record(
        &self,
        authority: &AuthorizedPlanningAccess,
        id: uuid::Uuid,
    ) -> Result<Option<PlanningRecordProjection>, PortError> {
        if !authority
            .actor
            .authority_scope
            .contains(&authority.target_scope)
            || authority.actor.holding_id != self.holding_id
        {
            return Err(PortError::Denied);
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let generation: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT generation FROM erpx_cqrs_read.projection_generations WHERE holding_id=$1 AND projection_name=$2 AND status='active'",
        )
        .bind(self.holding_id.as_uuid())
        .bind(PROJECTION_NAME)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
        let generation = generation.ok_or(PortError::Unavailable)?;
        let row = sqlx::query(
            "SELECT p.record_kind,p.authority_scope,p.projection,p.source_position,p.projected_at,
                    c.global_sequence AS checkpoint_position,
                    EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f
                      WHERE f.holding_id=p.holding_id AND f.projection_name=p.projection_name
                        AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure
             FROM erpx_planning_read.governed_record_projection p
             LEFT JOIN erpx_cqrs_read.projection_checkpoints c
               ON c.holding_id=p.holding_id AND c.projection_name=p.projection_name
              AND c.generation=p.generation AND c.partition_key=p.partition_key
             WHERE p.holding_id=$1 AND p.projection_name=$2 AND p.generation=$3 AND p.record_id=$4",
        )
        .bind(self.holding_id.as_uuid())
        .bind(PROJECTION_NAME)
        .bind(generation)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        let Some(row) = row else { return Ok(None) };
        let (source_position, checkpoint_position) = plan_projection_positions(&row)?;
        let scope: erpx_kernel::AuthorityScope = serde_json::from_value(
            row.try_get("authority_scope")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        if scope != authority.target_scope {
            return Err(PortError::Internal);
        }
        let document = row.try_get("projection").map_err(|_| PortError::Internal)?;
        let record = match row
            .try_get::<String, _>("record_kind")
            .map_err(|_| PortError::Internal)?
            .as_str()
        {
            "scenario" => PlanningGovernedRecord::Scenario(
                serde_json::from_value(document).map_err(|_| PortError::Internal)?,
            ),
            "recommendation" => PlanningGovernedRecord::Recommendation(
                serde_json::from_value(document).map_err(|_| PortError::Internal)?,
            ),
            _ => return Err(PortError::Internal),
        };
        Ok(Some(PlanningRecordProjection {
            record,
            generation,
            source_position,
            checkpoint_position,
            projected_at: row
                .try_get("projected_at")
                .map_err(|_| PortError::Internal)?,
        }))
    }
    async fn get_run(
        &self,
        authority: &AuthorizedPlanningAccess,
        id: RunId,
    ) -> Result<Option<PlanningRunProjection>, PortError> {
        if !authority
            .actor
            .authority_scope
            .contains(&authority.target_scope)
        {
            return Err(PortError::Denied);
        }
        if authority.actor.holding_id != self.holding_id {
            return Err(PortError::Denied);
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let generation: Option<uuid::Uuid> = sqlx::query_scalar("SELECT generation FROM erpx_cqrs_read.projection_generations WHERE holding_id=$1 AND projection_name='planning.runs' AND status='active'")
            .bind(self.holding_id.as_uuid()).fetch_optional(&mut *tx).await.map_err(map_error)?;
        let generation = generation.ok_or(PortError::Unavailable)?;
        let row = sqlx::query("SELECT p.authority_scope,p.projection,p.generation,p.partition_key,p.source_position,p.projected_at,c.global_sequence AS checkpoint_position,EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f WHERE f.holding_id=p.holding_id AND f.projection_name=p.projection_name AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure FROM erpx_planning_read.run_projection p LEFT JOIN erpx_cqrs_read.projection_checkpoints c ON c.holding_id=p.holding_id AND c.projection_name=p.projection_name AND c.generation=p.generation AND c.partition_key=p.partition_key WHERE p.holding_id=$1 AND p.projection_name='planning.runs' AND p.generation=$2 AND p.run_id=$3")
            .bind(self.holding_id.as_uuid()).bind(generation).bind(id.as_uuid()).fetch_optional(&mut *tx).await.map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let source_position = u64::try_from(
            row.try_get::<i64, _>("source_position")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        let checkpoint_position = row
            .try_get::<Option<i64>, _>("checkpoint_position")
            .map_err(|_| PortError::Internal)?
            .and_then(|v| u64::try_from(v).ok())
            .ok_or(PortError::Stale)?;
        if row
            .try_get::<bool, _>("has_failure")
            .map_err(|_| PortError::Internal)?
            || checkpoint_position < source_position
        {
            return Err(PortError::Stale);
        }
        let scope: erpx_kernel::AuthorityScope = serde_json::from_value(
            row.try_get("authority_scope")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        if scope != authority.target_scope {
            return Err(PortError::Internal);
        }
        Ok(Some(PlanningRunProjection {
            run: serde_json::from_value(
                row.try_get("projection").map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            generation,
            source_position,
            checkpoint_position,
            projected_at: row
                .try_get("projected_at")
                .map_err(|_| PortError::Internal)?,
        }))
    }

    async fn get_plan(
        &self,
        authority: &AuthorizedPlanningAccess,
        id: uuid::Uuid,
    ) -> Result<Option<erpx_planning::PlanningPlanProjection>, PortError> {
        if !authority
            .actor
            .authority_scope
            .contains(&authority.target_scope)
        {
            return Err(PortError::Denied);
        }
        if authority.actor.holding_id != self.holding_id {
            return Err(PortError::Denied);
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let generation: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT generation FROM erpx_cqrs_read.projection_generations WHERE holding_id=$1 AND projection_name=$2 AND status='active'",
        )
        .bind(self.holding_id.as_uuid())
        .bind(PROJECTION_NAME)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
        let generation = generation.ok_or(PortError::Unavailable)?;
        let row = sqlx::query(
            "SELECT p.authority_scope,p.projection,p.source_position,p.projected_at,
                    c.global_sequence AS checkpoint_position,
                    EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f
                      WHERE f.holding_id=p.holding_id AND f.projection_name=p.projection_name
                        AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure
             FROM erpx_planning_read.plan_projection p
             LEFT JOIN erpx_cqrs_read.projection_checkpoints c
               ON c.holding_id=p.holding_id AND c.projection_name=p.projection_name
              AND c.generation=p.generation AND c.partition_key=p.partition_key
             WHERE p.holding_id=$1 AND p.projection_name=$2 AND p.generation=$3 AND p.plan_id=$4",
        )
        .bind(self.holding_id.as_uuid())
        .bind(PROJECTION_NAME)
        .bind(generation)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        let Some(row) = row else { return Ok(None) };
        let (source_position, checkpoint_position) = plan_projection_positions(&row)?;
        let scope: erpx_kernel::AuthorityScope = serde_json::from_value(
            row.try_get("authority_scope")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        if scope != authority.target_scope {
            return Err(PortError::Internal);
        }
        Ok(Some(erpx_planning::PlanningPlanProjection {
            plan: serde_json::from_value(
                row.try_get("projection").map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            generation,
            source_position,
            checkpoint_position,
            projected_at: row
                .try_get("projected_at")
                .map_err(|_| PortError::Internal)?,
        }))
    }
}

fn plan_projection_positions(row: &sqlx::postgres::PgRow) -> Result<(u64, u64), PortError> {
    let source = u64::try_from(
        row.try_get::<i64, _>("source_position")
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let checkpoint = row
        .try_get::<Option<i64>, _>("checkpoint_position")
        .map_err(|_| PortError::Internal)?
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(PortError::Stale)?;
    if row
        .try_get::<bool, _>("has_failure")
        .map_err(|_| PortError::Internal)?
        || checkpoint < source
    {
        return Err(PortError::Stale);
    }
    Ok((source, checkpoint))
}

async fn set_holding(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: uuid::Uuid,
) -> Result<(), PortError> {
    sqlx::query("SELECT set_config('app.holding_id', $1, true)")
        .bind(holding_id.to_string())
        .execute(&mut **tx)
        .await
        .map_err(map_error)?;
    Ok(())
}
fn map_error(error: sqlx::Error) -> PortError {
    match &error {
        sqlx::Error::Database(database) if database.code().as_deref() == Some("23505") => {
            PortError::Conflict
        }
        _ => PortError::Internal,
    }
}

#[cfg(test)]
#[path = "../../schema_test.rs"]
mod schema_test;
#[cfg(test)]
mod tests {
    use crate::schema_test;
    const WRITE_DDL: &str =
        include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
    const READ_DDL: &str =
        include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");
    const RECORD_READ_DDL: &str =
        include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");
    #[test]
    fn schema_pins_inputs_runs_releases_and_pegging() {
        assert!(WRITE_DDL.contains("input_pins"));
        assert!(WRITE_DDL.contains("run_guards"));
        assert!(WRITE_DDL.contains("release_guards"));
        assert!(READ_DDL.contains("pegging_projection"));
        assert!(READ_DDL.contains("exception_projection"));
    }
    #[test]
    fn read_adapter_never_reads_write_authority() {
        let source = include_str!("lib.rs");
        let read = source
            .split("pub struct SqlxPlanningReadRepository")
            .nth(1)
            .unwrap();
        assert!(!read.contains("erpx_planning_write.aggregate_versions"));
        assert!(read.contains("plan_projection"));
        assert!(read.contains("status='active'"));
    }
    #[test]
    fn normalized_authority_has_receipt_audit_outbox_and_rehydration() {
        let owned = schema_test::select_schema_statements(WRITE_DDL, &["erpx_planning_write"]);
        assert!(!owned.contains("aggregate_snapshots"));
        assert!(!owned.contains("snapshot jsonb"));
        for marker in [
            "scenarios",
            "runs",
            "recommendations",
            "aggregate_rehydration",
            "audit_log",
            "outbox",
        ] {
            assert!(WRITE_DDL.contains(marker));
        }
        let source = include_str!("lib.rs");
        assert!(source.contains("ReadPool::for_holding"));
        assert!(source.contains("status='active'"));
        let read = source
            .split("pub struct SqlxPlanningReadRepository")
            .nth(1)
            .unwrap()
            .split("async fn set_holding")
            .next()
            .unwrap();
        assert!(!read.contains(&["ORDER BY projected_at DESC", " LIMIT 1"].concat()));
    }

    #[test]
    fn governed_records_are_generation_managed_and_holding_isolated() {
        for marker in [
            "governed_record_projection",
            "record_kind IN ('scenario', 'recommendation')",
            "projection_generations",
            "ENABLE ROW LEVEL SECURITY",
            "FORCE ROW LEVEL SECURITY",
            "WITH CHECK",
        ] {
            assert!(RECORD_READ_DDL.contains(marker));
        }
        let source = include_str!("projection.rs");
        assert!(source.contains("GOVERNED_RECORD_SNAPSHOT_EVENT_TYPE"));
        assert!(source.contains("ctx.generation"));
    }

    #[test]
    fn multi_aggregate_commit_takes_outbox_order_lock_before_row_locks() {
        let source = include_str!("lib.rs");
        let lock = source
            .find("SELECT erpx_cqrs_write.lock_outbox_commit_order()")
            .expect("commit-order lock");
        let aggregate_loop = source
            .find("for (index, aggregate) in request.aggregates.iter().enumerate()")
            .expect("multi-aggregate lock loop");
        assert!(lock < aggregate_loop);
    }
}
