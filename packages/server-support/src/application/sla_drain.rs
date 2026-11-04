//! Scheduled Support SLA drain policy. The adapter owns the retained SQL; this
//! module owns the interval, bounded failure backoff and restart recovery. It
//! never opens a transaction, reads HTTP or calls a provider.

use erpx_server_shared::public::BusinessError;

pub const DEFAULT_INTERVAL_MS: i64 = 30_000;
pub const MIN_INTERVAL_MS: i64 = 1_000;
pub const MAX_INTERVAL_MS: i64 = 300_000;
pub const MAX_BACKOFF_MS: i64 = 300_000;
const MAX_BACKOFF_SHIFT: u32 = 4;

/// A worker restart must run the first sweep immediately, then retain the
/// configured cadence. Transient failures back off exponentially up to the
/// same five-minute ceiling as the interval instead of stopping the worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlaDrainSchedule {
    interval_ms: i64,
    next_due_at: i64,
    consecutive_failures: u32,
}

impl SlaDrainSchedule {
    pub fn start(now: i64, interval_ms: i64) -> Result<Self, BusinessError> {
        if !(MIN_INTERVAL_MS..=MAX_INTERVAL_MS).contains(&interval_ms) {
            return Err(BusinessError::invalid(
                "SUPPORT_SLA_DRAIN_INTERVAL_INVALID",
                "Choose a Support SLA drain interval between one and three hundred seconds.",
            ));
        }
        Ok(Self {
            interval_ms,
            next_due_at: now,
            consecutive_failures: 0,
        })
    }

    pub fn interval_ms(&self) -> i64 {
        self.interval_ms
    }

    pub fn next_due_at(&self) -> i64 {
        self.next_due_at
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    pub fn due(&self, now: i64) -> bool {
        now >= self.next_due_at
    }

    /// Sleep duration for the worker supervisor. A backward clock reports zero
    /// rather than postponing the retained sweep indefinitely.
    pub fn next_delay_ms(&self, now: i64) -> i64 {
        self.next_due_at.saturating_sub(now).max(0)
    }

    pub fn succeeded(&mut self, now: i64) {
        self.consecutive_failures = 0;
        self.next_due_at = now.saturating_add(self.interval_ms);
    }

    pub fn failed(&mut self, now: i64) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        let shift = (self.consecutive_failures - 1).min(MAX_BACKOFF_SHIFT);
        let multiplier = 1_i64 << shift;
        let delay = self
            .interval_ms
            .saturating_mul(multiplier)
            .min(MAX_BACKOFF_MS);
        self.next_due_at = now.saturating_add(delay);
    }

    /// Explicit recovery after restart or a replacement worker takes over.
    pub fn restart(&mut self, now: i64) {
        self.consecutive_failures = 0;
        self.next_due_at = now;
    }
}

/// One due worker tick. A failure records the backoff before returning the
/// caller's error so the supervisor can log it and continue without losing the
/// recovery deadline. A tick that is not due performs no database work.
pub async fn run_sla_drain_tick<F, Fut, E>(
    schedule: &mut SlaDrainSchedule,
    now: i64,
    drain: F,
) -> Result<Option<i64>, E>
where
    F: FnOnce(i64) -> Fut,
    Fut: std::future::Future<Output = Result<i64, E>>,
{
    if !schedule.due(now) {
        return Ok(None);
    }
    match drain(now).await {
        Ok(drained) => {
            schedule.succeeded(now);
            Ok(Some(drained))
        }
        Err(error) => {
            schedule.failed(now);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::ready;

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        let mut future = Box::pin(future);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => panic!("test future unexpectedly pending"),
        }
    }

    #[test]
    fn starts_due_immediately_and_schedules_the_interval_after_success() {
        let mut schedule = SlaDrainSchedule::start(1_000, DEFAULT_INTERVAL_MS).unwrap();
        assert!(schedule.due(1_000));
        assert_eq!(schedule.next_delay_ms(1_000), 0);
        assert_eq!(
            block_on(run_sla_drain_tick(&mut schedule, 1_000, |at| {
                assert_eq!(at, 1_000);
                ready(Ok::<i64, &'static str>(2))
            }))
            .unwrap(),
            Some(2)
        );
        assert_eq!(schedule.consecutive_failures(), 0);
        assert_eq!(schedule.next_due_at(), 31_000);
        assert!(!schedule.due(30_999));
        assert_eq!(schedule.next_delay_ms(30_999), 1);
    }

    #[test]
    fn skips_a_tick_that_is_not_due_without_calling_the_owner() {
        let mut schedule = SlaDrainSchedule::start(1_000, 5_000).unwrap();
        let called = std::sync::atomic::AtomicBool::new(false);
        let outcome = block_on(run_sla_drain_tick(&mut schedule, 999, |_| {
            called.store(true, std::sync::atomic::Ordering::SeqCst);
            ready(Ok::<i64, &'static str>(1))
        }));
        assert!(outcome.unwrap().is_none());
        assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(schedule.next_delay_ms(999), 1);
    }

    #[test]
    fn bounds_exponential_failure_backoff_and_keeps_retrying() {
        let mut schedule = SlaDrainSchedule::start(0, 30_000).unwrap();
        let mut now = 0;
        let mut expected_delay = 30_000;
        for failure in 1..=5 {
            let error = block_on(run_sla_drain_tick(&mut schedule, now, |_| {
                ready(Err::<i64, &'static str>("database unavailable"))
            }))
            .unwrap_err();
            assert_eq!(error, "database unavailable");
            assert_eq!(schedule.consecutive_failures(), failure);
            assert_eq!(schedule.next_due_at(), now + expected_delay);
            assert_eq!(schedule.next_delay_ms(now), expected_delay);
            now = schedule.next_due_at();
            expected_delay = (expected_delay * 2).min(MAX_BACKOFF_MS);
        }
        assert_eq!(schedule.next_due_at(), 750_000);
    }

    #[test]
    fn restart_runs_immediately_and_success_clears_failure_history() {
        let mut schedule = SlaDrainSchedule::start(5_000, 30_000).unwrap();
        block_on(run_sla_drain_tick(&mut schedule, 5_000, |_| {
            ready(Err::<i64, &'static str>("stopped"))
        }))
        .unwrap_err();
        assert_eq!(schedule.consecutive_failures(), 1);
        assert_eq!(schedule.next_due_at(), 35_000);
        schedule.restart(70_000);
        assert_eq!(schedule.consecutive_failures(), 0);
        assert!(schedule.due(70_000));
        assert_eq!(
            block_on(run_sla_drain_tick(&mut schedule, 70_000, |_| {
                ready(Ok::<i64, &'static str>(0))
            }))
            .unwrap(),
            Some(0)
        );
        assert_eq!(schedule.next_due_at(), 100_000);
        assert_eq!(schedule.consecutive_failures(), 0);
    }

    #[test]
    fn rejects_intervals_outside_the_bounded_worker_window() {
        for interval in [0, MIN_INTERVAL_MS - 1, MAX_INTERVAL_MS + 1] {
            assert_eq!(
                SlaDrainSchedule::start(0, interval).unwrap_err().code,
                "SUPPORT_SLA_DRAIN_INTERVAL_INVALID"
            );
        }
    }
}
