//! Portable outbox supervisor decisions: retained backoff, dead-letter
//! boundary, policy bounds, registry ownership, attestation proof and stable
//! restart/failure logs. No database is opened.
#![allow(dead_code)]

use erpx_server_runtime::public::postgres::{
    DEFAULT_MAX_EVENTS_PER_PASS, MAX_EVENTS_PER_PASS_LIMIT, OutboxAttestation,
    OutboxHandlerRegistry, OutboxHolding, OutboxPassReport, OutboxRetrySchedule, OutboxSupervisor,
    OutboxSupervisorPolicy, WorkerFuture, WorkerOutcome, failure_log_line, restart_log_line,
    retry_delay_millis, retry_schedule,
};
use erpx_server_runtime::public::postgres::{OutboxJob, OutboxJobHandler};
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};

struct NoopHandler;
impl OutboxJobHandler for NoopHandler {
    fn handle<'a>(&'a self, _: &'a PgPool, _: &'a OutboxJob, _: i64) -> WorkerFuture<'a> {
        Box::pin(async { Ok(WorkerOutcome::Complete) })
    }
}

fn attestation(holding: &str) -> OutboxAttestation {
    OutboxAttestation::new(
        holding,
        "erpx_dev_rust_contract",
        "erpx_test_admin",
        true,
        1_097,
        &"a".repeat(64),
    )
    .unwrap()
}

fn lazy_pool() -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://unused")
        .unwrap()
}

fn holding(holding_id: &str) -> OutboxHolding {
    OutboxHolding::new(
        holding_id,
        lazy_pool(),
        "e.event_type LIKE 'field.order.%'",
        attestation(holding_id),
    )
    .unwrap()
}

fn registry(holdings: &[&str]) -> OutboxHandlerRegistry {
    let mut registry = OutboxHandlerRegistry::new();
    for holding in holdings {
        registry.register(holding, Arc::new(NoopHandler)).unwrap();
    }
    registry
}

#[test]
fn bounded_backoff_and_sixth_attempt_dead_letter_match_the_retained_worker() {
    let now = 1_800_000_000_000_i64;
    for (attempts, delay) in [
        (0, 1_000),
        (1, 2_000),
        (2, 4_000),
        (3, 8_000),
        (4, 16_000),
        (5, 32_000),
        (6, 60_000),
        (7, 60_000),
        (60, 60_000),
    ] {
        assert_eq!(retry_delay_millis(attempts), delay);
        assert_eq!(
            retry_schedule(attempts, now),
            OutboxRetrySchedule {
                next_attempt_at: now + delay,
                dead_letter_at: (attempts >= 6).then_some(now),
            }
        );
    }
    assert_eq!(
        retry_schedule(5, now).dead_letter_at,
        None,
        "the fifth failed attempt must remain retryable"
    );
    assert_eq!(
        retry_schedule(6, now).dead_letter_at,
        Some(now),
        "the sixth failed attempt is terminal"
    );
    assert_eq!(
        retry_schedule(6, i64::MAX).next_attempt_at,
        i64::MAX,
        "clock overflow saturates instead of wrapping into the past"
    );
}

#[test]
fn policy_rejects_unbounded_or_lease_consuming_cadence() {
    let policy = OutboxSupervisorPolicy::default();
    assert_eq!(policy.max_events_per_pass, DEFAULT_MAX_EVENTS_PER_PASS);
    assert!(policy.validate().is_ok());

    let mut zero = policy;
    zero.idle_interval = Duration::ZERO;
    assert_eq!(
        zero.validate().unwrap_err().code,
        "OUTBOX_SUPERVISOR_POLICY_INVALID"
    );

    let mut unbounded = policy;
    unbounded.max_events_per_pass = MAX_EVENTS_PER_PASS_LIMIT + 1;
    assert!(unbounded.validate().is_err());

    let mut no_capacity = policy;
    no_capacity.max_events_per_pass = 0;
    assert!(no_capacity.validate().is_err());

    let mut stale_heartbeat = policy;
    stale_heartbeat.lease_refresh = Duration::from_millis(90_000);
    assert!(stale_heartbeat.validate().is_err());
}

#[tokio::test]
async fn registry_requires_one_distinct_native_handler_per_holding() {
    let mut registry = OutboxHandlerRegistry::new();
    registry.register("h001", Arc::new(NoopHandler)).unwrap();
    assert!(registry.handler("h001").is_some());
    assert!(registry.handler("h002").is_none());
    assert_eq!(registry.len(), 1);
    assert_eq!(
        registry
            .register("h001", Arc::new(NoopHandler))
            .unwrap_err()
            .code,
        "OUTBOX_HANDLER_DUPLICATE"
    );
    assert_eq!(
        registry
            .register(" ", Arc::new(NoopHandler))
            .unwrap_err()
            .code,
        "OUTBOX_HOLDING_INVALID"
    );

    assert_eq!(
        OutboxSupervisor::new(
            vec![holding("h001")],
            &OutboxHandlerRegistry::new(),
            OutboxSupervisorPolicy::default()
        )
        .unwrap_err()
        .code,
        "OUTBOX_HANDLER_REQUIRED"
    );
}

