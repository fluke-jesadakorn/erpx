//! Coordinator-owned G13 migration worker.
//!
//! The handler owns the control-plane worker boundary. The target executor
//! remains the authority for source-fenced target writes, read projection,
//! rollback rehearsal, reconciliation, and their outbox evidence; this
//! handler composes that executor with lease-loss detection and the durable
//! control read-model projection. It never changes command ownership.

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    sync::Arc,
    time::Duration as StdDuration,
};

use chrono::{Duration, Utc};
use erpx_infrastructure::{
    G13_EXECUTE_PERMISSION, G13AuthorizationProof, SqlxControlPlane, SqlxG13MigrationStore,
};
use erpx_worker::{
    G13TargetRuntimeHandler, NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError,
};
use serde_json::json;
use sqlx::Row;
use tokio::{
    sync::{OnceCell, oneshot},
    task::JoinHandle,
};
use uuid::Uuid;

pub(crate) const G13_MIGRATION_HANDLER_NAME: &str = "g13_migration";
const G13_WORK_LEASE_SECONDS: i64 = 5 * 60;

#[derive(Debug)]
struct LeaseSupervisor {
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<Result<(), String>>,
}

impl LeaseSupervisor {
    fn start(control: Arc<SqlxControlPlane>, worker_id: Uuid) -> Self {
        let (stop, mut stop_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(lease_poll_interval(StdDuration::from_secs(
                u64::try_from(G13_WORK_LEASE_SECONDS).unwrap_or(1),
            )));
            let mut known_claims = BTreeMap::new();
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        renew_claims(
                            &control,
                            worker_id,
                            &mut known_claims,
                            G13_WORK_LEASE_SECONDS,
                        ).await?;
                    }
                    _ = &mut stop_rx => return Ok(()),
                }
            }
        });
        Self {
            stop: Some(stop),
            task,
        }
    }

    async fn stop(mut self) -> Result<(), String> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        self.task
            .await
            .map_err(|error| format!("G13 lease supervisor stopped unexpectedly: {error}"))?
    }
}

