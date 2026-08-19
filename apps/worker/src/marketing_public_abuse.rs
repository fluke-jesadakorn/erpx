//! Bounded control-plane cleanup for anonymous Marketing abuse windows.
//!
//! This handler owns only coarse counter cleanup. It never routes a holding
//! database, receives client identity, or persists request fingerprints.
//! Registration is intentionally left to the worker composition root.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_infrastructure::{MAX_MARKETING_PUBLIC_ABUSE_LIMIT, PersistenceError, SqlxControlPlane};
use thiserror::Error;
use uuid::Uuid;

use crate::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const MARKETING_PUBLIC_ABUSE_CLEANUP_HANDLER_NAME: &str = "marketing_public_abuse_cleanup";
pub const MARKETING_PUBLIC_ABUSE_CLEANUP_MAX_BATCH_SIZE: u32 = MAX_MARKETING_PUBLIC_ABUSE_LIMIT;
pub const MARKETING_PUBLIC_ABUSE_CLEANUP_DEFAULT_BATCH_SIZE: u32 =
    MARKETING_PUBLIC_ABUSE_CLEANUP_MAX_BATCH_SIZE;
pub const MARKETING_PUBLIC_ABUSE_CLEANUP_RETENTION_SECONDS: i64 = 86_400;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MarketingPublicAbuseCleanupError {
    #[error("marketing public abuse cleanup configuration error: {0}")]
    Configuration(String),
    #[error("marketing public abuse cleanup operation failed")]
    Operation,
}

impl From<MarketingPublicAbuseCleanupError> for WorkerRuntimeError {
    fn from(error: MarketingPublicAbuseCleanupError) -> Self {
        match error {
            MarketingPublicAbuseCleanupError::Configuration(message) => {
                WorkerRuntimeError::Configuration(message)
            }
            MarketingPublicAbuseCleanupError::Operation => WorkerRuntimeError::Configuration(
                "marketing public abuse cleanup operation failed".into(),
            ),
        }
    }
}

/// The only persistence operation this handler is allowed to perform.
#[async_trait]
pub trait MarketingPublicAbusePurgePort: Send + Sync + 'static {
    async fn purge_marketing_public_abuse_windows(
        &self,
        before: DateTime<Utc>,
        limit: u32,
    ) -> Result<u64, PersistenceError>;
}

#[async_trait]
impl MarketingPublicAbusePurgePort for SqlxControlPlane {
    async fn purge_marketing_public_abuse_windows(
        &self,
        before: DateTime<Utc>,
        limit: u32,
    ) -> Result<u64, PersistenceError> {
        SqlxControlPlane::purge_marketing_public_abuse_windows(self, before, limit).await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarketingPublicAbuseCleanupReport {
    pub deleted: u64,
}

#[derive(Debug, Clone)]
pub struct MarketingPublicAbuseCleanupRunner<P> {
    purge: P,
    batch_size: u32,
}

impl<P> MarketingPublicAbuseCleanupRunner<P>
where
    P: MarketingPublicAbusePurgePort,
{
    pub fn new(purge: P, batch_size: u32) -> Result<Self, MarketingPublicAbuseCleanupError> {
        if batch_size == 0 || batch_size > MARKETING_PUBLIC_ABUSE_CLEANUP_MAX_BATCH_SIZE {
            return Err(MarketingPublicAbuseCleanupError::Configuration(format!(
                "marketing public abuse cleanup batch size must be 1..={MARKETING_PUBLIC_ABUSE_CLEANUP_MAX_BATCH_SIZE}"
            )));
        }
        Ok(Self { purge, batch_size })
    }

    pub const fn batch_size(&self) -> u32 {
        self.batch_size
    }

    pub async fn run_once_at(
        &self,
        now: DateTime<Utc>,
    ) -> Result<MarketingPublicAbuseCleanupReport, MarketingPublicAbuseCleanupError> {
        let before = now
            .checked_sub_signed(Duration::seconds(
                MARKETING_PUBLIC_ABUSE_CLEANUP_RETENTION_SECONDS,
            ))
            .ok_or_else(|| {
                MarketingPublicAbuseCleanupError::Configuration(
                    "marketing public abuse cleanup timestamp is invalid".into(),
                )
            })?;
        let deleted = self
            .purge
            .purge_marketing_public_abuse_windows(before, self.batch_size)
            .await
            .map_err(redacted_purge_error)?;
        Ok(MarketingPublicAbuseCleanupReport { deleted })
    }
}

#[async_trait]
impl<P> NamedWorkerHandler for MarketingPublicAbuseCleanupRunner<P>
where
    P: MarketingPublicAbusePurgePort,
{
    fn name(&self) -> &str {
        MARKETING_PUBLIC_ABUSE_CLEANUP_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "marketing public abuse cleanup worker_id is invalid".into(),
            ));
        }
        let report = self.run_once_at(Utc::now()).await?;
        Ok(map_handler_report(report))
    }
}

