//! Native PMS guest-account, charge, adjustment and transfer orchestration.
//!
//! The repository owns only PMS rows under the caller's serializable unit.
//! Current Account/assignment authority is a public port supplied by
//! composition; this use case never mints an actor, Company, permission or
//! staff label. Cash/card settlement, refunds, invoice/credit-note issuance and
//! journal posting stay outside this owner port.
use crate::application::room_board::PmsBoardFuture;
use crate::domain::guest_account::{
    PmsGuestAccountActor, PmsGuestAccountCommand, PmsGuestAccountFacts, PmsGuestAccountInput,
    PmsGuestAccountPlan, PmsGuestAccountResult, invalid, plan_guest_account,
};
use crate::domain::room_board::{not_found, record_not_found};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use sha2::{Digest, Sha256};

pub trait PmsGuestAccountRepository: Send {
    type Error: From<BusinessError> + Send;
    /// Replay one committed receipt for this exact command key. A mismatched
    /// request hash is an idempotency conflict, never a second write.
    fn replay<'a>(
        &'a mut self,
        key: &'a CommandKey,
        request_hash: &'a str,
        command: &'a PmsGuestAccountCommand,
    ) -> PmsBoardFuture<'a, Option<PmsGuestAccountResult>, Self::Error>;
    /// Lock the property row and load every owner fact this command needs.
    fn load<'a>(
        &'a mut self,
        actor: &'a PmsGuestAccountActor,
        command: &'a PmsGuestAccountCommand,
    ) -> PmsBoardFuture<'a, PmsGuestAccountFacts, Self::Error>;
    /// Persist the planned rows, property version, receipt, audit and outbox
    /// together in the caller's serializable transaction.
    fn commit<'a>(
        &'a mut self,
        actor: &'a PmsGuestAccountActor,
        key: &'a CommandKey,
        request_hash: &'a str,
        plan: PmsGuestAccountPlan,
        now: i64,
    ) -> PmsBoardFuture<'a, PmsGuestAccountResult, Self::Error>;
}

fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "The retained result is unavailable.",
    )
}

/// Execute one retained guest-account/charge command as one idempotent owner
/// command.
pub async fn pms_guest_account<R>(
    actor: &PmsGuestAccountActor,
    key: &CommandKey,
    command: &PmsGuestAccountCommand,
    repository: &mut R,
    now: i64,
) -> Result<PmsGuestAccountResult, R::Error>
where
    R: PmsGuestAccountRepository,
{
    actor.require_command(command.operation(), now)?;
    let mut normalized = command.clone();
    normalized.normalize();
    normalized.validate()?;
    let request_hash = guest_account_request_hash(actor, &normalized)?;
    if let Some(result) = repository.replay(key, &request_hash, &normalized).await? {
        if result.property_id != normalized.property_id
            || result.version != normalized.expected_version + 1
            || result.id.is_empty()
            || result.account_id.is_empty()
        {
            return Err(idempotency_conflict().into());
        }
        return Ok(result);
    }
    let facts = repository.load(actor, &normalized).await?;
    if facts.property.id != normalized.property_id
        || facts.property.company_id != actor.board.company_id
    {
        return Err(not_found().into());
    }
    match &normalized.input {
        PmsGuestAccountInput::Create(_) if facts.guest.is_none() => {
            return Err(record_not_found().into());
        }
        PmsGuestAccountInput::Transfer(_) => {
            if facts.original_entry.is_none() || facts.destination.is_none() {
                return Err(record_not_found().into());
            }
        }
        PmsGuestAccountInput::Adjustment(input)
            if !input.original_entry_id.is_empty() && facts.original_entry.is_none() =>
        {
            return Err(record_not_found().into());
        }
        _ => {}
    }
    if normalized.input.account_id().is_some() && facts.account.is_none() {
        return Err(record_not_found().into());
    }
    let plan = plan_guest_account(actor, key, &facts, &normalized, now)?;
    repository
        .commit(actor, key, &request_hash, plan, now)
        .await
}

/// Stable request fingerprint used by the command receipt. It covers the same
/// actor, Company/branch scope, operation, target property/account/entry and
/// bounded input as the retained command receipt.
pub fn guest_account_request_hash(
    actor: &PmsGuestAccountActor,
    command: &PmsGuestAccountCommand,
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
        #[serde(rename = "guest_account.create")]
        Create {
            guest_id: &'a str,
            reservation_id: &'a str,
            name: &'a str,
        },
        #[serde(rename = "guest_account.update")]
        Update { account_id: &'a str, name: &'a str },
        #[serde(rename = "charge.add")]
        ChargeAdd {
            account_id: &'a str,
            price: i64,
            description: &'a str,
            stay_id: &'a str,
            source_key: &'a str,
        },
        #[serde(rename = "charge.import_pos")]
        ChargeImport {
            account_id: &'a str,
            stay_id: &'a str,
            source_key: &'a str,
            description: &'a str,
            net: i64,
            service: i64,
            tax: i64,
            total: i64,
        },
        #[serde(rename = "guest_account.adjustment")]
        Adjustment {
            account_id: &'a str,
            original_entry_id: &'a str,
            description: &'a str,
            net: i64,
            service: i64,
            tax: i64,
            total: i64,
            source_key: &'a str,
        },
        #[serde(rename = "guest_account.transfer")]
        Transfer {
            entry_id: &'a str,
            to_account_id: &'a str,
            reason: &'a str,
        },
    }
    let input = match &command.input {
        PmsGuestAccountInput::Create(input) => Input::Create {
            guest_id: &input.guest_id,
            reservation_id: &input.reservation_id,
            name: &input.name,
        },
        PmsGuestAccountInput::Update(input) => Input::Update {
            account_id: &input.account_id,
            name: &input.name,
        },
        PmsGuestAccountInput::ChargeAdd(input) => Input::ChargeAdd {
            account_id: &input.account_id,
            price: input.price,
            description: &input.description,
            stay_id: &input.stay_id,
            source_key: &input.source_key,
        },
        PmsGuestAccountInput::ChargeImport(input) => Input::ChargeImport {
            account_id: &input.account_id,
            stay_id: &input.stay_id,
            source_key: &input.source_key,
            description: &input.description,
            net: input.net,
            service: input.service,
            tax: input.tax,
            total: input.total,
        },
        PmsGuestAccountInput::Adjustment(input) => Input::Adjustment {
            account_id: &input.account_id,
            original_entry_id: &input.original_entry_id,
            description: &input.description,
            net: input.net,
            service: input.service,
            tax: input.tax,
            total: input.total,
            source_key: &input.source_key,
        },
        PmsGuestAccountInput::Transfer(input) => Input::Transfer {
            entry_id: &input.entry_id,
            to_account_id: &input.to_account_id,
            reason: &input.reason,
        },
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
