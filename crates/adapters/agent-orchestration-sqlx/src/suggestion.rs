use async_trait::async_trait;
use chrono::Utc;
use erpx_agent_orchestration::{
    SuggestionApplicationError, SuggestionCommit, SuggestionEnvelope, SuggestionId,
    SuggestionOwner, SuggestionProviderKind, SuggestionReceipt, SuggestionRepository,
    SuggestionStatus,
};
use erpx_kernel::{ActorKind, AuthorityScope};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::{command::scope_key, holding::HoldingBoundPool};

#[derive(Debug, Clone)]
pub struct SqlxSuggestionRepository {
    pool: HoldingBoundPool,
}

impl SqlxSuggestionRepository {
    pub fn try_new(
        pool: sqlx::PgPool,
        holding_id: Uuid,
    ) -> Result<Self, SuggestionApplicationError> {
        Ok(Self {
            pool: HoldingBoundPool::try_new(pool, holding_id)
                .map_err(|_| SuggestionApplicationError::Unavailable)?,
        })
    }
}

#[async_trait]
impl SuggestionRepository for SqlxSuggestionRepository {
    async fn load(
        &self,
        scope: &AuthorityScope,
        id: SuggestionId,
    ) -> Result<Option<SuggestionEnvelope>, SuggestionApplicationError> {
        scope
            .validate()
            .map_err(|_| SuggestionApplicationError::Denied)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| SuggestionApplicationError::Unavailable)?;
        let row = sqlx::query(
            "SELECT envelope_document
               FROM erpx_agent_write.suggestion_envelopes
              WHERE holding_id=$1 AND suggestion_id=$2",
        )
        .bind(self.pool.holding_id())
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| SuggestionApplicationError::Unavailable)?;
        tx.commit()
            .await
            .map_err(|_| SuggestionApplicationError::Unavailable)?;
        let Some(row) = row else { return Ok(None) };
        let document = row
            .try_get::<Option<serde_json::Value>, _>("envelope_document")
            .map_err(|_| SuggestionApplicationError::Unavailable)?
            .ok_or(SuggestionApplicationError::Unavailable)?;
        let envelope: SuggestionEnvelope = serde_json::from_value(document)
            .map_err(|_| SuggestionApplicationError::Unavailable)?;
        envelope
            .validate()
            .map_err(SuggestionApplicationError::Domain)?;
        if !scope.contains(envelope.scope()) {
            return Err(SuggestionApplicationError::Denied);
        }
        Ok(Some(envelope))
    }

    async fn commit(
        &self,
        scope: &AuthorityScope,
        commit: SuggestionCommit,
    ) -> Result<SuggestionReceipt, SuggestionApplicationError> {
        scope
            .validate()
            .map_err(|_| SuggestionApplicationError::Denied)?;
        commit
            .actor
            .validate()
            .map_err(|_| SuggestionApplicationError::InvalidRequest)?;
        commit
            .envelope
            .validate()
            .map_err(SuggestionApplicationError::Domain)?;
        if commit.command_id.is_nil()
            || commit.idempotency_key.trim().is_empty()
            || commit.idempotency_key.len() > 256
            || !scope.contains(commit.envelope.scope())
        {
            return Err(SuggestionApplicationError::InvalidRequest);
        }

        let envelope_document = serde_json::to_value(&commit.envelope)
            .map_err(|_| SuggestionApplicationError::Unavailable)?;
        let request_digest = digest(&serde_json::json!({
            "command_id": commit.command_id,
            "expected_version": commit.expected_version,
            "actor": commit.actor,
            "envelope": envelope_document,
        }))?;
        let scope_key = scope_key(commit.envelope.scope());
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| SuggestionApplicationError::Unavailable)?;

        if let Some(row) = sqlx::query(
            "SELECT request_digest,suggestion_id,aggregate_version,
                    result_envelope_document
               FROM erpx_agent_write.suggestion_command_receipts
              WHERE holding_id=$1 AND scope_key=$2 AND idempotency_key=$3
              FOR UPDATE",
        )
        .bind(self.pool.holding_id())
        .bind(&scope_key)
        .bind(&commit.idempotency_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| SuggestionApplicationError::Unavailable)?
        {
            let matches = row.try_get::<String, _>("request_digest").ok().as_deref()
                == Some(request_digest.as_str())
                && row.try_get::<Uuid, _>("suggestion_id").ok()
                    == Some(commit.envelope.id().as_uuid())
                && row
                    .try_get::<i64, _>("aggregate_version")
                    .ok()
                    .and_then(|value| u64::try_from(value).ok())
                    == Some(commit.envelope.version());
            if !matches {
                return Err(SuggestionApplicationError::VersionConflict);
            }
            let result: SuggestionEnvelope = serde_json::from_value(
                row.try_get("result_envelope_document")
                    .map_err(|_| SuggestionApplicationError::Unavailable)?,
            )
            .map_err(|_| SuggestionApplicationError::Unavailable)?;
            result
                .validate()
                .map_err(SuggestionApplicationError::Domain)?;
            tx.commit()
                .await
                .map_err(|_| SuggestionApplicationError::Unavailable)?;
            return Ok(SuggestionReceipt {
                envelope: result,
                replayed: true,
            });
        }

        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version
               FROM erpx_agent_write.suggestion_envelopes
              WHERE holding_id=$1 AND suggestion_id=$2 FOR UPDATE",
        )
        .bind(self.pool.holding_id())
        .bind(commit.envelope.id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| SuggestionApplicationError::Unavailable)?;
        let expected = i64::try_from(commit.expected_version)
            .map_err(|_| SuggestionApplicationError::VersionConflict)?;
        let next = i64::try_from(commit.envelope.version())
            .map_err(|_| SuggestionApplicationError::VersionConflict)?;
        let version_matches = match current {
            None => commit.expected_version == 0 && commit.envelope.version() == 1,
            Some(current) => current == expected && next == current.saturating_add(1),
        };
        if !version_matches {
            return Err(SuggestionApplicationError::VersionConflict);
        }

        let field_patch_document = commit
            .envelope
            .field_patch()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|_| SuggestionApplicationError::Unavailable)?;
        let conflicts = serde_json::to_value(commit.envelope.field_patch_conflicts())
            .map_err(|_| SuggestionApplicationError::Unavailable)?;
        let human_review_document = commit
            .envelope
            .human_field_patch_review()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|_| SuggestionApplicationError::Unavailable)?;
        let actor_kind = commit.envelope.applied_actor_kind().map(actor_kind_token);

        let result = sqlx::query(
            "INSERT INTO erpx_agent_write.suggestion_envelopes(
                holding_id,suggestion_id,aggregate_version,scope_key,owner_context,
                capability,source_record_kind,source_record_id,source_record_version,
                schema_version,policy_version,payload_digest,output_digest,provider_kind,
                provider_ref,model_ref,prompt_digest,invocation_evidence_ref,
                suggestion_status,requested_by,applied_by,failure_code,created_at,updated_at,
                envelope_document,field_patch_document,field_patch_conflicts,
                human_review_document,applied_actor_kind,applied_decision_ref,applied_session_id)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,
                    $19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31)
             ON CONFLICT(holding_id,suggestion_id) DO UPDATE SET
                aggregate_version=EXCLUDED.aggregate_version,
                output_digest=EXCLUDED.output_digest,
                suggestion_status=EXCLUDED.suggestion_status,
                applied_by=EXCLUDED.applied_by,failure_code=EXCLUDED.failure_code,
                updated_at=EXCLUDED.updated_at,envelope_document=EXCLUDED.envelope_document,
                field_patch_document=EXCLUDED.field_patch_document,
                field_patch_conflicts=EXCLUDED.field_patch_conflicts,
                human_review_document=EXCLUDED.human_review_document,
                applied_actor_kind=EXCLUDED.applied_actor_kind,
                applied_decision_ref=EXCLUDED.applied_decision_ref,
                applied_session_id=EXCLUDED.applied_session_id
             WHERE erpx_agent_write.suggestion_envelopes.aggregate_version=$32",
        )
        .bind(self.pool.holding_id())
        .bind(commit.envelope.id().as_uuid())
        .bind(next)
        .bind(&scope_key)
        .bind(owner_token(commit.envelope.owner()))
        .bind(commit.envelope.capability().token())
        .bind(&commit.envelope.source().record_kind)
        .bind(commit.envelope.source().record_id)
        .bind(
            i64::try_from(commit.envelope.source().record_version)
                .map_err(|_| SuggestionApplicationError::Unavailable)?,
        )
        .bind(i32::from(commit.envelope.schema_version()))
        .bind(i64::from(commit.envelope.policy_version()))
        .bind(commit.envelope.payload_digest())
        .bind(commit.envelope.output_digest())
        .bind(provider_token(commit.envelope.provenance().provider_kind))
        .bind(&commit.envelope.provenance().provider_ref)
        .bind(&commit.envelope.provenance().model_ref)
        .bind(&commit.envelope.provenance().prompt_digest)
        .bind(&commit.envelope.provenance().invocation_evidence_ref)
        .bind(status_token(commit.envelope.status()))
        .bind(commit.envelope.requested_by())
        .bind(commit.envelope.applied_by())
        .bind(commit.envelope.failure_code())
        .bind(commit.envelope.created_at())
        .bind(commit.envelope.updated_at())
        .bind(&envelope_document)
        .bind(field_patch_document)
        .bind(conflicts)
        .bind(human_review_document)
        .bind(actor_kind)
        .bind(commit.envelope.applied_decision_ref())
        .bind(commit.envelope.applied_session_id().map(|id| id.as_uuid()))
        .bind(expected)
        .execute(&mut *tx)
        .await
        .map_err(|_| SuggestionApplicationError::Unavailable)?;
        if result.rows_affected() != 1 {
            return Err(SuggestionApplicationError::VersionConflict);
        }

        let committed_at = Utc::now();
        sqlx::query(
            "INSERT INTO erpx_agent_write.suggestion_command_receipts(
                holding_id,scope_key,idempotency_key,command_id,request_digest,
                suggestion_id,aggregate_version,result_envelope_document,committed_at)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(self.pool.holding_id())
        .bind(&scope_key)
        .bind(&commit.idempotency_key)
        .bind(commit.command_id)
        .bind(&request_digest)
        .bind(commit.envelope.id().as_uuid())
        .bind(next)
        .bind(&envelope_document)
        .bind(committed_at)
        .execute(&mut *tx)
        .await
        .map_err(|_| SuggestionApplicationError::Unavailable)?;

        let actor = serde_json::to_value(&commit.actor)
            .map_err(|_| SuggestionApplicationError::Unavailable)?;
        let audit_id = stable_id(commit.command_id, b"suggestion-audit");
        let event_id = stable_id(commit.command_id, b"suggestion-event");
        let partition = format!("holding:{}", self.pool.holding_id());
        sqlx::query(
            "INSERT INTO erpx_cqrs_write.audit_log(
                holding_id,audit_id,context_name,partition_key,command_id,correlation_id,
                active_scope,actor,audit_context,action,aggregate_type,aggregate_id,
                aggregate_version,details,occurred_at)
             VALUES($1,$2,'agent_orchestration',$3,$4,$4,$5,$6,$7,
                    'agent.suggestion.changed','suggestion',$8,$9,$10,$11)",
        )
        .bind(self.pool.holding_id())
        .bind(audit_id)
        .bind(&partition)
        .bind(commit.command_id)
        .bind(
            serde_json::to_value(commit.envelope.scope())
                .map_err(|_| SuggestionApplicationError::Unavailable)?,
        )
        .bind(&actor)
        .bind(serde_json::json!({"purpose":"typed-field-suggestion","customer_visible":false}))
        .bind(commit.envelope.id().as_uuid())
        .bind(next)
        .bind(serde_json::json!({"status":status_token(commit.envelope.status()),"typed":true}))
        .bind(commit.envelope.updated_at())
        .execute(&mut *tx)
        .await
        .map_err(|_| SuggestionApplicationError::Unavailable)?;
        sqlx::query(
            "INSERT INTO erpx_cqrs_write.outbox_events(
                holding_id,event_id,context_name,partition_key,command_id,event_type,
                event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,
                actor,correlation_id,causation_id,payload,occurred_at,available_at)
             VALUES($1,$2,'agent_orchestration',$3,$4,'agent.suggestion.changed',2,
                    'suggestion',$5,$6,$7,$8,$4,NULL,$9,$10,$10)",
        )
        .bind(self.pool.holding_id())
        .bind(event_id)
        .bind(&partition)
        .bind(commit.command_id)
        .bind(commit.envelope.id().as_uuid())
        .bind(next)
        .bind(
            serde_json::to_value(commit.envelope.scope())
                .map_err(|_| SuggestionApplicationError::Unavailable)?,
        )
        .bind(actor)
        .bind(serde_json::json!({"suggestion":envelope_document,"typed":true}))
        .bind(commit.envelope.updated_at())
        .execute(&mut *tx)
        .await
        .map_err(|_| SuggestionApplicationError::Unavailable)?;

        tx.commit()
            .await
            .map_err(|_| SuggestionApplicationError::Unavailable)?;
        Ok(SuggestionReceipt {
            envelope: commit.envelope,
            replayed: false,
        })
    }
}

