use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_collaboration::{
    Audience, CollaborationStreamEvent, CommentProjectionContent, OpaqueInboxFact, PortError,
    Thread, ThreadId, ThreadProjection,
};
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_kernel::{AuthorityScope, CommandId, CorrelationId, HoldingId};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    CONTEXT_NAME, EVENT_VERSION, INBOX_PROJECTION, INBOX_PROJECTION_FACT_EVENT, STREAM_PROJECTION,
    STREAM_PROJECTION_FACT_EVENT, THREAD_PROJECTION, THREAD_PROJECTION_FACT_EVENT,
};

const AGGREGATE_TYPE: &str = "thread";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborationThreadProjectionFact {
    pub holding_id: HoldingId,
    pub thread_id: ThreadId,
    pub aggregate_version: u64,
    pub visibility_scope: AuthorityScope,
    pub audience: Audience,
    pub projection: ThreadProjection,
    pub source_event_cursor: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborationStreamProjectionFact {
    pub holding_id: HoldingId,
    pub thread_id: ThreadId,
    pub aggregate_version: u64,
    pub visibility_scope: AuthorityScope,
    pub events: Vec<CollaborationStreamEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborationInboxProjectionFact {
    pub holding_id: HoldingId,
    pub thread_id: ThreadId,
    pub aggregate_version: u64,
    pub visibility_scope: AuthorityScope,
    pub facts: Vec<OpaqueInboxFact>,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn append_projection_facts(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    command_id: CommandId,
    correlation_id: CorrelationId,
    actor: serde_json::Value,
    thread: &Thread,
    last_event_cursor: u64,
    stream_events: Vec<CollaborationStreamEvent>,
    inbox_facts: &[OpaqueInboxFact],
) -> Result<(), PortError> {
    if holding_id != thread.policy().holding_id()
        || last_event_cursor == 0
        || stream_events.is_empty()
        || stream_events.iter().any(|event| {
            event.holding_id != holding_id
                || event.thread_id != thread.id()
                || event.cursor == 0
                || event.aggregate_version == 0
        })
        || inbox_facts.iter().any(|fact| {
            fact.holding_id != holding_id
                || fact.thread_id != thread.id()
                || fact.anchor_record_id != thread.anchor().record().id()
        })
    {
        return Err(PortError::InvalidState);
    }

    let scope = thread.policy().visibility_scope().clone();
    let thread_fact = CollaborationThreadProjectionFact {
        holding_id,
        thread_id: thread.id(),
        aggregate_version: thread.version(),
        visibility_scope: scope.clone(),
        audience: thread.audience().clone(),
        projection: ThreadProjection::from_authorized_thread(thread, last_event_cursor),
        source_event_cursor: last_event_cursor,
        updated_at: thread.updated_at(),
    };
    append_fact(
        tx,
        holding_id,
        command_id,
        correlation_id,
        actor.clone(),
        thread.id(),
        thread.version(),
        &scope,
        THREAD_PROJECTION_FACT_EVENT,
        &thread_fact,
        thread.updated_at(),
    )
    .await?;

    let stream_fact = CollaborationStreamProjectionFact {
        holding_id,
        thread_id: thread.id(),
        aggregate_version: thread.version(),
        visibility_scope: scope.clone(),
        events: stream_events,
    };
    append_fact(
        tx,
        holding_id,
        command_id,
        correlation_id,
        actor.clone(),
        thread.id(),
        thread.version(),
        &scope,
        STREAM_PROJECTION_FACT_EVENT,
        &stream_fact,
        thread.updated_at(),
    )
    .await?;

    if !inbox_facts.is_empty() {
        let inbox_fact = CollaborationInboxProjectionFact {
            holding_id,
            thread_id: thread.id(),
            aggregate_version: thread.version(),
            visibility_scope: scope.clone(),
            facts: inbox_facts.to_vec(),
        };
        append_fact(
            tx,
            holding_id,
            command_id,
            correlation_id,
            actor,
            thread.id(),
            thread.version(),
            &scope,
            INBOX_PROJECTION_FACT_EVENT,
            &inbox_fact,
            thread.updated_at(),
        )
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn append_fact<T: Serialize>(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    command_id: CommandId,
    correlation_id: CorrelationId,
    actor: serde_json::Value,
    thread_id: ThreadId,
    aggregate_version: u64,
    scope: &AuthorityScope,
    event_type: &str,
    payload: &T,
    occurred_at: DateTime<Utc>,
) -> Result<(), PortError> {
    let event_id = Uuid::new_v5(
        &command_id.as_uuid(),
        format!("{event_type}:{}", thread_id.as_uuid()).as_bytes(),
    );
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.outbox_events \
         (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,NULL,$14,$15,$15)",
    )
    .bind(holding_id.as_uuid())
    .bind(event_id)
    .bind(CONTEXT_NAME)
    .bind(partition(thread_id))
    .bind(command_id.as_uuid())
    .bind(event_type)
    .bind(i32::try_from(EVENT_VERSION).map_err(|_| PortError::InvalidState)?)
    .bind(AGGREGATE_TYPE)
    .bind(thread_id.as_uuid())
    .bind(i64::try_from(aggregate_version).map_err(|_| PortError::InvalidState)?)
    .bind(serde_json::to_value(scope).map_err(|_| PortError::InvalidState)?)
    .bind(actor)
    .bind(correlation_id.as_uuid())
    .bind(serde_json::to_value(payload).map_err(|_| PortError::InvalidState)?)
    .bind(occurred_at)
    .execute(&mut **tx)
    .await
    .map_err(|_| PortError::Internal)?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct CollaborationThreadProjector {
    holding_id: HoldingId,
    events: Vec<String>,
}

impl CollaborationThreadProjector {
    pub fn new(holding_id: HoldingId) -> Self {
        Self {
            holding_id,
            events: vec![THREAD_PROJECTION_FACT_EVENT.into()],
        }
    }
}

#[async_trait]
impl Projector for CollaborationThreadProjector {
    fn name(&self) -> &str {
        THREAD_PROJECTION
    }

    fn event_types(&self) -> &[String] {
        &self.events
    }

    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        set_holding(tx, self.holding_id).await?;
        let fact: CollaborationThreadProjectionFact = decode(event, THREAD_PROJECTION_FACT_EVENT)?;
        validate_envelope(
            event,
            context,
            THREAD_PROJECTION,
            self.holding_id,
            fact.holding_id,
            fact.thread_id,
            fact.aggregate_version,
            &fact.visibility_scope,
        )?;
        let expected_portal = match &fact.audience {
            Audience::Internal { .. } => None,
            Audience::ExternalPortal { portal, .. } => Some(*portal),
        };
        if fact.source_event_cursor == 0
            || fact.projection.id != fact.thread_id
            || fact.projection.projection_cursor != fact.source_event_cursor
            || fact.projection.external_portal != expected_portal
        {
            return Err(failure("invalid collaboration thread projection fact"));
        }
        let search_text = fact
            .projection
            .comments
            .iter()
            .filter_map(|comment| match &comment.content {
                CommentProjectionContent::Visible(document) => Some(document.plain_text()),
                CommentProjectionContent::Tombstone | CommentProjectionContent::Restricted => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        sqlx::query(
            "INSERT INTO erpx_collaboration_read.thread_projection \
             (holding_id,projection_name,generation,thread_id,anchor_record_id,visibility_scope,audience,projection,search_text,source_event_cursor,updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) \
             ON CONFLICT (holding_id,projection_name,generation,thread_id) DO UPDATE SET \
              anchor_record_id=EXCLUDED.anchor_record_id,visibility_scope=EXCLUDED.visibility_scope, \
              audience=EXCLUDED.audience,projection=EXCLUDED.projection,search_text=EXCLUDED.search_text, \
              source_event_cursor=EXCLUDED.source_event_cursor,updated_at=EXCLUDED.updated_at \
             WHERE erpx_collaboration_read.thread_projection.source_event_cursor < EXCLUDED.source_event_cursor",
        )
        .bind(self.holding_id.as_uuid())
        .bind(THREAD_PROJECTION)
        .bind(context.generation)
        .bind(fact.thread_id.as_uuid())
        .bind(fact.projection.anchor_record_id.as_uuid())
        .bind(serde_json::to_value(&fact.visibility_scope).map_err(map_failure)?)
        .bind(serde_json::to_value(&fact.audience).map_err(map_failure)?)
        .bind(serde_json::to_value(&fact.projection).map_err(map_failure)?)
        .bind(search_text)
        .bind(i64::try_from(fact.source_event_cursor).map_err(map_failure)?)
        .bind(fact.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(map_failure)?;
        Ok(())
    }

    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(tx, context, self.holding_id, "thread_projection").await
    }
}

#[derive(Debug, Clone)]
pub struct CollaborationStreamProjector {
    holding_id: HoldingId,
    events: Vec<String>,
}

impl CollaborationStreamProjector {
    pub fn new(holding_id: HoldingId) -> Self {
        Self {
            holding_id,
            events: vec![STREAM_PROJECTION_FACT_EVENT.into()],
        }
    }
}

#[async_trait]
impl Projector for CollaborationStreamProjector {
    fn name(&self) -> &str {
        STREAM_PROJECTION
    }

    fn event_types(&self) -> &[String] {
        &self.events
    }

    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        set_holding(tx, self.holding_id).await?;
        let fact: CollaborationStreamProjectionFact = decode(event, STREAM_PROJECTION_FACT_EVENT)?;
        validate_envelope(
            event,
            context,
            STREAM_PROJECTION,
            self.holding_id,
            fact.holding_id,
            fact.thread_id,
            fact.aggregate_version,
            &fact.visibility_scope,
        )?;
        if fact.events.is_empty() {
            return Err(failure("empty collaboration stream projection fact"));
        }
        for value in fact.events {
            if value.holding_id != self.holding_id
                || value.thread_id != fact.thread_id
                || value.cursor == 0
                || value.aggregate_version == 0
            {
                return Err(failure("invalid collaboration stream event"));
            }
            let inserted = sqlx::query(
                "INSERT INTO erpx_collaboration_read.stream_projection \
                 (holding_id,projection_name,generation,thread_id,event_cursor,aggregate_version,event_kind,occurred_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
            )
            .bind(self.holding_id.as_uuid())
            .bind(STREAM_PROJECTION)
            .bind(context.generation)
            .bind(value.thread_id.as_uuid())
            .bind(i64::try_from(value.cursor).map_err(map_failure)?)
            .bind(i64::try_from(value.aggregate_version).map_err(map_failure)?)
            .bind(serde_json::to_value(&value.kind).map_err(map_failure)?)
            .bind(value.occurred_at)
            .execute(&mut **tx)
            .await
            .map_err(map_failure)?
            .rows_affected();
            if inserted == 0 {
                let exact: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM erpx_collaboration_read.stream_projection \
                     WHERE holding_id=$1 AND projection_name=$2 AND generation=$3 AND thread_id=$4 \
                     AND event_cursor=$5 AND aggregate_version=$6 AND event_kind=$7 AND occurred_at=$8)",
                )
                .bind(self.holding_id.as_uuid())
                .bind(STREAM_PROJECTION)
                .bind(context.generation)
                .bind(value.thread_id.as_uuid())
                .bind(i64::try_from(value.cursor).map_err(map_failure)?)
                .bind(i64::try_from(value.aggregate_version).map_err(map_failure)?)
                .bind(serde_json::to_value(&value.kind).map_err(map_failure)?)
                .bind(value.occurred_at)
                .fetch_one(&mut **tx)
                .await
                .map_err(map_failure)?;
                if !exact {
                    return Err(failure("collaboration stream cursor collision"));
                }
            }
        }
        Ok(())
    }

    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(tx, context, self.holding_id, "stream_projection").await
    }
}

#[derive(Debug, Clone)]
pub struct CollaborationInboxProjector {
    holding_id: HoldingId,
    events: Vec<String>,
}

impl CollaborationInboxProjector {
    pub fn new(holding_id: HoldingId) -> Self {
        Self {
            holding_id,
            events: vec![INBOX_PROJECTION_FACT_EVENT.into()],
        }
    }
}

#[async_trait]
impl Projector for CollaborationInboxProjector {
    fn name(&self) -> &str {
        INBOX_PROJECTION
    }

    fn event_types(&self) -> &[String] {
        &self.events
    }

    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        set_holding(tx, self.holding_id).await?;
        let fact: CollaborationInboxProjectionFact = decode(event, INBOX_PROJECTION_FACT_EVENT)?;
        validate_envelope(
            event,
            context,
            INBOX_PROJECTION,
            self.holding_id,
            fact.holding_id,
            fact.thread_id,
            fact.aggregate_version,
            &fact.visibility_scope,
        )?;
        if fact.facts.is_empty() {
            return Err(failure("empty collaboration inbox projection fact"));
        }
        for value in fact.facts {
            if value.holding_id != self.holding_id || value.thread_id != fact.thread_id {
                return Err(failure("invalid collaboration inbox fact"));
            }
            let inserted = sqlx::query(
                "INSERT INTO erpx_collaboration_read.opaque_mention_inbox_projection \
                 (event_id,holding_id,projection_name,generation,recipient_principal_id,thread_id,mention_event_id,anchor_record_id,created_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
            )
            .bind(value.event_id)
            .bind(self.holding_id.as_uuid())
            .bind(INBOX_PROJECTION)
            .bind(context.generation)
            .bind(value.recipient_principal_id.as_uuid())
            .bind(value.thread_id.as_uuid())
            .bind(value.mention_event_id.as_uuid())
            .bind(value.anchor_record_id.as_uuid())
            .bind(value.created_at)
            .execute(&mut **tx)
            .await
            .map_err(map_failure)?
            .rows_affected();
            if inserted == 0 {
                let exact: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM erpx_collaboration_read.opaque_mention_inbox_projection \
                     WHERE event_id=$1 AND holding_id=$2 AND projection_name=$3 AND generation=$4 \
                     AND recipient_principal_id=$5 AND thread_id=$6 AND mention_event_id=$7 \
                     AND anchor_record_id=$8 AND created_at=$9)",
                )
                .bind(value.event_id)
                .bind(self.holding_id.as_uuid())
                .bind(INBOX_PROJECTION)
                .bind(context.generation)
                .bind(value.recipient_principal_id.as_uuid())
                .bind(value.thread_id.as_uuid())
                .bind(value.mention_event_id.as_uuid())
                .bind(value.anchor_record_id.as_uuid())
                .bind(value.created_at)
                .fetch_one(&mut **tx)
                .await
                .map_err(map_failure)?;
                if !exact {
                    return Err(failure("collaboration inbox identity collision"));
                }
            }
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
            self.holding_id,
            "opaque_mention_inbox_projection",
        )
        .await
    }
}

