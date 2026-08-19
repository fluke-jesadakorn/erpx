use erpx_field_service::{FieldServiceAggregate, PortError};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft,
    RecordTypeId, StableRecordRef, catalog::ContextId, record_policy_id,
};
use erpx_kernel::{CommandId, CorrelationId, DataClassification, HoldingId, RecordId};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(super) fn field_service_directory_draft(
    aggregate: &FieldServiceAggregate,
) -> Result<Option<ItemDescriptorDraft>, PortError> {
    let (record_type, id, state, label) = match aggregate {
        FieldServiceAggregate::Agreement(value) => (
            "field-service.agreement",
            value.id().as_uuid(),
            "active".to_owned(),
            "Field service agreement",
        ),
        FieldServiceAggregate::Record(value) => (
            value.kind().record_type(),
            value.id(),
            serde_json::to_value(value.status())
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .ok_or(PortError::InvalidState)?,
            "Field service governed record",
        ),
        FieldServiceAggregate::Order(_) => return Ok(None),
    };
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(id).map_err(|_| PortError::InvalidState)?,
        ContextId::FieldService,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::InvalidState)?,
    )
    .map_err(|_| PortError::InvalidState)?;
    let draft = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::InvalidState)?,
        record,
        scope: aggregate.scope().clone(),
        lifecycle_state: state,
        classification: DataClassification::Confidential,
        descriptor_version: aggregate.version(),
        authorized_search_text: format!("{label} {id}"),
    };
    draft.validate().map_err(|_| PortError::InvalidState)?;
    Ok(Some(draft))
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
    sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES ($1,$2,'field_service',$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,NULL,$13,clock_timestamp(),clock_timestamp())")
        .bind(holding_id.as_uuid()).bind(event_id).bind(format!("item-directory:{}",record_id.simple()))
        .bind(command_id.as_uuid()).bind(ITEM_DIRECTORY_OWNER_EVENT_TYPE)
        .bind(i32::try_from(ITEM_DIRECTORY_FACT_VERSION).map_err(|_|PortError::InvalidState)?)
        .bind(draft.record.record_type().as_str()).bind(record_id)
        .bind(i64::try_from(draft.descriptor_version).map_err(|_|PortError::InvalidState)?)
        .bind(serde_json::to_value(&draft.scope).map_err(|_|PortError::InvalidState)?)
        .bind(actor).bind(correlation_id.as_uuid())
        .bind(serde_json::to_value(draft).map_err(|_|PortError::InvalidState)?)
        .execute(&mut **tx).await.map_err(|_|PortError::Internal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn composite_order_does_not_fabricate_request_dispatch_or_visit_records() {
        let source = include_str!("directory.rs");
        assert!(source.contains("FieldServiceAggregate::Agreement"));
        for fabricated in [
            concat!("field-service.", "request"),
            concat!("field-service.", "dispatch"),
            concat!("field-service.", "visit"),
        ] {
            assert!(!source.contains(fabricated));
        }
    }
}
