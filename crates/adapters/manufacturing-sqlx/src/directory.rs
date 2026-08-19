use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft,
    RecordTypeId, StableRecordRef, catalog::ContextId, record_policy_id,
};
use erpx_kernel::{CommandId, CorrelationId, DataClassification, HoldingId, RecordId};
use erpx_manufacturing::{ManufacturingAggregate, PortError};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(super) fn manufacturing_directory_drafts(
    aggregate: &ManufacturingAggregate,
) -> Result<Vec<ItemDescriptorDraft>, PortError> {
    let (record_type, id, state, label) = match aggregate {
        ManufacturingAggregate::Specification(value) => (
            "manufacturing.specification",
            value.reference().specification_id.as_uuid(),
            token(value.status())?,
            "Manufacturing specification",
        ),
        ManufacturingAggregate::Order(value) => (
            "manufacturing.order",
            value.id().as_uuid(),
            token(value.status())?,
            "Manufacturing order",
        ),
        ManufacturingAggregate::Record(value) => (
            value.kind().record_type(),
            value.id(),
            token(value.status())?,
            "Manufacturing governed record",
        ),
    };
    Ok(vec![draft(
        record_type,
        id,
        aggregate.version(),
        aggregate.scope(),
        state,
        label,
    )?])
}

fn draft(
    record_type: &str,
    id: Uuid,
    version: u64,
    scope: &erpx_kernel::AuthorityScope,
    lifecycle_state: String,
    label: &str,
) -> Result<ItemDescriptorDraft, PortError> {
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(id).map_err(|_| PortError::InvalidState)?,
        ContextId::Manufacturing,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::InvalidState)?,
    )
    .map_err(|_| PortError::InvalidState)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::InvalidState)?,
        record,
        scope: scope.clone(),
        lifecycle_state,
        classification: DataClassification::Confidential,
        descriptor_version: version,
        authorized_search_text: format!("{label} {id}"),
    };
    value.validate().map_err(|_| PortError::InvalidState)?;
    Ok(value)
}

fn token(value: impl serde::Serialize) -> Result<String, PortError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .ok_or(PortError::InvalidState)
}

pub(super) async fn append_directory_draft(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    command_id: CommandId,
    correlation_id: CorrelationId,
    actor: serde_json::Value,
    draft: &ItemDescriptorDraft,
) -> Result<(), PortError> {
    let record_id = draft.record.id().as_uuid();
    let label = format!(
        "{}:{}:{}",
        ITEM_DIRECTORY_OWNER_EVENT_TYPE,
        draft.record.record_type().as_str(),
        record_id
    );
    let event_id = Uuid::new_v5(&command_id.as_uuid(), label.as_bytes());
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.outbox_events
         (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,
          aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,
          causation_id,payload,occurred_at,available_at)
         VALUES ($1,$2,'manufacturing',$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,NULL,$13,
                 clock_timestamp(),clock_timestamp())",
    )
    .bind(holding_id.as_uuid())
    .bind(event_id)
    .bind(format!("item-directory:{}", record_id.simple()))
    .bind(command_id.as_uuid())
    .bind(ITEM_DIRECTORY_OWNER_EVENT_TYPE)
    .bind(i32::try_from(ITEM_DIRECTORY_FACT_VERSION).map_err(|_| PortError::InvalidState)?)
    .bind(draft.record.record_type().as_str())
    .bind(record_id)
    .bind(i64::try_from(draft.descriptor_version).map_err(|_| PortError::InvalidState)?)
    .bind(serde_json::to_value(&draft.scope).map_err(|_| PortError::InvalidState)?)
    .bind(actor)
    .bind(correlation_id.as_uuid())
    .bind(serde_json::to_value(draft).map_err(|_| PortError::InvalidState)?)
    .execute(&mut **tx)
    .await
    .map_err(|_| PortError::Internal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn publisher_is_atomic_and_never_uses_snapshot_inference() {
        let source = include_str!("lib.rs");
        let fact = source
            .find("append_directory_draft(")
            .expect("directory fact");
        let receipt = source[fact..]
            .find("command_receipts")
            .map(|offset| fact + offset)
            .expect("receipt after directory fact");
        let commit = source[receipt..]
            .find("tx.commit()")
            .map(|offset| receipt + offset)
            .expect("transaction commit");
        assert!(fact < receipt && receipt < commit);
        assert!(!source.contains(concat!("ItemDescriptorDraft::from_", "snapshot")));
    }
}
