//! PostgreSQL execution for the native Executive Quality evidence search.
//!
//! The selection is byte-identical to the retired `inspectionEvidenceSource`
//! search leg; only `?` placeholders become numbered `$n` binds. No row lock,
//! command receipt or cross-owner write is introduced, and no SQL is prepared
//! or executed by TypeScript.

use crate::application::quality_evidence::{
    QualityEvidenceFuture, QualityEvidenceReader,
};
use crate::domain::quality_evidence::{
    date_millis, InspectionEvidenceQuery, InspectionEvidenceRow, invalid,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{Arguments, Row, postgres::PgArguments};

/// Exact retired TypeScript head: Quality inspection joined to its hold and
/// the two locked Commercial owner label projections.
const HEAD: &str = r#"SELECT i.id,i.hold_id,i.result,i.created_at,i.reviewed_version,p.name product_name,w.name warehouse_name
    FROM inventory_quality_inspections i
    JOIN inventory_quality_holds h ON h.id=i.hold_id
    JOIN (SELECT "id","company_id","sku","name","unit","price_minor","tax_bps","status","version","tracking_mode" FROM commerce_products) p ON p.id=h.product_id AND p.company_id=h.company_id
    JOIN (SELECT "id","company_id","branch_id","code","name","status","version" FROM commerce_warehouses) w ON w.id=h.warehouse_id AND w.company_id=h.company_id
    WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND i.created_at>=? AND i.created_at<?"#;

/// Retained case-insensitive search predicate over product name and result.
const SEARCH: &str =
    " AND (position(lower(?) in lower(p.name))>0 OR position(lower(?) in lower(i.result))>0)";

/// Retained keyset cursor predicate; both binds are the decoded cursor facts.
const CURSOR: &str = " AND (i.created_at,i.id)<(?,?)";

const TAIL: &str = " ORDER BY i.created_at DESC,i.id DESC LIMIT ?";

/// Binds in the exact order the retired statement supplied them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Bind {
    Text(String),
    Opt(Option<String>),
    Int(i64),
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "QUALITY_EVIDENCE_READ_UNAVAILABLE",
        "The native Quality evidence service is unavailable.",
    )
}

/// Translate only `?` placeholders into numbered Postgres binds. The selected
/// SQL contains no `?` inside a literal, so the mapping is exact.
fn numbered(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 8);
    let mut index = 0_usize;
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

/// Render the owner SQL and positional binds. The `limit + 1` page is retained
/// so the application can detect `hasMore` without a second count query.
pub fn render(query: &InspectionEvidenceQuery) -> Result<(String, Vec<Bind>), BusinessError> {
    let from = date_millis(&query.from).ok_or_else(invalid)?;
    let to = date_millis(&query.to)
        .and_then(|value| value.checked_add(86_400_000))
        .ok_or_else(invalid)?;
    let mut values = vec![
        Bind::Text(query.company.clone()),
        Bind::Opt(query.branch.clone()),
        Bind::Opt(query.branch.clone()),
        Bind::Int(from),
        Bind::Int(to),
    ];
    let mut sql = HEAD.to_owned();
    if !query.query.is_empty() {
        sql.push_str(SEARCH);
        values.push(Bind::Text(query.query.clone()));
        values.push(Bind::Text(query.query.clone()));
    }
    if let Some(cursor) = &query.cursor {
        sql.push_str(CURSOR);
        values.push(Bind::Int(cursor.created_at));
        values.push(Bind::Text(cursor.id.clone()));
    }
    sql.push_str(TAIL);
    values.push(Bind::Int(query.limit as i64 + 1));
    Ok((numbered(&sql), values))
}

fn args(binds: &[Bind]) -> Result<PgArguments, BusinessError> {
    let mut args = PgArguments::default();
    for bind in binds {
        match bind {
            Bind::Text(value) => {
                args.add(value.as_str()).map_err(|_| unavailable())?;
            }
            Bind::Opt(value) => {
                args.add(value.as_deref()).map_err(|_| unavailable())?;
            }
            Bind::Int(value) => {
                args.add(*value).map_err(|_| unavailable())?;
            }
        }
    }
    Ok(args)
}

