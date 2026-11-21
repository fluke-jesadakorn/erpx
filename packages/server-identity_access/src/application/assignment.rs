//! Identity Access assignment use cases.
//!
//! Ports supply the current assigning authority, the owning serializable
//! transaction, replay and persisted readback. This layer never opens a
//! transaction or reads an HTTP request.
use crate::domain::assignment::{
    AssignmentActor, AssignmentGuardFacts, AssignmentReceipt, AssignmentRevocation,
    AssignmentRoleRow, AssignmentRow, MAX_WORKSPACE_ROWS, REVOKE_OPERATION, ReceiptSnapshot,
    RevocationPlan, access_changed, assignment_changed, assignment_not_found, idempotency_conflict,
    last_administrator, last_recovery_administrator, readback_invalid, request_invalid,
    revocation_identity, scope_forbidden, self_access_change, source_invalid,
};
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId};
use serde_json::Value;
use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;

pub type AssignmentFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

fn receipt_value(
    actor: &AssignmentActor,
    id: &str,
    operation: &str,
    request_hash: &str,
    receipt: Option<ReceiptSnapshot>,
) -> Result<Option<Value>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_hash != request_hash
        || receipt.operation != operation
        || receipt.actor_id != actor.user_id.as_str()
        || receipt.record_id != id
    {
        return Err(idempotency_conflict());
    }
    let authority: Value =
        serde_json::from_str(&receipt.authority_json).map_err(|_| readback_invalid())?;
    if authority["assignmentId"].as_str() != Some(actor.assignment_id.as_str())
        || authority["companyId"].as_str() != Some(actor.company_id.as_str())
    {
        return Err(idempotency_conflict());
    }
    let parsed: AssignmentReceipt =
        serde_json::from_str(&receipt.result_json).map_err(|_| readback_invalid())?;
    if parsed.id != id || parsed.status != "revoked" || parsed.version < 2 {
        return Err(readback_invalid());
    }
    Ok(Some(parsed.to_wire()))
}

fn require_scope(row: &AssignmentRow, actor: &AssignmentActor) -> Result<(), BusinessError> {
    if !actor.can_withdraw(row) {
        return Err(scope_forbidden());
    }
    Ok(())
}

fn validate_workspace_roles(rows: Vec<AssignmentRoleRow>) -> Result<Vec<Value>, BusinessError> {
    if rows.len() > MAX_WORKSPACE_ROWS {
        return Err(source_invalid());
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        row.validate_source()?;
        if !seen.insert(row.id.clone()) {
            return Err(source_invalid());
        }
        out.push(row.to_wire());
    }
    Ok(out)
}

fn validate_workspace_rows(
    rows: Vec<AssignmentRow>,
    actor: &AssignmentActor,
) -> Result<Vec<Value>, BusinessError> {
    if rows.len() > MAX_WORKSPACE_ROWS {
        return Err(source_invalid());
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        row.validate_source()?;
        if row.company_id != actor.company_id.as_str() || !actor.can_withdraw(&row) {
            return Err(source_invalid());
        }
        if !seen.insert(row.id.clone()) {
            return Err(source_invalid());
        }
        out.push(row.to_wire());
    }
    Ok(out)
}

pub trait AssignmentRepository {
    type Error: From<BusinessError>;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a AssignmentActor,
        write: bool,
        now: i64,
    ) -> AssignmentFuture<'a, bool, Self::Error>;
    fn digest<'a>(&'a mut self, value: &'a str) -> AssignmentFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> AssignmentFuture<'a, Option<ReceiptSnapshot>, Self::Error>;
    fn workspace_rows<'a>(
        &'a mut self,
        actor: &'a AssignmentActor,
        now: i64,
    ) -> AssignmentFuture<'a, Vec<AssignmentRow>, Self::Error>;
    fn workspace_roles<'a>(
        &'a mut self,
        actor: &'a AssignmentActor,
        role_ids: &'a [String],
    ) -> AssignmentFuture<'a, Vec<AssignmentRoleRow>, Self::Error>;
    fn assignment_for_update<'a>(
        &'a mut self,
        actor: &'a AssignmentActor,
        id: &'a str,
        now: i64,
    ) -> AssignmentFuture<'a, Option<AssignmentRow>, Self::Error>;
    fn guard_facts<'a>(
        &'a mut self,
        actor: &'a AssignmentActor,
        row: &'a AssignmentRow,
        now: i64,
    ) -> AssignmentFuture<'a, AssignmentGuardFacts, Self::Error>;
    fn commit_revoke<'a>(
        &'a mut self,
        actor: &'a AssignmentActor,
        key: &'a str,
        plan: &'a RevocationPlan,
        request_hash: &'a str,
    ) -> AssignmentFuture<'a, Value, Self::Error>;
}

