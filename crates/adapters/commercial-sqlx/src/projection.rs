use async_trait::async_trait;
use erpx_commercial::{
    CatalogPlanStatus, CatalogPlanView, EntitlementView, SubscriptionStatus, SubscriptionView,
};
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{ItemDescriptorDraft, commercial};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    CATALOG_PLAN_AGGREGATE, CATALOG_PLAN_PROJECTION, CATALOG_PLAN_SNAPSHOT_EVENT, CONTEXT_NAME,
    ENTITLEMENT_AGGREGATE, ENTITLEMENT_PROJECTION, ENTITLEMENT_SNAPSHOT_EVENT, EVENT_VERSION,
    SUBSCRIPTION_AGGREGATE, SUBSCRIPTION_PROJECTION, SUBSCRIPTION_SNAPSHOT_EVENT,
    catalog_plan_partition, entitlement_partition, subscription_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "view", rename_all = "snake_case")]
pub enum CommercialSnapshot {
    CatalogPlan(CatalogPlanView),
    Subscription(SubscriptionView),
    Entitlement(EntitlementView),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommercialOutboxPayload {
    Integration(commercial::Event),
    Snapshot(CommercialSnapshot),
    Directory(ItemDescriptorDraft),
}

macro_rules! projector_type {
    ($name:ident, $projection:expr, $event:expr) => {
        #[derive(Debug, Clone)]
        pub struct $name {
            event_types: Vec<String>,
        }

        impl $name {
            pub fn new() -> Self {
                Self {
                    event_types: vec![$event.into()],
                }
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl ProjectorIdentity for $name {
            fn projection_name(&self) -> &'static str {
                $projection
            }
            fn accepted_event(&self) -> &'static str {
                $event
            }
            fn event_types_ref(&self) -> &[String] {
                &self.event_types
            }
        }
    };
}

trait ProjectorIdentity {
    fn projection_name(&self) -> &'static str;
    fn accepted_event(&self) -> &'static str;
    fn event_types_ref(&self) -> &[String];
}

projector_type!(
    CatalogPlanProjector,
    CATALOG_PLAN_PROJECTION,
    CATALOG_PLAN_SNAPSHOT_EVENT
);
projector_type!(
    SubscriptionProjector,
    SUBSCRIPTION_PROJECTION,
    SUBSCRIPTION_SNAPSHOT_EVENT
);
projector_type!(
    EntitlementProjector,
    ENTITLEMENT_PROJECTION,
    ENTITLEMENT_SNAPSHOT_EVENT
);

#[async_trait]
impl Projector for CatalogPlanProjector {
    fn name(&self) -> &str {
        self.projection_name()
    }
    fn event_types(&self) -> &[String] {
        self.event_types_ref()
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        validate_envelope(self, context, event, CATALOG_PLAN_AGGREGATE)?;
        let CommercialSnapshot::CatalogPlan(view) = decode_snapshot(event)? else {
            return Err(ProjectionFailure::new(
                "commercial catalog event carried another snapshot type",
            ));
        };
        if view.version != event.aggregate_version
            || view.id.as_uuid() != event.aggregate_id.as_uuid()
            || event.active_scope != AuthorityScope::Platform
            || event.partition != catalog_plan_partition(view.id)
        {
            return Err(ProjectionFailure::new(
                "commercial catalog snapshot metadata mismatch",
            ));
        }
        let state = serde_json::to_value(&view).map_err(failure)?;
        let result = sqlx::query(
            "INSERT INTO erpx_commercial_read.catalog_plan_projection
                (projection_name, generation, partition_key, catalog_plan_id,
                 source_aggregate_version, source_global_sequence, plan_key, display_name,
                 amount_minor, currency, plan_status, state, aggregate_updated_at, projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,now())
             ON CONFLICT (projection_name,generation,partition_key,catalog_plan_id)
             DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
               source_global_sequence=EXCLUDED.source_global_sequence, plan_key=EXCLUDED.plan_key,
               display_name=EXCLUDED.display_name, amount_minor=EXCLUDED.amount_minor,
               currency=EXCLUDED.currency, plan_status=EXCLUDED.plan_status, state=EXCLUDED.state,
               aggregate_updated_at=EXCLUDED.aggregate_updated_at, projected_at=now()
             WHERE erpx_commercial_read.catalog_plan_projection.source_aggregate_version < EXCLUDED.source_aggregate_version"
        ).bind(CATALOG_PLAN_PROJECTION).bind(context.generation).bind(&event.partition)
         .bind(view.id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?)
         .bind(&view.key).bind(&view.display_name).bind(view.amount_minor).bind(view.currency.as_str())
         .bind(catalog_status(view.status)).bind(state).bind(view.updated_at)
         .execute(&mut **transaction).await.map_err(failure)?;
        ensure_advanced(result.rows_affected(), "catalog plan")
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset_table(
            transaction,
            context,
            "erpx_commercial_read.catalog_plan_projection",
        )
        .await
    }
}

#[async_trait]
impl Projector for SubscriptionProjector {
    fn name(&self) -> &str {
        self.projection_name()
    }
    fn event_types(&self) -> &[String] {
        self.event_types_ref()
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        validate_envelope(self, context, event, SUBSCRIPTION_AGGREGATE)?;
        let CommercialSnapshot::Subscription(view) = decode_snapshot(event)? else {
            return Err(ProjectionFailure::new(
                "commercial subscription event carried another snapshot type",
            ));
        };
        let scope_key = holding_scope_key(&view.scope)?;
        if view.version != event.aggregate_version
            || view.id.as_uuid() != event.aggregate_id.as_uuid()
            || view.scope != event.active_scope
            || event.partition != subscription_partition(view.id)
        {
            return Err(ProjectionFailure::new(
                "commercial subscription snapshot metadata mismatch",
            ));
        }
        let state = serde_json::to_value(&view).map_err(failure)?;
        let result = sqlx::query(
            "INSERT INTO erpx_commercial_read.subscription_projection
                (projection_name,generation,partition_key,subscription_id,source_aggregate_version,
                 source_global_sequence,active_scope,scope_key,customer_id,catalog_plan_id,
                 subscription_status,amount_minor,currency,state,aggregate_updated_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,now())
             ON CONFLICT (holding_id,projection_name,generation,partition_key,subscription_id)
             DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
               source_global_sequence=EXCLUDED.source_global_sequence,active_scope=EXCLUDED.active_scope,
               scope_key=EXCLUDED.scope_key,customer_id=EXCLUDED.customer_id,
               catalog_plan_id=EXCLUDED.catalog_plan_id,subscription_status=EXCLUDED.subscription_status,
               amount_minor=EXCLUDED.amount_minor,currency=EXCLUDED.currency,state=EXCLUDED.state,
               aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
             WHERE erpx_commercial_read.subscription_projection.source_aggregate_version < EXCLUDED.source_aggregate_version"
        ).bind(SUBSCRIPTION_PROJECTION).bind(context.generation).bind(&event.partition)
         .bind(view.id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?)
         .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key)
         .bind(view.customer_id).bind(view.plan_id.as_uuid()).bind(subscription_status(view.status))
         .bind(view.amount_minor).bind(view.currency.as_str()).bind(state).bind(view.updated_at)
         .execute(&mut **transaction).await.map_err(failure)?;
        ensure_advanced(result.rows_affected(), "subscription")
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset_table(
            transaction,
            context,
            "erpx_commercial_read.subscription_projection",
        )
        .await
    }
}

