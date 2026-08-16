use async_trait::async_trait;
use erpx_collaboration::{
    AuthorizedSearchAccess, AuthorizedThreadAccess, CollaborationStreamEvent, OpaqueInboxFact,
    PortError, ThreadId, ThreadIdPage, ThreadProjection, ThreadReadRepository, ThreadSearchPage,
    ThreadSearchRequest,
};
use erpx_kernel::{PrincipalId, RecordId, TenantId};
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::{INBOX_PROJECTION, STREAM_PROJECTION, THREAD_PROJECTION};

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
        set_tenant(&mut tx, access.tenant_id()).await?;
        let generation =
            require_active_generation(&mut tx, access.tenant_id(), THREAD_PROJECTION).await?;
        let (actor_realm, actor_portal) = audience_realm(access.external_portal());
        let row = sqlx::query(
            "SELECT anchor_record_id, projection FROM erpx_collaboration_read.thread_projection \
             WHERE tenant_id=$1 AND projection_name=$2 AND generation=$3 AND thread_id=$4 \
             AND erpx_collaboration_read.scope_contains($5, visibility_scope) \
             AND erpx_collaboration_read.audience_allows(audience, $6, $5, $7, $8)",
        )
        .bind(access.tenant_id().as_uuid())
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
        set_tenant(&mut tx, access.tenant_id()).await?;
        let generation =
            require_active_generation(&mut tx, access.tenant_id(), THREAD_PROJECTION).await?;
        let (actor_realm, actor_portal) = audience_realm(access.external_portal());
        // Audience and hierarchy authorization are intentionally evaluated
        // before the full-text predicate in both source and SQL order.
        let rows = sqlx::query(
            "SELECT projection, count(*) OVER() AS authorized_count \
             FROM erpx_collaboration_read.thread_projection \
             WHERE tenant_id=$1 AND projection_name=$2 AND generation=$3 \
             AND erpx_collaboration_read.scope_contains($4, visibility_scope) \
             AND erpx_collaboration_read.audience_allows(audience, $5, $4, $6, $7) \
             AND search_vector @@ websearch_to_tsquery('simple', $8) \
             ORDER BY updated_at DESC, thread_id \
             LIMIT $9 OFFSET $10",
        )
        .bind(access.tenant_id().as_uuid())
        .bind(THREAD_PROJECTION)
        .bind(generation)
        .bind(serde_json::to_value(access.authority_scope()).map_err(|_| PortError::InvalidState)?)
        .bind(access.actor().id().as_uuid())
        .bind(actor_realm)
        .bind(actor_portal)
        .bind(&request.query)
        .bind(i64::from(request.page_size))
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
        let items = rows
            .into_iter()
            .map(|row| {
                let value: serde_json::Value =
                    row.try_get("projection").map_err(|_| PortError::Internal)?;
                serde_json::from_value(value).map_err(|_| PortError::Internal)
            })
            .collect::<Result<Vec<ThreadProjection>, PortError>>()?;
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
        set_tenant(&mut tx, access.tenant_id()).await?;
        let generation =
            require_active_generation(&mut tx, access.tenant_id(), THREAD_PROJECTION).await?;
        let (actor_realm, actor_portal) = audience_realm(access.external_portal());
        // Only opaque IDs cross this candidate query. Comment projections are
        // loaded later, after the application authorizes each exact thread.
        let rows = sqlx::query(
            "SELECT thread_id, count(*) OVER() AS authorized_count \
             FROM erpx_collaboration_read.thread_projection \
             WHERE tenant_id=$1 AND projection_name=$2 AND generation=$3 AND anchor_record_id=$4 \
             AND erpx_collaboration_read.scope_contains($5, visibility_scope) \
             AND erpx_collaboration_read.audience_allows(audience, $6, $5, $7, $8) \
             ORDER BY updated_at DESC, thread_id LIMIT $9 OFFSET $10",
        )
        .bind(access.tenant_id().as_uuid())
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
        set_tenant(&mut tx, access.tenant_id()).await?;
        let generation =
            require_active_generation(&mut tx, access.tenant_id(), STREAM_PROJECTION).await?;
        let rows = sqlx::query(
            "SELECT event_cursor, aggregate_version, event_kind, occurred_at \
             FROM erpx_collaboration_read.stream_projection \
             WHERE tenant_id=$1 AND projection_name=$2 AND generation=$3 \
             AND thread_id=$4 AND event_cursor>$5 \
             ORDER BY event_cursor LIMIT $6",
        )
        .bind(access.tenant_id().as_uuid())
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
                    tenant_id: access.tenant_id(),
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
        tenant_id: TenantId,
        cursor: u64,
        limit: u16,
    ) -> Result<Vec<OpaqueInboxFact>, PortError> {
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let generation = require_active_generation(&mut tx, tenant_id, INBOX_PROJECTION).await?;
        let rows = sqlx::query(
            "SELECT inbox_cursor, event_id, thread_id, mention_event_id, anchor_record_id, created_at \
             FROM erpx_collaboration_read.opaque_mention_inbox_projection \
             WHERE tenant_id=$1 AND projection_name=$2 AND generation=$3 \
             AND recipient_principal_id=$4 AND inbox_cursor>$5 \
             ORDER BY inbox_cursor LIMIT $6",
        )
        .bind(tenant_id.as_uuid())
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
                    tenant_id,
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

async fn set_tenant(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: TenantId,
) -> Result<(), PortError> {
    let expected = tenant_id.to_string();
    let bound = sqlx::query_scalar::<_, String>("SELECT set_config('app.tenant_id', $1, true)")
        .bind(tenant_id.to_string())
        .fetch_one(&mut **tx)
        .await
        .map_err(map_database_error)?;
    (bound == expected).then_some(()).ok_or(PortError::Denied)
}

async fn require_active_generation(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: TenantId,
    projection: &str,
) -> Result<uuid::Uuid, PortError> {
    sqlx::query_scalar(
        "SELECT generation FROM erpx_cqrs_read.projection_generations \
         WHERE tenant_id=$1 AND projection_name=$2 AND status='active'",
    )
    .bind(tenant_id.as_uuid())
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
