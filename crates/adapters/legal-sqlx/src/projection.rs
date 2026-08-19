use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{ItemDescriptorDraft, legal};
use erpx_legal::ports::{AccessEntryView, ContractView, DocumentView, MatterView};
use erpx_legal::{ContractStatus, LegalAggregate, MatterStatus, legal_hold_record_ref_for};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    ACCESS_AGGREGATE_TYPE, ACCESS_PROJECTION, ACCESS_SNAPSHOT_EVENT, CONTEXT_NAME,
    CONTRACT_AGGREGATE_TYPE, CONTRACT_PROJECTION, CONTRACT_SNAPSHOT_EVENT, DOCUMENT_AGGREGATE_TYPE,
    DOCUMENT_PROJECTION, DOCUMENT_SNAPSHOT_EVENT, EVENT_VERSION, MATTER_AGGREGATE_TYPE,
    MATTER_PROJECTION, MATTER_SNAPSHOT_EVENT, access_partition, command::scope_key,
    contract_partition, document_partition, matter_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum LegalSnapshot {
    Contract(ContractView),
    Document(DocumentView),
    AccessEntry(AccessEntryView),
    Matter(MatterView),
}
impl From<&LegalAggregate> for LegalSnapshot {
    fn from(value: &LegalAggregate) -> Self {
        match value {
            LegalAggregate::Contract(value) => Self::Contract(value.into()),
            LegalAggregate::Document(value) => Self::Document(value.into()),
            LegalAggregate::AccessEntry(value) => Self::AccessEntry(value.into()),
            LegalAggregate::Matter(value) => Self::Matter(value.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LegalOutboxPayload {
    Integration(legal::Event),
    Snapshot(LegalSnapshot),
    Directory(ItemDescriptorDraft),
}

macro_rules! projector_type {
    ($name:ident,$projection:expr,$event:expr) => {
        #[derive(Debug, Clone)]
        pub struct $name {
            events: Vec<String>,
        }
        impl $name {
            pub fn new() -> Self {
                Self {
                    events: vec![$event.into()],
                }
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl $name {
            fn projection_name(&self) -> &'static str {
                $projection
            }
            fn subscribed_events(&self) -> &[String] {
                &self.events
            }
        }
    };
}
projector_type!(
    LegalContractProjector,
    CONTRACT_PROJECTION,
    CONTRACT_SNAPSHOT_EVENT
);
projector_type!(
    LegalMatterProjector,
    MATTER_PROJECTION,
    MATTER_SNAPSHOT_EVENT
);
projector_type!(
    LegalDocumentProjector,
    DOCUMENT_PROJECTION,
    DOCUMENT_SNAPSHOT_EVENT
);
projector_type!(
    LegalAccessHistoryProjector,
    ACCESS_PROJECTION,
    ACCESS_SNAPSHOT_EVENT
);

#[async_trait]
impl Projector for LegalContractProjector {
    fn name(&self) -> &str {
        self.projection_name()
    }
    fn event_types(&self) -> &[String] {
        self.subscribed_events()
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let LegalSnapshot::Contract(view) = validate_event(
            context,
            event,
            CONTRACT_PROJECTION,
            CONTRACT_AGGREGATE_TYPE,
            CONTRACT_SNAPSHOT_EVENT,
        )?
        else {
            return Err(ProjectionFailure::new(
                "Legal contract projector received another aggregate",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        let parties = serde_json::to_value(&view.parties).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_legal_read.contract_projection (projection_name,generation,partition_key,contract_id,source_aggregate_version,source_global_sequence,active_scope,scope_key,reference,title,object_ref,content_sha256,retention_until,contract_status,parties,created_by,approved_by,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,now()) ON CONFLICT (holding_id,projection_name,generation,partition_key,contract_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,reference=EXCLUDED.reference,title=EXCLUDED.title,object_ref=EXCLUDED.object_ref,content_sha256=EXCLUDED.content_sha256,retention_until=EXCLUDED.retention_until,contract_status=EXCLUDED.contract_status,parties=EXCLUDED.parties,created_by=EXCLUDED.created_by,approved_by=EXCLUDED.approved_by,state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_legal_read.contract_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(CONTRACT_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.contract_id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?)
            .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?).bind(&view.reference).bind(&view.title).bind(&view.object_ref).bind(&view.content_sha256)
            .bind(view.retention_until).bind(contract_status_name(view.status)).bind(parties).bind(&view.created_by).bind(&view.approved_by).bind(state).bind(view.created_at).bind(view.updated_at)
            .execute(&mut **tx).await.map_err(failure)?;
        Ok(())
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(
            tx,
            context,
            "erpx_legal_read.contract_projection",
            CONTRACT_PROJECTION,
        )
        .await
    }
}

#[async_trait]
impl Projector for LegalDocumentProjector {
    fn name(&self) -> &str {
        self.projection_name()
    }
    fn event_types(&self) -> &[String] {
        self.subscribed_events()
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let LegalSnapshot::Document(view) = validate_event(
            context,
            event,
            DOCUMENT_PROJECTION,
            DOCUMENT_AGGREGATE_TYPE,
            DOCUMENT_SNAPSHOT_EVENT,
        )?
        else {
            return Err(ProjectionFailure::new(
                "Legal document projector received another aggregate",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        let versions = serde_json::to_value(&view.versions).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_legal_read.document_projection (projection_name,generation,partition_key,document_id,source_aggregate_version,source_global_sequence,active_scope,scope_key,title,versions,current_document_version,archived,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,now()) ON CONFLICT (holding_id,projection_name,generation,partition_key,document_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,title=EXCLUDED.title,versions=EXCLUDED.versions,current_document_version=EXCLUDED.current_document_version,archived=EXCLUDED.archived,state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_legal_read.document_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(DOCUMENT_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.document_id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?)
            .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?).bind(&view.title).bind(versions).bind(to_i64(view.current_version)?).bind(view.archived).bind(state).bind(view.created_at).bind(view.updated_at)
            .execute(&mut **tx).await.map_err(failure)?;
        Ok(())
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(
            tx,
            context,
            "erpx_legal_read.document_projection",
            DOCUMENT_PROJECTION,
        )
        .await
    }
}

#[async_trait]
impl Projector for LegalAccessHistoryProjector {
    fn name(&self) -> &str {
        self.projection_name()
    }
    fn event_types(&self) -> &[String] {
        self.subscribed_events()
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let LegalSnapshot::AccessEntry(view) = validate_event(
            context,
            event,
            ACCESS_PROJECTION,
            ACCESS_AGGREGATE_TYPE,
            ACCESS_SNAPSHOT_EVENT,
        )?
        else {
            return Err(ProjectionFailure::new(
                "Legal access projector received another aggregate",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_legal_read.access_history_projection (projection_name,generation,partition_key,access_entry_id,source_aggregate_version,source_global_sequence,active_scope,scope_key,document_id,document_version,actor_ref,action,purpose,observed_at,state,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,now()) ON CONFLICT (holding_id,projection_name,generation,partition_key,access_entry_id) DO NOTHING")
            .bind(ACCESS_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.access_entry_id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?)
            .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?).bind(view.document_id.as_uuid()).bind(to_i64(view.document_version)?)
            .bind(&view.actor_ref).bind(&view.action).bind(&view.purpose).bind(view.observed_at).bind(state).execute(&mut **tx).await.map_err(failure)?;
        Ok(())
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(
            tx,
            context,
            "erpx_legal_read.access_history_projection",
            ACCESS_PROJECTION,
        )
        .await
    }
}

#[async_trait]
impl Projector for LegalMatterProjector {
    fn name(&self) -> &str {
        self.projection_name()
    }
    fn event_types(&self) -> &[String] {
        self.subscribed_events()
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let LegalSnapshot::Matter(view) = validate_event(
            context,
            event,
            MATTER_PROJECTION,
            MATTER_AGGREGATE_TYPE,
            MATTER_SNAPSHOT_EVENT,
        )?
        else {
            return Err(ProjectionFailure::new(
                "Legal matter projector received another aggregate",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        let links = serde_json::to_value(&view.links).map_err(failure)?;
        let legal_hold = serde_json::to_value(&view.legal_hold).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_legal_read.matter_projection (projection_name,generation,partition_key,matter_id,source_aggregate_version,source_global_sequence,active_scope,scope_key,reference,title,matter_status,links,legal_hold,created_by,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,now()) ON CONFLICT (holding_id,projection_name,generation,partition_key,matter_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,reference=EXCLUDED.reference,title=EXCLUDED.title,matter_status=EXCLUDED.matter_status,links=EXCLUDED.links,legal_hold=EXCLUDED.legal_hold,created_by=EXCLUDED.created_by,state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_legal_read.matter_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(MATTER_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.matter_id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?)
            .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?).bind(&view.reference).bind(&view.title).bind(matter_status_name(view.status)).bind(links).bind(legal_hold).bind(&view.created_by).bind(state).bind(view.created_at).bind(view.updated_at)
            .execute(&mut **tx).await.map_err(failure)?;
        if let Some(hold) = &view.legal_hold {
            let hold_ref = legal_hold_record_ref_for(view.matter_id).map_err(failure)?;
            sqlx::query("INSERT INTO erpx_legal_read.legal_hold_projection
                (holding_id,projection_name,generation,partition_key,legal_hold_id,matter_id,
                 source_aggregate_version,source_global_sequence,active_scope,scope_key,
                 matter_reference,hold_active,reason,placed_by,placed_at,released_by,released_at,
                 aggregate_updated_at,projected_at)
                VALUES (erpx_cqrs_read.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,now())
                ON CONFLICT (holding_id,projection_name,generation,partition_key,legal_hold_id)
                DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
                 source_global_sequence=EXCLUDED.source_global_sequence,
                 hold_active=EXCLUDED.hold_active,released_by=EXCLUDED.released_by,
                 released_at=EXCLUDED.released_at,
                 aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
                WHERE erpx_legal_read.legal_hold_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
                .bind(MATTER_PROJECTION).bind(context.generation).bind(&event.partition)
                .bind(hold_ref.id().as_uuid()).bind(view.matter_id.as_uuid())
                .bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?)
                .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?)
                .bind(&view.reference).bind(hold.active).bind(&hold.reason).bind(&hold.placed_by)
                .bind(hold.placed_at).bind(&hold.released_by).bind(hold.released_at).bind(view.updated_at)
                .execute(&mut **tx).await.map_err(failure)?;
        }
        Ok(())
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(
            tx,
            context,
            "erpx_legal_read.matter_projection",
            MATTER_PROJECTION,
        )
        .await?;
        reset(
            tx,
            context,
            "erpx_legal_read.legal_hold_projection",
            MATTER_PROJECTION,
        )
        .await
    }
}

fn validate_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    projection: &str,
    aggregate_type: &str,
    event_type: &str,
) -> Result<LegalSnapshot, ProjectionFailure> {
    if context.projection != projection
        || event.context != CONTEXT_NAME
        || event.aggregate_type != aggregate_type
        || event.event_type != event_type
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "Legal projector received incompatible event",
        ));
    }
    let payload: LegalOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    let LegalOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "Legal snapshot event carried integration payload",
        ));
    };
    let matches = match &snapshot {
        LegalSnapshot::Contract(v) => {
            aggregate_type == CONTRACT_AGGREGATE_TYPE
                && v.version == event.aggregate_version
                && v.contract_id.as_uuid() == event.aggregate_id.as_uuid()
                && v.scope == event.active_scope
                && event.partition == contract_partition(v.contract_id)
        }
        LegalSnapshot::Document(v) => {
            aggregate_type == DOCUMENT_AGGREGATE_TYPE
                && v.version == event.aggregate_version
                && v.document_id.as_uuid() == event.aggregate_id.as_uuid()
                && v.scope == event.active_scope
                && event.partition == document_partition(v.document_id)
        }
        LegalSnapshot::AccessEntry(v) => {
            aggregate_type == ACCESS_AGGREGATE_TYPE
                && v.version == event.aggregate_version
                && v.access_entry_id.as_uuid() == event.aggregate_id.as_uuid()
                && v.scope == event.active_scope
                && event.partition == access_partition(v.access_entry_id)
        }
        LegalSnapshot::Matter(v) => {
            aggregate_type == MATTER_AGGREGATE_TYPE
                && v.version == event.aggregate_version
                && v.matter_id.as_uuid() == event.aggregate_id.as_uuid()
                && v.scope == event.active_scope
                && event.partition == matter_partition(v.matter_id)
        }
    };
    if !matches {
        return Err(ProjectionFailure::new("Legal snapshot metadata mismatch"));
    }
    Ok(snapshot)
}
async fn reset(
    tx: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    table: &str,
    projection: &str,
) -> Result<(), ProjectionFailure> {
    let statement = format!("DELETE FROM {table} WHERE projection_name=$1 AND generation=$2");
    sqlx::query(&statement)
        .bind(projection)
        .bind(context.generation)
        .execute(&mut **tx)
        .await
        .map_err(failure)?;
    Ok(())
}
pub(crate) const fn contract_status_name(value: ContractStatus) -> &'static str {
    match value {
        ContractStatus::Draft => "draft",
        ContractStatus::Review => "review",
        ContractStatus::Approved => "approved",
        ContractStatus::Active => "active",
        ContractStatus::Expired => "expired",
        ContractStatus::Terminated => "terminated",
        ContractStatus::Archived => "archived",
    }
}
pub(crate) const fn matter_status_name(value: MatterStatus) -> &'static str {
    match value {
        MatterStatus::Open => "open",
        MatterStatus::Closed => "closed",
    }
}
fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| ProjectionFailure::new("numeric value is too large"))
}
fn failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn four_projectors_are_snapshot_specific() {
        assert_eq!(
            LegalContractProjector::new().event_types(),
            &[CONTRACT_SNAPSHOT_EVENT.to_owned()]
        );
        assert_eq!(
            LegalDocumentProjector::new().event_types(),
            &[DOCUMENT_SNAPSHOT_EVENT.to_owned()]
        );
        assert_eq!(
            LegalAccessHistoryProjector::new().event_types(),
            &[ACCESS_SNAPSHOT_EVENT.to_owned()]
        );
        assert_eq!(
            LegalMatterProjector::new().event_types(),
            &[MATTER_SNAPSHOT_EVENT.to_owned()]
        );
    }
    #[test]
    fn projectors_are_duplicate_safe_and_rebuildable() {
        let source = include_str!("projection.rs");
        assert_eq!(
            source
                .matches(concat!(
                    "source_aggregate_version < ",
                    "EXCLUDED.source_aggregate_version"
                ))
                .count(),
            4
        );
        assert!(source.contains("DO NOTHING"));
        assert_eq!(source.matches(concat!("async fn ", "reset(")).count(), 5);
    }
}
