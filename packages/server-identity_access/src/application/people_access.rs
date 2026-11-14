//! People relationship preview/apply use cases.
//!
//! Ports supply current authority, the serializable owner transaction, replay,
//! the reviewed snapshot and persisted readback. This layer never opens a
//! transaction or reads an HTTP request.
use crate::domain::people_access::{
    PeopleAccessActor, RELATIONSHIP_APPLY_OPERATION, RELATIONSHIP_MANAGE_PERMISSION,
    RelationshipApplyPlan, RelationshipChange, RelationshipPreviewPlan, RelationshipPreviewRow,
    RelationshipReceipt, RelationshipSnapshot, access_object_unavailable, access_object_changed,
    idempotency_conflict, preview_changed, preview_expired, preview_summary, readback_invalid,
    relationship_apply_identity, relationship_id, protected_account,
    self_access_change, source_invalid, sponsor_unavailable, validate_change, validate_snapshot,
    valid_token, MAX_ORGANIZATIONS, MAX_SUBJECTS, MAX_TEAMS, MAX_UNITS, PREVIEW_LIFETIME_MS,
};
use erpx_server_shared::public::{BusinessError, CommandKey};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type PeopleAccessFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait PeopleRelationshipRepository {
    type Error: From<BusinessError>;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a PeopleAccessActor,
        permission: &'a str,
        now: i64,
    ) -> PeopleAccessFuture<'a, bool, Self::Error>;
    fn digest<'a>(&'a mut self, value: &'a str) -> PeopleAccessFuture<'a, String, Self::Error>;
    fn snapshot<'a>(
        &'a mut self,
        actor: &'a PeopleAccessActor,
    ) -> PeopleAccessFuture<'a, RelationshipSnapshot, Self::Error>;
    fn active_employee<'a>(
        &'a mut self,
        actor: &'a PeopleAccessActor,
        person_id: &'a str,
    ) -> PeopleAccessFuture<'a, bool, Self::Error>;
    fn same_person_subject<'a>(
        &'a mut self,
        actor: &'a PeopleAccessActor,
        user_id: Option<&'a str>,
        person_id: Option<&'a str>,
    ) -> PeopleAccessFuture<'a, bool, Self::Error>;
    fn protected_account<'a>(
        &'a mut self,
        actor: &'a PeopleAccessActor,
        user_id: &'a str,
    ) -> PeopleAccessFuture<'a, bool, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> PeopleAccessFuture<'a, Option<RelationshipReceipt>, Self::Error>;
    fn commit_preview<'a>(
        &'a mut self,
        actor: &'a PeopleAccessActor,
        plan: &'a RelationshipPreviewPlan,
    ) -> PeopleAccessFuture<'a, String, Self::Error>;
    fn preview_for_update<'a>(
        &'a mut self,
        actor: &'a PeopleAccessActor,
        token: &'a str,
        now: i64,
    ) -> PeopleAccessFuture<'a, Option<RelationshipPreviewRow>, Self::Error>;
    fn consume_preview<'a>(
        &'a mut self,
        actor: &'a PeopleAccessActor,
        token: &'a str,
    ) -> PeopleAccessFuture<'a, (), Self::Error>;
    fn commit_apply<'a>(
        &'a mut self,
        actor: &'a PeopleAccessActor,
        key: &'a str,
        plan: &'a RelationshipApplyPlan,
        request_hash: &'a str,
    ) -> PeopleAccessFuture<'a, Value, Self::Error>;
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Bound the reviewed source so a preview cannot fingerprint an unbounded scan.
fn bounded(snapshot: &RelationshipSnapshot) -> Result<(), BusinessError> {
    if snapshot.organizations.len() > MAX_ORGANIZATIONS
        || snapshot.teams.len() > MAX_TEAMS
        || snapshot.subjects.len() > MAX_SUBJECTS
        || snapshot.units.len() > MAX_UNITS
    {
        return Err(source_invalid());
    }
    Ok(())
}

async fn require_current<R: PeopleRelationshipRepository>(
    repository: &mut R,
    actor: &PeopleAccessActor,
    permission: &str,
    now: i64,
) -> Result<(), R::Error> {
    if !repository.current_authority(actor, permission, now).await? {
        return Err(access_object_changed().into());
    }
    Ok(())
}

