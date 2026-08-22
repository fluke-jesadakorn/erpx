//! Holding-local SQLx adapter for the disposable federated Item Directory.
//!
//! The adapter accepts only capabilities created after current identity/role
//! authorization. Effective record grants are owner-published projection
//! facts; descriptor contents are never used to infer authority.

use std::{error::Error, fmt};

use chrono::{DateTime, Duration, Utc};
use erpx_integration_contracts::{
    AuthorizedItemSummary, CanonicalRouteMoved, EffectiveGrantChanged, EffectiveGrantRevoked,
    HoldingItemDirectoryFact, ItemDescriptor, ItemDescriptorChanged, ItemDirectoryFact,
    ItemTombstoned, RecordTypeId, ReferenceRelationChanged, ReferenceRelationRevoked,
    StableRecordRef,
};
use erpx_kernel::{AuthorityScope, HoldingId, PolicyId, PrincipalId, RecordId};
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
    holding_id: HoldingId,
    principal_id: PrincipalId,
    authority_scope: AuthorityScope,
}

impl DirectoryAccess {
    /// Must be called only after the server has authenticated the principal and
    /// authorized the Item Directory read capability at this exact scope.
    pub fn try_from_authorization(
        holding_id: HoldingId,
        principal_id: PrincipalId,
        authority_scope: AuthorityScope,
    ) -> Result<Self, DirectoryAdapterError> {
        authority_scope
            .validate()
            .map_err(|_| DirectoryAdapterError::Invalid("authority_scope"))?;
        Ok(Self {
            holding_id,
            principal_id,
            authority_scope,
        })
    }

    pub const fn holding_id(&self) -> HoldingId {
        self.holding_id
    }

    pub const fn principal_id(&self) -> PrincipalId {
        self.principal_id
    }

