//! Native position-move commands.
//!
//! The repository supplies current sources, current IAM authority and owns the
//! serializable transaction; this layer keeps the retained command order:
//! preview token, expiry, effective-date, scope and graph blockers, optimistic
//! version, idempotent replay and the cancellation lock.
use crate::domain::organization_nodes::{permission_required, OrganizationActor};
use crate::domain::position_moves::{
    hash_value, leave_default_recommendation, position_move_not_found, prepare_move,
    ExistingLeaveFact, LeaveAssignmentPlan, LeavePolicyFact, MoveMember, MovePosition,
    PositionMoveFacts, PositionMoveInput, PositionMovePersonWire, PositionMovePreview,
    PositionMoveRecord, PreparedPositionMove, PREVIEW_TTL_MS,
};
use erpx_server_shared::public::{BusinessError, Clock, TimestampMillis};
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;

pub type PositionMoveFuture<'a, T, E = BusinessError> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait PositionMoveRepository: Send {
    type Error: From<BusinessError> + Send;

    /// Re-check the current assignment and role permission in the transaction.
    fn authorize<'a>(
        &'a mut self,
        actor: &'a OrganizationActor,
        permission: &'a str,
    ) -> PositionMoveFuture<'a, (), Self::Error>;

    /// Resolve the civil date in the requested timezone (PostgreSQL tz data).
    fn local_date<'a>(
        &'a mut self,
        timestamp: i64,
        timezone: &'a str,
    ) -> PositionMoveFuture<'a, String, Self::Error>;

    fn facts<'a>(
        &'a mut self,
        actor: &'a OrganizationActor,
        position_id: &'a str,
        date: &'a str,
        exclude_move_id: Option<&'a str>,
    ) -> PositionMoveFuture<'a, PositionMoveFacts, Self::Error>;

    fn replay<'a>(
        &'a mut self,
        actor: &'a crate::application::organization_nodes::OrganizationCommandActor,
        key: &'a str,
        operation: &'a str,
        record_id: &'a str,
        input: &'a Value,
    ) -> PositionMoveFuture<'a, Option<Value>, Self::Error>;

    fn schedule<'a>(
        &'a mut self,
        actor: &'a crate::application::organization_nodes::OrganizationCommandActor,
        input: &'a PositionMoveInput,
        key: &'a str,
        payload: &'a Value,
        prepared: &'a PreparedPositionMove,
        now: i64,
    ) -> PositionMoveFuture<'a, Value, Self::Error>;

    fn rows<'a>(
        &'a mut self,
        actor: &'a OrganizationActor,
        id: Option<&'a str>,
    ) -> PositionMoveFuture<'a, Vec<PositionMoveRecord>, Self::Error>;

    fn cancel<'a>(
        &'a mut self,
        actor: &'a crate::application::organization_nodes::OrganizationCommandActor,
        id: &'a str,
        expected_version: i64,
        key: &'a str,
        now: i64,
    ) -> PositionMoveFuture<'a, Value, Self::Error>;
}

pub fn require_permission(
    actor: &OrganizationActor,
    permission: &str,
) -> Result<(), BusinessError> {
    if actor.has_permission(permission) {
        Ok(())
    } else {
        Err(permission_required())
    }
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        code,
        message,
    )
}

async fn visible_rows<R: PositionMoveRepository>(
    repository: &mut R,
    actor: &OrganizationActor,
    id: Option<&str>,
) -> Result<Vec<PositionMoveRecord>, R::Error> {
    require_permission(actor, "organization.manage")?;
    repository.authorize(actor, "organization.manage").await?;
    let can_read_people = actor.has_permission("people.manage");
    let rows = repository.rows(actor, id).await?;
    let rows = rows
        .into_iter()
        .filter(|row| row.preview.affected_count == 0 || can_read_people)
        .collect::<Vec<_>>();
    if id.is_some() && rows.is_empty() {
        return Err(position_move_not_found().into());
    }
    Ok(rows)
}

