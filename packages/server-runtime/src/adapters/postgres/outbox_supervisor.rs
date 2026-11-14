//! General native outbox supervisor for every registered Holding.
//!
//! This replaces the retained TypeScript `runOne` loop without owning any
//! business handler. Composition supplies one already-attested writable pool,
//! one reviewed static owner predicate and one handler per Holding. The loop is
//! strictly sequential: exactly one handler is in flight at a time (the single
//! heavy slot), every claim is `LIMIT 1`/`FOR UPDATE SKIP LOCKED`, and one
//! Holding's failure never prevents the remaining Holdings from being drained
//! in the same pass.
//!
//! Retained semantics are preserved exactly:
//!
//! * claim: `attempts = attempts + 1`, `claim_until = now + 90s`, ready events
//!   only, expired-or-absent leases only, skip locked, terminal rows excluded;
//! * renew: same 90s lease every 15s while the handler still owns the token;
//! * complete: `processed_at = published_at = now`, clear lease/error, exact
//!   token; an already terminal event is idempotent success;
//! * fail: `next_attempt_at = now + min(60s, 1s * 2^attempts)`,
//!   `last_error = UTF-16 slice(0, 64)`, dead-letter on the sixth attempt;
//! * idle cadence 1s, busy cadence 10ms, graceful shutdown, restart logging.
use crate::adapters::postgres::PersistenceError;
use crate::adapters::postgres::attestation::{
    WorkerAttestationRequest, WorkerDatabaseAttestation, attest_worker_database,
};
use crate::adapters::postgres::jobs::{LeaseCompletion, claim, complete_lease, fail, renew};
use crate::adapters::postgres::migrations::{MigrationFile, MigrationTrack};
use crate::adapters::postgres::pool::DatabasePool;
use crate::adapters::postgres::worker::{OutboxJobHandler, WorkerOutcome};
use crate::domain::jobs::{
    DEFAULT_BUSY_POLL_MILLIS, DEFAULT_IDLE_POLL_MILLIS, DEFAULT_LEASE_MILLIS,
    DEFAULT_LEASE_REFRESH_MILLIS, DEFAULT_MAX_EVENTS_PER_PASS, DEFAULT_RESTART_BACKOFF_MILLIS,
    MAX_ATTEMPTS, MAX_EVENTS_PER_PASS_LIMIT, OutboxJob, truncate_error_code,
};
use erpx_server_shared::public::{BusinessError, Clock};
use sqlx::PgPool;
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

/// A stable, sanitized supervisor failure. It never carries SQL, parameters or
/// provider text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboxSupervisorError {
    pub code: &'static str,
}

impl OutboxSupervisorError {
    pub fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl std::fmt::Display for OutboxSupervisorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code)
    }
}
impl std::error::Error for OutboxSupervisorError {}
impl From<PersistenceError> for OutboxSupervisorError {
    fn from(error: PersistenceError) -> Self {
        Self::new(error.code)
    }
}
impl From<sqlx::Error> for OutboxSupervisorError {
    fn from(error: sqlx::Error) -> Self {
        Self::new(PersistenceError::from(error).code)
    }
}

/// Durable attestation proof retained by the supervisor for each Holding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboxAttestation {
    pub holding_id: String,
    pub database: String,
    pub role: String,
    pub primary: bool,
    pub migration_count: usize,
    pub migration_sha256: String,
}

impl OutboxAttestation {
    pub fn new(
        holding_id: &str,
        database: &str,
        role: &str,
        primary: bool,
        migration_count: usize,
        migration_sha256: &str,
    ) -> Result<Self, OutboxSupervisorError> {
        if holding_id.trim().is_empty()
            || holding_id.len() > 128
            || database.trim().is_empty()
            || database.len() > 128
            || role.trim().is_empty()
            || role.len() > 128
            || migration_count == 0
            || migration_sha256.len() != 64
            || !migration_sha256
                .bytes()
                .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
        {
            return Err(OutboxSupervisorError::new("OUTBOX_ATTESTATION_INVALID"));
        }
        Ok(Self {
            holding_id: holding_id.to_owned(),
            database: database.to_owned(),
            role: role.to_owned(),
            primary,
            migration_count,
            migration_sha256: migration_sha256.to_owned(),
        })
    }

