//! Native PMS staff reservation command orchestration.
//!
//! The repository owns only PMS rows under the caller's serializable unit.
//! Current Account/assignment authority is a public port supplied by
//! composition; this use case never mints an actor, Company, permission or
//! staff label. Public booking, payment providers and Finance posting are not
//! part of this slice.
use crate::application::room_board::PmsBoardFuture;
use crate::domain::reservation::{
    PmsReservationActor, PmsReservationCommand, PmsReservationCommandInput, PmsReservationFacts,
    PmsReservationPlan, PmsReservationResult, invalid, plan_reservation_command,
};
use crate::domain::room_board::record_not_found;
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use sha2::{Digest, Sha256};

pub trait PmsReservationRepository: Send {
    type Error: From<BusinessError> + Send;
    /// Replay one committed receipt for this exact command key. A mismatched
    /// request hash is an idempotency conflict, never a second write.
    fn replay<'a>(
        &'a mut self,
        key: &'a CommandKey,
        request_hash: &'a str,
        command: &'a PmsReservationCommand,
    ) -> PmsBoardFuture<'a, Option<PmsReservationResult>, Self::Error>;
    /// Lock the property row and load every owner fact this command needs.
    fn load<'a>(
        &'a mut self,
        actor: &'a PmsReservationActor,
        command: &'a PmsReservationCommand,
    ) -> PmsBoardFuture<'a, PmsReservationFacts, Self::Error>;
    /// Persist the planned rows, property version, receipt, audit and outbox
    /// together in the caller's serializable transaction.
    fn commit<'a>(
        &'a mut self,
        actor: &'a PmsReservationActor,
        key: &'a CommandKey,
        request_hash: &'a str,
        plan: PmsReservationPlan,
        now: i64,
    ) -> PmsBoardFuture<'a, PmsReservationResult, Self::Error>;
}

/// Execute one retained staff reservation command as one idempotent owner
/// command.
pub async fn pms_reservation<R>(
    actor: &PmsReservationActor,
    key: &CommandKey,
    command: &PmsReservationCommand,
    repository: &mut R,
    now: i64,
) -> Result<PmsReservationResult, R::Error>
where
    R: PmsReservationRepository,
{
    actor.require_command(command.operation(), now)?;
    let mut normalized = command.clone();
    normalized.normalize();
    normalized.validate()?;
    let request_hash = reservation_request_hash(actor, &normalized)?;
    if let Some(result) = repository.replay(key, &request_hash, &normalized).await? {
        if result.property_id != normalized.property_id
            || result.version < 1
            || result.version != normalized.expected_version + 1
            || (normalized.input.result_id_is_target()
                && normalized
                    .input
                    .reservation_id()
                    .is_some_and(|target| target != result.id))
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
    if normalized.input.reservation_id().is_some()
        && facts.reservation.as_ref().is_none_or(|reservation| {
            Some(reservation.id.as_str()) != normalized.input.reservation_id()
        })
    {
        return Err(record_not_found().into());
    }
    let plan = plan_reservation_command(actor, key, &facts, &normalized, now)?;
    repository
        .commit(actor, key, &request_hash, plan, now)
        .await
}

/// Stable request fingerprint used by the command receipt. It covers the same
/// actor, Company/branch scope, operation, target property/reservation and
/// bounded input as the retained command receipt.
pub fn reservation_request_hash(
    actor: &PmsReservationActor,
    command: &PmsReservationCommand,
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
        #[serde(rename = "reservation.create")]
        Create {
            property_id: &'a str,
            guest_id: Option<&'a str>,
            guest: Option<Guest<'a>>,
            source: &'a str,
            campaign_code: &'a str,
            external_ref: &'a str,
            group_name: &'a str,
            notes: &'a str,
            block_id: &'a str,
            rooms: Vec<Line<'a>>,
        },
        #[serde(rename = "reservation.change")]
        Change {
            reservation_id: &'a str,
            expected_reservation_version: Option<i64>,
            reason: &'a str,
            notes: Option<&'a str>,
            group_name: Option<&'a str>,
            campaign_code: Option<&'a str>,
            rooms: Vec<Line<'a>>,
        },
        #[serde(rename = "reservation.cancel")]
        Cancel {
            reservation_id: &'a str,
            expected_reservation_version: Option<i64>,
            reason: &'a str,
            fee: i64,
        },
    }
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Guest<'a> {
        name: &'a str,
        email: &'a str,
        phone: &'a str,
    }
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Line<'a> {
        type_id: &'a str,
        rate_id: &'a str,
        arrival: &'a str,
        departure: &'a str,
        adults: i64,
        child_ages: &'a [i64],
        quantity: i64,
    }
    fn lines<'a>(rooms: &'a [crate::domain::reservation::PmsReservationLine]) -> Vec<Line<'a>> {
        rooms
            .iter()
            .map(|line| Line {
                type_id: &line.type_id,
                rate_id: &line.rate_id,
                arrival: &line.arrival,
                departure: &line.departure,
                adults: line.adults,
                child_ages: &line.child_ages,
                quantity: line.quantity,
            })
            .collect()
    }
    let input = match &command.input {
        PmsReservationCommandInput::Create(input) => Input::Create {
            property_id: &command.property_id,
            guest_id: input.guest_id.as_deref(),
            guest: input.guest.as_ref().map(|guest| Guest {
                name: &guest.name,
                email: &guest.email,
                phone: &guest.phone,
            }),
            source: &input.source,
            campaign_code: &input.campaign_code,
            external_ref: &input.external_ref,
            group_name: &input.group_name,
            notes: &input.notes,
            block_id: &input.block_id,
            rooms: lines(&input.rooms),
        },
        PmsReservationCommandInput::Change(input) => Input::Change {
            reservation_id: &input.reservation_id,
            expected_reservation_version: input.expected_reservation_version,
            reason: &input.reason,
            notes: input.notes.as_deref(),
            group_name: input.group_name.as_deref(),
            campaign_code: input.campaign_code.as_deref(),
            rooms: lines(&input.rooms),
        },
        PmsReservationCommandInput::Cancel(input) => Input::Cancel {
            reservation_id: &input.reservation_id,
            expected_reservation_version: input.expected_reservation_version,
            reason: &input.reason,
            fee: input.fee,
        },
    };
    let record_id = command
        .input
        .reservation_id()
        .unwrap_or(&command.property_id);
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