/// Relationship-specific guards: an external sponsor must be an active
/// employee, and an administrator may not edit their own or a protected
/// recovery relationship.
async fn guard_subject<R: PeopleRelationshipRepository>(
    repository: &mut R,
    actor: &PeopleAccessActor,
    change: &RelationshipChange,
    snapshot: &RelationshipSnapshot,
) -> Result<(), R::Error> {
    let RelationshipChange::Subject {
        id,
        sponsor_person_id,
        ..
    } = change
    else {
        return Ok(());
    };
    if let Some(person_id) = sponsor_person_id
        && !repository.active_employee(actor, person_id).await?
    {
        return Err(sponsor_unavailable().into());
    }
    let Some(existing) = id
        .as_deref()
        .map(|id| {
            snapshot
                .subjects
                .iter()
                .find(|row| row.id == id)
                .ok_or_else(access_object_unavailable)
        })
        .transpose()?
    else {
        return Ok(());
    };
    let user_id = existing.user_id.as_deref();
    if repository
        .same_person_subject(actor, user_id, existing.person_id.as_deref())
        .await?
    {
        return Err(self_access_change().into());
    }
    if let Some(user_id) = user_id
        && repository.protected_account(actor, user_id).await?
    {
        return Err(protected_account().into());
    }
    Ok(())
}

/// Validate, fingerprint and persist a reviewed relationship change. The token
/// is generated by the owner adapter and bound to this actor, assignment,
/// session and snapshot.
pub async fn preview_relationship<R: PeopleRelationshipRepository>(
    repository: &mut R,
    actor: &PeopleAccessActor,
    input: Value,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require(RELATIONSHIP_MANAGE_PERMISSION, now)?;
    let change = validate_change(
        serde_json::from_value::<RelationshipChange>(input)
            .map_err(|_| crate::domain::people_access::request_invalid())?,
    )?;
    require_current(repository, actor, RELATIONSHIP_MANAGE_PERMISSION, now).await?;
    let snapshot = repository.snapshot(actor).await?;
    bounded(&snapshot)?;
    validate_snapshot(&snapshot, &change)?;
    guard_subject(repository, actor, &change, &snapshot).await?;
    let change_json = serde_json::to_string(&change).map_err(|_| readback_invalid())?;
    let fingerprint = repository.digest(&snapshot.fingerprint_input()?).await?;
    if !valid_hash(&fingerprint) {
        return Err(readback_invalid().into());
    }
    let expires_at = now + PREVIEW_LIFETIME_MS;
    let plan = RelationshipPreviewPlan {
        company_id: actor.company_id.as_str().to_owned(),
        actor_id: actor.user_id.as_str().to_owned(),
        assignment_id: actor.assignment_id.as_str().to_owned(),
        session_id: actor.session_id.as_str().to_owned(),
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
        "summary": preview_summary(&change),
        "affected": [],
        "expiresAt": expires_at,
    }))
}

/// A consumed preview cannot be re-read, so the recorded receipt is the only
/// replay source. The same Idempotency-Key with the same request identity
/// returns the recorded result; any other reuse is a conflict.
fn receipt_value(
    actor: &PeopleAccessActor,
    request_hash: &str,
    receipt: Option<RelationshipReceipt>,
) -> Result<Option<Value>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_hash != request_hash
        || receipt.operation != RELATIONSHIP_APPLY_OPERATION
        || receipt.actor_id != actor.user_id.as_str()
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

/// Apply exactly one reviewed relationship change. The preview token is
/// single-use: the row is consumed inside the same serializable unit that
/// writes the relationship, receipt, audit and outbox records.
pub async fn apply_relationship<R: PeopleRelationshipRepository>(
    repository: &mut R,
    actor: &PeopleAccessActor,
    key: &CommandKey,
    token: &str,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require(RELATIONSHIP_MANAGE_PERMISSION, now)?;
    if !valid_token(token) {
        return Err(preview_expired().into());
    }
    require_current(repository, actor, RELATIONSHIP_MANAGE_PERMISSION, now).await?;
    let identity = relationship_apply_identity(actor, key.as_str(), token);
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
        serde_json::from_str::<RelationshipChange>(&preview.change_json)
            .map_err(|_| readback_invalid())?,
    )?;
    let id = change
        .id()
        .map(str::to_owned)
        .unwrap_or_else(|| relationship_id(&change, &hash));
    let snapshot = repository.snapshot(actor).await?;
    bounded(&snapshot)?;
    let fingerprint = repository.digest(&snapshot.fingerprint_input()?).await?;
    if fingerprint != preview.fingerprint {
        return Err(preview_changed().into());
    }
    validate_snapshot(&snapshot, &change)?;
    guard_subject(repository, actor, &change, &snapshot).await?;
    let retained = change
        .id()
        .and_then(|id| snapshot.subjects.iter().find(|row| row.id == id));
    let expected_version = change.expected_version();
    let plan = RelationshipApplyPlan {
        token: token.to_owned(),
        id: id.clone(),
        change: change.clone(),
        expected_version,
        resulting_version: expected_version + 1,
        retained_user_id: retained.and_then(|row| row.user_id.clone()),
        retained_person_id: retained.and_then(|row| row.person_id.clone()),
        now,
    };
    let result = repository.commit_apply(actor, key.as_str(), &plan, &hash).await?;
    repository.consume_preview(actor, token).await?;
    if result["id"].as_str() != Some(id.as_str())
        || result["version"].as_i64() != Some(expected_version + 1)
    {
        return Err(readback_invalid().into());
    }
    Ok(result)
}
