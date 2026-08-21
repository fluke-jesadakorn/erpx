//! Audited control-plane gateway for Platform-scoped Collaboration records.
//!
//! This is intentionally a separate adapter from the holding-local RLS store.
//! A Platform request can never acquire a synthetic `HoldingId` and can never
//! be passed to any holding-local RLS session setting.

use erpx_kernel::{ActorAttribution, ActorKind, AuthorityScope, CorrelationId, PrincipalId};
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    DirectoryAdapterError, DirectoryLookup, DirectorySearchPage, DirectorySearchRequest,
    ProjectionFreshness,
};
use erpx_collaboration::{PortError, RelationPage, RelationView};
use erpx_integration_contracts::{AuthorizedItemSummary, ItemDescriptor};
use erpx_kernel::RecordId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalCollaborationAccess {
    principal_id: PrincipalId,
    session_id: Uuid,
    correlation_id: CorrelationId,
}

impl GlobalCollaborationAccess {
    /// Only a directly authenticated Platform human can enter the global
    /// gateway. Represented/support actors and non-human kinds fail closed.
    pub fn try_from_authenticated_platform_human(
        attribution: &ActorAttribution,
        authority_scope: &AuthorityScope,
        correlation_id: CorrelationId,
    ) -> Result<Self, PortError> {
        attribution.validate().map_err(|_| PortError::Denied)?;
        if authority_scope != &AuthorityScope::Platform
            || attribution.kind != ActorKind::PlatformUser
            || attribution.represented_principal_ref.is_some()
            || attribution.support_capability_id.is_some()
        {
            return Err(PortError::Denied);
        }
        let principal_id = Uuid::parse_str(&attribution.principal_ref)
            .ok()
            .and_then(|value| PrincipalId::try_from_uuid(value).ok())
            .ok_or(PortError::Denied)?;
        let session_id = attribution
            .session_id
            .map(|session| session.as_uuid())
            .ok_or(PortError::Denied)?;
        Ok(Self {
            principal_id,
            session_id,
            correlation_id,
        })
    }

    pub const fn principal_id(&self) -> PrincipalId {
        self.principal_id
    }

    pub const fn session_id(&self) -> Uuid {
        self.session_id
    }