async fn require_current<R: AssignmentRepository>(
    repository: &mut R,
    actor: &AssignmentActor,
    write: bool,
    now: i64,
) -> Result<(), R::Error> {
    if !repository.current_authority(actor, write, now).await? {
        return Err(access_changed().into());
    }
    Ok(())
}

pub async fn assignment_workspace<R: AssignmentRepository>(
    repository: &mut R,
    actor: &AssignmentActor,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_read(now)?;
    require_current(repository, actor, false, now).await?;
    let rows = repository.workspace_rows(actor, now).await?;
    let role_ids = rows
        .iter()
        .map(|row| row.role.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let role_rows = if role_ids.is_empty() {
        Vec::new()
    } else {
        repository.workspace_roles(actor, &role_ids).await?
    };
    let assignments = validate_workspace_rows(rows, actor)?;
    let roles = validate_workspace_roles(role_rows)?;
    Ok(serde_json::json!({
        "assignments": assignments,
        "roles": roles,
        "evaluatedAt": now,
        "currentAssignmentId": actor.assignment_id.as_str(),
        "currentUserId": actor.user_id.as_str(),
        "scopeKind": actor.scope_kind,
        "scopeNodeId": actor.scope_node_id.as_ref().map(RecordId::as_str),
    }))
}

pub async fn revoke_assignment<R: AssignmentRepository>(
    repository: &mut R,
    actor: &AssignmentActor,
    key: &CommandKey,
    id: &str,
    input: &AssignmentRevocation,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    let input = input.clone().validate()?;
    if RecordId::new(id.to_owned()).is_err() {
        return Err(assignment_not_found().into());
    }
    require_current(repository, actor, true, now).await?;
    let identity = revocation_identity(actor, key.as_str(), id, &input, REVOKE_OPERATION);
    let hash = repository.digest(&identity).await?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        id,
        REVOKE_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let row = repository
        .assignment_for_update(actor, id, now)
        .await?
        .ok_or_else(assignment_not_found)?;
    row.validate_source()?;
    if row.company_id != actor.company_id.as_str() {
        return Err(assignment_not_found().into());
    }
    require_scope(&row, actor)?;
    if let Some(prior) = receipt_value(
        actor,
        id,
        REVOKE_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let facts = repository.guard_facts(actor, &row, now).await?;
    if actor.same_person_user(&row.user_id) || facts.same_person {
        return Err(self_access_change().into());
    }
    if row.revoked_at.is_some() || row.version != input.expected_version {
        return Err(assignment_changed().into());
    }
    if facts.recovery_member && !facts.other_recovery_administrator {
        return Err(last_recovery_administrator().into());
    }
    if row.role == "holding_owner" && !facts.other_administrator {
        return Err(last_administrator().into());
    }
    if input.reason.trim().is_empty() {
        return Err(request_invalid().into());
    }
    let plan = RevocationPlan {
        id: row.id.clone(),
        user_id: row.user_id.clone(),
        role: row.role.clone(),
        company_id: row.company_id.clone(),
        branch_id: row.branch_id.clone(),
        department_id: if row.scope_kind == "department" {
            row.scope_node_id.clone()
        } else {
            None
        },
        assignment_version: input.expected_version + 1,
        expected_version: input.expected_version,
        scope_kind: row.scope_kind.clone(),
        scope_node_id: row.scope_node_id.clone(),
        reason: input.reason.clone(),
        revoked_at: now,
    };
    repository
        .commit_revoke(actor, key.as_str(), &plan, &hash)
        .await
}
