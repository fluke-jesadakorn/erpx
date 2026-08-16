use crate::{
    CHANNEL_PROJECTION, CHANNEL_SNAPSHOT_EVENT, JOURNEY_PROJECTION, JOURNEY_SNAPSHOT_EVENT,
    LISTING_PROJECTION, LISTING_SNAPSHOT_EVENT,
};
use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_ecommerce::CommerceAggregate;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommerceSnapshot {
    pub aggregate: CommerceAggregate,
}
#[derive(Debug, Clone)]
pub struct CommerceChannelProjector {
    types: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct CommerceListingProjector {
    types: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct CommerceJourneyProjector {
    types: Vec<String>,
}
impl CommerceChannelProjector {
    pub fn new() -> Self {
        Self {
            types: vec![CHANNEL_SNAPSHOT_EVENT.into()],
        }
    }
}
impl Default for CommerceChannelProjector {
    fn default() -> Self {
        Self::new()
    }
}
impl CommerceListingProjector {
    pub fn new() -> Self {
        Self {
            types: vec![LISTING_SNAPSHOT_EVENT.into()],
        }
    }
}
impl Default for CommerceListingProjector {
    fn default() -> Self {
        Self::new()
    }
}
impl CommerceJourneyProjector {
    pub fn new() -> Self {
        Self {
            types: vec![JOURNEY_SNAPSHOT_EVENT.into()],
        }
    }
}
impl Default for CommerceJourneyProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for CommerceChannelProjector {
    fn name(&self) -> &str {
        CHANNEL_PROJECTION
    }
    fn event_types(&self) -> &[String] {
        &self.types
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ctx: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        validate(ctx, event, CHANNEL_PROJECTION, CHANNEL_SNAPSHOT_EVENT)?;
        let snapshot: CommerceSnapshot =
            serde_json::from_value(event.payload.clone()).map_err(failure)?;
        let CommerceAggregate::Channel(channel) = snapshot.aggregate else {
            return Err(ProjectionFailure::new(
                "commerce channel snapshot type mismatch",
            ));
        };
        metadata(&CommerceAggregate::Channel(channel.clone()), event)?;
        changed(
            sqlx::query(
                "INSERT INTO erpx_ecommerce_read.channel_projection
                 (projection_name,generation,partition_key,channel_id,scope_key,tenant_scope,
                  channel_code,channel_status,source_aggregate_version,source_global_sequence,projected_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,clock_timestamp())
                 ON CONFLICT (tenant_id,projection_name,generation,partition_key,channel_id)
                 DO UPDATE SET channel_code=EXCLUDED.channel_code,
                   channel_status=EXCLUDED.channel_status,
                   source_aggregate_version=EXCLUDED.source_aggregate_version,
                   source_global_sequence=EXCLUDED.source_global_sequence,
                   projected_at=clock_timestamp()
                 WHERE erpx_ecommerce_read.channel_projection.source_aggregate_version
                       < EXCLUDED.source_aggregate_version",
            )
            .bind(CHANNEL_PROJECTION)
            .bind(ctx.generation)
            .bind(&event.partition)
            .bind(channel.id().as_uuid())
            .bind(scope_key(channel.scope())?)
            .bind(serde_json::to_value(channel.scope()).map_err(failure)?)
            .bind(channel.code())
            .bind(enum_name(channel.status())?)
            .bind(i64v(event.aggregate_version)?)
            .bind(i64v(event.global_sequence)?)
            .execute(&mut **tx)
            .await
            .map_err(failure)?,
            "commerce channel snapshot version did not advance",
        )
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ctx: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(tx, ctx, "erpx_ecommerce_read.channel_projection").await
    }
}

#[async_trait]
impl Projector for CommerceListingProjector {
    fn name(&self) -> &str {
        LISTING_PROJECTION
    }
    fn event_types(&self) -> &[String] {
        &self.types
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ctx: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        validate(ctx, event, LISTING_PROJECTION, LISTING_SNAPSHOT_EVENT)?;
        let snapshot: CommerceSnapshot =
            serde_json::from_value(event.payload.clone()).map_err(failure)?;
        let CommerceAggregate::Listing(listing) = snapshot.aggregate else {
            return Err(ProjectionFailure::new(
                "commerce listing snapshot type mismatch",
            ));
        };
        metadata(&CommerceAggregate::Listing(listing.clone()), event)?;
        changed(sqlx::query("INSERT INTO erpx_ecommerce_read.listing_projection (projection_name,generation,partition_key,listing_id,scope_key,tenant_scope,channel_id,listing_status,price_minor,currency,available,source_aggregate_version,source_global_sequence,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,clock_timestamp()) ON CONFLICT (tenant_id,projection_name,generation,partition_key,listing_id) DO UPDATE SET listing_status=EXCLUDED.listing_status,price_minor=EXCLUDED.price_minor,currency=EXCLUDED.currency,available=EXCLUDED.available,source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,projected_at=clock_timestamp() WHERE erpx_ecommerce_read.listing_projection.source_aggregate_version<EXCLUDED.source_aggregate_version").bind(LISTING_PROJECTION).bind(ctx.generation).bind(&event.partition).bind(listing.id().as_uuid()).bind(scope_key(listing.scope())?).bind(serde_json::to_value(listing.scope()).map_err(failure)?).bind(listing.channel_id().as_uuid()).bind(enum_name(listing.status())?).bind(listing.price().minor_units()).bind(listing.price().currency().as_str()).bind(listing.available()).bind(i64v(event.aggregate_version)?).bind(i64v(event.global_sequence)?).execute(&mut **tx).await.map_err(failure)?,"commerce listing snapshot version did not advance")
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ctx: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(tx, ctx, "erpx_ecommerce_read.listing_projection").await
    }
}

#[async_trait]
impl Projector for CommerceJourneyProjector {
    fn name(&self) -> &str {
        JOURNEY_PROJECTION
    }
    fn event_types(&self) -> &[String] {
        &self.types
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ctx: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        validate(ctx, event, JOURNEY_PROJECTION, JOURNEY_SNAPSHOT_EVENT)?;
        let snapshot: CommerceSnapshot =
            serde_json::from_value(event.payload.clone()).map_err(failure)?;
        metadata(&snapshot.aggregate, event)?;
        let (owner, status, order_ref) = match &snapshot.aggregate {
            CommerceAggregate::Cart(v) => (v.owner(), enum_name(v.status())?, None),
            CommerceAggregate::Checkout(v) => (
                v.owner(),
                enum_name(v.status())?,
                v.order_ref()
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(failure)?,
            ),
            CommerceAggregate::Return(v) => {
                let state = serde_json::to_value(v).map_err(failure)?;
                (
                    v.owner(),
                    enum_name(v.status())?,
                    state.get("order_ref").cloned(),
                )
            }
            CommerceAggregate::Fulfillment(v) => (
                v.owner(),
                enum_name(v.status())?,
                Some(serde_json::to_value(v.order()).map_err(failure)?),
            ),
            _ => {
                return Err(ProjectionFailure::new(
                    "commerce journey snapshot type mismatch",
                ));
            }
        };
        let aggregate = &snapshot.aggregate;
        changed(sqlx::query("INSERT INTO erpx_ecommerce_read.journey_projection (projection_name,generation,partition_key,record_id,record_type,scope_key,tenant_scope,owner_realm,owner_principal_id,lifecycle_status,order_ref,source_aggregate_version,source_global_sequence,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,clock_timestamp()) ON CONFLICT (tenant_id,projection_name,generation,partition_key,record_id) DO UPDATE SET lifecycle_status=EXCLUDED.lifecycle_status,order_ref=EXCLUDED.order_ref,source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,projected_at=clock_timestamp() WHERE erpx_ecommerce_read.journey_projection.source_aggregate_version<EXCLUDED.source_aggregate_version").bind(JOURNEY_PROJECTION).bind(ctx.generation).bind(&event.partition).bind(aggregate.id_uuid()).bind(variant(aggregate.aggregate_type())?).bind(scope_key(aggregate.scope())?).bind(serde_json::to_value(aggregate.scope()).map_err(failure)?).bind(enum_name(owner.realm)?).bind(owner.principal_id).bind(status).bind(order_ref).bind(i64v(event.aggregate_version)?).bind(i64v(event.global_sequence)?).execute(&mut **tx).await.map_err(failure)?,"commerce journey snapshot version did not advance")
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ctx: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(tx, ctx, "erpx_ecommerce_read.journey_projection").await
    }
}

fn validate(
    ctx: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    name: &str,
    event_type: &str,
) -> Result<(), ProjectionFailure> {
    if ctx.projection != name
        || event.context != "ecommerce"
        || event.event_type != event_type
        || event.event_version != 1
    {
        return Err(ProjectionFailure::new(
            "commerce projector received an incompatible event",
        ));
    }
    Ok(())
}
fn metadata(a: &CommerceAggregate, e: &ProjectionEvent) -> Result<(), ProjectionFailure> {
    if a.id_uuid() != e.aggregate_id.as_uuid()
        || a.version() != e.aggregate_version
        || a.scope() != &e.tenant_scope
        || crate::aggregate_partition(a.aggregate_type(), a.id_uuid()) != e.partition
    {
        return Err(ProjectionFailure::new(
            "commerce snapshot metadata mismatch",
        ));
    }
    Ok(())
}
async fn reset(
    tx: &mut Transaction<'_, Postgres>,
    ctx: &ProjectorContext<'_>,
    table: &str,
) -> Result<(), ProjectionFailure> {
    let q = format!("DELETE FROM {table} WHERE projection_name=$1 AND generation=$2");
    sqlx::query(&q)
        .bind(ctx.projection)
        .bind(ctx.generation)
        .execute(&mut **tx)
        .await
        .map_err(failure)?;
    Ok(())
}
fn changed(r: sqlx::postgres::PgQueryResult, m: &str) -> Result<(), ProjectionFailure> {
    if r.rows_affected() != 1 {
        return Err(ProjectionFailure::new(m));
    }
    Ok(())
}
fn scope_key(s: &erpx_kernel::TenantScope) -> Result<String, ProjectionFailure> {
    match s {
        erpx_kernel::TenantScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}")),
        erpx_kernel::TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}/b:{branch_id}")),
        _ => Err(ProjectionFailure::new(
            "commerce projection scope must be exact",
        )),
    }
}
fn variant(k: &str) -> Result<&'static str, ProjectionFailure> {
    match k {
        "commerce_cart" => Ok("cart"),
        "commerce_checkout" => Ok("checkout"),
        "commerce_return" => Ok("return"),
        "commerce_fulfillment_request" => Ok("fulfillment_request"),
        _ => Err(ProjectionFailure::new(
            "unsupported commerce journey projection",
        )),
    }
}
fn enum_name(v: impl serde::Serialize) -> Result<String, ProjectionFailure> {
    serde_json::to_value(v)
        .map_err(failure)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ProjectionFailure::new("commerce enum must encode as text"))
}
fn i64v(v: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(v).map_err(|_| ProjectionFailure::new("commerce numeric value is too large"))
}
fn failure(e: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projectors_are_generation_managed() {
        assert_eq!(CommerceChannelProjector::new().name(), CHANNEL_PROJECTION);
        assert_eq!(CommerceListingProjector::new().name(), LISTING_PROJECTION);
        assert_eq!(CommerceJourneyProjector::new().name(), JOURNEY_PROJECTION);
        assert!(include_str!("runner_projection.rs").contains("ctx.generation"));
    }
}
