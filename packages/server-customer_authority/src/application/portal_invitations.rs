//! Native Customer Portal invitation use cases.
//!
//! Ports supply the owning transaction, current binding/identity/invitation
//! facts, replay and the committed readback. This layer never opens a
//! transaction, reads HTTP or mints authority.
use crate::domain::access::AccessActor;
use crate::domain::portal_invitations::{
    ActiveInvitation, InvitationBinding, InvitationCommand, InvitationHistory, InvitationPlan,
    InvitationReceipt, idempotency_conflict, issue_plan, not_found, source_changed,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type InvitationFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct InvitationReceiptSnapshot {
    pub request_hash: String,
    pub result_json: String,
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
}

pub trait InvitationRepository: Send {
    type Error: From<BusinessError> + Send;

    fn binding<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        binding_id: &'a str,
    ) -> InvitationFuture<'a, Option<InvitationBinding>, Self::Error>;

    fn active<'a>(
        &'a mut self,
        binding_id: &'a str,
    ) -> InvitationFuture<'a, Option<ActiveInvitation>, Self::Error>;

    fn digest<'a>(&'a mut self, value: &'a str) -> InvitationFuture<'a, String, Self::Error>;

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> InvitationFuture<'a, Option<InvitationReceiptSnapshot>, Self::Error>;

    #[allow(clippy::too_many_arguments)]
    fn create<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        key: &'a str,
        operation: &'a str,
        request_hash: &'a str,
        plan: &'a InvitationPlan,
        binding_id: &'a str,
        expected_version: i64,
    ) -> InvitationFuture<'a, InvitationReceipt, Self::Error>;

    fn history<'a>(
        &'a mut self,
        binding_id: &'a str,
    ) -> InvitationFuture<'a, InvitationHistory, Self::Error>;
}

/// Exact request digest payload; kept in the retained TypeScript field order.
fn command_digest_input(command: &InvitationCommand) -> String {
    serde_json::json!({
        "action": command.action.as_str(),
        "expectedVersion": command.expected_version,
        "reason": command.reason,
    })
    .to_string()
}

fn replay(
    snapshot: InvitationReceiptSnapshot,
    actor: &AccessActor,
    operation: &str,
    binding_id: &str,
    request_hash: &str,
) -> Result<Value, BusinessError> {
    if snapshot.request_hash != request_hash
        || snapshot.operation != operation
        || snapshot.actor_id != actor.user_id.as_str()
        || snapshot.record_id != binding_id
    {
        return Err(idempotency_conflict());
    }
    serde_json::from_str(&snapshot.result_json).map_err(|_| source_changed())
}

/// Issue or resend one invitation for an approved binding.
#[allow(clippy::too_many_arguments)]
pub async fn issue_invitation<R: InvitationRepository>(
    repository: &mut R,
    actor: &AccessActor,
    binding_id: &str,
    key: &str,
    command: InvitationCommand,
    secret: &str,
    holding_slug: &str,
    now: i64,
) -> Result<Value, R::Error> {
    CommandKey::new(key.to_owned())?;
    let operation = command.action.operation();
    let digest = repository.digest(&command_digest_input(&command)).await?;
    if let Some(snapshot) = repository.receipt(key).await? {
        return Ok(replay(snapshot, actor, &operation, binding_id, &digest)?);
    }
    let binding = repository
        .binding(actor, binding_id)
        .await?
        .ok_or_else(not_found)?;
    let active = repository.active(binding_id).await?;
    let plan = issue_plan(
        actor,
        &binding,
        active.as_ref(),
        &command,
        secret,
        holding_slug,
        now,
    )?;
    let receipt = repository
        .create(
            actor,
            key,
            &operation,
            &digest,
            &plan,
            binding_id,
            command.expected_version,
        )
        .await?;
    Ok(serde_json::to_value(receipt).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "CUSTOMER_ACCESS_READBACK_INVALID",
            "The customer invitation was not persisted as submitted.",
        )
    })?)
}

pub async fn invitation_history<R: InvitationRepository>(
    repository: &mut R,
    binding_id: &str,
) -> Result<InvitationHistory, R::Error> {
    repository.history(binding_id).await
}
