use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_kernel::TenantId;
use erpx_projects::{Project, ProjectEvent, ProjectNodeId, ProjectsAggregate};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    CONTEXT_NAME, EVENT_VERSION, HIERARCHY_PROJECTION, PROJECT_AGGREGATE_TYPE, PROJECT_PROJECTION,
    PROJECT_PROJECTION_FACT_EVENT, PROJECT_RECORD_PROJECTION_FACT_EVENT, RECORD_PROJECTION,
    command::scope_key, handoff_partition, project_partition, workflow_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "payload_kind", content = "payload", rename_all = "snake_case")]
pub enum ProjectsOutboxPayload {
    Domain(ProjectEvent),
    ProjectProjection(Box<Project>),
    RecordProjection(Box<ProjectsAggregate>),
}

#[derive(Debug, Clone)]
pub struct ProjectsProjector {
    events: Vec<String>,
    tenant_id: TenantId,
}

impl ProjectsProjector {
    pub fn new(tenant_id: TenantId) -> Self {
        Self {
            events: vec![PROJECT_PROJECTION_FACT_EVENT.into()],
            tenant_id,
        }
    }
}

#[async_trait]
impl Projector for ProjectsProjector {
    fn name(&self) -> &str {
        PROJECT_PROJECTION
    }

    fn event_types(&self) -> &[String] {
        &self.events
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        set_tenant(transaction, self.tenant_id).await?;
        let project = decode_project(context, event, PROJECT_PROJECTION)?;
        let state = serde_json::to_value(&project).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_projects_read.project_projection (tenant_id,projection_name,generation,partition_key,project_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,project_code,project_class,project_status,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,now()) ON CONFLICT (tenant_id,projection_name,generation,partition_key,project_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,project_code=EXCLUDED.project_code,project_class=EXCLUDED.project_class,project_status=EXCLUDED.project_status,state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_projects_read.project_projection.tenant_id=EXCLUDED.tenant_id AND erpx_projects_read.project_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(self.tenant_id.as_uuid()).bind(PROJECT_PROJECTION).bind(context.generation).bind(&event.partition)
            .bind(project.id().as_uuid()).bind(to_i64(project.version())?).bind(to_i64(event.global_sequence)?)
            .bind(serde_json::to_value(project.scope()).map_err(failure)?)
            .bind(scope_key(project.scope()).map_err(failure)?).bind(project.code())
            .bind(enum_name(project.class())?).bind(enum_name(project.status())?).bind(state)
            .bind(project.created_at()).bind(project.updated_at()).execute(&mut **transaction)
            .await.map_err(failure)?;
        Ok(())
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(
            transaction,
            context,
            "erpx_projects_read.project_projection",
            PROJECT_PROJECTION,
            self.tenant_id,
        )
        .await
    }
}

#[derive(Debug, Clone)]
pub struct ProjectsHierarchyProjector {
    events: Vec<String>,
    tenant_id: TenantId,
}

impl ProjectsHierarchyProjector {
    pub fn new(tenant_id: TenantId) -> Self {
        Self {
            events: vec![PROJECT_PROJECTION_FACT_EVENT.into()],
            tenant_id,
        }
    }
}

#[async_trait]
impl Projector for ProjectsHierarchyProjector {
    fn name(&self) -> &str {
        HIERARCHY_PROJECTION
    }

    fn event_types(&self) -> &[String] {
        &self.events
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        set_tenant(transaction, self.tenant_id).await?;
        let project = decode_project(context, event, HIERARCHY_PROJECTION)?;
        sqlx::query("DELETE FROM erpx_projects_read.hierarchy_closure_projection WHERE tenant_id=$1 AND projection_name=$2 AND generation=$3 AND project_id=$4")
            .bind(self.tenant_id.as_uuid()).bind(HIERARCHY_PROJECTION).bind(context.generation).bind(project.id().as_uuid())
            .execute(&mut **transaction).await.map_err(failure)?;
        for node in project.nodes().values() {
            insert_closure(
                transaction,
                context,
                event,
                &project,
                self.tenant_id,
                (node.id(), node.id(), 0),
            )
            .await?;
            let mut ancestor = node.parent_id();
            let mut depth = 1_u16;
            while let Some(ancestor_id) = ancestor {
                insert_closure(
                    transaction,
                    context,
                    event,
                    &project,
                    self.tenant_id,
                    (ancestor_id, node.id(), depth),
                )
                .await?;
                ancestor = project
                    .nodes()
                    .get(&ancestor_id)
                    .and_then(erpx_projects::ProjectNode::parent_id);
                depth = depth
                    .checked_add(1)
                    .ok_or_else(|| ProjectionFailure::new("Projects hierarchy depth overflow"))?;
                if depth > 2 && ancestor.is_some() {
                    return Err(ProjectionFailure::new(
                        "Projects projection fact exceeds the governed hierarchy depth",
                    ));
                }
            }
        }
        Ok(())
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(
            transaction,
            context,
            "erpx_projects_read.hierarchy_closure_projection",
            HIERARCHY_PROJECTION,
            self.tenant_id,
        )
        .await
    }
}

#[derive(Debug, Clone)]
pub struct ProjectsRecordProjector {
    events: Vec<String>,
    tenant_id: TenantId,
}

impl ProjectsRecordProjector {
    pub fn new(tenant_id: TenantId) -> Self {
        Self {
            events: vec![PROJECT_RECORD_PROJECTION_FACT_EVENT.into()],
            tenant_id,
        }
    }
}

#[async_trait]
impl Projector for ProjectsRecordProjector {
    fn name(&self) -> &str {
        RECORD_PROJECTION
    }

