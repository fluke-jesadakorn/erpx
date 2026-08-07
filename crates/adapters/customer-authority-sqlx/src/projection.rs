use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionEvent, ProjectionFailure, Projector, ProjectorContext,
};
use erpx_customer_authority::{CustomerAuthorityView, CustomerStatus};
use erpx_integration_contracts::customer;
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, PROJECTION_NAME, SNAPSHOT_EVENT_TYPE,
    customer_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerAuthoritySnapshot {
    pub customer: CustomerAuthorityView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CustomerAuthorityOutboxPayload {
    Integration(customer::Event),
    Snapshot(CustomerAuthoritySnapshot),
}

#[derive(Debug, Clone)]
pub struct CustomerAuthorityProjector {
    event_types: Vec<String>,
}

impl CustomerAuthorityProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![SNAPSHOT_EVENT_TYPE.into()],
        }
    }
}

impl Default for CustomerAuthorityProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for CustomerAuthorityProjector {
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
        let view = snapshot.customer;
        let state = serde_json::to_value(&view).map_err(projection_failure)?;
        let result = sqlx::query(
            "INSERT INTO erpx_customer_authority_read.customer_projection
                (projection_name, generation, partition_key, customer_id,
                 source_aggregate_version, source_global_sequence, customer_code,
                 trading_name, legal_name, country, customer_status,
                 primary_authority, primary_scope_key, merged_into_customer_id,
                 state, aggregate_created_at, aggregate_updated_at, projected_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                     $13, $14, $15, $16, $17, now())
             ON CONFLICT (projection_name, generation, partition_key, customer_id)
             DO UPDATE SET source_aggregate_version = EXCLUDED.source_aggregate_version,
                           source_global_sequence = EXCLUDED.source_global_sequence,
                           customer_code = EXCLUDED.customer_code,
                           trading_name = EXCLUDED.trading_name,
                           legal_name = EXCLUDED.legal_name,
                           country = EXCLUDED.country,
                           customer_status = EXCLUDED.customer_status,
                           primary_authority = EXCLUDED.primary_authority,
                           primary_scope_key = EXCLUDED.primary_scope_key,
                           merged_into_customer_id = EXCLUDED.merged_into_customer_id,
                           state = EXCLUDED.state,
                           aggregate_created_at = EXCLUDED.aggregate_created_at,
                           aggregate_updated_at = EXCLUDED.aggregate_updated_at,
                           projected_at = now()
             WHERE erpx_customer_authority_read.customer_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(PROJECTION_NAME)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(view.customer_id.as_uuid())
        .bind(to_i64(view.version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(view.code.as_str())
        .bind(&view.trading_name)
        .bind(&view.legal_name)
        .bind(&view.country)
        .bind(status_name(view.status))
        .bind(serde_json::to_value(&view.primary_authority).map_err(projection_failure)?)
        .bind(scope_key(&view.primary_authority).map_err(projection_failure)?)
        .bind(view.merged_into.map(|id| id.as_uuid()))
        .bind(state)
        .bind(view.created_at)
        .bind(view.updated_at)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        // A different event carrying an older/equal snapshot can arrive after
        // a retry. It is a safe no-op; crucially, do not replace the authority
        // link children unless the parent projection version advanced.
        if result.rows_affected() == 0 {
            return Ok(());
        }

        sqlx::query(
            "DELETE FROM erpx_customer_authority_read.authority_link_projection
             WHERE projection_name = $1 AND generation = $2
               AND partition_key = $3 AND customer_id = $4",
        )
        .bind(PROJECTION_NAME)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(view.customer_id.as_uuid())
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        for authority in &view.authority_links {
            scope_parts(authority).map_err(projection_failure)?;
            sqlx::query(
                "INSERT INTO erpx_customer_authority_read.authority_link_projection
                    (projection_name, generation, partition_key, customer_id,
                     scope_key, authority_scope, is_primary, source_aggregate_version,
                     source_global_sequence, projected_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now())",
            )
            .bind(PROJECTION_NAME)
            .bind(context.generation)
            .bind(&event.partition)
            .bind(view.customer_id.as_uuid())
            .bind(scope_key(authority).map_err(projection_failure)?)
            .bind(serde_json::to_value(authority).map_err(projection_failure)?)
            .bind(authority == &view.primary_authority)
            .bind(to_i64(view.version)?)
            .bind(to_i64(event.global_sequence)?)
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
            "DELETE FROM erpx_customer_authority_read.authority_link_projection
             WHERE projection_name = $1 AND generation = $2",
        )
        .bind(PROJECTION_NAME)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        sqlx::query(
            "DELETE FROM erpx_customer_authority_read.customer_projection
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
) -> Result<CustomerAuthoritySnapshot, ProjectionFailure> {
    if context.projection != PROJECTION_NAME
        || event.context != CONTEXT_NAME
        || event.aggregate_type != AGGREGATE_TYPE
        || event.event_type != SNAPSHOT_EVENT_TYPE
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "customer authority projector received an incompatible event contract",
        ));
    }
    let payload: CustomerAuthorityOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(projection_failure)?;
    let CustomerAuthorityOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "customer authority snapshot event carried an integration payload",
        ));
    };
    let view = &snapshot.customer;
    if view.version != event.aggregate_version
        || view.customer_id.as_uuid() != event.aggregate_id.as_uuid()
        || event.partition != customer_partition(view.customer_id)
        || !event.tenant_scope.contains(&view.primary_authority)
        || !view.authority_links.contains(&view.primary_authority)
        || view
            .authority_links
            .iter()
            .any(|scope| scope_parts(scope).is_err())
    {
        return Err(ProjectionFailure::new(
            "customer authority snapshot metadata does not match its outbox envelope",
        ));
    }
    Ok(snapshot)
}