#[async_trait]
impl Projector for EntitlementProjector {
    fn name(&self) -> &str {
        self.projection_name()
    }
    fn event_types(&self) -> &[String] {
        self.event_types_ref()
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        validate_envelope(self, context, event, ENTITLEMENT_AGGREGATE)?;
        let CommercialSnapshot::Entitlement(view) = decode_snapshot(event)? else {
            return Err(ProjectionFailure::new(
                "commercial entitlement event carried another snapshot type",
            ));
        };
        let scope_key = holding_scope_key(&view.scope)?;
        if view.version != event.aggregate_version
            || view.id.as_uuid() != event.aggregate_id.as_uuid()
            || view.scope != event.active_scope
            || event.partition != entitlement_partition(view.id)
        {
            return Err(ProjectionFailure::new(
                "commercial entitlement snapshot metadata mismatch",
            ));
        }
        let state = serde_json::to_value(&view).map_err(failure)?;
        let result = sqlx::query(
            "INSERT INTO erpx_commercial_read.entitlement_projection
                (projection_name,generation,partition_key,entitlement_id,source_aggregate_version,
                 source_global_sequence,active_scope,scope_key,subscription_id,capability,enabled,
                 seat_capacity,company_capacity,branch_capacity,allocated_seats,allocated_companies,
                 allocated_branches,state,aggregate_updated_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,now())
             ON CONFLICT (holding_id,projection_name,generation,partition_key,entitlement_id)
             DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
               source_global_sequence=EXCLUDED.source_global_sequence,active_scope=EXCLUDED.active_scope,
               scope_key=EXCLUDED.scope_key,subscription_id=EXCLUDED.subscription_id,
               capability=EXCLUDED.capability,enabled=EXCLUDED.enabled,seat_capacity=EXCLUDED.seat_capacity,
               company_capacity=EXCLUDED.company_capacity,branch_capacity=EXCLUDED.branch_capacity,
               allocated_seats=EXCLUDED.allocated_seats,allocated_companies=EXCLUDED.allocated_companies,
               allocated_branches=EXCLUDED.allocated_branches,state=EXCLUDED.state,
               aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
             WHERE erpx_commercial_read.entitlement_projection.source_aggregate_version < EXCLUDED.source_aggregate_version"
        ).bind(ENTITLEMENT_PROJECTION).bind(context.generation).bind(&event.partition)
         .bind(view.id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?)
         .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key)
         .bind(view.subscription_id.as_uuid()).bind(&view.capability).bind(view.enabled)
         .bind(i64::from(view.capacity.seats)).bind(i64::from(view.capacity.companies)).bind(i64::from(view.capacity.branches))
         .bind(i64::from(view.allocated.seats)).bind(i64::from(view.allocated.companies)).bind(i64::from(view.allocated.branches))
         .bind(state).bind(view.updated_at).execute(&mut **transaction).await.map_err(failure)?;
        ensure_advanced(result.rows_affected(), "entitlement")
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset_table(
            transaction,
            context,
            "erpx_commercial_read.entitlement_projection",
        )
        .await
    }
}