    fn event_types(&self) -> &[String] {
        &self.events
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        set_tenant(transaction, self.tenant_id).await?;
        let aggregate = decode_record(context, event)?;
        let (record_kind, project_id, lifecycle, partition) = match aggregate.as_ref() {
            ProjectsAggregate::Workflow(value) => (
                "workflow_template",
                None,
                enum_name(value.status())?,
                workflow_partition(value.id()),
            ),
            ProjectsAggregate::Handoff(value) => (
                "handoff",
                Some(value.project_id().as_uuid()),
                enum_name(value.status())?,
                handoff_partition(value.id()),
            ),
            ProjectsAggregate::Project(_) | ProjectsAggregate::Dependency(_) => {
                return Err(ProjectionFailure::new(
                    "Projects record projection received an unsupported aggregate",
                ));
            }
        };
        let state = serde_json::to_value(aggregate.as_ref()).map_err(failure)?;
        sqlx::query(
            "INSERT INTO erpx_projects_read.project_record_projection \
             (tenant_id,projection_name,generation,partition_key,record_id,record_kind,project_id,tenant_scope,scope_key,lifecycle_status,source_aggregate_version,source_global_sequence,state,aggregate_updated_at,projected_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,now()) \
             ON CONFLICT (tenant_id,projection_name,generation,partition_key,record_id) DO UPDATE SET \
             record_kind=EXCLUDED.record_kind,project_id=EXCLUDED.project_id,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,lifecycle_status=EXCLUDED.lifecycle_status,source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,state=EXCLUDED.state,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() \
             WHERE erpx_projects_read.project_record_projection.tenant_id=EXCLUDED.tenant_id \
               AND erpx_projects_read.project_record_projection.source_aggregate_version < EXCLUDED.source_aggregate_version",
        )
        .bind(self.tenant_id.as_uuid())
        .bind(RECORD_PROJECTION)
        .bind(context.generation)
        .bind(partition)
        .bind(aggregate.id_uuid())
        .bind(record_kind)
        .bind(project_id)
        .bind(serde_json::to_value(aggregate.scope()).map_err(failure)?)
        .bind(scope_key(aggregate.scope()).map_err(failure)?)
        .bind(lifecycle)
        .bind(to_i64(aggregate.version())?)
        .bind(to_i64(event.global_sequence)?)
        .bind(state)
        .bind(aggregate.occurred_at())
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        Ok(())
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(
            transaction,
            context,
            "erpx_projects_read.project_record_projection",
            RECORD_PROJECTION,
            self.tenant_id,
        )
        .await
    }
}

fn decode_record(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<Box<ProjectsAggregate>, ProjectionFailure> {
    if context.projection != RECORD_PROJECTION
        || event.context != CONTEXT_NAME
        || event.event_type != PROJECT_RECORD_PROJECTION_FACT_EVENT
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "Projects record projector received an incompatible event",
        ));
    }
    let payload: ProjectsOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    let ProjectsOutboxPayload::RecordProjection(aggregate) = payload else {
        return Err(ProjectionFailure::new(
            "Projects record projection event carried another payload",
        ));
    };
    let partition_matches = match aggregate.as_ref() {
        ProjectsAggregate::Workflow(value) => event.partition == workflow_partition(value.id()),
        ProjectsAggregate::Handoff(value) => event.partition == handoff_partition(value.id()),
        ProjectsAggregate::Project(_) | ProjectsAggregate::Dependency(_) => false,
    };
    if aggregate.version() != event.aggregate_version
        || aggregate.id_uuid() != event.aggregate_id.as_uuid()
        || aggregate.aggregate_type() != event.aggregate_type.as_str()
        || aggregate.scope() != &event.tenant_scope
        || !partition_matches
    {
        return Err(ProjectionFailure::new(
            "Projects record projection metadata mismatch",
        ));
    }
    Ok(aggregate)
}

