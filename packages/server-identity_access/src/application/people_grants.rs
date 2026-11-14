//! People access preview/apply use cases.
//!
//! Ports supply current authority, the serializable owner transaction, replay,
//! the reviewed snapshot, delegation facts and persisted readback. This layer
//! never opens a transaction or reads an HTTP request.
use crate::domain::people_grants::{
    GrantApplyPlan, GrantPreviewPlan, GrantPreviewRow, GrantReceipt,
    GrantSnapshot, MAX_ASSIGNMENTS, MAX_ROLES, MAX_SUBJECTS, MAX_UNITS, PREVIEW_LIFETIME_MS,
    PeopleGrantActor, PeopleGrantChange, access_object_changed, grant_apply_identity,
    grant_id, idempotency_conflict, invitation_id, preview_changed, preview_expired,
    preview_summary, readback_invalid, request_invalid, source_invalid, valid_token,
    validate_change, validate_snapshot, GRANT_APPLY_OPERATION,
};
use crate::domain::permission_catalog::PermissionCatalog;
use erpx_server_shared::public::{BusinessError, CommandKey};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type PeopleGrantFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait PeopleGrantRepository {
    type Error: From<BusinessError>;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a PeopleGrantActor,
        permission: &'a str,
        now: i64,
    ) -> PeopleGrantFuture<'a, bool, Self::Error>;
    fn digest<'a>(&'a mut self, value: &'a str) -> PeopleGrantFuture<'a, String, Self::Error>;
    fn snapshot<'a>(
        &'a mut self,
        actor: &'a PeopleGrantActor,
    ) -> PeopleGrantFuture<'a, GrantSnapshot, Self::Error>;
    /// Alias-aware self check the reviewed snapshot cannot decide on its own.
    fn same_person<'a>(
        &'a mut self,
        actor: &'a PeopleGrantActor,
        user_id: &'a str,
    ) -> PeopleGrantFuture<'a, bool, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> PeopleGrantFuture<'a, Option<GrantReceipt>, Self::Error>;
    fn commit_preview<'a>(
        &'a mut self,
        actor: &'a PeopleGrantActor,
        plan: &'a GrantPreviewPlan,
    ) -> PeopleGrantFuture<'a, String, Self::Error>;
    fn preview_for_update<'a>(
        &'a mut self,
        actor: &'a PeopleGrantActor,
        token: &'a str,
        now: i64,
    ) -> PeopleGrantFuture<'a, Option<GrantPreviewRow>, Self::Error>;
    fn consume_preview<'a>(
        &'a mut self,
        actor: &'a PeopleGrantActor,
        token: &'a str,
    ) -> PeopleGrantFuture<'a, (), Self::Error>;
    fn commit_apply<'a>(
        &'a mut self,
        actor: &'a PeopleGrantActor,
        key: &'a str,
        plan: &'a GrantApplyPlan,
        request_hash: &'a str,
    ) -> PeopleGrantFuture<'a, Value, Self::Error>;
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn bounded(snapshot: &GrantSnapshot) -> Result<(), BusinessError> {
    if snapshot.subjects.len() > MAX_SUBJECTS
        || snapshot.roles.len() > MAX_ROLES
        || snapshot.assignments.len() > MAX_ASSIGNMENTS
        || snapshot.units.len() > MAX_UNITS
    {
        return Err(source_invalid());
    }
    Ok(())
}

async fn require_current<R: PeopleGrantRepository>(
    repository: &mut R,
    actor: &PeopleGrantActor,
    permission: &str,
    now: i64,
) -> Result<(), R::Error> {
    if !repository.current_authority(actor, permission, now).await? {
        return Err(access_object_changed().into());
    }
    Ok(())
}

/// Refuse a command for a platform or unselected Company actor before any read.
fn require_scope(actor: &PeopleGrantActor) -> Result<(), BusinessError> {
    if actor.platform || actor.company_id.is_empty() {
        return Err(crate::domain::people_grants::company_scope_required());
    }
    Ok(())
}

/// Validate, fingerprint and persist a reviewed access change.
pub async fn preview_grant_change<R: PeopleGrantRepository>(
    repository: &mut R,
    actor: &PeopleGrantActor,
    catalog: &PermissionCatalog,
    input: Value,
    now: i64,
) -> Result<Value, R::Error> {
    require_scope(actor)?;
    let change = validate_change(
        serde_json::from_value::<PeopleGrantChange>(input).map_err(|_| request_invalid())?,
    )?;
    let permission = change.required_permission();
    if !actor.has(permission) {
        return Err(crate::domain::people_grants::permission_required().into());
    }
    require_current(repository, actor, permission, now).await?;
    let snapshot = repository.snapshot(actor).await?;
    bounded(&snapshot)?;
    validate_snapshot(&snapshot, actor, catalog, &change, now)?;
    if let Some(user_id) = match &change {
        PeopleGrantChange::Revoke { .. } => snapshot
            .assignment(change.caps_id().unwrap_or_default())
            .map(|row| row.user_id.clone()),
        _ => None,
    } && repository.same_person(actor, &user_id).await?
    {
        return Err(crate::domain::people_grants::self_access_change().into());
    }
    let change_json = serde_json::to_string(&change).map_err(|_| readback_invalid())?;
    let fingerprint = repository
        .digest(&snapshot.fingerprint_input()?)
        .await?;
    if !valid_hash(&fingerprint) {
        return Err(readback_invalid().into());
    }
    let summary = preview_summary(&snapshot, &change);
    let expires_at = now + PREVIEW_LIFETIME_MS;
    let plan = GrantPreviewPlan {
        company_id: actor.company_id.clone(),
        actor_id: actor.user_id.clone(),
        assignment_id: actor.assignment_id.clone(),
        session_id: actor.session_id.clone(),
        change_json,
        fingerprint,
        expires_at,
        now,
    };
    let token = repository.commit_preview(actor, &plan).await?;
    if !valid_token(&token) {
        return Err(readback_invalid().into());
    }
    Ok(serde_json::json!({
        "token": token,
        "change": change,
        "summary": summary,
        "affected": [],
        "expiresAt": expires_at,
    }))
}

