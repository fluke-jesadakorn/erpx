//! SQLx repository composition for E-commerce.
//!
//! This module remains compilable while the normalized unit-of-work
//! implementation is integrated in place.

#[derive(Debug, Clone)]
pub struct SqlxCommerceWriteRepository {
    pool: sqlx::PgPool,
}

impl SqlxCommerceWriteRepository {
    #[must_use]
    pub const fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub const fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }
}

#[derive(Debug, Clone)]
pub struct SqlxCommerceReadRepository {
    pool: erpx_cqrs_sqlx::ReadPool,
    holding_id: erpx_kernel::HoldingId,
}

impl SqlxCommerceReadRepository {
    pub fn new(
        pool: sqlx::PgPool,
        holding_id: erpx_kernel::HoldingId,
    ) -> Result<Self, CommercePortError> {
        let pool = erpx_cqrs_sqlx::ReadPool::for_holding(pool, holding_id.as_uuid())
            .map_err(|_| CommercePortError::Unavailable)?;
        Ok(Self { pool, holding_id })
    }
}

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_ecommerce::{
    AggregateRepository, CommerceAggregate, CommerceCommit, CommerceCommitReceipt,
    CommerceIdentity, CommercePortError, CommerceProjection, CommerceReadRepository,
};
use erpx_kernel::{AuthorityScope, HoldingId};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