    pub const fn correlation_id(&self) -> CorrelationId {
        self.correlation_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalCollaborationOutcome {
    Allowed,
    Denied,
    Conflict,
    Unavailable,
}

impl GlobalCollaborationOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
            Self::Conflict => "conflict",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Explicit control write/read pools are injected by the server composition.
/// Holding database pools are not accepted by this type's constructor.
#[derive(Debug, Clone)]
pub struct SqlxGlobalCollaborationGateway {
    control_write: PgPool,
    control_read: PgPool,
}

impl SqlxGlobalCollaborationGateway {
    pub const fn from_control_plane(control_write: PgPool, control_read: PgPool) -> Self {
        Self {
            control_write,
            control_read,
        }
    }

    pub const fn control_write(&self) -> &PgPool {
        &self.control_write
    }

    pub const fn control_read(&self) -> &PgPool {
        &self.control_read
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn audit(
        &self,
        access: &GlobalCollaborationAccess,
        operation: &'static str,
        source_record_id: Option<Uuid>,
        target_record_id: Option<Uuid>,
        attachment_id: Option<Uuid>,
        command_id: Option<Uuid>,
        outcome: GlobalCollaborationOutcome,
    ) -> Result<(), PortError> {
        if !matches!(
            operation,
            "reference.resolve"
                | "reference.search"
                | "relation.list"
                | "relation.add"
                | "relation.revoke"
                | "thread.get"
                | "thread.list"
                | "thread.start"
                | "attachment.prepare"
                | "attachment.finalize"
                | "attachment.download"
        ) {
            return Err(PortError::InvalidState);
        }
        sqlx::query(
            "INSERT INTO erpx_global_collaboration.gateway_audit
             (audit_id,operation,actor_principal_id,actor_session_id,authority_scope,
              source_record_id,target_record_id,attachment_id,command_id,correlation_id,
              outcome,occurred_at)
             VALUES ($1,$2,$3,$4,to_jsonb('Platform'::TEXT),$5,$6,$7,$8,$9,$10,clock_timestamp())",
        )
        .bind(Uuid::new_v4())
        .bind(operation)
        .bind(access.principal_id().as_uuid())
        .bind(access.session_id())
        .bind(source_record_id)
        .bind(target_record_id)
        .bind(attachment_id)
        .bind(command_id)
        .bind(access.correlation_id().as_uuid())
        .bind(outcome.as_str())
        .execute(&self.control_write)
        .await
        .map_err(|_| PortError::Unavailable)?;
        Ok(())
    }

    /// Prove a full Platform record reference against the current descriptor
    /// and effective grant without returning descriptor payload.
    pub async fn authorize_record(
        &self,
        access: &GlobalCollaborationAccess,
        record: &erpx_integration_contracts::StableRecordRef,
        require_relate: bool,
    ) -> Result<(), PortError> {
        record.validate().map_err(|_| PortError::Denied)?;
        let exact_ref = serde_json::to_value(record).map_err(|_| PortError::Denied)?;
        let authorized = sqlx::query_scalar::<_, bool>(
            "WITH checkpoint AS MATERIALIZED (
               SELECT applied_position
               FROM erpx_global_collaboration_read.projection_state
               WHERE singleton=TRUE AND state='ready'
                 AND applied_position=source_high_watermark
                 AND last_success_at BETWEEN statement_timestamp()-INTERVAL '60 seconds'
                                         AND statement_timestamp()
             ), authorized AS MATERIALIZED (
               SELECT 1
               FROM checkpoint
               JOIN erpx_global_collaboration_read.descriptors descriptor
                 ON descriptor.record_id=$1 AND descriptor.record_ref=$2::JSONB
                AND descriptor.record_scope=to_jsonb('Platform'::TEXT)
                AND descriptor.tombstoned_at IS NULL
                AND descriptor.source_event_position<=checkpoint.applied_position
               JOIN erpx_global_collaboration_read.effective_grants grant_fact
                 ON grant_fact.record_id=descriptor.record_id
                AND grant_fact.principal_id=$3 AND grant_fact.can_view
                AND (NOT $4 OR grant_fact.can_relate)
                AND grant_fact.authorization_policy_id=descriptor.authorization_policy_id
                AND grant_fact.policy_version=descriptor.authorization_policy_version
                AND grant_fact.source_event_position>=descriptor.source_event_position
                AND grant_fact.source_event_position<=checkpoint.applied_position
                AND grant_fact.effective_at<=statement_timestamp()
                AND grant_fact.expires_at>statement_timestamp()
             )
             SELECT EXISTS (SELECT 1 FROM authorized)",
        )
        .bind(record.id().as_uuid())
        .bind(exact_ref)
        .bind(access.principal_id().as_uuid())
        .bind(require_relate)
        .fetch_one(&self.control_read)
        .await
        .map_err(|_| PortError::Unavailable)?;
        authorized.then_some(()).ok_or(PortError::Denied)
    }

    /// Resolve remains available to legacy NULL-label descriptors. Current
    /// authorization, policy version and checkpoint are evaluated before the
    /// descriptor is returned in the same control-read snapshot.
    pub async fn resolve_reference(
        &self,
        access: &GlobalCollaborationAccess,
        record_id: RecordId,
    ) -> Result<DirectoryLookup<ItemDescriptor>, DirectoryAdapterError> {
        let row = sqlx::query(
            "WITH checkpoint AS MATERIALIZED (
               SELECT state,source_high_watermark,applied_position,last_success_at,
                      COALESCE(state='ready' AND applied_position=source_high_watermark
                       AND last_success_at BETWEEN statement_timestamp()-INTERVAL '60 seconds'
                                               AND statement_timestamp(),FALSE) AS fresh
               FROM erpx_global_collaboration_read.projection_state WHERE singleton=TRUE
             ), authorized AS MATERIALIZED (
               SELECT descriptor.descriptor
               FROM checkpoint
               JOIN erpx_global_collaboration_read.descriptors descriptor
                 ON descriptor.record_id=$1 AND descriptor.record_scope=to_jsonb('Platform'::TEXT)
                AND descriptor.tombstoned_at IS NULL
                AND descriptor.source_event_position<=checkpoint.applied_position
               JOIN erpx_global_collaboration_read.effective_grants grant_fact
                 ON grant_fact.record_id=descriptor.record_id AND grant_fact.principal_id=$2
                AND grant_fact.can_view
                AND grant_fact.authorization_policy_id=descriptor.authorization_policy_id
                AND grant_fact.policy_version=descriptor.authorization_policy_version
                AND grant_fact.source_event_position>=descriptor.source_event_position
                AND grant_fact.source_event_position<=checkpoint.applied_position
                AND grant_fact.effective_at<=statement_timestamp()
                AND grant_fact.expires_at>statement_timestamp()
               WHERE checkpoint.fresh
             )
             SELECT checkpoint.fresh,authorized.descriptor
             FROM checkpoint LEFT JOIN authorized ON TRUE",
        )
        .bind(record_id.as_uuid())
        .bind(access.principal_id().as_uuid())
        .fetch_optional(&self.control_read)
        .await
        .map_err(|_| DirectoryAdapterError::Unavailable)?;
        let Some(row) = row else {
            return Ok(DirectoryLookup::Unavailable);
        };
        if !row.try_get::<bool, _>("fresh").unwrap_or(false) {
            return Ok(DirectoryLookup::Degraded(ProjectionFreshness::Stale));
        }
        let Some(document) = row
            .try_get::<Option<serde_json::Value>, _>("descriptor")
            .map_err(|_| DirectoryAdapterError::Unavailable)?
        else {
            return Ok(DirectoryLookup::Neutral);
        };
        let descriptor = serde_json::from_value::<ItemDescriptor>(document)
            .map_err(|_| DirectoryAdapterError::Unavailable)?;
        descriptor
            .validate()
            .map_err(|_| DirectoryAdapterError::Unavailable)?;
        if descriptor.record().id() != record_id
            || descriptor.scope() != &erpx_kernel::AuthorityScope::Platform
        {
            return Ok(DirectoryLookup::Unavailable);
        }
        Ok(DirectoryLookup::Visible {
            value: descriptor,
            freshness: ProjectionFreshness::Fresh,
        })
    }

    /// Picker search excludes legacy descriptors without an owner-published
    /// display label before match/count/page are observable.
    pub async fn search_references(
        &self,
        access: &GlobalCollaborationAccess,
        request: &DirectorySearchRequest,
    ) -> Result<DirectoryLookup<DirectorySearchPage>, DirectoryAdapterError> {
        request.validate()?;
        let offset = request
            .cursor
            .as_deref()
            .map(str::parse::<i64>)
            .transpose()
            .map_err(|_| DirectoryAdapterError::Invalid("cursor"))?
            .unwrap_or(0);
        let rows = sqlx::query(
            "WITH checkpoint AS MATERIALIZED (
               SELECT state,source_high_watermark,applied_position,last_success_at,
                      COALESCE(state='ready' AND applied_position=source_high_watermark
                       AND last_success_at BETWEEN statement_timestamp()-INTERVAL '60 seconds'
                                               AND statement_timestamp(),FALSE) AS fresh
               FROM erpx_global_collaboration_read.projection_state WHERE singleton=TRUE
             ), authorized AS MATERIALIZED (
               SELECT descriptor.descriptor,descriptor.search_vector,
                      descriptor.source_event_position,descriptor.record_id
               FROM checkpoint
               JOIN erpx_global_collaboration_read.descriptors descriptor
                 ON descriptor.record_scope=to_jsonb('Platform'::TEXT)
                AND descriptor.tombstoned_at IS NULL AND descriptor.display_label IS NOT NULL
                AND descriptor.source_event_position<=checkpoint.applied_position
               JOIN erpx_global_collaboration_read.effective_grants grant_fact
                 ON grant_fact.record_id=descriptor.record_id AND grant_fact.principal_id=$1
                AND grant_fact.can_view
                AND grant_fact.authorization_policy_id=descriptor.authorization_policy_id
                AND grant_fact.policy_version=descriptor.authorization_policy_version
                AND grant_fact.source_event_position>=descriptor.source_event_position
                AND grant_fact.source_event_position<=checkpoint.applied_position
                AND grant_fact.effective_at<=statement_timestamp()
                AND grant_fact.expires_at>statement_timestamp()
               WHERE checkpoint.fresh
             ), matching AS MATERIALIZED (
               SELECT descriptor,source_event_position,record_id FROM authorized
               WHERE search_vector @@ websearch_to_tsquery('simple',$2)
             ), authorized_count AS MATERIALIZED (
               SELECT count(*)::BIGINT AS authorized_count FROM matching
             ), page AS MATERIALIZED (
               SELECT descriptor FROM matching
               ORDER BY source_event_position DESC,record_id LIMIT $3 OFFSET $4
             )
             SELECT checkpoint.fresh,authorized_count.authorized_count,page.descriptor
             FROM checkpoint CROSS JOIN authorized_count LEFT JOIN page ON TRUE",
        )
        .bind(access.principal_id().as_uuid())
        .bind(&request.query)
        .bind(i64::from(request.page_size))
        .bind(offset)
        .fetch_all(&self.control_read)
        .await
        .map_err(|_| DirectoryAdapterError::Unavailable)?;
        let Some(first) = rows.first() else {
            return Ok(DirectoryLookup::Unavailable);
        };
        if !first.try_get::<bool, _>("fresh").unwrap_or(false) {
            return Ok(DirectoryLookup::Degraded(ProjectionFreshness::Stale));
        }
        let authorized_count = u64::try_from(
            first
                .try_get::<i64, _>("authorized_count")
                .map_err(|_| DirectoryAdapterError::Unavailable)?,
        )
        .map_err(|_| DirectoryAdapterError::Unavailable)?;
        let items = rows
            .iter()
            .filter_map(
                |row| match row.try_get::<Option<serde_json::Value>, _>("descriptor") {
                    Ok(Some(value)) => Some(
                        serde_json::from_value::<ItemDescriptor>(value)
                            .map_err(|_| DirectoryAdapterError::Unavailable),
                    ),
                    Ok(None) => None,
                    Err(_) => Some(Err(DirectoryAdapterError::Unavailable)),
                },
            )
            .collect::<Result<Vec<_>, _>>()?;
        let authorized_summaries = items
            .iter()
            .map(AuthorizedItemSummary::try_from_descriptor)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| DirectoryAdapterError::Unavailable)?;
        let consumed = offset
            .checked_add(
                i64::try_from(items.len()).map_err(|_| DirectoryAdapterError::Unavailable)?,
            )
            .ok_or(DirectoryAdapterError::Unavailable)?;
        Ok(DirectoryLookup::Visible {
            value: DirectorySearchPage {
                items,
                authorized_summaries,
                next_cursor: (u64::try_from(consumed)
                    .map_err(|_| DirectoryAdapterError::Unavailable)?
                    < authorized_count)
                    .then(|| consumed.to_string()),
                authorized_count,
            },
            freshness: ProjectionFreshness::Fresh,
        })
    }

    /// Lists both incoming and outgoing Platform relations. The anchor and the
    /// opposite endpoint grant are authorized before count/page/payload in one
    /// current projection snapshot.
    pub async fn list_relations(
        &self,
        access: &GlobalCollaborationAccess,
        anchor: &erpx_integration_contracts::StableRecordRef,
        include_revoked: bool,
        page_size: u16,
        cursor: Option<&str>,
    ) -> Result<RelationPage, PortError> {
        if page_size == 0 || page_size > 100 {
            return Err(PortError::InvalidState);
        }
        let offset = cursor
            .map(str::parse::<i64>)
            .transpose()
            .map_err(|_| PortError::InvalidState)?
            .unwrap_or(0);
        let anchor_json = serde_json::to_value(anchor).map_err(|_| PortError::InvalidState)?;
        let rows = sqlx::query(
            "WITH checkpoint AS MATERIALIZED (
               SELECT applied_position,
                      COALESCE(state='ready' AND applied_position=source_high_watermark
                       AND last_success_at BETWEEN statement_timestamp()-INTERVAL '60 seconds'
                                               AND statement_timestamp(),FALSE) AS fresh
               FROM erpx_global_collaboration_read.projection_state WHERE singleton=TRUE
             ), anchor_authorized AS MATERIALIZED (
               SELECT checkpoint.applied_position
               FROM checkpoint
               JOIN erpx_global_collaboration_read.descriptors descriptor
                 ON descriptor.record_id=$1 AND descriptor.record_ref=$2::JSONB
                AND descriptor.record_scope=to_jsonb('Platform'::TEXT)
                AND descriptor.tombstoned_at IS NULL
                AND descriptor.source_event_position<=checkpoint.applied_position
               JOIN erpx_global_collaboration_read.effective_grants grant_fact
                 ON grant_fact.record_id=descriptor.record_id AND grant_fact.principal_id=$3
                AND grant_fact.can_view AND grant_fact.can_relate
                AND grant_fact.authorization_policy_id=descriptor.authorization_policy_id
                AND grant_fact.policy_version=descriptor.authorization_policy_version
                AND grant_fact.source_event_position>=descriptor.source_event_position
                AND grant_fact.source_event_position<=checkpoint.applied_position
                AND grant_fact.effective_at<=statement_timestamp()
                AND grant_fact.expires_at>statement_timestamp()
               WHERE checkpoint.fresh
             ), candidates AS MATERIALIZED (
               SELECT relation.* FROM anchor_authorized
               JOIN erpx_global_collaboration_read.relations relation
                 ON ((relation.source_record_id=$1 AND relation.source_record_ref=$2::JSONB)
                  OR (relation.target_record_id=$1 AND relation.target_record_ref=$2::JSONB))
                AND relation.committed_sequence<=anchor_authorized.applied_position
                AND ($4 OR relation.revoked_at IS NULL)
             ), authorized AS MATERIALIZED (
               SELECT candidate.relation_id,candidate.relation_document,candidate.committed_sequence
               FROM candidates candidate JOIN anchor_authorized ON TRUE
               JOIN erpx_global_collaboration_read.descriptors descriptor
                 ON descriptor.record_id=CASE WHEN candidate.source_record_id=$1
                    THEN candidate.target_record_id ELSE candidate.source_record_id END
                AND descriptor.record_ref=CASE WHEN candidate.source_record_id=$1
                    THEN candidate.target_record_ref ELSE candidate.source_record_ref END
                AND descriptor.record_scope=to_jsonb('Platform'::TEXT)
                AND descriptor.tombstoned_at IS NULL
                AND descriptor.source_event_position<=anchor_authorized.applied_position
               JOIN erpx_global_collaboration_read.effective_grants grant_fact
                 ON grant_fact.record_id=descriptor.record_id AND grant_fact.principal_id=$3
                AND grant_fact.can_view AND grant_fact.can_relate
                AND grant_fact.authorization_policy_id=descriptor.authorization_policy_id
                AND grant_fact.policy_version=descriptor.authorization_policy_version
                AND grant_fact.source_event_position>=descriptor.source_event_position
                AND grant_fact.source_event_position<=anchor_authorized.applied_position
                AND grant_fact.effective_at<=statement_timestamp()
                AND grant_fact.expires_at>statement_timestamp()
             ), authorized_count AS MATERIALIZED (
               SELECT count(*)::BIGINT AS count FROM authorized
             ), page AS MATERIALIZED (
               SELECT relation_id,relation_document,committed_sequence FROM authorized
               ORDER BY committed_sequence,relation_id LIMIT $5 OFFSET $6
             )
             SELECT checkpoint.fresh,checkpoint.applied_position,authorized_count.count,
                    page.relation_document,page.committed_sequence
             FROM checkpoint CROSS JOIN authorized_count LEFT JOIN page ON checkpoint.fresh",
        )
        .bind(anchor.id().as_uuid())
        .bind(anchor_json)
        .bind(access.principal_id().as_uuid())
        .bind(include_revoked)
        .bind(i64::from(page_size))
        .bind(offset)
        .fetch_all(&self.control_read)
        .await
        .map_err(|_| PortError::Unavailable)?;
        let Some(first) = rows.first() else {
            return Err(PortError::Unavailable);
        };
        if !first.try_get::<bool, _>("fresh").unwrap_or(false) {
            return Err(PortError::Stale);
        }
        let projection_checkpoint = u64::try_from(
            first
                .try_get::<i64, _>("applied_position")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        let authorized_count = u64::try_from(
            first
                .try_get::<i64, _>("count")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        let mut items = Vec::new();
        for row in rows {
            let Some(document) = row
                .try_get::<Option<serde_json::Value>, _>("relation_document")
                .map_err(|_| PortError::Internal)?
            else {
                continue;
            };
            let mut relation = serde_json::from_value::<RelationView>(document)
                .map_err(|_| PortError::Internal)?;
            relation.committed_sequence = u64::try_from(
                row.try_get::<i64, _>("committed_sequence")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?;
            relation.validate().map_err(|_| PortError::Internal)?;
            items.push(relation);
        }
        let consumed = offset
            .checked_add(i64::try_from(items.len()).map_err(|_| PortError::Internal)?)
            .ok_or(PortError::Internal)?;
        Ok(RelationPage {
            items,
            next_cursor: (u64::try_from(consumed).map_err(|_| PortError::Internal)?
                < authorized_count)
                .then(|| consumed.to_string()),
            authorized_count,
            projection_checkpoint,
            stale: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_kernel::SessionId;

    fn attribution(kind: ActorKind) -> ActorAttribution {
        ActorAttribution {
            principal_ref: Uuid::from_u128(1).to_string(),
            kind,
            session_id: Some(SessionId::try_from_uuid(Uuid::from_u128(2)).expect("session")),
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }

    #[test]
    fn global_gateway_accepts_only_direct_platform_humans() {
        let correlation = CorrelationId::try_from_uuid(Uuid::from_u128(3)).expect("correlation");
        assert!(
            GlobalCollaborationAccess::try_from_authenticated_platform_human(
                &attribution(ActorKind::PlatformUser),
                &AuthorityScope::Platform,
                correlation,
            )
            .is_ok()
        );
        for kind in [
            ActorKind::Agent,
            ActorKind::Service,
            ActorKind::SystemWorker,
        ] {
            assert!(
                GlobalCollaborationAccess::try_from_authenticated_platform_human(
                    &attribution(kind),
                    &AuthorityScope::Platform,
                    correlation,
                )
                .is_err()
            );
        }
        assert!(
            GlobalCollaborationAccess::try_from_authenticated_platform_human(
                &attribution(ActorKind::HoldingUser),
                &AuthorityScope::Holding {
                    holding_id: Uuid::from_u128(4),
                },
                correlation,
            )
            .is_err()
        );
    }

    #[test]
    fn global_gateway_source_never_binds_a_holding_rls_setting() {
        let source = include_str!("global.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(!production.contains("app.holding_id"));
        assert!(!production.contains("SqlxItemDirectory"));
        assert!(production.contains("gateway_audit"));
        assert!(production.contains("descriptor.record_ref=$2::JSONB"));
    }
}
