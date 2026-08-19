use chrono::{DateTime, Utc};
use erpx_collaboration::{PortError, Thread};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft,
    RecordTypeId, StableRecordRef, catalog::ContextId, record_policy_id,
};
use erpx_kernel::{CommandId, CorrelationId, HoldingId, RecordId};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(super) fn collaboration_directory_drafts(
    thread: &Thread,
) -> Result<Vec<ItemDescriptorDraft>, PortError> {
    let policy = thread.policy();
    let scope = policy.visibility_scope();
    let mut values = vec![draft(
        "collaboration.thread",
        thread.id().as_uuid(),
        thread.version(),
        token(thread.state())?,
        format!("Discussion {}", thread.id().as_uuid()),
        thread,
    )?];
    for promotion in thread.promotions() {
        let state = if promotion.promoted_record.is_some() {
            "promoted"
        } else {
            "proposed"
        };
        values.push(draft(
            "collaboration.promotion",
            promotion.id.as_uuid(),
            thread.version(),
            state.into(),
            format!(
                "{} promotion {}",
                token(promotion.kind)?,
                promotion.id.as_uuid()
            ),
            thread,
        )?);
    }
    for moderation in thread.moderation_cases() {
        values.push(draft(
            "collaboration.moderation-case",
            moderation.id.as_uuid(),
            thread.version(),
            token(moderation.state)?,
            format!("Moderation case {}", moderation.id.as_uuid()),
            thread,
        )?);
    }
    if values.iter().any(|value| &value.scope != scope) {
        return Err(PortError::InvalidState);
    }
    Ok(values)
}

fn draft(
    record_type: &str,
    id: Uuid,
    version: u64,
    lifecycle_state: String,
    authorized_search_text: String,
    thread: &Thread,
) -> Result<ItemDescriptorDraft, PortError> {
    let policy = thread.policy();
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(id).map_err(|_| PortError::InvalidState)?,
        ContextId::Collaboration,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::InvalidState)?,
    )
    .map_err(|_| PortError::InvalidState)?;
    let authorization_policy = record_policy_id(&record).map_err(|_| PortError::InvalidState)?;
    let value = ItemDescriptorDraft {
        record,
        scope: policy.visibility_scope().clone(),
        lifecycle_state,
        authorization_policy,
        classification: policy.classification(),
        descriptor_version: version,
        authorized_search_text,
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

#[allow(clippy::too_many_arguments)]
pub(super) async fn append_directory_draft(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    command_id: CommandId,
    correlation_id: CorrelationId,
    actor: serde_json::Value,
    occurred_at: DateTime<Utc>,
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
    sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES ($1,$2,'collaboration',$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,NULL,$13,$14,$14)")
        .bind(holding_id.as_uuid()).bind(event_id).bind(format!("item-directory:{}",record_id.simple()))
        .bind(command_id.as_uuid()).bind(ITEM_DIRECTORY_OWNER_EVENT_TYPE)
        .bind(i32::try_from(ITEM_DIRECTORY_FACT_VERSION).map_err(|_|PortError::InvalidState)?)
        .bind(draft.record.record_type().as_str()).bind(record_id)
        .bind(i64::try_from(draft.descriptor_version).map_err(|_|PortError::InvalidState)?)
        .bind(serde_json::to_value(&draft.scope).map_err(|_|PortError::InvalidState)?)
        .bind(actor).bind(correlation_id.as_uuid())
        .bind(serde_json::to_value(draft).map_err(|_|PortError::InvalidState)?)
        .bind(occurred_at).execute(&mut **tx).await.map_err(|_|PortError::Internal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn descriptor_publication_excludes_sensitive_fields() {
        let source = include_str!("publication.rs");
        for forbidden in [
            concat!("Comment", "Document"),
            concat!("source", "_digest"),
            concat!("reason", "_code"),
            concat!("evi", "dence"),
        ] {
            assert!(!source.contains(forbidden));
        }
        assert!(source.contains("policy.visibility_scope()"));
        assert!(source.contains("record_policy_id(&record)"));
        assert!(source.contains("policy.classification()"));
    }

    #[test]
    fn publisher_is_inside_the_thread_transaction_before_receipt() {
        let source = include_str!("write.rs");
        let fact = source.find("append_directory_draft(").expect("publisher");
        let receipt = source[fact..]
            .find("command_receipts")
            .map(|offset| fact + offset)
            .expect("receipt");
        let commit = source[receipt..]
            .find("tx.commit()")
            .map(|offset| receipt + offset)
            .expect("commit");
        assert!(fact < receipt && receipt < commit);
    }
}