    pub fn from_worker(
        holding_id: &str,
        attestation: WorkerDatabaseAttestation,
    ) -> Result<Self, OutboxSupervisorError> {
        Self::new(
            holding_id,
            &attestation.database,
            &attestation.role,
            attestation.primary,
            attestation.migration_count,
            &attestation.migration_sha256,
        )
    }

    fn validate(&self, holding_id: &str) -> Result<(), OutboxSupervisorError> {
        let normalized = Self::new(
            &self.holding_id,
            &self.database,
            &self.role,
            self.primary,
            self.migration_count,
            &self.migration_sha256,
        )?;
        if normalized.holding_id != holding_id || !self.primary {
            return Err(OutboxSupervisorError::new("OUTBOX_ATTESTATION_INVALID"));
        }
        Ok(())
    }
}

/// Attest one Holding's writable database before its pool is admitted to the
/// supervisor. The reviewed migration inventory is read-only; this never
/// migrates or repairs a schema.
pub async fn attest_outbox_holding(
    pool: &DatabasePool,
    holding_id: &str,
    expected_database: Option<&str>,
    track: MigrationTrack,
    files: &[MigrationFile],
) -> Result<OutboxAttestation, OutboxSupervisorError> {
    let attestation = attest_worker_database(
        pool,
        WorkerAttestationRequest {
            expected_database,
            track,
            files,
        },
    )
    .await?;
    OutboxAttestation::from_worker(holding_id, attestation)
}

/// One registered Holding partition. `owned` is a static SQL predicate; request,
/// user and provider input must never be interpolated into it.
pub struct OutboxHolding {
    pub holding_id: String,
    pub write: PgPool,
    pub owned: String,
    pub attestation: OutboxAttestation,
}

impl std::fmt::Debug for OutboxHolding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutboxHolding")
            .field("holding_id", &self.holding_id)
            .field("owned_bytes", &self.owned.len())
            .field("attestation", &self.attestation)
            .finish_non_exhaustive()
    }
}

impl OutboxHolding {
    pub fn new(
        holding_id: &str,
        write: PgPool,
        owned: &str,
        attestation: OutboxAttestation,
    ) -> Result<Self, OutboxSupervisorError> {
        if holding_id.trim().is_empty()
            || holding_id.len() > 128
            || owned.trim().is_empty()
            || owned.len() > 16 * 1024
        {
            return Err(OutboxSupervisorError::new("OUTBOX_HOLDING_INVALID"));
        }
        attestation.validate(holding_id)?;
        Ok(Self {
            holding_id: holding_id.to_owned(),
            write,
            owned: owned.to_owned(),
            attestation,
        })
    }
}

/// Explicit handler registry. Every registered Holding must have exactly one
/// already-native outbox effect; the supervisor never invents a business
/// handler or falls back to a default.
#[derive(Default)]
pub struct OutboxHandlerRegistry {
    handlers: BTreeMap<String, Arc<dyn OutboxJobHandler>>,
}

impl OutboxHandlerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        holding_id: &str,
        handler: Arc<dyn OutboxJobHandler>,
    ) -> Result<(), OutboxSupervisorError> {
        if holding_id.trim().is_empty() || holding_id.len() > 128 {
            return Err(OutboxSupervisorError::new("OUTBOX_HOLDING_INVALID"));
        }
        if self.handlers.contains_key(holding_id) {
            return Err(OutboxSupervisorError::new("OUTBOX_HANDLER_DUPLICATE"));
        }
        self.handlers.insert(holding_id.to_owned(), handler);
        Ok(())
    }

    pub fn handler(&self, holding_id: &str) -> Option<Arc<dyn OutboxJobHandler>> {
        self.handlers.get(holding_id).cloned()
    }

    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