fn projection(row: &sqlx::postgres::PgRow) -> Result<InspectionEvidenceRow, BusinessError> {
    let get = |name: &str| -> Result<String, BusinessError> {
        row.try_get::<String, _>(name).map_err(|_| unavailable())
    };
    Ok(InspectionEvidenceRow {
        id: get("id")?,
        hold_id: get("hold_id")?,
        result: get("result")?,
        created_at: row
            .try_get::<i64, _>("created_at")
            .map_err(|_| unavailable())?,
        reviewed_version: row
            .try_get::<i64, _>("reviewed_version")
            .map_err(|_| unavailable())?,
        product_name: get("product_name")?,
        warehouse_name: get("warehouse_name")?,
    })
}

/// Pool-backed owner reader. Composition mounts one per registered Holding;
/// no caller transaction or SQL from TypeScript is required.
pub struct PgQualityEvidenceReader {
    pool: DatabasePool,
}

impl PgQualityEvidenceReader {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl QualityEvidenceReader for PgQualityEvidenceReader {
    fn inspection_evidence<'a>(
        &'a self,
        query: &'a InspectionEvidenceQuery,
    ) -> QualityEvidenceFuture<'a, Vec<InspectionEvidenceRow>> {
        Box::pin(async move {
            let (sql, binds) = render(query)?;
            let arguments = args(&binds)?;
            let rows = sqlx::query_with(sqlx::AssertSqlSafe(sql), arguments)
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter().map(projection).collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::quality_evidence::EvidenceCursor;

    fn query() -> InspectionEvidenceQuery {
        InspectionEvidenceQuery {
            company: "co".into(),
            branch: Some("br".into()),
            from: "2026-01-01".into(),
            to: "2026-01-31".into(),
            currency: "THB".into(),
            query: String::new(),
            limit: 25,
            cursor: None,
        }
    }

    #[test]
    fn head_sql_is_owner_local_lock_free_and_byte_stable() {
        assert!(HEAD.starts_with("SELECT i.id,i.hold_id,i.result,"));
        assert!(!HEAD.contains("ORDER BY"));
        assert!(HEAD.contains("FROM inventory_quality_inspections i"));
        for forbidden in [
            "FOR UPDATE",
            "FOR SHARE",
            "LOCK TABLE",
            "pg_advisory",
            "INSERT ",
            "UPDATE ",
            "DELETE ",
            "command_receipts",
        ] {
            assert!(!HEAD.contains(forbidden), "search must stay read-only: {forbidden}");
        }
        assert!(HEAD.contains(
            r#"JOIN (SELECT "id","company_id","sku","name","unit","price_minor","tax_bps","status","version","tracking_mode" FROM commerce_products) p"#
        ));
    }

    #[test]
    fn render_preserves_bind_order_and_limit_sentinel() {
        let (sql, binds) = render(&query()).unwrap();
        assert!(sql.starts_with("SELECT i.id,i.hold_id,i.result,"));
        assert!(sql.ends_with("ORDER BY i.created_at DESC,i.id DESC LIMIT $6"));
        assert_eq!(binds.len(), 6);
        assert_eq!(binds[0], Bind::Text("co".into()));
        assert_eq!(binds[1], Bind::Opt(Some("br".into())));
        assert_eq!(binds[3], Bind::Int(1_767_225_600_000));
        assert_eq!(binds[5], Bind::Int(26));
        let max = sql
            .split('$')
            .skip(1)
            .filter_map(|part| {
                part.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<usize>()
                    .ok()
            })
            .max()
            .unwrap_or(0);
        assert_eq!(max, binds.len());
    }

    #[test]
    fn render_adds_search_and_cursor_predicates_before_limit() {
        let mut q = query();
        q.query = "rice".into();
        q.cursor = Some(EvidenceCursor {
            created_at: 1_767_225_600_000,
            id: "i1".into(),
        });
        let (sql, binds) = render(&q).unwrap();
        assert!(sql.contains("position(lower($6) in lower(p.name))"));
        assert!(sql.contains("(i.created_at,i.id)<($8,$9)"));
        assert_eq!(binds.len(), 10);
        assert_eq!(binds[6], Bind::Text("rice".into()));
        assert_eq!(binds[7], Bind::Int(1_767_225_600_000));
        assert_eq!(binds[8], Bind::Text("i1".into()));
        assert_eq!(binds[9], Bind::Int(26));
    }

    #[test]
    fn numbered_translates_every_placeholder() {
        assert_eq!(numbered("a=? b=? c=?"), "a=$1 b=$2 c=$3");
        assert_eq!(numbered("no binds"), "no binds");
    }
}