fn validate_envelope(
    projector: &impl ProjectorIdentity,
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    aggregate_type: &str,
) -> Result<(), ProjectionFailure> {
    if context.projection != projector.projection_name()
        || event.context != CONTEXT_NAME
        || event.aggregate_type != aggregate_type
        || event.event_type != projector.accepted_event()
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "commercial projector received an incompatible event contract",
        ));
    }
    Ok(())
}

fn decode_snapshot(event: &ProjectionEvent) -> Result<CommercialSnapshot, ProjectionFailure> {
    let payload: CommercialOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    match payload {
        CommercialOutboxPayload::Snapshot(snapshot) => Ok(snapshot),
        CommercialOutboxPayload::Integration(_) | CommercialOutboxPayload::Directory(_) => Err(
            ProjectionFailure::new("commercial snapshot event carried an integration payload"),
        ),
    }
}

fn holding_scope_key(scope: &AuthorityScope) -> Result<String, ProjectionFailure> {
    match scope {
        AuthorityScope::Holding { holding_id } if !holding_id.is_nil() => {
            Ok(format!("holding:{holding_id}"))
        }
        _ => Err(ProjectionFailure::new(
            "commercial holding snapshot is not holding scoped",
        )),
    }
}

async fn reset_table(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    table: &str,
) -> Result<(), ProjectionFailure> {
    let statement = format!("DELETE FROM {table} WHERE projection_name = $1 AND generation = $2");
    sqlx::query(&statement)
        .bind(context.projection)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
    Ok(())
}

fn ensure_advanced(rows: u64, aggregate: &str) -> Result<(), ProjectionFailure> {
    if rows == 1 {
        Ok(())
    } else {
        Err(ProjectionFailure::new(format!(
            "commercial {aggregate} version did not advance"
        )))
    }
}

pub(crate) const fn catalog_status(value: CatalogPlanStatus) -> &'static str {
    match value {
        CatalogPlanStatus::Draft => "draft",
        CatalogPlanStatus::Published => "published",
    }
}

pub(crate) const fn subscription_status(value: SubscriptionStatus) -> &'static str {
    match value {
        SubscriptionStatus::Trial => "trial",
        SubscriptionStatus::Active => "active",
        SubscriptionStatus::PastDue => "past_due",
        SubscriptionStatus::Suspended => "suspended",
        SubscriptionStatus::Ended => "ended",
    }
}

fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value)
        .map_err(|_| ProjectionFailure::new("commercial numeric value is too large"))
}

fn failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projectors_consume_only_their_private_snapshot_contract() {
        let catalog = CatalogPlanProjector::new();
        let subscription = SubscriptionProjector::new();
        let entitlement = EntitlementProjector::new();
        assert_eq!(
            catalog.event_types(),
            &[CATALOG_PLAN_SNAPSHOT_EVENT.to_owned()]
        );
        assert_eq!(
            subscription.event_types(),
            &[SUBSCRIPTION_SNAPSHOT_EVENT.to_owned()]
        );
        assert_eq!(
            entitlement.event_types(),
            &[ENTITLEMENT_SNAPSHOT_EVENT.to_owned()]
        );
    }

    #[test]
    fn frozen_enum_names_are_stable() {
        assert_eq!(catalog_status(CatalogPlanStatus::Published), "published");
        assert_eq!(subscription_status(SubscriptionStatus::PastDue), "past_due");
    }

    #[test]
    fn holding_scope_key_rejects_broader_or_narrower_scope() {
        assert!(holding_scope_key(&AuthorityScope::Platform).is_err());
        assert!(
            holding_scope_key(&AuthorityScope::Company {
                holding_id: uuid::Uuid::from_u128(1),
                company_id: uuid::Uuid::from_u128(2)
            })
            .is_err()
        );
    }
}
