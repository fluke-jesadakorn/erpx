//! Tenant-local SQLx adapter for the disposable federated Item Directory.
//!
//! The adapter accepts only capabilities created after current identity/role
//! authorization. Effective record grants are owner-published projection
//! facts; descriptor contents are never used to infer authority.

use std::{error::Error, fmt};

use chrono::{DateTime, Duration, Utc};
use erpx_integration_contracts::{
    CanonicalRouteMoved, EffectiveGrantChanged, EffectiveGrantRevoked, ItemDescriptor,
    ItemDescriptorChanged, ItemDirectoryFact, ItemTombstoned, ReferenceRelationChanged,
    ReferenceRelationRevoked, StableRecordRef, TenantItemDirectoryFact,
};
use erpx_kernel::{PolicyId, PrincipalId, RecordId, TenantId, TenantScope};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

pub const DIRECTORY_FRESHNESS_SECONDS: i64 = 60;
pub const MAX_DIRECTORY_SEARCH_BYTES: usize = 256;
pub const MAX_DIRECTORY_INDEX_TEXT_BYTES: usize = 4_096;
pub const MAX_DIRECTORY_PAGE_SIZE: u16 = 100;
pub const CORE_RELATION_TYPES: [&str; 7] = [
    "relates-to",
    "blocks",
    "depends-on",
    "implements",
    "supersedes",
    "derived-from",
    "evidence-for",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectoryAdapterError {
    Invalid(&'static str),
    Unavailable,
}

impl fmt::Display for DirectoryAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(field) => write!(formatter, "invalid Item Directory {field}"),
            Self::Unavailable => formatter.write_str("Item Directory is unavailable"),
        }
    }
}

