//! Native scheduled Staff Work alert sweep. The Operations owner application
//! (`escalate_staff_tasks`) plans and stages the same due-soon/overdue alerts as
//! the retired TS worker, on a caller-owned serializable unit.
use erpx_identity_access_server::public::SystemAccountClock;
use erpx_operations_server::public::staff_tasks::{
    PgStaffTaskSources, PgStaffTasks, ProjectFact, StaffTaskFactDelegates, StaffTaskFuture,
    StaffTaskRepository, escalate_staff_tasks,
};
use erpx_projects_server::public::staff_facts::staff_project;
use erpx_platform_runtime_server::public::postgres::{DatabasePool, PoolLimits};
use erpx_platform_shared_server::public::{BusinessError, Clock, FailureKind};
use sqlx::{Connection, PgConnection, PgPool};
use std::time::Duration;

struct AlertSources {
    production: bool,
}

impl StaffTaskFactDelegates for AlertSources {
    fn feature_enabled<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
    ) -> StaffTaskFuture<'a, bool, BusinessError> {
        Box::pin(async move {
            if !self.production {
                return Ok(true);
            }
            let connection = unit.ok_or_else(|| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "TASK_PMS_FEATURE_SOURCE_REQUIRED",
                    "Staff Work requires the PMS feature-availability port.",
                )
            })?;
            // The artifact is unbuilt: acceptedReleasePredicate is FALSE, so
            // the effective TS predicate is rollout_state='pilot' only.
            let enabled: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM company_feature_rollouts \
                 WHERE company_id=$1 AND module_id='operations' AND rollout_state='pilot')",
            )
            .bind(company_id)
            .fetch_one(connection)
            .await
            .map_err(|_| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "TASK_PMS_FEATURE_UNAVAILABLE",
                    "The PMS feature-availability port is unavailable.",
                )
            })?;
            Ok(enabled)
        })
    }

    fn project<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        project_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<ProjectFact>, BusinessError> {
        Box::pin(async move {
            let connection = unit.ok_or_else(|| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "TASK_PROJECT_SOURCE_REQUIRED",
                    "Staff Work requires the Projects port.",
                )
            })?;
            Ok(staff_project(connection, company_id, project_id)
                .await?
                .map(|fact| ProjectFact {
                    owner_user_id: fact.owner_user_id,
                    title: fact.title,
                }))
        })
    }
}

/// One serializable sweep of the caller's Holding database.
pub async fn run_once(write: &PgPool, now: i64) -> Result<(u64, u64), BusinessError> {
    let production = std::env::var("NODE_ENV").ok().as_deref() == Some("production");
    let mut tx = write.begin().await.map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "STAFF_TASK_ALERTS_UNAVAILABLE",
            "The scheduled Staff Work alert sweep is unavailable.",
        )
    })?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "STAFF_TASK_ALERTS_UNAVAILABLE",
                "The scheduled Staff Work alert sweep is unavailable.",
            )
        })?;
    let mut repository = PgStaffTasks::new(&mut *tx);
    let mut sources = PgStaffTaskSources::new(AlertSources { production });
    let result = escalate_staff_tasks(&mut repository, &mut sources, now, 86_400_000)
        .await
        .map_err(|error| match error {
            erpx_platform_runtime_server::public::postgres::PgTransactionError::Business(error) => error,
            erpx_platform_runtime_server::public::postgres::PgTransactionError::Persistence(_) => {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "STAFF_TASK_ALERTS_UNAVAILABLE",
                    "The scheduled Staff Work alert sweep is unavailable.",
                )
            }
        })?;
    drop(repository);
    tx.commit().await.map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "STAFF_TASK_ALERTS_UNAVAILABLE",
            "The scheduled Staff Work alert sweep is unavailable.",
        )
    })?;
    Ok((result.due_soon.max(0) as u64, result.overdue.max(0) as u64))
}

#[cfg(feature = "private-field-api")]
pub(crate) async fn run(
    databases: Vec<crate::runtime_database::RuntimeDatabase>,
    mut stop: tokio::sync::watch::Receiver<bool>,
) {
    let limits = PoolLimits {
        max_connections: 2,
        acquire_timeout: Duration::from_secs(5),
    };
    let mut pools = Vec::new();
    for database in databases {
        match database.connect(limits).await {
            Ok(pool) => pools.push(pool),
            Err(_) => {
                eprintln!("STAFF_TASK_ALERTS_UNAVAILABLE");
                return;
            }
        }
    }
    loop {
        if *stop.borrow() {
            return;
        }
        let now = SystemAccountClock.now().get();
        for pool in &pools {
            if run_once(pool.raw(), now).await.is_err() {
                eprintln!("STAFF_TASK_ALERTS_UNAVAILABLE");
            }
        }
        tokio::select! {
            _ = stop.changed() => return,
            _ = tokio::time::sleep(Duration::from_secs(120)) => {}
        }
    }
}
