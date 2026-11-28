//! Native customer-portal projection and acknowledgement use cases.
//!
//! The composition root supplies the verified portal-session proof as
//! [`PortalCustomerActor`], the current Commercial purchase visibility as
//! [`PortalPurchaseScope`], the Files shared-file read, and one Holding
//! transaction (the customer-portal owned unit) around each call. This module
//! never verifies a session, evaluates package capability SQL, writes a source
//! order, or emits a notification/audit/outbox row itself; those are explicit
//! ports.
use crate::domain::portal::{
    CustomerPortalItem, CustomerPortalProjection, CustomerSharedFile, PROJECTION_VERSION,
    PortalAction, PortalActionRow, PortalBinding, PortalCommandFacts, PortalCommandInput,
    PortalCommandOutcome, PortalCommandPurpose, PortalCustomerActor, PortalPurchaseScope,
    action_already_recorded, attachment_action, binding_changed, binding_expired,
    binding_unavailable, command_key_required, idempotency_conflict, identity_required,
    projection_invalid, request_invalid, source_unavailable,
    valid_identifier, valid_key, valid_read_id, valid_request_hash,
};
use erpx_platform_shared_server::public::BusinessError;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

pub type PortalFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Command transport preserves Files' bounded required count without making
/// Files depend on this owner or changing the shared BusinessError contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PortalCommandError {
    Business(BusinessError),
    AttachmentRequired { required: u8 },
}
impl From<BusinessError> for PortalCommandError {
    fn from(error: BusinessError) -> Self { Self::Business(error) }
}

/// Read and record primitives over the existing customer portal tables. The
/// caller owns the serializable Holding transaction and the exact request
/// authority recheck after deferred effects.
pub trait CustomerPortalRepository: Send {
    type Error: From<BusinessError> + Send;

    fn projection<'a>(
        &'a mut self,
        actor: &'a PortalCustomerActor,
        scope: &'a PortalPurchaseScope,
        now: i64,
        read_id: &'a str,
    ) -> PortalFuture<'a, Vec<CustomerPortalItem>, Self::Error>;

    fn replay<'a>(
        &'a mut self,
        key: &'a str,
    ) -> PortalFuture<'a, Option<PortalActionRow>, Self::Error>;

    fn binding<'a>(
        &'a mut self,
        binding_id: &'a str,
        email: &'a str,
    ) -> PortalFuture<'a, Option<PortalBinding>, Self::Error>;

    fn identity<'a>(
        &'a mut self,
        binding_id: &'a str,
        actor: &'a PortalCustomerActor,
    ) -> PortalFuture<'a, bool, Self::Error>;

    fn existing<'a>(
        &'a mut self,
        binding_id: &'a str,
        action: PortalAction,
    ) -> PortalFuture<'a, Option<PortalActionRow>, Self::Error>;

    fn record<'a>(
        &'a mut self,
        command: &'a PortalCommandFacts,
    ) -> PortalFuture<'a, (), Self::Error>;
}

/// Files owner read of the approved private previews for these bindings.
pub trait CustomerPortalSharedFiles: Send {
    type Error: From<BusinessError> + Send;

    fn shared_files<'a>(
        &'a mut self,
        actor: &'a PortalCustomerActor,
        binding_ids: &'a [String],
        now: i64,
    ) -> PortalFuture<'a, BTreeMap<String, Vec<CustomerSharedFile>>, Self::Error>;
}

/// Side effects owned by other owners. Every method must already run on the
/// same Holding transaction as [`CustomerPortalRepository::record`]; the
/// application calls them in this order and lets the composition commit once.
pub trait CustomerPortalEffects: Send {
    type Error: From<BusinessError> + Send;

    /// Commercial `sales.portal` authority and the scheduled feature release.
    /// Composition must also recheck the exact binding and request identity
    /// after deferred effects, before commit.
    fn authorize<'a>(
        &'a mut self,
        actor: &'a PortalCustomerActor,
        binding: &'a PortalBinding,
        purpose: PortalCommandPurpose,
    ) -> PortalFuture<'a, (), Self::Error>;

    /// Files attachment-evidence preparation for `request_support`.
    fn evidence<'a>(
        &'a mut self,
        command: &'a PortalCommandFacts,
    ) -> PortalFuture<'a, Option<PortalCommandEvidence>, Self::Error>;

    /// Collaboration staff notification.
    fn notify<'a>(
        &'a mut self,
        command: &'a PortalCommandFacts,
        message: &'a str,
    ) -> PortalFuture<'a, (), Self::Error>;

    /// Workflow/audit receipt.
    fn audit<'a>(
        &'a mut self,
        command: &'a PortalCommandFacts,
        event_id: &'a str,
    ) -> PortalFuture<'a, (), Self::Error>;

    /// Workflow outbox event carrying the Files manifest.
    fn outbox<'a>(
        &'a mut self,
        command: &'a PortalCommandFacts,
        event_id: &'a str,
        manifest: &'a [serde_json::Value],
    ) -> PortalFuture<'a, (), Self::Error>;
}