impl Error for DirectoryAdapterError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionFreshness {
    Fresh,
    Stale,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectoryProjectionCheckpoint {
    pub generation: u64,
    pub source_high_watermark: u64,
    pub applied_position: u64,
    pub rebuilding: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectoryLookup<T> {
    Visible {
        value: T,
        freshness: ProjectionFreshness,
    },
    /// Exact same outcome for an absent record and a denied record.
    Neutral,
    /// No protected payload is returned while the projection is stale.
    Degraded(ProjectionFreshness),
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryAccess {
    tenant_id: TenantId,
    principal_id: PrincipalId,
    authority_scope: TenantScope,
}

impl DirectoryAccess {
    /// Must be called only after the server has authenticated the principal and
    /// authorized the Item Directory read capability at this exact scope.
    pub fn try_from_authorization(
        tenant_id: TenantId,
        principal_id: PrincipalId,
        authority_scope: TenantScope,
    ) -> Result<Self, DirectoryAdapterError> {
        authority_scope
            .validate()
            .map_err(|_| DirectoryAdapterError::Invalid("authority_scope"))?;
        Ok(Self {
            tenant_id,
            principal_id,
            authority_scope,
        })
    }

    pub const fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }

    pub const fn principal_id(&self) -> PrincipalId {
        self.principal_id
    }

    pub const fn authority_scope(&self) -> &TenantScope {
        &self.authority_scope
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedDirectoryRecord {
    access: DirectoryAccess,
    record_id: RecordId,
}

impl AuthorizedDirectoryRecord {
    pub const fn access(&self) -> &DirectoryAccess {
        &self.access
    }

    pub const fn record_id(&self) -> RecordId {
        self.record_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectorySearchRequest {
    pub query: String,
    pub page_size: u16,
    pub cursor: Option<String>,
}

impl DirectorySearchRequest {
    pub fn validate(&self) -> Result<(), DirectoryAdapterError> {
        validate_search_text(&self.query, MAX_DIRECTORY_SEARCH_BYTES, "search query")?;
        if self.page_size == 0 || self.page_size > MAX_DIRECTORY_PAGE_SIZE {
            return Err(DirectoryAdapterError::Invalid("page_size"));
        }
        parse_cursor(self.cursor.as_deref())?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectorySearchPage {
    pub items: Vec<ItemDescriptor>,
    pub next_cursor: Option<String>,
    /// Count is computed only after current grant and scope filters.
    pub authorized_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryBacklinks {
    pub incoming: Vec<StableRecordRef>,
    pub outgoing: Vec<StableRecordRef>,
    pub next_cursor: Option<String>,
    /// Never includes a denied or tombstoned linked record.
    pub authorized_count: u64,
}

#[derive(Debug, Clone)]
pub struct SqlxItemDirectory {
    pool: PgPool,
}

impl SqlxItemDirectory {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Checks an opaque effective grant and hierarchy scope before any
    /// descriptor, search text, relation, or count is selected.
    pub async fn authorize_record(
        &self,
        access: &DirectoryAccess,
        record_id: RecordId,
    ) -> Result<Option<AuthorizedDirectoryRecord>, DirectoryAdapterError> {
        if self.freshness(access.tenant_id(), Utc::now()).await? != ProjectionFreshness::Fresh {
            return Err(DirectoryAdapterError::Unavailable);
        }
        let mut tx = self.begin(access.tenant_id()).await?;
        let authorized = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (\
               SELECT 1 \
               FROM erpx_collaboration_read.item_directory_effective_grants grant_fact \
               JOIN erpx_collaboration_read.item_directory_descriptors descriptor \
                 ON descriptor.tenant_id=grant_fact.tenant_id \
                AND descriptor.record_id=grant_fact.record_id \
                AND descriptor.authorization_policy_id=grant_fact.authorization_policy_id \
                AND descriptor.authorization_policy_version=grant_fact.policy_version \
                AND grant_fact.source_event_position>=descriptor.source_event_position \
               WHERE grant_fact.tenant_id=$1 AND grant_fact.record_id=$2 \
                 AND grant_fact.principal_id=$3 AND grant_fact.can_view \
                 AND grant_fact.effective_at<=statement_timestamp() \
                 AND grant_fact.expires_at>statement_timestamp() \
                 AND descriptor.tombstoned_at IS NULL \
                 AND erpx_collaboration_read.scope_contains($4, descriptor.record_scope)\
             )",
        )
        .bind(access.tenant_id().as_uuid())
        .bind(record_id.as_uuid())
        .bind(access.principal_id().as_uuid())
        .bind(scope_json(access.authority_scope())?)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        Ok(authorized.then(|| AuthorizedDirectoryRecord {
            access: access.clone(),
            record_id,
        }))
    }

    pub async fn resolve(
        &self,
        access: &DirectoryAccess,
        record_id: RecordId,
        now: DateTime<Utc>,
    ) -> Result<DirectoryLookup<ItemDescriptor>, DirectoryAdapterError> {
        let freshness = self.freshness(access.tenant_id(), now).await?;
        if freshness == ProjectionFreshness::Unavailable {
            return Ok(DirectoryLookup::Unavailable);
        }
        if freshness == ProjectionFreshness::Stale {
            return Ok(DirectoryLookup::Degraded(freshness));
        }
        let Some(authorized) = self.authorize_record(access, record_id).await? else {
            return Ok(DirectoryLookup::Neutral);
        };
        let mut tx = self.begin(access.tenant_id()).await?;
        let value = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT descriptor FROM erpx_collaboration_read.item_directory_descriptors \
             WHERE tenant_id=$1 AND record_id=$2 AND tombstoned_at IS NULL",
        )
        .bind(access.tenant_id().as_uuid())
        .bind(authorized.record_id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        let Some(value) = value else {
            return Ok(DirectoryLookup::Neutral);
        };
        let descriptor = serde_json::from_value::<ItemDescriptor>(value)
            .map_err(|_| DirectoryAdapterError::Unavailable)?;
        descriptor
            .validate()
            .map_err(|_| DirectoryAdapterError::Unavailable)?;
        if descriptor.record().id() != record_id {
            return Ok(DirectoryLookup::Unavailable);
        }
        Ok(DirectoryLookup::Visible {
            value: descriptor,
            freshness,
        })
    }

    pub async fn search(
        &self,
        access: &DirectoryAccess,
        request: &DirectorySearchRequest,
        now: DateTime<Utc>,
    ) -> Result<DirectoryLookup<DirectorySearchPage>, DirectoryAdapterError> {
        request.validate()?;
        let freshness = self.freshness(access.tenant_id(), now).await?;
        if freshness == ProjectionFreshness::Unavailable {
            return Ok(DirectoryLookup::Unavailable);
        }
        if freshness == ProjectionFreshness::Stale {
            return Ok(DirectoryLookup::Degraded(freshness));
        }
        let offset = parse_cursor(request.cursor.as_deref())?;
        let mut tx = self.begin(access.tenant_id()).await?;
        // The grant join and scope predicate deliberately precede the FTS
        // predicate in source and form the only rows eligible for matching.
        let rows = sqlx::query(
            "WITH authorized AS (\
               SELECT descriptor.* \
               FROM erpx_collaboration_read.item_directory_effective_grants grant_fact \
               JOIN erpx_collaboration_read.item_directory_descriptors descriptor \
                 ON descriptor.tenant_id=grant_fact.tenant_id \
                AND descriptor.record_id=grant_fact.record_id \
                AND descriptor.authorization_policy_id=grant_fact.authorization_policy_id \
                AND descriptor.authorization_policy_version=grant_fact.policy_version \
                AND grant_fact.source_event_position>=descriptor.source_event_position \
               WHERE grant_fact.tenant_id=$1 AND grant_fact.principal_id=$2 \
                 AND grant_fact.can_view AND descriptor.tombstoned_at IS NULL \
                 AND grant_fact.effective_at<=$7 AND grant_fact.expires_at>$7 \
                 AND erpx_collaboration_read.scope_contains($3, descriptor.record_scope)\
             ) \
             SELECT descriptor, count(*) OVER() AS authorized_count \
             FROM authorized \
             WHERE search_vector @@ websearch_to_tsquery('simple', $4) \
             ORDER BY source_event_position DESC, record_id \
             LIMIT $5 OFFSET $6",
        )
        .bind(access.tenant_id().as_uuid())
        .bind(access.principal_id().as_uuid())
        .bind(scope_json(access.authority_scope())?)
        .bind(&request.query)
        .bind(i64::from(request.page_size))
        .bind(offset)
        .bind(now)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        let authorized_count = authorized_count(&rows)?;
        let items = rows
            .iter()
            .map(|row| {
                let value: serde_json::Value = row
                    .try_get("descriptor")
                    .map_err(|_| DirectoryAdapterError::Unavailable)?;
                let descriptor = serde_json::from_value::<ItemDescriptor>(value)
                    .map_err(|_| DirectoryAdapterError::Unavailable)?;
                descriptor
                    .validate()
                    .map_err(|_| DirectoryAdapterError::Unavailable)?;
                Ok(descriptor)
            })
            .collect::<Result<Vec<_>, DirectoryAdapterError>>()?;
        let consumed = offset
            .checked_add(
                i64::try_from(items.len()).map_err(|_| DirectoryAdapterError::Unavailable)?,
            )
            .ok_or(DirectoryAdapterError::Unavailable)?;
        let next_cursor = (u64::try_from(consumed)
            .map_err(|_| DirectoryAdapterError::Unavailable)?
            < authorized_count)
            .then(|| consumed.to_string());
        Ok(DirectoryLookup::Visible {
            value: DirectorySearchPage {
                items,
                next_cursor,
                authorized_count,
            },
            freshness,
        })
    }

    pub async fn backlinks(
        &self,
        access: &DirectoryAccess,
        record_id: RecordId,
        page_size: u16,
        cursor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<DirectoryLookup<DirectoryBacklinks>, DirectoryAdapterError> {
        if page_size == 0 || page_size > MAX_DIRECTORY_PAGE_SIZE {
            return Err(DirectoryAdapterError::Invalid("page_size"));
        }
        let offset = parse_cursor(cursor)?;
        let freshness = self.freshness(access.tenant_id(), now).await?;
        if freshness == ProjectionFreshness::Unavailable {
            return Ok(DirectoryLookup::Unavailable);
        }
        if freshness == ProjectionFreshness::Stale {
            return Ok(DirectoryLookup::Degraded(freshness));
        }
        if self.authorize_record(access, record_id).await?.is_none() {
            return Ok(DirectoryLookup::Neutral);
        }
        let mut tx = self.begin(access.tenant_id()).await?;
        let rows = sqlx::query(
            "WITH linked AS (\
               SELECT relation.relation_id, relation.source_record_id, relation.target_record_id, \
                      CASE WHEN relation.source_record_id=$2 \
                           THEN relation.target_record_id ELSE relation.source_record_id END AS linked_id, \
                      relation.target_record_id=$2 AS incoming \
               FROM erpx_collaboration_read.item_directory_relations relation \
               WHERE relation.tenant_id=$1 AND relation.tombstoned_at IS NULL \
                 AND (relation.source_record_id=$2 OR relation.target_record_id=$2)\
             ), authorized AS (\
               SELECT linked.*, descriptor.descriptor \
               FROM linked \
               JOIN erpx_collaboration_read.item_directory_effective_grants grant_fact \
                ON grant_fact.tenant_id=$1 AND grant_fact.record_id=linked.linked_id \
                AND grant_fact.principal_id=$3 AND grant_fact.can_view \
                AND grant_fact.effective_at<=$7 AND grant_fact.expires_at>$7 \
               JOIN erpx_collaboration_read.item_directory_descriptors descriptor \
                 ON descriptor.tenant_id=grant_fact.tenant_id \
                AND descriptor.record_id=grant_fact.record_id \
                AND descriptor.authorization_policy_id=grant_fact.authorization_policy_id \
                AND descriptor.authorization_policy_version=grant_fact.policy_version \
                AND grant_fact.source_event_position>=descriptor.source_event_position \
               WHERE descriptor.tombstoned_at IS NULL \
                 AND erpx_collaboration_read.scope_contains($4, descriptor.record_scope)\
             ) \
             SELECT descriptor, incoming, count(*) OVER() AS authorized_count \
             FROM authorized ORDER BY relation_id LIMIT $5 OFFSET $6",
        )
        .bind(access.tenant_id().as_uuid())
        .bind(record_id.as_uuid())
        .bind(access.principal_id().as_uuid())
        .bind(scope_json(access.authority_scope())?)
        .bind(i64::from(page_size))
        .bind(offset)
        .bind(now)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        let authorized_count = authorized_count(&rows)?;
        let mut incoming = Vec::new();
        let mut outgoing = Vec::new();
        for row in &rows {
            let value: serde_json::Value = row
                .try_get("descriptor")
                .map_err(|_| DirectoryAdapterError::Unavailable)?;
            let descriptor = serde_json::from_value::<ItemDescriptor>(value)
                .map_err(|_| DirectoryAdapterError::Unavailable)?;
            descriptor
                .validate()
                .map_err(|_| DirectoryAdapterError::Unavailable)?;
            if row
                .try_get::<bool, _>("incoming")
                .map_err(|_| DirectoryAdapterError::Unavailable)?
            {
                incoming.push(descriptor.record().clone());
            } else {
                outgoing.push(descriptor.record().clone());
            }
        }
        let consumed = offset
            .checked_add(i64::try_from(rows.len()).map_err(|_| DirectoryAdapterError::Unavailable)?)
            .ok_or(DirectoryAdapterError::Unavailable)?;
        let next_cursor = (u64::try_from(consumed)
            .map_err(|_| DirectoryAdapterError::Unavailable)?
            < authorized_count)
            .then(|| consumed.to_string());
        Ok(DirectoryLookup::Visible {
            value: DirectoryBacklinks {
                incoming,
                outgoing,
                next_cursor,
                authorized_count,
            },
            freshness,
        })
    }

    pub async fn freshness(
        &self,
        tenant_id: TenantId,
        now: DateTime<Utc>,
    ) -> Result<ProjectionFreshness, DirectoryAdapterError> {
        let mut tx = self.begin(tenant_id).await?;
        let row = sqlx::query(
            "SELECT state, source_high_watermark, applied_position, last_success_at \
             FROM erpx_collaboration_read.item_directory_projection_state \
             WHERE tenant_id=$1",
        )
        .bind(tenant_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        let Some(row) = row else {
            return Ok(ProjectionFreshness::Unavailable);
        };
        projection_freshness(
            row.try_get("state")
                .map_err(|_| DirectoryAdapterError::Unavailable)?,
            row.try_get("source_high_watermark")
                .map_err(|_| DirectoryAdapterError::Unavailable)?,
            row.try_get("applied_position")
                .map_err(|_| DirectoryAdapterError::Unavailable)?,
            row.try_get("last_success_at")
                .map_err(|_| DirectoryAdapterError::Unavailable)?,
            now,
        )
    }

    async fn begin(
        &self,
        tenant_id: TenantId,
    ) -> Result<Transaction<'_, Postgres>, DirectoryAdapterError> {
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        Ok(tx)
    }
}

#[derive(Debug, Clone)]
pub struct ItemDirectoryProjectionAdapter {
    pool: PgPool,
}

impl ItemDirectoryProjectionAdapter {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Applies only a validated, owner-emitted typed fact. No descriptor,
    /// route, grant, or relation is inferred from an arbitrary event payload.
    pub async fn apply_fact(
        &self,
        publication: &TenantItemDirectoryFact,
        projected_at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        publication
            .validate()
            .map_err(|_| DirectoryAdapterError::Invalid("typed owner fact"))?;
        match &publication.fact {
            ItemDirectoryFact::DescriptorChanged(value) => {
                self.apply_descriptor_changed(publication.tenant_id, value, projected_at)
                    .await
            }
            ItemDirectoryFact::EffectiveGrantChanged(value) => {
                self.apply_effective_grant(publication.tenant_id, value, projected_at)
                    .await
            }
            ItemDirectoryFact::EffectiveGrantRevoked(value) => {
                self.revoke_effective_grant(publication.tenant_id, value, projected_at)
                    .await
            }
            ItemDirectoryFact::RelationChanged(value) => {
                self.apply_reference_relation(publication.tenant_id, value, projected_at)
                    .await
            }
            ItemDirectoryFact::RelationRevoked(value) => {
                self.revoke_reference_relation(publication.tenant_id, value, projected_at)
                    .await
            }
            ItemDirectoryFact::RouteMoved(value) => {
                self.apply_route_move(publication.tenant_id, value, projected_at)
                    .await
            }
            ItemDirectoryFact::Tombstoned(value) => {
                self.apply_item_tombstone(publication.tenant_id, value, projected_at)
                    .await
            }
        }
    }

    async fn apply_descriptor_changed(
        &self,
        tenant_id: TenantId,
        value: &ItemDescriptorChanged,
        projected_at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        self.apply_descriptor(&DirectoryDescriptorPublication {
            tenant_id,
            descriptor: value.descriptor.clone(),
            authorized_search_text: value.authorized_search_text.clone(),
            projected_at,
        })
        .await
    }

    async fn apply_effective_grant(
        &self,
        tenant_id: TenantId,
        value: &EffectiveGrantChanged,
        projected_at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        self.apply_grant(&DirectoryGrantPublication {
            tenant_id,
            record_id: value.record.id(),
            principal_id: value.principal_id,
            authorization_policy: value.authorization_policy,
            policy_version: value.policy_version,
            can_view: true,
            effective_at: value.effective_at,
            expires_at: value.expires_at,
            source_event_position: value.source_event_position,
            projected_at,
        })
        .await
    }

    async fn apply_reference_relation(
        &self,
        tenant_id: TenantId,
        value: &ReferenceRelationChanged,
        projected_at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        self.apply_relation(&DirectoryRelationPublication {
            tenant_id,
            relation_id: value.relation_id,
            source_record_id: value.source.id(),
            target_record_id: value.target.id(),
            relation_type: value.relation_type.clone(),
            relation_version: value.relation_version,
            source_event_position: value.source_event_position,
            projected_at,
        })
        .await
    }

    pub async fn begin_rebuild(
        &self,
        tenant_id: TenantId,
        generation: u64,
        source_high_watermark: u64,
        at: DateTime<Utc>,
    ) -> Result<u64, DirectoryAdapterError> {
        let generation = positive_i64(generation, "rebuild_generation")?;
        let source = i64::try_from(source_high_watermark)
            .map_err(|_| DirectoryAdapterError::Invalid("source_high_watermark"))?;
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let current = sqlx::query(
            "SELECT rebuild_generation,source_high_watermark,applied_position,state \
             FROM erpx_collaboration_read.item_directory_projection_state \
             WHERE tenant_id=$1 FOR UPDATE",
        )
        .bind(tenant_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        if let Some(current) = current {
            let current_generation: i64 = current
                .try_get("rebuild_generation")
                .map_err(|_| DirectoryAdapterError::Unavailable)?;
            let current_source: i64 = current
                .try_get("source_high_watermark")
                .map_err(|_| DirectoryAdapterError::Unavailable)?;
            let current_position: i64 = current
                .try_get("applied_position")
                .map_err(|_| DirectoryAdapterError::Unavailable)?;
            let current_state: String = current
                .try_get("state")
                .map_err(|_| DirectoryAdapterError::Unavailable)?;
            if current_generation == generation
                && current_source == source
                && current_state == "rebuilding"
            {
                tx.commit().await.map_err(map_database_error)?;
                return nonnegative_u64(current_position, "applied_position");
            }
            if current_generation >= generation {
                return Err(DirectoryAdapterError::Invalid("rebuild_generation"));
            }
        }
        sqlx::query(
            "INSERT INTO erpx_collaboration_read.item_directory_projection_state \
             (tenant_id,rebuild_generation,source_high_watermark,applied_position,state,last_success_at,updated_at) \
             VALUES ($1,$2,$3,0,'rebuilding',NULL,$4) \
             ON CONFLICT (tenant_id) DO UPDATE SET \
              rebuild_generation=EXCLUDED.rebuild_generation, \
              source_high_watermark=EXCLUDED.source_high_watermark, applied_position=0, \
              state='rebuilding', last_success_at=NULL, updated_at=EXCLUDED.updated_at \
             WHERE erpx_collaboration_read.item_directory_projection_state.rebuild_generation \
                   < EXCLUDED.rebuild_generation",
        )
        .bind(tenant_id.as_uuid())
        .bind(generation)
        .bind(source)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        for table in [
            "item_directory_relations",
            "item_directory_effective_grants",
            "item_directory_descriptors",
            "item_directory_relation_tombstones",
            "item_directory_grant_tombstones",
            "item_directory_tombstones",
        ] {
            let statement =
                format!("DELETE FROM erpx_collaboration_read.{table} WHERE tenant_id=$1");
            sqlx::query(&statement)
                .bind(tenant_id.as_uuid())
                .execute(&mut *tx)
                .await
                .map_err(map_database_error)?;
        }
        tx.commit().await.map_err(map_database_error)?;
        Ok(0)
    }

    pub async fn checkpoint(
        &self,
        tenant_id: TenantId,
    ) -> Result<Option<DirectoryProjectionCheckpoint>, DirectoryAdapterError> {
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let row = sqlx::query(
            "SELECT rebuild_generation,source_high_watermark,applied_position,state \
             FROM erpx_collaboration_read.item_directory_projection_state WHERE tenant_id=$1",
        )
        .bind(tenant_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        row.map(|row| {
            let state: String = row
                .try_get("state")
                .map_err(|_| DirectoryAdapterError::Unavailable)?;
            Ok(DirectoryProjectionCheckpoint {
                generation: nonnegative_u64(
                    row.try_get("rebuild_generation")
                        .map_err(|_| DirectoryAdapterError::Unavailable)?,
                    "rebuild_generation",
                )?,
                source_high_watermark: nonnegative_u64(
                    row.try_get("source_high_watermark")
                        .map_err(|_| DirectoryAdapterError::Unavailable)?,
                    "source_high_watermark",
                )?,
                applied_position: nonnegative_u64(
                    row.try_get("applied_position")
                        .map_err(|_| DirectoryAdapterError::Unavailable)?,
                    "applied_position",
                )?,
                rebuilding: state == "rebuilding",
            })
        })
        .transpose()
    }

    pub async fn begin_incremental(
        &self,
        tenant_id: TenantId,
        source_high_watermark: u64,
        at: DateTime<Utc>,
    ) -> Result<DirectoryProjectionCheckpoint, DirectoryAdapterError> {
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let source = i64::try_from(source_high_watermark)
            .map_err(|_| DirectoryAdapterError::Invalid("source_high_watermark"))?;
        let row = sqlx::query(
            "UPDATE erpx_collaboration_read.item_directory_projection_state \
             SET source_high_watermark=GREATEST(source_high_watermark,$2),state='rebuilding',updated_at=$3 \
             WHERE tenant_id=$1 AND state IN ('ready','rebuilding') \
               AND applied_position<=GREATEST(source_high_watermark,$2) \
             RETURNING rebuild_generation,source_high_watermark,applied_position",
        )
        .bind(tenant_id.as_uuid())
        .bind(source)
        .bind(at)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        let Some(row) = row else {
            return Err(DirectoryAdapterError::Unavailable);
        };
        Ok(DirectoryProjectionCheckpoint {
            generation: nonnegative_u64(
                row.try_get("rebuild_generation")
                    .map_err(|_| DirectoryAdapterError::Unavailable)?,
                "rebuild_generation",
            )?,
            source_high_watermark: nonnegative_u64(
                row.try_get("source_high_watermark")
                    .map_err(|_| DirectoryAdapterError::Unavailable)?,
                "source_high_watermark",
            )?,
            applied_position: nonnegative_u64(
                row.try_get("applied_position")
                    .map_err(|_| DirectoryAdapterError::Unavailable)?,
                "applied_position",
            )?,
            rebuilding: true,
        })
    }

    pub async fn advance_checkpoint(
        &self,
        tenant_id: TenantId,
        generation: u64,
        expected_position: u64,
        applied_position: u64,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        if applied_position < expected_position {
            return Err(DirectoryAdapterError::Invalid("applied_position"));
        }
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let result = sqlx::query(
            "UPDATE erpx_collaboration_read.item_directory_projection_state \
             SET applied_position=$4,updated_at=$5 \
             WHERE tenant_id=$1 AND rebuild_generation=$2 AND applied_position=$3 \
               AND state='rebuilding' AND $4<=source_high_watermark",
        )
        .bind(tenant_id.as_uuid())
        .bind(positive_i64(generation, "rebuild_generation")?)
        .bind(
            i64::try_from(expected_position)
                .map_err(|_| DirectoryAdapterError::Invalid("expected_position"))?,
        )
        .bind(
            i64::try_from(applied_position)
                .map_err(|_| DirectoryAdapterError::Invalid("applied_position"))?,
        )
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        if result.rows_affected() != 1 {
            return Err(DirectoryAdapterError::Unavailable);
        }
        tx.commit().await.map_err(map_database_error)?;
        Ok(())
    }

    pub async fn apply_descriptor(
        &self,
        publication: &DirectoryDescriptorPublication,
    ) -> Result<(), DirectoryAdapterError> {
        publication.validate()?;
        let descriptor = &publication.descriptor;
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, publication.tenant_id).await?;
        let tombstoned: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM erpx_collaboration_read.item_directory_tombstones \
             WHERE tenant_id=$1 AND record_id=$2 AND source_event_position >= $3)",
        )
        .bind(publication.tenant_id.as_uuid())
        .bind(descriptor.record().id().as_uuid())
        .bind(positive_i64(
            descriptor.source_event_position(),
            "source_event_position",
        )?)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_database_error)?;
        if tombstoned {
            tx.commit().await.map_err(map_database_error)?;
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO erpx_collaboration_read.item_directory_descriptors \
             (tenant_id,record_id,owner_context,record_type,record_scope,authorization_policy_id, \
              authorization_policy_version,descriptor,canonical_route,authorized_search_text, \
              descriptor_version,source_event_position,projected_at,tombstoned_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,NULL) \
             ON CONFLICT (tenant_id,record_id) DO UPDATE SET \
              owner_context=EXCLUDED.owner_context, record_type=EXCLUDED.record_type, \
              record_scope=EXCLUDED.record_scope, \
              authorization_policy_id=EXCLUDED.authorization_policy_id, \
              authorization_policy_version=EXCLUDED.authorization_policy_version, \
              descriptor=EXCLUDED.descriptor, canonical_route=EXCLUDED.canonical_route, \
              authorized_search_text=EXCLUDED.authorized_search_text, \
              descriptor_version=EXCLUDED.descriptor_version, \
              source_event_position=EXCLUDED.source_event_position, \
              projected_at=EXCLUDED.projected_at, tombstoned_at=NULL \
             WHERE erpx_collaboration_read.item_directory_descriptors.source_event_position \
                   < EXCLUDED.source_event_position",
        )
        .bind(publication.tenant_id.as_uuid())
        .bind(descriptor.record().id().as_uuid())
        .bind(descriptor.record().owner_context().slug())
        .bind(descriptor.record().record_type().as_str())
        .bind(scope_json(descriptor.scope())?)
        .bind(descriptor.authorization_policy().as_uuid())
        .bind(positive_i64(
            descriptor.authorization_policy_version(),
            "authorization_policy_version",
        )?)
        .bind(
            serde_json::to_value(descriptor)
                .map_err(|_| DirectoryAdapterError::Invalid("descriptor"))?,
        )
        .bind(descriptor.canonical_route())
        .bind(&publication.authorized_search_text)
        .bind(positive_i64(
            descriptor.descriptor_version(),
            "descriptor_version",
        )?)
        .bind(positive_i64(
            descriptor.source_event_position(),
            "source_event_position",
        )?)
        .bind(publication.projected_at)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        Ok(())
    }

    pub async fn apply_grant(
        &self,
        publication: &DirectoryGrantPublication,
    ) -> Result<(), DirectoryAdapterError> {
        publication.validate()?;
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, publication.tenant_id).await?;
        let blocked: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM erpx_collaboration_read.item_directory_grant_tombstones \
             WHERE tenant_id=$1 AND record_id=$2 AND principal_id=$3 \
               AND source_event_position >= $4)",
        )
        .bind(publication.tenant_id.as_uuid())
        .bind(publication.record_id.as_uuid())
        .bind(publication.principal_id.as_uuid())
        .bind(positive_i64(
            publication.source_event_position,
            "source_event_position",
        )?)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_database_error)?;
        if blocked {
            tx.commit().await.map_err(map_database_error)?;
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO erpx_collaboration_read.item_directory_effective_grants \
             (tenant_id,record_id,principal_id,authorization_policy_id,policy_version,can_view, \
              effective_at,expires_at,source_event_position,projected_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) \
             ON CONFLICT (tenant_id,record_id,principal_id) DO UPDATE SET \
              authorization_policy_id=EXCLUDED.authorization_policy_id, \
              policy_version=EXCLUDED.policy_version, can_view=EXCLUDED.can_view, \
              effective_at=EXCLUDED.effective_at, expires_at=EXCLUDED.expires_at, \
              source_event_position=EXCLUDED.source_event_position, projected_at=EXCLUDED.projected_at \
             WHERE erpx_collaboration_read.item_directory_effective_grants.source_event_position \
                   < EXCLUDED.source_event_position",
        )
        .bind(publication.tenant_id.as_uuid())
        .bind(publication.record_id.as_uuid())
        .bind(publication.principal_id.as_uuid())
        .bind(publication.authorization_policy.as_uuid())
        .bind(positive_i64(publication.policy_version, "policy_version")?)
        .bind(publication.can_view)
        .bind(publication.effective_at)
        .bind(publication.expires_at)
        .bind(positive_i64(publication.source_event_position, "source_event_position")?)
        .bind(publication.projected_at)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        Ok(())
    }

    pub async fn tombstone_record(
        &self,
        tenant_id: TenantId,
        record_id: RecordId,
        source_event_position: u64,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        self.apply_item_tombstone(
            tenant_id,
            &ItemTombstoned {
                record_id,
                descriptor_version: source_event_position,
                source_event_position,
            },
            at,
        )
        .await
    }

    async fn apply_item_tombstone(
        &self,
        tenant_id: TenantId,
        value: &ItemTombstoned,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        let position = positive_i64(value.source_event_position, "source_event_position")?;
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        sqlx::query(
            "INSERT INTO erpx_collaboration_read.item_directory_tombstones \
             (tenant_id,record_id,descriptor_version,source_event_position,projected_at) \
             VALUES ($1,$2,$3,$4,$5) ON CONFLICT (tenant_id,record_id) DO UPDATE SET \
              descriptor_version=EXCLUDED.descriptor_version, \
              source_event_position=EXCLUDED.source_event_position,projected_at=EXCLUDED.projected_at \
             WHERE erpx_collaboration_read.item_directory_tombstones.source_event_position \
                   < EXCLUDED.source_event_position",
        )
        .bind(tenant_id.as_uuid())
        .bind(value.record_id.as_uuid())
        .bind(positive_i64(value.descriptor_version, "descriptor_version")?)
        .bind(position)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        // Hard purge makes search, grants, and both backlink directions
        // disappear atomically through tenant-composite cascading FKs.
        sqlx::query(
            "DELETE FROM erpx_collaboration_read.item_directory_descriptors \
             WHERE tenant_id=$1 AND record_id=$2 AND source_event_position<=$3",
        )
        .bind(tenant_id.as_uuid())
        .bind(value.record_id.as_uuid())
        .bind(position)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        Ok(())
    }

    async fn revoke_effective_grant(
        &self,
        tenant_id: TenantId,
        value: &EffectiveGrantRevoked,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        let position = positive_i64(value.source_event_position, "source_event_position")?;
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        sqlx::query(
            "INSERT INTO erpx_collaboration_read.item_directory_grant_tombstones \
             (tenant_id,record_id,principal_id,authorization_policy_id,policy_version,source_event_position,projected_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7) \
             ON CONFLICT (tenant_id,record_id,principal_id) DO UPDATE SET \
              authorization_policy_id=EXCLUDED.authorization_policy_id,policy_version=EXCLUDED.policy_version, \
              source_event_position=EXCLUDED.source_event_position,projected_at=EXCLUDED.projected_at \
             WHERE erpx_collaboration_read.item_directory_grant_tombstones.source_event_position \
                   < EXCLUDED.source_event_position",
        )
        .bind(tenant_id.as_uuid()).bind(value.record_id.as_uuid()).bind(value.principal_id.as_uuid())
        .bind(value.authorization_policy.as_uuid()).bind(positive_i64(value.policy_version,"policy_version")?)
        .bind(position).bind(at).execute(&mut *tx).await.map_err(map_database_error)?;
        sqlx::query(
            "DELETE FROM erpx_collaboration_read.item_directory_effective_grants \
             WHERE tenant_id=$1 AND record_id=$2 AND principal_id=$3 \
               AND source_event_position<=$4",
        )
        .bind(tenant_id.as_uuid())
        .bind(value.record_id.as_uuid())
        .bind(value.principal_id.as_uuid())
        .bind(position)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        Ok(())
    }

    async fn apply_route_move(
        &self,
        tenant_id: TenantId,
        value: &CanonicalRouteMoved,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        let position = positive_i64(value.source_event_position, "source_event_position")?;
        let version = positive_i64(value.descriptor_version, "descriptor_version")?;
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let result = sqlx::query(
            "UPDATE erpx_collaboration_read.item_directory_descriptors SET \
              descriptor=jsonb_set(jsonb_set(jsonb_set(descriptor,'{canonical_route}',to_jsonb($5::text),true),'{descriptor_version}',to_jsonb($6::bigint),true),'{source_event_position}',to_jsonb($7::bigint),true), \
              canonical_route=$5,descriptor_version=$6,source_event_position=$7,projected_at=$8 \
             WHERE tenant_id=$1 AND record_id=$2 AND owner_context=$3 AND record_type=$4 \
               AND tombstoned_at IS NULL AND source_event_position<$7",
        )
        .bind(tenant_id.as_uuid()).bind(value.record.id().as_uuid())
        .bind(value.record.owner_context().slug()).bind(value.record.record_type().as_str())
        .bind(&value.canonical_route).bind(version).bind(position).bind(at)
        .execute(&mut *tx).await.map_err(map_database_error)?;
        if result.rows_affected() == 0 {
            let settled: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM erpx_collaboration_read.item_directory_descriptors \
                 WHERE tenant_id=$1 AND record_id=$2 AND source_event_position >= $3) \
                 OR EXISTS (SELECT 1 FROM erpx_collaboration_read.item_directory_tombstones \
                 WHERE tenant_id=$1 AND record_id=$2 AND source_event_position >= $3)",
            )
            .bind(tenant_id.as_uuid())
            .bind(value.record.id().as_uuid())
            .bind(position)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_database_error)?;
            if !settled {
                return Err(DirectoryAdapterError::Unavailable);
            }
        }
        tx.commit().await.map_err(map_database_error)?;
        Ok(())
    }

