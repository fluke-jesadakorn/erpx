use async_trait::async_trait;
use erpx_agent_orchestration::{
    AgentDefinition, AgentDefinitionStatus, AgentVersionStatus, Assignment, AssignmentCommit,
    AssignmentState, BudgetLayerKind, BudgetReservation, DefinitionCommit, DelegationCommit,
    InvocationOrigin, OrchestrationUnitOfWork, PortError, RootAssignmentCommit,
};
use erpx_kernel::TenantScope;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};

use crate::{normalized, runtime::SqlxAgentRuntime};

#[async_trait]
impl OrchestrationUnitOfWork for SqlxAgentRuntime {
    async fn commit_definition(
        &self,
        commit: DefinitionCommit,
    ) -> Result<AgentDefinition, PortError> {
        if !commit.authority_scope.contains(commit.definition.scope()) {
            return Err(PortError::Denied);
        }
        let scope = scope_key(commit.definition.scope());
        let request_digest = digest(&commit.definition)?;
        let tenant_id = self.tenant_id();
        let mut tx = self.database.begin().await?;
        if let Some(replayed) = replay_definition(
            &mut tx,
            tenant_id,
            &scope,
            &commit.idempotency_key,
            &request_digest,
            commit.definition.id(),
        )
        .await?
        {
            tx.commit().await.map_err(|_| PortError::Persistence)?;
            return Ok(replayed);
        }
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_agent_write.agent_definitions
             WHERE tenant_id=$1 AND agent_definition_id=$2 FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(commit.definition.id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        validate_expected(
            commit.expected_version,
            current,
            commit.definition.version(),
        )?;
        persist_definition(&mut tx, tenant_id, &scope, &commit.definition).await?;
        let directory_actor =
            serde_json::to_value(&commit.actor).map_err(|_| PortError::Persistence)?;
        for draft in crate::publication::definition_directory_drafts(&commit.definition)? {
            crate::publication::append_with_idempotency(
                &mut tx,
                tenant_id,
                &commit.idempotency_key,
                directory_actor.clone(),
                commit.definition.updated_at(),
                &draft,
            )
            .await?;
        }
        insert_receipt(
            &mut tx,
            tenant_id,
            &scope,
            &commit.idempotency_key,
            &request_digest,
            "agent_definition",
            commit.definition.id().as_uuid(),
            commit.definition.version(),
            commit.definition.updated_at(),
        )
        .await?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(commit.definition)
    }

    async fn commit_assignment(&self, commit: AssignmentCommit) -> Result<Assignment, PortError> {
        if !commit.authority_scope.contains(commit.assignment.scope()) {
            return Err(PortError::Denied);
        }
        let tenant_id = self.tenant_id();
        let mut tx = self.database.begin().await?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_agent_write.agent_assignments
             WHERE tenant_id=$1 AND assignment_id=$2 FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(commit.assignment.id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        validate_expected(
            commit.expected_version,
            current,
            commit.assignment.version(),
        )?;
        update_assignment(&mut tx, tenant_id, &commit.assignment).await?;
        if commit.assignment.state() == AssignmentState::Cancelled {
            release_budget(
                &mut tx,
                tenant_id,
                commit.assignment.budget_reservation_id(),
            )
            .await?;
        }
        append_assignment_state_fact(
            &mut tx,
            tenant_id,
            &commit.assignment,
            serde_json::to_value(&commit.actor).map_err(|_| PortError::Persistence)?,
            commit.assignment.updated_at(),
        )
        .await?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(commit.assignment)
    }

    async fn commit_root_assignment(
        &self,
        commit: RootAssignmentCommit,
    ) -> Result<Assignment, PortError> {
        validate_root_commit(&commit)?;
        let scope = scope_key(commit.assignment.scope());
        let request_digest = digest(&json!({
            "assignment": &commit.assignment,
            "reservation": &commit.reservation,
            "prompt_ref": &commit.prompt_ref,
            "payload_manifest": &commit.payload_manifest,
        }))?;
        let tenant_id = self.tenant_id();
        let mut tx = self.database.begin().await?;
        if let Some(replayed) = replay_assignment(
            &mut tx,
            tenant_id,
            &scope,
            &commit.assignment.root_invocation().idempotency_key,
            &request_digest,
            commit.assignment.id(),
        )
        .await?
        {
            tx.commit().await.map_err(|_| PortError::Persistence)?;
            return Ok(replayed);
        }
        reserve_budget(
            &mut tx,
            tenant_id,
            &commit.reservation,
            commit.assignment.id(),
        )
        .await?;
        insert_assignment(
            &mut tx,
            tenant_id,
            &scope,
            &commit.assignment,
            &commit.reservation,
            &commit.prompt_ref,
            &commit.payload_manifest,
            false,
        )
        .await?;
        insert_receipt(
            &mut tx,
            tenant_id,
            &scope,
            &commit.assignment.root_invocation().idempotency_key,
            &request_digest,
            "assignment",
            commit.assignment.id().as_uuid(),
            commit.assignment.version(),
            commit.assignment.created_at(),
        )
        .await?;
        append_assignment_state_fact(
            &mut tx,
            tenant_id,
            &commit.assignment,
            serde_json::to_value(&commit.actor).map_err(|_| PortError::Persistence)?,
            commit.assignment.created_at(),
        )
        .await?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(commit.assignment)
    }

    async fn commit_delegation(&self, commit: DelegationCommit) -> Result<Assignment, PortError> {
        if !commit.authority_scope.contains(commit.child.scope())
            || commit.child.parent_assignment_id() != Some(commit.parent_assignment_id)
            || commit.reason_digest.len() != 64
        {
            return Err(PortError::Denied);
        }
        let scope = scope_key(commit.child.scope());
        let request_digest = digest(&json!({
            "child": &commit.child,
            "reservation": &commit.reservation,
            "reason_digest": &commit.reason_digest,
            "prompt_ref": &commit.prompt_ref,
            "payload_manifest": &commit.payload_manifest,
        }))?;
        let tenant_id = self.tenant_id();
        let mut tx = self.database.begin().await?;
        if let Some(replayed) = replay_assignment(
            &mut tx,
            tenant_id,
            &scope,
            &commit.idempotency_key,
            &request_digest,
            commit.child.id(),
        )
        .await?
        {
            tx.commit().await.map_err(|_| PortError::Persistence)?;
            return Ok(replayed);
        }
        let parent_locked: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_agent_write.agent_assignments
             WHERE tenant_id=$1 AND assignment_id=$2 AND aggregate_version=$3
               AND assignment_status='running' FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(commit.parent_assignment_id.as_uuid())
        .bind(to_i64(commit.parent_expected_version)?)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        if parent_locked.is_none() {
            return Err(PortError::VersionConflict);
        }
        let parent = normalized::load_assignment(&mut tx, tenant_id, commit.parent_assignment_id)
            .await?
            .ok_or(PortError::VersionConflict)?;
        if commit.child.root_assignment_id() != parent.root_assignment_id()
            || commit.child.lineage().last() != Some(&commit.child.pin().definition_id)
        {
            return Err(PortError::Persistence);
        }
        reserve_budget(&mut tx, tenant_id, &commit.reservation, commit.child.id()).await?;
        insert_assignment(
            &mut tx,
            tenant_id,
            &scope,
            &commit.child,
            &commit.reservation,
            &commit.prompt_ref,
            &commit.payload_manifest,
            true,
        )
        .await?;
        sqlx::query(
            "INSERT INTO erpx_agent_write.assignment_delegations
                (tenant_id,child_assignment_id,root_assignment_id,parent_assignment_id,
                 parent_agent_definition_id,child_agent_definition_id,reason_digest,
                 scope_key,depth,hops,created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(tenant_id)
        .bind(commit.child.id().as_uuid())
        .bind(commit.child.root_assignment_id().as_uuid())
        .bind(parent.id().as_uuid())
        .bind(parent.pin().definition_id.as_uuid())
        .bind(commit.child.pin().definition_id.as_uuid())
        .bind(&commit.reason_digest)
        .bind(&scope)
        .bind(i32::from(commit.child.depth()))
        .bind(i32::from(commit.child.hops()))
        .bind(commit.child.created_at())
        .execute(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        insert_receipt(
            &mut tx,
            tenant_id,
            &scope,
            &commit.idempotency_key,
            &request_digest,
            "assignment",
            commit.child.id().as_uuid(),
            commit.child.version(),
            commit.child.created_at(),
        )
        .await?;
        append_assignment_state_fact(
            &mut tx,
            tenant_id,
            &commit.child,
            serde_json::to_value(&commit.actor).map_err(|_| PortError::Persistence)?,
            commit.child.created_at(),
        )
        .await?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(commit.child)
    }
}

async fn persist_definition(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: uuid::Uuid,
    scope: &str,
    definition: &AgentDefinition,
) -> Result<(), PortError> {
    sqlx::query(
        "INSERT INTO erpx_agent_write.agent_definitions
            (tenant_id,agent_definition_id,scope_key,tenant_scope,agent_principal_ref,
             business_owner_ref,technical_owner_ref,agent_name,definition_status,
             aggregate_version,created_at,updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
         ON CONFLICT (tenant_id,agent_definition_id) DO UPDATE SET
            business_owner_ref=EXCLUDED.business_owner_ref,
            technical_owner_ref=EXCLUDED.technical_owner_ref,
            agent_name=EXCLUDED.agent_name,definition_status=EXCLUDED.definition_status,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at",
    )
    .bind(tenant_id)
    .bind(definition.id().as_uuid())
    .bind(scope)
    .bind(serde_json::to_value(definition.scope()).map_err(|_| PortError::Persistence)?)
    .bind(definition.agent_principal_ref())
    .bind(definition.business_owner_ref())
    .bind(definition.technical_owner_ref())
    .bind(definition.name())
    .bind(definition_status_name(definition.status()))
    .bind(to_i64(definition.version())?)
    .bind(definition.created_at())
    .bind(definition.updated_at())
    .execute(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;

    for version in definition.versions().values() {
        let manifest = &version.manifest;
        sqlx::query(
            "INSERT INTO erpx_agent_write.agent_versions
                (tenant_id,agent_version_id,agent_definition_id,version_number,version_status,
                 purpose,provider_ref,model_ref,use_case,prompt_digest,policy_digest,
                 maximum_classification,security_review_ref,privacy_review_ref,
                 max_attempts,timeout_seconds,approved_by,approved_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)
             ON CONFLICT (tenant_id,agent_version_id) DO UPDATE SET
                version_status=EXCLUDED.version_status,approved_by=EXCLUDED.approved_by,
                approved_at=EXCLUDED.approved_at",
        )
        .bind(tenant_id)
        .bind(manifest.version_id.as_uuid())
        .bind(definition.id().as_uuid())
        .bind(i32::try_from(manifest.version_number).map_err(|_| PortError::Persistence)?)
        .bind(version_status_name(version.status))
        .bind(&manifest.purpose)
        .bind(&manifest.provider_ref)
        .bind(&manifest.model_ref)
        .bind(&manifest.use_case)
        .bind(&manifest.prompt_digest)
        .bind(&manifest.policy_digest)
        .bind(classification_name(manifest.maximum_classification))
        .bind(&manifest.security_review_ref)
        .bind(&manifest.privacy_review_ref)
        .bind(i32::from(manifest.max_attempts))
        .bind(i32::try_from(manifest.timeout_seconds).map_err(|_| PortError::Persistence)?)
        .bind(&version.approved_by)
        .bind(version.approved_at)
        .execute(&mut **tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        replace_text_set(
            tx,
            tenant_id,
            "agent_version_capabilities",
            "capability",
            manifest.version_id,
            &manifest.capabilities,
        )
        .await?;
        replace_text_set(
            tx,
            tenant_id,
            "agent_version_tools",
            "tool_ref",
            manifest.version_id,
            &manifest.tool_refs,
        )
        .await?;
        replace_text_set(
            tx,
            tenant_id,
            "agent_version_knowledge_scopes",
            "knowledge_scope",
            manifest.version_id,
            &manifest.knowledge_scopes,
        )
        .await?;
        replace_text_set(
            tx,
            tenant_id,
            "agent_version_evaluations",
            "evaluation_ref",
            manifest.version_id,
            &manifest.evaluation_refs,
        )
        .await?;
        sqlx::query(
            "DELETE FROM erpx_agent_write.agent_version_organization_scopes
             WHERE tenant_id=$1 AND agent_version_id=$2",
        )
        .bind(tenant_id)
        .bind(manifest.version_id.as_uuid())
        .execute(&mut **tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        for organization_scope in &manifest.organization_scopes {
            sqlx::query(
                "INSERT INTO erpx_agent_write.agent_version_organization_scopes
                    (tenant_id,agent_version_id,scope_key,tenant_scope) VALUES ($1,$2,$3,$4)",
            )
            .bind(tenant_id)
            .bind(manifest.version_id.as_uuid())
            .bind(scope_key(organization_scope))
            .bind(serde_json::to_value(organization_scope).map_err(|_| PortError::Persistence)?)
            .execute(&mut **tx)
            .await
            .map_err(|_| PortError::Persistence)?;
        }
    }
    Ok(())
}

async fn replace_text_set(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: uuid::Uuid,
    table: &str,
    column: &str,
    version_id: erpx_agent_orchestration::AgentVersionId,
    values: &std::collections::BTreeSet<String>,
) -> Result<(), PortError> {
    // Table and column values are private compile-time call sites, never user input.
    let delete =
        format!("DELETE FROM erpx_agent_write.{table} WHERE tenant_id=$1 AND agent_version_id=$2");
    sqlx::query(&delete)
        .bind(tenant_id)
        .bind(version_id.as_uuid())
        .execute(&mut **tx)
        .await
        .map_err(|_| PortError::Persistence)?;
    let insert = format!(
        "INSERT INTO erpx_agent_write.{table} (tenant_id,agent_version_id,{column}) VALUES ($1,$2,$3)"
    );
    for value in values {
        sqlx::query(&insert)
            .bind(tenant_id)
            .bind(version_id.as_uuid())
            .bind(value)
            .execute(&mut **tx)
            .await
            .map_err(|_| PortError::Persistence)?;
    }
    Ok(())
}

async fn reserve_budget(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: uuid::Uuid,
    reservation: &BudgetReservation,
    assignment_id: erpx_agent_orchestration::AssignmentId,
) -> Result<(), PortError> {
    for layer in &reservation.layers {
        let result = sqlx::query(
            "UPDATE erpx_agent_write.agent_budget_limits
             SET remaining_units=remaining_units-$3,
                 remaining_cost_micros=remaining_cost_micros-$4,
                 active_concurrency=active_concurrency+1,updated_at=$5
             WHERE layer_kind=$1 AND subject_ref=$2 AND tenant_id=$6 AND enabled=TRUE
               AND remaining_units >= $3 AND remaining_cost_micros >= $4
               AND active_concurrency < concurrency_limit",
        )
        .bind(budget_layer_name(layer.kind))
        .bind(&layer.subject_ref)
        .bind(to_i64(reservation.request.estimated_units)?)
        .bind(to_i64(reservation.request.estimated_cost_micros)?)
        .bind(reservation.reserved_at)
        .bind(tenant_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        if result.rows_affected() != 1 {
            return Err(PortError::BudgetUnavailable);
        }
    }
    sqlx::query(
        "INSERT INTO erpx_agent_write.agent_budget_reservations
            (tenant_id,reservation_id,assignment_id,estimated_units,estimated_cost_micros,
             stop_after_units,queue_priority,reservation_status,reserved_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,'reserved',$8)",
    )
    .bind(tenant_id)
    .bind(reservation.id.as_uuid())
    .bind(assignment_id.as_uuid())
    .bind(to_i64(reservation.request.estimated_units)?)
    .bind(to_i64(reservation.request.estimated_cost_micros)?)
    .bind(to_i64(reservation.request.stop_after_units)?)
    .bind(i32::from(reservation.request.queue_priority))
    .bind(reservation.reserved_at)
    .execute(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    for layer in &reservation.layers {
        sqlx::query(
            "INSERT INTO erpx_agent_write.agent_budget_reservation_layers
                (tenant_id,reservation_id,layer_kind,subject_ref,reserved_units,reserved_cost_micros)
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(tenant_id)
        .bind(reservation.id.as_uuid())
        .bind(budget_layer_name(layer.kind))
        .bind(&layer.subject_ref)
        .bind(to_i64(reservation.request.estimated_units)?)
        .bind(to_i64(reservation.request.estimated_cost_micros)?)
        .execute(&mut **tx)
        .await
        .map_err(|_| PortError::Persistence)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_assignment(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: uuid::Uuid,
    scope: &str,
    assignment: &Assignment,
    reservation: &BudgetReservation,
    prompt_ref: &str,
    payload_manifest: &erpx_agent_orchestration::PayloadManifest,
    delegated: bool,
) -> Result<(), PortError> {
    let trigger = if delegated {
        "delegation"
    } else {
        invocation_name(assignment.root_invocation().origin)
    };
    sqlx::query(
        "INSERT INTO erpx_agent_write.agent_assignments
            (tenant_id,assignment_id,root_assignment_id,parent_assignment_id,scope_key,tenant_scope,
             root_action_ref,root_initiating_principal_ref,trigger_kind,requested_agent_ref,
             root_idempotency_key,agent_definition_id,agent_version_id,provider_ref,model_ref,prompt_digest,
             policy_digest,encrypted_prompt_ref,payload_manifest,assignment_status,depth,
             hops,attempt,budget_reservation_id,queue_priority,aggregate_version,
             created_at,updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,
                 'queued',$20,$21,0,$22,$23,$24,$25,$26)",
    )
    .bind(tenant_id)
    .bind(assignment.id().as_uuid())
    .bind(assignment.root_assignment_id().as_uuid())
    .bind(assignment.parent_assignment_id().map(|id| id.as_uuid()))
    .bind(scope)
    .bind(serde_json::to_value(assignment.scope()).map_err(|_| PortError::Persistence)?)
    .bind(&assignment.root_invocation().source_action_ref)
    .bind(&assignment.root_invocation().initiating_principal_ref)
    .bind(trigger)
    .bind(&assignment.pin().agent_principal_ref)
    .bind(&assignment.root_invocation().idempotency_key)
    .bind(assignment.pin().definition_id.as_uuid())
    .bind(assignment.pin().manifest.version_id.as_uuid())
    .bind(&assignment.pin().manifest.provider_ref)
    .bind(&assignment.pin().manifest.model_ref)
    .bind(&assignment.pin().manifest.prompt_digest)
    .bind(&assignment.pin().manifest.policy_digest)
    .bind(prompt_ref)
    .bind(serde_json::to_value(payload_manifest).map_err(|_| PortError::Persistence)?)
    .bind(i32::from(assignment.depth()))
    .bind(i32::from(assignment.hops()))
    .bind(reservation.id.as_uuid())
    .bind(i32::from(reservation.request.queue_priority))
    .bind(to_i64(assignment.version())?)
    .bind(assignment.created_at())
    .bind(assignment.updated_at())
    .execute(&mut **tx)
    .await
    .map_err(|error| {
        if error
            .as_database_error()
            .is_some_and(|error| error.is_unique_violation())
        {
            PortError::IdempotencyConflict
        } else {
            PortError::Persistence
        }
    })?;
    Ok(())
}

async fn update_assignment(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: uuid::Uuid,
    assignment: &Assignment,
) -> Result<(), PortError> {
    let lease = assignment.lease();
    let failure = assignment.failure();
    let result = sqlx::query(
        "UPDATE erpx_agent_write.agent_assignments SET
            assignment_status=$3,attempt=$4,lease_token=$5,lease_owner=$6,
            lease_acquired_at=$7,lease_expires_at=$8,failure_code=$9,failure_retryable=$10,
            failure_detail_digest=$11,failure_failed_at=$12,output_digest=$13,
            cancellation_reason=$14,aggregate_version=$15,updated_at=$16
         WHERE tenant_id=$1 AND assignment_id=$2 AND aggregate_version=$17",
    )
    .bind(tenant_id)
    .bind(assignment.id().as_uuid())
    .bind(assignment_status_name(assignment.state()))
    .bind(i32::from(assignment.attempt()))
    .bind(lease.map(|lease| lease.token))
    .bind(lease.map(|lease| lease.worker_ref.as_str()))
    .bind(lease.map(|lease| lease.acquired_at))
    .bind(lease.map(|lease| lease.expires_at))
    .bind(failure.map(|failure| failure.code.as_str()))
    .bind(failure.map(|failure| failure.retryable))
    .bind(failure.map(|failure| failure.detail_digest.as_str()))
    .bind(failure.map(|failure| failure.failed_at))
    .bind(assignment.output_digest())
    .bind(assignment.cancellation_reason())
    .bind(to_i64(assignment.version())?)
    .bind(assignment.updated_at())
    .bind(to_i64(
        assignment
            .version()
            .checked_sub(1)
            .ok_or(PortError::VersionConflict)?,
    )?)
    .execute(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    if result.rows_affected() != 1 {
        return Err(PortError::VersionConflict);
    }
    Ok(())
}

async fn release_budget(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: uuid::Uuid,
    reservation_id: erpx_agent_orchestration::BudgetReservationId,
) -> Result<(), PortError> {
    let layers: Vec<(String, String, i64, i64)> = sqlx::query_as(
        "SELECT layer_kind,subject_ref,reserved_units,reserved_cost_micros
         FROM erpx_agent_write.agent_budget_reservation_layers
         WHERE tenant_id=$1 AND reservation_id=$2 FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(reservation_id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let changed = sqlx::query(
        "UPDATE erpx_agent_write.agent_budget_reservations
         SET reservation_status='cancelled',reconciled_at=now()
         WHERE tenant_id=$1 AND reservation_id=$2 AND reservation_status='reserved'",
    )
    .bind(tenant_id)
    .bind(reservation_id.as_uuid())
    .execute(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    if changed.rows_affected() == 1 {
        for (kind, subject, units, cost) in layers {
            sqlx::query(
                "UPDATE erpx_agent_write.agent_budget_limits
                 SET remaining_units=remaining_units+$4,
                     remaining_cost_micros=remaining_cost_micros+$5,
                     active_concurrency=active_concurrency-1,updated_at=now()
                 WHERE tenant_id=$1 AND layer_kind=$2 AND subject_ref=$3
                   AND active_concurrency>0",
            )
            .bind(tenant_id)
            .bind(kind)
            .bind(subject)
            .bind(units)
            .bind(cost)
            .execute(&mut **tx)
            .await
            .map_err(|_| PortError::Persistence)?;
        }
    }
    Ok(())
}

async fn replay_definition(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: uuid::Uuid,
    scope: &str,
    key: &str,
    request_digest: &str,
    expected_id: erpx_agent_orchestration::AgentDefinitionId,
) -> Result<Option<AgentDefinition>, PortError> {
    let receipt: Option<(String, uuid::Uuid)> = sqlx::query_as(
        "SELECT request_digest,aggregate_id FROM erpx_agent_write.agent_command_receipts
         WHERE tenant_id=$1 AND scope_key=$2 AND idempotency_key=$3 FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(scope)
    .bind(key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let Some((stored_digest, stored_id)) = receipt else {
        return Ok(None);
    };
    if stored_digest != request_digest || stored_id != expected_id.as_uuid() {
        return Err(PortError::IdempotencyConflict);
    }
    normalized::load_definition(tx, tenant_id, expected_id).await
}

async fn replay_assignment(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: uuid::Uuid,
    scope: &str,
    key: &str,
    request_digest: &str,
    expected_id: erpx_agent_orchestration::AssignmentId,
) -> Result<Option<Assignment>, PortError> {
    let receipt: Option<(String, uuid::Uuid)> = sqlx::query_as(
        "SELECT request_digest,aggregate_id FROM erpx_agent_write.agent_command_receipts
         WHERE tenant_id=$1 AND scope_key=$2 AND idempotency_key=$3 FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(scope)
    .bind(key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let Some((stored_digest, stored_id)) = receipt else {
        return Ok(None);
    };
    if stored_digest != request_digest || stored_id != expected_id.as_uuid() {
        return Err(PortError::IdempotencyConflict);
    }
    normalized::load_assignment(tx, tenant_id, expected_id).await
}

#[allow(clippy::too_many_arguments)]
async fn insert_receipt(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: uuid::Uuid,
    scope: &str,
    key: &str,
    request_digest: &str,
    aggregate_type: &str,
    aggregate_id: uuid::Uuid,
    aggregate_version: u64,
    committed_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), PortError> {
    sqlx::query(
        "INSERT INTO erpx_agent_write.agent_command_receipts
            (tenant_id,scope_key,idempotency_key,request_digest,aggregate_type,
             aggregate_id,aggregate_version,committed_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(tenant_id)
    .bind(scope)
    .bind(key)
    .bind(request_digest)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(to_i64(aggregate_version)?)
    .bind(committed_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| {
        if error
            .as_database_error()
            .is_some_and(|error| error.is_unique_violation())
        {
            PortError::IdempotencyConflict
        } else {
            PortError::Persistence
        }
    })?;
    Ok(())
}

/// Appends the authoritative, replayable notification request in the same
/// transaction as the assignment state change. The payload intentionally
/// contains only opaque identities, state, and version; protected prompt,
/// provider output, action payload, and failure details are never copied.
pub(crate) async fn append_assignment_state_fact(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: uuid::Uuid,
    assignment: &Assignment,
    actor: serde_json::Value,
    occurred_at: chrono::DateTime<chrono::Utc>,
) -> Result<uuid::Uuid, PortError> {
    let state = assignment_status_name(assignment.state());
    let event_id = stable_fact_uuid(tenant_id, assignment, state, b"event");
    let command_id = stable_fact_uuid(tenant_id, assignment, state, b"command");
    let correlation_id = stable_fact_uuid(tenant_id, assignment, state, b"correlation");
    let audit_id = stable_fact_uuid(tenant_id, assignment, state, b"audit");
    let partition_key = format!("tenant:{tenant_id}");
    let idempotency_key = format!(
        "assignment-state:{}:{}:{state}",
        assignment.id().as_uuid(),
        assignment.version()
    );
    let payload = json!({
        "assignment_id": assignment.id().as_uuid(),
        "root_assignment_id": assignment.root_assignment_id().as_uuid(),
        "recipient_principal_ref": assignment.root_invocation().initiating_principal_ref,
        "agent_principal_ref": assignment.pin().agent_principal_ref,
        "state": state,
        "aggregate_version": assignment.version(),
    });
    let request_digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&payload).map_err(|_| PortError::Persistence)?)
    );
    sqlx::query(
        "INSERT INTO erpx_agent_write.agent_command_receipts
            (tenant_id,scope_key,idempotency_key,request_digest,aggregate_type,
             aggregate_id,aggregate_version,committed_at)
         VALUES ($1,$2,$3,$4,'assignment',$5,$6,$7)
         ON CONFLICT (tenant_id,scope_key,idempotency_key) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(&partition_key)
    .bind(&idempotency_key)
    .bind(&request_digest)
    .bind(assignment.id().as_uuid())
    .bind(to_i64(assignment.version())?)
    .bind(occurred_at)
    .execute(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.audit_log
            (tenant_id,audit_id,context_name,partition_key,command_id,correlation_id,
             tenant_scope,actor,audit_context,action,aggregate_type,aggregate_id,
             aggregate_version,details,occurred_at)
         VALUES ($1,$2,'agent_orchestration',$3,$4,$5,$6,$7,$8,$9,
                 'assignment',$10,$11,$12,$13)
         ON CONFLICT (tenant_id,audit_id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(audit_id)
    .bind(&partition_key)
    .bind(command_id)
    .bind(correlation_id)
    .bind(serde_json::to_value(assignment.scope()).map_err(|_| PortError::Persistence)?)
    .bind(actor.clone())
    .bind(
        json!({"purpose":"agent-assignment-lifecycle","customer_visible":false,
                 "module":"agent-orchestration"}),
    )
    .bind("agent.assignment.state.changed")
    .bind(assignment.id().as_uuid())
    .bind(to_i64(assignment.version())?)
    .bind(json!({"opaque":true,"state":state,"aggregate_version":assignment.version()}))
    .bind(occurred_at)
    .execute(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let inserted = sqlx::query(
        "INSERT INTO erpx_cqrs_write.outbox_events
            (tenant_id,event_id,context_name,partition_key,command_id,event_type,event_version,
             aggregate_type,aggregate_id,aggregate_version,tenant_scope,actor,
             correlation_id,causation_id,payload,occurred_at,available_at)
         VALUES ($1,$2,'agent_orchestration',$3,$4,
                 'agent.assignment.state_notification.requested',1,
                 'assignment',$5,$6,$7,$8,$9,NULL,$10,$11,$11)
         ON CONFLICT (tenant_id,event_id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(event_id)
    .bind(partition_key)
    .bind(command_id)
    .bind(assignment.id().as_uuid())
    .bind(to_i64(assignment.version())?)
    .bind(serde_json::to_value(assignment.scope()).map_err(|_| PortError::Persistence)?)
    .bind(actor.clone())
    .bind(correlation_id)
    .bind(payload)
    .bind(occurred_at)
    .execute(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    if inserted.rows_affected() > 1 {
        return Err(PortError::Persistence);
    }
    crate::publication::append_assignment_directory_draft(
        tx,
        tenant_id,
        assignment,
        actor,
        occurred_at,
    )
    .await?;
    Ok(event_id)
}

fn stable_fact_uuid(
    tenant_id: uuid::Uuid,
    assignment: &Assignment,
    state: &str,
    purpose: &[u8],
) -> uuid::Uuid {
    let mut digest = Sha256::new();
    digest.update(b"erpx.agent.assignment.state.v1");
    digest.update(purpose);
    digest.update(tenant_id.as_bytes());
    digest.update(assignment.id().as_uuid().as_bytes());
    digest.update(assignment.version().to_be_bytes());
    digest.update(state.as_bytes());
    let bytes = digest.finalize();
    let mut uuid_bytes = [0_u8; 16];
    uuid_bytes.copy_from_slice(&bytes[..16]);
    // RFC 4122 variant and a name-derived version marker.
    uuid_bytes[6] = (uuid_bytes[6] & 0x0f) | 0x50;
    uuid_bytes[8] = (uuid_bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(uuid_bytes)
}

fn validate_root_commit(commit: &RootAssignmentCommit) -> Result<(), PortError> {
    if !commit.authority_scope.contains(commit.assignment.scope())
        || commit.assignment.parent_assignment_id().is_some()
        || commit.assignment.root_assignment_id() != commit.assignment.id()
        || commit.payload_manifest.contains_secret
        || commit.prompt_ref.trim().is_empty()
        || commit.reservation.id != commit.assignment.budget_reservation_id()
    {
        Err(PortError::Denied)
    } else {
        Ok(())
    }
}

fn validate_expected(
    expected: Option<u64>,
    current: Option<i64>,
    next: u64,
) -> Result<(), PortError> {
    match (expected, current) {
        (None, None) if next == 1 => Ok(()),
        (Some(expected), Some(current))
            if i64::try_from(expected).ok() == Some(current)
                && next == expected.saturating_add(1) =>
        {
            Ok(())
        }
        _ => Err(PortError::VersionConflict),
    }
}

fn digest(value: &impl serde::Serialize) -> Result<String, PortError> {
    let bytes = serde_json::to_vec(value).map_err(|_| PortError::Internal)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(crate) fn scope_key(scope: &TenantScope) -> String {
    match scope {
        TenantScope::Platform => "platform".into(),
        TenantScope::Holding { holding_id } => format!("holding:{holding_id}"),
        TenantScope::Company {
            holding_id,
            company_id,
        } => {
            format!("company:{holding_id}:{company_id}")
        }
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => {
            format!("branch:{holding_id}:{company_id}:{branch_id}")
        }
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => {
            let mut companies = company_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            let mut branches = branch_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            companies.sort_unstable();
            branches.sort_unstable();
            format!(
                "subtree:{holding_id}:companies={}:branches={}",
                companies.join(","),
                branches.join(",")
            )
        }
    }
}

fn definition_status_name(value: AgentDefinitionStatus) -> &'static str {
    match value {
        AgentDefinitionStatus::Draft => "draft",
        AgentDefinitionStatus::Approved => "approved",
        AgentDefinitionStatus::Suspended => "suspended",
        AgentDefinitionStatus::Retired => "retired",
    }
}

fn version_status_name(value: AgentVersionStatus) -> &'static str {
    match value {
        AgentVersionStatus::Draft => "draft",
        AgentVersionStatus::Approved => "approved",
        AgentVersionStatus::Suspended => "suspended",
        AgentVersionStatus::Retired => "retired",
    }
}

fn assignment_status_name(value: AssignmentState) -> &'static str {
    match value {
        AssignmentState::Queued => "queued",
        AssignmentState::Running => "running",
        AssignmentState::Waiting => "waiting",
        AssignmentState::Completed => "completed",
        AssignmentState::Failed => "failed",
        AssignmentState::Cancelled => "cancelled",
    }
}

fn invocation_name(value: InvocationOrigin) -> &'static str {
    match value {
        InvocationOrigin::Mention => "mention",
        InvocationOrigin::Assign => "assign",
        InvocationOrigin::Run => "run",
        InvocationOrigin::Compare => "compare",
        InvocationOrigin::Delegation => "delegation",
        InvocationOrigin::Schedule | InvocationOrigin::Event => "denied_proactive",
    }
}

fn budget_layer_name(value: BudgetLayerKind) -> &'static str {
    match value {
        BudgetLayerKind::Tenant => "tenant",
        BudgetLayerKind::Workspace => "workspace",
        BudgetLayerKind::Agent => "agent",
        BudgetLayerKind::Principal => "principal",
        BudgetLayerKind::RootAssignment => "root_assignment",
    }
}

fn classification_name(value: erpx_kernel::DataClassification) -> &'static str {
    match value {
        erpx_kernel::DataClassification::Public => "public",
        erpx_kernel::DataClassification::Internal => "internal",
        erpx_kernel::DataClassification::Confidential => "confidential",
        erpx_kernel::DataClassification::Restricted => "restricted",
    }
}

fn to_i64(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::Persistence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_and_delegation_commits_are_atomic_and_idempotent() {
        let source = include_str!("command.rs");
        assert!(source.contains("reserve_budget("));
        assert!(source.contains("normalized::load_assignment("));
        assert!(!source.contains(concat!("aggregate_", "state")));
        assert!(source.contains("insert_receipt("));
        assert!(source.contains("assignment_status='running' FOR UPDATE"));
        assert!(source.contains("PortError::IdempotencyConflict"));
        assert!(source.contains("append_assignment_state_fact("));
        assert!(source.contains("ON CONFLICT (tenant_id,event_id) DO NOTHING"));
        assert!(source.contains("WHERE tenant_id=$1"));
    }

    #[test]
    fn prohibited_triggers_have_no_persistable_mapping() {
        assert_eq!(invocation_name(InvocationOrigin::Event), "denied_proactive");
        assert_eq!(
            invocation_name(InvocationOrigin::Schedule),
            "denied_proactive"
        );
        let ddl =
            include_str!("../../../migration/migrations_tenant_write/0073_agent_orchestration.sql");
        assert!(!ddl.contains("'denied_proactive'"));
    }
}
