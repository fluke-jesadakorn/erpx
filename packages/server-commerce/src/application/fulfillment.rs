//! Native service-completion use case.
//!
//! The caller owns the serializable Holding transaction. This slice writes the
//! Commerce fulfillment header/line, increments the service order line, records
//! the resulting order state and retains receipt/audit/outbox. Goods delivery,
//! Inventory movement and the goods journal remain outside this slice.
use crate::application::order::{
    AuditEntry, CommerceOrderCommandRepository, CommerceOrderFuture, OutboxEntry, ReceiptEntry,
};
use crate::domain::fulfillment::{
    ServiceCompletionCommand, ServiceCompletionPlan, ServiceCompletionReceipt, ServiceLineFact,
    build_service_completion,
};
use crate::domain::order::{
    CommerceOrderActor, idempotency_conflict, not_found, readback_invalid, request_invalid,
    validate_hash, version_conflict,
};
use erpx_server_shared::public::{BusinessError, TimestampMillis};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceCompletionOutcome {
    pub receipt: ServiceCompletionReceipt,
    /// False when the command replayed an existing receipt; Finance evidence
    /// was already written by the winning invocation.
    pub created: bool,
}

pub trait CommerceFulfillmentRepository: CommerceOrderCommandRepository {
    fn service_line<'a>(
        &'a mut self,
        company: &'a str,
        order_id: &'a str,
        line_id: &'a str,
    ) -> CommerceOrderFuture<'a, Option<ServiceLineFact>, Self::Error>;

    fn insert_service_fulfillment<'a>(
        &'a mut self,
        plan: &'a ServiceCompletionPlan,
    ) -> CommerceOrderFuture<'a, bool, Self::Error>;

    fn insert_service_fulfillment_line<'a>(
        &'a mut self,
        plan: &'a ServiceCompletionPlan,
        line: &'a crate::domain::fulfillment::ServiceCompletionLinePlan,
    ) -> CommerceOrderFuture<'a, (), Self::Error>;

    fn increment_service_line<'a>(
        &'a mut self,
        line_id: &'a str,
        quantity_milli: i64,
    ) -> CommerceOrderFuture<'a, bool, Self::Error>;

    fn refresh_fulfillment_state<'a>(
        &'a mut self,
        plan: &'a ServiceCompletionPlan,
    ) -> CommerceOrderFuture<'a, Option<String>, Self::Error>;

    fn fulfillment_readback<'a>(
        &'a mut self,
        plan: &'a ServiceCompletionPlan,
    ) -> CommerceOrderFuture<'a, Option<(String, String, i64)>, Self::Error>;

    fn fulfillment_money_lines<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
    ) -> CommerceOrderFuture<'a, Vec<(String, i64, i64)>, Self::Error>;

    fn order_currency<'a>(
        &'a mut self,
        order_id: &'a str,
    ) -> CommerceOrderFuture<'a, Option<String>, Self::Error>;
}

fn parse_receipt(value: &str) -> Result<ServiceCompletionReceipt, BusinessError> {
    let parsed: serde_json::Value =
        serde_json::from_str(value).map_err(|_| crate::domain::order::source_invalid())?;
    let Some(object) = parsed.as_object() else {
        return Err(crate::domain::order::source_invalid());
    };
    if object.len() != 4
        || !object.contains_key("id")
        || !object.contains_key("fulfillmentId")
        || !object.contains_key("version")
        || !object.contains_key("state")
    {
        return Err(crate::domain::order::source_invalid());
    }
    let id = object
        .get("id")
        .and_then(|value| value.as_str())
        .ok_or_else(crate::domain::order::source_invalid)?;
    let fulfillment_id = object
        .get("fulfillmentId")
        .and_then(|value| value.as_str())
        .ok_or_else(crate::domain::order::source_invalid)?;
    let version = object
        .get("version")
        .and_then(|value| value.as_i64())
        .filter(|value| (1..=9_007_199_254_740_991).contains(value))
        .ok_or_else(crate::domain::order::source_invalid)?;
    let state = object
        .get("state")
        .and_then(|value| value.as_str())
        .filter(|value| matches!(*value, "partially_fulfilled" | "fulfilled"))
        .ok_or_else(crate::domain::order::source_invalid)?;
    Ok(ServiceCompletionReceipt {
        id: id.to_owned(),
        fulfillment_id: fulfillment_id.to_owned(),
        version,
        state: state.to_owned(),
    })
}