async fn renew_claims(
    control: &SqlxControlPlane,
    worker_id: Uuid,
    known_claims: &mut BTreeMap<Uuid, Uuid>,
    lease_seconds: i64,
) -> Result<(), String> {
    let current = sqlx::query(
        "SELECT work_id, claim_id
         FROM erpx_control.migration_work_items
         WHERE worker_id=$1 AND state='claimed'",
    )
    .bind(worker_id)
    .fetch_all(&control.pools().write)
    .await
    .map_err(|error| format!("G13 lease inspection failed: {error}"))?;

    let current_claims = current
        .into_iter()
        .map(|row| {
            Ok::<_, sqlx::Error>((
                row.try_get::<Uuid, _>("work_id")?,
                row.try_get::<Uuid, _>("claim_id")?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
        .map_err(|error| format!("G13 lease inspection returned invalid claim data: {error}"))?;

    let known = known_claims.clone();
    for (work_id, claim_id) in known {
        match current_claims.get(&work_id) {
            Some(current_claim_id) if *current_claim_id == claim_id => {
                let renewed = sqlx::query(
                    "UPDATE erpx_control.migration_work_items
                     SET lease_until=now()+$1::interval, updated_at=now()
                     WHERE work_id=$2 AND worker_id=$3 AND claim_id=$4
                       AND state='claimed' AND lease_until > now()",
                )
                .bind(format!("{lease_seconds} seconds"))
                .bind(work_id)
                .bind(worker_id)
                .bind(claim_id)
                .execute(&control.pools().write)
                .await
                .map_err(|error| format!("G13 lease renewal failed: {error}"))?
                .rows_affected();
                if renewed != 1 {
                    let state = sqlx::query(
                        "SELECT state, last_error
                         FROM erpx_control.migration_work_items WHERE work_id=$1",
                    )
                    .bind(work_id)
                    .fetch_optional(&control.pools().write)
                    .await
                    .map_err(|error| format!("G13 lease-loss inspection failed: {error}"))?;
                    if claim_was_lost(state.as_ref()) {
                        record_lease_loss(control, worker_id, work_id, claim_id).await?;
                        return Err(format!(
                            "G13 work lease lost for work {work_id} and claim {claim_id}"
                        ));
                    }
                    known_claims.remove(&work_id);
                }
            }
            Some(_) | None => {
                let state = sqlx::query(
                    "SELECT state, last_error
                     FROM erpx_control.migration_work_items WHERE work_id=$1",
                )
                .bind(work_id)
                .fetch_optional(&control.pools().write)
                .await
                .map_err(|error| format!("G13 lease-loss inspection failed: {error}"))?;
                if claim_was_lost(state.as_ref()) {
                    record_lease_loss(control, worker_id, work_id, claim_id).await?;
                    return Err(format!(
                        "G13 work lease lost for work {work_id} and claim {claim_id}"
                    ));
                }
                known_claims.remove(&work_id);
            }
        }
    }
    known_claims.extend(current_claims);
    Ok(())
}

async fn record_lease_loss(
    control: &SqlxControlPlane,
    worker_id: Uuid,
    work_id: Uuid,
    claim_id: Uuid,
) -> Result<(), String> {
    let idempotency_key = format!("migration-work-lease-lost:{work_id}:{claim_id}");
    sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id, event_type, event_version, aggregate_type, aggregate_id,
             aggregate_version, actor, correlation_id, idempotency_key,
             payload, occurred_at)
         VALUES ($1,'migration.work_lease_lost',1,'migration_work_item',$2,1,
                 $3,$4,$5,$6,now())
         ON CONFLICT (idempotency_key) DO NOTHING",
    )
    .bind(Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        idempotency_key.as_bytes(),
    ))
    .bind(work_id)
    .bind(json!({"actor_ref": format!("worker:{worker_id}")}))
    .bind(Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("g13-lease-loss-correlation:{work_id}:{claim_id}").as_bytes(),
    ))
    .bind(idempotency_key)
    .bind(json!({
        "work_id": work_id,
        "claim_id": claim_id,
        "worker_id": worker_id,
        "reason": "lease ownership was no longer durable",
    }))
    .execute(&control.pools().write)
    .await
    .map_err(|error| format!("G13 lease-loss evidence could not be queued: {error}"))?;
    Ok(())
}

fn lease_poll_interval(lease: StdDuration) -> StdDuration {
    let seconds = lease.as_secs().saturating_div(3).clamp(1, 30);
    StdDuration::from_secs(seconds)
}

fn claim_was_lost(row: Option<&sqlx::postgres::PgRow>) -> bool {
    let Some(row) = row else {
        return true;
    };
    claim_state_was_lost(
        row.try_get::<String, _>("state").ok().as_deref(),
        row.try_get::<Option<String>, _>("last_error")
            .ok()
            .flatten()
            .as_deref(),
    )
}

fn claim_state_was_lost(state: Option<&str>, last_error: Option<&str>) -> bool {
    match state {
        Some("succeeded") | Some("pending") => false,
        Some("retryable") | Some("dead_letter") => last_error == Some("worker lease expired"),
        Some("claimed") => true,
        _ => true,
    }
}

#[derive(Clone)]
pub(crate) struct G13MigrationRuntimeHandler {
    store: SqlxG13MigrationStore,
    proof: G13AuthorizationProof,
    target: Arc<OnceCell<G13TargetRuntimeHandler>>,
}

impl G13MigrationRuntimeHandler {
    pub(crate) fn from_env(
        control: Arc<SqlxControlPlane>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mfa_verified = env::var("ERPX_G13_WORKER_MFA_VERIFIED")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if !mfa_verified {
            return Err("ERPX_G13_WORKER_MFA_VERIFIED=true is required".into());
        }
        let mut permissions = BTreeSet::from([G13_EXECUTE_PERMISSION.to_owned()]);
        if env::var("ERPX_G13_WORKER_CUTOVER_PERMISSION")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            permissions.insert(erpx_infrastructure::G13_CUTOVER_PERMISSION.to_owned());
        }
        let break_glass = env::var("ERPX_G13_WORKER_BREAK_GLASS")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let actor_ref = resolve_actor_ref(env::var("ERPX_G13_WORKER_ACTOR_REF").ok(), break_glass)?;
        let proof = G13AuthorizationProof {
            actor_ref,
            permissions,
            mfa_verified,
            break_glass,
            expires_at: Utc::now() + Duration::seconds(300),
            cutover_approval_ref: None,
        };
        proof
            .validate(Utc::now(), false)
            .map_err(|_| "G13 worker proof is invalid")?;
        Ok(Self {
            store: SqlxG13MigrationStore::new(control),
            proof,
            target: Arc::new(OnceCell::new()),
        })
    }
}

