//! Native Workflow projection outbox handler and worker entrypoint.
//!
//! The native supervisor owns only the projection families that are ported
//! (task/routine, leave, overtime, payroll); every other event family stays
//! with the retained TS supervisor through the static `owned` predicate.
use erpx_server_operations::public::workflow_projection_source::task_projection_source;
use erpx_server_people_payroll::public::workflow_projection_source::{
    PeopleProjectionKind, people_projection_source,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationFile, MigrationTrack, OutboxAttestation, OutboxHolding,
    OutboxHandlerRegistry, OutboxJob, OutboxJobHandler, OutboxSupervisor, OutboxSupervisorPolicy,
    WorkerFuture, WorkerOutcome, attest_outbox_holding, record_outbox_attestation,
    run_outbox_supervisor, worker_failure,
};
use erpx_server_shared::public::Clock;
use erpx_server_workflow::public::job_projector::{
    EventMessage, EventPayload, ProjectionOwnerSources, ProjectionResult, ProjectionSource,
    ProjectionSourceFuture, project_event,
};
use sqlx::{Connection, PgConnection, PgPool};
use std::sync::Arc;

pub(crate) fn owned_predicate() -> String {
    let families = "(e.event_type LIKE 'task.%' OR e.event_type = 'routine.run' \
         OR (e.event_type LIKE 'leave.%' AND e.event_type NOT LIKE 'leave.policy.%') \
         OR (e.event_type LIKE 'overtime.%' AND e.event_type <> 'overtime.eligibility') \
         OR e.event_type LIKE 'people.payroll.%')";
    let quarantine = format!(
        "({}) OR ({}) OR ({}) OR ({}) OR ({}) OR ({})",
        erpx_server_field_service::public::field_outbox::QUARANTINE,
        erpx_server_operations::public::field_outbox::QUARANTINE,
        erpx_server_collaboration::public::field_outbox::QUARANTINE,
        erpx_server_files::public::field_outbox::QUARANTINE,
        erpx_server_finance::public::field_outbox::QUARANTINE,
        erpx_server_inventory::public::service_count_outbox::SERVICE_COUNT_OUTBOX_QUARANTINE,
    );
    format!("(({families}) AND NOT (({quarantine})))")
}

struct OwnerSources<'a> {
    write: &'a mut PgConnection,
}

fn people_fact(fact: erpx_server_people_payroll::public::workflow_projection_source::PeopleProjectionSource) -> ProjectionSource {
    ProjectionSource::new(
        fact.state,
        fact.version,
        fact.company_id,
        fact.branch_id,
        fact.department_id,
    )
}

impl ProjectionOwnerSources for OwnerSources<'_> {
    fn task<'a>(&'a mut self, id: &'a str) -> ProjectionSourceFuture<'a> {
        let connection = &mut *self.write;
        Box::pin(async move {
            Ok(
                task_projection_source(connection, id)
                    .await?
                    .map(|fact| {
                        ProjectionSource::new(
                            fact.state,
                            fact.version,
                            fact.company_id,
                            fact.branch_id,
                            fact.department_id,
                        )
                    }),
            )
        })
    }

    fn leave<'a>(&'a mut self, id: &'a str) -> ProjectionSourceFuture<'a> {
        let connection = &mut *self.write;
        Box::pin(async move {
            Ok(people_projection_source(connection, PeopleProjectionKind::Leave, id)
                .await?
                .map(people_fact))
        })
    }

    fn overtime<'a>(&'a mut self, id: &'a str) -> ProjectionSourceFuture<'a> {
        let connection = &mut *self.write;
        Box::pin(async move {
            Ok(people_projection_source(connection, PeopleProjectionKind::Overtime, id)
                .await?
                .map(people_fact))
        })
    }

    fn payroll<'a>(&'a mut self, id: &'a str) -> ProjectionSourceFuture<'a> {
        let connection = &mut *self.write;
        Box::pin(async move {
            Ok(people_projection_source(connection, PeopleProjectionKind::Payroll, id)
                .await?
                .map(people_fact))
        })
    }
}

struct WorkflowProjectionHandler {
    holding_id: String,
}