pub async fn preview_position_move<R: PositionMoveRepository>(
    repository: &mut R,
    actor: &OrganizationActor,
    input: &PositionMoveInput,
    clock: &(dyn Clock + Sync),
) -> Result<PositionMovePreview, R::Error> {
    require_permission(actor, "organization.manage")?;
    repository.authorize(actor, "organization.manage").await?;
    let now = clock.now().get();
    let date = repository
        .local_date(input.effective_at, &input.timezone)
        .await?;
    let facts = repository
        .facts(actor, &input.position_id, &date, None)
        .await?;
    if !facts.members.is_empty() {
        require_permission(actor, "people.manage")?;
        repository.authorize(actor, "people.manage").await?;
    }
    let prepared = prepare_move(
        actor.branch_id.as_ref().map(|value| value.as_str()),
        actor.department_id.as_ref().map(|value| value.as_str()),
        actor.has_permission("people.manage"),
        input,
        &facts,
        now + PREVIEW_TTL_MS,
    )?;
    Ok(prepared.preview)
}

pub async fn submit_position_move<R: PositionMoveRepository>(
    repository: &mut R,
    actor: &crate::application::organization_nodes::OrganizationCommandActor,
    input: &PositionMoveInput,
    preview_token: &str,
    expires_at: i64,
    key: &str,
    clock: &(dyn Clock + Sync),
) -> Result<Value, R::Error> {
    let projection = actor.projection();
    require_permission(&projection, "organization.manage")?;
    repository
        .authorize(&projection, "organization.manage")
        .await?;
    let id = format!("position-move-{key}");
    let payload = json!({
        "input": input,
        "previewToken": preview_token,
        "expiresAt": expires_at,
    });
    if let Some(prior) = repository
        .replay(
            actor,
            key,
            "organization.position-move.submit",
            &id,
            &payload,
        )
        .await?
    {
        return Ok(prior);
    }
    let now = clock.now().get();
    if expires_at <= now || expires_at > now + PREVIEW_TTL_MS {
        return Err(conflict(
            "POSITION_MOVE_PREVIEW_EXPIRED",
            "Preview the move again before confirming.",
        )
        .into());
    }
    if input.effective_at < now - 60_000 {
        return Err(BusinessError::invalid(
            "EFFECTIVE_DATE_PASSED",
            "Choose an effective time now or in the future.",
        )
        .into());
    }
    let date = repository
        .local_date(input.effective_at, &input.timezone)
        .await?;
    let facts = repository
        .facts(&projection, &input.position_id, &date, None)
        .await?;
    let prepared = prepare_move(
        projection.branch_id.as_ref().map(|value| value.as_str()),
        projection
            .department_id
            .as_ref()
            .map(|value| value.as_str()),
        projection.has_permission("people.manage"),
        input,
        &facts,
        expires_at,
    )?;
    if prepared.preview.preview_token != preview_token {
        return Err(conflict(
            "POSITION_MOVE_CHANGED",
            "The reviewed organization changed. Preview the entire move again.",
        )
        .into());
    }
    if !prepared.preview.ready {
        return Err(BusinessError::invalid(
            "POSITION_MOVE_NOT_READY",
            "The position move is not ready.",
        )
        .into());
    }
    repository
        .schedule(actor, input, key, &payload, &prepared, now)
        .await
}

pub async fn position_moves<R: PositionMoveRepository>(
    repository: &mut R,
    actor: &OrganizationActor,
    id: Option<&str>,
) -> Result<Vec<PositionMoveRecord>, R::Error> {
    visible_rows(repository, actor, id).await
}

pub async fn cancel_position_move<R: PositionMoveRepository>(
    repository: &mut R,
    actor: &crate::application::organization_nodes::OrganizationCommandActor,
    id: &str,
    expected_version: i64,
    key: &str,
    clock: &(dyn Clock + Sync),
) -> Result<Value, R::Error> {
    let projection = actor.projection();
    require_permission(&projection, "organization.manage")?;
    repository
        .authorize(&projection, "organization.manage")
        .await?;
    let current = visible_rows(repository, &projection, Some(id)).await?;
    let current = current
        .first()
        .cloned()
        .ok_or_else(position_move_not_found)?;
    let now = clock.now().get();
    let payload = json!({"expectedVersion": expected_version});
    if let Some(prior) = repository
        .replay(
            actor,
            key,
            "organization.position-move.cancel",
            id,
            &payload,
        )
        .await?
    {
        return Ok(prior);
    }
    if current.status != "scheduled" || current.effective_at <= now {
        return Err(conflict(
            "POSITION_MOVE_LOCKED",
            "Only a scheduled move before its effective time can be cancelled.",
        )
        .into());
    }
    repository
        .cancel(actor, id, expected_version, key, now)
        .await
}

