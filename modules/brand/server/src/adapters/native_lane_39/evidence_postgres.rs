//! Lane 39 Brand evidence postgres adapter: sqlx execution of the registered
//! Brand evidence search against the per-Holding pool.
//!
//! The statement is rebuilt with `QueryBuilder` from literal owner fragments
//! and positional binds. It is byte-identical (after `?` -> `$n`) to the exact
//! statement the registered Brand read plan emits; the Organization node
//! projection is the already-registered `ORG_NODES` source, not a new authority.

use crate::adapters::native_lane_39::evidence_application::{
    BrandEvidenceFuture, BrandEvidenceRead,
};
use crate::adapters::native_lane_39::evidence_domain::{
    BrandProfileRow, EvidenceSearchRequest, validate,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::Row;

/// Exact registered Organization node projection (same source as
/// `native_read_sql::ORG_NODES`).
const ORG_NODES: &str = "SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes";

const HEAD: &str = "SELECT p.scope_id id,b.display_name branch_name,p.document_mode,p.updated_at,p.primary_attachment_id,p.inverse_attachment_id FROM organization_brand_profiles p JOIN (";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BRAND_EVIDENCE_SEARCH_UNAVAILABLE",
        "The native Brand evidence service is unavailable.",
    )
}

pub struct PgBrandEvidenceRead {
    pool: DatabasePool,
}

impl PgBrandEvidenceRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

/// Render the exact registered statement for a declared predicate family set.
pub fn rendered_sql(request: &EvidenceSearchRequest) -> String {
    let mut query = sqlx::QueryBuilder::new(HEAD);
    query.push(ORG_NODES);
    query.push(") b ON b.id=p.scope_id AND b.company_id=p.company_id WHERE p.scope_kind='branch' AND p.company_id=");
    query.push_bind(request.company_id.as_str());
    query.push(" AND b.kind='branch' AND b.status='active' AND (");
    query.push_bind(request.branch_id.as_deref());
    query.push("::text IS NULL OR p.scope_id=");
    query.push_bind(request.branch_id.as_deref());
    query.push(")");
    if let (Some(from), Some(to)) = (request.from, request.to_exclusive) {
        query.push(" AND p.updated_at>=");
        query.push_bind(from);
        query.push(" AND p.updated_at<");
        query.push_bind(to);
    }
    if let Some(needle) = request.query.as_deref() {
        query.push(" AND (position(lower(");
        query.push_bind(needle);
        query.push(") in lower(b.display_name))>0 OR position(lower(");
        query.push_bind(needle);
        query.push(") in lower(p.document_mode))>0 OR position(lower(");
        query.push_bind(needle);
        query.push(") in to_char(to_timestamp(p.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'))>0)");
    }
    if let Some(cursor) = request.cursor.as_ref() {
        query.push(" AND (b.display_name>");
        query.push_bind(cursor.branch_name.as_str());
        query.push(" OR (b.display_name=");
        query.push_bind(cursor.branch_name.as_str());
        query.push(" AND p.scope_id>");
        query.push_bind(cursor.id.as_str());
        query.push("))");
    }
    query.push(" ORDER BY b.display_name,p.scope_id LIMIT ");
    query.push_bind(request.limit);
    query.into_sql().as_str().to_owned()
}

