//! Native Sales customer-notice reconciliation use case.
//!
//! The caller owns the retained serializable Holding transaction and current
//! reviewer authority. This module never opens, commits or retries a
//! transaction, never rewrites the immutable attempt and never resends mail.
use crate::domain::sales_notification_execution::NotificationSource;
use crate::domain::sales_notification_reconciliation::{
    OPERATION, ProviderObservation, ReconcileActor, ReconcileAttempt, ReconcileInput,
    ReconcileReceipt, authority_changed, idempotency_conflict, independent_reviewer_required,
    not_found, not_uncertain, provider_id_required, readback_invalid, request_invalid,
    source_changed, version_conflict,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use std::future::Future;

pub type ReconcileFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Persisted `command_receipts` identity used for exact replay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileReceiptSnapshot {
    pub request_hash: String,
    pub result_json: String,
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
}

/// One exact uncertain source/attempt settlement to persist.
#[derive(Clone, Debug)]
pub struct ReconcilePlan<'a> {
    pub actor: &'a ReconcileActor,
    pub key: &'a str,
    pub notification_id: &'a str,
    pub attempt: &'a ReconcileAttempt,
    pub source: &'a NotificationSource,
    pub input: &'a ReconcileInput,
    pub observation: &'a ProviderObservation,
    pub outcome: &'static str,
    pub version: i64,
    pub event_id: &'a str,
    pub receipt: &'a ReconcileReceipt,
    pub request_hash: &'a str,
    pub now: i64,
}

/// Owner persistence port. `commit` writes the immutable reconciliation,
/// receipt, audit and outbox rows, flushes deferred constraints, then rechecks
/// the current reviewer authority, settlement capability and exact readback.
pub trait NotificationReconciliationRepository: Send {
    type Error: From<BusinessError> + Send;

    fn lock_command<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ReconcileFuture<'a, (), Self::Error>;

    fn digest<'a>(&'a mut self, value: &'a str) -> ReconcileFuture<'a, String, Self::Error>;

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ReconcileFuture<'a, Option<ReconcileReceiptSnapshot>, Self::Error>;

    fn reconciliation_witness<'a>(
        &'a mut self,
        key: &'a str,
        notification_id: &'a str,
        attempt_id: &'a str,
        actor_id: &'a str,
        source_hash: &'a str,
    ) -> ReconcileFuture<'a, bool, Self::Error>;

    fn source<'a>(
        &'a mut self,
        notification_id: &'a str,
    ) -> ReconcileFuture<'a, Option<NotificationSource>, Self::Error>;

    fn attempt<'a>(
        &'a mut self,
        notification_id: &'a str,
        attempt_id: &'a str,
        company_id: &'a str,
    ) -> ReconcileFuture<'a, Option<ReconcileAttempt>, Self::Error>;

    fn settle_capability<'a>(
        &'a mut self,
        company_id: &'a str,
        history: bool,
        now: i64,
    ) -> ReconcileFuture<'a, (), Self::Error>;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a ReconcileActor,
        now: i64,
    ) -> ReconcileFuture<'a, bool, Self::Error>;

    fn commit<'a>(
        &'a mut self,
        plan: &'a ReconcilePlan<'a>,
        now: i64,
    ) -> ReconcileFuture<'a, (), Self::Error>;
}

/// Bounded read-only provider truth. Implementations never mutate, retry,
/// follow redirects or use an ambient proxy.
pub trait ProviderObservationPort: Send + Sync {
    fn observe<'a>(
        &'a self,
        provider_message_id: &'a str,
    ) -> ReconcileFuture<'a, ProviderObservation, BusinessError>;
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn replay_result(snapshot: &ReconcileReceiptSnapshot) -> Result<ReconcileReceipt, BusinessError> {
    let value: Value =
        serde_json::from_str(&snapshot.result_json).map_err(|_| readback_invalid())?;
    let object = value.as_object().ok_or_else(readback_invalid)?;
    let id = object.get("id").and_then(Value::as_str);
    let order_id = object.get("orderId").and_then(Value::as_str);
    let status = object.get("status").and_then(Value::as_str);
    let version = object.get("version").and_then(Value::as_i64);
    let reconciliation_id = object.get("reconciliationId").and_then(Value::as_str);
    match (id, order_id, status, version, reconciliation_id) {
        (Some(id), Some(order_id), Some(status), Some(version), Some(reconciliation_id))
            if !id.is_empty()
                && !order_id.is_empty()
                && matches!(status, "sent" | "cancelled")
                && version >= 2
                && !reconciliation_id.is_empty() =>
        {
            Ok(ReconcileReceipt {
                id: id.to_owned(),
                order_id: order_id.to_owned(),
                status: status.to_owned(),
                version,
                reconciliation_id: reconciliation_id.to_owned(),
            })
        }
        _ => Err(readback_invalid()),
    }
}

