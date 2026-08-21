use async_trait::async_trait;
use erpx_collaboration::{
    AuthorizedSearchAccess, AuthorizedThreadAccess, CollaborationStreamEvent, OpaqueInboxFact,
    PortError, ThreadId, ThreadIdPage, ThreadProjection, ThreadReadRepository, ThreadSearchPage,
    ThreadSearchRequest,
};
use erpx_kernel::{HoldingId, PrincipalId, RecordId};
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::{DIRECTORY_FRESHNESS_SECONDS, INBOX_PROJECTION, STREAM_PROJECTION, THREAD_PROJECTION};

#[derive(Debug, Clone)]
pub struct SqlxThreadReadRepository {
    pool: PgPool,
}

impl SqlxThreadReadRepository {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ThreadReadRepository for SqlxThreadReadRepository {
    async fn get(
        &self,
        access: &AuthorizedThreadAccess,
    ) -> Result<Option<ThreadProjection>, PortError> {
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, access.holding_id()).await?;
        let generation =
            require_active_generation(&mut tx, access.holding_id(), THREAD_PROJECTION).await?;
        let (actor_realm, actor_portal) = audience_realm(access.external_portal());
        let row = sqlx::query(
            "SELECT anchor_record_id, projection FROM erpx_collaboration_read.thread_projection \
             WHERE holding_id=$1 AND projection_name=$2 AND generation=$3 AND thread_id=$4 \
             AND erpx_collaboration_read.scope_contains($5, visibility_scope) \
             AND erpx_collaboration_read.audience_allows(audience, $6, $5, $7, $8)",
        )
        .bind(access.holding_id().as_uuid())
        .bind(THREAD_PROJECTION)
        .bind(generation)
        .bind(access.thread_id().as_uuid())
        .bind(serde_json::to_value(access.authority_scope()).map_err(|_| PortError::InvalidState)?)
        .bind(access.actor().id().as_uuid())
        .bind(actor_realm)
        .bind(actor_portal)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let anchor: uuid::Uuid = row
            .try_get("anchor_record_id")
            .map_err(|_| PortError::Internal)?;
        if anchor != access.anchor_record_id().as_uuid() {
            return Err(PortError::Internal);
        }
        let projection: serde_json::Value =
            row.try_get("projection").map_err(|_| PortError::Internal)?;
        serde_json::from_value(projection)
            .map(Some)
            .map_err(|_| PortError::Internal)
    }

