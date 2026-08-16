use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft,
    RecordTypeId, StableRecordRef, catalog::ContextId, record_policy_id,
};
use erpx_kernel::{CommandId, CorrelationId, DataClassification, RecordId, TenantId};
use erpx_learning::{LearningAggregate, PortError};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(super) fn learning_directory_draft(
    aggregate: &LearningAggregate,
) -> Result<ItemDescriptorDraft, PortError> {
    let (record_type, id, state, label) = match aggregate {
        LearningAggregate::Course(value) => (
            "learning.course",
            value.id().as_uuid(),
            token(value.status())?,
            "Learning course",
        ),
        LearningAggregate::Enrollment(value) => (
            "learning.enrollment",
            value.id().as_uuid(),
            token(value.status())?,
            "Learning enrollment",
        ),
        LearningAggregate::Record(value) => (
            value.kind().record_type(),
            value.id(),
            token(value.status())?,
            "Learning governed record",
        ),
    };
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(id).map_err(|_| PortError::InvalidState)?,
        ContextId::Learning,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::InvalidState)?,
    )
    .map_err(|_| PortError::InvalidState)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::InvalidState)?,
        record,
        scope: aggregate.scope().clone(),
        lifecycle_state: state,
        classification: DataClassification::Confidential,
        descriptor_version: aggregate.version(),
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
    tenant_id: TenantId,
    command_id: CommandId,
    correlation_id: CorrelationId,
    actor: serde_json::Value,
    draft: &ItemDescriptorDraft,
) -> Result<(Uuid, u64), PortError> {
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
    let global_sequence = sqlx::query_scalar::<_, i64>("INSERT INTO erpx_cqrs_write.outbox_events (tenant_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,tenant_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES ($1,$2,'learning',$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,NULL,$13,clock_timestamp(),clock_timestamp()) RETURNING global_sequence")
        .bind(tenant_id.as_uuid()).bind(event_id).bind(format!("item-directory:{}",record_id.simple()))
        .bind(command_id.as_uuid()).bind(ITEM_DIRECTORY_OWNER_EVENT_TYPE)
        .bind(i32::try_from(ITEM_DIRECTORY_FACT_VERSION).map_err(|_|PortError::InvalidState)?)
        .bind(draft.record.record_type().as_str()).bind(record_id)
        .bind(i64::try_from(draft.descriptor_version).map_err(|_|PortError::InvalidState)?)
        .bind(serde_json::to_value(&draft.scope).map_err(|_|PortError::InvalidState)?)
        .bind(actor).bind(correlation_id.as_uuid())
        .bind(serde_json::to_value(draft).map_err(|_|PortError::InvalidState)?)
        .fetch_one(&mut **tx).await.map_err(|_|PortError::Internal)?;
    Ok((
        event_id,
        u64::try_from(global_sequence).map_err(|_| PortError::Internal)?,
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn learning_publish_is_before_receipt_and_commit() {
        let source = include_str!("lib.rs");
        let fact = source.find("append_directory_draft(").unwrap();
        let receipt = source[fact..].find("command_receipts").unwrap() + fact;
        let commit = source[receipt..].find("tx.commit()").unwrap() + receipt;
        assert!(fact < receipt && receipt < commit);
    }
}
