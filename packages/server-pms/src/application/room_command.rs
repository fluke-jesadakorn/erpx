//! Native PMS room/housekeeping command orchestration.
//!
//! The repository owns only PMS rows under the caller's serializable unit.
//! Current Account/assignment authority and the People assignment scope for a
//! task owner are public ports supplied by composition; this use case never
//! mints an actor, Company, permission or staff label.
use crate::application::room_board::PmsBoardFuture;
use crate::domain::room_board::record_not_found;
use crate::domain::room_command::{
    PmsRoomCommand, PmsRoomCommandActor, PmsRoomCommandFacts, PmsRoomCommandInput,
    PmsRoomCommandPlan, PmsRoomCommandResult, invalid, plan_room_command,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use sha2::{Digest, Sha256};

pub trait PmsRoomCommandRepository: Send {
    type Error: From<BusinessError> + Send;
    /// Replay one committed receipt for this exact command key. A mismatched
    /// request hash is an idempotency conflict, never a second write.
    fn replay<'a>(
        &'a mut self,
        key: &'a CommandKey,
        request_hash: &'a str,
        command: &'a PmsRoomCommand,
    ) -> PmsBoardFuture<'a, Option<PmsRoomCommandResult>, Self::Error>;
    /// Lock the property row and load every owner fact this command needs.
    fn load<'a>(
        &'a mut self,
        actor: &'a PmsRoomCommandActor,
        command: &'a PmsRoomCommand,
    ) -> PmsBoardFuture<'a, PmsRoomCommandFacts, Self::Error>;
    /// Persist the planned rows, property version, receipt, audit and outbox
    /// together in the caller's serializable transaction.
    fn commit<'a>(
        &'a mut self,
        actor: &'a PmsRoomCommandActor,
        key: &'a CommandKey,
        request_hash: &'a str,
        plan: PmsRoomCommandPlan,
        now: i64,
    ) -> PmsBoardFuture<'a, PmsRoomCommandResult, Self::Error>;
}

/// People/assignment public boundary. The retained `task.assign` refuses an
/// owner that is not an active assignment for this hotel company/branch; this
/// use case never invents that fact from the client body.
pub trait PmsRoomAssigneeScopePort {
    type Error: Send;
    fn assignee_in_scope<'a>(
        &'a mut self,
        company_id: &'a str,
        branch_id: &'a str,
        assignee_id: &'a str,
        now: i64,
    ) -> PmsBoardFuture<'a, bool, Self::Error>;
}

/// Execute one retained room/housekeeping command as one idempotent owner
/// command.
pub async fn pms_room_command<R, A>(
    actor: &PmsRoomCommandActor,
    key: &CommandKey,
    command: &PmsRoomCommand,
    repository: &mut R,
    assignee_scope: &mut A,
    now: i64,
) -> Result<PmsRoomCommandResult, R::Error>
where
    R: PmsRoomCommandRepository,
    A: PmsRoomAssigneeScopePort,
    R::Error: From<A::Error>,
{
    actor.require_command(command.operation(), now)?;
    let mut normalized = command.clone();
    normalized.normalize();
    normalized.validate()?;
    let request_hash = room_command_request_hash(actor, &normalized)?;
    if let Some(result) = repository.replay(key, &request_hash, &normalized).await? {
        if result.property_id != normalized.property_id
            || result.version < 1
            || result.version != normalized.expected_version + 1
            || normalized
                .input
                .target_id()
                .is_some_and(|target| target != result.id)
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "The retained result is unavailable.",
            )
            .into());
        }
        return Ok(result);
    }
    let facts = repository.load(actor, &normalized).await?;
    if facts.property.id != normalized.property_id
        || facts.property.company_id != actor.board.company_id
    {
        return Err(crate::domain::room_board::not_found().into());
    }
    if let PmsRoomCommandInput::TaskAssign { assignee_id, .. } = &normalized.input
        && !assignee_scope
            .assignee_in_scope(
                &actor.board.company_id,
                &facts.property.branch_id,
                assignee_id,
                now,
            )
            .await
            .map_err(R::Error::from)?
    {
        return Err(BusinessError::invalid(
            "PMS_ASSIGNEE_SCOPE",
            "Owner is not authorized for this hotel",
        )
        .into());
    }
    if normalized.input.task_id().is_some() && facts.task.is_none() {
        return Err(record_not_found().into());
    }
    let plan = plan_room_command(actor, key, &facts, &normalized, now)?;
    repository
        .commit(actor, key, &request_hash, plan, now)
        .await
}

