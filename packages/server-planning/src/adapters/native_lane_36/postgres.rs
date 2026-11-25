//! sqlx execution for the native Planning lane-36 scoped workspace reads.
//!
//! Every statement below is owner-authored and fixed by the closed read kind;
//! every value is bound. Only Planning-owned relations are read and the
//! Company/branch predicate is mandatory, so a caller can never widen scope or
//! inject SQL. The TypeScript tails these statements replace are deleted.

use super::application::{LaneRead, LaneReadFuture};
use super::domain::{
    CalendarListItem, LaneData, LaneScope, RecommendationListItem, ResourceListItem,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, to_value};
use sqlx::Row;

/// Scoped recommendations: parent scenario owns the Company/branch identity.
pub const RECOMMENDATIONS_SQL: &str = "SELECT r.id,r.scenario_id,r.scenario_version,r.recommended_quantity_milli,r.required_minutes,r.feasible_date,r.status,r.created_by,r.reviewed_by,r.version,r.created_at,r.updated_at,s.company_id,s.branch_id FROM planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id WHERE s.company_id=$1 AND ($2::text IS NULL OR s.branch_id=$3) ORDER BY r.updated_at DESC";

/// Scoped production resources.
pub const RESOURCES_SQL: &str = "SELECT id,company_id,branch_id,code,name,daily_minutes,work_weekdays_json,status,version,created_by,created_at,updated_at FROM production_resources WHERE company_id=$1 AND ($2::text IS NULL OR branch_id=$3) ORDER BY name";

/// Scoped resource calendar, bounded to 500 rows as in the retired TS tail.
pub const CALENDAR_SQL: &str = "SELECT c.resource_id,c.work_date,c.available_minutes,c.reason,c.version,c.updated_by,c.updated_at FROM production_resource_calendar c JOIN production_resources r ON r.id=c.resource_id WHERE r.company_id=$1 AND ($2::text IS NULL OR r.branch_id=$3) ORDER BY c.work_date DESC LIMIT 500";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLANNING_LANE36_READ_UNAVAILABLE",
        "The native Planning scoped read is unavailable.",
    )
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PLANNING_LANE36_ROW_INVALID",
        "The Planning owner returned an invalid scoped row.",
    )
}

/// The single statement for a closed kind. No other discriminator resolves.
pub fn sql_for(kind: &str) -> Option<&'static str> {
    match kind {
        "recommendations" => Some(RECOMMENDATIONS_SQL),
        "resources" => Some(RESOURCES_SQL),
        "calendar" => Some(CALENDAR_SQL),
        _ => None,
    }
}

fn map_recommendation(row: &sqlx::postgres::PgRow) -> Result<Value, BusinessError> {
    let item = RecommendationListItem {
        id: row.try_get("id").map_err(|_| invalid())?,
        scenario_id: row.try_get("scenario_id").map_err(|_| invalid())?,
        scenario_version: row.try_get("scenario_version").map_err(|_| invalid())?,
        recommended_quantity_milli: row
            .try_get("recommended_quantity_milli")
            .map_err(|_| invalid())?,
        required_minutes: row.try_get("required_minutes").map_err(|_| invalid())?,
        feasible_date: row.try_get("feasible_date").map_err(|_| invalid())?,
        status: row.try_get("status").map_err(|_| invalid())?,
        created_by: row.try_get("created_by").map_err(|_| invalid())?,
        reviewed_by: row.try_get("reviewed_by").map_err(|_| invalid())?,
        version: row.try_get("version").map_err(|_| invalid())?,
        created_at: row.try_get("created_at").map_err(|_| invalid())?,
        updated_at: row.try_get("updated_at").map_err(|_| invalid())?,
        company_id: row.try_get("company_id").map_err(|_| invalid())?,
        branch_id: row.try_get("branch_id").map_err(|_| invalid())?,
    };
    to_value(item).map_err(|_| invalid())
}