    async fn search(
        &self,
        access: &AuthorizedSearchAccess,
        request: &ThreadSearchRequest,
    ) -> Result<ThreadSearchPage, PortError> {
        request.validate()?;
        let offset = request
            .cursor
            .as_deref()
            .map(str::parse::<i64>)
            .transpose()
            .map_err(|_| PortError::InvalidState)?
            .unwrap_or(0);
        if offset < 0 {
            return Err(PortError::InvalidState);
        }
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, access.holding_id()).await?;
        let generation =
            require_active_generation(&mut tx, access.holding_id(), THREAD_PROJECTION).await?;
        let (actor_realm, actor_portal) = audience_realm(access.external_portal());
        // The Item Directory freshness, exact anchor grant, hierarchy, and
        // audience gates form a materialized candidate set. Full-text
        // matching, payload loading, pagination, and counting can therefore
        // observe only authorized anchors. The outer freshness row is an
        // availability sentinel; an absent/stale directory never becomes an
        // apparently successful empty search.
        let rows = sqlx::query(
            "WITH directory_freshness AS MATERIALIZED (\
               SELECT COALESCE(\
                        state.state='ready' \
                        AND state.applied_position=state.source_high_watermark \
                        AND state.last_success_at IS NOT NULL \
                        AND state.last_success_at<=statement_timestamp() \
                        AND state.last_success_at>=statement_timestamp() \
                            - ($11::BIGINT * INTERVAL '1 second'), \
                        FALSE\
                      ) AS is_fresh, \
                      COALESCE(state.applied_position, -1) AS applied_position \
               FROM (VALUES (TRUE)) AS required(singleton) \
               LEFT JOIN erpx_collaboration_read.item_directory_projection_state state \
                 ON state.holding_id=$1\
             ), authorized AS MATERIALIZED (\
               SELECT thread.thread_id, thread.updated_at, thread.search_vector \
               FROM directory_freshness freshness \
               JOIN erpx_collaboration_read.thread_projection thread \
                 ON freshness.is_fresh \
                AND thread.holding_id=$1 \
                AND thread.projection_name=$2 \
                AND thread.generation=$3 \
               JOIN erpx_collaboration_read.item_directory_descriptors descriptor \
                 ON descriptor.holding_id=thread.holding_id \
                AND descriptor.record_id=thread.anchor_record_id \
               JOIN erpx_collaboration_read.item_directory_effective_grants grant_fact \
                 ON grant_fact.holding_id=descriptor.holding_id \
                AND grant_fact.record_id=descriptor.record_id \
                AND grant_fact.principal_id=$5 \
                AND grant_fact.authorization_policy_id=descriptor.authorization_policy_id \
                AND grant_fact.policy_version=descriptor.authorization_policy_version \
                AND grant_fact.source_event_position>=descriptor.source_event_position \
               WHERE grant_fact.can_view \
                 AND grant_fact.effective_at<=statement_timestamp() \
                 AND grant_fact.expires_at>statement_timestamp() \
                 AND descriptor.tombstoned_at IS NULL \
                 AND descriptor.source_event_position<=freshness.applied_position \
                 AND grant_fact.source_event_position<=freshness.applied_position \
                 AND erpx_collaboration_read.scope_contains($4, descriptor.record_scope) \
                 AND erpx_collaboration_read.scope_contains($4, thread.visibility_scope) \
                 AND erpx_collaboration_read.audience_allows(\
                       thread.audience, $5, $4, $6, $7\
                     )\
             ), matched AS MATERIALIZED (\
               SELECT thread_id, updated_at \
               FROM authorized \
               WHERE search_vector @@ websearch_to_tsquery('simple', $8)\
             ), matched_count AS MATERIALIZED (\
               SELECT count(*)::BIGINT AS authorized_count FROM matched\
             ), paged AS MATERIALIZED (\
               SELECT thread_id, updated_at \
               FROM matched \
               ORDER BY updated_at DESC, thread_id \
               LIMIT $9 OFFSET $10\
             ) \
             SELECT freshness.is_fresh, matched_count.authorized_count, \
                    paged.thread_id AS matched_thread_id, payload.projection \
             FROM directory_freshness freshness \
             CROSS JOIN matched_count \
             LEFT JOIN paged ON freshness.is_fresh \
             LEFT JOIN erpx_collaboration_read.thread_projection payload \
               ON payload.holding_id=$1 \
              AND payload.projection_name=$2 \
              AND payload.generation=$3 \
              AND payload.thread_id=paged.thread_id \
             ORDER BY paged.updated_at DESC NULLS LAST, paged.thread_id NULLS LAST",
        )
        .bind(access.holding_id().as_uuid())
        .bind(THREAD_PROJECTION)
        .bind(generation)
        .bind(serde_json::to_value(access.authority_scope()).map_err(|_| PortError::InvalidState)?)
        .bind(access.actor().id().as_uuid())
        .bind(actor_realm)
        .bind(actor_portal)
        .bind(&request.query)
        .bind(i64::from(request.page_size))
        .bind(offset)
        .bind(DIRECTORY_FRESHNESS_SECONDS)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        let first = rows.first().ok_or(PortError::Unavailable)?;
        if !first
            .try_get::<bool, _>("is_fresh")
            .map_err(|_| PortError::Internal)?
        {
            return Err(PortError::Unavailable);
        }
        let authorized_count = u64::try_from(
            first
                .try_get::<i64, _>("authorized_count")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let thread_id: Option<uuid::Uuid> = row
                .try_get("matched_thread_id")
                .map_err(|_| PortError::Internal)?;
            let value: Option<serde_json::Value> =
                row.try_get("projection").map_err(|_| PortError::Internal)?;
            match (thread_id, value) {
                (None, None) => {}
                (Some(thread_id), Some(value)) => {
                    let projection = serde_json::from_value::<ThreadProjection>(value)
                        .map_err(|_| PortError::Internal)?;
                    if projection.id.as_uuid() != thread_id {
                        return Err(PortError::Internal);
                    }
                    items.push(projection);
                }
                _ => return Err(PortError::Internal),
            }
        }
        if u64::try_from(items.len()).map_err(|_| PortError::Internal)? > authorized_count {
            return Err(PortError::Internal);
        }
        let consumed = offset
            .checked_add(i64::try_from(items.len()).map_err(|_| PortError::Internal)?)
            .ok_or(PortError::Internal)?;
        let next_cursor = (u64::try_from(consumed).map_err(|_| PortError::Internal)?
            < authorized_count)
            .then(|| consumed.to_string());
        Ok(ThreadSearchPage {
            items,
            next_cursor,
            authorized_count,
        })
    }

    async fn list_ids_for_record(
        &self,
        access: &AuthorizedSearchAccess,
        record_id: RecordId,
        page_size: u16,
        cursor: Option<&str>,
    ) -> Result<ThreadIdPage, PortError> {
        if page_size == 0 || page_size > 100 {
            return Err(PortError::InvalidState);
        }
        let offset = cursor
            .map(str::parse::<i64>)
            .transpose()
            .map_err(|_| PortError::InvalidState)?
            .unwrap_or(0);
        if offset < 0 {
            return Err(PortError::InvalidState);
        }
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, access.holding_id()).await?;
        let generation =
            require_active_generation(&mut tx, access.holding_id(), THREAD_PROJECTION).await?;
        let (actor_realm, actor_portal) = audience_realm(access.external_portal());
        // Only opaque IDs cross this candidate query. Comment projections are
        // loaded later, after the application authorizes each exact thread.
        let rows = sqlx::query(
            "SELECT thread_id, count(*) OVER() AS authorized_count \
             FROM erpx_collaboration_read.thread_projection \
             WHERE holding_id=$1 AND projection_name=$2 AND generation=$3 AND anchor_record_id=$4 \
             AND erpx_collaboration_read.scope_contains($5, visibility_scope) \
             AND erpx_collaboration_read.audience_allows(audience, $6, $5, $7, $8) \
             ORDER BY updated_at DESC, thread_id LIMIT $9 OFFSET $10",
        )
        .bind(access.holding_id().as_uuid())
        .bind(THREAD_PROJECTION)
        .bind(generation)
        .bind(record_id.as_uuid())
        .bind(serde_json::to_value(access.authority_scope()).map_err(|_| PortError::InvalidState)?)
        .bind(access.actor().id().as_uuid())
        .bind(actor_realm)
        .bind(actor_portal)
        .bind(i64::from(page_size))
        .bind(offset)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        let authorized_count = rows
            .first()
            .map(|row| row.try_get::<i64, _>("authorized_count"))
            .transpose()
            .map_err(|_| PortError::Internal)?
            .map(u64::try_from)
            .transpose()
            .map_err(|_| PortError::Internal)?
            .unwrap_or(0);
        let thread_ids = rows
            .into_iter()
            .map(|row| {
                ThreadId::try_from_uuid(row.try_get("thread_id").map_err(|_| PortError::Internal)?)
                    .map_err(|_| PortError::Internal)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let consumed = offset
            .checked_add(i64::try_from(thread_ids.len()).map_err(|_| PortError::Internal)?)
            .ok_or(PortError::Internal)?;
        let next_cursor = (u64::try_from(consumed).map_err(|_| PortError::Internal)?
            < authorized_count)
            .then(|| consumed.to_string());
        Ok(ThreadIdPage {
            thread_ids,
            next_cursor,
            authorized_count,
        })
    }

    async fn stream_after(
        &self,
        access: &AuthorizedThreadAccess,
        cursor: u64,
        limit: u16,
    ) -> Result<Vec<CollaborationStreamEvent>, PortError> {
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, access.holding_id()).await?;
        let generation =
            require_active_generation(&mut tx, access.holding_id(), STREAM_PROJECTION).await?;
        let rows = sqlx::query(
            "SELECT event_cursor, aggregate_version, event_kind, occurred_at \
             FROM erpx_collaboration_read.stream_projection \
             WHERE holding_id=$1 AND projection_name=$2 AND generation=$3 \
             AND thread_id=$4 AND event_cursor>$5 \
             ORDER BY event_cursor LIMIT $6",
        )
        .bind(access.holding_id().as_uuid())
        .bind(STREAM_PROJECTION)
        .bind(generation)
        .bind(access.thread_id().as_uuid())
        .bind(i64::try_from(cursor).map_err(|_| PortError::InvalidState)?)
        .bind(i64::from(limit))
        .fetch_all(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        rows.into_iter()
            .map(|row| {
                let kind: serde_json::Value =
                    row.try_get("event_kind").map_err(|_| PortError::Internal)?;
                Ok(CollaborationStreamEvent {
                    cursor: u64::try_from(
                        row.try_get::<i64, _>("event_cursor")
                            .map_err(|_| PortError::Internal)?,
                    )
                    .map_err(|_| PortError::Internal)?,
                    holding_id: access.holding_id(),
                    thread_id: access.thread_id(),
                    aggregate_version: u64::try_from(
                        row.try_get::<i64, _>("aggregate_version")
                            .map_err(|_| PortError::Internal)?,
                    )
                    .map_err(|_| PortError::Internal)?,
                    kind: serde_json::from_value(kind).map_err(|_| PortError::Internal)?,
                    occurred_at: row
                        .try_get("occurred_at")
                        .map_err(|_| PortError::Internal)?,
                })
            })
            .collect()
    }

    async fn inbox_after(
        &self,
        actor: PrincipalId,
        holding_id: HoldingId,
        cursor: u64,
        limit: u16,
    ) -> Result<Vec<OpaqueInboxFact>, PortError> {
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, holding_id).await?;
        let generation = require_active_generation(&mut tx, holding_id, INBOX_PROJECTION).await?;
        let rows = sqlx::query(
            "SELECT inbox_cursor, event_id, thread_id, mention_event_id, anchor_record_id, created_at \
             FROM erpx_collaboration_read.opaque_mention_inbox_projection \
             WHERE holding_id=$1 AND projection_name=$2 AND generation=$3 \
             AND recipient_principal_id=$4 AND inbox_cursor>$5 \
             ORDER BY inbox_cursor LIMIT $6",
        )
        .bind(holding_id.as_uuid())
        .bind(INBOX_PROJECTION)
        .bind(generation)
        .bind(actor.as_uuid())
        .bind(i64::try_from(cursor).map_err(|_| PortError::InvalidState)?)
        .bind(i64::from(limit))
        .fetch_all(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        rows.into_iter()
            .map(|row| {
                Ok(OpaqueInboxFact {
                    event_id: row.try_get("event_id").map_err(|_| PortError::Internal)?,
                    holding_id,
                    thread_id: erpx_collaboration::ThreadId::try_from_uuid(
                        row.try_get("thread_id").map_err(|_| PortError::Internal)?,
                    )
                    .map_err(|_| PortError::Internal)?,
                    mention_event_id: erpx_collaboration::MentionEventId::try_from_uuid(
                        row.try_get("mention_event_id")
                            .map_err(|_| PortError::Internal)?,
                    )
                    .map_err(|_| PortError::Internal)?,
                    recipient_principal_id: actor,
                    anchor_record_id: RecordId::try_from_uuid(
                        row.try_get("anchor_record_id")
                            .map_err(|_| PortError::Internal)?,
                    )
                    .map_err(|_| PortError::Internal)?,
                    created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
                })
            })
            .collect()
    }
}

