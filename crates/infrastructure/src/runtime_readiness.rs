//! Runtime-neutral readiness composition shared by inbound and worker adapters.
//!
//! These types intentionally live beside the durable/live infrastructure
//! observations. Missing, stale, future, or malformed evidence always fails
//! closed and configuration alone never proves readiness.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::PersistenceError;
use crate::provisioning::{
    DatabaseSide, HoldingBackupRun, HoldingDatabaseHealth, HoldingDatabaseRouter,
    HoldingReadinessObservation, HoldingStorageHealth, ReadinessObservationPolicy,
    ReadinessObservationState,
};

/// Fail-closed state used at runtime composition boundaries.
///
/// `Ready` is reserved for observed healthy state. Configuration or absent
/// metadata alone maps to `NotProbed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadinessState {
    Ready,
    NotProbed,
    Unavailable,
}

impl ReadinessState {
    pub fn from_status(status: &str) -> Self {
        match status {
            "ready" => Self::Ready,
            "not_probed" | "unprobed" => Self::NotProbed,
            _ => Self::Unavailable,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::NotProbed => "not_probed",
            Self::Unavailable => "unavailable",
        }
    }

    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }
}

impl From<ReadinessObservationState> for ReadinessState {
    fn from(state: ReadinessObservationState) -> Self {
        match state {
            ReadinessObservationState::Ready => Self::Ready,
            ReadinessObservationState::NotProbed => Self::NotProbed,
            ReadinessObservationState::Unavailable => Self::Unavailable,
        }
    }
}

