//! AI gateway policy, statement, cost and reconciliation commands. The port
//! owns SQL and the caller owns the transaction; this layer validates facts and
//! maps exact retained failure codes.
use crate::domain::gateway::{
    AiExistingCost, AiGatewayPolicyView, AiGatewayRow, AiPolicyChange, AiPolicyFacts,
    AiProviderStatement, AiReconciliationFact, AiSetEnabledInput, AiStatementEvidence,
    AiVerifiedCost, billing_month, cost_evidence_conflict, cost_evidence_invalid, currency_valid,
    digest_valid, month_bounds, policy_changed, policy_invalid, policy_missing, policy_unchanged,
    positive_integer, reason_required, statement_conflict, statement_invalid, statement_unbalanced,
};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use std::future::Future;

/// The outcome of the atomic compare-and-set on the singleton policy row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiSetEnabledOutcome {
    Updated,
    Missing,
    Changed,
    Unchanged,
}

pub trait AiGatewayPorts {
    type Error: From<BusinessError> + Send;

    fn policy(
        &mut self,
        start: i64,
        end: i64,
    ) -> impl Future<Output = Result<Option<AiPolicyFacts>, Self::Error>> + Send;

    fn set_enabled(
        &mut self,
        input: &AiSetEnabledInput,
        actor_id: &str,
        audit_id: &str,
        now: i64,
    ) -> impl Future<Output = Result<AiSetEnabledOutcome, Self::Error>> + Send;

