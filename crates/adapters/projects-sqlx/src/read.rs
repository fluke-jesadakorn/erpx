use async_trait::async_trait;
use chrono::Utc;
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_kernel::TenantScope;
use erpx_projects::{
    HierarchyRow, PortError, Project, ProjectGraphView, ProjectId, ProjectList, ProjectListFilter,
    ProjectRecordKind, ProjectRecordView, ProjectView, ProjectsAggregate, ReadProjection,
    ReadRepository,
};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    HIERARCHY_PROJECTION, PROJECT_PROJECTION, RECORD_PROJECTION, handoff_partition,
    project_partition, workflow_partition,
};

#[derive(Debug, Clone)]
pub struct ProjectsReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl ProjectsReadRepository {
    pub fn new(pool: ReadPool) -> Result<Self, CqrsSqlxError> {
        Self::with_waiter_config(pool, WaiterConfig::default())
    }

    pub fn with_waiter_config(pool: ReadPool, config: WaiterConfig) -> Result<Self, CqrsSqlxError> {
        Ok(Self {
            read_side: ReadSideStore::new(pool.clone()),
            waiter: SqlxProjectionWaiter::new(pool.clone(), config)?,
            pool,
        })
    }

    async fn checkpoint(
        &self,
        projection: &str,
        partition: Option<&str>,
        consistency: &QueryConsistency,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => match partition {
                Some(value) => self
                    .read_side
                    .checkpoint(projection, value)
                    .await
                    .map_err(map_read_error),
                None => Ok(None),
            },
            QueryConsistency::ReadYourWrites(requirement) => {
                if requirement.projection != projection
                    || partition.is_some_and(|value| value != requirement.partition)
                {
                    return Err(PortError::NotReady(ProjectionNotReady {
                        projection: requirement.projection.clone(),
                        partition: requirement.partition.clone(),
                        target: requirement.target,
                        observed: None,
                    }));
                }
                self.waiter
                    .wait_until_checked(requirement)
                    .await
                    .map(Some)
                    .map_err(|error| match error {
                        WaitError::NotReady(value) => PortError::NotReady(value),
                        WaitError::Database(_) => PortError::Unavailable,
                    })
            }
        }
    }

    async fn metadata(
        &self,
        projection: &str,
        checkpoint: Option<ProjectionCheckpoint>,
    ) -> Result<ProjectionMetadata, PortError> {
        let mut tx = self.pool.begin().await.map_err(map_read_error)?;
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM erpx_cqrs_read.projection_generations \
             WHERE tenant_id=erpx_cqrs_read.current_tenant_id() AND projection_name=$1 \
               AND status IN ('active','building') \
             ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(projection)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
        tx.commit().await.map_err(|_| PortError::Unavailable)?;
        let freshness = match status.as_deref() {
            Some("active") => FreshnessState::Current,
            Some("building") => FreshnessState::Rebuilding,
            _ => FreshnessState::Unavailable,
        };
        let lag_millis = checkpoint.as_ref().and_then(|value| {
            Utc::now()
                .signed_duration_since(value.updated_at)
                .to_std()
                .ok()
                .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        });
        Ok(ProjectionMetadata {
            checkpoint,
            freshness,
            lag_millis,
        })
    }

    async fn project(
        &self,
        authority: &TenantScope,
        id: ProjectId,
    ) -> Result<Option<Project>, PortError> {
        let scope = ScopeFilter::from(authority)?;
        let mut tx = self.pool.begin().await.map_err(map_read_error)?;
        let row = sqlx::query(
            "SELECT item.state FROM erpx_projects_read.project_projection item \
             JOIN erpx_cqrs_read.projection_generations generation \
               ON generation.tenant_id=item.tenant_id \
              AND generation.projection_name=item.projection_name \
              AND generation.generation=item.generation AND generation.status='active' \
             WHERE item.tenant_id=erpx_projects_read.current_tenant_id() \
               AND item.projection_name=$1 AND item.partition_key=$2 AND item.project_id=$3 \
               AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))",
        )
        .bind(PROJECT_PROJECTION)
        .bind(project_partition(id))
        .bind(id.as_uuid())
        .bind(scope.platform)
        .bind(&scope.exact)
        .bind(&scope.prefixes)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
        tx.commit().await.map_err(|_| PortError::Unavailable)?;
        let value = row
            .map(|row| {
                serde_json::from_value::<Project>(
                    row.try_get("state").map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)
            })
            .transpose()?;
        if value
            .as_ref()
            .is_some_and(|project| project.id() != id || !authority.contains(project.scope()))
        {
            return Err(PortError::Internal);
        }
        Ok(value)
    }

    async fn node_record(
        &self,
        authority: &TenantScope,
        id: Uuid,
    ) -> Result<Option<(Project, u64)>, PortError> {
        let scope = ScopeFilter::from(authority)?;
        let mut tx = self.pool.begin().await.map_err(map_read_error)?;
        let rows = sqlx::query(
            "SELECT item.state,item.source_global_sequence \
             FROM erpx_projects_read.hierarchy_closure_projection closure \
             JOIN erpx_cqrs_read.projection_generations closure_generation \
               ON closure_generation.tenant_id=closure.tenant_id \
              AND closure_generation.projection_name=closure.projection_name \
              AND closure_generation.generation=closure.generation \
              AND closure_generation.status='active' \
             JOIN erpx_projects_read.project_projection item \
               ON item.tenant_id=closure.tenant_id AND item.project_id=closure.project_id \
              AND item.projection_name=$2 \
             JOIN erpx_cqrs_read.projection_generations item_generation \
               ON item_generation.tenant_id=item.tenant_id \
              AND item_generation.projection_name=item.projection_name \
              AND item_generation.generation=item.generation \
              AND item_generation.status='active' \
             WHERE closure.tenant_id=erpx_projects_read.current_tenant_id() \
               AND closure.projection_name=$3 AND closure.descendant_node_id=$1 \
               AND closure.depth=0 \
               AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[])) \
             ORDER BY item.project_id LIMIT 2",
        )
        .bind(id)
        .bind(PROJECT_PROJECTION)
        .bind(HIERARCHY_PROJECTION)
        .bind(scope.platform)
        .bind(&scope.exact)
        .bind(&scope.prefixes)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
        tx.commit().await.map_err(|_| PortError::Unavailable)?;
        if rows.len() > 1 {
            return Err(PortError::Internal);
        }
        rows.into_iter()
            .next()
            .map(|row| {
                let project = serde_json::from_value::<Project>(
                    row.try_get("state").map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?;
                let source_position = u64::try_from(
                    row.try_get::<i64, _>("source_global_sequence")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?;
                let node_id = erpx_projects::ProjectNodeId::try_from_uuid(id)
                    .map_err(|_| PortError::Internal)?;
                if !authority.contains(project.scope()) || !project.nodes().contains_key(&node_id) {
                    return Err(PortError::Internal);
                }
                Ok((project, source_position))
            })
            .transpose()
    }

    async fn projected_record(
        &self,
        authority: &TenantScope,
        id: Uuid,
        kind: ProjectRecordKind,
    ) -> Result<Option<(ProjectsAggregate, u64)>, PortError> {
        let expected_kind = match kind {
            ProjectRecordKind::WorkflowTemplate => "workflow_template",
            ProjectRecordKind::Handoff => "handoff",
            ProjectRecordKind::Phase | ProjectRecordKind::Item => return Err(PortError::Internal),
        };
        let scope = ScopeFilter::from(authority)?;
        let mut tx = self.pool.begin().await.map_err(map_read_error)?;
        let row = sqlx::query(
            "SELECT item.state,item.source_global_sequence \
             FROM erpx_projects_read.project_record_projection item \
             JOIN erpx_cqrs_read.projection_generations generation \
               ON generation.tenant_id=item.tenant_id \
              AND generation.projection_name=item.projection_name \
              AND generation.generation=item.generation AND generation.status='active' \
             WHERE item.tenant_id=erpx_projects_read.current_tenant_id() \
               AND item.projection_name=$1 AND item.record_id=$2 AND item.record_kind=$3 \
               AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))",
        )
        .bind(RECORD_PROJECTION)
        .bind(id)
        .bind(expected_kind)
        .bind(scope.platform)
        .bind(&scope.exact)
        .bind(&scope.prefixes)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
        tx.commit().await.map_err(|_| PortError::Unavailable)?;
        row.map(|row| {
            let aggregate = serde_json::from_value::<ProjectsAggregate>(
                row.try_get("state").map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?;
            let source_position = u64::try_from(
                row.try_get::<i64, _>("source_global_sequence")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?;
            if aggregate.id_uuid() != id || !authority.contains(aggregate.scope()) {
                return Err(PortError::Internal);
            }
            Ok((aggregate, source_position))
        })
        .transpose()
    }
}

#[async_trait]
impl ReadRepository for ProjectsReadRepository {
    async fn get_project(
        &self,
        authority: &TenantScope,
        id: ProjectId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ProjectView>>, PortError> {
        let partition = project_partition(id);
        let checkpoint = self
            .checkpoint(PROJECT_PROJECTION, Some(&partition), consistency)
            .await?;
        let value = self
            .project(authority, id)
            .await?
            .map(|project| ProjectView { project });
        Ok(ReadProjection {
            value,
            metadata: self.metadata(PROJECT_PROJECTION, checkpoint).await?,
        })
    }

    async fn get_graph(
        &self,
        authority: &TenantScope,
        id: ProjectId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ProjectGraphView>>, PortError> {
        let partition = project_partition(id);
        let checkpoint = self
            .checkpoint(HIERARCHY_PROJECTION, Some(&partition), consistency)
            .await?;
        let Some(project) = self.project(authority, id).await? else {
            return Ok(ReadProjection {
                value: None,
                metadata: self.metadata(HIERARCHY_PROJECTION, checkpoint).await?,
            });
        };
        let mut tx = self.pool.begin().await.map_err(map_read_error)?;
        let rows = sqlx::query(
            "SELECT closure.descendant_node_id,closure.ancestor_node_id,closure.depth \
             FROM erpx_projects_read.hierarchy_closure_projection closure \
             JOIN erpx_cqrs_read.projection_generations generation \
               ON generation.tenant_id=closure.tenant_id \
              AND generation.projection_name=closure.projection_name \
              AND generation.generation=closure.generation AND generation.status='active' \
             WHERE closure.tenant_id=erpx_projects_read.current_tenant_id() \
               AND closure.projection_name=$1 AND closure.project_id=$2 \
             ORDER BY closure.descendant_node_id,closure.depth DESC",
        )
        .bind(HIERARCHY_PROJECTION)
        .bind(id.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
        tx.commit().await.map_err(|_| PortError::Unavailable)?;
        let hierarchy = project
            .nodes()
            .values()
            .map(|node| {
                let ancestor_ids = rows
                    .iter()
                    .filter_map(|row| {
                        let descendant: Uuid = row.try_get("descendant_node_id").ok()?;
                        let depth: i16 = row.try_get("depth").ok()?;
                        (descendant == node.id().as_uuid() && depth > 0).then(|| {
                            row.try_get("ancestor_node_id")
                                .ok()
                                .and_then(|id| erpx_projects::ProjectNodeId::try_from_uuid(id).ok())
                        })?
                    })
                    .collect();
                HierarchyRow {
                    node: node.clone(),
                    ancestor_ids,
                    depth: hierarchy_depth(&rows, node.id().as_uuid()),
                }
            })
            .collect();
        Ok(ReadProjection {
            value: Some(ProjectGraphView { project, hierarchy }),
            metadata: self.metadata(HIERARCHY_PROJECTION, checkpoint).await?,
        })
    }

    async fn get_record(
        &self,
        authority: &TenantScope,
        id: Uuid,
        kind: ProjectRecordKind,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ProjectRecordView>>, PortError> {
        let (value, metadata) = match kind {
            ProjectRecordKind::Phase | ProjectRecordKind::Item => {
                let checkpoint = self
                    .checkpoint(HIERARCHY_PROJECTION, None, consistency)
                    .await?;
                let value = self.node_record(authority, id).await?.and_then(
                    |(project, _source_position)| {
                        let node_id = erpx_projects::ProjectNodeId::try_from_uuid(id).ok()?;
                        let node = project.nodes().get(&node_id)?.clone();
                        let matches_kind = match kind {
                            ProjectRecordKind::Phase => node.kind().is_container(),
                            ProjectRecordKind::Item => !node.kind().is_container(),
                            ProjectRecordKind::WorkflowTemplate | ProjectRecordKind::Handoff => {
                                false
                            }
                        };
                        matches_kind.then(|| ProjectRecordView::Node {
                            project_id: project.id(),
                            node,
                        })
                    },
                );
                (
                    value,
                    self.metadata(HIERARCHY_PROJECTION, checkpoint).await?,
                )
            }
            ProjectRecordKind::WorkflowTemplate | ProjectRecordKind::Handoff => {
                let partition = match kind {
                    ProjectRecordKind::WorkflowTemplate => workflow_partition(
                        erpx_projects::WorkflowTemplateId::try_from_uuid(id)
                            .map_err(|_| PortError::Internal)?,
                    ),
                    ProjectRecordKind::Handoff => handoff_partition(
                        erpx_projects::HandoffId::try_from_uuid(id)
                            .map_err(|_| PortError::Internal)?,
                    ),
                    ProjectRecordKind::Phase | ProjectRecordKind::Item => unreachable!(),
                };
                let checkpoint = self
                    .checkpoint(RECORD_PROJECTION, Some(&partition), consistency)
                    .await?;
                let value = self.projected_record(authority, id, kind).await?.and_then(
                    |(aggregate, _source_position)| match (kind, aggregate) {
                        (
                            ProjectRecordKind::WorkflowTemplate,
                            ProjectsAggregate::Workflow(value),
                        ) => Some(ProjectRecordView::WorkflowTemplate(value)),
                        (ProjectRecordKind::Handoff, ProjectsAggregate::Handoff(value)) => {
                            Some(ProjectRecordView::Handoff(value))
                        }
                        _ => None,
                    },
                );
                (value, self.metadata(RECORD_PROJECTION, checkpoint).await?)
            }
        };
        Ok(ReadProjection { value, metadata })
    }

    async fn list_projects(
        &self,
        authority: &TenantScope,
        filter: &ProjectListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<ProjectList>, PortError> {
        if filter.page_size == 0 || filter.page_size > 100 {
            return Err(PortError::Internal);
        }
        let checkpoint = self
            .checkpoint(PROJECT_PROJECTION, None, consistency)
            .await?;
        let after = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let scope = ScopeFilter::from(authority)?;
        let mut tx = self.pool.begin().await.map_err(map_read_error)?;
        let rows = sqlx::query(
            "SELECT item.state,item.project_id FROM erpx_projects_read.project_projection item \
             JOIN erpx_cqrs_read.projection_generations generation \
               ON generation.tenant_id=item.tenant_id AND generation.projection_name=item.projection_name \
              AND generation.generation=item.generation AND generation.status='active' \
             WHERE item.tenant_id=erpx_projects_read.current_tenant_id() AND item.projection_name=$1 \
               AND ($2 OR item.scope_key=ANY($3::text[]) OR item.scope_key LIKE ANY($4::text[])) \
               AND ($5::uuid IS NULL OR item.project_id>$5) ORDER BY item.project_id LIMIT $6",
        )
        .bind(PROJECT_PROJECTION).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes)
        .bind(after).bind(i64::from(filter.page_size) + 1).fetch_all(&mut *tx).await
        .map_err(|_| PortError::Unavailable)?;
        tx.commit().await.map_err(|_| PortError::Unavailable)?;
        let mut projects = rows
            .into_iter()
            .map(|row| {
                serde_json::from_value::<Project>(
                    row.try_get("state").map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if projects
            .iter()
            .any(|project| !authority.contains(project.scope()))
        {
            return Err(PortError::Internal);
        }
        let next_cursor = if projects.len() > usize::from(filter.page_size) {
            projects.pop();
            projects
                .last()
                .map(|project| project.id().as_uuid().simple().to_string())
        } else {
            None
        };
        Ok(ReadProjection {
            value: ProjectList {
                projects,
                next_cursor,
            },
            metadata: self.metadata(PROJECT_PROJECTION, checkpoint).await?,
        })
    }
}

fn hierarchy_depth(rows: &[sqlx::postgres::PgRow], node_id: Uuid) -> u16 {
    rows.iter()
        .filter_map(|row| {
            let descendant: Uuid = row.try_get("descendant_node_id").ok()?;
            (descendant == node_id).then(|| row.try_get::<i16, _>("depth").ok())?
        })
        .filter_map(|value| u16::try_from(value).ok())
        .max()
        .unwrap_or(0)
}

fn decode_cursor(value: &str) -> Result<Uuid, PortError> {
    let raw = u128::from_str_radix(value, 16).map_err(|_| PortError::Internal)?;
    Ok(Uuid::from_u128(raw))
}

struct ScopeFilter {
    platform: bool,
    exact: Vec<String>,
    prefixes: Vec<String>,
}
impl ScopeFilter {
    fn from(scope: &TenantScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Denied)?;
        Ok(match scope {
            TenantScope::Platform => Self {
                platform: true,
                exact: vec![],
                prefixes: vec![],
            },
            TenantScope::Holding { holding_id } => Self {
                platform: false,
                exact: vec![],
                prefixes: vec![format!("h:{holding_id}/%")],
            },
            TenantScope::Company {
                holding_id,
                company_id,
            } => {
                let key = format!("h:{holding_id}/c:{company_id}");
                Self {
                    platform: false,
                    exact: vec![key.clone()],
                    prefixes: vec![format!("{key}/%")],
                }
            }
            TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Self {
                platform: false,
                exact: vec![format!("h:{holding_id}/c:{company_id}/b:{branch_id}")],
                prefixes: vec![],
            },
            TenantScope::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => Self {
                platform: false,
                exact: company_ids
                    .iter()
                    .flat_map(|company| {
                        if branch_ids.is_empty() {
                            vec![format!("h:{holding_id}/c:{company}")]
                        } else {
                            branch_ids
                                .iter()
                                .map(|branch| format!("h:{holding_id}/c:{company}/b:{branch}"))
                                .collect()
                        }
                    })
                    .collect(),
                prefixes: vec![],
            },
        })
    }
}

fn map_read_error(error: CqrsSqlxError) -> PortError {
    match error {
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn query_source_joins_only_exact_tenant_active_generations() {
        let source = include_str!("read.rs");
        assert!(source.contains("generation.tenant_id=item.tenant_id"));
        assert!(source.contains("generation.status='active'"));
        assert!(source.contains("FreshnessState::Rebuilding"));
        assert!(source.contains("FreshnessState::Unavailable"));
    }
}
