//! Support customer ticket-portal use cases.
//!
//! Callers supply the already-authorized staff/customer context, the owner
//! transaction, replay lookup and the exact owner effect application. This layer
//! decides order, purpose, version and independent-review requirements; it never
//! opens a transaction, invokes a provider or writes a receipt itself.
#![allow(async_fn_in_trait)]

use crate::domain::ticket_portal::{
    AcknowledgementInput, AcknowledgementResult, PortalActor, PortalBindingRow, PortalCustomer,
    PortalPlan, PortalResponse, PortalRights, PortalTicket, validate_command_key,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortalPurpose {
    History,
    NewWork,
    SettleExisting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortalSourceImage {
    pub bindings: String,
    pub ticket: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriorAcknowledgement {
    pub binding_id: String,
    pub user_id: String,
    pub request_hash: String,
    pub result: AcknowledgementResult,
}

/// Owner/composition boundary. The implementation owns the Holding transaction
/// and current authority; it may not independently commit, retry or call out.
pub trait TicketPortalPorts {
    fn now(&self) -> i64;

    async fn authorize_staff(
        &mut self,
        actor: &PortalActor,
        manage: bool,
    ) -> Result<(), BusinessError>;

    async fn authorize_capability(
        &mut self,
        company_id: &str,
        purpose: PortalPurpose,
    ) -> Result<(), BusinessError>;

    async fn rights(&mut self, actor: &PortalActor) -> Result<PortalRights, BusinessError>;

    async fn installed(&mut self) -> Result<bool, BusinessError>;

    async fn replay(
        &mut self,
        actor: &PortalActor,
        key: &str,
        operation: &str,
        record_id: &str,
        input: &Value,
    ) -> Result<Option<Value>, BusinessError>;

    async fn replay_proof(
        &mut self,
        actor: &PortalActor,
        key: &str,
        binding_id: &str,
        action: &str,
        result: &Value,
    ) -> Result<(), BusinessError>;

    async fn ticket(
        &mut self,
        actor: &PortalActor,
        id: &str,
    ) -> Result<PortalTicket, BusinessError>;

    async fn binding(
        &mut self,
        actor: &PortalActor,
        id: &str,
    ) -> Result<Option<PortalBindingRow>, BusinessError>;

    async fn current_response(
        &mut self,
        binding_id: &str,
    ) -> Result<Option<PortalResponse>, BusinessError>;

    /// Apply the owner rows for this plan. The composition retains its command
    /// receipt/audit/outbox on the same connection before returning.
    async fn apply(
        &mut self,
        actor: &PortalActor,
        key: &str,
        operation: &str,
        plan: &PortalPlan,
        input: &Value,
        result: &Value,
    ) -> Result<(), BusinessError>;

    /// Capture the exact binding/ticket image before owner effects.
    async fn snapshot(
        &mut self,
        binding_ids: &[String],
        ticket_id: Option<&str>,
    ) -> Result<PortalSourceImage, BusinessError>;

    /// Recheck every current source image before the transaction completes.
    async fn guard(
        &mut self,
        actor: &PortalActor,
        binding_ids: &[String],
        ticket_id: Option<&str>,
        purpose: PortalPurpose,
        expected: &PortalSourceImage,
    ) -> Result<(), BusinessError>;

    async fn customer_binding(
        &mut self,
        customer: &PortalCustomer,
        id: &str,
    ) -> Result<PortalBindingRow, BusinessError>;

    async fn acknowledgement_prior(
        &mut self,
        key: &str,
    ) -> Result<Option<PriorAcknowledgement>, BusinessError>;

    async fn apply_acknowledgement(
        &mut self,
        customer: &PortalCustomer,
        key: &str,
        plan: &PortalPlan,
        input: &Value,
        result: &Value,
    ) -> Result<(), BusinessError>;

    async fn guard_acknowledgement(
        &mut self,
        customer: &PortalCustomer,
        binding_id: &str,
        response_id: &str,
    ) -> Result<(), BusinessError>;
}

fn to_value<T: serde::Serialize>(value: &T) -> Result<Value, BusinessError> {
    serde_json::to_value(value)
        .map_err(|_| BusinessError::invalid("SUPPORT_PORTAL_INPUT_INVALID", "Invalid request."))
}

fn operation(action: &str) -> String {
    format!("support.ticket-portal.{action}")
}

pub async fn prepare<P: TicketPortalPorts>(
    ports: &mut P,
    actor: &PortalActor,
    raw: &Value,
    key: &str,
) -> Result<Value, BusinessError> {
    validate_command_key(key)?;
    let input = crate::domain::ticket_portal::parse_prepare(raw)?;
    let wire = to_value(&input)?;
    ports.authorize_staff(actor, true).await?;
    let ticket = ports.ticket(actor, &input.ticket_id).await?;
    let prior = ports
        .replay(
            actor,
            key,
            "support.ticket-portal.prepare",
            &format!("support-portal-{key}"),
            &wire,
        )
        .await?;
    let purpose = if prior.is_some() {
        PortalPurpose::History
    } else {
        PortalPurpose::NewWork
    };
    ports
        .authorize_capability(&actor.company_id, purpose)
        .await?;
    if let Some(prior) = prior {
        ports
            .replay_proof(
                actor,
                key,
                &format!("support-portal-{key}"),
                "prepare",
                &prior,
            )
            .await?;
        let expected = ports
            .snapshot(&[format!("support-portal-{key}")], Some(&input.ticket_id))
            .await?;
        ports
            .guard(
                actor,
                &[format!("support-portal-{key}")],
                Some(&input.ticket_id),
                purpose,
                &expected,
            )
            .await?;
        return Ok(prior);
    }
    let plan =
        crate::domain::ticket_portal::plan_prepare(actor, &input, &ticket, key, ports.now())?;
    let PortalPlan::Prepare { result, .. } = &plan else {
        return Err(BusinessError::invalid(
            "SUPPORT_PORTAL_INPUT_INVALID",
            "Invalid Support Portal plan.",
        ));
    };
    let result = to_value(result)?;
    ports
        .apply(
            actor,
            key,
            "support.ticket-portal.prepare",
            &plan,
            &wire,
            &result,
        )
        .await?;
    let expected = ports
        .snapshot(&[format!("support-portal-{key}")], Some(&input.ticket_id))
        .await?;
    ports
        .guard(
            actor,
            &[format!("support-portal-{key}")],
            Some(&input.ticket_id),
            purpose,
            &expected,
        )
        .await?;
    Ok(result)
}

pub async fn decide<P: TicketPortalPorts>(
    ports: &mut P,
    actor: &PortalActor,
    id: &str,
    raw: &Value,
    key: &str,
) -> Result<Value, BusinessError> {
    validate_command_key(key)?;
    let input = crate::domain::ticket_portal::parse_decision(raw)?;
    let wire = to_value(&input)?;
    ports.authorize_staff(actor, true).await?;
    let binding = ports.binding(actor, id).await?.ok_or_else(|| {
        BusinessError::new(
            erpx_server_shared::public::FailureKind::NotFound,
            "SUPPORT_PORTAL_NOT_FOUND",
            "Support customer access is unavailable.",
        )
    })?;
    let operation = operation(input.action.as_str());
    let prior = ports.replay(actor, key, &operation, id, &wire).await?;
    let purpose = if prior.is_some() {
        PortalPurpose::History
    } else if input.action.as_str() == "approve" {
        PortalPurpose::NewWork
    } else {
        PortalPurpose::SettleExisting
    };
    ports
        .authorize_capability(&actor.company_id, purpose)
        .await?;
    if let Some(prior) = prior {
        ports
            .replay_proof(actor, key, id, input.action.as_str(), &prior)
            .await?;
        let expected = ports
            .snapshot(&[id.to_owned()], Some(&binding.ticket_id))
            .await?;
        ports
            .guard(
                actor,
                &[id.to_owned()],
                Some(&binding.ticket_id),
                purpose,
                &expected,
            )
            .await?;
        return Ok(prior);
    }
    let ticket = ports.ticket(actor, &binding.ticket_id).await?;
    let plan = crate::domain::ticket_portal::plan_decision(
        actor,
        &binding,
        &ticket,
        &input,
        key,
        ports.now(),
    )?;
    let PortalPlan::Decide { result, .. } = &plan else {
        return Err(BusinessError::invalid(
            "SUPPORT_PORTAL_INPUT_INVALID",
            "Invalid Support Portal plan.",
        ));
    };
    let result = to_value(result)?;
    ports
        .apply(actor, key, &operation, &plan, &wire, &result)
        .await?;
    let expected = ports
        .snapshot(&[id.to_owned()], Some(&binding.ticket_id))
        .await?;
    ports
        .guard(
            actor,
            &[id.to_owned()],
            Some(&binding.ticket_id),
            purpose,
            &expected,
        )
        .await?;
    Ok(result)
}

pub async fn respond<P: TicketPortalPorts>(
    ports: &mut P,
    actor: &PortalActor,
    id: &str,
    raw: &Value,
    key: &str,
) -> Result<Value, BusinessError> {
    validate_command_key(key)?;
    let input = crate::domain::ticket_portal::parse_response(raw)?;
    let wire = to_value(&input)?;
    ports.authorize_staff(actor, true).await?;
    let binding = ports.binding(actor, id).await?.ok_or_else(|| {
        BusinessError::new(
            erpx_server_shared::public::FailureKind::NotFound,
            "SUPPORT_PORTAL_NOT_FOUND",
            "Support customer access is unavailable.",
        )
    })?;
    let prior = ports
        .replay(actor, key, "support.ticket-portal.respond", id, &wire)
        .await?;
    let purpose = if prior.is_some() {
        PortalPurpose::History
    } else {
        PortalPurpose::SettleExisting
    };
    ports
        .authorize_capability(&actor.company_id, purpose)
        .await?;
    if let Some(prior) = prior {
        ports
            .replay_proof(actor, key, id, "respond", &prior)
            .await?;
        let expected = ports
            .snapshot(&[id.to_owned()], Some(&binding.ticket_id))
            .await?;
        ports
            .guard(
                actor,
                &[id.to_owned()],
                Some(&binding.ticket_id),
                purpose,
                &expected,
            )
            .await?;
        return Ok(prior);
    }
    let current = ports.current_response(id).await?;
    let plan = crate::domain::ticket_portal::plan_response(
        actor,
        &binding,
        crate::domain::ticket_portal::response_version(current.as_ref()),
        &input,
        key,
        ports.now(),
    )?;
    let PortalPlan::Respond { result, .. } = &plan else {
        return Err(BusinessError::invalid(
            "SUPPORT_PORTAL_INPUT_INVALID",
            "Invalid Support Portal plan.",
        ));
    };
    let result = to_value(result)?;
    ports
        .apply(
            actor,
            key,
            "support.ticket-portal.respond",
            &plan,
            &wire,
            &result,
        )
        .await?;
    let expected = ports
        .snapshot(&[id.to_owned()], Some(&binding.ticket_id))
        .await?;
    ports
        .guard(
            actor,
            &[id.to_owned()],
            Some(&binding.ticket_id),
            purpose,
            &expected,
        )
        .await?;
    Ok(result)
}

/// Verified customer acknowledgement. The caller has already established the
/// private customer session and purchase/release scope; the port reloads the
/// binding under those facts.
pub async fn acknowledge<P: TicketPortalPorts>(
    ports: &mut P,
    customer: &PortalCustomer,
    id: &str,
    raw: &Value,
    key: &str,
) -> Result<Value, BusinessError> {
    validate_command_key(key)?;
    let input: AcknowledgementInput = crate::domain::ticket_portal::parse_acknowledgement(raw)?;
    let wire = to_value(&input)?;
    let prior = ports.acknowledgement_prior(key).await?;
    if let Some(prior) = prior {
        if prior.binding_id != id || prior.user_id != customer.user_id {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This acknowledgement key belongs to another request.",
            ));
        }
        let expected_binding = ports.customer_binding(customer, id).await?;
        if prior.request_hash != expected_request_hash(customer, &expected_binding, &input) {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This acknowledgement key belongs to another request.",
            ));
        }
        ports
            .guard_acknowledgement(customer, id, &input.response_id)
            .await?;
        return to_value(&prior.result);
    }
    let binding = ports.customer_binding(customer, id).await?;
    let response = ports.current_response(id).await?.ok_or_else(|| {
        BusinessError::new(
            erpx_server_shared::public::FailureKind::NotFound,
            "SUPPORT_PORTAL_NOT_FOUND",
            "This verified customer ticket is unavailable.",
        )
    })?;
    let plan = crate::domain::ticket_portal::plan_acknowledgement(
        customer,
        &binding,
        &response,
        &input,
        key,
        ports.now(),
    )?;
    let PortalPlan::Acknowledge { result, .. } = &plan else {
        return Err(BusinessError::invalid(
            "SUPPORT_PORTAL_INPUT_INVALID",
            "Invalid Support Portal plan.",
        ));
    };
    let result = to_value(result)?;
    ports
        .apply_acknowledgement(customer, key, &plan, &wire, &result)
        .await?;
    ports
        .guard_acknowledgement(customer, id, &input.response_id)
        .await?;
    Ok(result)
}

fn expected_request_hash(
    customer: &PortalCustomer,
    binding: &PortalBindingRow,
    input: &AcknowledgementInput,
) -> String {
    crate::domain::ticket_portal::digest(&serde_json::json!({
        "actor": {
            "userId": customer.user_id,
            "email": customer.normalized_email(),
            "holdingId": customer.holding_id,
        },
        "bindingId": binding.id,
        "input": {
            "expectedBindingVersion": input.expected_binding_version,
            "responseId": input.response_id,
            "responseVersion": input.response_version,
            "note": input.note,
        },
    }))
}
