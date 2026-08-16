use std::collections::{BTreeMap, BTreeSet};

use erpx_agent_orchestration::{
    ActionCardId, ActionCardProposal, AgentDefinition, AgentDefinitionId, AgentDefinitionStatus,
    AgentManifest, AgentPin, AgentVersion, AgentVersionId, AgentVersionStatus, Assignment,
    AssignmentFailure, AssignmentId, AssignmentLease, AssignmentState, BudgetReservationId,
    Checkpoint, CheckpointId, InvocationOrigin, PortError, PrincipalKind, ProposedActionStatus,
    RootInvocation,
};
use erpx_kernel::{DataClassification, TenantScope};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

pub(crate) async fn load_definition(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    id: AgentDefinitionId,
) -> Result<Option<AgentDefinition>, PortError> {
    let row = sqlx::query(
        "SELECT tenant_scope,agent_principal_ref,business_owner_ref,technical_owner_ref,
                agent_name,definition_status,aggregate_version,created_at,updated_at
         FROM erpx_agent_write.agent_definitions
         WHERE tenant_id=$1 AND agent_definition_id=$2",
    )
    .bind(tenant_id)
    .bind(id.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let Some(row) = row else { return Ok(None) };
    let version_rows = sqlx::query(
        "SELECT agent_version_id,version_number,version_status,purpose,provider_ref,
                model_ref,use_case,prompt_digest,policy_digest,maximum_classification,
                security_review_ref,privacy_review_ref,max_attempts,timeout_seconds,
                approved_by,approved_at
         FROM erpx_agent_write.agent_versions
         WHERE tenant_id=$1 AND agent_definition_id=$2
         ORDER BY version_number,agent_version_id",
    )
    .bind(tenant_id)
    .bind(id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let mut versions = BTreeMap::new();
    for version_row in version_rows {
        let version_id = AgentVersionId::try_from_uuid(
            version_row
                .try_get("agent_version_id")
                .map_err(|_| PortError::Persistence)?,
        )
        .map_err(|_| PortError::Persistence)?;
        let capabilities = load_text_set(
            tx,
            tenant_id,
            "agent_version_capabilities",
            "capability",
            version_id,
        )
        .await?;
        let tool_refs =
            load_text_set(tx, tenant_id, "agent_version_tools", "tool_ref", version_id).await?;
        let knowledge_scopes = load_text_set(
            tx,
            tenant_id,
            "agent_version_knowledge_scopes",
            "knowledge_scope",
            version_id,
        )
        .await?;
        let evaluation_refs = load_text_set(
            tx,
            tenant_id,
            "agent_version_evaluations",
            "evaluation_ref",
            version_id,
        )
        .await?;
        let organization_scope_values: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT tenant_scope
             FROM erpx_agent_write.agent_version_organization_scopes
             WHERE tenant_id=$1 AND agent_version_id=$2 ORDER BY scope_key",
        )
        .bind(tenant_id)
        .bind(version_id.as_uuid())
        .fetch_all(&mut **tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        let organization_scopes = organization_scope_values
            .into_iter()
            .map(|value| serde_json::from_value(value).map_err(|_| PortError::Persistence))
            .collect::<Result<Vec<TenantScope>, _>>()?;
        let manifest = AgentManifest {
            version_id,
            version_number: positive_u32(
                version_row
                    .try_get("version_number")
                    .map_err(|_| PortError::Persistence)?,
            )?,
            purpose: version_row
                .try_get("purpose")
                .map_err(|_| PortError::Persistence)?,
            provider_ref: version_row
                .try_get("provider_ref")
                .map_err(|_| PortError::Persistence)?,
            model_ref: version_row
                .try_get("model_ref")
                .map_err(|_| PortError::Persistence)?,
            use_case: version_row
                .try_get("use_case")
                .map_err(|_| PortError::Persistence)?,
            prompt_digest: version_row
                .try_get("prompt_digest")
                .map_err(|_| PortError::Persistence)?,
            policy_digest: version_row
                .try_get("policy_digest")
                .map_err(|_| PortError::Persistence)?,
            capabilities,
            tool_refs,
            knowledge_scopes,
            organization_scopes,
            maximum_classification: parse_classification(
                version_row
                    .try_get("maximum_classification")
                    .map_err(|_| PortError::Persistence)?,
            )?,
            security_review_ref: version_row
                .try_get("security_review_ref")
                .map_err(|_| PortError::Persistence)?,
            privacy_review_ref: version_row
                .try_get("privacy_review_ref")
                .map_err(|_| PortError::Persistence)?,
            evaluation_refs,
            max_attempts: positive_u16(
                version_row
                    .try_get("max_attempts")
                    .map_err(|_| PortError::Persistence)?,
            )?,
            timeout_seconds: positive_u32(
                version_row
                    .try_get("timeout_seconds")
                    .map_err(|_| PortError::Persistence)?,
            )?,
        };
        let version = AgentVersion {
            manifest,
            status: parse_version_status(
                version_row
                    .try_get("version_status")
                    .map_err(|_| PortError::Persistence)?,
            )?,
            approved_by: version_row
                .try_get("approved_by")
                .map_err(|_| PortError::Persistence)?,
            approved_at: version_row
                .try_get("approved_at")
                .map_err(|_| PortError::Persistence)?,
        };
        if versions.insert(version_id, version).is_some() {
            return Err(PortError::Persistence);
        }
    }
    AgentDefinition::rehydrate(
        id,
        serde_json::from_value(
            row.try_get("tenant_scope")
                .map_err(|_| PortError::Persistence)?,
        )
        .map_err(|_| PortError::Persistence)?,
        row.try_get("agent_principal_ref")
            .map_err(|_| PortError::Persistence)?,
        row.try_get("business_owner_ref")
            .map_err(|_| PortError::Persistence)?,
        row.try_get("technical_owner_ref")
            .map_err(|_| PortError::Persistence)?,
        row.try_get("agent_name")
            .map_err(|_| PortError::Persistence)?,
        parse_definition_status(
            row.try_get("definition_status")
                .map_err(|_| PortError::Persistence)?,
        )?,
        versions,
        positive_u64(
            row.try_get("aggregate_version")
                .map_err(|_| PortError::Persistence)?,
        )?,
        row.try_get("created_at")
            .map_err(|_| PortError::Persistence)?,
        row.try_get("updated_at")
            .map_err(|_| PortError::Persistence)?,
    )
    .map(Some)
    .map_err(|_| PortError::Persistence)
}

pub(crate) async fn load_assignment(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    id: AssignmentId,
) -> Result<Option<Assignment>, PortError> {
    let row = sqlx::query(
        "SELECT root_assignment_id,parent_assignment_id,tenant_scope,agent_definition_id,
                agent_version_id,assignment_status,depth,hops,attempt,lease_token,lease_owner,
                lease_acquired_at,lease_expires_at,output_digest,failure_code,failure_retryable,
                failure_detail_digest,failure_failed_at,cancellation_reason,budget_reservation_id,
                aggregate_version,created_at,updated_at
         FROM erpx_agent_write.agent_assignments
         WHERE tenant_id=$1 AND assignment_id=$2",
    )
    .bind(tenant_id)
    .bind(id.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let Some(row) = row else { return Ok(None) };
    let root_id = AssignmentId::try_from_uuid(
        row.try_get("root_assignment_id")
            .map_err(|_| PortError::Persistence)?,
    )
    .map_err(|_| PortError::Persistence)?;
    let root_row = sqlx::query(
        "SELECT root_action_ref,root_initiating_principal_ref,root_idempotency_key,trigger_kind
         FROM erpx_agent_write.agent_assignments
         WHERE tenant_id=$1 AND assignment_id=$2",
    )
    .bind(tenant_id)
    .bind(root_id.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?
    .ok_or(PortError::Persistence)?;
    let definition_id = AgentDefinitionId::try_from_uuid(
        row.try_get("agent_definition_id")
            .map_err(|_| PortError::Persistence)?,
    )
    .map_err(|_| PortError::Persistence)?;
    let version_id = AgentVersionId::try_from_uuid(
        row.try_get("agent_version_id")
            .map_err(|_| PortError::Persistence)?,
    )
    .map_err(|_| PortError::Persistence)?;
    let definition = load_definition(tx, tenant_id, definition_id)
        .await?
        .ok_or(PortError::Persistence)?;
    let version = definition
        .versions()
        .get(&version_id)
        .ok_or(PortError::Persistence)?;
    let pin = AgentPin {
        definition_id,
        agent_principal_ref: definition.agent_principal_ref().to_owned(),
        manifest: version.manifest.clone(),
    };
    let lineage_ids: Vec<Uuid> = sqlx::query_scalar(
        "WITH RECURSIVE lineage AS (
             SELECT assignment_id,parent_assignment_id,agent_definition_id,depth
             FROM erpx_agent_write.agent_assignments
             WHERE tenant_id=$1 AND assignment_id=$2
             UNION ALL
             SELECT parent.assignment_id,parent.parent_assignment_id,
                    parent.agent_definition_id,parent.depth
             FROM erpx_agent_write.agent_assignments parent
             JOIN lineage child ON child.parent_assignment_id=parent.assignment_id
             WHERE parent.tenant_id=$1
         ) SELECT agent_definition_id FROM lineage ORDER BY depth,assignment_id",
    )
    .bind(tenant_id)
    .bind(id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let lineage = lineage_ids
        .into_iter()
        .map(AgentDefinitionId::try_from_uuid)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| PortError::Persistence)?;
    let checkpoint_rows = sqlx::query(
        "SELECT checkpoint_id,step,state_digest,retry_safe,created_at
         FROM erpx_agent_write.assignment_checkpoints
         WHERE tenant_id=$1 AND assignment_id=$2 ORDER BY step,checkpoint_id",
    )
    .bind(tenant_id)
    .bind(id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let checkpoints = checkpoint_rows
        .into_iter()
        .map(|checkpoint| {
            Ok(Checkpoint {
                checkpoint_id: CheckpointId::try_from_uuid(
                    checkpoint
                        .try_get("checkpoint_id")
                        .map_err(|_| PortError::Persistence)?,
                )
                .map_err(|_| PortError::Persistence)?,
                step: positive_u32(
                    checkpoint
                        .try_get("step")
                        .map_err(|_| PortError::Persistence)?,
                )?,
                state_digest: checkpoint
                    .try_get("state_digest")
                    .map_err(|_| PortError::Persistence)?,
                retry_safe: checkpoint
                    .try_get("retry_safe")
                    .map_err(|_| PortError::Persistence)?,
                created_at: checkpoint
                    .try_get("created_at")
                    .map_err(|_| PortError::Persistence)?,
            })
        })
        .collect::<Result<Vec<_>, PortError>>()?;
    let lease_token: Option<Uuid> = row
        .try_get("lease_token")
        .map_err(|_| PortError::Persistence)?;
    let lease = lease_token
        .map(|token| {
            Ok(AssignmentLease {
                token,
                worker_ref: required_optional(
                    row.try_get("lease_owner")
                        .map_err(|_| PortError::Persistence)?,
                )?,
                acquired_at: required_optional(
                    row.try_get("lease_acquired_at")
                        .map_err(|_| PortError::Persistence)?,
                )?,
                expires_at: required_optional(
                    row.try_get("lease_expires_at")
                        .map_err(|_| PortError::Persistence)?,
                )?,
            })
        })
        .transpose()?;
    let failure_code: Option<String> = row
        .try_get("failure_code")
        .map_err(|_| PortError::Persistence)?;
    let failure = failure_code
        .map(|code| {
            Ok(AssignmentFailure {
                code,
                retryable: required_optional(
                    row.try_get("failure_retryable")
                        .map_err(|_| PortError::Persistence)?,
                )?,
                detail_digest: required_optional(
                    row.try_get("failure_detail_digest")
                        .map_err(|_| PortError::Persistence)?,
                )?,
                failed_at: required_optional(
                    row.try_get("failure_failed_at")
                        .map_err(|_| PortError::Persistence)?,
                )?,
            })
        })
        .transpose()?;
    Assignment::rehydrate(
        id,
        root_id,
        row.try_get::<Option<Uuid>, _>("parent_assignment_id")
            .map_err(|_| PortError::Persistence)?
            .map(AssignmentId::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Persistence)?,
        RootInvocation {
            initiating_principal_ref: root_row
                .try_get("root_initiating_principal_ref")
                .map_err(|_| PortError::Persistence)?,
            initiating_principal_kind: PrincipalKind::Human,
            origin: parse_invocation_origin(
                root_row
                    .try_get("trigger_kind")
                    .map_err(|_| PortError::Persistence)?,
            )?,
            source_action_ref: root_row
                .try_get("root_action_ref")
                .map_err(|_| PortError::Persistence)?,
            idempotency_key: root_row
                .try_get("root_idempotency_key")
                .map_err(|_| PortError::Persistence)?,
        },
        pin,
        serde_json::from_value(
            row.try_get("tenant_scope")
                .map_err(|_| PortError::Persistence)?,
        )
        .map_err(|_| PortError::Persistence)?,
        parse_assignment_state(
            row.try_get("assignment_status")
                .map_err(|_| PortError::Persistence)?,
        )?,
        lineage,
        nonnegative_u16(row.try_get("depth").map_err(|_| PortError::Persistence)?)?,
        nonnegative_u16(row.try_get("hops").map_err(|_| PortError::Persistence)?)?,
        nonnegative_u16(row.try_get("attempt").map_err(|_| PortError::Persistence)?)?,
        lease,
        checkpoints,
        failure,
        row.try_get("output_digest")
            .map_err(|_| PortError::Persistence)?,
        row.try_get("cancellation_reason")
            .map_err(|_| PortError::Persistence)?,
        BudgetReservationId::try_from_uuid(
            row.try_get("budget_reservation_id")
                .map_err(|_| PortError::Persistence)?,
        )
        .map_err(|_| PortError::Persistence)?,
        positive_u64(
            row.try_get("aggregate_version")
                .map_err(|_| PortError::Persistence)?,
        )?,
        row.try_get("created_at")
            .map_err(|_| PortError::Persistence)?,
        row.try_get("updated_at")
            .map_err(|_| PortError::Persistence)?,
    )
    .map(Some)
    .map_err(|_| PortError::Persistence)
}

pub(crate) async fn load_action_card(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    id: ActionCardId,
) -> Result<Option<(TenantScope, ActionCardProposal)>, PortError> {
    let row = sqlx::query(
        "SELECT a.tenant_scope,c.assignment_id,c.agent_principal_ref,c.owner_context,
                c.command_type,c.payload_digest,c.action_status,c.confirmed_by_human_ref
         FROM erpx_agent_write.agent_action_cards c
         JOIN erpx_agent_write.agent_assignments a
           ON a.tenant_id=c.tenant_id AND a.assignment_id=c.assignment_id
         WHERE c.tenant_id=$1 AND c.action_card_id=$2",
    )
    .bind(tenant_id)
    .bind(id.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let Some(row) = row else { return Ok(None) };
    let scope: TenantScope = serde_json::from_value(
        row.try_get("tenant_scope")
            .map_err(|_| PortError::Persistence)?,
    )
    .map_err(|_| PortError::Persistence)?;
    let status = match row
        .try_get::<String, _>("action_status")
        .map_err(|_| PortError::Persistence)?
        .as_str()
    {
        "proposed" => ProposedActionStatus::Proposed,
        "rejected" => ProposedActionStatus::Rejected,
        "confirmed_for_handoff" => ProposedActionStatus::ConfirmedForHandoff,
        _ => return Err(PortError::Persistence),
    };
    let card = ActionCardProposal::rehydrate(
        id,
        AssignmentId::try_from_uuid(
            row.try_get("assignment_id")
                .map_err(|_| PortError::Persistence)?,
        )
        .map_err(|_| PortError::Persistence)?,
        row.try_get("agent_principal_ref")
            .map_err(|_| PortError::Persistence)?,
        row.try_get("owner_context")
            .map_err(|_| PortError::Persistence)?,
        row.try_get("command_type")
            .map_err(|_| PortError::Persistence)?,
        row.try_get("payload_digest")
            .map_err(|_| PortError::Persistence)?,
        status,
        row.try_get("confirmed_by_human_ref")
            .map_err(|_| PortError::Persistence)?,
    )
    .map_err(|_| PortError::Persistence)?;
    Ok(Some((scope, card)))
}

async fn load_text_set(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    table: &str,
    column: &str,
    version_id: AgentVersionId,
) -> Result<BTreeSet<String>, PortError> {
    let statement = format!(
        "SELECT {column} FROM erpx_agent_write.{table}
         WHERE tenant_id=$1 AND agent_version_id=$2 ORDER BY {column}"
    );
    let values: Vec<String> = sqlx::query_scalar(&statement)
        .bind(tenant_id)
        .bind(version_id.as_uuid())
        .fetch_all(&mut **tx)
        .await
        .map_err(|_| PortError::Persistence)?;
    let unique = values.iter().collect::<BTreeSet<_>>();
    if unique.len() != values.len() {
        return Err(PortError::Persistence);
    }
    Ok(values.into_iter().collect())
}

fn parse_definition_status(value: String) -> Result<AgentDefinitionStatus, PortError> {
    match value.as_str() {
        "draft" => Ok(AgentDefinitionStatus::Draft),
        "approved" => Ok(AgentDefinitionStatus::Approved),
        "suspended" => Ok(AgentDefinitionStatus::Suspended),
        "retired" => Ok(AgentDefinitionStatus::Retired),
        _ => Err(PortError::Persistence),
    }
}
fn parse_version_status(value: String) -> Result<AgentVersionStatus, PortError> {
    match value.as_str() {
        "draft" => Ok(AgentVersionStatus::Draft),
        "approved" => Ok(AgentVersionStatus::Approved),
        "suspended" => Ok(AgentVersionStatus::Suspended),
        "retired" => Ok(AgentVersionStatus::Retired),
        _ => Err(PortError::Persistence),
    }
}
fn parse_assignment_state(value: String) -> Result<AssignmentState, PortError> {
    match value.as_str() {
        "queued" => Ok(AssignmentState::Queued),
        "running" => Ok(AssignmentState::Running),
        "waiting" => Ok(AssignmentState::Waiting),
        "completed" => Ok(AssignmentState::Completed),
        "failed" => Ok(AssignmentState::Failed),
        "cancelled" => Ok(AssignmentState::Cancelled),
        _ => Err(PortError::Persistence),
    }
}
fn parse_invocation_origin(value: String) -> Result<InvocationOrigin, PortError> {
    match value.as_str() {
        "mention" => Ok(InvocationOrigin::Mention),
        "assign" => Ok(InvocationOrigin::Assign),
        "run" => Ok(InvocationOrigin::Run),
        "compare" => Ok(InvocationOrigin::Compare),
        _ => Err(PortError::Persistence),
    }
}
fn parse_classification(value: String) -> Result<DataClassification, PortError> {
    match value.as_str() {
        "public" => Ok(DataClassification::Public),
        "internal" => Ok(DataClassification::Internal),
        "confidential" => Ok(DataClassification::Confidential),
        "restricted" => Ok(DataClassification::Restricted),
        _ => Err(PortError::Persistence),
    }
}
fn required_optional<T>(value: Option<T>) -> Result<T, PortError> {
    value.ok_or(PortError::Persistence)
}
fn positive_u64(value: i64) -> Result<u64, PortError> {
    if value <= 0 {
        Err(PortError::Persistence)
    } else {
        u64::try_from(value).map_err(|_| PortError::Persistence)
    }
}
fn positive_u32(value: i32) -> Result<u32, PortError> {
    if value <= 0 {
        Err(PortError::Persistence)
    } else {
        u32::try_from(value).map_err(|_| PortError::Persistence)
    }
}
fn positive_u16(value: i32) -> Result<u16, PortError> {
    if value <= 0 {
        Err(PortError::Persistence)
    } else {
        u16::try_from(value).map_err(|_| PortError::Persistence)
    }
}
fn nonnegative_u16(value: i32) -> Result<u16, PortError> {
    u16::try_from(value).map_err(|_| PortError::Persistence)
}

#[cfg(test)]
mod tests {
    #[test]
    fn loader_uses_only_normalized_authoritative_tables() {
        let source = include_str!("normalized.rs");
        assert!(source.contains("agent_definitions"));
        assert!(source.contains("agent_versions"));
        assert!(source.contains("agent_assignments"));
        assert!(source.contains("assignment_checkpoints"));
        assert!(!source.contains(concat!("aggregate_", "state")));
        assert!(!source.contains(concat!("agent_definition_", "snap", "shots")));
        assert!(!source.contains(concat!("assignment_", "snap", "shots")));
    }
}