impl BrandEvidenceRead for PgBrandEvidenceRead {
    fn search<'a>(
        &'a self,
        request: &'a EvidenceSearchRequest,
    ) -> BrandEvidenceFuture<'a, Vec<BrandProfileRow>> {
        let pool = self.pool.clone();
        let request = request.clone();
        Box::pin(async move {
            validate(&request)?;
            let mut query = sqlx::QueryBuilder::new(HEAD);
            query.push(ORG_NODES);
            query.push(") b ON b.id=p.scope_id AND b.company_id=p.company_id WHERE p.scope_kind='branch' AND p.company_id=");
            query.push_bind(request.company_id.as_str());
            query.push(" AND b.kind='branch' AND b.status='active' AND (");
            query.push_bind(request.branch_id.as_deref());
            query.push("::text IS NULL OR p.scope_id=");
            query.push_bind(request.branch_id.as_deref());
            query.push(")");
            if let (Some(from), Some(to)) = (request.from, request.to_exclusive) {
                query.push(" AND p.updated_at>=");
                query.push_bind(from);
                query.push(" AND p.updated_at<");
                query.push_bind(to);
            }
            if let Some(needle) = request.query.as_deref() {
                query.push(" AND (position(lower(");
                query.push_bind(needle);
                query.push(") in lower(b.display_name))>0 OR position(lower(");
                query.push_bind(needle);
                query.push(") in lower(p.document_mode))>0 OR position(lower(");
                query.push_bind(needle);
                query.push(") in to_char(to_timestamp(p.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'))>0)");
            }
            if let Some(cursor) = request.cursor.as_ref() {
                query.push(" AND (b.display_name>");
                query.push_bind(cursor.branch_name.as_str());
                query.push(" OR (b.display_name=");
                query.push_bind(cursor.branch_name.as_str());
                query.push(" AND p.scope_id>");
                query.push_bind(cursor.id.as_str());
                query.push("))");
            }
            query.push(" ORDER BY b.display_name,p.scope_id LIMIT ");
            query.push_bind(request.limit);
            let rows = query
                .build()
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(BrandProfileRow {
                        id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                        branch_name: row.try_get::<String, _>("branch_name").map_err(|_| unavailable())?,
                        document_mode: row.try_get::<String, _>("document_mode").map_err(|_| unavailable())?,
                        updated_at: row.try_get::<i64, _>("updated_at").map_err(|_| unavailable())?,
                        primary_attachment_id: row
                            .try_get::<Option<String>, _>("primary_attachment_id")
                            .map_err(|_| unavailable())?,
                        inverse_attachment_id: row
                            .try_get::<Option<String>, _>("inverse_attachment_id")
                            .map_err(|_| unavailable())?,
                    })
                })
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_39::evidence_domain::EvidenceCursor;
    use crate::adapters::native_read_sql::{plan_brand_read, EvidenceFlags, RD_EVIDENCE_SEARCH};
    use erpx_platform_runtime_server::public::http_support::serde_json::json;

    fn request(all: bool) -> EvidenceSearchRequest {
        EvidenceSearchRequest {
            company_id: "co".into(),
            branch_id: Some("br".into()),
            from: all.then_some(1),
            to_exclusive: all.then_some(2),
            query: all.then(|| "q".into()),
            cursor: all.then(|| EvidenceCursor {
                branch_name: "n".into(),
                id: "id".into(),
            }),
            limit: 26,
        }
    }

    /// Convert the registered `?` statement into positional `$n` form.
    fn positional(sql: &str) -> String {
        let mut index = 0usize;
        let mut out = String::with_capacity(sql.len());
        for ch in sql.chars() {
            if ch == '?' {
                index += 1;
                out.push('$');
                out.push_str(&index.to_string());
            } else {
                out.push(ch);
            }
        }
        out
    }

    #[test]
    fn rendered_sql_matches_the_registered_owner_plan_byte_for_byte() {
        for all in [false, true] {
            let req = request(all);
            let mut values: Vec<erpx_platform_runtime_server::public::http_support::serde_json::Value> =
                vec![json!("co"), json!("br"), json!("br")];
            if all {
                values.extend([json!(1), json!(2), json!("q"), json!("q"), json!("q"), json!("n"), json!("n"), json!("id")]);
            }
            values.push(json!(26));
            let planned = plan_brand_read(
                RD_EVIDENCE_SEARCH,
                &values,
                EvidenceFlags {
                    range: all,
                    query: all,
                    cursor: all,
                },
            )
            .unwrap();
            assert_eq!(rendered_sql(&req), positional(&planned.sql), "family set all={all}");
        }
    }

    #[test]
    fn sql_never_contains_caller_query_text() {
        let mut req = request(true);
        req.query = Some("DROP_TABLE_MARKER".into());
        let sql = rendered_sql(&req);
        assert!(!sql.contains("DROP_TABLE_MARKER"));
        assert!(sql.starts_with(HEAD));
        assert!(sql.ends_with(" ORDER BY b.display_name,p.scope_id LIMIT $12"));
    }
}
