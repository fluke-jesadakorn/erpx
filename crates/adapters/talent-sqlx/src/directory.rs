use chrono::{DateTime, Utc};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft,
    RecordTypeId, StableRecordRef, catalog::ContextId, record_policy_id,
};
use erpx_kernel::{CommandId, CorrelationId, DataClassification, HoldingId, RecordId};
use erpx_talent::{TalentAggregate, TalentPortError};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(super) fn talent_directory_draft(
    aggregate: &TalentAggregate,
) -> Result<Option<ItemDescriptorDraft>, TalentPortError> {
    let (record_type, state, label, classification) = match aggregate {
        TalentAggregate::Requisition(value) => (
            "talent.requisition",
            token(value.status())?,
            "Talent requisition",
            DataClassification::Confidential,
        ),
        TalentAggregate::Candidate(value) => (
            "talent.candidate",
            token(value.status())?,
            "Candidate record",
            DataClassification::Restricted,
        ),
        TalentAggregate::Application(value) => (
            "talent.application",
            token(value.status())?,
            "Candidate application",
            DataClassification::Restricted,
        ),
        TalentAggregate::Offer(value) => (
            "talent.offer",
            token(value.status())?,
            "Candidate offer",
            DataClassification::Restricted,
        ),
        TalentAggregate::Proposal(value) => (
            "talent.proposal",
            token(value.status())?,
            "Talent proposal",
            DataClassification::Confidential,
        ),
        TalentAggregate::Record(value) => (
            value.kind().record_type(),
            token(value.status())?,
            "Governed talent record",
            DataClassification::Restricted,
        ),
        // Employment handoff evidence is not a separately routed Talent
        // record in the accepted catalog.
        TalentAggregate::HireHandoff(_) => return Ok(None),
    };
    let id = aggregate.id_uuid();
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(id).map_err(|_| TalentPortError::Invalid)?,
        ContextId::RecruitingTalent,
        RecordTypeId::try_new(record_type).map_err(|_| TalentPortError::Invalid)?,
    )
    .map_err(|_| TalentPortError::Invalid)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| TalentPortError::Invalid)?,
        record,
        scope: aggregate.scope().clone(),
        lifecycle_state: state,
        classification,
        descriptor_version: aggregate.version(),
        // Never index profile/attachment/scorecard/offer evidence or a human
        // display name. The authorized directory receives only an opaque label.
        authorized_search_text: format!("{label} {id}"),
    };
    value.validate().map_err(|_| TalentPortError::Invalid)?;
    Ok(Some(value))
}

fn token(value: impl serde::Serialize) -> Result<String, TalentPortError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .ok_or(TalentPortError::Invalid)
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
) -> Result<(), TalentPortError> {
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
         VALUES ($1,$2,'recruiting_talent',$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,NULL,$13,$14,$14)",
    )
    .bind(holding_id.as_uuid())
    .bind(event_id)
    .bind(format!("item-directory:{}", record_id.simple()))
    .bind(command_id.as_uuid())
    .bind(ITEM_DIRECTORY_OWNER_EVENT_TYPE)
    .bind(i32::try_from(ITEM_DIRECTORY_FACT_VERSION).map_err(|_| TalentPortError::Invalid)?)
    .bind(draft.record.record_type().as_str())
    .bind(record_id)
    .bind(i64::try_from(draft.descriptor_version).map_err(|_| TalentPortError::Invalid)?)
    .bind(serde_json::to_value(&draft.scope).map_err(|_| TalentPortError::Invalid)?)
    .bind(actor)
    .bind(correlation_id.as_uuid())
    .bind(serde_json::to_value(draft).map_err(|_| TalentPortError::Invalid)?)
    .bind(occurred_at)
    .execute(&mut **tx)
    .await
    .map_err(|_| TalentPortError::Internal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn candidate_directory_mapping_is_opaque_and_realm_safe() {
        let source = include_str!("directory.rs");
        for private in [
            concat!("profile", "_digest"),
            concat!("attachment", "_digests"),
            concat!("scorecard", "_digest"),
        ] {
            assert!(!source.contains(private));
        }
        assert!(source.contains("DataClassification::Restricted"));
        assert!(source.contains("'recruiting_talent'"));
    }
}
