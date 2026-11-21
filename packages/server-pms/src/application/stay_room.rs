//! Native PMS room-assignment/room-move command orchestration.
//!
//! The repository owns only PMS rows under the caller's serializable unit.
//! Commercial package entitlement and current Account/assignment authority are
//! public ports supplied by composition; this use case never mints an actor,
//! Company, permission or staff label.
use crate::application::room_board::PmsBoardFuture;
use crate::domain::room_board::{PmsRecord, record_not_found};
use crate::domain::stay_room::{
    PmsStayRoomActor, PmsStayRoomCommand, PmsStayRoomEntitlement, PmsStayRoomFacts,
    PmsStayRoomPlan, PmsStayRoomResult, invalid, plan_stay_room, record_invalid,
};
use erpx_server_shared::public::{BusinessError, CommandKey};
use sha2::{Digest, Sha256};

pub trait PmsStayRoomRepository: Send {
    type Error: From<BusinessError> + Send;
    /// Replay one committed receipt for this exact command key. A mismatched
    /// request hash is an idempotency conflict, never a second write.
    fn replay<'a>(
        &'a mut self,
        key: &'a CommandKey,
        request_hash: &'a str,
        command: &'a PmsStayRoomCommand,
    ) -> PmsBoardFuture<'a, Option<PmsStayRoomResult>, Self::Error>;
    /// Lock the property row and load every owner fact this command needs.
    fn load<'a>(
        &'a mut self,
        actor: &'a PmsStayRoomActor,
        command: &'a PmsStayRoomCommand,
    ) -> PmsBoardFuture<'a, PmsStayRoomFacts, Self::Error>;
    /// Persist the planned rows, property version, receipt, audit and outbox
    /// together in the caller's serializable transaction.
    fn commit<'a>(
        &'a mut self,
        actor: &'a PmsStayRoomActor,
        key: &'a CommandKey,
        request_hash: &'a str,
        plan: PmsStayRoomPlan,
        now: i64,
    ) -> PmsBoardFuture<'a, PmsStayRoomResult, Self::Error>;
}

/// Commercial public `pms.front-desk` settlement boundary.
pub trait PmsStayRoomEntitlementPort {
    type Error: Send;
    fn entitlement<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> PmsBoardFuture<'a, PmsStayRoomEntitlement, Self::Error>;
}

/// Execute `stay.assign` or `stay.move` as one idempotent owner command.
pub async fn pms_stay_room<R, E>(
    actor: &PmsStayRoomActor,
    key: &CommandKey,
    command: &PmsStayRoomCommand,
    repository: &mut R,
    entitlement: &mut E,
    now: i64,
) -> Result<PmsStayRoomResult, R::Error>
where
    R: PmsStayRoomRepository,
    E: PmsStayRoomEntitlementPort,
    R::Error: From<E::Error>,
{
    actor.require_command(now)?;
    let mut normalized = command.clone();
    normalized.normalize();
    normalized.validate()?;
    entitlement
        .entitlement(&actor.board.company_id)
        .await
        .map_err(R::Error::from)?
        .require_settle_existing()?;
    let request_hash = stay_room_request_hash(actor, &normalized)?;
    if let Some(result) = repository.replay(key, &request_hash, &normalized).await? {
        if result.id != normalized.stay_id
            || result.property_id != normalized.property_id
            || result.version < 1
            || result.version != normalized.expected_version + 1
        {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "The retained result is unavailable.",
            )
            .into());
        }
        return Ok(result);
    }
    let facts = repository.load(actor, &normalized).await?;
    if facts.stay.id != normalized.stay_id {
        return Err(record_not_found().into());
    }
    let plan = plan_stay_room(actor, &facts, &normalized, now)?;
    repository
        .commit(actor, key, &request_hash, plan, now)
        .await
}

/// Stable request fingerprint used by the command receipt. It covers the same
/// actor, Company/branch scope, operation, target property and bounded input as
/// the retained command receipt.
pub fn stay_room_request_hash(
    actor: &PmsStayRoomActor,
    command: &PmsStayRoomCommand,
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
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        property_id: &'a str,
        room_id: &'a str,
        reason: &'a str,
        expected_version: i64,
        expected_stay_version: Option<i64>,
    }
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
        record_id: &command.property_id,
        input: Input {
            property_id: &command.property_id,
            room_id: &command.room_id,
            reason: &command.reason,
            expected_version: command.expected_version,
            expected_stay_version: command.expected_stay_version,
        },
    };
    let payload = serde_json::to_vec(&value).map_err(|_| invalid())?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}

/// Validate the stored stay shape before the adapter writes. Retained engine
/// fields (`prices`, guest accounts, folio entries) stay untouched by this
/// slice; the planner verifies the stay record is a JSON object up front.
pub(crate) fn assert_stay_record(record: &PmsRecord) -> Result<(), BusinessError> {
    if record.kind != "stay" || !record.data.is_object() {
        return Err(record_invalid());
    }
    Ok(())
}