/// A consumed preview cannot be re-read, so the recorded receipt is the only
/// replay source. The same Idempotency-Key with the same request identity
/// returns the recorded result; any other reuse is a conflict.
fn receipt_value(
    actor: &PeopleGrantActor,
    request_hash: &str,
    receipt: Option<GrantReceipt>,
) -> Result<Option<Value>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_hash != request_hash
        || receipt.operation != GRANT_APPLY_OPERATION
        || receipt.actor_id != actor.user_id
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
    let parsed: Value =
        serde_json::from_str(&receipt.result_json).map_err(|_| readback_invalid())?;
    if parsed["id"].as_str() != Some(receipt.record_id.as_str()) {
        return Err(readback_invalid());
    }
    Ok(Some(parsed))
}

/// Apply exactly one reviewed access change. The preview token is single-use:
/// the row is consumed in the same serializable unit that writes the account,
/// role, assignment, invitation, receipt, audit and outbox records.
pub async fn apply_grant_change<R: PeopleGrantRepository>(
    repository: &mut R,
    actor: &PeopleGrantActor,
    catalog: &PermissionCatalog,
    key: &CommandKey,
    token: &str,
    now: i64,
) -> Result<Value, R::Error> {
    require_scope(actor)?;
    if !valid_token(token) {
        return Err(preview_expired().into());
    }
    let identity = grant_apply_identity(actor, key.as_str(), token);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(readback_invalid().into());
    }
    if let Some(prior) = receipt_value(actor, &hash, repository.receipt(key.as_str()).await?)? {
        return Ok(prior);
    }
    let Some(preview) = repository.preview_for_update(actor, token, now).await? else {
        return Err(preview_expired().into());
    };
    let change = validate_change(
        serde_json::from_str::<PeopleGrantChange>(&preview.change_json)
            .map_err(|_| readback_invalid())?,
    )?;
    let permission = change.required_permission();
    if !actor.has(permission) {
        return Err(crate::domain::people_grants::permission_required().into());
    }
    require_current(repository, actor, permission, now).await?;
    let snapshot = repository.snapshot(actor).await?;
    bounded(&snapshot)?;
    let fingerprint = repository
        .digest(&snapshot.fingerprint_input()?)
        .await?;
    if fingerprint != preview.fingerprint {
        return Err(preview_changed().into());
    }
    validate_snapshot(&snapshot, actor, catalog, &change, now)?;
    let plan = apply_plan(&snapshot, &change, token, &hash, now)?;
    if matches!(change, PeopleGrantChange::Revoke { .. })
        && plan
            .assignment_id
            .as_deref()
            .is_some_and(|id| snapshot.assignment(id).is_some_and(|row| row.user_id == actor.user_id))
    {
        return Err(crate::domain::people_grants::self_access_change().into());
    }
    let result = repository
        .commit_apply(actor, key.as_str(), &plan, &hash)
        .await?;
    repository.consume_preview(actor, token).await?;
    if result["id"].as_str() != Some(plan.id.as_str()) {
        return Err(readback_invalid().into());
    }
    Ok(result)
}

/// Resolve the concrete write plan for one reviewed change.
fn apply_plan(
    snapshot: &GrantSnapshot,
    change: &PeopleGrantChange,
    token: &str,
    hash: &str,
    now: i64,
) -> Result<GrantApplyPlan, BusinessError> {
    let mut plan = GrantApplyPlan {
        token: token.to_owned(),
        id: change
            .caps_id()
            .map(str::to_owned)
            .unwrap_or_else(|| grant_id(change, hash)),
        subject_id: None,
        subject: None,
        assignment_id: None,
        invitation_id: None,
        role_revision: None,
        valid_until: None,
        scope_kind: None,
        scope_node_id: None,
        expected_version: 0,
        change: change.clone(),
        now,
    };
    match change {
        PeopleGrantChange::Account { .. } => {}
        PeopleGrantChange::Role {
            id, expected_version, ..
        } => {
            plan.expected_version = *expected_version;
            plan.id = id.clone().unwrap_or(plan.id);
        }
        PeopleGrantChange::Grant {
            subject_id,
            role_id,
            scope_kind,
            scope_node_id,
            valid_until,
            ..
        }
        | PeopleGrantChange::Invite {
            subject_id,
            role_id,
            scope_kind,
            scope_node_id,
            valid_until,
            ..
        } => {
            plan.subject_id = Some(subject_id.clone());
            plan.subject = snapshot.subject(subject_id).cloned();
            plan.valid_until = Some(*valid_until);
            plan.scope_kind = Some(scope_kind.clone());
            plan.scope_node_id = scope_node_id.clone();
            plan.role_revision = snapshot.role(role_id).map(|row| row.revision);
            plan.id = subject_id.clone();
            if matches!(change, PeopleGrantChange::Invite { .. }) {
                plan.invitation_id = Some(invitation_id(change, hash));
            }
        }
        PeopleGrantChange::Revoke {
            assignment_id,
            expected_version,
            ..
        } => {
            plan.id = assignment_id.clone();
            plan.assignment_id = Some(assignment_id.clone());
            plan.expected_version = *expected_version;
        }
    }
    Ok(plan)
}