fn decode<T: for<'de> Deserialize<'de>>(
    event: &ProjectionEvent,
    expected_event: &str,
) -> Result<T, ProjectionFailure> {
    if event.context != CONTEXT_NAME
        || event.aggregate_type != AGGREGATE_TYPE
        || event.event_type != expected_event
        || event.event_version != EVENT_VERSION
    {
        return Err(failure("incompatible collaboration projection event"));
    }
    serde_json::from_value(event.payload.clone()).map_err(map_failure)
}

#[allow(clippy::too_many_arguments)]
fn validate_envelope(
    event: &ProjectionEvent,
    context: &ProjectorContext<'_>,
    projection: &str,
    expected_holding: HoldingId,
    fact_holding: HoldingId,
    thread_id: ThreadId,
    aggregate_version: u64,
    visibility_scope: &AuthorityScope,
) -> Result<(), ProjectionFailure> {
    if context.projection != projection
        || fact_holding != expected_holding
        || event.aggregate_id.as_uuid() != thread_id.as_uuid()
        || event.aggregate_version != aggregate_version
        || &event.active_scope != visibility_scope
        || event.partition != partition(thread_id)
    {
        return Err(failure("collaboration projection envelope mismatch"));
    }
    Ok(())
}

fn partition(thread_id: ThreadId) -> String {
    format!("collaboration.thread:{}", thread_id.as_uuid().simple())
}