/// Bounded loop cadence and capacity. `max_events_per_pass` bounds the number
/// of Holding partitions handled sequentially in one pass; the claim inside
/// each partition is still exactly one event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutboxSupervisorPolicy {
    pub idle_interval: Duration,
    pub busy_interval: Duration,
    pub lease_refresh: Duration,
    pub restart_backoff: Duration,
    pub max_events_per_pass: usize,
}

impl Default for OutboxSupervisorPolicy {
    fn default() -> Self {
        Self {
            idle_interval: Duration::from_millis(DEFAULT_IDLE_POLL_MILLIS),
            busy_interval: Duration::from_millis(DEFAULT_BUSY_POLL_MILLIS),
            lease_refresh: Duration::from_millis(DEFAULT_LEASE_REFRESH_MILLIS),
            restart_backoff: Duration::from_millis(DEFAULT_RESTART_BACKOFF_MILLIS),
            max_events_per_pass: DEFAULT_MAX_EVENTS_PER_PASS,
        }
    }
}

impl OutboxSupervisorPolicy {
    pub fn validate(&self) -> Result<(), OutboxSupervisorError> {
        if self.idle_interval.is_zero()
            || self.busy_interval.is_zero()
            || self.restart_backoff.is_zero()
            || self.lease_refresh.is_zero()
            || self.lease_refresh >= Duration::from_millis(DEFAULT_LEASE_MILLIS as u64)
            || self.max_events_per_pass == 0
            || self.max_events_per_pass > MAX_EVENTS_PER_PASS_LIMIT
        {
            return Err(OutboxSupervisorError::new(
                "OUTBOX_SUPERVISOR_POLICY_INVALID",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutboxHoldingOutcome {
    Idle,
    Completed {
        event_id: String,
        /// The event was already terminal when acknowledgement ran.
        replayed: bool,
    },
    Retried {
        event_id: String,
        attempts: i64,
        code: String,
        dead_lettered: bool,
    },
    /// A persistence/lease failure. The event was not acknowledged and remains
    /// eligible after its lease expires. The loop logs and backs off.
    Rejected {
        event_id: Option<String>,
        code: &'static str,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OutboxPassReport {
    pub holdings: Vec<(String, OutboxHoldingOutcome)>,
    pub completed: usize,
    pub retried: usize,
    pub dead_lettered: usize,
    pub rejected: usize,
}

impl OutboxPassReport {
    pub fn worked(&self) -> bool {
        self.completed > 0 || self.retried > 0
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OutboxRunSummary {
    pub passes: usize,
    pub completed: usize,
    pub retried: usize,
    pub dead_lettered: usize,
    pub rejected: usize,
    pub restarts: usize,
    pub graceful_shutdown: bool,
}

struct OutboxTarget {
    holding: OutboxHolding,
    handler: Arc<dyn OutboxJobHandler>,
}

/// Sequential multi-Holding supervisor. It owns no business handler beyond the
/// registry supplied by composition.
pub struct OutboxSupervisor {
    targets: Vec<OutboxTarget>,
    policy: OutboxSupervisorPolicy,
    token_counter: AtomicU64,
}

impl std::fmt::Debug for OutboxSupervisor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutboxSupervisor")
            .field(
                "holdings",
                &self
                    .targets
                    .iter()
                    .map(|target| target.holding.holding_id.as_str())
                    .collect::<Vec<_>>(),
            )
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl OutboxSupervisor {
    pub fn new(
        holdings: Vec<OutboxHolding>,
        registry: &OutboxHandlerRegistry,
        policy: OutboxSupervisorPolicy,
    ) -> Result<Self, OutboxSupervisorError> {
        policy.validate()?;
        if holdings.is_empty() {
            return Err(OutboxSupervisorError::new("OUTBOX_HOLDING_REQUIRED"));
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut targets = Vec::with_capacity(holdings.len());
        for holding in holdings {
            if !seen.insert(holding.holding_id.clone()) {
                return Err(OutboxSupervisorError::new("OUTBOX_HOLDING_DUPLICATE"));
            }
            let handler = registry
                .handler(&holding.holding_id)
                .ok_or_else(|| OutboxSupervisorError::new("OUTBOX_HANDLER_REQUIRED"))?;
            targets.push(OutboxTarget { holding, handler });
        }
        Ok(Self {
            targets,
            policy,
            token_counter: AtomicU64::new(0),
        })
    }

    pub fn policy(&self) -> OutboxSupervisorPolicy {
        self.policy
    }

    pub fn attestations(&self) -> Vec<OutboxAttestation> {
        self.targets
            .iter()
            .map(|target| target.holding.attestation.clone())
            .collect()
    }

    fn token(&self, now: i64) -> String {
        let sequence = self.token_counter.fetch_add(1, Ordering::Relaxed);
        format!("native-supervisor:{}:{now}:{sequence}", std::process::id())
    }

    /// One bounded sequential pass. Every Holding is attempted even when an
    /// earlier Holding is idle, fails or loses its lease.
    pub async fn run_once(&self, now: i64) -> OutboxPassReport {
        let mut report = OutboxPassReport::default();
        if now < 0 {
            for target in self.targets.iter().take(self.policy.max_events_per_pass) {
                report.rejected = report.rejected.saturating_add(1);
                report.holdings.push((
                    target.holding.holding_id.clone(),
                    OutboxHoldingOutcome::Rejected {
                        event_id: None,
                        code: "INVALID_OUTBOX_CLOCK",
                    },
                ));
            }
            return report;
        }
        for target in self.targets.iter().take(self.policy.max_events_per_pass) {
            let outcome = self.run_holding(target, now).await;
            match &outcome {
                OutboxHoldingOutcome::Completed { .. } => {
                    report.completed = report.completed.saturating_add(1)
                }
                OutboxHoldingOutcome::Retried { dead_lettered, .. } => {
                    report.retried = report.retried.saturating_add(1);
                    if *dead_lettered {
                        report.dead_lettered = report.dead_lettered.saturating_add(1);
                    }
                }
                OutboxHoldingOutcome::Rejected { .. } => {
                    report.rejected = report.rejected.saturating_add(1)
                }
                OutboxHoldingOutcome::Idle => {}
            }
            report
                .holdings
                .push((target.holding.holding_id.clone(), outcome));
        }
        report
    }

    async fn run_holding(&self, target: &OutboxTarget, now: i64) -> OutboxHoldingOutcome {
        let holding = &target.holding;
        let token = self.token(now);
        let mut connection = match holding.write.acquire().await {
            Ok(connection) => connection,
            Err(_) => {
                return OutboxHoldingOutcome::Rejected {
                    event_id: None,
                    code: "OUTBOX_CLAIM_CONNECTION_FAILED",
                };
            }
        };
        let job = match claim(&mut connection, &holding.owned, &token, now).await {
            Ok(Some(job)) => job,
            Ok(None) => return OutboxHoldingOutcome::Idle,
            Err(_) => {
                return OutboxHoldingOutcome::Rejected {
                    event_id: None,
                    code: "OUTBOX_CLAIM_FAILED",
                };
            }
        };
        drop(connection);
        self.handle_claim(target, job, now).await
    }

    async fn handle_claim(
        &self,
        target: &OutboxTarget,
        job: OutboxJob,
        now: i64,
    ) -> OutboxHoldingOutcome {
        let holding = &target.holding;
        let event_id = job.id.clone();
        let started = std::time::Instant::now();
        let work = target.handler.handle(&holding.write, &job, now);
        tokio::pin!(work);
        let mut refresh = tokio::time::interval(self.policy.lease_refresh);
        refresh.tick().await;
        let handled = loop {
            tokio::select! {
                result = &mut work => break HandleResult::Done(result),
                _ = refresh.tick() => {
                    let clock = now.saturating_add(elapsed_millis(started));
                    let renewed = match holding.write.acquire().await {
                        Ok(mut connection) => renew(&mut connection, &job.id, &job.lease_token, clock).await,
                        Err(_) => Err(sqlx::Error::PoolClosed),
                    };
                    match renewed {
                        Ok(true) => {}
                        Ok(false) => break HandleResult::LeaseLost,
                        Err(_) => break HandleResult::RenewFailed,
                    }
                }
            }
        };
        let clock = now.saturating_add(elapsed_millis(started));
        let mut connection = match holding.write.acquire().await {
            Ok(connection) => connection,
            Err(_) => {
                return OutboxHoldingOutcome::Rejected {
                    event_id: Some(event_id.clone()),
                    code: "OUTBOX_ACK_CONNECTION_FAILED",
                };
            }
        };
        match handled {
            HandleResult::LeaseLost => OutboxHoldingOutcome::Rejected {
                event_id: Some(event_id.clone()),
                code: "WORKER_LEASE_LOST",
            },
            HandleResult::RenewFailed => OutboxHoldingOutcome::Rejected {
                event_id: Some(event_id.clone()),
                code: "OUTBOX_RENEW_FAILED",
            },
            HandleResult::Done(Ok(WorkerOutcome::Complete)) => {
                match complete_lease(&mut connection, &job.id, &job.lease_token, clock).await {
                    Ok(LeaseCompletion::Applied) => OutboxHoldingOutcome::Completed {
                        event_id: event_id.clone(),
                        replayed: false,
                    },
                    Ok(LeaseCompletion::AlreadyCompleted) => OutboxHoldingOutcome::Completed {
                        event_id: event_id.clone(),
                        replayed: true,
                    },
                    Ok(LeaseCompletion::LeaseLost) => OutboxHoldingOutcome::Rejected {
                        event_id: Some(event_id.clone()),
                        code: "WORKER_LEASE_LOST",
                    },
                    Err(_) => OutboxHoldingOutcome::Rejected {
                        event_id: Some(event_id.clone()),
                        code: "OUTBOX_COMPLETE_FAILED",
                    },
                }
            }
            HandleResult::Done(Ok(WorkerOutcome::Fail(code))) => {
                self.record_failure(&mut connection, holding, &job, job.attempts, code, clock)
                    .await
            }
            HandleResult::Done(Err(error)) => {
                self.record_failure(
                    &mut connection,
                    holding,
                    &job,
                    job.attempts,
                    error.code,
                    clock,
                )
                .await
            }
        }
    }

    async fn record_failure(
        &self,
        connection: &mut sqlx::PgConnection,
        holding: &OutboxHolding,
        job: &OutboxJob,
        attempts: i64,
        code: &str,
        clock: i64,
    ) -> OutboxHoldingOutcome {
        match fail(connection, &job.id, &job.lease_token, attempts, clock, code).await {
            Ok(true) => {
                let truncated = truncate_error_code(code);
                let dead_lettered = attempts >= MAX_ATTEMPTS;
                eprintln!(
                    "{}",
                    failure_log_line(&holding.holding_id, &job.id, attempts, &truncated)
                );
                OutboxHoldingOutcome::Retried {
                    event_id: job.id.clone(),
                    attempts,
                    code: truncated,
                    dead_lettered,
                }
            }
            Ok(false) => OutboxHoldingOutcome::Rejected {
                event_id: Some(job.id.clone()),
                code: "WORKER_LEASE_LOST",
            },
            Err(_) => OutboxHoldingOutcome::Rejected {
                event_id: Some(job.id.clone()),
                code: "OUTBOX_FAILURE_RECORD_FAILED",
            },
        }
    }

    /// Drain until shutdown. A failed pass is logged as a restart event and the
    /// loop backs off; it never acknowledges an unpersisted effect and never
    /// exits silently with work outstanding.
    pub async fn run<F>(
        self,
        clock: Arc<dyn Clock + Send + Sync>,
        shutdown: F,
    ) -> Result<OutboxRunSummary, OutboxSupervisorError>
    where
        F: Future<Output = ()> + Send,
    {
        let mut shutdown = Box::pin(shutdown);
        let mut summary = OutboxRunSummary::default();
        loop {
            let now = clock.now().get();
            let report = tokio::select! {
                _ = &mut shutdown => {
                    summary.graceful_shutdown = true;
                    break;
                }
                report = self.run_once(now) => report,
            };
            summary.passes = summary.passes.saturating_add(1);
            summary.completed = summary.completed.saturating_add(report.completed);
            summary.retried = summary.retried.saturating_add(report.retried);
            summary.dead_lettered = summary.dead_lettered.saturating_add(report.dead_lettered);
            summary.rejected = summary.rejected.saturating_add(report.rejected);
            if report.rejected > 0 {
                for (holding_id, outcome) in &report.holdings {
                    if let OutboxHoldingOutcome::Rejected { code, .. } = outcome {
                        summary.restarts = summary.restarts.saturating_add(1);
                        eprintln!("{}", restart_log_line(holding_id, code));
                    }
                }
                tokio::select! {
                    _ = &mut shutdown => {
                        summary.graceful_shutdown = true;
                        break;
                    }
                    _ = tokio::time::sleep(self.policy.restart_backoff) => {}
                }
            } else {
                let interval = if report.worked() {
                    self.policy.busy_interval
                } else {
                    self.policy.idle_interval
                };
                tokio::select! {
                    _ = &mut shutdown => {
                        summary.graceful_shutdown = true;
                        break;
                    }
                    _ = tokio::time::sleep(interval) => {}
                }
            }
        }
        Ok(summary)
    }
}

enum HandleResult {
    Done(Result<WorkerOutcome, BusinessError>),
    LeaseLost,
    RenewFailed,
}

fn elapsed_millis(started: std::time::Instant) -> i64 {
    i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX)
}

/// Stable structured restart log. It contains only the registered Holding id
/// and a finite error code.
pub fn restart_log_line(holding_id: &str, code: &str) -> String {
    serde_json::json!({
        "event": "outbox_supervisor_restart",
        "holdingId": holding_id,
        "code": code,
    })
    .to_string()
}

/// Stable structured per-attempt failure log retained from the TypeScript
/// worker, with the same bounded fields and no payload or provider text.
pub fn failure_log_line(holding_id: &str, event_id: &str, attempts: i64, code: &str) -> String {
    serde_json::json!({
        "event": "job_failed",
        "holdingId": holding_id,
        "eventId": event_id,
        "attempt": attempts,
        "code": code,
    })
    .to_string()
}

/// Record the supervisor's attestation proof as one durable `runtime_health`
/// row on Control. Composition owns the Control pool and timing; this never
/// opens a business transaction.
pub async fn record_outbox_attestation(
    control: &PgPool,
    service: &str,
    supervisor: &OutboxSupervisor,
    now: i64,
) -> Result<(), OutboxSupervisorError> {
    if service.is_empty()
        || service.len() > 64
        || !service
            .bytes()
            .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == b'-')
        || now < 0
    {
        return Err(OutboxSupervisorError::new("OUTBOX_ATTESTATION_ROW_INVALID"));
    }
    let attestations = supervisor
        .attestations()
        .into_iter()
        .map(|attestation| {
            serde_json::json!({
                "holdingId": attestation.holding_id,
                "database": attestation.database,
                "role": attestation.role,
                "primary": attestation.primary,
                "migrationCount": attestation.migration_count,
                "migrationSha256": attestation.migration_sha256,
            })
        })
        .collect::<Vec<_>>();
    let details = serde_json::json!({
        "runtime": "native-rust",
        "supervisor": "outbox",
        "attestations": attestations,
    })
    .to_string();
    sqlx::query(
        "INSERT INTO runtime_health(service,heartbeat_at,details_json) VALUES($1,$2,$3) \
         ON CONFLICT(service) DO UPDATE SET heartbeat_at=excluded.heartbeat_at,\
         details_json=excluded.details_json",
    )
    .bind(service)
    .bind(now)
    .bind(details)
    .execute(control)
    .await?;
    Ok(())
}

/// Free-function parity with the retained worker bootstrap. The caller keeps
/// pool admission, attestation, Control heartbeat and process supervision.
pub async fn run_outbox_supervisor<F>(
    supervisor: OutboxSupervisor,
    clock: Arc<dyn Clock + Send + Sync>,
    shutdown: F,
) -> Result<OutboxRunSummary, OutboxSupervisorError>
where
    F: Future<Output = ()> + Send,
{
    supervisor.run(clock, shutdown).await
}