fn owner_token(owner: SuggestionOwner) -> &'static str {
    match owner {
        SuggestionOwner::Sales => "sales",
        SuggestionOwner::Finance => "finance",
        SuggestionOwner::RecordOwner(owner) => owner.slug(),
    }
}

fn provider_token(provider: SuggestionProviderKind) -> &'static str {
    match provider {
        SuggestionProviderKind::Fixture => "fixture",
        SuggestionProviderKind::Ollama => "ollama",
        SuggestionProviderKind::OpenAiCompatible => "open_ai_compatible",
    }
}

fn status_token(status: SuggestionStatus) -> &'static str {
    match status {
        SuggestionStatus::Queued => "queued",
        SuggestionStatus::Running => "running",
        SuggestionStatus::Ready => "ready",
        SuggestionStatus::Conflicted => "conflicted",
        SuggestionStatus::Applied => "applied",
        SuggestionStatus::Rejected => "rejected",
        SuggestionStatus::Superseded => "superseded",
        SuggestionStatus::Failed => "failed",
    }
}

fn actor_kind_token(kind: ActorKind) -> &'static str {
    match kind {
        ActorKind::PlatformUser => "platform_user",
        ActorKind::HoldingUser => "holding_user",
        ActorKind::Agent => "agent",
        ActorKind::Service => "service",
        ActorKind::SystemWorker => "system_worker",
    }
}

fn digest(value: &serde_json::Value) -> Result<String, SuggestionApplicationError> {
    let bytes = serde_json::to_vec(value).map_err(|_| SuggestionApplicationError::Unavailable)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn stable_id(command_id: Uuid, suffix: &[u8]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(command_id.as_bytes());
    digest.update(suffix);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.finalize()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