    pub const fn authority_scope(&self) -> &AuthorityScope {
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
    pub record_type: Option<String>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectoryAutocompleteTarget {
    Human {
        principal_id: PrincipalId,
        label: String,
    },
    Agent {
        principal_id: PrincipalId,
        definition_id: Uuid,
        version_id: Uuid,
        label: String,
    },
    Record {
        record: StableRecordRef,
        label: String,
    },
}

impl DirectorySearchRequest {
    pub fn validate(&self) -> Result<(), DirectoryAdapterError> {
        if self.query.is_empty() {
            if self.record_type.is_none() {
                return Err(DirectoryAdapterError::Invalid("search query"));
            }
        } else {
            validate_search_text(&self.query, MAX_DIRECTORY_SEARCH_BYTES, "search query")?;
        }
        if self
            .record_type
            .as_deref()
            .is_some_and(|value| RecordTypeId::try_new(value).is_err())
        {
            return Err(DirectoryAdapterError::Invalid("record_type"));
        }
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
    pub authorized_summaries: Vec<AuthorizedItemSummary>,
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
        let now = Utc::now();
        let mut tx = self.begin(access.holding_id()).await?;
        let row = sqlx::query(
            "WITH projection AS MATERIALIZED (\
               SELECT state,source_high_watermark,applied_position,last_success_at \
               FROM erpx_collaboration_read.item_directory_projection_state \
               WHERE holding_id=$1\
             ), authorized AS MATERIALIZED (\
               SELECT 1 \
               FROM projection \
               JOIN erpx_collaboration_read.item_directory_effective_grants grant_fact \
                 ON grant_fact.holding_id=$1 AND grant_fact.record_id=$2 \
                AND grant_fact.principal_id=$3 AND grant_fact.can_view \
               JOIN erpx_collaboration_read.item_directory_descriptors descriptor \
                 ON descriptor.holding_id=grant_fact.holding_id \
                AND descriptor.record_id=grant_fact.record_id \
                AND descriptor.authorization_policy_id=grant_fact.authorization_policy_id \
                AND descriptor.authorization_policy_version=grant_fact.policy_version \
                AND grant_fact.source_event_position>=descriptor.source_event_position \
               WHERE projection.state='ready' \
                 AND projection.applied_position=projection.source_high_watermark \
                 AND projection.last_success_at BETWEEN $5-INTERVAL '60 seconds' AND $5 \
                 AND descriptor.source_event_position<=projection.applied_position \
                 AND grant_fact.source_event_position<=projection.applied_position \
                 AND grant_fact.effective_at<=$5 AND grant_fact.expires_at>$5 \
                 AND descriptor.tombstoned_at IS NULL \
                 AND erpx_collaboration_read.scope_contains($4, descriptor.record_scope)\
             ) \
             SELECT projection.state,projection.source_high_watermark, \
                    projection.applied_position,projection.last_success_at, \
                    EXISTS (SELECT 1 FROM authorized) AS authorized \
             FROM projection",
        )
        .bind(access.holding_id().as_uuid())
        .bind(record_id.as_uuid())
        .bind(access.principal_id().as_uuid())
        .bind(scope_json(access.authority_scope())?)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        let Some(row) = row else {
            return Err(DirectoryAdapterError::Unavailable);
        };
        if projection_freshness_from_row(&row, now)? != ProjectionFreshness::Fresh {
            return Err(DirectoryAdapterError::Unavailable);
        }
        let authorized = row
            .try_get::<bool, _>("authorized")
            .map_err(|_| DirectoryAdapterError::Unavailable)?;
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
        let mut tx = self.begin(access.holding_id()).await?;
        // Projection freshness, the complete effective-grant proof, hierarchy
        // containment, and the protected descriptor are read by one SQL
        // statement. A projection replacement can therefore expose either
        // the old authorized descriptor or no descriptor, never a newer
        // payload that was not covered by the observed grant.
        let row = sqlx::query(
            "WITH projection AS MATERIALIZED (\
               SELECT state,source_high_watermark,applied_position,last_success_at \
               FROM erpx_collaboration_read.item_directory_projection_state \
               WHERE holding_id=$1\
             ), authorized AS MATERIALIZED (\
               SELECT descriptor.descriptor \
               FROM projection \
               JOIN erpx_collaboration_read.item_directory_effective_grants grant_fact \
                 ON grant_fact.holding_id=$1 AND grant_fact.record_id=$2 \
                AND grant_fact.principal_id=$3 AND grant_fact.can_view \
               JOIN erpx_collaboration_read.item_directory_descriptors descriptor \
                 ON descriptor.holding_id=grant_fact.holding_id \
                AND descriptor.record_id=grant_fact.record_id \
                AND descriptor.authorization_policy_id=grant_fact.authorization_policy_id \
                AND descriptor.authorization_policy_version=grant_fact.policy_version \
                AND grant_fact.source_event_position>=descriptor.source_event_position \
               WHERE projection.state='ready' \
                 AND projection.applied_position=projection.source_high_watermark \
                 AND projection.last_success_at BETWEEN $5-INTERVAL '60 seconds' AND $5 \
                 AND descriptor.source_event_position<=projection.applied_position \
                 AND grant_fact.source_event_position<=projection.applied_position \
                 AND grant_fact.effective_at<=$5 AND grant_fact.expires_at>$5 \
                 AND descriptor.tombstoned_at IS NULL \
                 AND erpx_collaboration_read.scope_contains($4, descriptor.record_scope)\
             ) \
             SELECT projection.state,projection.source_high_watermark, \
                    projection.applied_position,projection.last_success_at, \
                    authorized.descriptor \
             FROM projection LEFT JOIN authorized ON TRUE",
        )
        .bind(access.holding_id().as_uuid())
        .bind(record_id.as_uuid())
        .bind(access.principal_id().as_uuid())
        .bind(scope_json(access.authority_scope())?)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        let Some(row) = row else {
            return Ok(DirectoryLookup::Unavailable);
        };
        let freshness = projection_freshness_from_row(&row, now)?;
        if freshness == ProjectionFreshness::Unavailable {
            return Ok(DirectoryLookup::Unavailable);
        }
        if freshness == ProjectionFreshness::Stale {
            return Ok(DirectoryLookup::Degraded(freshness));
        }
        let value = row
            .try_get::<Option<serde_json::Value>, _>("descriptor")
            .map_err(|_| DirectoryAdapterError::Unavailable)?;
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
        let offset = parse_cursor(request.cursor.as_deref())?;
        let mut tx = self.begin(access.holding_id()).await?;
        // Freshness and the grant join precede the FTS predicate in the same
        // statement and form the only rows eligible for payload or counts.
        let rows = sqlx::query(
            "WITH projection AS MATERIALIZED (\
               SELECT state,source_high_watermark,applied_position,last_success_at \
               FROM erpx_collaboration_read.item_directory_projection_state \
               WHERE holding_id=$1\
             ), authorized AS MATERIALIZED (\
               SELECT descriptor.descriptor,descriptor.search_vector,descriptor.record_type, \
                      descriptor.source_event_position,descriptor.record_id \
               FROM projection \
               JOIN erpx_collaboration_read.item_directory_effective_grants grant_fact \
                 ON grant_fact.holding_id=$1 AND grant_fact.principal_id=$2 \
                AND grant_fact.can_view \
               JOIN erpx_collaboration_read.item_directory_descriptors descriptor \
                 ON descriptor.holding_id=grant_fact.holding_id \
                AND descriptor.record_id=grant_fact.record_id \
                AND descriptor.authorization_policy_id=grant_fact.authorization_policy_id \
                AND descriptor.authorization_policy_version=grant_fact.policy_version \
                AND grant_fact.source_event_position>=descriptor.source_event_position \
               WHERE projection.state='ready' \
                 AND projection.applied_position=projection.source_high_watermark \
                 AND projection.last_success_at BETWEEN $7-INTERVAL '60 seconds' AND $7 \
                 AND descriptor.source_event_position<=projection.applied_position \
                 AND grant_fact.source_event_position<=projection.applied_position \
                 AND descriptor.tombstoned_at IS NULL \
                 AND descriptor.display_label IS NOT NULL \
                 AND grant_fact.effective_at<=$7 AND grant_fact.expires_at>$7 \
                 AND erpx_collaboration_read.scope_contains($3, descriptor.record_scope)\
             ), matching AS MATERIALIZED (\
               SELECT descriptor,source_event_position,record_id \
               FROM authorized \
               WHERE ($4='' OR search_vector @@ websearch_to_tsquery('simple', $4)) \
                 AND ($8::TEXT IS NULL OR record_type=$8)\
             ), authorized_count AS (\
               SELECT count(*)::BIGINT AS authorized_count FROM matching\
             ), page AS (\
               SELECT descriptor FROM matching \
               ORDER BY source_event_position DESC,record_id LIMIT $5 OFFSET $6\
             ) \
             SELECT projection.state,projection.source_high_watermark, \
                    projection.applied_position,projection.last_success_at, \
                    page.descriptor,authorized_count.authorized_count \
             FROM projection CROSS JOIN authorized_count LEFT JOIN page ON TRUE",
        )
        .bind(access.holding_id().as_uuid())
        .bind(access.principal_id().as_uuid())
        .bind(scope_json(access.authority_scope())?)
        .bind(&request.query)
        .bind(i64::from(request.page_size))
        .bind(offset)
        .bind(now)
        .bind(request.record_type.as_deref())
        .fetch_all(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        let Some(first) = rows.first() else {
            return Ok(DirectoryLookup::Unavailable);
        };
        let freshness = projection_freshness_from_row(first, now)?;
        if freshness == ProjectionFreshness::Unavailable {
            return Ok(DirectoryLookup::Unavailable);
        }
        if freshness == ProjectionFreshness::Stale {
            return Ok(DirectoryLookup::Degraded(freshness));
        }
        let authorized_count = authorized_count(&rows)?;
        let items = rows
            .iter()
            .filter_map(|row| {
                let value = match row
                    .try_get::<Option<serde_json::Value>, _>("descriptor")
                    .map_err(|_| DirectoryAdapterError::Unavailable)
                {
                    Ok(Some(value)) => value,
                    Ok(None) => return None,
                    Err(error) => return Some(Err(error)),
                };
                Some((|| {
                    let descriptor = serde_json::from_value::<ItemDescriptor>(value)
                        .map_err(|_| DirectoryAdapterError::Unavailable)?;
                    descriptor
                        .validate()
                        .map_err(|_| DirectoryAdapterError::Unavailable)?;
                    Ok(descriptor)
                })())
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
        let authorized_summaries = items
            .iter()
            .map(AuthorizedItemSummary::try_from_descriptor)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| DirectoryAdapterError::Unavailable)?;
        Ok(DirectoryLookup::Visible {
            value: DirectorySearchPage {
                items,
                authorized_summaries,
                next_cursor,
                authorized_count,
            },
            freshness,
        })
    }

    /// Reauthorizes the anchor and selects only authorized people, approved
    /// Agent versions, and records under the same Item Directory checkpoint
    /// and PostgreSQL snapshot. A denied anchor produces no candidate labels,
    /// counts, or existence signal.
    pub async fn autocomplete(
        &self,
        access: &DirectoryAccess,
        anchor: &StableRecordRef,
        query: &str,
        page_size: u16,
        now: DateTime<Utc>,
    ) -> Result<DirectoryLookup<Vec<DirectoryAutocompleteTarget>>, DirectoryAdapterError> {
        anchor
            .validate()
            .map_err(|_| DirectoryAdapterError::Invalid("anchor"))?;
        validate_search_text(query, 128, "autocomplete query")?;
        if page_size == 0 || page_size > 50 {
            return Err(DirectoryAdapterError::Invalid("page_size"));
        }
        let mut tx = self.begin(access.holding_id()).await?;
        let rows = sqlx::query(
            "WITH projection AS MATERIALIZED (\
               SELECT state,source_high_watermark,applied_position,last_success_at \
               FROM erpx_collaboration_read.item_directory_projection_state \
               WHERE holding_id=$1\
             ), anchor_authorized AS MATERIALIZED (\
               SELECT projection.applied_position \
               FROM projection \
               JOIN erpx_collaboration_read.item_directory_effective_grants grant_fact \
                 ON grant_fact.holding_id=$1 AND grant_fact.record_id=$2 \
                AND grant_fact.principal_id=$3 AND grant_fact.can_view \
               JOIN erpx_collaboration_read.item_directory_descriptors descriptor \
                 ON descriptor.holding_id=grant_fact.holding_id \
                AND descriptor.record_id=grant_fact.record_id \
                AND descriptor.authorization_policy_id=grant_fact.authorization_policy_id \
                AND descriptor.authorization_policy_version=grant_fact.policy_version \
                AND grant_fact.source_event_position>=descriptor.source_event_position \
               WHERE projection.state='ready' \
                 AND projection.applied_position=projection.source_high_watermark \
                 AND projection.last_success_at BETWEEN $6-INTERVAL '60 seconds' AND $6 \
                 AND descriptor.source_event_position<=projection.applied_position \
                 AND grant_fact.source_event_position<=projection.applied_position \
                 AND grant_fact.effective_at<=$6 AND grant_fact.expires_at>$6 \
                 AND descriptor.tombstoned_at IS NULL \
                 AND erpx_collaboration_read.scope_contains($4,descriptor.record_scope)\
             ), human_generation AS MATERIALIZED (\
               SELECT generation FROM erpx_cqrs_read.projection_generations \
               WHERE holding_id=$1 AND projection_name='identity_access.principals' \
                 AND status='active'\
             ), agent_generation AS MATERIALIZED (\
               SELECT generation FROM erpx_cqrs_read.projection_generations \
               WHERE holding_id=$1 AND projection_name='agent_orchestration.agent_directory' \
                 AND status='active'\
             ), candidates AS MATERIALIZED (\
               SELECT 'human'::TEXT AS target_kind,p.principal_id,NULL::UUID AS definition_id, \
                      NULL::UUID AS version_id,NULL::JSONB AS descriptor,p.display_name AS label, \
                      p.source_global_sequence AS source_position \
               FROM anchor_authorized anchor \
               JOIN human_generation g ON TRUE \
               JOIN erpx_identity_access_read.principal_projection p \
                 ON p.holding_id=$1 AND p.generation=g.generation \
                AND p.principal_status='active' \
                AND p.source_global_sequence<=anchor.applied_position \
               WHERE erpx_collaboration_read.scope_contains($4,p.holding_scope) \
                 AND p.display_name ILIKE '%'||$5||'%' \
               UNION ALL \
               SELECT 'agent'::TEXT,a.agent_definition_id,a.agent_definition_id, \
                      versions.version_id,NULL::JSONB,a.agent_name,a.source_global_sequence \
               FROM anchor_authorized anchor \
               JOIN agent_generation g ON TRUE \
               JOIN erpx_agent_read.agent_directory_projection a \
                 ON a.holding_id=$1 AND a.generation=g.generation \
                AND a.definition_status='approved' \
                AND a.source_global_sequence<=anchor.applied_position \
               CROSS JOIN LATERAL unnest(a.approved_version_ids) versions(version_id) \
               WHERE erpx_collaboration_read.scope_contains($4,a.holding_scope) \
                 AND a.agent_name ILIKE '%'||$5||'%' \
               UNION ALL \
               SELECT 'record'::TEXT,NULL::UUID,NULL::UUID,NULL::UUID,d.descriptor, \
                      d.display_label,d.source_event_position \
               FROM anchor_authorized anchor \
               JOIN erpx_collaboration_read.item_directory_effective_grants grant_fact \
                 ON grant_fact.holding_id=$1 AND grant_fact.principal_id=$3 \
                AND grant_fact.can_view AND grant_fact.effective_at<=$6 \
                AND grant_fact.expires_at>$6 \
               JOIN erpx_collaboration_read.item_directory_descriptors d \
                 ON d.holding_id=grant_fact.holding_id AND d.record_id=grant_fact.record_id \
                AND d.authorization_policy_id=grant_fact.authorization_policy_id \
                AND d.authorization_policy_version=grant_fact.policy_version \
                AND grant_fact.source_event_position>=d.source_event_position \
               WHERE d.tombstoned_at IS NULL AND d.display_label IS NOT NULL \
                 AND d.source_event_position<=anchor.applied_position \
                 AND grant_fact.source_event_position<=anchor.applied_position \
                 AND erpx_collaboration_read.scope_contains($4,d.record_scope) \
                 AND d.search_vector @@ websearch_to_tsquery('simple',$5)\
             ), page AS (\
               SELECT * FROM candidates \
               ORDER BY target_kind,label,source_position DESC LIMIT $7\
             ) \
             SELECT projection.state,projection.source_high_watermark, \
                    projection.applied_position,projection.last_success_at, \
                    EXISTS(SELECT 1 FROM anchor_authorized) AS anchor_authorized, \
                    page.target_kind,page.principal_id,page.definition_id,page.version_id, \
                    page.descriptor,page.label \
             FROM projection LEFT JOIN page ON TRUE",
        )
        .bind(access.holding_id().as_uuid())
        .bind(anchor.id().as_uuid())
        .bind(access.principal_id().as_uuid())
        .bind(scope_json(access.authority_scope())?)
        .bind(query)
        .bind(now)
        .bind(i64::from(page_size))
        .fetch_all(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        let Some(first) = rows.first() else {
            return Ok(DirectoryLookup::Unavailable);
        };
        let freshness = projection_freshness_from_row(first, now)?;
        if freshness == ProjectionFreshness::Unavailable {
            return Ok(DirectoryLookup::Unavailable);
        }
        if freshness == ProjectionFreshness::Stale {
            return Ok(DirectoryLookup::Degraded(freshness));
        }
        if !first
            .try_get::<bool, _>("anchor_authorized")
            .map_err(|_| DirectoryAdapterError::Unavailable)?
        {
            return Ok(DirectoryLookup::Neutral);
        }
        let mut targets = Vec::new();
        for row in rows {
            let Some(kind) = row
                .try_get::<Option<String>, _>("target_kind")
                .map_err(|_| DirectoryAdapterError::Unavailable)?
            else {
                continue;
            };
            let label = row
                .try_get::<Option<String>, _>("label")
                .map_err(|_| DirectoryAdapterError::Unavailable)?
                .filter(|label| !label.trim().is_empty())
                .ok_or(DirectoryAdapterError::Unavailable)?;
            let target = match kind.as_str() {
                "human" => DirectoryAutocompleteTarget::Human {
                    principal_id: PrincipalId::try_from_uuid(
                        row.try_get("principal_id")
                            .map_err(|_| DirectoryAdapterError::Unavailable)?,
                    )
                    .map_err(|_| DirectoryAdapterError::Unavailable)?,
                    label,
                },
                "agent" => DirectoryAutocompleteTarget::Agent {
                    principal_id: PrincipalId::try_from_uuid(
                        row.try_get("principal_id")
                            .map_err(|_| DirectoryAdapterError::Unavailable)?,
                    )
                    .map_err(|_| DirectoryAdapterError::Unavailable)?,
                    definition_id: row
                        .try_get("definition_id")
                        .map_err(|_| DirectoryAdapterError::Unavailable)?,
                    version_id: row
                        .try_get("version_id")
                        .map_err(|_| DirectoryAdapterError::Unavailable)?,
                    label,
                },
                "record" => {
                    let descriptor: serde_json::Value = row
                        .try_get("descriptor")
                        .map_err(|_| DirectoryAdapterError::Unavailable)?;
                    let descriptor = serde_json::from_value::<ItemDescriptor>(descriptor)
                        .map_err(|_| DirectoryAdapterError::Unavailable)?;
                    descriptor
                        .validate()
                        .map_err(|_| DirectoryAdapterError::Unavailable)?;
                    DirectoryAutocompleteTarget::Record {
                        record: descriptor.record().clone(),
                        label,
                    }
                }
                _ => return Err(DirectoryAdapterError::Unavailable),
            };
            targets.push(target);
        }
        Ok(DirectoryLookup::Visible {
            value: targets,
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
        let mut tx = self.begin(access.holding_id()).await?;
        let rows = sqlx::query(
            "WITH projection AS MATERIALIZED (\
               SELECT state,source_high_watermark,applied_position,last_success_at \
               FROM erpx_collaboration_read.item_directory_projection_state \
               WHERE holding_id=$1\
             ), anchor_authorized AS MATERIALIZED (\
               SELECT projection.applied_position \
               FROM projection \
               JOIN erpx_collaboration_read.item_directory_effective_grants grant_fact \
                 ON grant_fact.holding_id=$1 AND grant_fact.record_id=$2 \
                AND grant_fact.principal_id=$3 AND grant_fact.can_view \
               JOIN erpx_collaboration_read.item_directory_descriptors descriptor \
                 ON descriptor.holding_id=grant_fact.holding_id \
                AND descriptor.record_id=grant_fact.record_id \
                AND descriptor.authorization_policy_id=grant_fact.authorization_policy_id \
                AND descriptor.authorization_policy_version=grant_fact.policy_version \
                AND grant_fact.source_event_position>=descriptor.source_event_position \
               WHERE projection.state='ready' \
                 AND projection.applied_position=projection.source_high_watermark \
                 AND projection.last_success_at BETWEEN $7-INTERVAL '60 seconds' AND $7 \
                 AND descriptor.source_event_position<=projection.applied_position \
                 AND grant_fact.source_event_position<=projection.applied_position \
                 AND grant_fact.effective_at<=$7 AND grant_fact.expires_at>$7 \
                 AND descriptor.tombstoned_at IS NULL \
                 AND erpx_collaboration_read.scope_contains($4, descriptor.record_scope)\
             ), linked AS MATERIALIZED (\
               SELECT anchor_authorized.applied_position,relation.relation_id, \
                      relation.source_record_id,relation.target_record_id, \
                      CASE WHEN relation.source_record_id=$2 \
                           THEN relation.target_record_id ELSE relation.source_record_id END AS linked_id, \
                      relation.target_record_id=$2 AS incoming \
               FROM anchor_authorized \
               JOIN erpx_collaboration_read.item_directory_relations relation \
                 ON relation.holding_id=$1 AND relation.tombstoned_at IS NULL \
                 AND relation.source_event_position<=anchor_authorized.applied_position \
                 AND (relation.source_record_id=$2 OR relation.target_record_id=$2)\
             ), authorized AS MATERIALIZED (\
               SELECT linked.*, descriptor.descriptor \
               FROM linked \
               JOIN erpx_collaboration_read.item_directory_effective_grants grant_fact \
                ON grant_fact.holding_id=$1 AND grant_fact.record_id=linked.linked_id \
                AND grant_fact.principal_id=$3 AND grant_fact.can_view \
                AND grant_fact.effective_at<=$7 AND grant_fact.expires_at>$7 \
               JOIN erpx_collaboration_read.item_directory_descriptors descriptor \
                 ON descriptor.holding_id=grant_fact.holding_id \
                AND descriptor.record_id=grant_fact.record_id \
                AND descriptor.authorization_policy_id=grant_fact.authorization_policy_id \
                AND descriptor.authorization_policy_version=grant_fact.policy_version \
                AND grant_fact.source_event_position>=descriptor.source_event_position \
               WHERE descriptor.tombstoned_at IS NULL \
                 AND descriptor.source_event_position<=linked.applied_position \
                 AND grant_fact.source_event_position<=linked.applied_position \
                 AND erpx_collaboration_read.scope_contains($4, descriptor.record_scope)\
             ), authorized_count AS (\
               SELECT count(*)::BIGINT AS authorized_count FROM authorized\
             ), page AS (\
               SELECT descriptor,incoming,relation_id FROM authorized \
               ORDER BY relation_id LIMIT $5 OFFSET $6\
             ) \
             SELECT projection.state,projection.source_high_watermark, \
                    projection.applied_position,projection.last_success_at, \
                    EXISTS (SELECT 1 FROM anchor_authorized) AS anchor_authorized, \
                    page.descriptor,page.incoming,authorized_count.authorized_count \
             FROM projection CROSS JOIN authorized_count LEFT JOIN page ON TRUE",
        )
        .bind(access.holding_id().as_uuid())
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
        let Some(first) = rows.first() else {
            return Ok(DirectoryLookup::Unavailable);
        };
        let freshness = projection_freshness_from_row(first, now)?;
        if freshness == ProjectionFreshness::Unavailable {
            return Ok(DirectoryLookup::Unavailable);
        }
        if freshness == ProjectionFreshness::Stale {
            return Ok(DirectoryLookup::Degraded(freshness));
        }
        if !first
            .try_get::<bool, _>("anchor_authorized")
            .map_err(|_| DirectoryAdapterError::Unavailable)?
        {
            return Ok(DirectoryLookup::Neutral);
        }
        let authorized_count = authorized_count(&rows)?;
        let mut incoming = Vec::new();
        let mut outgoing = Vec::new();
        for row in &rows {
            let value = row
                .try_get::<Option<serde_json::Value>, _>("descriptor")
                .map_err(|_| DirectoryAdapterError::Unavailable)?;
            let Some(value) = value else {
                continue;
            };
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
        let returned_count = incoming
            .len()
            .checked_add(outgoing.len())
            .ok_or(DirectoryAdapterError::Unavailable)?;
        let consumed = offset
            .checked_add(
                i64::try_from(returned_count).map_err(|_| DirectoryAdapterError::Unavailable)?,
            )
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
        holding_id: HoldingId,
        now: DateTime<Utc>,
    ) -> Result<ProjectionFreshness, DirectoryAdapterError> {
        let mut tx = self.begin(holding_id).await?;
        let row = sqlx::query(
            "SELECT state, source_high_watermark, applied_position, last_success_at \
             FROM erpx_collaboration_read.item_directory_projection_state \
             WHERE holding_id=$1",
        )
        .bind(holding_id.as_uuid())
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
        holding_id: HoldingId,
    ) -> Result<Transaction<'_, Postgres>, DirectoryAdapterError> {
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, holding_id).await?;
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
        publication: &HoldingItemDirectoryFact,
        projected_at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        publication
            .validate()
            .map_err(|_| DirectoryAdapterError::Invalid("typed owner fact"))?;
        match &publication.fact {
            ItemDirectoryFact::DescriptorChanged(value) => {
                self.apply_descriptor_changed(publication.holding_id, value, projected_at)
                    .await
            }
            ItemDirectoryFact::EffectiveGrantChanged(value) => {
                self.apply_effective_grant(publication.holding_id, value, projected_at)
                    .await
            }
            ItemDirectoryFact::EffectiveGrantRevoked(value) => {
                self.revoke_effective_grant(publication.holding_id, value, projected_at)
                    .await
            }
            ItemDirectoryFact::RelationChanged(value) => {
                self.apply_reference_relation(publication.holding_id, value, projected_at)
                    .await
            }
            ItemDirectoryFact::RelationRevoked(value) => {
                self.revoke_reference_relation(publication.holding_id, value, projected_at)
                    .await
            }
            ItemDirectoryFact::RouteMoved(value) => {
                self.apply_route_move(publication.holding_id, value, projected_at)
                    .await
            }
            ItemDirectoryFact::Tombstoned(value) => {
                self.apply_item_tombstone(publication.holding_id, value, projected_at)
                    .await
            }
        }
    }

    async fn apply_descriptor_changed(
        &self,
        holding_id: HoldingId,
        value: &ItemDescriptorChanged,
        projected_at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        self.apply_descriptor(&DirectoryDescriptorPublication {
            holding_id,
            descriptor: value.descriptor.clone(),
            authorized_search_text: value.authorized_search_text.clone(),
            projected_at,
        })
        .await
    }

    async fn apply_effective_grant(
        &self,
        holding_id: HoldingId,
        value: &EffectiveGrantChanged,
        projected_at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        self.apply_grant(&DirectoryGrantPublication {
            holding_id,
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
        holding_id: HoldingId,
        value: &ReferenceRelationChanged,
        projected_at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        self.apply_relation(&DirectoryRelationPublication {
            holding_id,
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
        holding_id: HoldingId,
        generation: u64,
        source_high_watermark: u64,
        at: DateTime<Utc>,
    ) -> Result<u64, DirectoryAdapterError> {
        let generation = positive_i64(generation, "rebuild_generation")?;
        let source = i64::try_from(source_high_watermark)
            .map_err(|_| DirectoryAdapterError::Invalid("source_high_watermark"))?;
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, holding_id).await?;
        let current = sqlx::query(
            "SELECT rebuild_generation,source_high_watermark,applied_position,state \
             FROM erpx_collaboration_read.item_directory_projection_state \
             WHERE holding_id=$1 FOR UPDATE",
        )
        .bind(holding_id.as_uuid())
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
             (holding_id,rebuild_generation,source_high_watermark,applied_position,state,last_success_at,updated_at) \
             VALUES ($1,$2,$3,0,'rebuilding',NULL,$4) \
             ON CONFLICT (holding_id) DO UPDATE SET \
              rebuild_generation=EXCLUDED.rebuild_generation, \
              source_high_watermark=EXCLUDED.source_high_watermark, applied_position=0, \
              state='rebuilding', last_success_at=NULL, updated_at=EXCLUDED.updated_at \
             WHERE erpx_collaboration_read.item_directory_projection_state.rebuild_generation \
                   < EXCLUDED.rebuild_generation",
        )
        .bind(holding_id.as_uuid())
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
                format!("DELETE FROM erpx_collaboration_read.{table} WHERE holding_id=$1");
            sqlx::query(&statement)
                .bind(holding_id.as_uuid())
                .execute(&mut *tx)
                .await
                .map_err(map_database_error)?;
        }
        tx.commit().await.map_err(map_database_error)?;
        Ok(0)
    }

    pub async fn checkpoint(
        &self,
        holding_id: HoldingId,
    ) -> Result<Option<DirectoryProjectionCheckpoint>, DirectoryAdapterError> {
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, holding_id).await?;
        let row = sqlx::query(
            "SELECT rebuild_generation,source_high_watermark,applied_position,state \
             FROM erpx_collaboration_read.item_directory_projection_state WHERE holding_id=$1",
        )
        .bind(holding_id.as_uuid())
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
        holding_id: HoldingId,
        source_high_watermark: u64,
        at: DateTime<Utc>,
    ) -> Result<DirectoryProjectionCheckpoint, DirectoryAdapterError> {
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, holding_id).await?;
        let source = i64::try_from(source_high_watermark)
            .map_err(|_| DirectoryAdapterError::Invalid("source_high_watermark"))?;
        let row = sqlx::query(
            "INSERT INTO erpx_collaboration_read.item_directory_projection_state \
                (holding_id,rebuild_generation,source_high_watermark,applied_position,state,last_success_at,updated_at) \
             VALUES ($1,1,$2,0,'rebuilding',NULL,$3) \
             ON CONFLICT (holding_id) DO UPDATE SET \
              source_high_watermark=GREATEST( \
                erpx_collaboration_read.item_directory_projection_state.source_high_watermark, \
                EXCLUDED.source_high_watermark), \
              state='rebuilding',updated_at=EXCLUDED.updated_at \
             WHERE erpx_collaboration_read.item_directory_projection_state.state IN ('ready','rebuilding') \
               AND erpx_collaboration_read.item_directory_projection_state.applied_position \
                   <= GREATEST( \
                     erpx_collaboration_read.item_directory_projection_state.source_high_watermark, \
                     EXCLUDED.source_high_watermark) \
             RETURNING rebuild_generation,source_high_watermark,applied_position",
        )
        .bind(holding_id.as_uuid())
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
        holding_id: HoldingId,
        generation: u64,
        expected_position: u64,
        applied_position: u64,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        if applied_position < expected_position {
            return Err(DirectoryAdapterError::Invalid("applied_position"));
        }
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, holding_id).await?;
        let result = sqlx::query(
            "UPDATE erpx_collaboration_read.item_directory_projection_state \
             SET applied_position=$4,updated_at=$5 \
             WHERE holding_id=$1 AND rebuild_generation=$2 AND applied_position=$3 \
               AND state='rebuilding' AND $4<=source_high_watermark",
        )
        .bind(holding_id.as_uuid())
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
        set_holding(&mut tx, publication.holding_id).await?;
        let tombstoned: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM erpx_collaboration_read.item_directory_tombstones \
             WHERE holding_id=$1 AND record_id=$2 AND source_event_position >= $3)",
        )
        .bind(publication.holding_id.as_uuid())
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
             (holding_id,record_id,owner_context,record_type,record_scope,authorization_policy_id, \
              authorization_policy_version,descriptor,canonical_route,display_label,authorized_search_text, \
              descriptor_version,source_event_position,projected_at,tombstoned_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,NULL) \
             ON CONFLICT (holding_id,record_id) DO UPDATE SET \
              owner_context=EXCLUDED.owner_context, record_type=EXCLUDED.record_type, \
              record_scope=EXCLUDED.record_scope, \
              authorization_policy_id=EXCLUDED.authorization_policy_id, \
              authorization_policy_version=EXCLUDED.authorization_policy_version, \
              descriptor=EXCLUDED.descriptor, canonical_route=EXCLUDED.canonical_route, \
              display_label=EXCLUDED.display_label, \
              authorized_search_text=EXCLUDED.authorized_search_text, \
              descriptor_version=EXCLUDED.descriptor_version, \
              source_event_position=EXCLUDED.source_event_position, \
              projected_at=EXCLUDED.projected_at, tombstoned_at=NULL \
             WHERE erpx_collaboration_read.item_directory_descriptors.source_event_position \
                   < EXCLUDED.source_event_position",
        )
        .bind(publication.holding_id.as_uuid())
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
        .bind(descriptor.display_label())
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
        set_holding(&mut tx, publication.holding_id).await?;
        let blocked: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM erpx_collaboration_read.item_directory_grant_tombstones \
             WHERE holding_id=$1 AND record_id=$2 AND principal_id=$3 \
               AND source_event_position >= $4)",
        )
        .bind(publication.holding_id.as_uuid())
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
             (holding_id,record_id,principal_id,authorization_policy_id,policy_version,can_view, \
              effective_at,expires_at,source_event_position,projected_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) \
             ON CONFLICT (holding_id,record_id,principal_id) DO UPDATE SET \
              authorization_policy_id=EXCLUDED.authorization_policy_id, \
              policy_version=EXCLUDED.policy_version, can_view=EXCLUDED.can_view, \
              effective_at=EXCLUDED.effective_at, expires_at=EXCLUDED.expires_at, \
              source_event_position=EXCLUDED.source_event_position, projected_at=EXCLUDED.projected_at \
             WHERE erpx_collaboration_read.item_directory_effective_grants.source_event_position \
                   < EXCLUDED.source_event_position",
        )
        .bind(publication.holding_id.as_uuid())
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
        holding_id: HoldingId,
        record_id: RecordId,
        source_event_position: u64,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        self.apply_item_tombstone(
            holding_id,
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
        holding_id: HoldingId,
        value: &ItemTombstoned,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        let position = positive_i64(value.source_event_position, "source_event_position")?;
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, holding_id).await?;
        sqlx::query(
            "INSERT INTO erpx_collaboration_read.item_directory_tombstones \
             (holding_id,record_id,descriptor_version,source_event_position,projected_at) \
             VALUES ($1,$2,$3,$4,$5) ON CONFLICT (holding_id,record_id) DO UPDATE SET \
              descriptor_version=EXCLUDED.descriptor_version, \
              source_event_position=EXCLUDED.source_event_position,projected_at=EXCLUDED.projected_at \
             WHERE erpx_collaboration_read.item_directory_tombstones.source_event_position \
                   < EXCLUDED.source_event_position",
        )
        .bind(holding_id.as_uuid())
        .bind(value.record_id.as_uuid())
        .bind(positive_i64(value.descriptor_version, "descriptor_version")?)
        .bind(position)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        // Hard purge makes search, grants, and both backlink directions
        // disappear atomically through holding-composite cascading FKs.
        sqlx::query(
            "DELETE FROM erpx_collaboration_read.item_directory_descriptors \
             WHERE holding_id=$1 AND record_id=$2 AND source_event_position<=$3",
        )
        .bind(holding_id.as_uuid())
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
        holding_id: HoldingId,
        value: &EffectiveGrantRevoked,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        let position = positive_i64(value.source_event_position, "source_event_position")?;
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, holding_id).await?;
        sqlx::query(
            "INSERT INTO erpx_collaboration_read.item_directory_grant_tombstones \
             (holding_id,record_id,principal_id,authorization_policy_id,policy_version,source_event_position,projected_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7) \
             ON CONFLICT (holding_id,record_id,principal_id) DO UPDATE SET \
              authorization_policy_id=EXCLUDED.authorization_policy_id,policy_version=EXCLUDED.policy_version, \
              source_event_position=EXCLUDED.source_event_position,projected_at=EXCLUDED.projected_at \
             WHERE erpx_collaboration_read.item_directory_grant_tombstones.source_event_position \
                   < EXCLUDED.source_event_position",
        )
        .bind(holding_id.as_uuid()).bind(value.record_id.as_uuid()).bind(value.principal_id.as_uuid())
        .bind(value.authorization_policy.as_uuid()).bind(positive_i64(value.policy_version,"policy_version")?)
        .bind(position).bind(at).execute(&mut *tx).await.map_err(map_database_error)?;
        sqlx::query(
            "DELETE FROM erpx_collaboration_read.item_directory_effective_grants \
             WHERE holding_id=$1 AND record_id=$2 AND principal_id=$3 \
               AND source_event_position<=$4",
        )
        .bind(holding_id.as_uuid())
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
        holding_id: HoldingId,
        value: &CanonicalRouteMoved,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        let position = positive_i64(value.source_event_position, "source_event_position")?;
        let version = positive_i64(value.descriptor_version, "descriptor_version")?;
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, holding_id).await?;
        let result = sqlx::query(
            "UPDATE erpx_collaboration_read.item_directory_descriptors SET \
              descriptor=jsonb_set(jsonb_set(jsonb_set(descriptor,'{canonical_route}',to_jsonb($5::text),true),'{descriptor_version}',to_jsonb($6::bigint),true),'{source_event_position}',to_jsonb($7::bigint),true), \
              canonical_route=$5,descriptor_version=$6,source_event_position=$7,projected_at=$8 \
             WHERE holding_id=$1 AND record_id=$2 AND owner_context=$3 AND record_type=$4 \
               AND tombstoned_at IS NULL AND source_event_position<$7",
        )
        .bind(holding_id.as_uuid()).bind(value.record.id().as_uuid())
        .bind(value.record.owner_context().slug()).bind(value.record.record_type().as_str())
        .bind(&value.canonical_route).bind(version).bind(position).bind(at)
        .execute(&mut *tx).await.map_err(map_database_error)?;
        if result.rows_affected() == 0 {
            let settled: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM erpx_collaboration_read.item_directory_descriptors \
                 WHERE holding_id=$1 AND record_id=$2 AND source_event_position >= $3) \
                 OR EXISTS (SELECT 1 FROM erpx_collaboration_read.item_directory_tombstones \
                 WHERE holding_id=$1 AND record_id=$2 AND source_event_position >= $3)",
            )
            .bind(holding_id.as_uuid())
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
        set_holding(&mut tx, publication.holding_id).await?;
        let blocked: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM erpx_collaboration_read.item_directory_relation_tombstones \
             WHERE holding_id=$1 AND relation_id=$2 AND source_event_position >= $3)",
        )
        .bind(publication.holding_id.as_uuid())
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
             (holding_id,relation_id,source_record_id,target_record_id,relation_type, \
              relation_version,source_event_position,projected_at,tombstoned_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,NULL) \
             ON CONFLICT (holding_id,relation_id) DO UPDATE SET \
              source_record_id=EXCLUDED.source_record_id, \
              target_record_id=EXCLUDED.target_record_id, \
              relation_type=EXCLUDED.relation_type, relation_version=EXCLUDED.relation_version, \
              source_event_position=EXCLUDED.source_event_position, \
              projected_at=EXCLUDED.projected_at, tombstoned_at=NULL \
             WHERE erpx_collaboration_read.item_directory_relations.source_event_position \
                   < EXCLUDED.source_event_position",
        )
        .bind(publication.holding_id.as_uuid())
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
        holding_id: HoldingId,
        relation_id: Uuid,
        source_event_position: u64,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        self.revoke_reference_relation(
            holding_id,
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
        holding_id: HoldingId,
        value: &ReferenceRelationRevoked,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        value
            .validate()
            .map_err(|_| DirectoryAdapterError::Invalid("relation revocation"))?;
        let position = positive_i64(value.source_event_position, "source_event_position")?;
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, holding_id).await?;
        sqlx::query(
            "INSERT INTO erpx_collaboration_read.item_directory_relation_tombstones \
             (holding_id,relation_id,source_event_position,projected_at) VALUES ($1,$2,$3,$4) \
             ON CONFLICT (holding_id,relation_id) DO UPDATE SET \
              source_event_position=EXCLUDED.source_event_position,projected_at=EXCLUDED.projected_at \
             WHERE erpx_collaboration_read.item_directory_relation_tombstones.source_event_position \
                   < EXCLUDED.source_event_position",
        )
        .bind(holding_id.as_uuid())
        .bind(value.relation_id)
        .bind(position)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        sqlx::query(
            "DELETE FROM erpx_collaboration_read.item_directory_relations \
             WHERE holding_id=$1 AND relation_id=$2 AND source_event_position<=$3",
        )
        .bind(holding_id.as_uuid())
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
        holding_id: HoldingId,
        generation: u64,
        source_high_watermark: u64,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryAdapterError> {
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, holding_id).await?;
        let result = sqlx::query(
            "UPDATE erpx_collaboration_read.item_directory_projection_state \
             SET applied_position=$3,state='ready',last_success_at=$4,updated_at=$4 \
             WHERE holding_id=$1 AND rebuild_generation=$2 \
               AND source_high_watermark=$3 AND state='rebuilding'",
        )
        .bind(holding_id.as_uuid())
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
    pub holding_id: HoldingId,
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
    pub holding_id: HoldingId,
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
    pub holding_id: HoldingId,
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

