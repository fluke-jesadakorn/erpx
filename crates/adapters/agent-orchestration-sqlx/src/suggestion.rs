use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::Utc;
use erpx_agent_orchestration::{
    SuggestionApplicationError, SuggestionCommit, SuggestionEnvelope, SuggestionId,
    SuggestionOwner, SuggestionProviderKind, SuggestionReceipt, SuggestionRepository,
    SuggestionStatus,
};
use erpx_integration_contracts::{
    SuggestedFieldValue, owner_field_value_digest, record_editable_field_schema,
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

    /// Returns typed envelopes for one already-authorized record. Callers must
    /// authorize the full record reference through the Item Directory before
    /// invoking this method; the repository additionally enforces the
    /// canonical authority scope on every returned envelope.
    pub async fn list_by_record(
        &self,
        scope: &AuthorityScope,
        source_record_kind: &str,
        source_record_id: Uuid,
        page_size: u16,
    ) -> Result<Vec<SuggestionEnvelope>, SuggestionApplicationError> {
        scope
            .validate()
            .map_err(|_| SuggestionApplicationError::Denied)?;
        if source_record_kind.trim().is_empty()
            || source_record_kind.len() > 128
            || source_record_id.is_nil()
            || page_size == 0
            || page_size > 100
        {
            return Err(SuggestionApplicationError::InvalidRequest);
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| SuggestionApplicationError::Unavailable)?;
        let rows = sqlx::query(
            "SELECT envelope_document
               FROM erpx_agent_write.suggestion_envelopes
              WHERE holding_id=$1 AND source_record_kind=$2 AND source_record_id=$3
              ORDER BY updated_at DESC,suggestion_id DESC
              LIMIT $4",
        )
        .bind(self.pool.holding_id())
        .bind(source_record_kind)
        .bind(source_record_id)
        .bind(i64::from(page_size))
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| SuggestionApplicationError::Unavailable)?;
        tx.commit()
            .await
            .map_err(|_| SuggestionApplicationError::Unavailable)?;
        rows.into_iter()
            .map(|row| {
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
                Ok(envelope)
            })
            .collect()
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
                    result_envelope_document,committed_at
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
            let event_id = stable_id(commit.command_id, b"suggestion-event");
            let global_sequence = sqlx::query_scalar::<_, i64>(
                "SELECT global_sequence FROM erpx_cqrs_write.outbox_events
                  WHERE holding_id=$1 AND event_id=$2",
            )
            .bind(self.pool.holding_id())
            .bind(event_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| SuggestionApplicationError::Unavailable)?
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(SuggestionApplicationError::Unavailable)?;
            let committed_at = row
                .try_get::<chrono::DateTime<Utc>, _>("committed_at")
                .map_err(|_| SuggestionApplicationError::Unavailable)?;
            tx.commit()
                .await
                .map_err(|_| SuggestionApplicationError::Unavailable)?;
            return Ok(SuggestionReceipt {
                envelope: result,
                replayed: true,
                event_id,
                global_sequence,
                committed_at,
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

        if commit.envelope.status() == SuggestionStatus::Applied {
            apply_owner_field_patch(&mut tx, self.pool.holding_id(), &commit).await?;
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
        let global_sequence = sqlx::query_scalar::<_, i64>(
            "INSERT INTO erpx_cqrs_write.outbox_events(
                holding_id,event_id,context_name,partition_key,command_id,event_type,
                event_version,aggregate_type,aggregate_id,aggregate_version,active_scope,
                actor,correlation_id,causation_id,payload,occurred_at,available_at)
             VALUES($1,$2,'agent_orchestration',$3,$4,'agent.suggestion.changed',2,
                    'suggestion',$5,$6,$7,$8,$4,NULL,$9,$10,$10)
             RETURNING global_sequence",
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
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| SuggestionApplicationError::Unavailable)?;
        let global_sequence =
            u64::try_from(global_sequence).map_err(|_| SuggestionApplicationError::Unavailable)?;

        tx.commit()
            .await
            .map_err(|_| SuggestionApplicationError::Unavailable)?;
        Ok(SuggestionReceipt {
            envelope: commit.envelope,
            replayed: false,
            event_id,
            global_sequence,
            committed_at,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct OwnerFieldPatchRoute {
    context_name: &'static str,
    application_table: &'static str,
    base_record_query: &'static str,
}

fn owner_field_patch_route(record_type: &str) -> Option<OwnerFieldPatchRoute> {
    match record_type {
        "sales.quote" => Some(OwnerFieldPatchRoute {
            context_name: "sales",
            application_table: "erpx_sales_write.record_field_patch_applications",
            base_record_query: "SELECT aggregate_version AS record_version,
                jsonb_build_object(
                  'quote.valid_until',jsonb_build_object('kind','date_time','value',valid_until)
                ) AS base_values
                FROM erpx_sales_write.quote_guards
                WHERE holding_id=$1 AND quote_id=$2 FOR UPDATE",
        }),
        "customer-360.note" => Some(OwnerFieldPatchRoute {
            context_name: "customer_360",
            application_table: "erpx_customer_360_write.record_field_patch_applications",
            base_record_query: "SELECT record_version,
                jsonb_build_object() AS base_values
                FROM erpx_customer_360_write.owned_records
                WHERE holding_id=$1 AND record_id=$2 AND record_kind='note' FOR UPDATE",
        }),
        "procurement.requisition" => Some(OwnerFieldPatchRoute {
            context_name: "procurement",
            application_table: "erpx_procurement_write.record_field_patch_applications",
            base_record_query: "SELECT record_version,
                jsonb_build_object() AS base_values
                FROM erpx_procurement_write.business_records
                WHERE holding_id=$1 AND record_id=$2 AND record_kind='requisition' FOR UPDATE",
        }),
        "maintenance.request" => Some(OwnerFieldPatchRoute {
            context_name: "maintenance",
            application_table: "erpx_maintenance_write.record_field_patch_applications",
            base_record_query: "SELECT record_version,
                jsonb_build_object() AS base_values
                FROM erpx_maintenance_write.business_records
                WHERE holding_id=$1 AND record_id=$2 AND record_kind='request' FOR UPDATE",
        }),
        "marketing.content" => Some(OwnerFieldPatchRoute {
            context_name: "marketing",
            application_table: "erpx_marketing_write.record_field_patch_applications",
            base_record_query: "SELECT record_version,
                jsonb_build_object(
                  'content.title',jsonb_build_object('kind','text','value',record_title)
                ) AS base_values
                FROM erpx_marketing_write.business_records
                WHERE holding_id=$1 AND record_id=$2 AND record_kind='content' FOR UPDATE",
        }),
        _ => None,
    }
}

async fn apply_owner_field_patch(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    holding_id: Uuid,
    commit: &SuggestionCommit,
) -> Result<(), SuggestionApplicationError> {
    if commit.actor.kind != ActorKind::HoldingUser
        || commit.actor.represented_principal_ref.is_some()
        || commit.actor.support_capability_id.is_some()
    {
        return Err(SuggestionApplicationError::HumanDecisionRequired);
    }
    let session_id = commit
        .actor
        .session_id
        .ok_or(SuggestionApplicationError::HumanDecisionRequired)?;
    let patch = commit
        .envelope
        .field_patch()
        .ok_or(SuggestionApplicationError::InvalidRequest)?;
    patch
        .validate()
        .map_err(|_| SuggestionApplicationError::InvalidRequest)?;
    let review = commit
        .envelope
        .human_field_patch_review()
        .ok_or(SuggestionApplicationError::HumanDecisionRequired)?;
    let decision_ref = commit
        .envelope
        .applied_decision_ref()
        .ok_or(SuggestionApplicationError::HumanDecisionRequired)?;
    if review.reviewer_principal_ref != commit.actor.principal_ref
        || review.reviewer_session_id != session_id
        || commit.envelope.applied_session_id() != Some(session_id)
        || commit.envelope.applied_actor_kind() != Some(ActorKind::HoldingUser)
    {
        return Err(SuggestionApplicationError::HumanDecisionRequired);
    }
    let record_type = patch.source.record().record_type().as_str();
    let route =
        owner_field_patch_route(record_type).ok_or(SuggestionApplicationError::InvalidRequest)?;
    let schema = record_editable_field_schema(record_type)
        .filter(|schema| {
            schema.supports_agent_patches()
                && schema.version == patch.schema_version
                && schema.owner == patch.source.record().owner_context()
        })
        .ok_or(SuggestionApplicationError::InvalidRequest)?;
    let permission = patch
        .changes
        .iter()
        .filter_map(|change| schema.field(change.field.field_id()))
        .map(|field| field.human_apply_permission)
        .collect::<std::collections::BTreeSet<_>>();
    if permission.len() != 1 {
        return Err(SuggestionApplicationError::Denied);
    }
    let permission = permission
        .into_iter()
        .next()
        .ok_or(SuggestionApplicationError::Denied)?;

    let base = sqlx::query(route.base_record_query)
        .bind(holding_id)
        .bind(patch.source.record().id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|_| SuggestionApplicationError::Unavailable)?
        .ok_or(SuggestionApplicationError::NotFound)?;
    let current_record_version = u64::try_from(
        base.try_get::<i64, _>("record_version")
            .map_err(|_| SuggestionApplicationError::Unavailable)?,
    )
    .map_err(|_| SuggestionApplicationError::Unavailable)?;
    if current_record_version != patch.source.version() {
        return Err(SuggestionApplicationError::VersionConflict);
    }
    let base_values: BTreeMap<String, SuggestedFieldValue> = serde_json::from_value(
        base.try_get("base_values")
            .map_err(|_| SuggestionApplicationError::Unavailable)?,
    )
    .map_err(|_| SuggestionApplicationError::Unavailable)?;

    let state_query = format!(
        "SELECT application_id,owner_revision,field_values
         FROM {} WHERE holding_id=$1 AND record_id=$2
         ORDER BY owner_revision DESC LIMIT 1 FOR UPDATE",
        route.application_table
    );
    let previous = sqlx::query(&state_query)
        .bind(holding_id)
        .bind(patch.source.record().id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|_| SuggestionApplicationError::Unavailable)?;
    let (previous_application_id, previous_revision, mut values) = if let Some(row) = previous {
        (
            Some(
                row.try_get::<Uuid, _>("application_id")
                    .map_err(|_| SuggestionApplicationError::Unavailable)?,
            ),
            u64::try_from(
                row.try_get::<i64, _>("owner_revision")
                    .map_err(|_| SuggestionApplicationError::Unavailable)?,
            )
            .map_err(|_| SuggestionApplicationError::Unavailable)?,
            serde_json::from_value::<BTreeMap<String, SuggestedFieldValue>>(
                row.try_get("field_values")
                    .map_err(|_| SuggestionApplicationError::Unavailable)?,
            )
            .map_err(|_| SuggestionApplicationError::Unavailable)?,
        )
    } else {
        (None, 0, BTreeMap::new())
    };
    for change in &patch.changes {
        let current = values
            .get(change.field.field_id())
            .or_else(|| base_values.get(change.field.field_id()))
            .cloned()
            .unwrap_or(SuggestedFieldValue::Unset);
        let current_digest = owner_field_value_digest(&current)
            .map_err(|_| SuggestionApplicationError::Unavailable)?;
        if !current_digest.eq_ignore_ascii_case(&change.previous_value_digest) {
            return Err(SuggestionApplicationError::VersionConflict);
        }
        values.insert(
            change.field.field_id().to_owned(),
            change.proposed_value.clone(),
        );
    }
    let owner_revision = previous_revision
        .checked_add(1)
        .ok_or(SuggestionApplicationError::VersionConflict)?;
    let application_id = stable_id(commit.command_id, b"owner-field-patch-application");
    let field_values =
        serde_json::to_value(&values).map_err(|_| SuggestionApplicationError::Unavailable)?;
    let patch_document =
        serde_json::to_value(patch).map_err(|_| SuggestionApplicationError::Unavailable)?;
    let review_document =
        serde_json::to_value(review).map_err(|_| SuggestionApplicationError::Unavailable)?;
    let request_digest = digest(&serde_json::json!({
        "command_id": commit.command_id,
        "suggestion_id": commit.envelope.id(),
        "patch": patch_document,
        "review": review_document,
        "permission": permission,
    }))?;
    let insert = format!(
        "INSERT INTO {}(
           holding_id,application_id,command_id,suggestion_id,record_id,record_type,
           source_record_version,schema_version,owner_revision,previous_application_id,
           field_values,patch_document,human_review_document,request_digest,
           human_apply_permission,actor_principal_ref,actor_session_id,decision_ref,applied_at)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)",
        route.application_table
    );
    sqlx::query(&insert)
        .bind(holding_id)
        .bind(application_id)
        .bind(commit.command_id)
        .bind(commit.envelope.id().as_uuid())
        .bind(patch.source.record().id().as_uuid())
        .bind(record_type)
        .bind(
            i64::try_from(patch.source.version())
                .map_err(|_| SuggestionApplicationError::InvalidRequest)?,
        )
        .bind(
            i64::try_from(patch.schema_version)
                .map_err(|_| SuggestionApplicationError::InvalidRequest)?,
        )
        .bind(
            i64::try_from(owner_revision)
                .map_err(|_| SuggestionApplicationError::VersionConflict)?,
        )
        .bind(previous_application_id)
        .bind(&field_values)
        .bind(&patch_document)
        .bind(&review_document)
        .bind(&request_digest)
        .bind(permission)
        .bind(&commit.actor.principal_ref)
        .bind(session_id.as_uuid())
        .bind(decision_ref)
        .bind(review.reviewed_at)
        .execute(&mut **transaction)
        .await
        .map_err(|error| {
            if error
                .as_database_error()
                .is_some_and(|database| database.is_unique_violation())
            {
                SuggestionApplicationError::VersionConflict
            } else {
                SuggestionApplicationError::Unavailable
            }
        })?;

    let actor =
        serde_json::to_value(&commit.actor).map_err(|_| SuggestionApplicationError::Unavailable)?;
    let active_scope = serde_json::to_value(commit.envelope.scope())
        .map_err(|_| SuggestionApplicationError::Unavailable)?;
    let partition = format!(
        "{}:{}",
        route.context_name,
        patch.source.record().id().as_uuid().simple()
    );
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.audit_log(
           holding_id,audit_id,context_name,partition_key,command_id,correlation_id,
           active_scope,actor,audit_context,action,aggregate_type,aggregate_id,
           aggregate_version,details,occurred_at)
         VALUES($1,$2,$3,$4,$5,$5,$6,$7,$8,'owner.field_patch.applied',
                'record_field_state',$9,$10,$11,$12)",
    )
    .bind(holding_id)
    .bind(stable_id(commit.command_id, b"owner-field-patch-audit"))
    .bind(route.context_name)
    .bind(&partition)
    .bind(commit.command_id)
    .bind(&active_scope)
    .bind(&actor)
    .bind(serde_json::json!({"purpose":"human-applied-field-suggestion","customer_visible":false}))
    .bind(patch.source.record().id().as_uuid())
    .bind(i64::try_from(owner_revision).map_err(|_| SuggestionApplicationError::Unavailable)?)
    .bind(serde_json::json!({"suggestion_id":commit.envelope.id(),"permission":permission,"typed":true}))
    .bind(review.reviewed_at)
    .execute(&mut **transaction)
    .await
    .map_err(|_| SuggestionApplicationError::Unavailable)?;
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.outbox_events(
           holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,
           aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,
           causation_id,payload,occurred_at,available_at)
         VALUES($1,$2,$3,$4,$5,'owner.record_field_patch.applied',1,
                'record_field_state',$6,$7,$8,$9,$5,NULL,$10,$11,$11)",
    )
    .bind(holding_id)
    .bind(stable_id(commit.command_id, b"owner-field-patch-event"))
    .bind(route.context_name)
    .bind(partition)
    .bind(commit.command_id)
    .bind(patch.source.record().id().as_uuid())
    .bind(i64::try_from(owner_revision).map_err(|_| SuggestionApplicationError::Unavailable)?)
    .bind(active_scope)
    .bind(actor)
    .bind(serde_json::json!({
        "record": patch.source.record(),
        "source_record_version": patch.source.version(),
        "schema_version": patch.schema_version,
        "owner_revision": owner_revision,
        "field_values": field_values,
        "suggestion_id": commit.envelope.id(),
        "human_apply_permission": permission,
    }))
    .bind(review.reviewed_at)
    .execute(&mut **transaction)
    .await
    .map_err(|_| SuggestionApplicationError::Unavailable)?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::owner_field_patch_route;

    #[test]
    fn owner_patch_routes_are_closed_to_the_five_published_editable_families() {
        let expected = [
            (
                "sales.quote",
                "sales",
                "erpx_sales_write.record_field_patch_applications",
            ),
            (
                "customer-360.note",
                "customer_360",
                "erpx_customer_360_write.record_field_patch_applications",
            ),
            (
                "procurement.requisition",
                "procurement",
                "erpx_procurement_write.record_field_patch_applications",
            ),
            (
                "maintenance.request",
                "maintenance",
                "erpx_maintenance_write.record_field_patch_applications",
            ),
            (
                "marketing.content",
                "marketing",
                "erpx_marketing_write.record_field_patch_applications",
            ),
        ];
        for (record_type, context, table) in expected {
            let route = owner_field_patch_route(record_type).expect("published owner route");
            assert_eq!(route.context_name, context);
            assert_eq!(route.application_table, table);
            assert!(route.base_record_query.contains("record_version"));
            assert!(route.base_record_query.contains("holding_id=$1"));
            assert!(route.base_record_query.contains("FOR UPDATE"));
        }
        for unsupported in [
            "finance.payment",
            "procurement.purchase-order",
            "planning.plan",
            "custom.record",
            "",
        ] {
            assert!(
                owner_field_patch_route(unsupported).is_none(),
                "unsupported owner route opened for {unsupported}"
            );
        }
    }

    #[test]
    fn owner_patch_is_committed_before_the_suggestion_and_in_the_same_transaction() {
        let source = include_str!("suggestion.rs");
        let version_guard = source
            .find("if !version_matches")
            .expect("suggestion version guard");
        let owner_apply = source
            .find("apply_owner_field_patch(&mut tx")
            .expect("owner application");
        let suggestion_write = source
            .find("INSERT INTO erpx_agent_write.suggestion_envelopes")
            .expect("suggestion write");
        let commit = source.rfind("tx.commit()").expect("transaction commit");
        assert!(version_guard < owner_apply);
        assert!(owner_apply < suggestion_write);
        assert!(suggestion_write < commit);
        assert!(source.contains("owner.record_field_patch.applied"));
        assert!(source.contains("ActorKind::HoldingUser"));
        assert!(source.contains("represented_principal_ref.is_some()"));
        assert!(source.contains("support_capability_id.is_some()"));
    }
}
