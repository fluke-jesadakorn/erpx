//! Holding-local Phase 2 record-relation graph persistence and projection.

use async_trait::async_trait;
use erpx_collaboration::{
    AuthorizedRelationRecordAccess, CollaborationAction, PortError, RecordRelationDraft,
    RecordRelationGraph, RecordRelationReadRepository, RecordRelationRepository, RelationDraftKind,
    RelationGraphCommit, RelationMutationReceipt, RelationPage, RelationView,
    record_relation_graph_thread_id,
};
use erpx_cqrs_sqlx::ProjectionEvent;
use erpx_kernel::{ActorKind, HoldingId, PrincipalKind};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::DIRECTORY_FRESHNESS_SECONDS;

pub const RECORD_RELATION_DRAFT_EVENT_TYPE: &str = "collaboration.record_relation_draft.v1";

#[derive(Debug, Clone)]
pub struct SqlxRecordRelationRepository {
    write_pool: PgPool,
    read_pool: PgPool,
}

impl SqlxRecordRelationRepository {
    pub const fn new(write_pool: PgPool, read_pool: PgPool) -> Self {
        Self {
            write_pool,
            read_pool,
        }
    }

    fn validate_access(
        source: &AuthorizedRelationRecordAccess,
        target: Option<&AuthorizedRelationRecordAccess>,
    ) -> Result<(Uuid, Uuid), PortError> {
        if !matches!(
            source.action(),
            CollaborationAction::RelateRecord
                | CollaborationAction::RevokeRelation
                | CollaborationAction::ListRecordRelations
        ) || source.external_portal().is_some()
            || source.actor().kind() != PrincipalKind::Human
            || !matches!(
                source.attribution().kind,
                ActorKind::HoldingUser | ActorKind::PlatformUser
            )
            || source.attribution().represented_principal_ref.is_some()
            || source.attribution().support_capability_id.is_some()
        {
            return Err(PortError::Denied);
        }
        let principal_id =
            Uuid::parse_str(&source.attribution().principal_ref).map_err(|_| PortError::Denied)?;
        if principal_id != source.actor().id().as_uuid() {
            return Err(PortError::Denied);
        }
        let session_id = source
            .attribution()
            .session_id
            .map(|session| session.as_uuid())
            .ok_or(PortError::Denied)?;
        if let Some(target) = target
            && (target.holding_id() != source.holding_id()
                || target.actor() != source.actor()
                || target.attribution() != source.attribution()
                || target.authority_scope() != source.authority_scope()
                || target.external_portal() != source.external_portal()
                || target.action() != source.action())
        {
            return Err(PortError::Denied);
        }
        Ok((principal_id, session_id))
    }
}

#[async_trait]
impl RecordRelationRepository for SqlxRecordRelationRepository {
    async fn load_graph(
        &self,
        source: &AuthorizedRelationRecordAccess,
    ) -> Result<Option<RecordRelationGraph>, PortError> {
        Self::validate_access(source, None)?;
        let mut tx = self.write_pool.begin().await.map_err(database)?;
        set_holding(&mut tx, source.holding_id()).await?;
        let graph = load_graph_in(&mut tx, source, false).await?;
        tx.commit().await.map_err(database)?;
        Ok(graph)
    }