/// Deterministic preview-token digest used by tests and adapters.
pub fn preview_digest(input: &PositionMoveInput, snapshot_hash: &str, expires_at: i64) -> String {
    hash_value(&json!({
        "input": input,
        "snapshotHash": snapshot_hash,
        "expiresAt": expires_at,
    }))
}

/// Fixed clock for owner tests and deterministic adapters.
#[derive(Clone, Copy, Debug)]
pub struct FixedClock(pub TimestampMillis);

impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        self.0
    }
}

/// A scheduled move claimed under the owner transaction, with the saved
/// authorizing actor and assignment. The actor is reconstructed by the adapter
/// from the stored `actor_json`; it grants no authority until `assignment_valid`
/// rechecks the current IAM rows.
#[derive(Clone, Debug)]
pub struct ClaimedPositionMove {
    pub record: PositionMoveRecord,
    pub actor: OrganizationActor,
    pub authorizing_assignment_id: String,
    pub created_by: String,
    pub snapshot_hash: String,
}

/// Why one claimed move was not applied. The caller rolls back the owner unit
/// and records `needs_review` in a fresh transaction. `move_id` is present once
/// a scheduled row was claimed, even if a later validation failed.
#[derive(Debug)]
pub enum DrainFailure<E> {
    Business {
        move_id: Option<String>,
        error: BusinessError,
    },
    Repository {
        move_id: Option<String>,
        error: E,
    },
}

impl<E> DrainFailure<E> {
    pub fn move_id(&self) -> Option<&str> {
        match self {
            Self::Business { move_id, .. } | Self::Repository { move_id, .. } => move_id.as_deref(),
        }
    }
}

/// One claimed move either applied or nothing was claimable.
#[derive(Debug)]
pub enum DrainOutcome {
    Empty,
    Applied { move_id: String },
}

/// Apply-only repository. Every method runs in the transaction that claimed
/// the move; the adapter never opens an independent unit or releases the
/// advisory claim between validation and effects.
pub trait PositionMoveDrainRepository: PositionMoveRepository {
    fn claim<'a>(
        &'a mut self,
        now: i64,
    ) -> PositionMoveFuture<'a, Option<ClaimedPositionMove>, Self::Error>;

    fn assignment_valid<'a>(
        &'a mut self,
        claimed: &'a ClaimedPositionMove,
        affected_count: usize,
        now: i64,
    ) -> PositionMoveFuture<'a, bool, Self::Error>;

    fn set_payroll_date<'a>(&'a mut self, date: &'a str)
        -> PositionMoveFuture<'a, (), Self::Error>;

    fn apply_position<'a>(
        &'a mut self,
        company_id: &'a str,
        candidate: &'a MovePosition,
        now: i64,
    ) -> PositionMoveFuture<'a, (), Self::Error>;

    fn apply_holder<'a>(
        &'a mut self,
        claimed: &'a ClaimedPositionMove,
        input: &'a PositionMoveInput,
        holder: &'a PositionMovePersonWire,
        original: &'a MoveMember,
        now: i64,
    ) -> PositionMoveFuture<'a, (), Self::Error>;

    fn leave_policy<'a>(
        &'a mut self,
        company_id: &'a str,
        level_id: &'a str,
    ) -> PositionMoveFuture<'a, Option<LeavePolicyFact>, Self::Error>;

    fn existing_leave<'a>(
        &'a mut self,
        company_id: &'a str,
        engagement_id: &'a str,
        date: &'a str,
    ) -> PositionMoveFuture<'a, Option<ExistingLeaveFact>, Self::Error>;

    fn stage_leave<'a>(
        &'a mut self,
        actor: &'a OrganizationActor,
        command_id: &'a str,
        engagement_id: &'a str,
        plan: &'a LeaveAssignmentPlan,
        now: i64,
    ) -> PositionMoveFuture<'a, (), Self::Error>;

    fn complete<'a>(
        &'a mut self,
        claimed: &'a ClaimedPositionMove,
        input: &'a PositionMoveInput,
        affected_count: usize,
        now: i64,
    ) -> PositionMoveFuture<'a, (), Self::Error>;

    fn needs_review<'a>(
        &'a mut self,
        id: &'a str,
        code: &'a str,
        now: i64,
    ) -> PositionMoveFuture<'a, (), Self::Error>;
}