/// Generate each independent event identifier at its actual write boundary.
pub trait PortalEventIdentifiers {
    type Error: From<BusinessError>;
    fn next_event_id(&mut self) -> Result<String, Self::Error>;
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PortalCommandEvidence {
    pub manifest: Vec<serde_json::Value>,
}

pub async fn customer_portal_projection<U, Err>(
    actor: &PortalCustomerActor,
    scope: &PortalPurchaseScope,
    unit: &mut U,
    now: i64,
    read_id: &str,
) -> Result<CustomerPortalProjection, Err>
where
    U: CustomerPortalRepository<Error = Err> + CustomerPortalSharedFiles<Error = Err>,
    Err: From<BusinessError>,
{
    if !valid_read_id(read_id) {
        return Err(projection_invalid().into());
    }
    scope.validate()?;
    let mut items = unit.projection(actor, scope, now, read_id).await?;
    let ids: Vec<String> = items.iter().map(|item| item.binding_id.clone()).collect();
    let shared = unit.shared_files(actor, &ids, now).await?;
    for item in &mut items {
        item.shared_files = shared.get(&item.binding_id).cloned().unwrap_or_default();
    }
    let projection = CustomerPortalProjection {
        items,
        projection_version: PROJECTION_VERSION,
    };
    projection.validate()?;
    Ok(projection)
}

/// Record one idempotent acknowledgement. `request_hash` and `id` are supplied
/// by composition (a hash of the verified actor plus validated input, and a
/// unique identifier); the owner revalidates every fact against the current
/// binding before the first write.
#[allow(clippy::too_many_arguments)]
pub async fn customer_portal_command<U, Err>(
    actor: &PortalCustomerActor,
    input: &PortalCommandInput,
    key: &str,
    request_hash: &str,
    id: &str,
    now: i64,
    unit: &mut U,
) -> Result<PortalCommandOutcome, Err>
where
    U: CustomerPortalRepository<Error = Err> + CustomerPortalEffects<Error = Err> + PortalEventIdentifiers<Error = Err>,
    Err: From<BusinessError>,
{
    if !valid_key(key) {
        return Err(command_key_required().into());
    }
    if !valid_request_hash(request_hash) || !valid_identifier(id) {
        return Err(request_invalid().into());
    }
    input.validate()?;

    let prior = unit.replay(key).await?;
    let binding = unit
        .binding(&input.binding_id, &actor.email)
        .await?
        .ok_or_else(binding_unavailable)?;
    binding.validate()?;
    if !binding.is_current(now) {
        return Err(binding_expired().into());
    }
    if prior.is_none() && binding.binding_version != input.expected_binding_version {
        return Err(binding_changed().into());
    }
    if !binding.is_actionable() {
        return Err(source_unavailable().into());
    }
    if !unit.identity(&binding.binding_id, actor).await? {
        return Err(identity_required().into());
    }
    unit
        .authorize(
            actor,
            &binding,
            if prior.is_some() {
                PortalCommandPurpose::History
            } else {
                PortalCommandPurpose::SettleExisting
            },
        )
        .await?;
    if let Some(prior) = prior {
        if prior.user_id != actor.user_id || prior.request_hash != request_hash {
            return Err(idempotency_conflict().into());
        }
        return Ok(PortalCommandOutcome::from_row(&prior));
    }
    if unit
        .existing(&binding.binding_id, input.action)
        .await?
        .is_some()
    {
        return Err(action_already_recorded().into());
    }
    if input.action != PortalAction::RequestSupport && !input.attachment_set_ids.is_empty() {
        return Err(attachment_action().into());
    }
    if !input.attachment_set_ids.is_empty() {
        actor.require_attachment_context()?;
    }
    let command = PortalCommandFacts {
        input: input.clone(),
        key: key.to_owned(),
        id: id.to_owned(),
        now,
        request_hash: request_hash.to_owned(),
        actor: actor.clone(),
        binding,
    };
    let evidence = if command.input.attachment_set_ids.is_empty() {
        None
    } else {
        unit.evidence(&command).await?
    };
    unit.record(&command).await?;
    let message = match command.input.action {
        PortalAction::RequestSupport => {
            "Customer requested support on an order; review and contact the customer."
        }
        PortalAction::AcknowledgeDelivery => {
            "Customer acknowledged delivery; review the order and fulfillment record."
        }
    };
    unit.notify(&command, message).await?;
    let event_id = unit.next_event_id()?;
    unit.audit(&command, &event_id).await?;
    let outbox_id = unit.next_event_id()?;
    let manifest = evidence
        .as_ref()
        .map(|value| value.manifest.as_slice())
        .unwrap_or(&[]);
    unit.outbox(&command, &outbox_id, manifest).await?;
    Ok(PortalCommandOutcome::new(
        command.id,
        command.binding.binding_id.clone(),
        command.binding.order_id.clone(),
        command.input.action.as_str(),
        "recorded".to_owned(),
        command.now,
    ))
}