    async fn commit_graph(
        &self,
        source: &AuthorizedRelationRecordAccess,
        target: &AuthorizedRelationRecordAccess,
        commit: RelationGraphCommit,
    ) -> Result<RelationMutationReceipt, PortError> {
        let (actor_id, session_id) = Self::validate_access(source, Some(target))?;
        commit.identity.validate()?;
        commit
            .graph
            .validate_loaded()
            .map_err(|_| PortError::InvalidState)?;
        commit.draft.validate()?;
        if commit.draft.holding_id != source.holding_id()
            || commit.graph.source() != source.record()
            || commit.draft.relation.source != *source.record()
            || commit.draft.relation.target != *target.record()
            || commit.graph.version() != commit.expected_version.saturating_add(1)
        {
            return Err(PortError::Denied);
        }
        match commit.draft.kind {
            RelationDraftKind::Added if commit.draft.relation.created_by != source.actor() => {
                return Err(PortError::Denied);
            }
            RelationDraftKind::Revoked
                if commit.draft.relation.revoked_by != Some(source.actor()) =>
            {
                return Err(PortError::Denied);
            }
            _ => {}
        }
        let graph_thread_id = record_relation_graph_thread_id(source.record())
            .map_err(|_| PortError::InvalidState)?;
        let mut tx = self.write_pool.begin().await.map_err(database)?;
        set_holding(&mut tx, source.holding_id()).await?;

        if let Some(row) = sqlx::query(
            "SELECT request_digest,result_relation_document
             FROM erpx_collaboration.record_relation_command_receipts
             WHERE holding_id=$1 AND idempotency_key=$2",
        )
        .bind(source.holding_id().as_uuid())
        .bind(&commit.identity.idempotency_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?
        {
            let digest: Vec<u8> = row
                .try_get("request_digest")
                .map_err(|_| PortError::Internal)?;
            if digest.as_slice() != commit.identity.request_digest {
                return Err(PortError::Conflict);
            }
            let relation = serde_json::from_value::<RelationView>(
                row.try_get("result_relation_document")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?;
            relation.validate().map_err(|_| PortError::Internal)?;
            tx.commit().await.map_err(database)?;
            return Ok(RelationMutationReceipt {
                relation,
                replayed: true,
            });
        }

        let current = load_graph_in(&mut tx, source, true).await?;
        match current {
            Some(current)
                if current.version() == commit.expected_version
                    && current.source() == source.record() => {}
            None if commit.expected_version == 0 => {}
            _ => return Err(PortError::Conflict),
        }

        let source_json =
            serde_json::to_value(source.record()).map_err(|_| PortError::InvalidState)?;
        let target_json =
            serde_json::to_value(target.record()).map_err(|_| PortError::InvalidState)?;
        let graph_version = to_i64(commit.graph.version())?;
        if commit.expected_version == 0 {
            let changed = sqlx::query(
                "INSERT INTO erpx_collaboration.record_relation_graphs
                 (holding_id,source_record_id,source_record_ref,graph_thread_id,
                  aggregate_version,created_at,updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$6) ON CONFLICT DO NOTHING",
            )
            .bind(source.holding_id().as_uuid())
            .bind(source.record().id().as_uuid())
            .bind(&source_json)
            .bind(graph_thread_id.as_uuid())
            .bind(graph_version)
            .bind(commit.draft.relation.created_at)
            .execute(&mut *tx)
            .await
            .map_err(database)?
            .rows_affected();
            if changed != 1 {
                return Err(PortError::Conflict);
            }
        } else {
            let changed = sqlx::query(
                "UPDATE erpx_collaboration.record_relation_graphs
                 SET aggregate_version=$5,updated_at=$6
                 WHERE holding_id=$1 AND source_record_id=$2
                   AND source_record_ref=$3::JSONB AND graph_thread_id=$4
                   AND aggregate_version=$7",
            )
            .bind(source.holding_id().as_uuid())
            .bind(source.record().id().as_uuid())
            .bind(&source_json)
            .bind(graph_thread_id.as_uuid())
            .bind(graph_version)
            .bind(relation_time(&commit.draft))
            .bind(to_i64(commit.expected_version)?)
            .execute(&mut *tx)
            .await
            .map_err(database)?
            .rows_affected();
            if changed != 1 {
                return Err(PortError::Conflict);
            }
        }

        let relation = &commit.draft.relation;
        let relation_document =
            serde_json::to_value(relation).map_err(|_| PortError::InvalidState)?;
        let relation_kind = relation_kind_name(&relation.kind)?;
        let changed = match commit.draft.kind {
            RelationDraftKind::Added => sqlx::query(
                "INSERT INTO erpx_collaboration.record_relations
                 (holding_id,relation_id,source_record_id,target_record_id,source_record_ref,
                  target_record_ref,graph_thread_id,relation_kind,relation_version,graph_version,
                  relation_document,relation_state,created_by_principal_id,created_by_session_id,
                  created_at,revoked_by_principal_id,revoked_by_session_id,revoked_at,updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$9,$10,'active',$11,$12,$13,NULL,NULL,NULL,$13)",
            )
            .bind(source.holding_id().as_uuid())
            .bind(relation.id.as_uuid())
            .bind(relation.source.id().as_uuid())
            .bind(relation.target.id().as_uuid())
            .bind(&source_json)
            .bind(&target_json)
            .bind(graph_thread_id.as_uuid())
            .bind(relation_kind)
            .bind(to_i64(relation.version)?)
            .bind(&relation_document)
            .bind(actor_id)
            .bind(session_id)
            .bind(relation.created_at)
            .execute(&mut *tx)
            .await,
            RelationDraftKind::Revoked => sqlx::query(
                "UPDATE erpx_collaboration.record_relations
                 SET relation_version=$9,graph_version=$9,relation_document=$10,
                     relation_state='revoked',revoked_by_principal_id=$11,
                     revoked_by_session_id=$12,revoked_at=$13,updated_at=$13
                 WHERE holding_id=$1 AND relation_id=$2 AND source_record_id=$3
                   AND target_record_id=$4 AND source_record_ref=$5::JSONB
                   AND target_record_ref=$6::JSONB AND graph_thread_id=$7
                   AND relation_kind=$8 AND relation_state='active'",
            )
            .bind(source.holding_id().as_uuid())
            .bind(relation.id.as_uuid())
            .bind(relation.source.id().as_uuid())
            .bind(relation.target.id().as_uuid())
            .bind(&source_json)
            .bind(&target_json)
            .bind(graph_thread_id.as_uuid())
            .bind(relation_kind)
            .bind(to_i64(relation.version)?)
            .bind(&relation_document)
            .bind(actor_id)
            .bind(session_id)
            .bind(relation.revoked_at.ok_or(PortError::InvalidState)?)
            .execute(&mut *tx)
            .await,
        }
        .map_err(database)?
        .rows_affected();
        if changed != 1 {
            return Err(PortError::Conflict);
        }

        let event_id = Uuid::new_v4();
        let event_kind = match commit.draft.kind {
            RelationDraftKind::Added => "added",
            RelationDraftKind::Revoked => "revoked",
        };
        let at = relation_time(&commit.draft);
        sqlx::query(
            "INSERT INTO erpx_collaboration.record_relation_events
             (holding_id,event_id,relation_id,source_record_id,graph_thread_id,graph_version,
              event_kind,event_document,actor_principal_id,actor_session_id,command_id,
              correlation_id,occurred_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
        )
        .bind(source.holding_id().as_uuid())
        .bind(event_id)
        .bind(relation.id.as_uuid())
        .bind(relation.source.id().as_uuid())
        .bind(graph_thread_id.as_uuid())
        .bind(graph_version)
        .bind(event_kind)
        .bind(&relation_document)
        .bind(actor_id)
        .bind(session_id)
        .bind(commit.identity.command_id.as_uuid())
        .bind(commit.identity.correlation_id.as_uuid())
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(database)?;

        let draft_document =
            serde_json::to_value(&commit.draft).map_err(|_| PortError::InvalidState)?;
        let actor_document =
            serde_json::to_value(source.attribution()).map_err(|_| PortError::InvalidState)?;
        let scope_document =
            serde_json::to_value(source.authority_scope()).map_err(|_| PortError::InvalidState)?;
        let committed_sequence: i64 = sqlx::query_scalar(
            "INSERT INTO erpx_cqrs_write.outbox_events
             (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,
              aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,
              causation_id,payload,occurred_at,available_at)
             VALUES ($1,$2,'collaboration',$3,$4,$5,1,'record_relation_graph',$6,$7,
                     $8,$9,$10,NULL,$11,$12,$12)
             RETURNING global_sequence",
        )
        .bind(source.holding_id().as_uuid())
        .bind(event_id)
        .bind(source.record().id().to_string())
        .bind(commit.identity.command_id.as_uuid())
        .bind(RECORD_RELATION_DRAFT_EVENT_TYPE)
        .bind(source.record().id().as_uuid())
        .bind(graph_version)
        .bind(scope_document)
        .bind(actor_document)
        .bind(commit.identity.correlation_id.as_uuid())
        .bind(&draft_document)
        .bind(at)
        .fetch_one(&mut *tx)
        .await
        .map_err(database)?;

        sqlx::query(
            "INSERT INTO erpx_collaboration.record_relation_publication_drafts
             (holding_id,event_id,relation_id,source_record_id,graph_version,fact_document,
              committed_sequence,projected_at,created_at)
             VALUES ($1,$2,$3,$4,$5,$6,NULL,NULL,$7)",
        )
        .bind(source.holding_id().as_uuid())
        .bind(event_id)
        .bind(relation.id.as_uuid())
        .bind(relation.source.id().as_uuid())
        .bind(graph_version)
        .bind(&draft_document)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(database)?;

        let mut receipt_relation = relation.clone();
        receipt_relation.committed_sequence = to_u64(committed_sequence)?;
        let receipt_document =
            serde_json::to_value(&receipt_relation).map_err(|_| PortError::InvalidState)?;
        sqlx::query(
            "INSERT INTO erpx_collaboration.record_relation_command_receipts
             (holding_id,idempotency_key,command_id,request_digest,source_record_id,
              relation_id,graph_version,event_id,result_relation_document,created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(source.holding_id().as_uuid())
        .bind(&commit.identity.idempotency_key)
        .bind(commit.identity.command_id.as_uuid())
        .bind(commit.identity.request_digest.to_vec())
        .bind(source.record().id().as_uuid())
        .bind(relation.id.as_uuid())
        .bind(graph_version)
        .bind(event_id)
        .bind(receipt_document)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(database)?;
        tx.commit().await.map_err(database)?;
        Ok(RelationMutationReceipt {
            relation: receipt_relation,
            replayed: false,
        })
    }
}

#[async_trait]
impl RecordRelationReadRepository for SqlxRecordRelationRepository {
    async fn list_relations(
        &self,
        source: &AuthorizedRelationRecordAccess,
        include_revoked: bool,
        page_size: u16,
        cursor: Option<&str>,
    ) -> Result<RelationPage, PortError> {
        Self::validate_access(source, None)?;
        let offset = cursor
            .map(str::parse::<i64>)
            .transpose()
            .map_err(|_| PortError::InvalidState)?
            .unwrap_or(0);
        if offset < 0 || page_size == 0 || page_size > 100 {
            return Err(PortError::InvalidState);
        }
        let source_json =
            serde_json::to_value(source.record()).map_err(|_| PortError::InvalidState)?;
        let mut tx = self.read_pool.begin().await.map_err(database)?;
        set_holding(&mut tx, source.holding_id()).await?;
        let rows = sqlx::query(
            "WITH checkpoint AS MATERIALIZED (
               SELECT state,source_high_watermark,applied_position,last_success_at,
                      COALESCE(state='ready' AND applied_position=source_high_watermark
                       AND last_success_at BETWEEN statement_timestamp()-($7::BIGINT*INTERVAL '1 second')
                                               AND statement_timestamp(),FALSE) AS fresh
               FROM erpx_collaboration_read.item_directory_projection_state
               WHERE holding_id=$1
             ), source_authorized AS MATERIALIZED (
               SELECT checkpoint.applied_position
               FROM checkpoint
               JOIN erpx_collaboration_read.item_directory_descriptors descriptor
                 ON descriptor.holding_id=$1 AND descriptor.record_id=$2
                AND descriptor.descriptor->'record'=$3::JSONB
                AND descriptor.tombstoned_at IS NULL
                AND descriptor.source_event_position<=checkpoint.applied_position
               JOIN erpx_collaboration_read.item_directory_effective_grants grant_fact
                 ON grant_fact.holding_id=descriptor.holding_id
                AND grant_fact.record_id=descriptor.record_id
                AND grant_fact.principal_id=$4 AND grant_fact.can_view
                AND grant_fact.authorization_policy_id=descriptor.authorization_policy_id
                AND grant_fact.policy_version=descriptor.authorization_policy_version
                AND grant_fact.source_event_position>=descriptor.source_event_position
                AND grant_fact.source_event_position<=checkpoint.applied_position
                AND grant_fact.effective_at<=statement_timestamp()
                AND grant_fact.expires_at>statement_timestamp()
               WHERE checkpoint.fresh
                 AND erpx_collaboration_read.scope_contains($5,descriptor.record_scope)
             ), candidates AS MATERIALIZED (
               SELECT relation.*
               FROM source_authorized
               JOIN erpx_collaboration_read.record_relation_projection relation
                 ON relation.holding_id=$1
                AND ((relation.source_record_id=$2 AND relation.source_record_ref=$3::JSONB)
                  OR (relation.target_record_id=$2 AND relation.target_record_ref=$3::JSONB))
                AND relation.committed_sequence<=source_authorized.applied_position
                AND ($9 OR relation.revoked_at IS NULL)
             ), authorized AS MATERIALIZED (
               SELECT candidate.relation_id,candidate.relation_document,
                      candidate.committed_sequence
               FROM candidates candidate
               JOIN source_authorized ON TRUE
               JOIN erpx_collaboration_read.item_directory_descriptors descriptor
                 ON descriptor.holding_id=$1
                AND descriptor.record_id=CASE WHEN candidate.source_record_id=$2
                     THEN candidate.target_record_id ELSE candidate.source_record_id END
                AND descriptor.descriptor->'record'=CASE WHEN candidate.source_record_id=$2
                     THEN candidate.target_record_ref ELSE candidate.source_record_ref END
                AND descriptor.tombstoned_at IS NULL
                AND descriptor.source_event_position<=source_authorized.applied_position
               JOIN erpx_collaboration_read.item_directory_effective_grants grant_fact
                 ON grant_fact.holding_id=descriptor.holding_id
                AND grant_fact.record_id=descriptor.record_id
                AND grant_fact.principal_id=$4 AND grant_fact.can_view
                AND grant_fact.authorization_policy_id=descriptor.authorization_policy_id
                AND grant_fact.policy_version=descriptor.authorization_policy_version
                AND grant_fact.source_event_position>=descriptor.source_event_position
                AND grant_fact.source_event_position<=source_authorized.applied_position
                AND grant_fact.effective_at<=statement_timestamp()
                AND grant_fact.expires_at>statement_timestamp()
               WHERE erpx_collaboration_read.scope_contains($5,descriptor.record_scope)
             ), authorized_count AS MATERIALIZED (
               SELECT count(*)::BIGINT AS count FROM authorized
             ), page AS MATERIALIZED (
               SELECT relation_id,relation_document,committed_sequence FROM authorized
               ORDER BY committed_sequence,relation_id LIMIT $6 OFFSET $8
             )
             SELECT checkpoint.fresh,checkpoint.applied_position,authorized_count.count,
                    page.relation_document,page.committed_sequence
             FROM checkpoint CROSS JOIN authorized_count LEFT JOIN page ON checkpoint.fresh",
        )
        .bind(source.holding_id().as_uuid())
        .bind(source.record().id().as_uuid())
        .bind(source_json)
        .bind(source.actor().id().as_uuid())
        .bind(serde_json::to_value(source.authority_scope()).map_err(|_| PortError::InvalidState)?)
        .bind(i64::from(page_size))
        .bind(DIRECTORY_FRESHNESS_SECONDS)
        .bind(offset)
        .bind(include_revoked)
        .fetch_all(&mut *tx)
        .await
        .map_err(database)?;
        tx.commit().await.map_err(database)?;
        let Some(first) = rows.first() else {
            return Err(PortError::Unavailable);
        };
        let fresh: bool = first.try_get("fresh").map_err(|_| PortError::Internal)?;
        if !fresh {
            return Err(PortError::Stale);
        }
        let projection_checkpoint = to_u64(
            first
                .try_get::<i64, _>("applied_position")
                .map_err(|_| PortError::Internal)?,
        )?;
        let authorized_count = to_u64(
            first
                .try_get::<i64, _>("count")
                .map_err(|_| PortError::Internal)?,
        )?;
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
            relation.committed_sequence = to_u64(
                row.try_get::<i64, _>("committed_sequence")
                    .map_err(|_| PortError::Internal)?,
            )?;
            relation.validate().map_err(|_| PortError::Internal)?;
            items.push(relation);
        }
        let consumed = offset
            .checked_add(i64::try_from(items.len()).map_err(|_| PortError::Internal)?)
            .ok_or(PortError::Internal)?;
        Ok(RelationPage {
            items,
            next_cursor: (to_u64(consumed)? < authorized_count).then(|| consumed.to_string()),
            authorized_count,
            projection_checkpoint,
            stale: false,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SqlxRecordRelationProjection {
    holding_id: HoldingId,
    write_pool: PgPool,
    read_pool: PgPool,
}

impl SqlxRecordRelationProjection {
    pub const fn new(holding_id: HoldingId, write_pool: PgPool, read_pool: PgPool) -> Self {
        Self {
            holding_id,
            write_pool,
            read_pool,
        }
    }

    pub async fn apply(&self, event: &ProjectionEvent) -> Result<(), PortError> {
        if event.event_type != RECORD_RELATION_DRAFT_EVENT_TYPE || event.event_version != 1 {
            return Err(PortError::InvalidState);
        }
        let draft: RecordRelationDraft =
            serde_json::from_value(event.payload.clone()).map_err(|_| PortError::InvalidState)?;
        draft.validate()?;
        if draft.holding_id != self.holding_id
            || draft.relation.source.id().as_uuid() != event.aggregate_id.as_uuid()
            || draft.relation.version != event.aggregate_version
        {
            return Err(PortError::InvalidState);
        }
        let mut relation = draft.relation.clone();
        relation.committed_sequence = event.global_sequence;
        relation.validate().map_err(|_| PortError::InvalidState)?;
        let mut tx = self.read_pool.begin().await.map_err(database)?;
        set_holding(&mut tx, self.holding_id).await?;
        sqlx::query(
            "INSERT INTO erpx_collaboration_read.record_relation_projection
             (holding_id,relation_id,source_record_id,target_record_id,source_record_ref,
              target_record_ref,graph_thread_id,relation_kind,relation_version,graph_version,
              relation_document,created_by_principal_id,created_at,revoked_by_principal_id,
              revoked_at,committed_sequence,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$9,$10,$11,$12,$13,$14,$15,$16)
             ON CONFLICT (holding_id,relation_id) DO UPDATE SET
              target_record_id=EXCLUDED.target_record_id,
              target_record_ref=EXCLUDED.target_record_ref,
              relation_kind=EXCLUDED.relation_kind,
              relation_version=EXCLUDED.relation_version,
              graph_version=EXCLUDED.graph_version,
              relation_document=EXCLUDED.relation_document,
              revoked_by_principal_id=EXCLUDED.revoked_by_principal_id,
              revoked_at=EXCLUDED.revoked_at,
              committed_sequence=EXCLUDED.committed_sequence,
              projected_at=EXCLUDED.projected_at
             WHERE erpx_collaboration_read.record_relation_projection.committed_sequence
                   < EXCLUDED.committed_sequence",
        )
        .bind(self.holding_id.as_uuid())
        .bind(relation.id.as_uuid())
        .bind(relation.source.id().as_uuid())
        .bind(relation.target.id().as_uuid())
        .bind(serde_json::to_value(&relation.source).map_err(|_| PortError::InvalidState)?)
        .bind(serde_json::to_value(&relation.target).map_err(|_| PortError::InvalidState)?)
        .bind(
            record_relation_graph_thread_id(&relation.source)
                .map_err(|_| PortError::InvalidState)?
                .as_uuid(),
        )
        .bind(relation_kind_name(&relation.kind)?)
        .bind(to_i64(relation.version)?)
        .bind(serde_json::to_value(&relation).map_err(|_| PortError::InvalidState)?)
        .bind(relation.created_by.id().as_uuid())
        .bind(relation.created_at)
        .bind(relation.revoked_by.map(|actor| actor.id().as_uuid()))
        .bind(relation.revoked_at)
        .bind(to_i64(event.global_sequence)?)
        .bind(chrono::Utc::now())
        .execute(&mut *tx)
        .await
        .map_err(database)?;
        tx.commit().await.map_err(database)?;

        let mut tx = self.write_pool.begin().await.map_err(database)?;
        set_holding(&mut tx, self.holding_id).await?;
        let changed = sqlx::query(
            "UPDATE erpx_collaboration.record_relation_publication_drafts
             SET committed_sequence=$3,projected_at=$4
             WHERE holding_id=$1 AND event_id=$2
               AND (committed_sequence IS NULL OR committed_sequence=$3)",
        )
        .bind(self.holding_id.as_uuid())
        .bind(event.event_id.as_uuid())
        .bind(to_i64(event.global_sequence)?)
        .bind(chrono::Utc::now())
        .execute(&mut *tx)
        .await
        .map_err(database)?
        .rows_affected();
        if changed != 1 {
            return Err(PortError::Conflict);
        }
        tx.commit().await.map_err(database)
    }
}

async fn load_graph_in(
    tx: &mut Transaction<'_, Postgres>,
    source: &AuthorizedRelationRecordAccess,
    lock: bool,
) -> Result<Option<RecordRelationGraph>, PortError> {
    let suffix = if lock { " FOR UPDATE" } else { "" };
    let query = format!(
        "SELECT source_record_ref,aggregate_version
         FROM erpx_collaboration.record_relation_graphs
         WHERE holding_id=$1 AND source_record_id=$2 AND source_record_ref=$3::JSONB{suffix}"
    );
    let source_json = serde_json::to_value(source.record()).map_err(|_| PortError::InvalidState)?;
    let Some(row) = sqlx::query(&query)
        .bind(source.holding_id().as_uuid())
        .bind(source.record().id().as_uuid())
        .bind(&source_json)
        .fetch_optional(&mut **tx)
        .await
        .map_err(database)?
    else {
        return Ok(None);
    };
    let relations = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT relation_document FROM erpx_collaboration.record_relations
         WHERE holding_id=$1 AND source_record_id=$2 ORDER BY graph_version,relation_id",
    )
    .bind(source.holding_id().as_uuid())
    .bind(source.record().id().as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(database)?;
    let graph = serde_json::from_value::<RecordRelationGraph>(serde_json::json!({
        "source": row.try_get::<serde_json::Value, _>("source_record_ref").map_err(|_| PortError::Internal)?,
        "version": to_u64(row.try_get::<i64, _>("aggregate_version").map_err(|_| PortError::Internal)?)?,
        "relations": relations,
    }))
    .map_err(|_| PortError::Internal)?;
    graph.validate_loaded().map_err(|_| PortError::Internal)?;
    Ok(Some(graph))
}

fn relation_kind_name(kind: &erpx_collaboration::RelationKind) -> Result<&'static str, PortError> {
    Ok(match kind {
        erpx_collaboration::RelationKind::RelatesTo => "relates_to",
        erpx_collaboration::RelationKind::Blocks => "blocks",
        erpx_collaboration::RelationKind::DependsOn => "depends_on",
        erpx_collaboration::RelationKind::Implements => "implements",
        erpx_collaboration::RelationKind::Supersedes => "supersedes",
        erpx_collaboration::RelationKind::DerivedFrom => "derived_from",
        erpx_collaboration::RelationKind::EvidenceFor => "evidence_for",
        erpx_collaboration::RelationKind::Custom { .. } => return Err(PortError::InvalidState),
    })
}

fn relation_time(draft: &RecordRelationDraft) -> chrono::DateTime<chrono::Utc> {
    draft
        .relation
        .revoked_at
        .unwrap_or(draft.relation.created_at)
}

async fn set_holding(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
) -> Result<(), PortError> {
    let expected = holding_id.to_string();
    let bound = sqlx::query_scalar::<_, String>("SELECT set_config('app.holding_id', $1, true)")
        .bind(&expected)
        .fetch_one(&mut **tx)
        .await
        .map_err(database)?;
    (bound == expected).then_some(()).ok_or(PortError::Denied)
}

fn to_i64(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::InvalidState)
}

fn to_u64(value: i64) -> Result<u64, PortError> {
    u64::try_from(value).map_err(|_| PortError::Internal)
}

fn database(_: sqlx::Error) -> PortError {
    PortError::Unavailable
}

#[cfg(test)]
mod tests {
    #[test]
    fn relation_query_authorizes_both_full_refs_before_payload_count_and_page() {
        let source = include_str!("relation.rs");
        let query = source
            .split("WITH checkpoint AS MATERIALIZED")
            .nth(1)
            .expect("relation list query")
            .split(".bind(source.holding_id")
            .next()
            .expect("query boundary");
        let source_gate = query.find("source_authorized AS MATERIALIZED").unwrap();
        let target_gate = query.find("authorized AS MATERIALIZED").unwrap();
        let count = query.find("authorized_count AS MATERIALIZED").unwrap();
        let page = query.find("page AS MATERIALIZED").unwrap();
        let payload = query.find("page.relation_document").unwrap();
        assert!(source_gate < target_gate && target_gate < count && count < page && page < payload);
        assert!(query.contains("descriptor.descriptor->'record'=$3::JSONB"));
        assert!(
            query.contains(
                "(relation.source_record_id=$2 AND relation.source_record_ref=$3::JSONB)"
            )
        );
        assert!(
            query.contains(
                "(relation.target_record_id=$2 AND relation.target_record_ref=$3::JSONB)"
            )
        );
        assert!(
            query.contains(
                "descriptor.descriptor->'record'=CASE WHEN candidate.source_record_id=$2"
            )
        );
        assert!(
            query.contains("THEN candidate.target_record_ref ELSE candidate.source_record_ref END")
        );
        assert!(query.contains("relation.committed_sequence<=source_authorized.applied_position"));
    }

    #[test]
    fn write_path_requires_human_session_and_emits_outbox_draft_atomically() {
        let source = include_str!("relation.rs");
        assert!(source.contains("source.attribution().session_id"));
        assert!(source.contains("record_relation_publication_drafts"));
        assert!(source.contains("RETURNING global_sequence"));
        assert!(source.contains("record_relation_command_receipts"));
        assert!(!source.contains("RelationKind::Custom { .. } => \""));
    }
}