    pub async fn apply_relation(
        &self,
        publication: &DirectoryRelationPublication,
    ) -> Result<(), DirectoryAdapterError> {
        publication.validate()?;
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, publication.tenant_id).await?;
        let blocked: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM erpx_collaboration_read.item_directory_relation_tombstones \
             WHERE tenant_id=$1 AND relation_id=$2 AND source_event_position >= $3)",
        )
        .bind(publication.tenant_id.as_uuid())
        .bind(publication.relation_id)
        .bind(positive_i64(
            publication.source_event_position,
            "source_event_position",
        )?)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_database_error)?;
        if blocked {
            tx.commit().await.map_err(map_database_error)?;
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO erpx_collaboration_read.item_directory_relations \
             (tenant_id,relation_id,source_record_id,target_record_id,relation_type, \
              relation_version,source_event_position,projected_at,tombstoned_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,NULL) \
             ON CONFLICT (tenant_id,relation_id) DO UPDATE SET \
              source_record_id=EXCLUDED.source_record_id, \
              target_record_id=EXCLUDED.target_record_id, \
              relation_type=EXCLUDED.relation_type, relation_version=EXCLUDED.relation_version, \
              source_event_position=EXCLUDED.source_event_position, \
              projected_at=EXCLUDED.projected_at, tombstoned_at=NULL \
             WHERE erpx_collaboration_read.item_directory_relations.source_event_position \
                   < EXCLUDED.source_event_position",
        )
        .bind(publication.tenant_id.as_uuid())
        .bind(publication.relation_id)
        .bind(publication.source_record_id.as_uuid())
        .bind(publication.target_record_id.as_uuid())
        .bind(&publication.relation_type)
        .bind(positive_i64(
            publication.relation_version,
            "relation_version",
        )?)
        .bind(positive_i64(
            publication.source_event_position,
            "source_event_position",
        )?)
        .bind(publication.projected_at)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        Ok(())
    }

    pub async fn tombstone_relation(
        &self,
        tenant_id: TenantId,
        relation_id: Uuid,
        source_event_position: u64,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        self.revoke_reference_relation(
            tenant_id,
            &ReferenceRelationRevoked {
                relation_id,
                source_event_position,
            },
            at,
        )
        .await
    }

    async fn revoke_reference_relation(
        &self,
        tenant_id: TenantId,
        value: &ReferenceRelationRevoked,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        value
            .validate()
            .map_err(|_| DirectoryAdapterError::Invalid("relation revocation"))?;
        let position = positive_i64(value.source_event_position, "source_event_position")?;
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        sqlx::query(
            "INSERT INTO erpx_collaboration_read.item_directory_relation_tombstones \
             (tenant_id,relation_id,source_event_position,projected_at) VALUES ($1,$2,$3,$4) \
             ON CONFLICT (tenant_id,relation_id) DO UPDATE SET \
              source_event_position=EXCLUDED.source_event_position,projected_at=EXCLUDED.projected_at \
             WHERE erpx_collaboration_read.item_directory_relation_tombstones.source_event_position \
                   < EXCLUDED.source_event_position",
        )
        .bind(tenant_id.as_uuid())
        .bind(value.relation_id)
        .bind(position)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        sqlx::query(
            "DELETE FROM erpx_collaboration_read.item_directory_relations \
             WHERE tenant_id=$1 AND relation_id=$2 AND source_event_position<=$3",
        )
        .bind(tenant_id.as_uuid())
        .bind(value.relation_id)
        .bind(position)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        Ok(())
    }

    pub async fn complete_rebuild(
        &self,
        tenant_id: TenantId,
        generation: u64,
        source_high_watermark: u64,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let result = sqlx::query(
            "UPDATE erpx_collaboration_read.item_directory_projection_state \
             SET applied_position=$3,state='ready',last_success_at=$4,updated_at=$4 \
             WHERE tenant_id=$1 AND rebuild_generation=$2 \
               AND source_high_watermark=$3 AND state='rebuilding'",
        )
        .bind(tenant_id.as_uuid())
        .bind(positive_i64(generation, "rebuild_generation")?)
        .bind(
            i64::try_from(source_high_watermark)
                .map_err(|_| DirectoryAdapterError::Invalid("source_high_watermark"))?,
        )
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        if result.rows_affected() != 1 {
            return Err(DirectoryAdapterError::Unavailable);
        }
        tx.commit().await.map_err(map_database_error)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryDescriptorPublication {
    pub tenant_id: TenantId,
    pub descriptor: ItemDescriptor,
    pub authorized_search_text: String,
    pub projected_at: DateTime<Utc>,
}

impl DirectoryDescriptorPublication {
    pub fn validate(&self) -> Result<(), DirectoryAdapterError> {
        self.descriptor
            .validate()
            .map_err(|_| DirectoryAdapterError::Invalid("descriptor"))?;
        validate_search_text(
            &self.authorized_search_text,
            MAX_DIRECTORY_INDEX_TEXT_BYTES,
            "authorized_search_text",
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectoryGrantPublication {
    pub tenant_id: TenantId,
    pub record_id: RecordId,
    pub principal_id: PrincipalId,
    pub authorization_policy: PolicyId,
    pub policy_version: u64,
    pub can_view: bool,
    pub effective_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub source_event_position: u64,
    pub projected_at: DateTime<Utc>,
}

impl DirectoryGrantPublication {
    pub fn validate(&self) -> Result<(), DirectoryAdapterError> {
        positive_i64(self.policy_version, "policy_version")?;
        positive_i64(self.source_event_position, "source_event_position")?;
        if self.expires_at <= self.effective_at {
            return Err(DirectoryAdapterError::Invalid("grant validity window"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryRelationPublication {
    pub tenant_id: TenantId,
    pub relation_id: Uuid,
    pub source_record_id: RecordId,
    pub target_record_id: RecordId,
    pub relation_type: String,
    pub relation_version: u64,
    pub source_event_position: u64,
    pub projected_at: DateTime<Utc>,
}

impl DirectoryRelationPublication {
    pub fn validate(&self) -> Result<(), DirectoryAdapterError> {
        if self.relation_id.is_nil()
            || self.source_record_id == self.target_record_id
            || !CORE_RELATION_TYPES.contains(&self.relation_type.as_str())
        {
            return Err(DirectoryAdapterError::Invalid("relation"));
        }
        positive_i64(self.relation_version, "relation_version")?;
        positive_i64(self.source_event_position, "source_event_position")?;
        Ok(())
    }
}

fn projection_freshness(
    state: String,
    source_high_watermark: i64,
    applied_position: i64,
    last_success_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<ProjectionFreshness, DirectoryAdapterError> {
    if source_high_watermark < 0 || applied_position < 0 || applied_position > source_high_watermark
    {
        return Err(DirectoryAdapterError::Unavailable);
    }
    Ok(match state.as_str() {
        "ready"
            if applied_position == source_high_watermark
                && last_success_at.is_some_and(|last| {
                    last <= now && now - last <= Duration::seconds(DIRECTORY_FRESHNESS_SECONDS)
                }) =>
        {
            ProjectionFreshness::Fresh
        }
        "ready" | "rebuilding" => ProjectionFreshness::Stale,
        "unavailable" => ProjectionFreshness::Unavailable,
        _ => return Err(DirectoryAdapterError::Unavailable),
    })
}

fn validate_search_text(
    value: &str,
    max_bytes: usize,
    field: &'static str,
) -> Result<(), DirectoryAdapterError> {
    if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(DirectoryAdapterError::Invalid(field));
    }
    Ok(())
}

fn parse_cursor(value: Option<&str>) -> Result<i64, DirectoryAdapterError> {
    let Some(value) = value else {
        return Ok(0);
    };
    if value.is_empty() || value.len() > 20 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(DirectoryAdapterError::Invalid("cursor"));
    }
    value
        .parse::<i64>()
        .ok()
        .filter(|value| *value >= 0)
        .ok_or(DirectoryAdapterError::Invalid("cursor"))
}

fn positive_i64(value: u64, field: &'static str) -> Result<i64, DirectoryAdapterError> {
    i64::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or(DirectoryAdapterError::Invalid(field))
}

fn nonnegative_u64(value: i64, field: &'static str) -> Result<u64, DirectoryAdapterError> {
    u64::try_from(value).map_err(|_| DirectoryAdapterError::Invalid(field))
}

fn authorized_count(rows: &[sqlx::postgres::PgRow]) -> Result<u64, DirectoryAdapterError> {
    rows.first()
        .map(|row| row.try_get::<i64, _>("authorized_count"))
        .transpose()
        .map_err(|_| DirectoryAdapterError::Unavailable)?
        .unwrap_or(0)
        .try_into()
        .map_err(|_| DirectoryAdapterError::Unavailable)
}

fn scope_json(value: &TenantScope) -> Result<serde_json::Value, DirectoryAdapterError> {
    serde_json::to_value(value).map_err(|_| DirectoryAdapterError::Invalid("authority_scope"))
}

async fn set_tenant(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: TenantId,
) -> Result<(), DirectoryAdapterError> {
    let expected = tenant_id.to_string();
    let bound = sqlx::query_scalar::<_, String>("SELECT set_config('app.tenant_id', $1, true)")
        .bind(tenant_id.to_string())
        .fetch_one(&mut **tx)
        .await
        .map_err(map_database_error)?;
    (bound == expected)
        .then_some(())
        .ok_or(DirectoryAdapterError::Unavailable)
}

fn map_database_error(_: sqlx::Error) -> DirectoryAdapterError {
    DirectoryAdapterError::Unavailable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_is_bounded_and_rebuild_is_stale() {
        let now = Utc::now();
        assert_eq!(
            projection_freshness("ready".into(), 4, 4, Some(now), now).unwrap(),
            ProjectionFreshness::Fresh
        );
        assert_eq!(
            projection_freshness("ready".into(), 4, 4, Some(now - Duration::seconds(61)), now,)
                .unwrap(),
            ProjectionFreshness::Stale
        );
        assert_eq!(
            projection_freshness("rebuilding".into(), 4, 2, None, now).unwrap(),
            ProjectionFreshness::Stale
        );
    }

    #[test]
    fn query_source_authorizes_before_search_and_never_counts_hidden_rows() {
        let source = include_str!("directory.rs");
        let grants = source.find("WITH authorized AS").expect("authorized CTE");
        let matching = source
            .find("search_vector @@ websearch_to_tsquery")
            .expect("full-text match");
        assert!(grants < matching);
        let unscoped_count = [
            "count(*) FROM erpx_collaboration_read.",
            "item_directory_descriptors",
        ]
        .concat();
        assert!(!source.contains(&unscoped_count));
    }

    #[test]
    fn typed_revocations_purge_visibility_and_block_delayed_facts() {
        let source = include_str!("directory.rs");
        assert!(source.contains("DELETE FROM erpx_collaboration_read.item_directory_descriptors"));
        assert!(source.contains("item_directory_grant_tombstones"));
        assert!(source.contains("item_directory_relation_tombstones"));
        assert!(source.contains("source_event_position >= $4"));
        assert!(source.contains("source_event_position >= $3"));
        assert!(source.matches("grant_fact.effective_at<=").count() >= 3);
        assert!(source.matches("grant_fact.expires_at>").count() >= 3);
        assert!(
            source
                .matches("grant_fact.source_event_position>=descriptor.source_event_position")
                .count()
                >= 3
        );
        assert!(
            source
                .matches("descriptor.authorization_policy_version=grant_fact.policy_version")
                .count()
                >= 3
        );
        assert!(!source.contains("ItemTombstoned {\n                record_type"));
    }

    #[test]
    fn candidate_rebuild_is_resumable_and_clears_only_tenant_local_candidate_rows() {
        let source = include_str!("directory.rs");
        let rebuild = source
            .split("pub async fn begin_rebuild")
            .nth(1)
            .expect("begin rebuild")
            .split("pub async fn checkpoint")
            .next()
            .expect("rebuild boundary");
        let resume = rebuild.find("current_state == \"rebuilding\"").unwrap();
        let first_delete = rebuild.find("DELETE FROM").unwrap();
        assert!(
            resume < first_delete,
            "retry must resume before clearing rows"
        );
        for table in [
            "item_directory_relations",
            "item_directory_effective_grants",
            "item_directory_descriptors",
            "item_directory_relation_tombstones",
            "item_directory_grant_tombstones",
            "item_directory_tombstones",
        ] {
            assert!(rebuild.contains(table), "candidate rebuild misses {table}");
        }
        assert!(rebuild.contains("WHERE tenant_id=$1"));
        assert!(!rebuild.contains("TRUNCATE"));
    }

    #[test]
    fn checkpoint_advance_is_generation_and_position_cas_fenced() {
        let source = include_str!("directory.rs");
        let checkpoint = source
            .split("pub async fn advance_checkpoint")
            .nth(1)
            .expect("checkpoint advance")
            .split("pub async fn apply_descriptor")
            .next()
            .expect("checkpoint boundary");
        assert!(checkpoint.contains("rebuild_generation=$2"));
        assert!(checkpoint.contains("applied_position=$3"));
        assert!(checkpoint.contains("state='rebuilding'"));
        assert!(checkpoint.contains("$4<=source_high_watermark"));
        assert!(checkpoint.contains("rows_affected() != 1"));
    }

    #[test]
    fn route_move_updates_only_the_existing_typed_record_identity() {
        let source = include_str!("directory.rs");
        let route_move = source
            .split("async fn apply_route_move")
            .nth(1)
            .expect("route move implementation")
            .split("pub async fn apply_relation")
            .next()
            .expect("route move boundary");
        assert!(route_move.contains("owner_context=$3 AND record_type=$4"));
        assert!(route_move.contains("source_event_position<$7"));
        assert!(route_move.contains("item_directory_tombstones"));
        assert!(!route_move.contains("INSERT INTO"));
    }
}