const fn audience_realm(
    portal: Option<erpx_collaboration::ExternalPortal>,
) -> (&'static str, Option<&'static str>) {
    match portal {
        None => ("internal", None),
        Some(erpx_collaboration::ExternalPortal::Customer) => ("external_portal", Some("customer")),
        Some(erpx_collaboration::ExternalPortal::Supplier) => ("external_portal", Some("supplier")),
        Some(erpx_collaboration::ExternalPortal::Candidate) => {
            ("external_portal", Some("candidate"))
        }
        Some(erpx_collaboration::ExternalPortal::Commerce) => ("external_portal", Some("commerce")),
        Some(erpx_collaboration::ExternalPortal::Signatory) => {
            ("external_portal", Some("signatory"))
        }
    }
}

async fn set_holding(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
) -> Result<(), PortError> {
    let expected = holding_id.to_string();
    let bound = sqlx::query_scalar::<_, String>("SELECT set_config('app.holding_id', $1, true)")
        .bind(holding_id.to_string())
        .fetch_one(&mut **tx)
        .await
        .map_err(map_database_error)?;
    (bound == expected).then_some(()).ok_or(PortError::Denied)
}

async fn require_active_generation(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    projection: &str,
) -> Result<uuid::Uuid, PortError> {
    sqlx::query_scalar(
        "SELECT generation FROM erpx_cqrs_read.projection_generations \
         WHERE holding_id=$1 AND projection_name=$2 AND status='active'",
    )
    .bind(holding_id.as_uuid())
    .bind(projection)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_database_error)?
    .ok_or(PortError::Stale)
}

fn map_database_error(value: sqlx::Error) -> PortError {
    match value.as_database_error().and_then(|error| error.code()) {
        Some(code) if code == "40001" => PortError::Stale,
        _ => PortError::Unavailable,
    }
}
