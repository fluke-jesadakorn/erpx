use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft,
    RecordTypeId, StableRecordRef, catalog::ContextId, record_policy_id,
};
use erpx_kernel::{CommandId, CorrelationId, DataClassification, RecordId, TenantId, TenantScope};
use erpx_planning::{PlanningAggregate, PortError};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(super) fn planning_directory_draft(
    aggregate: &PlanningAggregate,
    scope: &TenantScope,
) -> Result<Option<ItemDescriptorDraft>, PortError> {
    let (record_type, id, state, label) = match aggregate {
        PlanningAggregate::Scenario(value) => (
            "planning.scenario",
            value.id().as_uuid(),
            token(value.status())?,
            "Planning scenario",
        ),
        PlanningAggregate::Recommendation(value) => (
            "planning.recommendation",
            value.id.as_uuid(),
            token(value.status)?,
            "Planning recommendation",
        ),
        // Runs are authoritative computation evidence, but the accepted route
        // catalog has no `planning.run` first-class record family.
        PlanningAggregate::Run(_) => return Ok(None),
        PlanningAggregate::Plan(value) => (
            value.kind().record_type(),
            value.id(),
            token(value.status())?,
            "Planning plan",
        ),
    };
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(id).map_err(|_| PortError::InvalidState)?,
        ContextId::Planning,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::InvalidState)?,
    )
    .map_err(|_| PortError::InvalidState)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::InvalidState)?,
        record,
        scope: scope.clone(),
        lifecycle_state: state,
        classification: DataClassification::Confidential,
        descriptor_version: aggregate.version(),
        authorized_search_text: format!("{label} {id}"),
    };
    value.validate().map_err(|_| PortError::InvalidState)?;
    Ok(Some(value))
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
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.outbox_events
         (tenant_id,event_id,context_name,partition_key,command_id,event_type,event_version,
          aggregate_type,aggregate_id,aggregate_version,tenant_scope,actor,correlation_id,
          causation_id,payload,occurred_at,available_at)
         VALUES ($1,$2,'planning',$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,NULL,$13,
                 clock_timestamp(),clock_timestamp())",
    )
    .bind(tenant_id.as_uuid())
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
    fn run_is_not_falsely_mapped_to_a_plan_route() {
        let source = include_str!("directory.rs");
        assert!(source.contains("PlanningAggregate::Run(_) => return Ok(None)"));
        assert!(!source.contains("planning.run\""));
    }
}
