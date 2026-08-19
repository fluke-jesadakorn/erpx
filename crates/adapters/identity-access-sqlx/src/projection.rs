use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_identity_access::{PrincipalStatus, PrincipalView, RoleView};
use erpx_integration_contracts::{AuthorityEvidenceFact, ItemDescriptorDraft, identity_access};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    CONTEXT_NAME, EVENT_VERSION, PRINCIPAL_AGGREGATE_TYPE, PRINCIPAL_PROJECTION_NAME,
    PRINCIPAL_SNAPSHOT_EVENT_TYPE, ROLE_AGGREGATE_TYPE, ROLE_PROJECTION_NAME,
    ROLE_SNAPSHOT_EVENT_TYPE,
    command::{risk_name, scope_key},
    principal_partition, role_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "view", rename_all = "snake_case")]
pub enum IdentityAccessSnapshot {
    Principal(PrincipalView),
    Role(RoleView),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "payload_kind", content = "payload", rename_all = "snake_case")]
pub enum IdentityAccessOutboxPayload {
    Integration(identity_access::Event),
    Snapshot(IdentityAccessSnapshot),
    Directory(ItemDescriptorDraft),
    Authority(AuthorityEvidenceFact),
}

#[derive(Debug, Clone)]
pub struct IdentityAccessPrincipalProjector {
    event_types: Vec<String>,
}

impl IdentityAccessPrincipalProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![PRINCIPAL_SNAPSHOT_EVENT_TYPE.into()],
        }
    }
}

impl Default for IdentityAccessPrincipalProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for IdentityAccessPrincipalProjector {
    fn name(&self) -> &str {
        PRINCIPAL_PROJECTION_NAME
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
        let view = validate_principal_event(context, event)?;
        let state = serde_json::to_value(&view).map_err(projection_failure)?;
        let applied = sqlx::query(
            "INSERT INTO erpx_identity_access_read.principal_projection
                (projection_name, generation, partition_key, principal_id,
                 source_aggregate_version, source_global_sequence, active_scope,
                 scope_key, display_name, principal_status, state,
                 aggregate_created_at, aggregate_updated_at, projected_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, now())
             ON CONFLICT (holding_id,projection_name, generation, partition_key, principal_id)
             DO UPDATE SET source_aggregate_version = EXCLUDED.source_aggregate_version,
                           source_global_sequence = EXCLUDED.source_global_sequence,
                           active_scope = EXCLUDED.active_scope,
                           scope_key = EXCLUDED.scope_key,
                           display_name = EXCLUDED.display_name,
                           principal_status = EXCLUDED.principal_status,
                           state = EXCLUDED.state,
                           aggregate_created_at = EXCLUDED.aggregate_created_at,
                           aggregate_updated_at = EXCLUDED.aggregate_updated_at,
                           projected_at = now()
             WHERE erpx_identity_access_read.principal_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(PRINCIPAL_PROJECTION_NAME)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(view.principal_id.as_uuid())
        .bind(to_i64(view.version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(serde_json::to_value(&view.scope).map_err(projection_failure)?)
        .bind(scope_key(&view.scope))
        .bind(&view.display_name)
        .bind(status_name(view.status))
        .bind(state)
        .bind(view.created_at)
        .bind(view.updated_at)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;

        // Duplicate and out-of-order snapshots are safe no-ops. The shared
        // runner still records the event receipt/checkpoint transactionally.
        if applied.rows_affected() == 0 {
            return Ok(());
        }
        sqlx::query(
            "DELETE FROM erpx_identity_access_read.assignment_projection
             WHERE projection_name = $1 AND generation = $2
               AND partition_key = $3 AND principal_id = $4",
        )
        .bind(PRINCIPAL_PROJECTION_NAME)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(view.principal_id.as_uuid())
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        for assignment in &view.assignments {
            sqlx::query(
                "INSERT INTO erpx_identity_access_read.assignment_projection
                    (projection_name, generation, partition_key, principal_id,
                     assignment_id, role_id, assignment_scope, scope_key, risk_tier,
                     valid_from, valid_until, revoked_at, state, projected_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, now())",
            )
            .bind(PRINCIPAL_PROJECTION_NAME)
            .bind(context.generation)
            .bind(&event.partition)
            .bind(view.principal_id.as_uuid())
            .bind(assignment.assignment_id.as_uuid())
            .bind(assignment.role_id.as_uuid())
            .bind(serde_json::to_value(&assignment.scope).map_err(projection_failure)?)
            .bind(scope_key(&assignment.scope))
            .bind(risk_name(assignment.risk_tier))
            .bind(assignment.valid_from)
            .bind(assignment.valid_until)
            .bind(assignment.revoked_at)
            .bind(serde_json::to_value(assignment).map_err(projection_failure)?)
            .execute(&mut **transaction)
            .await
            .map_err(projection_failure)?;
        }
        Ok(())
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query(
            "DELETE FROM erpx_identity_access_read.assignment_projection
             WHERE projection_name = $1 AND generation = $2",
        )
        .bind(PRINCIPAL_PROJECTION_NAME)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        sqlx::query(
            "DELETE FROM erpx_identity_access_read.principal_projection
             WHERE projection_name = $1 AND generation = $2",
        )
        .bind(PRINCIPAL_PROJECTION_NAME)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct IdentityAccessRoleProjector {
    event_types: Vec<String>,
}

impl IdentityAccessRoleProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![ROLE_SNAPSHOT_EVENT_TYPE.into()],
        }
    }
}

impl Default for IdentityAccessRoleProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for IdentityAccessRoleProjector {
    fn name(&self) -> &str {
        ROLE_PROJECTION_NAME
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
        let view = validate_role_event(context, event)?;
        sqlx::query(
            "INSERT INTO erpx_identity_access_read.role_projection
                (projection_name, generation, partition_key, role_id,
                 source_aggregate_version, source_global_sequence, active_scope,
                 scope_key, role_code, display_name, risk_tier, permissions,
                 state, aggregate_created_at, aggregate_updated_at, projected_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, now())
             ON CONFLICT (holding_id,projection_name, generation, partition_key, role_id)
             DO UPDATE SET source_aggregate_version = EXCLUDED.source_aggregate_version,
                           source_global_sequence = EXCLUDED.source_global_sequence,
                           active_scope = EXCLUDED.active_scope,
                           scope_key = EXCLUDED.scope_key,
                           role_code = EXCLUDED.role_code,
                           display_name = EXCLUDED.display_name,
                           risk_tier = EXCLUDED.risk_tier,
                           permissions = EXCLUDED.permissions,
                           state = EXCLUDED.state,
                           aggregate_created_at = EXCLUDED.aggregate_created_at,
                           aggregate_updated_at = EXCLUDED.aggregate_updated_at,
                           projected_at = now()
             WHERE erpx_identity_access_read.role_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(ROLE_PROJECTION_NAME)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(view.role_id.as_uuid())
        .bind(to_i64(view.version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(serde_json::to_value(&view.scope).map_err(projection_failure)?)
        .bind(scope_key(&view.scope))
        .bind(view.code.as_str())
        .bind(&view.display_name)
        .bind(risk_name(view.risk_tier))
        .bind(serde_json::to_value(&view.permissions).map_err(projection_failure)?)
        .bind(serde_json::to_value(&view).map_err(projection_failure)?)
        .bind(view.created_at)
        .bind(view.updated_at)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        Ok(())
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query(
            "DELETE FROM erpx_identity_access_read.role_projection
             WHERE projection_name = $1 AND generation = $2",
        )
        .bind(ROLE_PROJECTION_NAME)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        Ok(())
    }
}

fn validate_principal_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<PrincipalView, ProjectionFailure> {
    if context.projection != PRINCIPAL_PROJECTION_NAME
        || event.context != CONTEXT_NAME
        || event.aggregate_type != PRINCIPAL_AGGREGATE_TYPE
        || event.event_type != PRINCIPAL_SNAPSHOT_EVENT_TYPE
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "identity access principal projector received an incompatible event",
        ));
    }
    let payload: IdentityAccessOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(projection_failure)?;
    let IdentityAccessOutboxPayload::Snapshot(IdentityAccessSnapshot::Principal(view)) = payload
    else {
        return Err(ProjectionFailure::new(
            "identity access principal event carried the wrong payload",
        ));
    };
    if view.version != event.aggregate_version
        || view.principal_id.as_uuid() != event.aggregate_id.as_uuid()
        || view.scope != event.active_scope
        || event.partition != principal_partition(view.principal_id)
    {
        return Err(ProjectionFailure::new(
            "identity access principal snapshot metadata does not match its envelope",
        ));
    }
    Ok(view)
}

fn validate_role_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<RoleView, ProjectionFailure> {
    if context.projection != ROLE_PROJECTION_NAME
        || event.context != CONTEXT_NAME
        || event.aggregate_type != ROLE_AGGREGATE_TYPE
        || event.event_type != ROLE_SNAPSHOT_EVENT_TYPE
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "identity access role projector received an incompatible event",
        ));
    }
    let payload: IdentityAccessOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(projection_failure)?;
    let IdentityAccessOutboxPayload::Snapshot(IdentityAccessSnapshot::Role(view)) = payload else {
        return Err(ProjectionFailure::new(
            "identity access role event carried the wrong payload",
        ));
    };
    if view.version != event.aggregate_version
        || view.role_id.as_uuid() != event.aggregate_id.as_uuid()
        || view.scope != event.active_scope
        || event.partition != role_partition(view.role_id)
    {
        return Err(ProjectionFailure::new(
            "identity access role snapshot metadata does not match its envelope",
        ));
    }
    Ok(view)
}

pub(crate) const fn status_name(status: PrincipalStatus) -> &'static str {
    match status {
        PrincipalStatus::Invited => "invited",
        PrincipalStatus::Active => "active",
        PrincipalStatus::Suspended => "suspended",
        PrincipalStatus::Disabled => "disabled",
    }
}

fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value)
        .map_err(|_| ProjectionFailure::new("identity access numeric value is too large"))
}

fn projection_failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use erpx_cqrs_sqlx::Projector;
    use erpx_identity_access::RiskTier;

    use super::*;

    #[test]
    fn projectors_consume_only_their_private_snapshot_contract() {
        let principals = IdentityAccessPrincipalProjector::new();
        let roles = IdentityAccessRoleProjector::new();
        assert_eq!(principals.name(), PRINCIPAL_PROJECTION_NAME);
        assert_eq!(principals.event_types(), &[PRINCIPAL_SNAPSHOT_EVENT_TYPE]);
        assert_eq!(roles.name(), ROLE_PROJECTION_NAME);
        assert_eq!(roles.event_types(), &[ROLE_SNAPSHOT_EVENT_TYPE]);
    }

    #[test]
    fn schema_enum_names_are_stable() {
        assert_eq!(status_name(PrincipalStatus::Suspended), "suspended");
        assert_eq!(risk_name(RiskTier::SupportDiagnostic), "support_diagnostic");
    }
}
