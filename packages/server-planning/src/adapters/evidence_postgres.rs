//! PostgreSQL adapter for the native Planning evidence SEARCH port.
//!
//! Every identifier below is owner-authored and fixed by the closed metric;
//! every value is bound. The statement reads Planning-owned scenario and
//! recommendation rows and the already-owned Specification/Product/Warehouse/
//! Organization label projections for presentation only. No cross-owner write,
//! purchase decision or caller SQL enters this adapter. Company/branch scope,
//! the bounded text query, the opaque cursor id and `limit + 1` are the only
//! paging controls.
use crate::application::evidence::{EvidenceFuture, EvidenceRepository};
use crate::domain::evidence::{
    CAPACITY_FEASIBLE_RECOMMENDATIONS, EvidenceActor, EvidenceEligibility, EvidenceQuery,
    EvidenceRow, PROPOSED_RECOMMENDATIONS, SHORTAGE_SCENARIOS,
};
use erpx_server_shared::public::BusinessError;
use sqlx::{PgConnection, Row};

const SELECT_HEAD: &str = r#"SELECT {id} AS id,s.id AS scenario_id,r.id AS recommendation_id,
 s.branch_id,branch.display_name AS branch_name,s.warehouse_id,w.name AS warehouse_name,
 s.specification_id,spec.name AS specification_name,spec.output_product_id,
 output.name AS output_product_name,s.specification_version,
 s.demand_quantity_milli::text AS demand_quantity_milli,s.due_date::text AS due_date,
 s.capacity_minutes::text AS capacity_minutes,s.status AS scenario_status,
 s.version AS scenario_version,s.created_at AS scenario_created_at,s.updated_at AS scenario_updated_at,
 r.status AS recommendation_status,r.recommended_quantity_milli::text AS recommended_quantity_milli,
 r.required_minutes::text AS required_minutes,r.feasible_date::text AS feasible_date,
 r.version AS recommendation_version,r.scenario_version AS scenario_version_ref,
 r.created_at AS recommendation_created_at,r.updated_at AS recommendation_updated_at,
 COALESCE(shortage.shortage_count,'0') AS shortage_count,
 s.shortages_json::text AS shortage_items_json"#;

const FROM: &str = r#" FROM planning_scenarios s
 LEFT JOIN manufacturing_specifications spec ON spec.id=s.specification_id AND spec.company_id=s.company_id
 LEFT JOIN commerce_products output ON output.id=spec.output_product_id AND output.company_id=s.company_id
 LEFT JOIN commerce_warehouses w ON w.id=s.warehouse_id AND w.company_id=s.company_id
 LEFT JOIN organization_nodes branch ON branch.id=s.branch_id AND branch.company_id=s.company_id
 LEFT JOIN planning_recommendations r ON r.scenario_id=s.id
 LEFT JOIN LATERAL (
   SELECT count(*) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0)::text AS shortage_count
   FROM jsonb_array_elements(COALESCE(s.shortages_json::jsonb,'[]'::jsonb)) AS entry(value)
 ) shortage ON TRUE"#;

const QUERY_TERMS: &str = r#"(position(lower($6) in lower(COALESCE(s.id,'')))>0
 OR position(lower($7) in lower(COALESCE(spec.name,'')))>0
 OR position(lower($8) in lower(COALESCE(w.name,'')))>0
 OR position(lower($9) in lower(COALESCE(branch.display_name,'')))>0
 OR position(lower($10) in lower(COALESCE(s.status,'')))>0
 OR position(lower($11) in lower(COALESCE(r.status,'')))>0
 OR position(lower($12) in lower(COALESCE(s.due_date::text,'')))>0
 OR position(lower($13) in lower(COALESCE(r.feasible_date::text,'')))>0)"#;

/// Build the exact bounded statement for one metric. Only the metric-selected
/// identifier and date/status predicates vary; no caller value is interpolated.
pub fn build_search_sql(metric: &str) -> String {
    let (id, metric_filter) = match metric {
        SHORTAGE_SCENARIOS => (
            "s.id",
            "s.due_date::text BETWEEN $4 AND $5 AND jsonb_path_exists(COALESCE(s.shortages_json::jsonb,'[]'::jsonb),'$[*] ? (@.shortageMilli > 0)')",
        ),
        PROPOSED_RECOMMENDATIONS => (
            "r.id",
            "r.feasible_date::text BETWEEN $4 AND $5 AND r.status='proposed'",
        ),
        CAPACITY_FEASIBLE_RECOMMENDATIONS => (
            "r.id",
            "r.feasible_date::text BETWEEN $4 AND $5 AND r.status IN ('proposed','approved') AND r.required_minutes<=s.capacity_minutes",
        ),
        _ => ("s.id", "FALSE"),
    };
    let head = SELECT_HEAD.replacen("{id}", id, 1);
    format!(
        "{head}{FROM}
 WHERE s.company_id=$1
 AND ($2::text IS NULL OR s.branch_id=$2)
 AND ($3::text[] IS NULL OR s.id=ANY($3))
 AND {metric_filter}
 AND {QUERY_TERMS}
 AND ($14::text IS NULL OR {id}>$14)
 ORDER BY {id} COLLATE \"C\" LIMIT $15"
    )
}

