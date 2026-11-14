//! Native PMS day-close command orchestration.
//!
//! The repository owns only PMS rows under the caller's serializable unit.
//! The day-control counts, Finance chart/account answers and — once it exists —
//! the room-night posting consumer are public ports supplied by composition.
//! This use case never mints an actor, a Company, a permission, a Finance
//! balance or a folio charge.
use crate::application::room_board::PmsBoardFuture;
use crate::domain::night_audit::{
    PmsNightAuditActor, PmsNightAuditCommand, PmsNightAuditFacts, PmsNightAuditGuard,
    PmsNightAuditOperation, PmsNightAuditPlan, PmsNightAuditResult, PmsNightDayControlFacts,
    PmsNightSetupFacts, PmsNightSetupScope, invalid, plan_night_audit,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use sha2::{Digest, Sha256};

/// Cross-owner day-close guard. POS, Finance and Inventory own these counts;
/// the PMS command only decides whether each count blocks the close.
#[derive(Clone, Debug)]
pub struct PmsNightControlScope {
    pub company_id: String,
    pub branch_id: String,
    pub property_id: String,
    pub business_date: String,
}

pub trait PmsNightAuditGuardPort {
    type Error: Send;
    /// Retained `hospitalityDayControls` counts for this hotel business date.
    fn day_controls<'a>(
        &'a mut self,
        scope: &'a PmsNightControlScope,
        now: i64,
    ) -> PmsBoardFuture<'a, PmsNightDayControlFacts, Self::Error>;
    /// Retained `validSetup`: the configured chart and control accounts still
    /// exist as the retained categories in the same company chart/currency.
    fn setup<'a>(
        &'a mut self,
        scope: &'a PmsNightSetupScope,
        now: i64,
    ) -> PmsBoardFuture<'a, PmsNightSetupFacts, Self::Error>;
}

pub trait PmsNightAuditRepository: Send {
    type Error: From<BusinessError> + Send;
    /// Replay one committed receipt for this exact command key. A mismatched
    /// request hash is an idempotency conflict, never a second write.
    fn replay<'a>(
        &'a mut self,
        key: &'a CommandKey,
        request_hash: &'a str,
        command: &'a PmsNightAuditCommand,
    ) -> PmsBoardFuture<'a, Option<PmsNightAuditResult>, Self::Error>;
    /// Lock the property row and load every owner fact this command needs.
    fn load<'a>(
        &'a mut self,
        actor: &'a PmsNightAuditActor,
        command: &'a PmsNightAuditCommand,
    ) -> PmsBoardFuture<'a, PmsNightAuditFacts, Self::Error>;
    /// Persist the planned rows, property version, job, receipt, audit and
    /// outbox together in the caller's serializable transaction.
    fn commit<'a>(
        &'a mut self,
        actor: &'a PmsNightAuditActor,
        key: &'a CommandKey,
        request_hash: &'a str,
        plan: PmsNightAuditPlan,
        now: i64,
    ) -> PmsBoardFuture<'a, PmsNightAuditResult, Self::Error>;
}

fn replay_shape_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "The retained result is unavailable.",
    )
}