/// Stable request fingerprint used by the command receipt. It covers the same
/// actor, Company/branch scope, operation, target property and bounded input as
/// the retained command receipt.
pub fn room_command_request_hash(
    actor: &PmsRoomCommandActor,
    command: &PmsRoomCommand,
) -> Result<String, BusinessError> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Fingerprint<'a> {
        actor: &'a str,
        assignment: &'a str,
        role: &'a str,
        revision: i64,
        assignment_version: i64,
        company: &'a str,
        branch: Option<&'a str>,
        department: Option<&'a str>,
        operation: &'a str,
        record_id: &'a str,
        input: Input<'a>,
    }
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase", tag = "kind")]
    enum Input<'a> {
        #[serde(rename = "room.save")]
        Save {
            room_id: Option<&'a str>,
            number: &'a str,
            type_id: &'a str,
            building: &'a str,
            floor: &'a str,
            active: bool,
        },
        #[serde(rename = "room.dnd")]
        Dnd { room_id: &'a str, dnd: bool },
        #[serde(rename = "room.observe")]
        Observe { room_id: &'a str, occupied: bool },
        #[serde(rename = "task.assign")]
        TaskAssign {
            task_id: &'a str,
            assignee_id: &'a str,
            priority: &'a str,
        },
        #[serde(rename = "task.start")]
        TaskStart {
            task_id: &'a str,
            checked: &'a [String],
            notes: &'a str,
        },
        #[serde(rename = "task.complete")]
        TaskComplete {
            task_id: &'a str,
            checked: &'a [String],
            notes: &'a str,
        },
        #[serde(rename = "task.inspect")]
        TaskInspect { task_id: &'a str },
    }
    let input = match &command.input {
        PmsRoomCommandInput::Save(input) => Input::Save {
            room_id: input.room_id.as_deref(),
            number: &input.number,
            type_id: &input.type_id,
            building: &input.building,
            floor: &input.floor,
            active: input.active,
        },
        PmsRoomCommandInput::Dnd { room_id, dnd } => Input::Dnd { room_id, dnd: *dnd },
        PmsRoomCommandInput::Observe { room_id, occupied } => Input::Observe {
            room_id,
            occupied: *occupied,
        },
        PmsRoomCommandInput::TaskAssign {
            task_id,
            assignee_id,
            priority,
        } => Input::TaskAssign {
            task_id,
            assignee_id,
            priority: priority.as_str(),
        },
        PmsRoomCommandInput::TaskStart {
            task_id,
            checked,
            notes,
        } => Input::TaskStart {
            task_id,
            checked,
            notes,
        },
        PmsRoomCommandInput::TaskComplete {
            task_id,
            checked,
            notes,
        } => Input::TaskComplete {
            task_id,
            checked,
            notes,
        },
        PmsRoomCommandInput::TaskInspect { task_id } => Input::TaskInspect { task_id },
    };
    let record_id = command.input.target_id().unwrap_or(&command.property_id);
    let value = Fingerprint {
        actor: &actor.board.user_id,
        assignment: &actor.assignment_id,
        role: &actor.role,
        revision: actor.revision,
        assignment_version: actor.assignment_version,
        company: &actor.board.company_id,
        branch: actor.board.branch_id.as_deref(),
        department: actor.board.department_id.as_deref(),
        operation: command.operation().as_str(),
        record_id,
        input,
    };
    let payload = serde_json::to_vec(&value).map_err(|_| invalid())?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}
