//! One iteration of the shared Service Count projection worker.
//! Composition supplies the static Inventory-owned claim statement.
use erpx_server_runtime::public::postgres::{
    OutboxJob, OutboxJobHandler, WorkerFuture, WorkerOutcome, WorkerSupervisor, WorkerTarget,
    worker_failure,
};
use erpx_server_workflow::public::service_count_outbox::{
    ServiceCountEvent, claim, complete, eligible, fail, project, renew,
};
use sqlx::PgPool;
use std::{sync::Arc, time::Instant};

/// Commit the Holding read projection before acknowledging the write-side lease.
/// A crash between the two commits is recovered through the idempotent receipt.
pub async fn run_once(
    write: &PgPool,
    read: &PgPool,
    token: &str,
    now: i64,
) -> Result<bool, sqlx::Error> {
    run_once_with_scope(write, read, token, now, COUNT_SCOPE).await
}
#[derive(Clone, Copy)]
pub(crate) struct ProjectionScope<'a> {
    pub quarantine: &'a str,
    pub eligible: &'a str,
    pub ambiguous: &'a str,
}
pub(crate) const COUNT_SCOPE: ProjectionScope<'static> = ProjectionScope {
    quarantine:
        erpx_server_inventory::public::service_count_outbox::SERVICE_COUNT_OUTBOX_QUARANTINE,
    eligible: erpx_server_inventory::public::service_count_outbox::SERVICE_COUNT_OUTBOX_ELIGIBLE,
    ambiguous: erpx_server_inventory::public::service_count_outbox::SERVICE_COUNT_OUTBOX_AMBIGUOUS,
};
pub(crate) async fn run_once_with_scope(
    write: &PgPool,
    read: &PgPool,
    token: &str,
    now: i64,
    scope: ProjectionScope<'_>,
) -> Result<bool, sqlx::Error> {
    let mut connection = write.acquire().await?;
    let Some(event) = claim(&mut connection, scope.quarantine, token, now).await? else {
        return Ok(false);
    };
    match eligible(&mut connection, scope.eligible, &event).await {
        Ok(true) => {}
        Ok(false) => {
            let ambiguous = match eligible(&mut connection, scope.ambiguous, &event).await {
                Ok(value) => value,
                Err(error) => {
                    fail(&mut connection, &event, now, "SOURCE_CHECK_FAILED").await?;
                    return Err(error);
                }
            };
            fail(
                &mut connection,
                &event,
                now,
                if ambiguous {
                    "SOURCE_AMBIGUOUS"
                } else {
                    "SOURCE_INVALID"
                },
            )
            .await?;
            return Ok(true);
        }
        Err(error) => {
            fail(&mut connection, &event, now, "SOURCE_CHECK_FAILED").await?;
            return Err(error);
        }
    }
    drop(connection);
    let start = Instant::now();
    let projection = async {
        let mut transaction = read.begin().await?;
        project(&mut transaction, &event, now).await?;
        transaction.commit().await
    };
    tokio::pin!(projection);
    let mut timer = tokio::time::interval(std::time::Duration::from_secs(15));
    timer.tick().await;
    let result = loop {
        tokio::select! {
            result=&mut projection => break result,
            _=timer.tick() => {
                let mut c=write.acquire().await?;
                let clock=now.saturating_add(i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX));
                if !renew(&mut c,&event,clock).await? { return Err(sqlx::Error::RowNotFound); }
            }
        }
    };
    let mut connection = write.acquire().await?;
    let clock = now.saturating_add(i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX));
    match result {
        Ok(()) => {
            if !complete(&mut connection, &event, clock).await? {
                return Err(sqlx::Error::RowNotFound);
            }
        }
        Err(error) => {
            fail(&mut connection, &event, clock, "PROJECTION_FAILED").await?;
            return Err(error);
        }
    }
    Ok(true)
}

/// Drain all registered Holding pairs on one worker task. The caller validates
/// both leased databases and their migration tracks before passing the pools.
pub async fn run(
    pairs: Vec<(PgPool, PgPool)>,
    shutdown: impl std::future::Future<Output = ()> + Send,
) {
    run_with_scope(pairs, shutdown, COUNT_SCOPE).await;
}
struct ProjectionHandler {
    read: PgPool,
    eligible: String,
    ambiguous: String,
}
impl OutboxJobHandler for ProjectionHandler {
    fn handle<'a>(&'a self, write: &'a PgPool, job: &'a OutboxJob, now: i64) -> WorkerFuture<'a> {
        Box::pin(async move {
            let event = ServiceCountEvent {
                id: job.id.clone(),
                aggregate_id: job.aggregate_id.clone(),
                aggregate_version: job.aggregate_version,
                attempts: job.attempts,
                lease_token: job.lease_token.clone(),
            };
            let mut connection = write
                .acquire()
                .await
                .map_err(|_| worker_failure("PROJECTION_DB_UNAVAILABLE"))?;
            match eligible(&mut connection, &self.eligible, &event).await {
                Ok(true) => {}
                Ok(false) => {
                    let ambiguous = eligible(&mut connection, &self.ambiguous, &event)
                        .await
                        .map_err(|_| worker_failure("SOURCE_CHECK_FAILED"))?;
                    return Ok(WorkerOutcome::Fail(if ambiguous {
                        "SOURCE_AMBIGUOUS"
                    } else {
                        "SOURCE_INVALID"
                    }));
                }
                Err(_) => return Err(worker_failure("SOURCE_CHECK_FAILED")),
            }
            drop(connection);
            let mut transaction = self
                .read
                .begin()
                .await
                .map_err(|_| worker_failure("PROJECTION_DB_UNAVAILABLE"))?;
            project(&mut transaction, &event, now)
                .await
                .map_err(|_| worker_failure("PROJECTION_FAILED"))?;
            transaction
                .commit()
                .await
                .map_err(|_| worker_failure("PROJECTION_FAILED"))?;
            Ok(WorkerOutcome::Complete)
        })
    }
}

pub(crate) async fn run_with_scope(
    pairs: Vec<(PgPool, PgPool)>,
    shutdown: impl std::future::Future<Output = ()> + Send,
    scope: ProjectionScope<'_>,
) {
    let targets = pairs
        .iter()
        .enumerate()
        .map(|(index, (write, read))| WorkerTarget {
            name: format!("service-count-{index}"),
            write: write.clone(),
            owned: scope.quarantine.to_owned(),
            handler: Arc::new(ProjectionHandler {
                read: read.clone(),
                eligible: scope.eligible.to_owned(),
                ambiguous: scope.ambiguous.to_owned(),
            }),
        })
        .collect::<Vec<_>>();
    let Ok(supervisor) = WorkerSupervisor::new(targets, std::time::Duration::from_millis(10))
        .and_then(|value| value.with_lease_refresh(std::time::Duration::from_secs(15)))
    else {
        return;
    };
    tokio::pin!(shutdown);
    loop {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        let worked = tokio::select! {
            _ = &mut shutdown => break,
            result = supervisor.run_once(now) => match result {
                Ok(completed) => completed > 0,
                Err(_) => {
                    eprintln!("scoped outbox projection iteration failed");
                    false
                }
            }
        };
        tokio::select! {
            _ = &mut shutdown => break,
            _ = tokio::time::sleep(std::time::Duration::from_millis(if worked { 10 } else { 1000 })) => {},
        }
    }
}