fn map_resource(row: &sqlx::postgres::PgRow) -> Result<Value, BusinessError> {
    let item = ResourceListItem {
        id: row.try_get("id").map_err(|_| invalid())?,
        company_id: row.try_get("company_id").map_err(|_| invalid())?,
        branch_id: row.try_get("branch_id").map_err(|_| invalid())?,
        code: row.try_get("code").map_err(|_| invalid())?,
        name: row.try_get("name").map_err(|_| invalid())?,
        daily_minutes: row.try_get("daily_minutes").map_err(|_| invalid())?,
        work_weekdays_json: row.try_get("work_weekdays_json").map_err(|_| invalid())?,
        status: row.try_get("status").map_err(|_| invalid())?,
        version: row.try_get("version").map_err(|_| invalid())?,
        created_by: row.try_get("created_by").map_err(|_| invalid())?,
        created_at: row.try_get("created_at").map_err(|_| invalid())?,
        updated_at: row.try_get("updated_at").map_err(|_| invalid())?,
    };
    to_value(item).map_err(|_| invalid())
}

fn map_calendar(row: &sqlx::postgres::PgRow) -> Result<Value, BusinessError> {
    let item = CalendarListItem {
        resource_id: row.try_get("resource_id").map_err(|_| invalid())?,
        work_date: row.try_get("work_date").map_err(|_| invalid())?,
        available_minutes: row.try_get("available_minutes").map_err(|_| invalid())?,
        reason: row.try_get("reason").map_err(|_| invalid())?,
        version: row.try_get("version").map_err(|_| invalid())?,
        updated_by: row.try_get("updated_by").map_err(|_| invalid())?,
        updated_at: row.try_get("updated_at").map_err(|_| invalid())?,
    };
    to_value(item).map_err(|_| invalid())
}

/// Pg-backed owner reader. Composition supplies the pool; no other owner's
/// table, permission or purchase decision is inferred from these reads.
pub struct PgLaneRead {
    pool: DatabasePool,
}

impl PgLaneRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl LaneRead for PgLaneRead {
    fn read<'a>(&'a self, scope: &'a LaneScope) -> LaneReadFuture<'a, LaneData> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let scope = scope.clone();
        Box::pin(async move {
            let statement = sql_for(&scope.kind).ok_or_else(unavailable)?;
            let company = scope.company_id.clone();
            let branch = scope.branch.clone();
            let kind = scope.kind.clone();
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let branch = branch.clone();
                let kind = kind.clone();
                Box::pin(async move {
                    let rows = sqlx::query(statement)
                        .bind(company.as_str())
                        .bind(branch.as_deref())
                        .bind(branch.as_deref())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let mut values = Vec::with_capacity(rows.len());
                    for row in &rows {
                        let value = match kind.as_str() {
                            "recommendations" => map_recommendation(row),
                            "resources" => map_resource(row),
                            "calendar" => map_calendar(row),
                            _ => Err(unavailable()),
                        }?;
                        values.push(value);
                    }
                    Ok(LaneData::List(values))
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_resolves_one_fixed_statement() {
        assert_eq!(sql_for("recommendations"), Some(RECOMMENDATIONS_SQL));
        assert_eq!(sql_for("resources"), Some(RESOURCES_SQL));
        assert_eq!(sql_for("calendar"), Some(CALENDAR_SQL));
        assert_eq!(sql_for("scenarios"), None);
        assert_eq!(sql_for(""), None);
    }

    #[test]
    fn every_statement_binds_company_and_both_branch_arguments() {
        for statement in [RECOMMENDATIONS_SQL, RESOURCES_SQL, CALENDAR_SQL] {
            assert!(statement.contains("company_id=$1"));
            assert!(statement.contains("($2::text IS NULL OR "));
            assert!(statement.contains("=$3)"));
            assert!(!statement.contains("LOCK"));
            assert!(!statement.contains("UPDATE"));
        }
    }

    #[test]
    fn statements_touch_only_planning_owned_relations() {
        let allowed = [
            "planning_recommendations",
            "planning_scenarios",
            "production_resources",
            "production_resource_calendar",
        ];
        for statement in [RECOMMENDATIONS_SQL, RESOURCES_SQL, CALENDAR_SQL] {
            let mut consumed = statement;
            while let Some(index) = consumed.find(" FROM ") {
                let tail = &consumed[index + 6..];
                let table = tail
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .next()
                    .unwrap_or("");
                assert!(allowed.contains(&table), "foreign relation: {table}");
                consumed = tail;
            }
        }
    }
}
