use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{ItemDescriptorDraft, organization};
use erpx_organization::{
    NodeView, OrganizationNodeKind, OrganizationNodeStatus, OrganizationPlacement,
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, LEGACY_NODE_SNAPSHOT_EVENT_VERSION, NODE_SNAPSHOT_EVENT_VERSION,
    PROJECTION_NAME, SNAPSHOT_EVENT_TYPE, canonical_node_scope, node_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationSnapshot {
    pub node: NodeView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrganizationOutboxPayload {
    Integration(organization::Event),
    Snapshot(OrganizationSnapshot),
    Directory(ItemDescriptorDraft),
}

#[derive(Debug, Clone)]
pub struct OrganizationNodeProjector {
    event_types: Vec<String>,
}

impl OrganizationNodeProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![SNAPSHOT_EVENT_TYPE.into()],
        }
    }
}

impl Default for OrganizationNodeProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for OrganizationNodeProjector {
    fn name(&self) -> &str {
        PROJECTION_NAME
    }

    fn event_types(&self) -> &[String] {
        &self.event_types
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let snapshot = validate_event(context, event)?;
        let view = snapshot.node;
        let state = serde_json::to_value(&view).map_err(projection_failure)?;
        let result = sqlx::query(
            "INSERT INTO erpx_organization_read.node_projection
                (projection_name, generation, partition_key, node_id,
                 source_aggregate_version, source_global_sequence, node_kind,
                 node_status, parent_node_id, holding_scope, scope_key, placement,
                 node_code, display_name, state, aggregate_created_at,
                 aggregate_updated_at, projected_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                     $13, $14, $15, $16, $17, now())
             ON CONFLICT (holding_id,projection_name, generation, partition_key, node_id)
             DO UPDATE SET source_aggregate_version = EXCLUDED.source_aggregate_version,
                           source_global_sequence = EXCLUDED.source_global_sequence,
                           node_kind = EXCLUDED.node_kind,
                           node_status = EXCLUDED.node_status,
                           parent_node_id = EXCLUDED.parent_node_id,
                           holding_scope = EXCLUDED.holding_scope,
                           scope_key = EXCLUDED.scope_key,
                           placement = EXCLUDED.placement,
                           node_code = EXCLUDED.node_code,
                           display_name = EXCLUDED.display_name,
                           state = EXCLUDED.state,
                           aggregate_created_at = EXCLUDED.aggregate_created_at,
                           aggregate_updated_at = EXCLUDED.aggregate_updated_at,
                           projected_at = now()
             WHERE erpx_organization_read.node_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(PROJECTION_NAME)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(view.node_id.as_uuid())
        .bind(to_i64(view.version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(kind_name(view.kind))
        .bind(status_name(view.status))
        .bind(view.parent_id.map(|id| id.as_uuid()))
        .bind(serde_json::to_value(&view.scope).map_err(projection_failure)?)
        .bind(scope_key(&view)?)
        .bind(serde_json::to_value(&view.placement).map_err(projection_failure)?)
        .bind(&view.code)
        .bind(&view.name)
        .bind(state)
        .bind(view.created_at)
        .bind(view.updated_at)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        if result.rows_affected() != 1 {
            return Err(ProjectionFailure::new(
                "organization snapshot aggregate version did not advance",
            ));
        }
        Ok(())
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query(
            "DELETE FROM erpx_organization_read.node_projection
             WHERE projection_name = $1 AND generation = $2",
        )
        .bind(PROJECTION_NAME)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        Ok(())
    }
}

fn validate_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<OrganizationSnapshot, ProjectionFailure> {
    if context.projection != PROJECTION_NAME {
        return Err(ProjectionFailure::new(
            "organization projector received an incompatible projection",
        ));
    }
    decode_snapshot_event(event)
}

/// Decodes both legacy and current Organization snapshots without depending
/// on a read-projection generation. Item Directory rebuilds use this path to
/// seed lineage for nodes that predate `organization.lineage_changed`.
pub fn decode_snapshot_event(
    event: &ProjectionEvent,
) -> Result<OrganizationSnapshot, ProjectionFailure> {
    if event.context != CONTEXT_NAME
        || event.aggregate_type != AGGREGATE_TYPE
        || event.event_type != SNAPSHOT_EVENT_TYPE
        || !matches!(
            event.event_version,
            LEGACY_NODE_SNAPSHOT_EVENT_VERSION | NODE_SNAPSHOT_EVENT_VERSION
        )
    {
        return Err(ProjectionFailure::new(
            "organization projector received an incompatible event contract",
        ));
    }
    let payload: OrganizationOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(projection_failure)?;
    let OrganizationOutboxPayload::Snapshot(mut snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "organization snapshot event carried an integration payload",
        ));
    };
    let view = &snapshot.node;
    if view.version != event.aggregate_version
        || view.node_id.as_uuid() != event.aggregate_id.as_uuid()
        || event.partition != node_partition(view.node_id)
        || view.kind != view.placement.kind()
        || view.parent_id != view.placement.parent_id(view.node_id)
    {
        return Err(ProjectionFailure::new(
            "organization snapshot metadata does not match its outbox envelope",
        ));
    }
    let expected_scope = view
        .placement
        .active_scope(view.node_id)
        .map_err(projection_failure)?;
    match event.event_version {
        NODE_SNAPSHOT_EVENT_VERSION => {
            if view.scope != expected_scope || event.active_scope != expected_scope {
                return Err(ProjectionFailure::new(
                    "organization v2 snapshot scope does not match its typed placement",
                ));
            }
        }
        LEGACY_NODE_SNAPSHOT_EVENT_VERSION => {
            if view.scope != event.active_scope {
                return Err(ProjectionFailure::new(
                    "legacy organization snapshot scope does not match its envelope",
                ));
            }
            snapshot.node.scope = canonical_node_scope(&view.placement, view.node_id, &view.scope)
                .map_err(ProjectionFailure::new)?;
        }
        _ => unreachable!("event version was checked above"),
    }
    Ok(snapshot)
}

/// Converts a validated historical snapshot into the same neutral lineage
/// witness emitted by current Organization commits. This is a rebuild-only
/// compatibility source; current incremental authority remains the dedicated
/// `organization.lineage_changed` fact.
pub fn lineage_witness_from_snapshot_event(
    event: &ProjectionEvent,
) -> Result<organization::OrganizationLineageWitness, ProjectionFailure> {
    let snapshot = decode_snapshot_event(event)?;
    let view = snapshot.node;
    let node_id = view.node_id;
    let placement = &view.placement;
    let key = organization::OrganizationLineageKey::try_new(
        placement.holding_id(node_id),
        node_id.as_uuid(),
    )
    .map_err(projection_failure)?;
    organization::OrganizationLineageWitness::try_new(
        key,
        integration_kind(view.kind),
        integration_status(view.status),
        placement.parent_id(node_id).map(|parent| parent.as_uuid()),
        lineage_ids(node_id, placement),
        view.version,
    )
    .map_err(projection_failure)
}

const fn integration_kind(kind: OrganizationNodeKind) -> organization::OrganizationNodeKind {
    match kind {
        OrganizationNodeKind::Holding => organization::OrganizationNodeKind::Holding,
        OrganizationNodeKind::Company => organization::OrganizationNodeKind::Company,
        OrganizationNodeKind::Headquarters => organization::OrganizationNodeKind::Headquarters,
        OrganizationNodeKind::Branch => organization::OrganizationNodeKind::Branch,
        OrganizationNodeKind::Department => organization::OrganizationNodeKind::Department,
        OrganizationNodeKind::Team => organization::OrganizationNodeKind::Team,
    }
}

const fn integration_status(
    status: OrganizationNodeStatus,
) -> organization::OrganizationNodeStatus {
    match status {
        OrganizationNodeStatus::Active => organization::OrganizationNodeStatus::Active,
        OrganizationNodeStatus::Suspended => organization::OrganizationNodeStatus::Suspended,
        OrganizationNodeStatus::Archived => organization::OrganizationNodeStatus::Archived,
    }
}

fn lineage_ids(
    node_id: erpx_organization::OrganizationNodeId,
    placement: &OrganizationPlacement,
) -> Vec<uuid::Uuid> {
    match placement {
        OrganizationPlacement::Holding => Vec::new(),
        OrganizationPlacement::Company { holding_id } => vec![*holding_id],
        OrganizationPlacement::Headquarters {
            holding_id,
            company_id,
        }
        | OrganizationPlacement::Branch {
            holding_id,
            company_id,
        } => vec![*holding_id, *company_id],
        OrganizationPlacement::Department {
            holding_id,
            company_id,
            branch_id,
        } => {
            let mut values = vec![*holding_id, *company_id];
            values.extend(branch_id);
            values
        }
        OrganizationPlacement::Team {
            holding_id,
            company_id,
            branch_id,
            department_id,
        } => {
            let mut values = vec![*holding_id, *company_id];
            values.extend(branch_id);
            values.push(*department_id);
            values
        }
    }
    .into_iter()
    .filter(|id| *id != node_id.as_uuid())
    .collect()
}

pub(crate) fn scope_key(view: &NodeView) -> Result<String, ProjectionFailure> {
    view.scope
        .validate()
        .map_err(|_| ProjectionFailure::new("organization scope is invalid"))?;
    let node_id = view.node_id.as_uuid();
    Ok(match &view.placement {
        OrganizationPlacement::Holding => format!("h:{node_id}"),
        OrganizationPlacement::Company { holding_id } => {
            format!("h:{holding_id}/c:{node_id}")
        }
        OrganizationPlacement::Headquarters {
            holding_id,
            company_id,
        } => format!("h:{holding_id}/c:{company_id}/hq:{node_id}"),
        OrganizationPlacement::Branch {
            holding_id,
            company_id,
        } => format!("h:{holding_id}/c:{company_id}/b:{node_id}"),
        OrganizationPlacement::Department {
            holding_id,
            company_id,
            branch_id,
        } => match branch_id {
            Some(branch_id) => {
                format!("h:{holding_id}/c:{company_id}/b:{branch_id}/d:{node_id}")
            }
            None => format!("h:{holding_id}/c:{company_id}/d:{node_id}"),
        },
        OrganizationPlacement::Team {
            holding_id,
            company_id,
            branch_id,
            department_id,
        } => match branch_id {
            Some(branch_id) => {
                format!("h:{holding_id}/c:{company_id}/b:{branch_id}/d:{department_id}/t:{node_id}")
            }
            None => {
                format!("h:{holding_id}/c:{company_id}/d:{department_id}/t:{node_id}")
            }
        },
    })
}

pub(crate) const fn kind_name(kind: OrganizationNodeKind) -> &'static str {
    match kind {
        OrganizationNodeKind::Holding => "holding",
        OrganizationNodeKind::Company => "company",
        OrganizationNodeKind::Headquarters => "headquarters",
        OrganizationNodeKind::Branch => "branch",
        OrganizationNodeKind::Department => "department",
        OrganizationNodeKind::Team => "team",
    }
}