fn projection_freshness_from_row(
    row: &sqlx::postgres::PgRow,
    now: DateTime<Utc>,
) -> Result<ProjectionFreshness, DirectoryAdapterError> {
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

fn scope_json(value: &AuthorityScope) -> Result<serde_json::Value, DirectoryAdapterError> {
    serde_json::to_value(value).map_err(|_| DirectoryAdapterError::Invalid("authority_scope"))
}

async fn set_holding(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
) -> Result<(), DirectoryAdapterError> {
    let expected = holding_id.to_string();
    let bound = sqlx::query_scalar::<_, String>("SELECT set_config('app.holding_id', $1, true)")
        .bind(holding_id.to_string())
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
        let search = source
            .split("pub async fn search")
            .nth(1)
            .expect("search")
            .split("pub async fn autocomplete")
            .next()
            .expect("search boundary");
        let grants = search
            .find("authorized AS MATERIALIZED")
            .expect("authorized CTE");
        let matching = search
            .find("search_vector @@ websearch_to_tsquery")
            .expect("full-text match");
        assert!(grants < matching);
        let unscoped_count = [
            "count(*) FROM erpx_collaboration_read.",
            "item_directory_descriptors",
        ]
        .concat();
        assert!(!source.contains(&unscoped_count));
        assert!(
            source
                .matches("erpx_collaboration_read.scope_contains(")
                .count()
                >= 3,
            "resolve, search, and backlinks must all apply lineage containment in SQL"
        );
    }

    #[test]
    fn directory_reads_bind_freshness_proof_and_payload_to_one_statement_snapshot() {
        let source = include_str!("directory.rs");
        let authorize = source
            .split("pub async fn authorize_record")
            .nth(1)
            .expect("authorize record")
            .split("pub async fn resolve")
            .next()
            .expect("authorize boundary");
        let resolve = source
            .split("pub async fn resolve")
            .nth(1)
            .expect("resolve")
            .split("pub async fn search")
            .next()
            .expect("resolve boundary");
        let search = source
            .split("pub async fn search")
            .nth(1)
            .expect("search")
            .split("pub async fn autocomplete")
            .next()
            .expect("search boundary");
        let autocomplete = source
            .split("pub async fn autocomplete")
            .nth(1)
            .expect("autocomplete")
            .split("pub async fn backlinks")
            .next()
            .expect("autocomplete boundary");
        let backlinks = source
            .split("pub async fn backlinks")
            .nth(1)
            .expect("backlinks")
            .split("pub async fn freshness")
            .next()
            .expect("backlinks boundary");

        for (name, operation) in [
            ("authorize_record", authorize),
            ("resolve", resolve),
            ("search", search),
            ("backlinks", backlinks),
        ] {
            assert_eq!(
                operation.matches("let rows = sqlx::query(").count()
                    + operation.matches("let row = sqlx::query(").count(),
                1,
                "{name} must select freshness, proof, and payload in one statement"
            );
            assert!(operation.contains("WITH projection AS MATERIALIZED"));
            assert!(operation.contains("projection.state='ready'"));
            assert!(
                operation.contains("projection.applied_position=projection.source_high_watermark")
            );
            assert!(operation.contains("projection.last_success_at BETWEEN"));
            assert!(
                operation.contains("descriptor.source_event_position<=projection.applied_position")
                    || operation
                        .contains("descriptor.source_event_position<=linked.applied_position")
            );
            assert!(
                operation.contains("grant_fact.source_event_position<=projection.applied_position")
                    || operation
                        .contains("grant_fact.source_event_position<=linked.applied_position")
            );
            assert!(
                operation.contains(
                    "descriptor.authorization_policy_id=grant_fact.authorization_policy_id"
                )
            );
            assert!(
                operation
                    .contains("descriptor.authorization_policy_version=grant_fact.policy_version")
            );
            assert!(
                operation
                    .contains("grant_fact.source_event_position>=descriptor.source_event_position")
            );
            assert!(operation.contains("erpx_collaboration_read.scope_contains("));
            assert!(!operation.contains("self.freshness("));
            assert!(!operation.contains("self.authorize_record("));
        }
        assert!(backlinks.contains("anchor_authorized AS MATERIALIZED"));
        assert!(backlinks.contains("FROM anchor_authorized"));
        assert_eq!(
            backlinks
                .matches("descriptor.source_event_position<=")
                .count(),
            2,
            "both anchor and linked descriptors must be checkpoint-fenced"
        );
        assert_eq!(
            backlinks
                .matches("grant_fact.source_event_position<=")
                .count(),
            2,
            "both anchor and linked grants must be checkpoint-fenced"
        );
        assert!(
            backlinks
                .contains("relation.source_event_position<=anchor_authorized.applied_position")
        );
        assert!(search.contains("authorized_count AS"));
        assert!(backlinks.contains("authorized_count AS"));
        assert_eq!(autocomplete.matches("let rows = sqlx::query(").count(), 1);
        assert!(autocomplete.contains("anchor_authorized AS MATERIALIZED"));
        assert!(autocomplete.contains("FROM anchor_authorized anchor"));
        assert!(
            autocomplete.contains("descriptor.source_event_position<=projection.applied_position")
        );
        assert!(
            autocomplete.contains("grant_fact.source_event_position<=projection.applied_position")
        );
        assert!(autocomplete.contains("p.source_global_sequence<=anchor.applied_position"));
        assert!(autocomplete.contains("a.source_global_sequence<=anchor.applied_position"));
        assert!(autocomplete.contains("d.source_event_position<=anchor.applied_position"));
        assert!(autocomplete.contains("grant_fact.source_event_position<=anchor.applied_position"));
        assert!(autocomplete.contains("a.definition_status='approved'"));
        assert!(autocomplete.contains("d.display_label IS NOT NULL"));
        assert!(!autocomplete.contains("self.resolve("));
        assert!(!autocomplete.contains("self.search("));
    }

    #[test]
    fn scope_containment_uses_the_active_canonical_organization_chain() {
        let ddl = include_str!(
            "../../../migration/migrations_holding_read/0002_record_collaboration_phase2.sql"
        );
        let function = ddl
            .split(
                "CREATE OR REPLACE FUNCTION erpx_collaboration_read.scope_contains(authority JSONB, target JSONB)",
            )
            .nth(1)
            .expect("scope containment function")
            .split("ALTER TABLE erpx_collaboration_read.item_directory_descriptors")
            .next()
            .expect("scope containment boundary");

        assert!(function.contains("LANGUAGE plpgsql\nSTABLE"));
        assert!(function.contains("erpx_authority_read.organization_nodes"));
        assert!(function.contains("candidate.node_kind IN ('department','team')"));
        assert!(function.contains("ancestor.node_status <> 'active'"));
        assert!(function.contains("ancestor.lineage_node_ids IS DISTINCT FROM"));
        assert!(function.contains("ancestor.node_kind IN ('department','team')"));
        assert!(!function.contains("company_ids"));
        assert!(!function.contains("branch_ids"));
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
    fn candidate_rebuild_is_resumable_and_clears_only_holding_local_candidate_rows() {
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
        assert!(rebuild.contains("WHERE holding_id=$1"));
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
