use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_kernel::AuthorityScope;
use erpx_organization::{
    NodeList, NodeListFilter, NodeView, OrganizationNodeId, PortError, ReadProjection,
    ReadRepository,
};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    PROJECTION_NAME, canonical_node_scope, node_partition,
    projection::{kind_name, status_name},
};

const CURSOR_SEPARATOR: char = '|';

#[derive(Debug, Clone)]
pub struct OrganizationReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl OrganizationReadRepository {
    pub fn new(pool: ReadPool) -> Result<Self, CqrsSqlxError> {
        Self::with_waiter_config(pool, WaiterConfig::default())
    }

    pub fn with_waiter_config(
        pool: ReadPool,
        waiter_config: WaiterConfig,
    ) -> Result<Self, CqrsSqlxError> {
        Ok(Self {
            read_side: ReadSideStore::new(pool.clone()),
            waiter: SqlxProjectionWaiter::new(pool.clone(), waiter_config)?,
            pool,
        })
    }

    async fn consistency_checkpoint(
        &self,
        consistency: &QueryConsistency,
        exact_partition: Option<&str>,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => match exact_partition {
                Some(partition) => self
                    .read_side
                    .checkpoint(PROJECTION_NAME, partition)
                    .await
                    .map_err(map_read_error),
                None => Ok(None),
            },
            QueryConsistency::ReadYourWrites(requirement) => {
                let valid_partition = requirement
                    .partition
                    .strip_prefix("organization.node:")
                    .is_some_and(|id| Uuid::parse_str(id).is_ok());
                if requirement.projection != PROJECTION_NAME
                    || !valid_partition
                    || exact_partition.is_some_and(|partition| partition != requirement.partition)
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
                        WaitError::NotReady(not_ready) => PortError::NotReady(not_ready),
                        WaitError::Database(_) => PortError::Unavailable,
                    })
            }
        }
    }

    async fn metadata(
        &self,
        checkpoint: Option<ProjectionCheckpoint>,
    ) -> Result<ProjectionMetadata, PortError> {
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status
             FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND status IN ('active', 'building')
             ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END
             LIMIT 1",
        )
        .bind(PROJECTION_NAME)
        .fetch_optional(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
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
}

