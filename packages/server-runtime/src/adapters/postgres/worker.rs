//! Generic durable outbox worker supervisor.
//!
//! Composition supplies one explicit static owner predicate and one handler per
//! selected Holding. The supervisor claims, heartbeats, completes and fails only
//! the exact leased event. It never discovers targets from request input and
//! never opens a business transaction on a handler's behalf.
use crate::adapters::postgres::PersistenceError;
use crate::adapters::postgres::jobs::{claim, complete, fail, renew};
use crate::adapters::postgres::unit_of_work::PgTransactionError;
use crate::domain::jobs::OutboxJob;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgPool;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

/// The exact durable result of one handled event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerOutcome {
    Complete,
    /// Record a failed attempt and continue with the next target.
    Fail(&'static str),
}

pub type WorkerFuture<'a> =
    Pin<Box<dyn Future<Output = Result<WorkerOutcome, BusinessError>> + Send + 'a>>;

/// One handler for one selected Holding partition.
pub trait OutboxJobHandler: Send + Sync {
    fn handle<'a>(&'a self, write: &'a PgPool, job: &'a OutboxJob, now: i64) -> WorkerFuture<'a>;
}

/// One explicit Holding target. `owned` is a static SQL predicate; request,
/// user and provider input must never be interpolated into it.
pub struct WorkerTarget {
    pub name: String,
    pub write: PgPool,
    pub owned: String,
    pub handler: Arc<dyn OutboxJobHandler>,
}

/// Durable multi-Holding supervisor. A handler failure is recorded as a failed
/// attempt and returned to the caller so the process supervisor can restart.
pub struct WorkerSupervisor {
    targets: Vec<WorkerTarget>,
    poll_interval: Duration,
    lease_refresh: Duration,
    token_counter: AtomicU64,
}

impl WorkerSupervisor {
    pub fn new(targets: Vec<WorkerTarget>, poll_interval: Duration) -> Result<Self, &'static str> {
        if targets.is_empty() {
            return Err("WORKER_TARGET_REQUIRED");
        }
        if poll_interval.is_zero() {
            return Err("WORKER_POLL_INTERVAL_INVALID");
        }
        for target in &targets {
            if target.name.trim().is_empty()
                || target.owned.trim().is_empty()
                || target.owned.len() > 16 * 1024
            {
                return Err("WORKER_TARGET_INVALID");
            }
        }
        Ok(Self {
            targets,
            poll_interval,
            lease_refresh: Duration::from_secs(30),
            token_counter: AtomicU64::new(0),
        })
    }

    pub fn with_lease_refresh(mut self, interval: Duration) -> Result<Self, &'static str> {
        if interval.is_zero()
            || interval >= Duration::from_millis(crate::domain::jobs::DEFAULT_LEASE_MILLIS as u64)
        {
            return Err("WORKER_LEASE_REFRESH_INVALID");
        }
        self.lease_refresh = interval;
        Ok(self)
    }

    fn token(&self, target: &str, now: i64) -> String {
        let sequence = self.token_counter.fetch_add(1, Ordering::Relaxed);
        format!("{target}:{now}:{}:{sequence}", std::process::id())
    }

    /// Run one pass across every configured target and return the number of
    /// completed events. A failed handler is recorded and returned.
    pub async fn run_once(&self, now: i64) -> Result<usize, PgTransactionError<BusinessError>> {
        let mut completed = 0usize;
        for target in &self.targets {
            let token = self.token(target.name.as_str(), now);
            let mut connection = target.write.acquire().await?;
            let Some(job) = claim(&mut connection, &target.owned, &token, now).await? else {
                continue;
            };
            drop(connection);

            let started = std::time::Instant::now();
            let work = target.handler.handle(&target.write, &job, now);
            tokio::pin!(work);
            let mut refresh = tokio::time::interval(self.lease_refresh);
            refresh.tick().await;
            let result = loop {
                tokio::select! {
                    result = &mut work => break result,
                    _ = refresh.tick() => {
                        let clock = now.saturating_add(elapsed_millis(started));
                        let mut connection = target.write.acquire().await?;
                        if !renew(&mut connection, &job.id, &token, clock).await? {
                            return Err(lease_lost().into());
                        }
                    }
                }
            };
            let clock = now.saturating_add(elapsed_millis(started));
            let mut connection = target.write.acquire().await?;
            match result {
                Ok(WorkerOutcome::Complete) => {
                    if !complete(&mut connection, &job.id, &token, clock).await? {
                        return Err(lease_lost().into());
                    }
                    completed = completed.saturating_add(1);
                }
                Ok(WorkerOutcome::Fail(code)) => {
                    if !fail(&mut connection, &job.id, &token, job.attempts, clock, code).await? {
                        return Err(lease_lost().into());
                    }
                }
                Err(error) => {
                    if !fail(
                        &mut connection,
                        &job.id,
                        &token,
                        job.attempts,
                        clock,
                        error.code,
                    )
                    .await?
                    {
                        return Err(lease_lost().into());
                    }
                    return Err(PgTransactionError::Business(error));
                }
            }
        }
        Ok(completed)
    }

    /// Drain until shutdown. Any target failure is returned so the process
    /// supervisor can restart the worker with the same explicit configuration.
    pub async fn run<F>(
        self,
        now: Arc<dyn Fn() -> i64 + Send + Sync>,
        shutdown: F,
    ) -> Result<(), PgTransactionError<BusinessError>>
    where
        F: Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                _ = &mut shutdown => return Ok(()),
                result = self.run_once(now()) => {
                    result?;
                }
            }
            tokio::select! {
                _ = &mut shutdown => return Ok(()),
                _ = tokio::time::sleep(self.poll_interval) => {}
            }
        }
    }
}

fn elapsed_millis(started: std::time::Instant) -> i64 {
    i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX)
}

fn lease_lost() -> PersistenceError {
    PersistenceError::new("WORKER_LEASE_LOST")
}

/// A stable error used when a handler must fail without leaking provider text.
pub fn worker_failure(code: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        code,
        "The durable worker effect is unavailable.",
    )
}
