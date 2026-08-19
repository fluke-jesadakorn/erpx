use chrono::{DateTime, Utc};
use erpx_ecommerce::{CommerceAggregate, CommercePortError};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft,
    RecordTypeId, StableRecordRef, catalog::ContextId, record_policy_id,
};
use erpx_kernel::{CommandId, CorrelationId, DataClassification, HoldingId, RecordId};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(super) fn commerce_directory_draft(
    aggregate: &CommerceAggregate,
) -> Result<Option<ItemDescriptorDraft>, CommercePortError> {
    let (record_type, state, label, classification) = match aggregate {
        CommerceAggregate::Listing(value) => (
            "commerce.catalog-publication",
            token(value.status())?,
            "Catalog publication",
            DataClassification::Internal,
        ),
        CommerceAggregate::Cart(value) => (
            "commerce.cart",
            token(value.status())?,
            "Commerce cart",
            DataClassification::Confidential,
        ),
        CommerceAggregate::Checkout(value) => (
            "commerce.order",
            token(value.status())?,
            "Commerce order orchestration",
            DataClassification::Confidential,
        ),
        CommerceAggregate::Return(value) => (
            "commerce.return",
            token(value.status())?,
            "Commerce return",
            DataClassification::Confidential,
        ),
        CommerceAggregate::Fulfillment(value) => (
            "commerce.fulfillment-request",
            token(value.status())?,
            "Commerce fulfillment request",
            DataClassification::Confidential,
        ),
        // Channel is governance for catalog publication; it has no accepted
        // first-class route in the target record catalog.
        CommerceAggregate::Channel(_) => return Ok(None),
    };
    let id = aggregate.id_uuid();
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(id).map_err(|_| CommercePortError::Invalid)?,
        ContextId::Ecommerce,
        RecordTypeId::try_new(record_type).map_err(|_| CommercePortError::Invalid)?,
    )
    .map_err(|_| CommercePortError::Invalid)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| CommercePortError::Invalid)?,
        record,
        scope: aggregate.scope().clone(),
        lifecycle_state: state,
        classification,
        descriptor_version: aggregate.version(),
        authorized_search_text: format!("{label} {id}"),
    };
    value.validate().map_err(|_| CommercePortError::Invalid)?;
    Ok(Some(value))
}

fn token(value: impl serde::Serialize) -> Result<String, CommercePortError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .ok_or(CommercePortError::Invalid)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn append_directory_draft(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    command_id: CommandId,
    correlation_id: CorrelationId,
    actor: serde_json::Value,
    occurred_at: DateTime<Utc>,
    draft: &ItemDescriptorDraft,
) -> Result<(), CommercePortError> {
    let record_id = draft.record.id().as_uuid();
    let event_id = Uuid::new_v5(
        &command_id.as_uuid(),
        format!(
            "{}:{}:{}",
            ITEM_DIRECTORY_OWNER_EVENT_TYPE,
            draft.record.record_type().as_str(),
            record_id
        )
        .as_bytes(),
    );
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.outbox_events
         (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,
          aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,
          causation_id,payload,occurred_at,available_at)
         VALUES ($1,$2,'ecommerce',$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,NULL,$13,$14,$14)",
    )
    .bind(holding_id.as_uuid())
    .bind(event_id)
    .bind(format!("item-directory:{}", record_id.simple()))
    .bind(command_id.as_uuid())
    .bind(ITEM_DIRECTORY_OWNER_EVENT_TYPE)
    .bind(i32::try_from(ITEM_DIRECTORY_FACT_VERSION).map_err(|_| CommercePortError::Invalid)?)
    .bind(draft.record.record_type().as_str())
    .bind(record_id)
    .bind(i64::try_from(draft.descriptor_version).map_err(|_| CommercePortError::Invalid)?)
    .bind(serde_json::to_value(&draft.scope).map_err(|_| CommercePortError::Invalid)?)
    .bind(actor)
    .bind(correlation_id.as_uuid())
    .bind(serde_json::to_value(draft).map_err(|_| CommercePortError::Invalid)?)
    .bind(occurred_at)
    .execute(&mut **tx)
    .await
    .map_err(|_| CommercePortError::Internal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn commerce_directory_draft_never_contains_cart_or_checkout_payloads() {
        let source = include_str!("directory.rs");
        assert!(!source.contains(concat!("event", "_payload")));
        assert!(!source.contains(concat!("Commerce", "Identity")));
        assert!(source.contains("CommerceAggregate::Channel(_) => return Ok(None)"));
    }
}