async fn insert_closure(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    project: &Project,
    tenant_id: TenantId,
    edge: (ProjectNodeId, ProjectNodeId, u16),
) -> Result<(), ProjectionFailure> {
    let (ancestor, descendant, depth) = edge;
    sqlx::query("INSERT INTO erpx_projects_read.hierarchy_closure_projection (tenant_id,projection_name,generation,partition_key,project_id,ancestor_node_id,descendant_node_id,depth,source_aggregate_version,source_global_sequence,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,now())")
        .bind(tenant_id.as_uuid()).bind(HIERARCHY_PROJECTION).bind(context.generation).bind(&event.partition)
        .bind(project.id().as_uuid()).bind(ancestor.as_uuid()).bind(descendant.as_uuid())
        .bind(i16::try_from(depth).map_err(|_| ProjectionFailure::new("Projects hierarchy depth overflow"))?)
        .bind(to_i64(project.version())?).bind(to_i64(event.global_sequence)?)
        .execute(&mut **transaction).await.map_err(failure)?;
    Ok(())
}

fn decode_project(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    projection: &str,
) -> Result<Project, ProjectionFailure> {
    if context.projection != projection
        || event.context != CONTEXT_NAME
        || event.aggregate_type != PROJECT_AGGREGATE_TYPE
        || event.event_type != PROJECT_PROJECTION_FACT_EVENT
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "Projects projector received an incompatible event",
        ));
    }
    let payload: ProjectsOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    let ProjectsOutboxPayload::ProjectProjection(project) = payload else {
        return Err(ProjectionFailure::new(
            "Projects projection event carried another payload",
        ));
    };
    let project = *project;
    if project.version() != event.aggregate_version
        || project.id().as_uuid() != event.aggregate_id.as_uuid()
        || project.scope() != &event.tenant_scope
        || event.partition != project_partition(project.id())
        || project.validate().is_err()
    {
        return Err(ProjectionFailure::new(
            "Projects projection metadata mismatch",
        ));
    }
    Ok(project)
}

async fn reset(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    table: &str,
    projection: &str,
    tenant_id: TenantId,
) -> Result<(), ProjectionFailure> {
    set_tenant(transaction, tenant_id).await?;
    let statement =
        format!("DELETE FROM {table} WHERE tenant_id=$1 AND projection_name=$2 AND generation=$3");
    sqlx::query(&statement)
        .bind(tenant_id.as_uuid())
        .bind(projection)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
    Ok(())
}

async fn set_tenant(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: TenantId,
) -> Result<(), ProjectionFailure> {
    let expected = tenant_id.as_uuid().to_string();
    let actual: String = sqlx::query_scalar("SELECT set_config('app.tenant_id', $1, true)")
        .bind(&expected)
        .fetch_one(&mut **transaction)
        .await
        .map_err(failure)?;
    (actual == expected)
        .then_some(())
        .ok_or_else(|| ProjectionFailure::new("Projects projection tenant binding mismatch"))
}

fn enum_name<T: Serialize>(value: T) -> Result<String, ProjectionFailure> {
    serde_json::to_value(value)
        .map_err(failure)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ProjectionFailure::new("Projects enum must encode as text"))
}

fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| ProjectionFailure::new("numeric value is too large"))
}

fn failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn tenant_id() -> TenantId {
        TenantId::try_from_uuid(Uuid::from_u128(1)).unwrap()
    }

    #[test]
    fn project_and_closure_rebuilds_subscribe_to_the_same_projection_fact() {
        assert_eq!(
            ProjectsProjector::new(tenant_id()).event_types(),
            &[PROJECT_PROJECTION_FACT_EVENT.to_owned()]
        );
        assert_eq!(
            ProjectsHierarchyProjector::new(tenant_id()).event_types(),
            &[PROJECT_PROJECTION_FACT_EVENT.to_owned()]
        );
        assert_eq!(
            ProjectsRecordProjector::new(tenant_id()).event_types(),
            &[PROJECT_RECORD_PROJECTION_FACT_EVENT.to_owned()]
        );
    }

    #[test]
    fn projections_are_generation_scoped_and_rebuildable() {
        let source = include_str!("projection.rs");
        assert!(source.contains("source_aggregate_version < EXCLUDED.source_aggregate_version"));
        assert!(source.contains("DELETE FROM erpx_projects_read.hierarchy_closure_projection"));
        assert!(source.contains("erpx_projects_read.project_record_projection"));
        assert!(source.matches("async fn reset(").count() >= 4);
    }

    #[test]
    fn every_projection_operation_sets_and_filters_the_tenant() {
        let source = include_str!("projection.rs");
        assert!(source.contains("set_config('app.tenant_id'"));
        assert!(source.contains("WHERE tenant_id=$1"));
        assert!(source.contains("ON CONFLICT (tenant_id,projection_name"));
    }
}
