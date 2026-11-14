//! Native Sales customer-notice delivery use case.
//!
//! The worker owns three durable steps per notice:
//! 1. claim the queued notice and commit one immutable attempt before I/O;
//! 2. re-read the exact source/consent/authority, dispatch at most once and
//!    commit the terminal `sent` state with the provider identity;
//! 3. on any failure mark the same attempt `unknown` (provider may have
//!    accepted) or `failed` (provider was never invoked).
//!
//! A retried SQL callback never invokes the provider twice: the dispatch trace
//! is shared across transaction retries exactly like the retained TypeScript
//! closure. Reconciliation, not this worker, settles `unknown` attempts.
use crate::domain::sales_notification_execution::{
    ATTEMPT_UNCERTAIN, ExecutionAttempt, ExecutionAuthorization, NotificationSource,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub type ExecutionFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchPlan {
    /// The retained sandbox adapter confirms a deterministic provider identity.
    Sandbox,
    /// The reviewed live adapter owns the actual provider call.
    Resend,
}

#[derive(Clone, Copy, Debug)]
pub struct OutboundEmail<'a> {
    pub notification_id: &'a str,
    pub recipient_email: &'a str,
    pub subject: &'a str,
    pub body_text: &'a str,
    pub plan: DispatchPlan,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DispatchOutcome {
    /// The provider confirmed one original message identity.
    Accepted { provider_id: String },
    /// The response was lost or ambiguous; the attempt stays `unknown`.
    Uncertain,
}

/// Bounded provider port. Implementations never open a transaction, never
/// invent authority and never retry an ambiguous response.
pub trait EmailDispatchPort: Send + Sync {
    /// Refuse missing provider configuration before the attempt becomes
    /// ambiguous. A hard failure here is never converted into a send.
    fn prepare(&self, plan: DispatchPlan) -> Result<(), BusinessError>;

    fn dispatch<'a>(
        &'a self,
        email: OutboundEmail<'a>,
    ) -> ExecutionFuture<'a, DispatchOutcome, BusinessError>;
}

/// Shared across SQL transaction retries so at most one provider call happens.
#[derive(Debug, Default)]
pub struct DispatchTrace {
    invoked: AtomicBool,
    provider_id: Mutex<Option<String>>,
}

impl DispatchTrace {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn invoked(&self) -> bool {
        self.invoked.load(Ordering::SeqCst)
    }

    pub(crate) fn provider_id(&self) -> Option<String> {
        self.provider_id
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    /// Record that the provider will be called exactly once for this stage.
    pub(crate) fn invoke(&self) {
        self.invoked.store(true, Ordering::SeqCst);
    }

    pub(crate) fn provider(&self, value: Option<String>) {
        *self
            .provider_id
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = value;
    }
}

/// One committed claim, re-read from the durable rows.
#[derive(Clone, Debug)]
pub struct DeliveryStage {
    pub source: NotificationSource,
    /// `None` only for the retained development boundary where no purchased
    /// queued authorization exists; every enabled mode requires the exact row.
    pub authorization: Option<ExecutionAuthorization>,
    pub attempt: ExecutionAttempt,
    pub plan: DispatchPlan,
}

/// Owner persistence port. Each method owns its complete serializable unit; a
/// claim is committed before the provider is invoked.
pub trait NotificationExecutionUnit: Send + Sync {
    type Error: From<BusinessError> + Send + 'static;

    fn company_of<'a>(
        &'a self,
        notification_id: &'a str,
    ) -> ExecutionFuture<'a, Option<String>, Self::Error>;

    fn claim<'a>(
        &'a self,
        company_id: &'a str,
        notification_id: &'a str,
        now: i64,
    ) -> ExecutionFuture<'a, Option<DeliveryStage>, Self::Error>;

    fn deliver<'a>(
        &'a self,
        stage: &'a DeliveryStage,
        dispatch: Arc<dyn EmailDispatchPort>,
        trace: Arc<DispatchTrace>,
        now: i64,
    ) -> ExecutionFuture<'a, (), Self::Error>;

    fn fail<'a>(
        &'a self,
        stage: &'a DeliveryStage,
        invoked: bool,
        provider_id: Option<&'a str>,
        now: i64,
    ) -> ExecutionFuture<'a, (), Self::Error>;

    fn next_due<'a>(&'a self, now: i64) -> ExecutionFuture<'a, Option<String>, Self::Error>;
}

/// Deliver exactly one due notice. Returns its id when the provider message was
/// committed as `sent`; `None` means the notice was not claimable.
pub async fn deliver_notification<U: NotificationExecutionUnit>(
    unit: &U,
    dispatch: Arc<dyn EmailDispatchPort>,
    notification_id: &str,
    now: i64,
) -> Result<Option<String>, U::Error> {
    let Some(company_id) = unit.company_of(notification_id).await? else {
        return Ok(None);
    };
    let Some(stage) = unit.claim(&company_id, notification_id, now).await? else {
        return Ok(None);
    };
    let trace = Arc::new(DispatchTrace::new());
    match unit
        .deliver(&stage, dispatch, Arc::clone(&trace), now)
        .await
    {
        Ok(()) => Ok(Some(stage.source.id.clone())),
        Err(error) => {
            let provider_id = trace.provider_id();
            unit.fail(&stage, trace.invoked(), provider_id.as_deref(), now)
                .await?;
            Err(error)
        }
    }
}

/// One bounded drain. A failed notice keeps its durable attempt visible to
/// reconciliation and is never retried automatically inside the same sweep.
pub async fn drain_notifications<U: NotificationExecutionUnit>(
    unit: &U,
    dispatch: Arc<dyn EmailDispatchPort>,
    now: i64,
    limit: usize,
) -> Result<u32, U::Error> {
    let mut sent = 0_u32;
    for _ in 0..limit.clamp(0, 20) {
        let Some(notification_id) = unit.next_due(now).await? else {
            break;
        };
        // A failed notice keeps its durable attempt visible to reconciliation;
        // the sweep continues exactly like the retained bounded drain.
        match deliver_notification(unit, Arc::clone(&dispatch), &notification_id, now).await {
            Ok(Some(_)) => sent += 1,
            Ok(None) => {}
            Err(_) => {}
        }
    }
    Ok(sent)
}

/// Deterministic sandbox identity retained from the original adapter.
pub fn sandbox_provider_id(notification_id: &str) -> String {
    format!("sandbox-{notification_id}")
}

/// Ambiguous provider responses are never converted into a retry.
pub fn uncertain_attempt() -> BusinessError {
    erpx_server_shared::public::BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        ATTEMPT_UNCERTAIN,
        "Reconcile the original provider response before another delivery.",
    )
}