pub(crate) fn scope_key(scope: &TenantScope) -> Result<String, CqrsSqlxError> {
    let (holding_id, company_id, branch_id) = scope_parts(scope)?;
    Ok(match (company_id, branch_id) {
        (None, None) => format!("h:{holding_id}"),
        (Some(company_id), None) => format!("h:{holding_id}/c:{company_id}"),
        (Some(company_id), Some(branch_id)) => {
            format!("h:{holding_id}/c:{company_id}/b:{branch_id}")
        }
        (None, Some(_)) => unreachable!("validated exact scope cannot have branch without company"),
    })
}

pub(crate) fn scope_parts(
    scope: &TenantScope,
) -> Result<(Uuid, Option<Uuid>, Option<Uuid>), CqrsSqlxError> {
    scope
        .validate()
        .map_err(|_| CqrsSqlxError::InvalidInput("customer authority scope is invalid"))?;
    match scope {
        TenantScope::Holding { holding_id } => Ok((*holding_id, None, None)),
        TenantScope::Company {
            holding_id,
            company_id,
        } => Ok((*holding_id, Some(*company_id), None)),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok((*holding_id, Some(*company_id), Some(*branch_id))),
        TenantScope::Platform | TenantScope::Subtree { .. } => Err(CqrsSqlxError::InvalidInput(
            "customer authority link is not exact",
        )),
    }
}

pub(crate) const fn status_name(status: CustomerStatus) -> &'static str {
    match status {
        CustomerStatus::Prospect => "prospect",
        CustomerStatus::Onboarding => "onboarding",
        CustomerStatus::Active => "active",
        CustomerStatus::Suspended => "suspended",
        CustomerStatus::Churned => "churned",
    }
}

fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value)
        .map_err(|_| ProjectionFailure::new("customer authority numeric value is too large"))
}

fn projection_failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projector_consumes_only_private_safe_snapshot_events() {
        let projector = CustomerAuthorityProjector::new();
        assert_eq!(projector.name(), PROJECTION_NAME);
        assert_eq!(projector.event_types(), &[SNAPSHOT_EVENT_TYPE.to_owned()]);
        let source = include_str!("projection.rs");
        assert!(!source.contains(concat!("registration_", "number")));
        assert!(!source.contains(concat!("tax_", "identifier")));
    }

    #[test]
    fn exact_scope_keys_are_stable_and_reject_broad_scopes() {
        let scope = TenantScope::Branch {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: Uuid::from_u128(3),
        };
        assert_eq!(
            scope_key(&scope).unwrap(),
            format!(
                "h:{}/c:{}/b:{}",
                Uuid::from_u128(1),
                Uuid::from_u128(2),
                Uuid::from_u128(3)
            )
        );
        assert!(scope_key(&TenantScope::Platform).is_err());
    }
}