    fn update_policy(
        &mut self,
        input: &AiPolicyChange,
        actor_id: &str,
        audit_id: &str,
        now: i64,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    fn insert_statement(
        &mut self,
        input: &AiProviderStatement,
        now: i64,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    fn statement_evidence(
        &mut self,
        id: &str,
    ) -> impl Future<Output = Result<Option<AiStatementEvidence>, Self::Error>> + Send;

    fn insert_cost_evidence(
        &mut self,
        input: &AiVerifiedCost,
        now: i64,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    fn existing_cost_evidence(
        &mut self,
        request_id: &str,
    ) -> impl Future<Output = Result<Option<AiExistingCost>, Self::Error>> + Send;

    fn reconcile(
        &mut self,
        statement_id: &str,
        now: i64,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    fn existing_reconciliation(
        &mut self,
        statement_id: &str,
    ) -> impl Future<Output = Result<Option<AiReconciliationFact>, Self::Error>> + Send;

    fn safe_rows(
        &mut self,
        start: i64,
        end: i64,
    ) -> impl Future<Output = Result<Vec<AiGatewayRow>, Self::Error>> + Send;
}

fn ok() -> Value {
    json!({"ok": true})
}

fn fields_required(value: &str) -> bool {
    !value.trim().is_empty()
}

pub async fn read_ai_gateway_policy<P: AiGatewayPorts>(
    ports: &mut P,
    now: i64,
) -> Result<AiGatewayPolicyView, P::Error> {
    let (start, end) = month_bounds(&billing_month(now)).map_err(Into::into)?;
    let facts = ports.policy(start, end).await?.ok_or_else(policy_missing)?;
    Ok(facts.into())
}

pub async fn set_ai_platform_key_enabled<P: AiGatewayPorts>(
    ports: &mut P,
    now: i64,
    audit_id: &str,
    input: &AiSetEnabledInput,
    actor_id: &str,
) -> Result<AiGatewayPolicyView, P::Error> {
    if !fields_required(actor_id) || !fields_required(&input.reason) {
        return Err(reason_required().into());
    }
    positive_integer(input.expected_updated_at)?;
    match ports.set_enabled(input, actor_id, audit_id, now).await? {
        AiSetEnabledOutcome::Updated => read_ai_gateway_policy(ports, now).await,
        AiSetEnabledOutcome::Missing => Err(policy_missing().into()),
        AiSetEnabledOutcome::Changed => Err(policy_changed().into()),
        AiSetEnabledOutcome::Unchanged => Err(policy_unchanged().into()),
    }
}

pub async fn update_ai_gateway_policy<P: AiGatewayPorts>(
    ports: &mut P,
    now: i64,
    audit_id: &str,
    input: &AiPolicyChange,
    actor_id: &str,
) -> Result<AiGatewayPolicyView, P::Error> {
    for value in [
        input.monthly_request_limit,
        input.monthly_spend_cap_satang,
        input.reservation_satang,
    ] {
        positive_integer(value)?;
        if value == 0 {
            return Err(policy_invalid().into());
        }
    }
    if !fields_required(actor_id) || !fields_required(&input.reason) {
        return Err(reason_required().into());
    }
    if !ports.update_policy(input, actor_id, audit_id, now).await? {
        return Err(policy_missing().into());
    }
    read_ai_gateway_policy(ports, now).await
}

pub async fn record_ai_provider_statement<P: AiGatewayPorts>(
    ports: &mut P,
    now: i64,
    input: &AiProviderStatement,
) -> Result<Value, P::Error> {
    positive_integer(input.amount_minor)?;
    let _ = month_bounds(&input.month).map_err(|_| statement_invalid())?;
    if !fields_required(&input.id)
        || !fields_required(&input.holding_id)
        || !fields_required(&input.provider)
        || !currency_valid(&input.currency)
        || !digest_valid(&input.statement_digest)
        || !fields_required(&input.verified_by)
    {
        return Err(statement_invalid().into());
    }
    if ports.insert_statement(input, now).await? {
        return Ok(ok());
    }
    let existing = ports
        .statement_evidence(&input.id)
        .await?
        .ok_or_else(statement_conflict)?;
    if existing.holding_id == input.holding_id
        && existing.provider == input.provider
        && existing.month == input.month
        && existing.currency == input.currency
        && existing.amount_minor == input.amount_minor
        && existing.statement_digest == input.statement_digest
    {
        Ok(ok())
    } else {
        Err(statement_conflict().into())
    }
}

pub async fn record_verified_ai_cost<P: AiGatewayPorts>(
    ports: &mut P,
    now: i64,
    input: &AiVerifiedCost,
) -> Result<Value, P::Error> {
    positive_integer(input.amount_minor)?;
    positive_integer(input.thb_satang)?;
    let fx_required = input
        .fx_evidence_ref
        .as_deref()
        .is_none_or(|value| value.trim().is_empty());
    if !currency_valid(&input.currency)
        || !fields_required(&input.provider_statement_id)
        || !fields_required(&input.verified_by)
        || (input.currency != "THB" && fx_required)
    {
        return Err(cost_evidence_invalid().into());
    }
    if ports.insert_cost_evidence(input, now).await? {
        return Ok(ok());
    }
    let existing = ports
        .existing_cost_evidence(&input.request_id)
        .await?
        .ok_or_else(cost_evidence_conflict)?;
    if existing.provider_statement_id == input.provider_statement_id
        && existing.currency == input.currency
        && existing.amount_minor == input.amount_minor
        && existing.thb_satang == input.thb_satang
        && existing.fx_evidence_ref == input.fx_evidence_ref
    {
        Ok(ok())
    } else {
        Err(cost_evidence_conflict().into())
    }
}

pub async fn reconcile_ai_provider_statement<P: AiGatewayPorts>(
    ports: &mut P,
    now: i64,
    statement_id: &str,
) -> Result<Value, P::Error> {
    if !ports.reconcile(statement_id, now).await? {
        let existing = ports.existing_reconciliation(statement_id).await?;
        if !existing.and_then(|fact| fact.reconciled_at).is_some() {
            return Err(statement_unbalanced().into());
        }
    }
    Ok(ok())
}

pub async fn safe_ai_gateway_rows<P: AiGatewayPorts>(
    ports: &mut P,
    month: &str,
) -> Result<Vec<AiGatewayRow>, P::Error> {
    let (start, end) = month_bounds(month).map_err(Into::into)?;
    Ok(ports.safe_rows(start, end).await?)
}