/// Aggregate component states without treating an empty set as healthy.
pub fn aggregate_readiness_states(
    states: impl IntoIterator<Item = ReadinessState>,
) -> ReadinessState {
    let mut observed = false;
    let mut not_probed = false;
    for state in states {
        observed = true;
        match state {
            ReadinessState::Ready => {}
            ReadinessState::NotProbed => not_probed = true,
            ReadinessState::Unavailable => return ReadinessState::Unavailable,
        }
    }
    if !observed || not_probed {
        ReadinessState::NotProbed
    } else {
        ReadinessState::Ready
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerHeartbeatReadiness {
    pub state: ReadinessState,
}

impl WorkerHeartbeatReadiness {
    /// Classify injected heartbeat evidence. Missing, future, stale, or a
    /// negative freshness window cannot prove liveness.
    pub fn from_last_seen(
        last_seen: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
        max_age: Duration,
    ) -> Self {
        let state = match last_seen {
            None => ReadinessState::NotProbed,
            Some(last_seen)
                if last_seen <= now
                    && max_age >= Duration::zero()
                    && now.signed_duration_since(last_seen) <= max_age =>
            {
                ReadinessState::Ready
            }
            Some(_) => ReadinessState::Unavailable,
        };
        Self { state }
    }

    pub const fn not_probed() -> Self {
        Self {
            state: ReadinessState::NotProbed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerHeartbeatObservation {
    pub worker_id: Uuid,
    pub last_seen: Option<DateTime<Utc>>,
    pub observed_at: DateTime<Utc>,
    pub state: ReadinessState,
}

/// In-process heartbeat state. The scheduler owns when `record_at` is called;
/// readiness becomes `Ready` only after a successful, recent observation.
#[derive(Debug, Clone)]
pub struct WorkerHeartbeat {
    worker_id: Uuid,
    last_seen: Arc<Mutex<Option<DateTime<Utc>>>>,
}

impl WorkerHeartbeat {
    pub fn new(worker_id: Uuid) -> Self {
        Self {
            worker_id,
            last_seen: Arc::new(Mutex::new(None)),
        }
    }

    pub fn record_at(&self, observed_at: DateTime<Utc>) -> bool {
        let Ok(mut last_seen) = self.last_seen.lock() else {
            return false;
        };
        *last_seen = Some(observed_at);
        true
    }

    pub fn observe(&self, now: DateTime<Utc>, max_age: Duration) -> WorkerHeartbeatObservation {
        let last_seen = self.last_seen.lock().ok().and_then(|last_seen| *last_seen);
        let state = WorkerHeartbeatReadiness::from_last_seen(last_seen, now, max_age).state;
        WorkerHeartbeatObservation {
            worker_id: self.worker_id,
            last_seen,
            observed_at: now,
            state,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerLiveReadinessPolicy {
    pub heartbeat_max_age: Duration,
    pub holding: ReadinessObservationPolicy,
}

impl Default for WorkerLiveReadinessPolicy {
    fn default() -> Self {
        Self {
            heartbeat_max_age: Duration::minutes(1),
            holding: ReadinessObservationPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerLiveReadinessObservation {
    pub heartbeat: WorkerHeartbeatObservation,
    pub holding: HoldingReadinessObservation,
    pub composition: WorkerReadinessComposition,
}

/// Readiness observer shared by executable composition roots.
///
/// It owns only explicit in-process heartbeat evidence. Holding observations
/// always come from the control-plane registry and live route probes.
#[derive(Debug, Clone)]
pub struct WorkerReadinessObserver {
    heartbeat: WorkerHeartbeat,
}

impl WorkerReadinessObserver {
    pub fn new(worker_id: Uuid) -> Self {
        Self {
            heartbeat: WorkerHeartbeat::new(worker_id),
        }
    }

    /// Compose an observer around scheduler-owned heartbeat state.
    pub fn from_heartbeat(heartbeat: WorkerHeartbeat) -> Self {
        Self { heartbeat }
    }

    pub fn heartbeat(&self) -> &WorkerHeartbeat {
        &self.heartbeat
    }

    pub fn record_heartbeat_at(&self, observed_at: DateTime<Utc>) -> bool {
        self.heartbeat.record_at(observed_at)
    }

    pub async fn observe_live_readiness(
        &self,
        router: &HoldingDatabaseRouter,
        holding_id: &str,
        now: DateTime<Utc>,
        policy: WorkerLiveReadinessPolicy,
    ) -> Result<WorkerLiveReadinessObservation, PersistenceError> {
        let holding = router
            .observe_holding_readiness(holding_id, now, policy.holding)
            .await?;
        let heartbeat = self.heartbeat.observe(now, policy.heartbeat_max_age);
        let composition = WorkerReadinessComposition {
            worker_heartbeat: heartbeat.state,
            holding_routing: holding.routing.state.into(),
            projection_freshness: holding.projection_freshness.state.into(),
            backup_pitr: holding.backup_pitr.state.into(),
        };
        Ok(WorkerLiveReadinessObservation {
            heartbeat,
            holding,
            composition,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerReadinessComposition {
    pub worker_heartbeat: ReadinessState,
    pub holding_routing: ReadinessState,
    pub projection_freshness: ReadinessState,
    pub backup_pitr: ReadinessState,
}

#[derive(Debug, Clone, Copy)]
pub struct WorkerReadinessMetadata<'a> {
    pub holding_id: &'a str,
    pub registry_state: Option<&'a str>,
    pub write_health: Option<&'a HoldingDatabaseHealth>,
    pub read_health: Option<&'a HoldingDatabaseHealth>,
    pub storage_health: Option<&'a HoldingStorageHealth>,
    pub projection_state: Option<&'a str>,
    pub latest_backup: Option<&'a HoldingBackupRun>,
}

impl WorkerReadinessComposition {
    pub const fn not_probed() -> Self {
        Self {
            worker_heartbeat: ReadinessState::NotProbed,
            holding_routing: ReadinessState::NotProbed,
            projection_freshness: ReadinessState::NotProbed,
            backup_pitr: ReadinessState::NotProbed,
        }
    }

    pub const fn is_ready(self) -> bool {
        self.worker_heartbeat.is_ready()
            && self.holding_routing.is_ready()
            && self.projection_freshness.is_ready()
            && self.backup_pitr.is_ready()
    }

    pub const fn overall(self) -> ReadinessState {
        if matches!(self.worker_heartbeat, ReadinessState::Unavailable)
            || matches!(self.holding_routing, ReadinessState::Unavailable)
            || matches!(self.projection_freshness, ReadinessState::Unavailable)
            || matches!(self.backup_pitr, ReadinessState::Unavailable)
        {
            ReadinessState::Unavailable
        } else if self.is_ready() {
            ReadinessState::Ready
        } else {
            ReadinessState::NotProbed
        }
    }

    pub fn from_metadata(
        heartbeat: WorkerHeartbeatReadiness,
        metadata: WorkerReadinessMetadata<'_>,
    ) -> Self {
        Self {
            worker_heartbeat: heartbeat.state,
            holding_routing: holding_routing_readiness(
                metadata.holding_id,
                metadata.registry_state,
                metadata.write_health,
                metadata.read_health,
                metadata.storage_health,
            ),
            projection_freshness: projection_freshness_readiness(metadata.projection_state),
            backup_pitr: backup_pitr_readiness(metadata.holding_id, metadata.latest_backup),
        }
    }
}

/// Classify only persisted holding health evidence. A healthy configuration
/// without all three observed health records remains `NotProbed`.
pub fn holding_routing_readiness(
    holding_id: &str,
    registry_state: Option<&str>,
    write_health: Option<&HoldingDatabaseHealth>,
    read_health: Option<&HoldingDatabaseHealth>,
    storage_health: Option<&HoldingStorageHealth>,
) -> ReadinessState {
    let Some(registry_state) = registry_state else {
        return ReadinessState::NotProbed;
    };
    if registry_state != "active" || holding_id.trim().is_empty() {
        return ReadinessState::Unavailable;
    }

    if write_health.is_some_and(|health| {
        health.holding_id != holding_id
            || !health.reachable
            || health.side != DatabaseSide::Write
            || health.database_key.trim().is_empty()
    }) || read_health.is_some_and(|health| {
        health.holding_id != holding_id
            || !health.reachable
            || health.side != DatabaseSide::Read
            || health.database_key.trim().is_empty()
    }) {
        return ReadinessState::Unavailable;
    }
    if storage_health.is_some_and(|health| {
        health.holding_id != holding_id
            || !health.reachable
            || health.storage_namespace.trim().is_empty()
    }) {
        return ReadinessState::Unavailable;
    }
    if write_health.is_none() || read_health.is_none() || storage_health.is_none() {
        return ReadinessState::NotProbed;
    }
    if !matches!(
        write_health.map(|health| health.side),
        Some(DatabaseSide::Write)
    ) || !matches!(
        read_health.map(|health| health.side),
        Some(DatabaseSide::Read)
    ) {
        return ReadinessState::Unavailable;
    }
    ReadinessState::Ready
}

pub fn projection_freshness_readiness(state: Option<&str>) -> ReadinessState {
    match state {
        None => ReadinessState::NotProbed,
        Some("activated") => ReadinessState::Ready,
        Some(_) => ReadinessState::Unavailable,
    }
}

pub fn backup_pitr_readiness(
    holding_id: &str,
    latest_backup: Option<&HoldingBackupRun>,
) -> ReadinessState {
    let Some(run) = latest_backup else {
        return ReadinessState::NotProbed;
    };
    if run.holding_id != holding_id
        || !matches!(run.kind.as_str(), "full" | "pitr" | "restore_test")
    {
        return ReadinessState::Unavailable;
    }
    if run.status == "completed" && run.verified && run.error.is_none() {
        ReadinessState::Ready
    } else {
        ReadinessState::Unavailable
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::Value;

    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 5, 10, 0, 0)
            .single()
            .expect("valid test timestamp")
    }

    fn database_health(
        holding_id: &str,
        side: DatabaseSide,
        database_key: &str,
        reachable: bool,
    ) -> HoldingDatabaseHealth {
        HoldingDatabaseHealth {
            holding_id: holding_id.into(),
            side,
            database_key: database_key.into(),
            schema_version: "1".into(),
            reachable,
            checked_at: now(),
            details: Value::Null,
        }
    }

    fn storage_health(holding_id: &str, reachable: bool) -> HoldingStorageHealth {
        HoldingStorageHealth {
            holding_id: holding_id.into(),
            storage_namespace: "holding-storage".into(),
            reachable,
            checked_at: now(),
            details: Value::Null,
        }
    }

    fn backup_run(holding_id: &str, status: &str, verified: bool) -> HoldingBackupRun {
        HoldingBackupRun {
            id: Uuid::from_u128(17),
            holding_id: holding_id.into(),
            kind: "full".into(),
            status: status.into(),
            backup_ref: "backup-chain-1".into(),
            restore_target_ref: Some("restore-target-1".into()),
            pitr_recovery_point: None,
            started_at: now(),
            completed_at: (status == "completed").then(now),
            verified,
            error: None,
        }
    }

    #[test]
    fn state_and_aggregation_fail_closed() {
        assert_eq!(ReadinessState::Ready.as_str(), "ready");
        assert_eq!(
            ReadinessState::from_status("unprobed"),
            ReadinessState::NotProbed
        );
        assert_eq!(
            ReadinessState::from_status("unknown"),
            ReadinessState::Unavailable
        );
        assert_eq!(aggregate_readiness_states([]), ReadinessState::NotProbed);
        assert_eq!(
            aggregate_readiness_states([ReadinessState::Ready, ReadinessState::NotProbed]),
            ReadinessState::NotProbed
        );
        assert_eq!(
            aggregate_readiness_states([ReadinessState::Ready, ReadinessState::Unavailable]),
            ReadinessState::Unavailable
        );
    }

    #[test]
    fn heartbeat_requires_recent_non_future_evidence() {
        let observed_at = now();
        assert_eq!(
            WorkerHeartbeatReadiness::from_last_seen(None, observed_at, Duration::seconds(10))
                .state,
            ReadinessState::NotProbed
        );
        assert_eq!(
            WorkerHeartbeatReadiness::from_last_seen(
                Some(observed_at - Duration::seconds(5)),
                observed_at,
                Duration::seconds(10),
            )
            .state,
            ReadinessState::Ready
        );
        assert_eq!(
            WorkerHeartbeatReadiness::from_last_seen(
                Some(observed_at + Duration::seconds(1)),
                observed_at,
                Duration::seconds(10),
            )
            .state,
            ReadinessState::Unavailable
        );
        assert_eq!(
            WorkerHeartbeatReadiness::from_last_seen(
                Some(observed_at),
                observed_at,
                Duration::seconds(-1),
            )
            .state,
            ReadinessState::Unavailable
        );
    }

    #[test]
    fn shared_heartbeat_starts_unprobed_and_observes_scheduler_marks() {
        let worker_id = Uuid::from_u128(7);
        let heartbeat = WorkerHeartbeat::new(worker_id);
        let observed_at = now();
        let missing = heartbeat.observe(observed_at, Duration::minutes(1));
        assert_eq!(missing.worker_id, worker_id);
        assert_eq!(missing.state, ReadinessState::NotProbed);

        assert!(heartbeat.record_at(observed_at - Duration::seconds(10)));
        assert_eq!(
            heartbeat.observe(observed_at, Duration::minutes(1)).state,
            ReadinessState::Ready
        );
        assert_eq!(
            heartbeat.observe(observed_at, Duration::seconds(5)).state,
            ReadinessState::Unavailable
        );
    }

    #[test]
    fn metadata_composition_preserves_existing_runtime_semantics() {
        let observed_at = now();
        let write = database_health("holding-test", DatabaseSide::Write, "holding-write", true);
        let read = database_health("holding-test", DatabaseSide::Read, "holding-read", true);
        let storage = storage_health("holding-test", true);
        let backup = backup_run("holding-test", "completed", true);
        let composition = WorkerReadinessComposition::from_metadata(
            WorkerHeartbeatReadiness::from_last_seen(
                Some(observed_at - Duration::seconds(1)),
                observed_at,
                Duration::seconds(30),
            ),
            WorkerReadinessMetadata {
                holding_id: "holding-test",
                registry_state: Some("active"),
                write_health: Some(&write),
                read_health: Some(&read),
                storage_health: Some(&storage),
                projection_state: Some("activated"),
                latest_backup: Some(&backup),
            },
        );
        assert!(composition.is_ready());
        assert_eq!(composition.overall(), ReadinessState::Ready);

        let failed = WorkerReadinessComposition {
            holding_routing: ReadinessState::Unavailable,
            ..composition
        };
        assert_eq!(failed.overall(), ReadinessState::Unavailable);
    }

    #[test]
    fn classifiers_do_not_promote_missing_or_failed_metadata() {
        let write = database_health("holding-test", DatabaseSide::Write, "holding-write", true);
        let read = database_health("holding-test", DatabaseSide::Read, "holding-read", true);
        let storage = storage_health("holding-test", true);
        assert_eq!(
            holding_routing_readiness(
                "holding-test",
                Some("active"),
                Some(&write),
                Some(&read),
                None,
            ),
            ReadinessState::NotProbed
        );
        assert_eq!(
            holding_routing_readiness(
                "holding-test",
                Some("active"),
                Some(&write),
                Some(&read),
                Some(&storage),
            ),
            ReadinessState::Ready
        );
        assert_eq!(
            projection_freshness_readiness(None),
            ReadinessState::NotProbed
        );
        assert_eq!(
            projection_freshness_readiness(Some("rebuilding")),
            ReadinessState::Unavailable
        );

        let completed = backup_run("holding-test", "completed", true);
        assert_eq!(
            backup_pitr_readiness("holding-test", Some(&completed)),
            ReadinessState::Ready
        );
        assert_eq!(
            backup_pitr_readiness("other-holding", Some(&completed)),
            ReadinessState::Unavailable
        );
    }
}