/// Apply exactly one claimed move with the reviewed preview and snapshot.
/// A changed authority, snapshot or readiness fails closed with
/// `POSITION_MOVE_AUTHORITY_CHANGED` / `POSITION_MOVE_REVIEW_CHANGED` before any
/// effect is retained. `DrainOutcome::Empty` means no claimable move was found.
pub async fn drain_once<R: PositionMoveDrainRepository>(
    repository: &mut R,
    now: i64,
) -> Result<DrainOutcome, DrainFailure<R::Error>> {
    let claimed = repository
        .claim(now)
        .await
        .map_err(|error| DrainFailure::Repository {
            move_id: None,
            error,
        })?;
    let Some(claimed) = claimed else {
        return Ok(DrainOutcome::Empty);
    };
    let move_id = Some(claimed.record.id.clone());
    let business = |error: BusinessError| DrainFailure::Business {
        move_id: move_id.clone(),
        error,
    };
    let repository_error = |error: R::Error| DrainFailure::Repository {
        move_id: move_id.clone(),
        error,
    };
    let record = claimed.record.clone();
    let input = record.definition.clone();
    require_permission(&claimed.actor, "organization.manage").map_err(business)?;
    if record.preview.affected_count > 0 {
        require_permission(&claimed.actor, "people.manage").map_err(business)?;
    }
    if !repository
        .assignment_valid(&claimed, record.preview.affected_count, now)
        .await
        .map_err(repository_error)?
    {
        return Err(business(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "POSITION_MOVE_AUTHORITY_CHANGED",
            "The authorizing HR role changed. Review the move again.",
        )));
    }
    let date = repository
        .local_date(record.effective_at, &input.timezone)
        .await
        .map_err(repository_error)?;
    let facts = repository
        .facts(&claimed.actor, &input.position_id, &date, Some(&record.id))
        .await
        .map_err(repository_error)?;
    let prepared = prepare_move(
        claimed.actor.branch_id.as_ref().map(|value| value.as_str()),
        claimed
            .actor
            .department_id
            .as_ref()
            .map(|value| value.as_str()),
        claimed.actor.has_permission("people.manage"),
        &input,
        &facts,
        record.preview.expires_at,
    )
    .map_err(business)?;
    if !prepared.preview.ready || prepared.snapshot_hash != claimed.snapshot_hash {
        return Err(business(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "POSITION_MOVE_REVIEW_CHANGED",
            "Organization, holders or leave defaults changed. Preview the entire move again.",
        )));
    }
    repository
        .set_payroll_date(&date)
        .await
        .map_err(repository_error)?;
    repository
        .apply_position(&record.company_id, &prepared.candidate, now)
        .await
        .map_err(repository_error)?;
    for holder in &prepared.preview.people {
        let original = prepared
            .members
            .iter()
            .find(|member| member.id == holder.engagement_id)
            .ok_or_else(|| {
                business(BusinessError::new(
                    erpx_server_shared::public::FailureKind::Unavailable,
                    "POSITION_MOVE_RETRY_REQUIRED",
                    "The reviewed holder engagement is unavailable.",
                ))
            })?;
        repository
            .apply_holder(&claimed, &input, holder, original, now)
            .await
            .map_err(repository_error)?;
        if holder.before.level_id != holder.after.level_id {
            let level_id = holder.after.level_id.as_deref().unwrap_or_default();
            let start_date = repository
                .local_date(record.effective_at, &input.timezone)
                .await
                .map_err(repository_error)?;
            let policy = repository
                .leave_policy(&record.company_id, level_id)
                .await
                .map_err(repository_error)?;
            let existing = repository
                .existing_leave(&record.company_id, &holder.engagement_id, &start_date)
                .await
                .map_err(repository_error)?;
            let plan = leave_default_recommendation(
                holder.before.level_id.as_deref(),
                level_id,
                &start_date,
                policy.as_ref(),
                existing.as_ref(),
            )
            .map_err(business)?;
            if let Some(plan) = plan {
                let command_id = format!("{}-{}", record.id, holder.engagement_id);
                repository
                    .stage_leave(
                        &claimed.actor,
                        &command_id,
                        &holder.engagement_id,
                        &plan,
                        now,
                    )
                    .await
                    .map_err(repository_error)?;
            }
        }
    }
    repository
        .complete(&claimed, &input, prepared.preview.affected_count, now)
        .await
        .map_err(repository_error)?;
    Ok(DrainOutcome::Applied { move_id: record.id })
}