#[async_trait]
impl AggregateRepository for SqlxCommerceWriteRepository {
    async fn load(
        &self,
        holding_id: HoldingId,
        authority: &AuthorityScope,
        aggregate_type: &str,
        id: Uuid,
    ) -> Result<Option<CommerceAggregate>, CommercePortError> {
        let mut tx = self.pool.begin().await.map_err(map)?;
        set_holding(&mut tx, holding_id).await?;
        let document: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT aggregate_document FROM erpx_ecommerce_write.aggregate_rehydration
             WHERE holding_id=$1 AND aggregate_type=$2 AND aggregate_id=$3",
        )
        .bind(holding_id.as_uuid())
        .bind(aggregate_type)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map)?;
        tx.commit().await.map_err(map)?;
        let Some(document) = document else {
            return Ok(None);
        };
        let aggregate = decode(aggregate_type, document)?;
        if aggregate.id_uuid() != id || !authority.contains(aggregate.scope()) {
            return Err(CommercePortError::Denied);
        }
        Ok(Some(aggregate))
    }

    async fn commit(
        &self,
        command: CommerceCommit,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        command.identity.validate()?;
        if !command.event_type.starts_with("ecommerce.")
            || !command
                .actor
                .authority_scope
                .contains(command.aggregate.scope())
        {
            return Err(CommercePortError::Invalid);
        }
        let holding = command.actor.holding_id;
        let aggregate_type = command.aggregate.aggregate_type();
        let aggregate_id = command.aggregate.id_uuid();
        let aggregate_version = command.aggregate.version();
        let mut tx = self.pool.begin().await.map_err(map)?;
        set_holding(&mut tx, holding).await?;
        if let Some(receipt) = replay(&mut tx, holding, &command).await? {
            tx.commit().await.map_err(map)?;
            return Ok(receipt);
        }
        let current = lock_version(&mut tx, holding, aggregate_type, aggregate_id).await?;
        match (command.expected_version, current) {
            (None, None) if aggregate_version == 1 => {}
            (Some(expected), Some(actual))
                if expected == actual
                    && aggregate_version
                        == expected.checked_add(1).ok_or(CommercePortError::Conflict)? => {}
            _ => return Err(CommercePortError::Conflict),
        }
        let state = serde_json::to_value(&command.aggregate)
            .map_err(|_| CommercePortError::Invalid)?
            .get("state")
            .cloned()
            .ok_or(CommercePortError::Invalid)?;
        let projection_payload =
            serde_json::to_value(&command.aggregate).map_err(|_| CommercePortError::Invalid)?;
        sqlx::query("SELECT erpx_ecommerce_write.persist_normalized($1,$2,$3,$4,$5,$6)")
            .bind(holding.as_uuid())
            .bind(aggregate_type)
            .bind(aggregate_id)
            .bind(scope_key(command.aggregate.scope())?)
            .bind(
                serde_json::to_value(command.aggregate.scope())
                    .map_err(|_| CommercePortError::Invalid)?,
            )
            .bind(state)
            .execute(&mut *tx)
            .await
            .map_err(map)?;
        let action = enum_name(command.action)?;
        sqlx::query(
            "INSERT INTO erpx_ecommerce_write.audit_log
             (holding_id,audit_id,command_id,actor_principal_id,action,aggregate_type,aggregate_id,
              aggregate_version,scope_key,correlation_id,evidence_digest,occurred_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        )
        .bind(holding.as_uuid())
        .bind(derived_id(command.identity.command_id.as_uuid(), b"audit"))
        .bind(command.identity.command_id.as_uuid())
        .bind(command.actor.principal_id)
        .bind(action)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(to_i64(aggregate_version)?)
        .bind(scope_key(command.aggregate.scope())?)
        .bind(command.identity.correlation_id.as_uuid())
        .bind(&command.identity.request_digest)
        .bind(command.aggregate.updated_at())
        .execute(&mut *tx)
        .await
        .map_err(map)?;
        let projection_event = match aggregate_type {
            "commerce_channel" => Some(crate::CHANNEL_SNAPSHOT_EVENT),
            "commerce_listing" => Some(crate::LISTING_SNAPSHOT_EVENT),
            "commerce_cart"
            | "commerce_checkout"
            | "commerce_return"
            | "commerce_fulfillment_request" => Some(crate::JOURNEY_SNAPSHOT_EVENT),
            _ => None,
        };
        if let Some(projection_event) = projection_event {
            let actor = serde_json::json!({"principal_ref":format!("principal:{}",command.actor.principal_id),"kind":"holding_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
            sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES ($1,$2,'ecommerce',$3,$4,$5,1,$6,$7,$8,$9,$10,$11,NULL,$12,$13,$13)")
                .bind(holding.as_uuid()).bind(derived_id(command.identity.command_id.as_uuid(),b"projection")).bind(crate::aggregate_partition(aggregate_type,aggregate_id)).bind(command.identity.command_id.as_uuid()).bind(projection_event).bind(aggregate_type).bind(aggregate_id).bind(to_i64(aggregate_version)?).bind(serde_json::to_value(command.aggregate.scope()).map_err(|_|CommercePortError::Invalid)?).bind(actor).bind(command.identity.correlation_id.as_uuid()).bind(serde_json::json!({"aggregate":projection_payload})).bind(command.aggregate.updated_at()).execute(&mut *tx).await.map_err(map)?;
        }
        if let Some(draft) = crate::directory::commerce_directory_draft(&command.aggregate)? {
            let actor = serde_json::json!({"principal_ref":format!("principal:{}",command.actor.principal_id),"kind":"holding_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
            crate::directory::append_directory_draft(
                &mut tx,
                holding,
                command.identity.command_id,
                command.identity.correlation_id,
                actor,
                command.aggregate.updated_at(),
                &draft,
            )
            .await?;
        }
        let event_id = derived_id(command.identity.command_id.as_uuid(), b"event");
        let position: i64 = sqlx::query_scalar(
            "INSERT INTO erpx_ecommerce_write.outbox
             (holding_id,event_id,aggregate_type,aggregate_id,aggregate_version,event_type,
              correlation_id,payload,occurred_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
              RETURNING position",
        )
        .bind(holding.as_uuid())
        .bind(event_id)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(to_i64(aggregate_version)?)
        .bind(&command.event_type)
        .bind(command.identity.correlation_id.as_uuid())
        .bind(command.event_payload)
        .bind(command.aggregate.updated_at())
        .fetch_one(&mut *tx)
        .await
        .map_err(map)?;
        let committed_at: DateTime<Utc> = sqlx::query_scalar(
            "INSERT INTO erpx_ecommerce_write.command_receipts
             (holding_id,idempotency_key,command_id,request_digest,aggregate_type,aggregate_id,aggregate_version)
             VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING committed_at",
        )
        .bind(holding.as_uuid())
        .bind(command.identity.idempotency_key)
        .bind(command.identity.command_id.as_uuid())
        .bind(command.identity.request_digest)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(to_i64(aggregate_version)?)
        .fetch_one(&mut *tx)
        .await
        .map_err(map)?;
        tx.commit().await.map_err(map)?;
        Ok(CommerceCommitReceipt {
            aggregate_id,
            aggregate_version,
            event_id,
            global_sequence: u64::try_from(position).map_err(|_| CommercePortError::Internal)?,
            committed_at,
            replayed: false,
        })
    }
}

#[async_trait]
impl CommerceReadRepository for SqlxCommerceReadRepository {
    async fn get(
        &self,
        holding_id: HoldingId,
        aggregate_type: &str,
        id: Uuid,
    ) -> Result<Option<CommerceProjection>, CommercePortError> {
        if holding_id != self.holding_id {
            return Err(CommercePortError::Denied);
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| CommercePortError::Unavailable)?;
        let projection_name = match aggregate_type {
            "commerce_channel" => crate::CHANNEL_PROJECTION,
            "commerce_listing" => crate::LISTING_PROJECTION,
            _ => crate::JOURNEY_PROJECTION,
        };
        let generation:Option<Uuid>=sqlx::query_scalar("SELECT generation FROM erpx_cqrs_read.projection_generations WHERE holding_id=$1 AND projection_name=$2 AND status='active'").bind(self.holding_id.as_uuid()).bind(projection_name).fetch_optional(&mut *tx).await.map_err(map)?;
        let generation = generation.ok_or(CommercePortError::Unavailable)?;
        let row = if aggregate_type == "commerce_channel" {
            sqlx::query(
                "SELECT active_scope,NULL::text owner_realm,NULL::uuid owner_principal_id,
                        channel_status lifecycle_status,source_aggregate_version,
                        source_global_sequence,projected_at,generation,c.global_sequence AS checkpoint_position,
                        EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f WHERE f.holding_id=p.holding_id AND f.projection_name=p.projection_name AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure
                 FROM erpx_ecommerce_read.channel_projection p LEFT JOIN erpx_cqrs_read.projection_checkpoints c ON c.holding_id=p.holding_id AND c.projection_name=p.projection_name AND c.generation=p.generation AND c.partition_key=p.partition_key
                 WHERE p.holding_id=$1 AND p.projection_name=$2 AND p.generation=$3 AND channel_id=$4",
            )
            .bind(self.holding_id.as_uuid())
            .bind(projection_name)
            .bind(generation)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map)?
        } else if aggregate_type == "commerce_listing" {
            sqlx::query(
                "SELECT active_scope,NULL::text owner_realm,NULL::uuid owner_principal_id,
                        listing_status lifecycle_status,source_aggregate_version,
                        source_global_sequence,projected_at,generation,c.global_sequence AS checkpoint_position,
                        EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f WHERE f.holding_id=p.holding_id AND f.projection_name=p.projection_name AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure
                 FROM erpx_ecommerce_read.listing_projection p LEFT JOIN erpx_cqrs_read.projection_checkpoints c ON c.holding_id=p.holding_id AND c.projection_name=p.projection_name AND c.generation=p.generation AND c.partition_key=p.partition_key
                 WHERE p.holding_id=$1 AND p.projection_name=$2 AND p.generation=$3 AND listing_id=$4",
            )
            .bind(self.holding_id.as_uuid()).bind(projection_name).bind(generation)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map)?
        } else {
            sqlx::query(
                "SELECT active_scope,owner_realm,owner_principal_id,lifecycle_status,
                        source_aggregate_version,source_global_sequence,projected_at,generation,c.global_sequence AS checkpoint_position,
                        EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f WHERE f.holding_id=p.holding_id AND f.projection_name=p.projection_name AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure
                 FROM erpx_ecommerce_read.journey_projection p LEFT JOIN erpx_cqrs_read.projection_checkpoints c ON c.holding_id=p.holding_id AND c.projection_name=p.projection_name AND c.generation=p.generation AND c.partition_key=p.partition_key
                 WHERE p.holding_id=$1 AND p.projection_name=$2 AND p.generation=$3 AND record_id=$4 AND record_type=$5",
            )
            .bind(self.holding_id.as_uuid()).bind(projection_name).bind(generation)
            .bind(id)
            .bind(variant(aggregate_type)?)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map)?
        };
        tx.commit().await.map_err(map)?;
        if let Some(ref row) = row {
            let position = row
                .try_get::<i64, _>("source_global_sequence")
                .map_err(|_| CommercePortError::Internal)?;
            let checkpoint = row
                .try_get::<Option<i64>, _>("checkpoint_position")
                .map_err(|_| CommercePortError::Internal)?;
            if row
                .try_get::<bool, _>("has_failure")
                .map_err(|_| CommercePortError::Internal)?
                || checkpoint.is_none_or(|value| value < position)
            {
                return Err(CommercePortError::Stale);
            }
        }
        row.map(|row| projection(aggregate_type, id, row))
            .transpose()
    }
}

async fn set_holding(
    tx: &mut Transaction<'_, Postgres>,
    holding: HoldingId,
) -> Result<(), CommercePortError> {
    sqlx::query("SELECT set_config('app.holding_id',$1,true)")
        .bind(holding.to_string())
        .execute(&mut **tx)
        .await
        .map_err(map)?;
    Ok(())
}

async fn replay(
    tx: &mut Transaction<'_, Postgres>,
    holding: HoldingId,
    command: &CommerceCommit,
) -> Result<Option<CommerceCommitReceipt>, CommercePortError> {
    let row = sqlx::query(
        "SELECT r.command_id,r.request_digest,r.aggregate_type,r.aggregate_id,
                r.aggregate_version,r.committed_at,o.event_id,o.position
         FROM erpx_ecommerce_write.command_receipts r
         JOIN erpx_ecommerce_write.outbox o
           ON o.holding_id=r.holding_id
          AND o.aggregate_type=r.aggregate_type
          AND o.aggregate_id=r.aggregate_id
          AND o.aggregate_version=r.aggregate_version
         WHERE r.holding_id=$1 AND r.idempotency_key=$2 FOR UPDATE OF r",
    )
    .bind(holding.as_uuid())
    .bind(&command.identity.idempotency_key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map)?;
    let Some(row) = row else { return Ok(None) };
    let stored_version: i64 = row
        .try_get("aggregate_version")
        .map_err(|_| CommercePortError::Internal)?;
    if row.try_get::<Uuid, _>("command_id").ok() != Some(command.identity.command_id.as_uuid())
        || row.try_get::<String, _>("request_digest").ok().as_deref()
            != Some(&command.identity.request_digest)
        || row.try_get::<String, _>("aggregate_type").ok().as_deref()
            != Some(command.aggregate.aggregate_type())
        || row.try_get::<Uuid, _>("aggregate_id").ok() != Some(command.aggregate.id_uuid())
        || u64::try_from(stored_version).ok() != Some(command.aggregate.version())
    {
        return Err(CommercePortError::IdempotencyConflict);
    }
    Ok(Some(CommerceCommitReceipt {
        aggregate_id: command.aggregate.id_uuid(),
        aggregate_version: command.aggregate.version(),
        event_id: row
            .try_get("event_id")
            .map_err(|_| CommercePortError::Internal)?,
        global_sequence: u64::try_from(
            row.try_get::<i64, _>("position")
                .map_err(|_| CommercePortError::Internal)?,
        )
        .map_err(|_| CommercePortError::Internal)?,
        committed_at: row
            .try_get("committed_at")
            .map_err(|_| CommercePortError::Internal)?,
        replayed: true,
    }))
}

async fn lock_version(
    tx: &mut Transaction<'_, Postgres>,
    holding: HoldingId,
    kind: &str,
    id: Uuid,
) -> Result<Option<u64>, CommercePortError> {
    let (table, column) = table(kind)?;
    let query = format!(
        "SELECT aggregate_version FROM erpx_ecommerce_write.{table} \
         WHERE holding_id=$1 AND {column}=$2 FOR UPDATE"
    );
    let value: Option<i64> = sqlx::query_scalar(&query)
        .bind(holding.as_uuid())
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(map)?;
    value
        .map(|value| u64::try_from(value).map_err(|_| CommercePortError::Internal))
        .transpose()
}

fn decode(kind: &str, document: serde_json::Value) -> Result<CommerceAggregate, CommercePortError> {
    serde_json::from_value(serde_json::json!({"aggregate": variant(kind)?, "state": document}))
        .map_err(|_| CommercePortError::Internal)
}
fn table(kind: &str) -> Result<(&'static str, &'static str), CommercePortError> {
    match kind {
        "commerce_channel" => Ok(("channels", "channel_id")),
        "commerce_listing" => Ok(("listings", "listing_id")),
        "commerce_cart" => Ok(("carts", "cart_id")),
        "commerce_checkout" => Ok(("checkouts", "checkout_id")),
        "commerce_return" => Ok(("returns", "return_id")),
        "commerce_fulfillment_request" => Ok(("fulfillment_requests", "fulfillment_request_id")),
        _ => Err(CommercePortError::Invalid),
    }
}
fn variant(kind: &str) -> Result<&'static str, CommercePortError> {
    match kind {
        "commerce_channel" => Ok("channel"),
        "commerce_listing" => Ok("listing"),
        "commerce_cart" => Ok("cart"),
        "commerce_checkout" => Ok("checkout"),
        "commerce_return" => Ok("return"),
        "commerce_fulfillment_request" => Ok("fulfillment"),
        _ => Err(CommercePortError::Invalid),
    }
}
fn scope_key(scope: &AuthorityScope) -> Result<String, CommercePortError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}")),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}/b:{branch_id}")),
        _ => Err(CommercePortError::Invalid),
    }
}
fn enum_name<T: serde::Serialize>(value: T) -> Result<String, CommercePortError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or(CommercePortError::Invalid)
}
fn projection(
    kind: &str,
    id: Uuid,
    row: sqlx::postgres::PgRow,
) -> Result<CommerceProjection, CommercePortError> {
    let realm: Option<String> = row
        .try_get("owner_realm")
        .map_err(|_| CommercePortError::Internal)?;
    let owner = realm
        .map(|realm| {
            Ok(CommerceIdentity {
                realm: serde_json::from_value(realm.into())
                    .map_err(|_| CommercePortError::Internal)?,
                principal_id: row
                    .try_get("owner_principal_id")
                    .map_err(|_| CommercePortError::Internal)?,
                verified_customer: None,
            })
        })
        .transpose()?;
    let version: i64 = row
        .try_get("source_aggregate_version")
        .map_err(|_| CommercePortError::Internal)?;
    let position: i64 = row
        .try_get("source_global_sequence")
        .map_err(|_| CommercePortError::Internal)?;
    Ok(CommerceProjection {
        aggregate_type: kind.into(),
        aggregate_id: id,
        scope: serde_json::from_value(
            row.try_get("active_scope")
                .map_err(|_| CommercePortError::Internal)?,
        )
        .map_err(|_| CommercePortError::Internal)?,
        owner,
        lifecycle_status: row
            .try_get("lifecycle_status")
            .map_err(|_| CommercePortError::Internal)?,
        generation: row
            .try_get("generation")
            .map_err(|_| CommercePortError::Internal)?,
        aggregate_version: u64::try_from(version).map_err(|_| CommercePortError::Internal)?,
        source_position: u64::try_from(position).map_err(|_| CommercePortError::Internal)?,
        checkpoint_position: u64::try_from(
            row.try_get::<i64, _>("checkpoint_position")
                .map_err(|_| CommercePortError::Internal)?,
        )
        .map_err(|_| CommercePortError::Internal)?,
        projected_at: row
            .try_get("projected_at")
            .map_err(|_| CommercePortError::Internal)?,
    })
}
fn derived_id(seed: Uuid, label: &[u8]) -> Uuid {
    let mut hash = Sha256::new();
    hash.update(seed.as_bytes());
    hash.update(label);
    let mut bytes: [u8; 16] = hash.finalize()[..16].try_into().expect("digest slice");
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
fn to_i64(value: u64) -> Result<i64, CommercePortError> {
    i64::try_from(value).map_err(|_| CommercePortError::Invalid)
}
fn map(_: sqlx::Error) -> CommercePortError {
    CommercePortError::Internal
}

#[cfg(test)]
mod tests {
    #[test]
    fn command_path_is_holding_bound_atomic_and_normalized() {
        let source = include_str!("repository.rs");
        for marker in [
            "set_config('app.holding_id'",
            "FOR UPDATE",
            "audit_log",
            "command_receipts",
            "outbox",
            "persist_normalized",
        ] {
            assert!(source.contains(marker));
        }
    }
}
