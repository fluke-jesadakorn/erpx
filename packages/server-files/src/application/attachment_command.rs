//! Generic Files attachment remove command preflight and ports.
//!
//! The use case opens no transaction and reads no HTTP request. Composition
//! supplies the current Account/Holding/Company authority and the owning
//! record's authorization; the Files adapter owns the link/audit SQL.
use crate::domain::attachment_command::{
    WithdrawalDecision, normalize_withdrawal_reason, valid_command_key, withdrawal_decision,
};
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;

/// Exact Files-owned target facts presented to the owning-record authority.
/// The port must reject a Company, branch, department or record version that
/// the current selected work context cannot reach.
pub struct WithdrawTarget<'a> {
    pub company_id: &'a str,
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
    pub link_id: &'a str,
    pub set_id: &'a str,
    pub target_type: &'a str,
    pub target_id: &'a str,
    pub purpose_key: &'a str,
    pub component_kind: &'a str,
    pub expected_target_version: i64,
    pub created_by: &'a str,
}

/// Composition-owned authority port. It is invoked before the Files row lock
/// and again by the caller after commit when the owner needs a post-image.
pub trait AttachmentWithdrawAuthority: Sync {
    fn authorize_withdraw<'a>(
        &'a self,
        c: &'a mut PgConnection,
        target: &'a WithdrawTarget<'a>,
    ) -> PgWorkFuture<'a, (), BusinessError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WithdrawAttachmentRequest<'a> {
    pub actor_id: &'a str,
    pub company_id: &'a str,
    pub link_id: &'a str,
    pub reason: &'a str,
    pub command_key: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WithdrawAttachmentResult {
    pub link_id: String,
    pub state: &'static str,
    pub replay: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedWithdrawal {
    pub reason: String,
    pub command_key: String,
}

/// Normalize exactly once, before any transaction opens or byte is written.
pub fn normalize_withdrawal(
    reason: &str,
    command_key: &str,
) -> Result<NormalizedWithdrawal, BusinessError> {
    if !valid_command_key(command_key) {
        return Err(BusinessError::invalid(
            "COMMAND_KEY_REQUIRED",
            "A valid Idempotency-Key is required.",
        ));
    }
    Ok(NormalizedWithdrawal {
        reason: normalize_withdrawal_reason(reason)?,
        command_key: command_key.to_owned(),
    })
}

pub struct WithdrawalFacts<'a> {
    pub state: &'a str,
    pub creator: &'a str,
    pub withdrawn_by: Option<&'a str>,
    pub previous_reason: Option<&'a str>,
    pub actor: &'a str,
    pub reason: &'a str,
}

pub fn prepare_withdrawal(facts: WithdrawalFacts<'_>) -> Result<WithdrawalDecision, BusinessError> {
    withdrawal_decision(
        facts.state,
        facts.creator,
        facts.withdrawn_by,
        facts.previous_reason,
        facts.actor,
        facts.reason,
    )
}