/// Record completion of service order lines after the current IAM write check,
/// exact order version/state, remaining quantity and invoice-date guards.
pub async fn fulfill_service_order<R: CommerceFulfillmentRepository>(
    actor: &CommerceOrderActor,
    command: &ServiceCompletionCommand,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<ServiceCompletionOutcome, R::Error> {
    actor.require_write(now)?;
    command.validate()?;
    repository.lock_command(command.key.as_str()).await?;
    let identity = crate::domain::fulfillment::service_completion_identity(actor, command);
    let request_hash = repository.digest(&identity).await?;
    validate_hash(&request_hash)?;
    if let Some(prior) = repository.receipt(command.key.as_str()).await? {
        if prior.request_hash != request_hash {
            return Err(idempotency_conflict().into());
        }
        let receipt = parse_receipt(&prior.result_json)?;
        if prior.operation.as_deref() != Some("commerce.order.fulfill")
            || prior.record_id.as_deref() != Some(command.order_id.as_str())
            || receipt.id != command.order_id.as_str()
            || receipt.fulfillment_id != format!("delivery-{}", command.key.as_str())
            || receipt.version != command.expected_version + 1
        {
            return Err(crate::domain::order::source_invalid().into());
        }
        return Ok(ServiceCompletionOutcome {
            receipt,
            created: false,
        });
    }
    if !repository
        .current_authority(actor, true, false, now)
        .await?
    {
        return Err(crate::domain::order::permission_required().into());
    }
    let order = repository
        .lock_order(actor, command.order_id.as_str())
        .await?
        .ok_or_else(not_found)?;
    let mut facts = Vec::with_capacity(command.lines.len());
    for line in &command.lines {
        facts.push(
            repository
                .service_line(&order.company_id, command.order_id.as_str(), &line.line_id)
                .await?,
        );
    }
    let plan = build_service_completion(actor, command, &order, &facts, now)?;
    if !repository.insert_service_fulfillment(&plan).await? {
        return Err(version_conflict().into());
    }
    for line in &plan.lines {
        repository
            .insert_service_fulfillment_line(&plan, line)
            .await?;
        if !repository
            .increment_service_line(&line.order_line_id, line.quantity_milli)
            .await?
        {
            return Err(version_conflict().into());
        }
    }
    let state = repository
        .refresh_fulfillment_state(&plan)
        .await?
        .ok_or_else(readback_invalid)?;
    let receipt = ServiceCompletionReceipt {
        id: plan.order_id.clone(),
        fulfillment_id: plan.id.clone(),
        version: plan.expected_version + 1,
        state: state.clone(),
    };
    let result_json = serde_json::to_string(&receipt.to_wire()).map_err(|_| request_invalid())?;
    if !repository
        .insert_receipt(
            &ReceiptEntry {
                key: command.key.as_str(),
                operation: "commerce.order.fulfill",
                actor,
                request_hash: &request_hash,
                record_id: &plan.order_id,
                result_json: &result_json,
            },
            now,
        )
        .await?
    {
        return Err(idempotency_conflict().into());
    }
    let authority = actor.authority_json();
    repository
        .audit(
            actor,
            &AuditEntry {
                key: command.key.as_str(),
                operation: "commerce.order.fulfill",
                record_type: "commerce_fulfillment",
                record_id: &plan.id,
                version: plan.expected_version + 1,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    repository
        .outbox(
            &OutboxEntry {
                key: command.key.as_str(),
                operation: "commerce.order.fulfill",
                record_type: "commerce_fulfillment",
                record_id: &plan.id,
                version: plan.expected_version + 1,
                state: &state,
                sales_id: None,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    let Some((readback_state, readback_fulfillment, readback_lines)) =
        repository.fulfillment_readback(&plan).await?
    else {
        return Err(readback_invalid().into());
    };
    if readback_state != state
        || readback_fulfillment != plan.id
        || readback_lines != plan.lines.len() as i64
    {
        return Err(readback_invalid().into());
    }
    Ok(ServiceCompletionOutcome {
        receipt,
        created: true,
    })
}