fn source_in_scope(source: &NotificationSource, actor: &ReconcileActor) -> bool {
    source.company_id == actor.company_id
        && source.source_company.as_deref() == Some(actor.company_id.as_str())
        && source.customer_company.as_deref() == Some(actor.company_id.as_str())
        && actor
            .branch_id
            .as_ref()
            .is_none_or(|branch| Some(branch) == source.branch_id.as_ref())
        && actor
            .department_id
            .as_ref()
            .is_none_or(|department| Some(department) == source.department_id.as_ref())
}

/// Settle one exact uncertain provider attempt under a different current Sales
/// approver. Replay returns the original immutable receipt without a provider
/// read.
pub async fn reconcile_customer_notification<R: NotificationReconciliationRepository>(
    actor: &ReconcileActor,
    notification_id: &str,
    raw_input: ReconcileInput,
    key: &str,
    repository: &mut R,
    provider: &dyn ProviderObservationPort,
    now: i64,
) -> Result<ReconcileReceipt, R::Error> {
    actor.require_approve()?;
    if key.is_empty() || key.len() > 200 || key.chars().any(char::is_control) {
        return Err(request_invalid().into());
    }
    let input = raw_input.validate()?;
    repository.lock_command(key).await?;
    let identity = input.identity(actor, notification_id, key);
    let request_hash = repository.digest(&identity).await?;
    if !valid_hash(&request_hash) {
        return Err(readback_invalid().into());
    }
    let prior = repository.receipt(key).await?;
    if let Some(snapshot) = prior.as_ref() {
        if snapshot.request_hash != request_hash
            || snapshot.operation != OPERATION
            || snapshot.record_id != notification_id
            || snapshot.actor_id != actor.user_id
        {
            return Err(idempotency_conflict().into());
        }
    }
    let source = repository
        .source(notification_id)
        .await?
        .ok_or_else(not_found)?;
    if !source_in_scope(&source, actor) {
        return Err(not_found().into());
    }
    let attempt = repository
        .attempt(notification_id, &input.attempt_id, &actor.company_id)
        .await?
        .ok_or_else(source_changed)?;
    if attempt.company_id != actor.company_id || attempt.notification_id != notification_id {
        return Err(not_found().into());
    }
    if attempt.source_hash != source.source_hash {
        return Err(source_changed().into());
    }
    if actor.same_person(&source.created_by) || actor.same_person(&attempt.actor_id) {
        return Err(independent_reviewer_required().into());
    }
    repository
        .settle_capability(&actor.company_id, prior.is_some(), now)
        .await?;
    if let Some(snapshot) = prior.as_ref() {
        if !repository
            .reconciliation_witness(
                key,
                notification_id,
                &attempt.id,
                &actor.user_id,
                &attempt.source_hash,
            )
            .await?
        {
            return Err(source_changed().into());
        }
        if !repository.current_authority(actor, now).await? {
            return Err(authority_changed().into());
        }
        return Ok(replay_result(snapshot)?);
    }
    if !source.release_ready {
        return Err(crate::domain::sales_notification_reconciliation::production_disabled().into());
    }
    if source.version != input.expected_version {
        return Err(version_conflict().into());
    }
    if source.status != "unknown" || attempt.state != "unknown" || source.attempts != attempt.attempt
    {
        return Err(not_uncertain().into());
    }
    let provider_message_id = attempt
        .provider_message_id
        .as_ref()
        .filter(|value| !value.is_empty())
        .ok_or_else(provider_id_required)?;
    if source.adapter != "resend" || source.provider_message_id.as_deref() != Some(provider_message_id)
    {
        return Err(provider_id_required().into());
    }
    let observation = provider
        .observe(provider_message_id)
        .await?
        .validate(&source, &attempt)?;
    let outcome = observation.last_event.outcome();
    let version = input.expected_version + 1;
    let event_id = format!("sales-notification-reconciliation-{key}");
    let receipt = ReconcileReceipt {
        id: notification_id.to_owned(),
        order_id: source.order_id.clone(),
        status: outcome.to_owned(),
        version,
        reconciliation_id: event_id.clone(),
    };
    let plan = ReconcilePlan {
        actor,
        key,
        notification_id,
        attempt: &attempt,
        source: &source,
        input: &input,
        observation: &observation,
        outcome,
        version,
        event_id: &event_id,
        receipt: &receipt,
        request_hash: &request_hash,
        now,
    };
    repository.commit(&plan, now).await?;
    Ok(receipt)
}