fn resolve_actor_ref(actor_ref: Option<String>, break_glass: bool) -> Result<String, &'static str> {
    match actor_ref {
        Some(actor_ref) if !actor_ref.trim().is_empty() => Ok(actor_ref),
        Some(_) => Err("ERPX_G13_WORKER_ACTOR_REF must not be blank"),
        None if break_glass => Ok("local:g13-worker".into()),
        None => {
            Err("ERPX_G13_WORKER_ACTOR_REF is required unless ERPX_G13_WORKER_BREAK_GLASS=true")
        }
    }
}

#[async_trait::async_trait]
impl NamedWorkerHandler for G13MigrationRuntimeHandler {
    fn name(&self) -> &str {
        G13_MIGRATION_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "G13 worker identity is required".into(),
            ));
        }
        self.proof
            .validate(Utc::now(), false)
            .map_err(|_| WorkerRuntimeError::Configuration("G13 worker proof expired".into()))?;
        let target = self
            .target
            .get_or_try_init(|| async {
                G13TargetRuntimeHandler::from_env(Arc::clone(self.store.control()))
                    .await
                    .map_err(|error| error.to_string())
            })
            .await
            .map_err(WorkerRuntimeError::Configuration)?;
        let supervisor = LeaseSupervisor::start(Arc::clone(self.store.control()), worker_id);
        let target_result = target.run_once(worker_id).await;
        let lease_result = supervisor.stop().await;
        let target_report = target_result?;
        if let Err(error) = lease_result {
            return Err(WorkerRuntimeError::Configuration(error));
        }
        let projected = self
            .store
            .project_read_model()
            .await
            .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
        Ok(WorkerHandlerReport {
            claimed: target_report.claimed,
            processed: target_report.processed.saturating_add(projected),
            published: target_report.published.saturating_add(projected),
            retried: target_report.retried,
            dead_lettered: target_report.dead_lettered,
            has_work: target_report.has_work || projected > 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{claim_state_was_lost, lease_poll_interval, resolve_actor_ref};

    #[test]
    fn command_owner_must_be_explicit_outside_local_break_glass() {
        assert!(resolve_actor_ref(None, false).is_err());
        assert_eq!(
            resolve_actor_ref(None, true).expect("break-glass fallback"),
            "local:g13-worker"
        );
        assert_eq!(
            resolve_actor_ref(Some("service:g13".into()), false).expect("explicit owner"),
            "service:g13"
        );
    }

    #[test]
    fn blank_command_owner_never_falls_back() {
        assert!(resolve_actor_ref(Some("  ".into()), true).is_err());
    }

    #[test]
    fn lease_polling_is_bounded_to_keep_a_second_worker_out_of_an_expired_claim() {
        assert_eq!(
            lease_poll_interval(Duration::from_secs(300)),
            Duration::from_secs(30)
        );
        assert_eq!(
            lease_poll_interval(Duration::from_secs(2)),
            Duration::from_secs(1)
        );
        assert_eq!(
            lease_poll_interval(Duration::from_secs(600)),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn normal_worker_retry_is_not_misreported_as_lease_loss() {
        assert!(!claim_state_was_lost(
            Some("retryable"),
            Some("target adapter unavailable")
        ));
        assert!(claim_state_was_lost(
            Some("retryable"),
            Some("worker lease expired")
        ));
        assert!(claim_state_was_lost(Some("claimed"), None));
        assert!(!claim_state_was_lost(Some("succeeded"), None));
        assert!(claim_state_was_lost(None, None));
    }
}
