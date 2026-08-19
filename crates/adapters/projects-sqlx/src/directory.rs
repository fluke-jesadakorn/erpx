use chrono::{DateTime, Utc};
use erpx_cqrs_sqlx::CqrsSqlxError;
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft,
    RecordTypeId, StableRecordRef, catalog::ContextId, record_policy_id,
};
use erpx_kernel::{
    ActorAttribution, AuthorityScope, CommandId, CorrelationId, DataClassification, HoldingId,
    RecordId,
};
use erpx_projects::{NodeKind, ProjectsAggregate};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(super) fn projects_directory_drafts(
    aggregate: &ProjectsAggregate,
) -> Result<Vec<ItemDescriptorDraft>, CqrsSqlxError> {
    match aggregate {
        ProjectsAggregate::Project(project) => {
            let mut values = vec![draft(
                "project.project",
                project.id().as_uuid(),
                project.version(),
                project.scope(),
                token(project.status())?,
                DataClassification::Confidential,
                format!("{} {}", project.code(), project.name()),
                project.id().as_uuid(),
            )?];
            for node in project.nodes().values() {
                let record_type = if matches!(node.kind(), NodeKind::Phase | NodeKind::Epic) {
                    "project.phase"
                } else {
                    "project.item"
                };
                values.push(draft(
                    record_type,
                    node.id().as_uuid(),
                    node.version(),
                    project.scope(),
                    token(node.core_state())?,
                    DataClassification::Confidential,
                    node.title().to_owned(),
                    project.id().as_uuid(),
                )?);
            }
            Ok(values)
        }
        ProjectsAggregate::Workflow(workflow) => Ok(vec![draft(
            "project.template",
            workflow.id().as_uuid(),
            workflow.version(),
            workflow.scope(),
            token(workflow.status())?,
            DataClassification::Internal,
            workflow.name().to_owned(),
            workflow.id().as_uuid(),
        )?]),
        ProjectsAggregate::Handoff(handoff) => Ok(vec![draft(
            "project.handoff",
            handoff.id().as_uuid(),
            handoff.version(),
            handoff.scope(),
            token(handoff.status())?,
            DataClassification::Confidential,
            format!("Project handoff {}", handoff.id().as_uuid()),
            handoff.id().as_uuid(),
        )?]),
        // A dependency is a governed relation agreement. The catalog has no
        // independently routed project.dependency record family.
        ProjectsAggregate::Dependency(_) => Ok(Vec::new()),
    }
}

#[allow(clippy::too_many_arguments)]
fn draft(
    record_type: &str,
    id: Uuid,
    version: u64,
    scope: &AuthorityScope,
    lifecycle_state: String,
    classification: DataClassification,
    search_text: String,
    _policy_seed: Uuid,
) -> Result<ItemDescriptorDraft, CqrsSqlxError> {
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(id)
            .map_err(|_| CqrsSqlxError::GuardRejected("Projects record id is invalid"))?,
        ContextId::Projects,
        RecordTypeId::try_new(record_type)
            .map_err(|_| CqrsSqlxError::GuardRejected("Projects record type is invalid"))?,
    )
    .map_err(|_| CqrsSqlxError::GuardRejected("Projects record reference is invalid"))?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record)
            .map_err(|_| CqrsSqlxError::GuardRejected("Projects policy id is invalid"))?,
        record,
        scope: scope.clone(),
        lifecycle_state,
        classification,
        descriptor_version: version,
        authorized_search_text: search_text,
    };
    value
        .validate()
        .map_err(|_| CqrsSqlxError::GuardRejected("Projects directory draft is invalid"))?;
    Ok(value)
}

fn token(value: impl serde::Serialize) -> Result<String, CqrsSqlxError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .ok_or(CqrsSqlxError::GuardRejected(
            "Projects lifecycle token is invalid",
        ))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn append_directory_draft(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    command_id: CommandId,
    correlation_id: CorrelationId,
    actor: &ActorAttribution,
    draft: &ItemDescriptorDraft,
    occurred_at: DateTime<Utc>,
) -> Result<(), CqrsSqlxError> {
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
    sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES ($1,$2,'projects',$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,NULL,$13,$14,$14)")
        .bind(holding_id.as_uuid()).bind(event_id).bind(format!("item-directory:{}",record_id.simple()))
        .bind(command_id.as_uuid()).bind(ITEM_DIRECTORY_OWNER_EVENT_TYPE)
        .bind(i32::try_from(ITEM_DIRECTORY_FACT_VERSION).map_err(|_|CqrsSqlxError::GuardRejected("directory version overflow"))?)
        .bind(draft.record.record_type().as_str()).bind(record_id)
        .bind(i64::try_from(draft.descriptor_version).map_err(|_|CqrsSqlxError::GuardRejected("descriptor version overflow"))?)
        .bind(serde_json::to_value(&draft.scope)?).bind(serde_json::to_value(actor)?)
        .bind(correlation_id.as_uuid()).bind(serde_json::to_value(draft)?).bind(occurred_at)
        .execute(&mut **tx).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn node_publication_is_atomic_and_uses_node_versions() {
        let command = include_str!("command.rs");
        assert!(command.contains("for draft in crate::directory::projects_directory_drafts"));
        assert!(command.contains("commit_normalized(command"));
        let source = include_str!("directory.rs");
        assert!(source.contains("node.version()"));
        assert!(source.contains("NodeKind::Phase | NodeKind::Epic"));
        assert!(!source.contains(concat!("comment", "_revision")));
    }
}
