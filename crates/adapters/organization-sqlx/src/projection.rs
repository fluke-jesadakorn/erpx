use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{ItemDescriptorDraft, organization};
use erpx_organization::{NodeView, OrganizationNodeKind, OrganizationNodeStatus};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, PROJECTION_NAME, SNAPSHOT_EVENT_TYPE,
    node_partition,
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
                 node_status, parent_node_id, tenant_scope, scope_key, placement,
                 node_code, display_name, state, aggregate_created_at,
                 aggregate_updated_at, projected_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                     $13, $14, $15, $16, $17, now())
             ON CONFLICT (tenant_id,projection_name, generation, partition_key, node_id)
             DO UPDATE SET source_aggregate_version = EXCLUDED.source_aggregate_version,
                           source_global_sequence = EXCLUDED.source_global_sequence,
                           node_kind = EXCLUDED.node_kind,
                           node_status = EXCLUDED.node_status,
                           parent_node_id = EXCLUDED.parent_node_id,
                           tenant_scope = EXCLUDED.tenant_scope,
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
        .bind(scope_key(&view.scope)?)
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
    if context.projection != PROJECTION_NAME
        || event.context != CONTEXT_NAME
        || event.aggregate_type != AGGREGATE_TYPE
        || event.event_type != SNAPSHOT_EVENT_TYPE
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "organization projector received an incompatible event contract",
        ));
    }
    let payload: OrganizationOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(projection_failure)?;
    let OrganizationOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "organization snapshot event carried an integration payload",
        ));
    };
    let view = &snapshot.node;
    if view.version != event.aggregate_version
        || view.node_id.as_uuid() != event.aggregate_id.as_uuid()
        || view.scope != event.tenant_scope
        || event.partition != node_partition(view.node_id)
        || view.placement.tenant_scope(view.node_id).ok().as_ref() != Some(&view.scope)
    {
        return Err(ProjectionFailure::new(
            "organization snapshot metadata does not match its outbox envelope",
        ));
    }
    Ok(snapshot)
}

pub(crate) fn scope_key(scope: &erpx_kernel::TenantScope) -> Result<String, ProjectionFailure> {
    scope
        .validate()
        .map_err(|_| ProjectionFailure::new("organization scope is invalid"))?;
    Ok(match scope {
        erpx_kernel::TenantScope::Platform => "platform".into(),
        erpx_kernel::TenantScope::Holding { holding_id } => format!("h:{holding_id}"),
        erpx_kernel::TenantScope::Company {
            holding_id,
            company_id,
        } => format!("h:{holding_id}/c:{company_id}"),
        erpx_kernel::TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("h:{holding_id}/c:{company_id}/b:{branch_id}"),
        erpx_kernel::TenantScope::Subtree { .. } => {
            return Err(ProjectionFailure::new(
                "organization nodes cannot own a subtree scope",
            ));
        }
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
    use super::*;

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
}