/// Execute one retained `night.start`/`night.resume` as one idempotent owner
/// command. The worker-only `night.step` has its own entry point.
pub async fn pms_night_audit<R, G>(
    actor: &PmsNightAuditActor,
    key: &CommandKey,
    command: &PmsNightAuditCommand,
    repository: &mut R,
    guard: &mut G,
    now: i64,
) -> Result<PmsNightAuditResult, R::Error>
where
    R: PmsNightAuditRepository,
    G: PmsNightAuditGuardPort,
    R::Error: From<G::Error>,
{
    if command.worker {
        return Err(invalid().into());
    }
    // The retained user entry point rejects `night.step` with 403
    // `PMS_WORKER_ONLY` before the receipt or the property lock.
    actor.require_command(command.operation(), false, now)?;
    command.validate()?;
    let request_hash = night_audit_request_hash(actor, command)?;
    if let Some(result) = repository.replay(key, &request_hash, command).await? {
        if !fits_replay(command, &result) {
            return Err(replay_shape_invalid().into());
        }
        return Ok(result);
    }
    let facts = repository.load(actor, command).await?;
    require_scope(actor, command, &facts)?;
    // Retained `night.start` reads the cross-owner day-control report; a resume
    // keeps the stored blockers and only revalidates the Finance setup.
    let controls = if command.operation() == PmsNightAuditOperation::Start {
        guard
            .day_controls(
                &PmsNightControlScope {
                    company_id: facts.property.company_id.clone(),
                    branch_id: facts.property.branch_id.clone(),
                    property_id: facts.property.id.clone(),
                    business_date: facts.property.business_date.clone(),
                },
                now,
            )
            .await
            .map_err(R::Error::from)?
    } else {
        PmsNightDayControlFacts::default()
    };
    let scope = PmsNightSetupScope::from_property(&facts.property.view()?);
    let setup = guard.setup(&scope, now).await.map_err(R::Error::from)?;
    let plan = plan_night_audit(
        actor,
        key,
        &facts,
        PmsNightAuditGuard::User {
            controls: &controls,
            setup: &setup,
            scope: &scope,
        },
        command,
        now,
    )?;
    repository
        .commit(actor, key, &request_hash, plan, now)
        .await
}

/// Execute the worker-only `night.step`. It reads PMS-owned facts only and
/// fails closed with `PMS_NIGHT_POSTING_PORT_REQUIRED` while any in-house stay
/// still lacks its night charge.
pub async fn pms_night_audit_step<R>(
    actor: &PmsNightAuditActor,
    key: &CommandKey,
    command: &PmsNightAuditCommand,
    repository: &mut R,
    now: i64,
) -> Result<PmsNightAuditResult, R::Error>
where
    R: PmsNightAuditRepository,
{
    if command.operation() != PmsNightAuditOperation::Step || !command.worker {
        return Err(invalid().into());
    }
    actor.require_command(command.operation(), true, now)?;
    command.validate()?;
    let request_hash = night_audit_request_hash(actor, command)?;
    if let Some(result) = repository.replay(key, &request_hash, command).await? {
        if !fits_replay(command, &result) {
            return Err(replay_shape_invalid().into());
        }
        return Ok(result);
    }
    let facts = repository.load(actor, command).await?;
    require_scope(actor, command, &facts)?;
    let plan = plan_night_audit(actor, key, &facts, PmsNightAuditGuard::Worker, command, now)?;
    repository
        .commit(actor, key, &request_hash, plan, now)
        .await
}

/// The receipt is the retained outcome for this exact request hash, so this
/// only checks that its identity shape can still satisfy the command.
fn fits_replay(command: &PmsNightAuditCommand, result: &PmsNightAuditResult) -> bool {
    result.property_id == command.property_id
        && result.version >= 1
        && (command.result_id_is_target() == (command.target_id() == Some(result.id.as_str())))
}

fn require_scope(
    actor: &PmsNightAuditActor,
    command: &PmsNightAuditCommand,
    facts: &PmsNightAuditFacts,
) -> Result<(), BusinessError> {
    if facts.property.id != command.property_id
        || facts.property.company_id != actor.board.company_id
        || actor
            .board
            .branch_id
            .as_ref()
            .is_some_and(|branch| branch != &facts.property.branch_id)
    {
        return Err(crate::domain::room_board::not_found());
    }
    if command.night_id.is_some()
        && facts
            .night
            .as_ref()
            .is_none_or(|night| Some(night.id.as_str()) != command.target_id())
    {
        return Err(crate::domain::room_board::record_not_found());
    }
    Ok(())
}

/// Stable request fingerprint used by the command receipt. It covers the same
/// actor, Company/branch scope, operation, target property/night and bounded
/// input as the retained command receipt.
pub fn night_audit_request_hash(
    actor: &PmsNightAuditActor,
    command: &PmsNightAuditCommand,
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
        worker: bool,
        expected_version: Option<i64>,
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
        record_id: command.target_id().unwrap_or(&command.property_id),
        worker: command.worker,
        expected_version: command.expected_version,
    };
    let payload = serde_json::to_vec(&value).map_err(|_| invalid())?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}
