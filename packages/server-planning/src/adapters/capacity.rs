//! PostgreSQL capacity reads for Planning.
//!
//! Routing operations and resources are read from their owner tables without
//! writing them. The caller's retained serializable Holding transaction is
//! required so the planner sees one snapshot; no commit is performed here.
use crate::application::capacity::{CapacityFuture, PlanningCapacityRepository};
use crate::domain::capacity::{CapacityCalendarDay, CapacityOperation};
use crate::domain::planning::source_invalid;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct PgPlanningCapacity<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgPlanningCapacity<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("PLANNING_CAPACITY_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }
}

impl PlanningCapacityRepository for PgPlanningCapacity<'_> {
    type Error = Error;

    fn operations<'a>(
        &'a mut self,
        specification_id: &'a str,
        company_id: &'a str,
    ) -> CapacityFuture<'a, Vec<CapacityOperation>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT o.id,o.name,o.resource_id,r.status,r.branch_id,o.setup_minutes,\
                 o.run_minutes_per_batch,r.work_weekdays_json,r.daily_minutes,\
                 r.version resource_version \
                 FROM manufacturing_specification_operations o \
                 JOIN production_resources r ON r.id=o.resource_id \
                 WHERE o.specification_id=$1 AND r.company_id=$2 ORDER BY o.ordinal",
            )
            .bind(specification_id)
            .bind(company_id)
            .fetch_all(&mut *self.connection)
            .await?;
            let mut operations = Vec::with_capacity(rows.len());
            for row in rows {
                let weekdays: String = row.try_get("work_weekdays_json")?;
                let work_weekdays: Vec<i64> =
                    serde_json::from_str(&weekdays).map_err(|_| source_invalid())?;
                if !(1..=7).contains(&work_weekdays.len()) {
                    return Err(source_invalid().into());
                }
                operations.push(CapacityOperation {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    resource_id: row.try_get("resource_id")?,
                    status: row.try_get("status")?,
                    branch_id: row.try_get("branch_id")?,
                    setup_minutes: row.try_get("setup_minutes")?,
                    run_minutes_per_batch: row.try_get("run_minutes_per_batch")?,
                    work_weekdays,
                    daily_minutes: row.try_get("daily_minutes")?,
                    resource_version: row.try_get("resource_version")?,
                });
            }
            Ok(operations)
        })
    }

    fn calendar<'a>(
        &'a mut self,
        resource_id: &'a str,
        work_date: &'a str,
    ) -> CapacityFuture<'a, Option<CapacityCalendarDay>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT available_minutes,version FROM production_resource_calendar \
                 WHERE resource_id=$1 AND work_date=$2",
            )
            .bind(resource_id)
            .bind(work_date)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(CapacityCalendarDay {
                    available_minutes: row.try_get("available_minutes")?,
                    version: row.try_get("version")?,
                })
            })
            .transpose()
        })
    }

    fn allocated_minutes<'a>(
        &'a mut self,
        resource_id: &'a str,
        work_date: &'a str,
    ) -> CapacityFuture<'a, i64, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let minutes: i64 = sqlx::query_scalar(
                "SELECT COALESCE(sum(minutes),0)::bigint FROM planning_resource_allocations \
                 WHERE resource_id=$1 AND work_date=$2 AND released_at IS NULL",
            )
            .bind(resource_id)
            .bind(work_date)
            .fetch_one(&mut *self.connection)
            .await?;
            Ok(minutes)
        })
    }
}
