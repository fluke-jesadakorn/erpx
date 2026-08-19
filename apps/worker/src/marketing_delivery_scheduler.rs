//! Restart-safe control-plane scheduling around the holding-routed Marketing
//! delivery pass. The scheduler lease is separate from holding delivery
//! leases; a successful pass is rescheduled, while bounded failures retry and
//! eventually remain in a durable dead-letter state.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_infrastructure::{
    MarketingDeliverySchedulerClaim, MarketingDeliverySchedulerError,
    MarketingDeliverySchedulerFailureOutcome, MarketingDeliverySchedulerLease,
    SqlxMarketingDeliverySchedulerRepository,
};
use uuid::Uuid;

use super::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const MARKETING_DELIVERY_SCHEDULER_HANDLER_NAME: &str = "marketing_delivery";
const MARKETING_DELIVERY_SCHEDULER_FAILURE: &str = "marketing_delivery_scheduler_failure";

#[async_trait]
pub trait MarketingDeliverySchedulerSource: Send + Sync + 'static {
    async fn claim_due(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
        lease: Duration,
    ) -> Result<Option<MarketingDeliverySchedulerClaim>, MarketingDeliverySchedulerError>;

    async fn renew(
        &self,
        claim: &MarketingDeliverySchedulerClaim,
        now: DateTime<Utc>,
        lease: Duration,
    ) -> Result<MarketingDeliverySchedulerLease, MarketingDeliverySchedulerError>;

    async fn complete(
        &self,
        claim: &MarketingDeliverySchedulerClaim,
        now: DateTime<Utc>,
    ) -> Result<(), MarketingDeliverySchedulerError>;

    async fn fail(
        &self,
        claim: &MarketingDeliverySchedulerClaim,
        now: DateTime<Utc>,
        error: &str,
    ) -> Result<MarketingDeliverySchedulerFailureOutcome, MarketingDeliverySchedulerError>;
}

#[async_trait]
impl MarketingDeliverySchedulerSource for SqlxMarketingDeliverySchedulerRepository {
    async fn claim_due(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
        lease: Duration,
    ) -> Result<Option<MarketingDeliverySchedulerClaim>, MarketingDeliverySchedulerError> {
        SqlxMarketingDeliverySchedulerRepository::claim_due(self, worker_id, now, lease).await
    }

    async fn renew(
        &self,
        claim: &MarketingDeliverySchedulerClaim,
        now: DateTime<Utc>,
        lease: Duration,
    ) -> Result<MarketingDeliverySchedulerLease, MarketingDeliverySchedulerError> {
        SqlxMarketingDeliverySchedulerRepository::renew(self, claim, now, lease).await
    }

    async fn complete(
        &self,
        claim: &MarketingDeliverySchedulerClaim,
        now: DateTime<Utc>,
    ) -> Result<(), MarketingDeliverySchedulerError> {
        SqlxMarketingDeliverySchedulerRepository::complete(self, claim, now).await
    }

    async fn fail(
        &self,
        claim: &MarketingDeliverySchedulerClaim,
        now: DateTime<Utc>,
        error: &str,
    ) -> Result<MarketingDeliverySchedulerFailureOutcome, MarketingDeliverySchedulerError> {
        SqlxMarketingDeliverySchedulerRepository::fail(self, claim, now, error).await
    }
}

#[derive(Debug, Clone)]
pub struct MarketingDeliverySchedulerHandler<S, H> {
    source: S,
    inner: H,
    lease: Duration,
}

impl<S, H> MarketingDeliverySchedulerHandler<S, H>
where
    S: MarketingDeliverySchedulerSource,
    H: NamedWorkerHandler,
{
    pub fn new(source: S, inner: H, lease: Duration) -> Result<Self, WorkerRuntimeError> {
        if lease < Duration::seconds(1) || lease > Duration::seconds(3_600) {
            return Err(WorkerRuntimeError::Configuration(
                "marketing delivery scheduler lease is outside its bound".into(),
            ));
        }
        Ok(Self {
            source,
            inner,
            lease,
        })
    }
}