#[tokio::test]
async fn supervisor_requires_attested_primary_unique_holdings() {
    assert_eq!(
        OutboxSupervisor::new(
            Vec::new(),
            &registry(&[]),
            OutboxSupervisorPolicy::default()
        )
        .unwrap_err()
        .code,
        "OUTBOX_HOLDING_REQUIRED"
    );

    assert_eq!(
        OutboxSupervisor::new(
            vec![holding("h001"), holding("h001")],
            &registry(&["h001"]),
            OutboxSupervisorPolicy::default()
        )
        .unwrap_err()
        .code,
        "OUTBOX_HOLDING_DUPLICATE"
    );

    let replica = OutboxAttestation::new(
        "h001",
        "erpx_dev_rust_contract",
        "erpx_test_admin",
        false,
        1_097,
        &"a".repeat(64),
    )
    .unwrap();
    assert_eq!(
        OutboxHolding::new("h001", lazy_pool(), "TRUE", replica)
            .unwrap_err()
            .code,
        "OUTBOX_ATTESTATION_INVALID"
    );

    assert_eq!(
        OutboxAttestation::new("h001", "db", "role", true, 0, &"a".repeat(64))
            .unwrap_err()
            .code,
        "OUTBOX_ATTESTATION_INVALID"
    );
    assert_eq!(
        OutboxAttestation::new("h001", "db", "role", true, 1, &"A".repeat(64))
            .unwrap_err()
            .code,
        "OUTBOX_ATTESTATION_INVALID"
    );
    assert_eq!(
        OutboxHolding::new("", lazy_pool(), "TRUE", attestation("h001"))
            .unwrap_err()
            .code,
        "OUTBOX_HOLDING_INVALID"
    );
    assert_eq!(
        OutboxHolding::new("h001", lazy_pool(), " ", attestation("h001"))
            .unwrap_err()
            .code,
        "OUTBOX_HOLDING_INVALID"
    );

    let supervisor = OutboxSupervisor::new(
        vec![holding("h001"), holding("h002")],
        &registry(&["h001", "h002"]),
        OutboxSupervisorPolicy::default(),
    )
    .unwrap();
    assert_eq!(supervisor.attestations().len(), 2);
    assert_eq!(supervisor.policy().max_events_per_pass, 64);
}

#[test]
fn restart_and_failure_logs_are_stable_and_do_not_leak_sql() {
    let restart: serde_json::Value =
        serde_json::from_str(&restart_log_line("h001", "OUTBOX_CLAIM_FAILED")).unwrap();
    assert_eq!(restart["event"], "outbox_supervisor_restart");
    assert_eq!(restart["holdingId"], "h001");
    assert_eq!(restart["code"], "OUTBOX_CLAIM_FAILED");
    assert!(restart.get("sql").is_none());
    assert!(restart.get("payload").is_none());

    let failure: serde_json::Value =
        serde_json::from_str(&failure_log_line("h001", "event-1", 6, "SOURCE_INVALID")).unwrap();
    assert_eq!(failure["event"], "job_failed");
    assert_eq!(failure["holdingId"], "h001");
    assert_eq!(failure["eventId"], "event-1");
    assert_eq!(failure["attempt"], 6);
    assert_eq!(failure["code"], "SOURCE_INVALID");
}

#[test]
fn pass_report_only_treats_durable_attempts_as_work() {
    let mut report = OutboxPassReport::default();
    assert!(!report.worked());
    report.completed = 1;
    assert!(report.worked());
    report = OutboxPassReport {
        retried: 1,
        ..OutboxPassReport::default()
    };
    assert!(report.worked());
    report = OutboxPassReport {
        rejected: 1,
        ..OutboxPassReport::default()
    };
    assert!(!report.worked(), "a rejected pass must back off, not spin");
}

#[tokio::test]
async fn an_invalid_clock_rejects_every_holding_without_touching_a_pool() {
    let supervisor = OutboxSupervisor::new(
        vec![holding("h001"), holding("h002")],
        &registry(&["h001", "h002"]),
        OutboxSupervisorPolicy::default(),
    )
    .unwrap();
    let report = supervisor.run_once(-1).await;
    assert_eq!(report.rejected, 2);
    assert_eq!(report.completed, 0);
    assert!(report.holdings.iter().all(|(_, outcome)| matches!(
        outcome,
        erpx_server_runtime::public::postgres::OutboxHoldingOutcome::Rejected {
            code: "INVALID_OUTBOX_CLOCK",
            ..
        }
    )));
}
