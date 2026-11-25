//! sqlx execution for the native Planning lane-36 scoped workspace reads.
//!
//! Every statement below is owner-authored and fixed by the closed read kind;
//! every value is bound. Only Planning-owned relations are read and the
//! Company predicate is mandatory, so a caller can never widen scope or inject
//! SQL. The TypeScript tails these statements replace are deleted.

use super::application::{LaneRead, LaneReadFuture};
use super::domain::{
    CalendarListItem, ComparisonRow, LaneData, LaneScope, RecommendationListItem, ResourceListItem,
    ScenarioRow, ScopedRecommendationRow, is_row_kind,
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

/// Single scenario row bounded by id, Company and optional branch scope.
pub const SCENARIO_SQL: &str = "SELECT id,company_id,branch_id,warehouse_id,specification_id,specification_version,demand_quantity_milli,due_date,capacity_minutes,shortages_json,status,created_by,version,created_at,updated_at,context_json FROM planning_scenarios WHERE id=$1 AND company_id=$2 AND ($3::text IS NULL OR branch_id=$4) LIMIT 1";

/// Single scoped recommendation row. Parent scenario owns the Company/branch.
pub const SCOPED_RECOMMENDATION_SQL: &str = "SELECT r.id,r.scenario_id,r.scenario_version,r.recommended_quantity_milli,r.required_minutes,r.feasible_date,r.status,r.created_by,r.reviewed_by,r.version,r.created_at,r.updated_at,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,s.context_json,s.created_by AS scenario_created_by FROM planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id WHERE r.id=$1 AND s.company_id=$2 AND ($3::text IS NULL OR s.branch_id=$4) LIMIT 1";

/// Single comparison row: same output, branch and specification.
pub const COMPARISON_SQL: &str = "SELECT s.id,s.version,s.shortages_json,r.required_minutes,r.feasible_date FROM planning_scenarios s JOIN planning_recommendations r ON r.scenario_id=s.id WHERE s.id=$1 AND s.company_id=$2 AND s.branch_id=$3 AND s.specification_id=$4 LIMIT 1";

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
        "scenario" => Some(SCENARIO_SQL),
        "scoped-recommendation" => Some(SCOPED_RECOMMENDATION_SQL),
        "comparison" => Some(COMPARISON_SQL),
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

fn map_scenario(row: &sqlx::postgres::PgRow) -> Result<Value, BusinessError> {
    let item = ScenarioRow {
        id: row.try_get("id").map_err(|_| invalid())?,
        company_id: row.try_get("company_id").map_err(|_| invalid())?,
        branch_id: row.try_get("branch_id").map_err(|_| invalid())?,
        warehouse_id: row.try_get("warehouse_id").map_err(|_| invalid())?,
        specification_id: row.try_get("specification_id").map_err(|_| invalid())?,
        specification_version: row.try_get("specification_version").map_err(|_| invalid())?,
        demand_quantity_milli: row.try_get("demand_quantity_milli").map_err(|_| invalid())?,
        due_date: row.try_get("due_date").map_err(|_| invalid())?,
        capacity_minutes: row.try_get("capacity_minutes").map_err(|_| invalid())?,
        shortages_json: row.try_get("shortages_json").map_err(|_| invalid())?,
        status: row.try_get("status").map_err(|_| invalid())?,
        created_by: row.try_get("created_by").map_err(|_| invalid())?,
        version: row.try_get("version").map_err(|_| invalid())?,
        created_at: row.try_get("created_at").map_err(|_| invalid())?,
        updated_at: row.try_get("updated_at").map_err(|_| invalid())?,
        context_json: row.try_get("context_json").map_err(|_| invalid())?,
    };
    to_value(item).map_err(|_| invalid())
}

fn map_scoped_recommendation(row: &sqlx::postgres::PgRow) -> Result<Value, BusinessError> {
    let item = ScopedRecommendationRow {
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
        warehouse_id: row.try_get("warehouse_id").map_err(|_| invalid())?,
        specification_id: row.try_get("specification_id").map_err(|_| invalid())?,
        specification_version: row.try_get("specification_version").map_err(|_| invalid())?,
        context_json: row.try_get("context_json").map_err(|_| invalid())?,
        scenario_created_by: row.try_get("scenario_created_by").map_err(|_| invalid())?,
    };
    to_value(item).map_err(|_| invalid())
}

fn map_comparison(row: &sqlx::postgres::PgRow) -> Result<Value, BusinessError> {
    let item = ComparisonRow {
        id: row.try_get("id").map_err(|_| invalid())?,
        version: row.try_get("version").map_err(|_| invalid())?,
        shortages_json: row.try_get("shortages_json").map_err(|_| invalid())?,
        required_minutes: row.try_get("required_minutes").map_err(|_| invalid())?,
        feasible_date: row.try_get("feasible_date").map_err(|_| invalid())?,
    };
    to_value(item).map_err(|_| invalid())
}

fn map(kind: &str, row: &sqlx::postgres::PgRow) -> Result<Value, BusinessError> {
    match kind {
        "recommendations" => map_recommendation(row),
        "resources" => map_resource(row),
        "calendar" => map_calendar(row),
        "scenario" => map_scenario(row),
        "scoped-recommendation" => map_scoped_recommendation(row),
        "comparison" => map_comparison(row),
        _ => Err(unavailable()),
    }
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
            let kind = scope.kind.clone();
            let company = Some(scope.company_id.clone());
            let branch = scope.branch.clone();
            let id = scope.id.clone();
            let comparison = scope.comparison_scenario_id.clone();
            let specification = scope.specification_id.clone();
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let kind = kind.clone();
                let company = company.clone();
                let branch = branch.clone();
                let id = id.clone();
                let comparison = comparison.clone();
                let specification = specification.clone();
                Box::pin(async move {
                    let values: Vec<Option<String>> = match kind.as_str() {
                        "recommendations" | "resources" | "calendar" => {
                            vec![company, branch.clone(), branch]
                        }
                        "scenario" | "scoped-recommendation" => {
                            vec![id, company, branch.clone(), branch]
                        }
                        // Unreachable: `sql_for` already resolved the kind.
                        "comparison" => vec![comparison, company, branch, specification],
                        _ => Vec::new(),
                    };
                    let mut query = sqlx::query(statement);
                    for value in &values {
                        query = query.bind(value.as_deref());
                    }
                    let rows = query
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    if is_row_kind(&kind) {
                        let row = rows
                            .first()
                            .map(|row| map(&kind, row))
                            .transpose()?;
                        Ok(LaneData::One(row))
                    } else {
                        let mut mapped = Vec::with_capacity(rows.len());
                        for row in &rows {
                            mapped.push(map(&kind, row)?);
                        }
                        Ok(LaneData::List(mapped))
                    }
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

    const ALL: [(&str, &str); 6] = [
        ("recommendations", RECOMMENDATIONS_SQL),
        ("resources", RESOURCES_SQL),
        ("calendar", CALENDAR_SQL),
        ("scenario", SCENARIO_SQL),
        ("scoped-recommendation", SCOPED_RECOMMENDATION_SQL),
        ("comparison", COMPARISON_SQL),
    ];

    #[test]
    fn each_kind_resolves_one_fixed_statement() {
        for (kind, sql) in ALL {
            assert_eq!(sql_for(kind), Some(sql));
        }
        assert_eq!(sql_for("scenarios"), None);
        assert_eq!(sql_for(""), None);
    }

    #[test]
    fn list_statements_bind_company_and_both_branch_arguments() {
        for statement in [RECOMMENDATIONS_SQL, RESOURCES_SQL, CALENDAR_SQL] {
            assert!(statement.contains("company_id=$1"));
            assert!(statement.contains("($2::text IS NULL OR "));
            assert!(statement.contains("=$3)"));
        }
    }

    #[test]
    fn row_statements_bind_their_closed_identity_fields() {
        assert!(SCENARIO_SQL.contains("id=$1 AND company_id=$2"));
        assert!(SCOPED_RECOMMENDATION_SQL.contains("r.id=$1 AND s.company_id=$2"));
        assert!(COMPARISON_SQL.contains("s.id=$1 AND s.company_id=$2"));
        assert!(COMPARISON_SQL.contains("s.branch_id=$3 AND s.specification_id=$4"));
        assert!(CALENDAR_SQL.contains("LIMIT 500"));
        for statement in [SCENARIO_SQL, SCOPED_RECOMMENDATION_SQL, COMPARISON_SQL] {
            assert!(statement.contains("LIMIT 1"));
        }
        for (_, statement) in ALL {
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
        for (_, statement) in ALL {
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
