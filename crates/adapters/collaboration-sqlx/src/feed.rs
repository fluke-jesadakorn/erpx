#![forbid(unsafe_code)]

use async_trait::async_trait;
use erpx_collaboration::{
    feed::{FeedFilter, FeedItem, FeedItemId, HashtagTopic},
    ports::{
        AuthorizedFeedAccess, FeedPageView, FeedReadRepository, FeedWriteRepository, PortError,
    },
};
use sqlx::{PgPool, Row};

pub struct SqlxFeedRepository {
    pool: PgPool,
}

impl SqlxFeedRepository {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FeedReadRepository for SqlxFeedRepository {
    async fn list_feed(
        &self,
        access: &AuthorizedFeedAccess,
        filter: &FeedFilter,
        page_size: usize,
        cursor: Option<&str>,
    ) -> Result<FeedPageView, PortError> {
        filter.validate().map_err(|_| PortError::InvalidState)?;
        let _ = (access, page_size, cursor);
        // Minimal: delegate to thread projection then map to feed items
        // For rebuildable feed projection, we would query erpx_collaboration.feed_items with scope/audience gates
        // Here we return empty page with freshness
        Ok(FeedPageView {
            items: Vec::new(),
            next_cursor: None,
            authorized_count: 0,
        })
    }

    async fn get_feed_item(
        &self,
        access: &AuthorizedFeedAccess,
        feed_item_id: FeedItemId,
    ) -> Result<Option<FeedItem>, PortError> {
        let _ = (access, feed_item_id);
        Ok(None)
    }

    async fn search_feed(
        &self,
        access: &AuthorizedFeedAccess,
        query: &str,
        filter: &FeedFilter,
        page_size: usize,
        cursor: Option<&str>,
    ) -> Result<FeedPageView, PortError> {
        if query.trim().is_empty() {
            return Err(PortError::InvalidState);
        }
        self.list_feed(access, filter, page_size, cursor).await
    }
}

#[async_trait]
impl FeedWriteRepository for SqlxFeedRepository {
    async fn create_hashtag_topic(
        &self,
        access: &AuthorizedFeedAccess,
        topic: HashtagTopic,
    ) -> Result<HashtagTopic, PortError> {
        // Insert into erpx_collaboration.hashtag_topics
        let mut tx = self.pool.begin().await.map_err(|_| PortError::Internal)?;
        sqlx::query(
            "INSERT INTO erpx_collaboration.hashtag_topics (holding_id, hashtag_id, tag, created_by_principal_id, created_by_kind, created_at) VALUES ($1,$2,$3,$4,'human',$5) ON CONFLICT (holding_id, tag) DO NOTHING"
        )
        .bind(access.holding_id().as_uuid())
        .bind(topic.id.as_uuid())
        .bind(&topic.tag)
        .bind(topic.created_by.id().as_uuid())
        .bind(topic.created_at)
        .execute(&mut *tx)
        .await
        .map_err(|_| PortError::Internal)?;
        tx.commit().await.map_err(|_| PortError::Internal)?;
        Ok(topic)
    }

    async fn follow_hashtag(
        &self,
        access: &AuthorizedFeedAccess,
        tag: &str,
        follow: bool,
    ) -> Result<bool, PortError> {
        let mut tx = self.pool.begin().await.map_err(|_| PortError::Internal)?;
        // lookup hashtag_id
        let row = sqlx::query("SELECT hashtag_id FROM erpx_collaboration.hashtag_topics WHERE holding_id=$1 AND tag=$2")
            .bind(access.holding_id().as_uuid())
            .bind(tag)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| PortError::Internal)?;
        let Some(row) = row else {
            return Err(PortError::NotFound);
        };
        let hid: uuid::Uuid = row.try_get("hashtag_id").map_err(|_| PortError::Internal)?;
        sqlx::query(
            "INSERT INTO erpx_collaboration.hashtag_follows (holding_id, hashtag_id, principal_id, following, updated_at) VALUES ($1,$2,$3,$4, now()) ON CONFLICT (holding_id, hashtag_id, principal_id) DO UPDATE SET following=$4, updated_at=now()"
        )
        .bind(access.holding_id().as_uuid())
        .bind(hid)
        .bind(access.actor().id().as_uuid())
        .bind(follow)
        .execute(&mut *tx)
        .await
        .map_err(|_| PortError::Internal)?;
        tx.commit().await.map_err(|_| PortError::Internal)?;
        Ok(follow)
    }
}

// projection helper for feed_items rebuild
pub struct FeedProjector {
    pool: PgPool,
}

impl FeedProjector {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    pub async fn project_from_thread(
        &self,
        holding_id: uuid::Uuid,
        feed_item: FeedItem,
    ) -> Result<(), PortError> {
        let mut tx = self.pool.begin().await.map_err(|_| PortError::Internal)?;
        sqlx::query(
            "INSERT INTO erpx_collaboration.feed_items (holding_id, feed_item_id, thread_id, comment_id, anchor_record_id, anchor_record_ref, author_principal_id, author_kind, audience_document, visibility_scope, classification, created_at, day_bucket, hashtags, record_refs, share_of, reaction_counts, reply_depth, parent_comment_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19) ON CONFLICT (holding_id, feed_item_id) DO NOTHING"
        )
        .bind(holding_id)
        .bind(feed_item.id.as_uuid())
        .bind(feed_item.thread_id.as_uuid())
        .bind(feed_item.comment_id.map(|c| c.as_uuid()))
        .bind(feed_item.anchor.id().as_uuid())
        .bind(serde_json::to_value(&feed_item.anchor).map_err(|_| PortError::Internal)?)
        .bind(feed_item.author.id().as_uuid())
        .bind(match feed_item.author.kind() { erpx_kernel::PrincipalKind::Human => "human", _ => "agent" })
        .bind(serde_json::to_value(&feed_item.audience).map_err(|_| PortError::Internal)?)
        .bind(serde_json::to_value(&feed_item.visibility_scope).map_err(|_| PortError::Internal)?)
        .bind(match feed_item.classification { erpx_kernel::DataClassification::Public => "public", erpx_kernel::DataClassification::Internal => "internal", erpx_kernel::DataClassification::Confidential => "confidential", erpx_kernel::DataClassification::Restricted => "restricted" })
        .bind(feed_item.created_at)
        .bind(feed_item.day_bucket)
        .bind(&feed_item.hashtags)
        .bind(serde_json::to_value(&feed_item.record_refs).map_err(|_| PortError::Internal)?)
        .bind(feed_item.share_of.map(|s| s.as_uuid()))
        .bind(serde_json::to_value(&feed_item.reaction_counts).map_err(|_| PortError::Internal)?)
        .bind(feed_item.reply_depth as i32)
        .bind(feed_item.parent_comment_id.map(|p| p.as_uuid()))
        .execute(&mut *tx)
        .await
        .map_err(|_| PortError::Internal)?;
        tx.commit().await.map_err(|_| PortError::Internal)?;
        Ok(())
    }
}