pub(crate) const fn status_name(status: OrganizationNodeStatus) -> &'static str {
    match status {
        OrganizationNodeStatus::Active => "active",
        OrganizationNodeStatus::Suspended => "suspended",
        OrganizationNodeStatus::Archived => "archived",
    }
}

fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value)
        .map_err(|_| ProjectionFailure::new("organization numeric value is too large"))
}

fn projection_failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use erpx_kernel::{
        ActorAttribution, ActorKind, AggregateId, AuthorityScope, CommandId, CorrelationId, EventId,
    };
    use erpx_organization::{OrganizationNode, OrganizationNodeId};
    use uuid::Uuid;

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn view(value: u128, placement: OrganizationPlacement) -> NodeView {
        let node_id = OrganizationNodeId::try_from_uuid(id(value)).unwrap();
        let aggregate = OrganizationNode::create(
            node_id,
            placement,
            format!("NODE-{value}"),
            format!("Node {value}"),
            chrono::Utc
                .with_ymd_and_hms(2026, 8, 21, 9, 0, 0)
                .single()
                .unwrap(),
        )
        .unwrap();
        NodeView::from(&aggregate)
    }

    fn event(view: NodeView, event_version: u32, active_scope: AuthorityScope) -> ProjectionEvent {
        ProjectionEvent {
            event_id: EventId::try_from_uuid(id(10)).unwrap(),
            global_sequence: 1,
            context: CONTEXT_NAME.into(),
            partition: node_partition(view.node_id),
            command_id: CommandId::try_from_uuid(id(11)).unwrap(),
            event_type: SNAPSHOT_EVENT_TYPE.into(),
            event_version,
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_id: AggregateId::try_from_uuid(view.node_id.as_uuid()).unwrap(),
            aggregate_version: view.version,
            active_scope,
            actor: ActorAttribution {
                principal_ref: "legacy-projector-test".into(),
                kind: ActorKind::Service,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            correlation_id: CorrelationId::try_from_uuid(id(12)).unwrap(),
            causation_id: None,
            payload: serde_json::to_value(OrganizationOutboxPayload::Snapshot(
                OrganizationSnapshot { node: view },
            ))
            .unwrap(),
            occurred_at: chrono::Utc
                .with_ymd_and_hms(2026, 8, 21, 9, 0, 0)
                .single()
                .unwrap(),
        }
    }

    #[test]
    fn projector_consumes_only_private_snapshot_events() {
        let projector = OrganizationNodeProjector::new();
        assert_eq!(projector.name(), PROJECTION_NAME);
        assert_eq!(projector.event_types(), &[SNAPSHOT_EVENT_TYPE.to_owned()]);
        assert!(
            !projector
                .event_types()
                .iter()
                .any(|event| event == "organization.unit_changed")
        );
    }

    #[test]
    fn database_names_are_stable() {
        assert_eq!(
            kind_name(OrganizationNodeKind::Headquarters),
            "headquarters"
        );
        assert_eq!(status_name(OrganizationNodeStatus::Archived), "archived");
    }

    #[test]
    fn scope_keys_preserve_department_and_team_lineage() {
        let department = view(
            4,
            OrganizationPlacement::Department {
                holding_id: id(1),
                company_id: id(2),
                branch_id: Some(id(3)),
            },
        );
        assert_eq!(
            scope_key(&department).unwrap(),
            concat!(
                "h:00000000-0000-0000-0000-000000000001/",
                "c:00000000-0000-0000-0000-000000000002/",
                "b:00000000-0000-0000-0000-000000000003/",
                "d:00000000-0000-0000-0000-000000000004"
            )
        );

        let team = view(
            5,
            OrganizationPlacement::Team {
                holding_id: id(1),
                company_id: id(2),
                branch_id: Some(id(3)),
                department_id: id(4),
            },
        );
        assert_eq!(
            scope_key(&team).unwrap(),
            concat!(
                "h:00000000-0000-0000-0000-000000000001/",
                "c:00000000-0000-0000-0000-000000000002/",
                "b:00000000-0000-0000-0000-000000000003/",
                "d:00000000-0000-0000-0000-000000000004/",
                "t:00000000-0000-0000-0000-000000000005"
            )
        );
    }

    #[test]
    fn legacy_department_snapshot_is_upcast_but_v2_requires_exact_scope() {
        let mut department = view(
            4,
            OrganizationPlacement::Department {
                holding_id: id(1),
                company_id: id(2),
                branch_id: Some(id(3)),
            },
        );
        let legacy_scope = AuthorityScope::Branch {
            holding_id: id(1),
            company_id: id(2),
            branch_id: id(3),
        };
        department.scope = legacy_scope.clone();
        let context = ProjectorContext {
            projection: PROJECTION_NAME,
            generation: id(20),
        };

        let legacy_event = event(
            department.clone(),
            LEGACY_NODE_SNAPSHOT_EVENT_VERSION,
            legacy_scope.clone(),
        );
        let normalized = validate_event(&context, &legacy_event).unwrap();
        assert_eq!(
            normalized.node.scope,
            AuthorityScope::Subtree {
                holding_id: id(1),
                organization_node_id: id(4),
            }
        );
        let witness = lineage_witness_from_snapshot_event(&legacy_event).unwrap();
        assert_eq!(
            witness.kind(),
            organization::OrganizationNodeKind::Department
        );
        assert_eq!(
            witness
                .lineage_node_ids()
                .iter()
                .map(|node| node.as_uuid())
                .collect::<Vec<_>>(),
            vec![id(1), id(2), id(3)]
        );

        assert!(
            validate_event(
                &context,
                &event(department, NODE_SNAPSHOT_EVENT_VERSION, legacy_scope),
            )
            .is_err()
        );

        let mut contradictory = view(
            4,
            OrganizationPlacement::Department {
                holding_id: id(1),
                company_id: id(2),
                branch_id: Some(id(3)),
            },
        );
        contradictory.kind = OrganizationNodeKind::Team;
        let exact_scope = contradictory.scope.clone();
        assert!(
            validate_event(
                &context,
                &event(contradictory, NODE_SNAPSHOT_EVENT_VERSION, exact_scope),
            )
            .is_err()
        );
    }
}