fn map_handler_report(report: MarketingPublicAbuseCleanupReport) -> WorkerHandlerReport {
    WorkerHandlerReport {
        processed: report.deleted,
        has_work: report.deleted != 0,
        ..WorkerHandlerReport::default()
    }
}

fn redacted_purge_error(_error: PersistenceError) -> MarketingPublicAbuseCleanupError {
    tracing::error!("marketing public abuse cleanup persistence failed");
    MarketingPublicAbuseCleanupError::Operation
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    type PurgeCalls = Arc<Mutex<Vec<(DateTime<Utc>, u32)>>>;

    #[derive(Clone)]
    struct FakePurge {
        calls: PurgeCalls,
        result: Result<u64, ()>,
    }

    #[async_trait]
    impl MarketingPublicAbusePurgePort for FakePurge {
        async fn purge_marketing_public_abuse_windows(
            &self,
            before: DateTime<Utc>,
            limit: u32,
        ) -> Result<u64, PersistenceError> {
            self.calls
                .lock()
                .expect("purge calls mutex")
                .push((before, limit));
            match self.result {
                Ok(deleted) => Ok(deleted),
                Err(()) => Err(PersistenceError::InvalidData(
                    "postgres://secret/password".into(),
                )),
            }
        }
    }

    fn runner(
        result: Result<u64, ()>,
        batch_size: u32,
    ) -> MarketingPublicAbuseCleanupRunner<FakePurge> {
        MarketingPublicAbuseCleanupRunner::new(
            FakePurge {
                calls: Arc::new(Mutex::new(Vec::new())),
                result,
            },
            batch_size,
        )
        .expect("valid cleanup runner")
    }

    #[test]
    fn handler_name_and_default_batch_are_stable() {
        assert_eq!(
            MARKETING_PUBLIC_ABUSE_CLEANUP_HANDLER_NAME,
            "marketing_public_abuse_cleanup"
        );
        assert_eq!(
            MARKETING_PUBLIC_ABUSE_CLEANUP_DEFAULT_BATCH_SIZE,
            MARKETING_PUBLIC_ABUSE_CLEANUP_MAX_BATCH_SIZE
        );
        assert_eq!(
            MARKETING_PUBLIC_ABUSE_CLEANUP_MAX_BATCH_SIZE,
            MAX_MARKETING_PUBLIC_ABUSE_LIMIT
        );
    }

    #[test]
    fn constructor_rejects_unbounded_batches() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        assert!(
            MarketingPublicAbuseCleanupRunner::new(
                FakePurge {
                    calls: Arc::clone(&calls),
                    result: Ok(0),
                },
                0,
            )
            .is_err()
        );
        assert!(
            MarketingPublicAbuseCleanupRunner::new(
                FakePurge {
                    calls,
                    result: Ok(0),
                },
                MARKETING_PUBLIC_ABUSE_CLEANUP_MAX_BATCH_SIZE + 1,
            )
            .is_err()
        );
    }

    #[test]
    fn report_mapping_exposes_only_cleanup_count() {
        let report = map_handler_report(MarketingPublicAbuseCleanupReport { deleted: 7 });
        assert_eq!(report.processed, 7);
        assert!(report.has_work);
        assert_eq!(report.claimed, 0);
        assert_eq!(report.published, 0);
        assert_eq!(report.retried, 0);
        assert_eq!(report.dead_lettered, 0);
    }

    #[tokio::test]
    async fn runner_uses_retention_cutoff_and_bounded_batch() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = MarketingPublicAbuseCleanupRunner::new(
            FakePurge {
                calls: Arc::clone(&calls),
                result: Ok(3),
            },
            12,
        )
        .expect("valid cleanup runner");
        let now = DateTime::parse_from_rfc3339("2026-07-26T12:34:56Z")
            .expect("timestamp")
            .with_timezone(&Utc);

        let report = runner.run_once_at(now).await.expect("cleanup report");

        assert_eq!(report, MarketingPublicAbuseCleanupReport { deleted: 3 });
        assert_eq!(
            calls.lock().expect("purge calls mutex").as_slice(),
            &[(
                now - Duration::seconds(MARKETING_PUBLIC_ABUSE_CLEANUP_RETENTION_SECONDS),
                12,
            )]
        );
    }

    #[test]
    fn persistence_errors_are_redacted() {
        let error = redacted_purge_error(PersistenceError::InvalidData(
            "postgres://secret/password".into(),
        ));
        assert_eq!(error, MarketingPublicAbuseCleanupError::Operation);
        assert!(!error.to_string().contains("secret"));
        let runtime: WorkerRuntimeError = error.into();
        assert_eq!(
            runtime.to_string(),
            "worker configuration error: marketing public abuse cleanup operation failed"
        );
    }

    #[tokio::test]
    async fn handler_rejects_nil_worker_id() {
        let handler = runner(Ok(0), 1);
        let error = handler
            .run_once(Uuid::nil())
            .await
            .expect_err("nil worker id must fail closed");
        assert_eq!(
            error.to_string(),
            "worker configuration error: marketing public abuse cleanup worker_id is invalid"
        );
    }
}