#[async_trait]
impl<S, H> NamedWorkerHandler for MarketingDeliverySchedulerHandler<S, H>
where
    S: MarketingDeliverySchedulerSource,
    H: NamedWorkerHandler,
{
    fn name(&self) -> &str {
        MARKETING_DELIVERY_SCHEDULER_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "marketing delivery scheduler worker ID is required".into(),
            ));
        }
        let claim = self
            .source
            .claim_due(worker_id, Utc::now(), self.lease)
            .await
            .map_err(redacted_scheduler_error)?;
        let Some(claim) = claim else {
            return Ok(WorkerHandlerReport::default());
        };
        match execute_with_lease_renewal(&self.source, &self.inner, &claim, self.lease).await {
            Ok(mut report) => {
                self.source
                    .complete(&claim, Utc::now())
                    .await
                    .map_err(redacted_scheduler_error)?;
                report.has_work = true;
                Ok(report)
            }
            Err(_) => {
                let outcome = self
                    .source
                    .fail(&claim, Utc::now(), MARKETING_DELIVERY_SCHEDULER_FAILURE)
                    .await
                    .map_err(redacted_scheduler_error)?;
                let mut report = WorkerHandlerReport {
                    claimed: 1,
                    processed: 1,
                    has_work: true,
                    ..WorkerHandlerReport::default()
                };
                match outcome {
                    MarketingDeliverySchedulerFailureOutcome::Retry { .. } => {
                        report.retried = 1;
                    }
                    MarketingDeliverySchedulerFailureOutcome::DeadLetter { .. } => {
                        report.dead_lettered = 1;
                    }
                }
                Ok(report)
            }
        }
    }
}

async fn execute_with_lease_renewal<S, H>(
    source: &S,
    inner: &H,
    claim: &MarketingDeliverySchedulerClaim,
    lease: Duration,
) -> Result<WorkerHandlerReport, WorkerRuntimeError>
where
    S: MarketingDeliverySchedulerSource,
    H: NamedWorkerHandler,
{
    let half_lease_millis = u64::try_from(lease.num_milliseconds() / 2)
        .unwrap_or(500)
        .max(1);
    let mut renewal = tokio::time::interval(std::time::Duration::from_millis(half_lease_millis));
    renewal.tick().await;
    let execution = inner.run_once(claim.lease.worker_id);
    tokio::pin!(execution);
    loop {
        tokio::select! {
            result = &mut execution => return result,
            _ = renewal.tick() => {
                source
                    .renew(claim, Utc::now(), lease)
                    .await
                    .map_err(|_| WorkerRuntimeError::Configuration(
                        "marketing delivery scheduler lease renewal failed".into(),
                    ))?;
            }
        }
    }
}

fn redacted_scheduler_error(_error: MarketingDeliverySchedulerError) -> WorkerRuntimeError {
    tracing::error!("Marketing delivery scheduler operation failed");
    WorkerRuntimeError::Configuration("Marketing delivery scheduler operation failed".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Default)]
    struct EmptySource;

    #[async_trait]
    impl MarketingDeliverySchedulerSource for EmptySource {
        async fn claim_due(
            &self,
            _worker_id: Uuid,
            _now: DateTime<Utc>,
            _lease: Duration,
        ) -> Result<Option<MarketingDeliverySchedulerClaim>, MarketingDeliverySchedulerError>
        {
            Ok(None)
        }

        async fn renew(
            &self,
            _claim: &MarketingDeliverySchedulerClaim,
            _now: DateTime<Utc>,
            _lease: Duration,
        ) -> Result<MarketingDeliverySchedulerLease, MarketingDeliverySchedulerError> {
            Err(MarketingDeliverySchedulerError::ClaimLost)
        }

        async fn complete(
            &self,
            _claim: &MarketingDeliverySchedulerClaim,
            _now: DateTime<Utc>,
        ) -> Result<(), MarketingDeliverySchedulerError> {
            Ok(())
        }

        async fn fail(
            &self,
            _claim: &MarketingDeliverySchedulerClaim,
            _now: DateTime<Utc>,
            _error: &str,
        ) -> Result<MarketingDeliverySchedulerFailureOutcome, MarketingDeliverySchedulerError>
        {
            Ok(MarketingDeliverySchedulerFailureOutcome::Retry {
                available_at: Utc::now(),
            })
        }
    }

    #[derive(Clone, Default)]
    struct EmptyHandler;

    #[async_trait]
    impl NamedWorkerHandler for EmptyHandler {
        fn name(&self) -> &str {
            "empty"
        }

        async fn run_once(
            &self,
            _worker_id: Uuid,
        ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
            Ok(WorkerHandlerReport::default())
        }
    }

    #[tokio::test]
    async fn scheduler_is_idle_without_a_due_control_plane_job() {
        let handler =
            MarketingDeliverySchedulerHandler::new(EmptySource, EmptyHandler, Duration::seconds(5))
                .expect("handler");
        let report = handler.run_once(Uuid::new_v4()).await.expect("run");
        assert_eq!(report, WorkerHandlerReport::default());
    }

    #[test]
    fn scheduler_rejects_zero_or_overlong_leases() {
        assert!(
            MarketingDeliverySchedulerHandler::new(EmptySource, EmptyHandler, Duration::zero())
                .is_err()
        );
        assert!(
            MarketingDeliverySchedulerHandler::new(
                EmptySource,
                EmptyHandler,
                Duration::seconds(3_601)
            )
            .is_err()
        );
    }
}
