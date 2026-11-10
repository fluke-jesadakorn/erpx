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
    projection_invalid, request_invalid, source_invalid, source_unavailable,
    valid_event_identifier, valid_identifier, valid_key, valid_read_id, valid_request_hash,
};
use erpx_server_shared::public::BusinessError;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

pub type PortalFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PortalCommandEvidence {
    pub manifest: Vec<serde_json::Value>,
}

pub async fn customer_portal_projection<R, F, Err>(
    actor: &PortalCustomerActor,
    scope: &PortalPurchaseScope,
    repository: &mut R,
    files: &mut F,
    now: i64,
    read_id: &str,
) -> Result<CustomerPortalProjection, Err>
where
    R: CustomerPortalRepository<Error = Err>,
    F: CustomerPortalSharedFiles<Error = Err>,
    Err: From<BusinessError>,
{
    if !valid_read_id(read_id) {
        return Err(projection_invalid().into());
    }
    scope.validate()?;
    let mut items = repository.projection(actor, scope, now, read_id).await?;
    let ids: Vec<String> = items.iter().map(|item| item.binding_id.clone()).collect();
    let shared = files.shared_files(actor, &ids, now).await?;
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
pub async fn customer_portal_command<R, E, Err>(
    actor: &PortalCustomerActor,
    input: &PortalCommandInput,
    key: &str,
    request_hash: &str,
    id: &str,
    now: i64,
    repository: &mut R,
    effects: &mut E,
) -> Result<PortalCommandOutcome, Err>
where
    R: CustomerPortalRepository<Error = Err>,
    E: CustomerPortalEffects<Error = Err>,
    Err: From<BusinessError>,
{
    if !valid_key(key) {
        return Err(command_key_required().into());
    }
    if !valid_request_hash(request_hash) || !valid_identifier(id) {
        return Err(request_invalid().into());
    }
    input.validate()?;

    let prior = repository.replay(key).await?;
    let binding = repository
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
    if !repository.identity(&binding.binding_id, actor).await? {
        return Err(identity_required().into());
    }
    effects
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
    if repository
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
        effects.evidence(&command).await?
    };
    repository.record(&command).await?;
    let message = match command.input.action {
        PortalAction::RequestSupport => {
            "Customer requested support on an order; review and contact the customer."
        }
        PortalAction::AcknowledgeDelivery => {
            "Customer acknowledged delivery; review the order and fulfillment record."
        }
    };
    effects.notify(&command, message).await?;
    let event_id = event_identifier(&command.id, "audit")?;
    effects.audit(&command, &event_id).await?;
    let outbox_id = event_identifier(&command.id, "outbox")?;
    let manifest = evidence
        .as_ref()
        .map(|value| value.manifest.as_slice())
        .unwrap_or(&[]);
    effects.outbox(&command, &outbox_id, manifest).await?;
    Ok(PortalCommandOutcome::new(
        command.id,
        command.binding.binding_id.clone(),
        command.binding.order_id.clone(),
        command.input.action.as_str(),
        "recorded".to_owned(),
        command.now,
    ))
}

fn event_identifier(id: &str, suffix: &str) -> Result<String, BusinessError> {
    let value = format!("{id}-{suffix}");
    if !valid_event_identifier(&value) {
        return Err(source_invalid());
    }
    Ok(value)
}