impl OutboxJobHandler for WorkflowProjectionHandler {
    fn handle<'a>(&'a self, write: &'a PgPool, job: &'a OutboxJob, now: i64) -> WorkerFuture<'a> {
        Box::pin(async move {
            let payload: EventPayload = serde_json::from_str(&job.payload_json)
                .map_err(|_| worker_failure("PROJECTION_PAYLOAD_INVALID"))?;
            let message = EventMessage {
                holding_id: self.holding_id.clone(),
                id: job.id.clone(),
                event_type: job.event_type.clone(),
                aggregate_id: job.aggregate_id.clone(),
                aggregate_version: job.aggregate_version,
                payload,
            };
            let mut sources_connection = write
                .acquire()
                .await
                .map_err(|_| worker_failure("PROJECTION_DB_UNAVAILABLE"))?;
            let mut projection_connection = write
                .acquire()
                .await
                .map_err(|_| worker_failure("PROJECTION_DB_UNAVAILABLE"))?;
            let mut sources = OwnerSources {
                write: &mut sources_connection,
            };
            let unit = &mut *projection_connection;
            let mut unit = unit.begin().await
                .map_err(|_| worker_failure("PROJECTION_DB_UNAVAILABLE"))?;
            match project_event(&mut unit, &mut sources, &message, now).await {
                Ok(ProjectionResult::Projected) => {
                    unit.commit()
                        .await
                        .map_err(|_| worker_failure("PROJECTION_COMMIT_FAILED"))?;
                    Ok(WorkerOutcome::Complete)
                }
                Ok(ProjectionResult::Unhandled) => {
                    unit.rollback()
                        .await
                        .map_err(|_| worker_failure("PROJECTION_ROLLBACK_FAILED"))?;
                    Ok(WorkerOutcome::Fail("PROJECTION_UNHANDLED"))
                }
                Err(error) => {
                    unit.rollback()
                        .await
                        .map_err(|_| worker_failure("PROJECTION_ROLLBACK_FAILED"))?;
                    Ok(WorkerOutcome::Fail(error.code))
                }
            }
        })
    }
}

/// Build and run the native projection supervisor for every registered
/// Holding. The caller owns process supervision and restart policy.
pub(crate) async fn run_workflow_projection_supervisor(
    holdings: Vec<(String, DatabasePool, OutboxAttestation)>,
    clock: Arc<dyn Clock + Send + Sync>,
    shutdown: impl std::future::Future<Output = ()> + Send,
) -> Result<(), &'static str> {
    let owned = owned_predicate();
    let mut registry = OutboxHandlerRegistry::new();
    let mut targets = Vec::new();
    for (holding_id, pool, proof) in holdings {
        registry
            .register(
                &holding_id,
                Arc::new(WorkflowProjectionHandler {
                    holding_id: holding_id.clone(),
                }),
            )
            .map_err(|_| "WORKFLOW_PROJECTION_HANDLER_INVALID")?;
        targets.push(
            OutboxHolding::new(&holding_id, pool.raw().clone(), &owned, proof)
                .map_err(|_| "WORKFLOW_PROJECTION_ATTESTATION_INVALID")?,
        );
    }
    let supervisor = OutboxSupervisor::new(targets, &registry, OutboxSupervisorPolicy::default())
        .map_err(|_| "WORKFLOW_PROJECTION_SUPERVISOR_INVALID")?;
    run_outbox_supervisor(supervisor, clock, shutdown)
        .await
        .map_err(|_| "WORKFLOW_PROJECTION_WORKER_FAILED")?;
    Ok(())
}

/// Attest one Holding writer before it can project.
pub(crate) async fn attest_projection_holding(
    pool: &DatabasePool,
    holding_id: &str,
    expected_database: Option<&str>,
    files: &[MigrationFile],
) -> Result<OutboxAttestation, &'static str> {
    attest_outbox_holding(
        pool,
        holding_id,
        expected_database,
        MigrationTrack::HoldingWrite,
        files,
    )
    .await
    .map_err(|_| "WORKFLOW_PROJECTION_SCHEMA_MISMATCH")
}

/// Persist the supervisor attestation row in Control. Kept here so the worker
/// entrypoint and production supervisor share one implementation.
pub(crate) async fn record_projection_attestation(
    control: &DatabasePool,
    holdings: Vec<(String, DatabasePool, OutboxAttestation)>,
    clock: Arc<dyn Clock + Send + Sync>,
) -> Result<(), &'static str> {
    let owned = owned_predicate();
    let mut registry = OutboxHandlerRegistry::new();
    let mut targets = Vec::new();
    for (holding_id, pool, proof) in holdings {
        registry
            .register(
                &holding_id,
                Arc::new(WorkflowProjectionHandler {
                    holding_id: holding_id.clone(),
                }),
            )
            .map_err(|_| "WORKFLOW_PROJECTION_HANDLER_INVALID")?;
        targets.push(
            OutboxHolding::new(&holding_id, pool.raw().clone(), &owned, proof)
                .map_err(|_| "WORKFLOW_PROJECTION_ATTESTATION_INVALID")?,
        );
    }
    let supervisor = OutboxSupervisor::new(targets, &registry, OutboxSupervisorPolicy::default())
        .map_err(|_| "WORKFLOW_PROJECTION_SUPERVISOR_INVALID")?;
    record_outbox_attestation(
        control.raw(),
        "workflow-projection",
        &supervisor,
        clock.now().get(),
    )
    .await
    .map_err(|_| "WORKFLOW_PROJECTION_ATTESTATION_FAILED")
}
