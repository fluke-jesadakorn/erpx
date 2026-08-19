use chrono::{DateTime, Utc};
use erpx_agent_orchestration::{
    AgentDefinition, AgentGovernanceRecord, AgentSession, Assignment, PortError,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft,
    RecordTypeId, StableRecordRef, catalog::ContextId, record_policy_id,
};
use erpx_kernel::{AuthorityScope, DataClassification, RecordId};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(super) fn definition_directory_drafts(
    definition: &AgentDefinition,
) -> Result<Vec<ItemDescriptorDraft>, PortError> {
    let mut values = vec![draft(
        "agent.definition",
        definition.id().as_uuid(),
        definition.version(),
        definition.scope(),
        token(definition.status())?,
        DataClassification::Internal,
        definition.name().to_owned(),
        definition.id().as_uuid(),
    )?];
    for version in definition.versions().values() {
        values.push(draft(
            "agent.version",
            version.manifest.version_id.as_uuid(),
            definition.version(),
            definition.scope(),
            token(version.status)?,
            DataClassification::Internal.stricter(version.manifest.maximum_classification),
            format!(
                "{} version {}",
                definition.name(),
                version.manifest.version_number
            ),
            definition.id().as_uuid(),
        )?);
    }
    Ok(values)
}

pub(super) fn assignment_directory_draft(
    assignment: &Assignment,
) -> Result<ItemDescriptorDraft, PortError> {
    draft(
        "agent.assignment",
        assignment.id().as_uuid(),
        assignment.version(),
        assignment.scope(),
        token(assignment.state())?,
        DataClassification::Confidential.stricter(assignment.pin().manifest.maximum_classification),
        format!("Agent assignment {}", assignment.id().as_uuid()),
        assignment.id().as_uuid(),
    )
}

pub(super) fn governance_directory_draft(
    record: &AgentGovernanceRecord,
) -> Result<ItemDescriptorDraft, PortError> {
    draft(
        record.kind().record_type(),
        record.id(),
        record.version(),
        record.scope(),
        token(record.status())?,
        DataClassification::Restricted,
        format!("Agent governance record {}", record.id()),
        record.id(),
    )
}

/// Private session content remains undiscoverable until a human-approved
/// publication exists. The published descriptor remains opaque.
pub(super) fn published_session_directory_draft(
    session: &AgentSession,
) -> Result<Option<ItemDescriptorDraft>, PortError> {
    if !session
        .messages()
        .iter()
        .any(|message| message.published_at.is_some())
    {
        return Ok(None);
    }
    let state = if session.expired_at().is_some() {
        "expired"
    } else if session.legal_hold() {
        "legal-hold"
    } else {
        "published"
    };
    draft(
        "agent.session",
        session.id().as_uuid(),
        session.version(),
        session.scope(),
        state.into(),
        DataClassification::Restricted,
        format!("Published AI session {}", session.id().as_uuid()),
        session.id().as_uuid(),
    )
    .map(Some)
}

#[allow(clippy::too_many_arguments)]
fn draft(
    record_type: &str,
    id: Uuid,
    version: u64,
    scope: &AuthorityScope,
    lifecycle_state: String,
    classification: DataClassification,
    authorized_search_text: String,
    _policy_seed: Uuid,
) -> Result<ItemDescriptorDraft, PortError> {
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(id).map_err(|_| PortError::Persistence)?,
        ContextId::AgentOrchestration,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::Persistence)?,
    )
    .map_err(|_| PortError::Persistence)?;
    let authorization_policy = record_policy_id(&record).map_err(|_| PortError::Persistence)?;
    let value = ItemDescriptorDraft {
        record,
        scope: scope.clone(),
        lifecycle_state,
        authorization_policy,
        classification,
        descriptor_version: version,
        authorized_search_text,
    };
    value.validate().map_err(|_| PortError::Persistence)?;
    Ok(value)
}

fn token(value: impl serde::Serialize) -> Result<String, PortError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .ok_or(PortError::Persistence)
}

pub(super) async fn append_assignment_directory_draft(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: Uuid,
    assignment: &Assignment,
    actor: serde_json::Value,
    occurred_at: DateTime<Utc>,
) -> Result<(), PortError> {
    let draft = assignment_directory_draft(assignment)?;
    append_with_idempotency(
        tx,
        holding_id,
        &format!(
            "assignment-directory:{}:{}",
            assignment.id().as_uuid(),
            assignment.version()
        ),
        actor,
        occurred_at,
        &draft,
    )
    .await
}

pub(super) async fn append_with_idempotency(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: Uuid,
    idempotency_key: &str,
    actor: serde_json::Value,
    occurred_at: DateTime<Utc>,
    draft: &ItemDescriptorDraft,
) -> Result<(), PortError> {
    let record_id = draft.record.id().as_uuid();
    let command_id = stable_id(
        holding_id,
        record_id,
        idempotency_key.as_bytes(),
        b"command",
    );
    let correlation_id = stable_id(
        holding_id,
        record_id,
        idempotency_key.as_bytes(),
        b"correlation",
    );
    let event_id = stable_id(
        holding_id,
        record_id,
        &draft.descriptor_version.to_be_bytes(),
        draft.record.record_type().as_str().as_bytes(),
    );
    sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES ($1,$2,'agent_orchestration',$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,NULL,$13,$14,$14) ON CONFLICT (holding_id,event_id) DO NOTHING")
        .bind(holding_id).bind(event_id).bind(format!("item-directory:{}",record_id.simple()))
        .bind(command_id).bind(ITEM_DIRECTORY_OWNER_EVENT_TYPE)
        .bind(i32::try_from(ITEM_DIRECTORY_FACT_VERSION).map_err(|_|PortError::Persistence)?)
        .bind(draft.record.record_type().as_str()).bind(record_id)
        .bind(i64::try_from(draft.descriptor_version).map_err(|_|PortError::Persistence)?)
        .bind(serde_json::to_value(&draft.scope).map_err(|_|PortError::Persistence)?)
        .bind(actor).bind(correlation_id)
        .bind(serde_json::to_value(draft).map_err(|_|PortError::Persistence)?)
        .bind(occurred_at).execute(&mut **tx).await.map_err(|_|PortError::Persistence)?;
    Ok(())
}

fn stable_id(holding_id: Uuid, aggregate_id: Uuid, value: &[u8], purpose: &[u8]) -> Uuid {
    let namespace = Uuid::new_v5(&holding_id, aggregate_id.as_bytes());
    let mut name = Vec::with_capacity(value.len() + purpose.len() + 1);
    name.extend_from_slice(value);
    name.push(0);
    name.extend_from_slice(purpose);
    Uuid::new_v5(&namespace, &name)
}

#[cfg(test)]
mod tests {
    #[test]
    fn private_material_is_structurally_absent_from_directory_publication() {
        let source = include_str!("publication.rs");
        for forbidden in [
            concat!("prompt", "_ref"),
            concat!("output", "_digest"),
            concat!("encrypted", "_payload_ref"),
            concat!("content", "_digest"),
            concat!("payload", "_digest"),
        ] {
            assert!(!source.contains(forbidden));
        }
        assert!(source.contains("published_at.is_some()"));
        assert!(source.contains("return Ok(None)"));
    }

    #[test]
    fn unsupported_agent_route_families_are_not_fabricated() {
        let source = include_str!("publication.rs");
        for absent in [
            concat!("agent.", "action-card"),
            concat!("agent.", "delegation-tree"),
            concat!("agent.", "budget-decision"),
        ] {
            assert!(!source.contains(absent));
        }
    }
}