pub struct PgEvidenceRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgEvidenceRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "EVIDENCE_VALUE_INVALID",
        "The Planning source returned an invalid evidence row.",
    )
}

fn map_row(row: &sqlx::postgres::PgRow) -> Result<EvidenceRow, BusinessError> {
    Ok(EvidenceRow {
        id: row.try_get("id").map_err(|_| invalid())?,
        scenario_id: row.try_get("scenario_id").map_err(|_| invalid())?,
        recommendation_id: row.try_get("recommendation_id").map_err(|_| invalid())?,
        branch_id: row.try_get("branch_id").map_err(|_| invalid())?,
        branch_name: row.try_get("branch_name").map_err(|_| invalid())?,
        warehouse_id: row.try_get("warehouse_id").map_err(|_| invalid())?,
        warehouse_name: row.try_get("warehouse_name").map_err(|_| invalid())?,
        specification_id: row.try_get("specification_id").map_err(|_| invalid())?,
        specification_name: row.try_get("specification_name").map_err(|_| invalid())?,
        output_product_id: row.try_get("output_product_id").map_err(|_| invalid())?,
        output_product_name: row.try_get("output_product_name").map_err(|_| invalid())?,
        specification_version: row.try_get("specification_version").map_err(|_| invalid())?,
        demand_quantity_milli: row.try_get("demand_quantity_milli").map_err(|_| invalid())?,
        due_date: row.try_get("due_date").map_err(|_| invalid())?,
        capacity_minutes: row.try_get("capacity_minutes").map_err(|_| invalid())?,
        scenario_status: row.try_get("scenario_status").map_err(|_| invalid())?,
        scenario_version: row.try_get("scenario_version").map_err(|_| invalid())?,
        scenario_created_at: row.try_get("scenario_created_at").map_err(|_| invalid())?,
        scenario_updated_at: row.try_get("scenario_updated_at").map_err(|_| invalid())?,
        recommendation_status: row.try_get("recommendation_status").map_err(|_| invalid())?,
        recommended_quantity_milli: row
            .try_get("recommended_quantity_milli")
            .map_err(|_| invalid())?,
        required_minutes: row.try_get("required_minutes").map_err(|_| invalid())?,
        feasible_date: row.try_get("feasible_date").map_err(|_| invalid())?,
        recommendation_version: row.try_get("recommendation_version").map_err(|_| invalid())?,
        scenario_version_ref: row.try_get("scenario_version_ref").map_err(|_| invalid())?,
        recommendation_created_at: row
            .try_get("recommendation_created_at")
            .map_err(|_| invalid())?,
        recommendation_updated_at: row
            .try_get("recommendation_updated_at")
            .map_err(|_| invalid())?,
        shortage_count: row.try_get("shortage_count").map_err(|_| invalid())?,
        shortage_items_json: row.try_get("shortage_items_json").map_err(|_| invalid())?,
    })
}

impl EvidenceRepository for PgEvidenceRead<'_> {
    fn search<'a>(
        &'a mut self,
        actor: &'a EvidenceActor,
        eligibility: &'a EvidenceEligibility,
        query: &'a EvidenceQuery,
    ) -> EvidenceFuture<'a, Vec<EvidenceRow>> {
        Box::pin(async move {
            let sql = build_search_sql(&query.metric);
            let eligible: Option<Vec<String>> = match eligibility {
                EvidenceEligibility::All => None,
                EvidenceEligibility::Scenarios(ids) => Some(ids.clone()),
            };
            let branch = actor.branch_id.as_deref();
            let q = query.query.as_str();
            let cursor = query.cursor_id.as_deref();
            let take = query.limit as i64 + 1;
            let fetched = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(actor.company_id.as_str())
                .bind(branch)
                .bind(eligible)
                .bind(query.from.as_str())
                .bind(query.to.as_str())
                .bind(q)
                .bind(q)
                .bind(q)
                .bind(q)
                .bind(q)
                .bind(q)
                .bind(q)
                .bind(q)
                .bind(cursor)
                .bind(take)
                .fetch_all(&mut *self.connection)
                .await
                .map_err(|_| invalid())?;
            fetched.iter().map(map_row).collect()
        })
    }
}
