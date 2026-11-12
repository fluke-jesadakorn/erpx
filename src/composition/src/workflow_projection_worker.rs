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
    EventMessage, EventPayload, PMS_SOCIAL_EVENTS, PmsSocialFuture, PmsSocialNotice,
    PmsSocialOriginal, PmsSocialPorts, PmsSocialPost, PmsSocialSource, ProjectionOwnerSources,
    ProjectionResult, ProjectionSource, ProjectionSourceFuture, project_event, project_pms_social,
};
use sqlx::{Connection, PgConnection, PgPool};
use std::sync::Arc;

pub(crate) fn owned_predicate() -> String {
    let pms_social_types = PMS_SOCIAL_EVENTS
        .iter()
        .map(|(event_type, _)| format!("'{event_type}'"))
        .collect::<Vec<_>>()
        .join(",");
    let families = format!(
        "(e.event_type LIKE 'task.%' OR e.event_type = 'routine.run' \
         OR (e.event_type LIKE 'leave.%' AND e.event_type NOT LIKE 'leave.policy.%') \
         OR (e.event_type LIKE 'overtime.%' AND e.event_type <> 'overtime.eligibility') \
         OR e.event_type LIKE 'people.payroll.%' \
         OR e.event_type IN ({pms_social_types}))"
    );
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

struct CompositionPmsSocialPorts;

impl PmsSocialPorts for CompositionPmsSocialPorts {
    fn original<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        event_id: &'a str,
        event_type: &'a str,
    ) -> PmsSocialFuture<'a, Option<PmsSocialOriginal>> {
        Box::pin(async move {
            erpx_server_workflow::public::job_projector::pms_social_original(
                connection,
                event_id,
                event_type,
            )
            .await
        })
    }

    fn payout_source<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        id: &'a str,
        property_id: &'a str,
    ) -> PmsSocialFuture<'a, Option<String>> {
        Box::pin(async move {
            erpx_server_pms::public::pms_social_event::pms_social_payout_source(
                connection,
                id,
                property_id,
            )
            .await
        })
    }

    fn source<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        id: Option<&'a str>,
        company_id: &'a str,
        property_id: &'a str,
    ) -> PmsSocialFuture<'a, Option<PmsSocialSource>> {
        Box::pin(async move {
            Ok(
                erpx_server_pms::public::pms_social_event::pms_social_source(
                    connection,
                    id,
                    company_id,
                    property_id,
                )
                .await?
                .map(|source| PmsSocialSource {
                    id: source.id,
                    kind: source.kind,
                    company_id: source.company_id,
                    branch_id: source.branch_id,
                    property_name: source.property_name,
                    business_date: source.business_date,
                    data_json: source.data_json,
                    version: source.version,
                }),
            )
        })
    }

    fn lock<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        post_id: &'a str,
    ) -> PmsSocialFuture<'a, ()> {
        Box::pin(async move {
            erpx_server_collaboration::public::pms_social_event::pms_social_lock(
                connection, post_id,
            )
            .await
        })
    }

    fn received<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        event_id: &'a str,
    ) -> PmsSocialFuture<'a, bool> {
        Box::pin(async move {
            erpx_server_collaboration::public::pms_social_event::pms_social_received(
                connection, event_id,
            )
            .await
        })
    }

    fn publish<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        post: PmsSocialPost,
        source_id: &'a str,
        event_id: &'a str,
        received_at: i64,
    ) -> PmsSocialFuture<'a, ()> {
        Box::pin(async move {
            erpx_server_collaboration::public::pms_social_event::pms_social_publish(
                connection,
                erpx_server_collaboration::public::pms_social_event::PmsSocialPost {
                    post_id: post.post_id,
                    company_id: post.company_id,
                    branch_id: post.branch_id,
                    actor_id: post.actor_id,
                    body: post.body,
                    command_id: post.command_id,
                    at: post.at,
                },
                source_id,
                event_id,
                received_at,
            )
            .await
        })
    }

    fn recipients<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        id: &'a str,
        company_id: &'a str,
        actor_id: &'a str,
        review: Option<&'a str>,
        recipient: Option<&'a str>,
    ) -> PmsSocialFuture<'a, Vec<String>> {
        Box::pin(async move {
            erpx_server_identity_access::public::pms_social_event_recipients::pms_social_recipients(
                connection,
                id,
                company_id,
                actor_id,
                review,
                recipient,
            )
            .await
        })
    }

    fn version<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        post_id: &'a str,
    ) -> PmsSocialFuture<'a, Option<i64>> {
        Box::pin(async move {
            erpx_server_collaboration::public::pms_social_event::pms_social_version(
                connection, post_id,
            )
            .await
        })
    }

    fn notify<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        notice: PmsSocialNotice,
    ) -> PmsSocialFuture<'a, ()> {
        Box::pin(async move {
            erpx_server_collaboration::public::pms_social_event::pms_social_notify(
                connection,
                erpx_server_collaboration::public::pms_social_event::PmsSocialNotice {
                    id: notice.id,
                    company_id: notice.company_id,
                    user_id: notice.user_id,
                    post_id: notice.post_id,
                    version: notice.version,
                    body: notice.body,
                    at: notice.at,
                },
            )
            .await
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
            if message.event_type.starts_with("pms.") {
                let mut pms_ports = CompositionPmsSocialPorts;
                match project_pms_social(&mut unit, &mut pms_ports, &message, now).await {
                    Ok(Some(())) => {
                        unit.commit()
                            .await
                            .map_err(|_| worker_failure("PROJECTION_COMMIT_FAILED"))?;
                        return Ok(WorkerOutcome::Complete);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        unit.rollback()
                            .await
                            .map_err(|_| worker_failure("PROJECTION_ROLLBACK_FAILED"))?;
                        return Ok(WorkerOutcome::Fail(error.code));
                    }
                }
            }
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