async fn set_holding(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
) -> Result<(), ProjectionFailure> {
    let expected = holding_id.as_uuid().to_string();
    let actual: String = sqlx::query_scalar("SELECT set_config('app.holding_id', $1, true)")
        .bind(&expected)
        .fetch_one(&mut **tx)
        .await
        .map_err(map_failure)?;
    if actual != expected {
        return Err(failure("collaboration projection holding binding mismatch"));
    }
    Ok(())
}

async fn reset(
    tx: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    holding_id: HoldingId,
    table: &str,
) -> Result<(), ProjectionFailure> {
    set_holding(tx, holding_id).await?;
    let statement = format!(
        "DELETE FROM erpx_collaboration_read.{table} WHERE holding_id=$1 AND projection_name=$2 AND generation=$3"
    );
    sqlx::query(&statement)
        .bind(holding_id.as_uuid())
        .bind(context.projection)
        .bind(context.generation)
        .execute(&mut **tx)
        .await
        .map_err(map_failure)?;
    Ok(())
}

fn map_failure(error: impl std::fmt::Display) -> ProjectionFailure {
    failure(error.to_string())
}

fn failure(message: impl Into<String>) -> ProjectionFailure {
    ProjectionFailure::new(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holding_id() -> HoldingId {
        HoldingId::try_from_uuid(Uuid::from_u128(1)).unwrap()
    }

    #[test]
    fn all_three_projection_streams_are_typed_and_rebuildable() {
        assert_eq!(
            CollaborationThreadProjector::new(holding_id()).event_types(),
            &[THREAD_PROJECTION_FACT_EVENT.to_owned()]
        );
        assert_eq!(
            CollaborationStreamProjector::new(holding_id()).event_types(),
            &[STREAM_PROJECTION_FACT_EVENT.to_owned()]
        );
        assert_eq!(
            CollaborationInboxProjector::new(holding_id()).event_types(),
            &[INBOX_PROJECTION_FACT_EVENT.to_owned()]
        );
        let source = include_str!("projection.rs");
        assert!(source.matches("async fn reset(").count() >= 4);
        assert!(source.contains("source_event_cursor < EXCLUDED.source_event_cursor"));
        assert!(source.contains("collaboration stream cursor collision"));
        assert!(source.contains("collaboration inbox identity collision"));
    }

    #[test]
    fn write_transaction_appends_safe_projection_facts_before_receipt() {
        let write = include_str!("write.rs");
        let projection = write
            .find("append_projection_facts(")
            .expect("projection facts");
        let receipt = write.find("command_receipts").expect("receipt");
        assert!(projection < receipt);
        for forbidden in [
            concat!("encrypted", "_object_id"),
            concat!("evidence", "_digest"),
            concat!("cipher", "text"),
        ] {
            assert!(!include_str!("projection.rs").contains(forbidden));
        }
    }
}