#[async_trait]
impl ReadRepository for OrganizationReadRepository {
    async fn get(
        &self,
        authority_scope: &AuthorityScope,
        node_id: OrganizationNodeId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<NodeView>>, PortError> {
        let partition = node_partition(node_id);
        let checkpoint = self
            .consistency_checkpoint(consistency, Some(&partition))
            .await?;
        let filter = ScopeFilter::from_scope(authority_scope)?;
        let row = sqlx::query(
            "SELECT projection.state
             FROM erpx_organization_read.node_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name = projection.projection_name
              AND generation.generation = projection.generation
              AND generation.status = 'active'
             WHERE projection.projection_name = $1
               AND projection.node_id = $2 AND projection.partition_key = $3
               AND ($4 OR projection.scope_key = ANY($5::text[])
                       OR projection.scope_key LIKE ANY($6::text[])
                       OR ($7::uuid IS NOT NULL AND (
                           projection.node_id = $7
                           OR projection.placement ->> 'holding_id' = $7::text
                           OR projection.placement ->> 'company_id' = $7::text
                           OR projection.placement ->> 'branch_id' = $7::text
                           OR projection.placement ->> 'department_id' = $7::text
                       )))",
        )
        .bind(PROJECTION_NAME)
        .bind(node_id.as_uuid())
        .bind(&partition)
        .bind(filter.platform)
        .bind(&filter.exact)
        .bind(&filter.prefixes)
        .bind(filter.subtree_node_id)
        .fetch_optional(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let view = row.map(decode_view).transpose()?;
        if view.as_ref().is_some_and(|view| {
            !view
                .placement
                .is_within_scope(view.node_id, authority_scope)
                .unwrap_or(false)
        }) {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value: view,
            metadata: self.metadata(checkpoint).await?,
        })
    }

    async fn list(
        &self,
        authority_scope: &AuthorityScope,
        filter: &NodeListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<NodeList>, PortError> {
        let checkpoint = self.consistency_checkpoint(consistency, None).await?;
        let scope = ScopeFilter::from_scope(authority_scope)?;
        let cursor = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let cursor_at = cursor.as_ref().map(|(updated_at, _)| *updated_at);
        let cursor_id = cursor.as_ref().map(|(_, node_id)| node_id.as_uuid());
        let kind = filter.kind.map(kind_name);
        let status = filter.status.map(status_name);
        let parent_id = filter.parent_id.map(|id| id.as_uuid());
        let limit = i64::from(filter.page_size) + 1;
        let rows = sqlx::query(
            "SELECT projection.state
             FROM erpx_organization_read.node_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name = projection.projection_name
              AND generation.generation = projection.generation
              AND generation.status = 'active'
             WHERE projection.projection_name = $1
               AND ($2 OR projection.scope_key = ANY($3::text[])
                       OR projection.scope_key LIKE ANY($4::text[])
                       OR ($5::uuid IS NOT NULL AND (
                           projection.node_id = $5
                           OR projection.placement ->> 'holding_id' = $5::text
                           OR projection.placement ->> 'company_id' = $5::text
                           OR projection.placement ->> 'branch_id' = $5::text
                           OR projection.placement ->> 'department_id' = $5::text
                       )))
               AND ($6::text IS NULL OR projection.node_kind = $6)
               AND ($7::text IS NULL OR projection.node_status = $7)
               AND ($8::uuid IS NULL OR projection.parent_node_id = $8)
               AND ($9::timestamptz IS NULL OR
                    (projection.aggregate_updated_at, projection.node_id) < ($9, $10))
             ORDER BY projection.aggregate_updated_at DESC, projection.node_id DESC
             LIMIT $11",
        )
        .bind(PROJECTION_NAME)
        .bind(scope.platform)
        .bind(&scope.exact)
        .bind(&scope.prefixes)
        .bind(scope.subtree_node_id)
        .bind(kind)
        .bind(status)
        .bind(parent_id)
        .bind(cursor_at)
        .bind(cursor_id)
        .bind(limit)
        .fetch_all(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let mut nodes = rows
            .into_iter()
            .map(decode_view)
            .collect::<Result<Vec<_>, _>>()?;
        if nodes.iter().any(|view| {
            !view
                .placement
                .is_within_scope(view.node_id, authority_scope)
                .unwrap_or(false)
        }) {
            return Err(PortError::Internal);
        }
        let next_cursor = if nodes.len() > usize::from(filter.page_size) {
            nodes.truncate(usize::from(filter.page_size));
            nodes
                .last()
                .map(|view| encode_cursor(view.updated_at, view.node_id))
        } else {
            None
        };
        Ok(ReadProjection {
            value: NodeList { nodes, next_cursor },
            metadata: self.metadata(checkpoint).await?,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ScopeFilter {
    platform: bool,
    exact: Vec<String>,
    prefixes: Vec<String>,
    subtree_node_id: Option<Uuid>,
}

impl ScopeFilter {
    fn from_scope(scope: &AuthorityScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Denied)?;
        Ok(match scope {
            AuthorityScope::Platform => Self {
                platform: true,
                exact: Vec::new(),
                prefixes: Vec::new(),
                subtree_node_id: None,
            },
            AuthorityScope::Holding { holding_id } => {
                let key = format!("h:{holding_id}");
                Self {
                    platform: false,
                    exact: vec![key.clone()],
                    prefixes: vec![format!("{key}/%")],
                    subtree_node_id: None,
                }
            }
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => {
                let key = format!("h:{holding_id}/c:{company_id}");
                Self {
                    platform: false,
                    exact: vec![key.clone()],
                    prefixes: vec![format!("{key}/%")],
                    subtree_node_id: None,
                }
            }
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => {
                let key = format!("h:{holding_id}/c:{company_id}/b:{branch_id}");
                Self {
                    platform: false,
                    exact: vec![key.clone()],
                    prefixes: vec![format!("{key}/%")],
                    subtree_node_id: None,
                }
            }
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => {
                let holding = format!("h:{holding_id}");
                let mut exact = Vec::new();
                let mut prefixes = Vec::new();
                if holding_id == organization_node_id {
                    exact.push(holding.clone());
                    prefixes.push(format!("{holding}/%"));
                } else {
                    // The exact node kind is intentionally resolved from the
                    // projected hierarchy rather than inferred from an opaque
                    // UUID. Every candidate is followed by a structural
                    // placement post-check before a row can leave the adapter.
                    let candidates = [
                        format!("{holding}/c:{organization_node_id}"),
                        format!("{holding}/c:%/hq:{organization_node_id}"),
                        format!("{holding}/c:%/b:{organization_node_id}"),
                        format!("{holding}/c:%/d:{organization_node_id}"),
                        format!("{holding}/c:%/b:%/d:{organization_node_id}"),
                        format!("{holding}/c:%/d:%/t:{organization_node_id}"),
                        format!("{holding}/c:%/b:%/d:%/t:{organization_node_id}"),
                    ];
                    for candidate in candidates {
                        prefixes.push(candidate.clone());
                        prefixes.push(format!("{candidate}/%"));
                    }
                }
                Self {
                    platform: false,
                    exact,
                    prefixes,
                    subtree_node_id: Some(*organization_node_id),
                }
            }
        })
    }
}

pub fn encode_cursor(updated_at: DateTime<Utc>, node_id: OrganizationNodeId) -> String {
    format!(
        "{}{}{}",
        updated_at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        CURSOR_SEPARATOR,
        node_id.as_uuid()
    )
}

pub fn decode_cursor(value: &str) -> Result<(DateTime<Utc>, OrganizationNodeId), PortError> {
    let (timestamp, node_id) = value
        .split_once(CURSOR_SEPARATOR)
        .ok_or(PortError::Internal)?;
    let updated_at = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| PortError::Internal)?
        .with_timezone(&Utc);
    let node_id = Uuid::parse_str(node_id).map_err(|_| PortError::Internal)?;
    let node_id = OrganizationNodeId::try_from_uuid(node_id).map_err(|_| PortError::Internal)?;
    Ok((updated_at, node_id))
}

fn decode_view(row: sqlx::postgres::PgRow) -> Result<NodeView, PortError> {
    let value: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    let view: NodeView = serde_json::from_value(value).map_err(|_| PortError::Internal)?;
    normalize_view(view)
}

fn normalize_view(mut view: NodeView) -> Result<NodeView, PortError> {
    if view.kind != view.placement.kind()
        || view.parent_id != view.placement.parent_id(view.node_id)
    {
        return Err(PortError::Internal);
    }
    // Disposable rows written before node-root scopes retain the legacy
    // company/branch witness. The typed placement is the stable source for a
    // deterministic read upcast until the generation is rebuilt.
    view.scope = canonical_node_scope(&view.placement, view.node_id, &view.scope)
        .map_err(|_| PortError::Internal)?;
    Ok(view)
}

fn map_read_error(error: CqrsSqlxError) -> PortError {
    match error {
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use erpx_organization::{OrganizationNode, OrganizationPlacement};

    use super::*;

    #[test]
    fn cursor_round_trips_without_precision_loss() {
        let at = Utc
            .with_ymd_and_hms(2026, 8, 5, 14, 30, 0)
            .single()
            .unwrap();
        let id = OrganizationNodeId::try_from_uuid(Uuid::from_u128(7)).unwrap();
        assert_eq!(decode_cursor(&encode_cursor(at, id)), Ok((at, id)));
    }

    #[test]
    fn malformed_or_nil_cursors_fail_closed() {
        assert_eq!(decode_cursor("not-a-cursor"), Err(PortError::Internal));
        assert_eq!(
            decode_cursor("2026-08-05T14:30:00Z|00000000-0000-0000-0000-000000000000"),
            Err(PortError::Internal)
        );
    }

    #[test]
    fn subtree_filter_covers_every_typed_hierarchy_position() {
        let company = ScopeFilter::from_scope(&AuthorityScope::Subtree {
            holding_id: Uuid::from_u128(1),
            organization_node_id: Uuid::from_u128(2),
        })
        .unwrap();
        assert!(company.exact.is_empty());
        assert!(company.prefixes.iter().any(|value| value
            == "h:00000000-0000-0000-0000-000000000001/c:00000000-0000-0000-0000-000000000002"));
        assert!(
            company
                .prefixes
                .iter()
                .any(|value| value.contains("/b:%/d:"))
        );
        assert!(company.prefixes.iter().any(|value| value.contains("/t:")));
        assert_eq!(company.subtree_node_id, Some(Uuid::from_u128(2)));

        let holding = ScopeFilter::from_scope(&AuthorityScope::Subtree {
            holding_id: Uuid::from_u128(1),
            organization_node_id: Uuid::from_u128(1),
        })
        .unwrap();
        assert_eq!(
            holding.exact,
            vec!["h:00000000-0000-0000-0000-000000000001"]
        );
        assert_eq!(holding.prefixes.len(), 1);
        assert_eq!(holding.subtree_node_id, Some(Uuid::from_u128(1)));
    }

    #[test]
    fn legacy_projection_view_is_upcast_without_changing_placement() {
        let node = OrganizationNode::create(
            OrganizationNodeId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
            OrganizationPlacement::Department {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(2),
                branch_id: Some(Uuid::from_u128(3)),
            },
            "OPS".into(),
            "Operations".into(),
            Utc.with_ymd_and_hms(2026, 8, 5, 14, 30, 0)
                .single()
                .unwrap(),
        )
        .unwrap();
        let mut legacy = NodeView::from(&node);
        legacy.scope = AuthorityScope::Branch {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: Uuid::from_u128(3),
        };

        let normalized = normalize_view(legacy.clone()).unwrap();
        assert_eq!(
            normalized.scope,
            AuthorityScope::Subtree {
                holding_id: Uuid::from_u128(1),
                organization_node_id: Uuid::from_u128(4),
            }
        );
        assert_eq!(
            normalized.parent_id,
            Some(OrganizationNodeId::try_from_uuid(Uuid::from_u128(3)).unwrap())
        );

        let mut contradictory = normalized;
        contradictory.kind = erpx_organization::OrganizationNodeKind::Team;
        assert_eq!(normalize_view(contradictory), Err(PortError::Internal));

        legacy.scope = AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(99),
        };
        assert_eq!(normalize_view(legacy), Err(PortError::Internal));
    }
}
