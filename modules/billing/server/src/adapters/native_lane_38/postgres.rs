//! PostgreSQL executor for the native lane 38 Billing watermark read.
//!
//! The statement is byte-identical to the rendered TypeScript read tail
//! (`billingProjectionSql` + four `billingWatermarkRevisionSql` heads) with the
//! single positional `?` rewritten to sqlx's `$1`. The md5/string_agg revision
//! aggregation stays in SQL because it is a deterministic set aggregate whose
//! byte output depends on Postgres jsonb text ordering; moving it to Rust would
//! change the watermark. The query, company-bound predicate, typed row and
//! response projection are all owned by this crate.
use super::application::{BillingWatermarkRead, Lane38Future};
use super::domain::{WatermarkRow, unavailable};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use sqlx::Row;

pub const WATERMARK_SQL: &str = r#"SELECT a.version::text account_version,
         (SELECT md5(COALESCE(string_agg(jsonb_build_array(e.id,e.plan_id,e.started_at,e.ended_at,e.pending_end,e.pending_plan,e.effective_at)::text,',' ORDER BY e.id),'')) FROM billing_enrollments e WHERE e.holding_id=a.holding_id) enrollment_revision,
         (SELECT md5(COALESCE(string_agg(jsonb_build_array(i.id,i.status,i.due_at,i.total_satang,i.updated_at,i.kind)::text,',' ORDER BY i.id),'')) FROM billing_invoices i WHERE i.holding_id=a.holding_id) invoice_revision,
         (SELECT md5(COALESCE(string_agg(jsonb_build_array(b.subscription_id,b.status,b.current_period_end,b.last_event_at,b.updated_at)::text,',' ORDER BY b.holding_id),'')) FROM beta_billing b WHERE b.holding_id=a.holding_id) legacy_subscription_revision,
         (SELECT md5(COALESCE(string_agg(jsonb_build_array(p.id,p.revision,p.name,p.effective_month)::text,',' ORDER BY p.id,p.revision),'')) FROM billing_plans p WHERE p.id IN (SELECT e.plan_id FROM billing_enrollments e WHERE e.holding_id=a.holding_id)) plan_revision
        FROM billing_accounts a WHERE a.holding_id=$1"#;

#[derive(Clone)]
pub struct PgBillingWatermarkRead {
    pool: DatabasePool,
}

impl PgBillingWatermarkRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl BillingWatermarkRead for PgBillingWatermarkRead {
    fn watermarks<'a>(&'a self, holding: &'a str) -> Lane38Future<'a, Option<WatermarkRow>> {
        let pool = self.pool.clone();
        let holding = holding.to_owned();
        Box::pin(async move {
            let row = sqlx::query(WATERMARK_SQL)
                .bind(holding)
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|r| {
                Ok(WatermarkRow {
                    account_version: r
                        .try_get::<String, _>("account_version")
                        .map_err(|_| unavailable())?,
                    enrollment_revision: r
                        .try_get::<String, _>("enrollment_revision")
                        .map_err(|_| unavailable())?,
                    invoice_revision: r
                        .try_get::<String, _>("invoice_revision")
                        .map_err(|_| unavailable())?,
                    legacy_subscription_revision: r
                        .try_get::<String, _>("legacy_subscription_revision")
                        .map_err(|_| unavailable())?,
                    plan_revision: r
                        .try_get::<String, _>("plan_revision")
                        .map_err(|_| unavailable())?,
                })
            })
            .transpose()
        })
    }
}

// ---- Executive-main evidence family (wave 2) ------------------------------
//
// The statement text is byte-identical to the retired TypeScript read tail
// (`billingEnrollmentSubscriptionSql` + `billingLegacySubscriptionSql` +
// `billingProjectionSql` heads). Only `?` is rewritten to positional `$n`; the
// owner also owns the typed row projection and the exact key set.

use super::application::BillingEvidenceRead;
use super::domain::{
    CommercialPointInput, CommercialSearchInput, OverduePointInput, OverdueSearchInput,
    unavailable as evidence_unavailable,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{Arguments, postgres::PgArguments};

pub const COMMERCIAL_SELECT: &str = r#"WITH subscriptions AS (
      SELECT 'enrollment'::text source_kind,e.id record_id,COALESCE(p.name,e.plan_id) plan_name,
        CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status,e.started_at,
        CASE WHEN e.pending_end THEN e.effective_at ELSE NULL END effective_at,a.version record_version
      FROM billing_accounts a JOIN billing_enrollments e ON e.holding_id=a.holding_id
      LEFT JOIN LATERAL (SELECT name FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1) p ON TRUE
      WHERE a.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)
      UNION ALL
      SELECT 'legacy'::text source_kind,b.holding_id record_id,'Legacy subscription'::text plan_name,b.status,
        NULL::bigint started_at,b.current_period_end effective_at,b.updated_at record_version
      FROM beta_billing b WHERE b.holding_id=? AND b.subscription_id IS NOT NULL AND b.status IN ('active','trialing')
        AND (b.current_period_end IS NULL OR b.current_period_end*1000>?)
    ) SELECT source_kind,record_id,plan_name,status,started_at,effective_at,record_version FROM subscriptions"#;

pub const OVERDUE_SOURCE: &str = r#"SELECT i.id,i.kind,i.status,i.total_satang,i.due_at,i.updated_at,p.month
    FROM billing_invoices i JOIN billing_periods p ON p.id=i.period_id AND p.holding_id=i.holding_id
    WHERE i.holding_id=? AND i.status IN ('open','uncollectible') AND i.due_at>=? AND i.due_at<? AND i.due_at<?"#;

#[derive(Clone, Debug)]
enum Bind {
    Text(String),
    Int(i64),
}

/// Rewrite each positional `?` to `$n` in first-appearance order.
fn placeholders(sql: &str) -> (String, usize) {
    let mut out = String::with_capacity(sql.len() + 8);
    let mut count = 0usize;
    for ch in sql.chars() {
        if ch == '?' {
            count += 1;
            out.push('$');
            out.push_str(&count.to_string());
        } else {
            out.push(ch);
        }
    }
    (out, count)
}

fn arguments(binds: &[Bind]) -> Result<PgArguments, BusinessError> {
    let mut args = PgArguments::default();
    for bind in binds {
        match bind {
            Bind::Text(value) => args
                .add(value.as_str())
                .map_err(|_| evidence_unavailable())?,
            Bind::Int(value) => args.add(*value).map_err(|_| evidence_unavailable())?,
        }
    }
    Ok(args)
}

fn commercial_row(row: &sqlx::postgres::PgRow) -> Result<Value, BusinessError> {
    let get = |name: &str| -> Result<String, BusinessError> {
        row.try_get::<String, _>(name)
            .map_err(|_| evidence_unavailable())
    };
    let number = |name: &str| -> Result<Option<i64>, BusinessError> {
        row.try_get::<Option<i64>, _>(name)
            .map_err(|_| evidence_unavailable())
    };
    Ok(json!({
        "source_kind": get("source_kind")?,
        "record_id": get("record_id")?,
        "plan_name": get("plan_name")?,
        "status": get("status")?,
        "started_at": number("started_at")?,
        "effective_at": number("effective_at")?,
        "record_version": number("record_version")?,
    }))
}

fn overdue_row(row: &sqlx::postgres::PgRow) -> Result<Value, BusinessError> {
    let get = |name: &str| -> Result<String, BusinessError> {
        row.try_get::<String, _>(name)
            .map_err(|_| evidence_unavailable())
    };
    let number = |name: &str| -> Result<i64, BusinessError> {
        row.try_get::<i64, _>(name)
            .map_err(|_| evidence_unavailable())
    };
    Ok(json!({
        "id": get("id")?,
        "kind": get("kind")?,
        "status": get("status")?,
        "total_satang": number("total_satang")?,
        "due_at": number("due_at")?,
        "updated_at": number("updated_at")?,
        "month": get("month")?,
    }))
}

async fn fetch(
    pool: &DatabasePool,
    sql: String,
    binds: Vec<Bind>,
    commercial: bool,
) -> Result<Vec<Value>, BusinessError> {
    let (sql, count) = placeholders(&sql);
    if count != binds.len() {
        return Err(evidence_unavailable());
    }
    let args = arguments(&binds)?;
    let rows = sqlx::query_with(sqlx::AssertSqlSafe(sql), args)
        .fetch_all(pool.raw())
        .await
        .map_err(|_| evidence_unavailable())?;
    if commercial {
        rows.iter().map(commercial_row).collect()
    } else {
        rows.iter().map(overdue_row).collect()
    }
}

fn base_commercial_binds(holding: &str, now: i64) -> Vec<Bind> {
    vec![
        Bind::Text(holding.to_owned()),
        Bind::Int(now),
        Bind::Int(now),
        Bind::Text(holding.to_owned()),
        Bind::Int(now),
    ]
}

#[derive(Clone)]
pub struct PgBillingEvidenceRead {
    pool: DatabasePool,
}

impl PgBillingEvidenceRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl BillingEvidenceRead for PgBillingEvidenceRead {
    fn commercial_search<'a>(
        &'a self,
        holding: &'a str,
        input: &'a CommercialSearchInput,
    ) -> super::application::Lane38Future<'a, Vec<Value>> {
        let pool = self.pool.clone();
        let holding = holding.to_owned();
        let input = input.clone();
        Box::pin(async move {
            let mut sql = format!("{COMMERCIAL_SELECT}\n    ");
            let mut binds = base_commercial_binds(&holding, input.now);
            let mut conditions: Vec<&str> = Vec::new();
            if !input.query.is_empty() {
                conditions
                    .push("position(lower(?) in lower(concat_ws(' ',plan_name,status,source_kind,started_at::text,effective_at::text)))>0");
                binds.push(Bind::Text(input.query.clone()));
            }
            if let (Some(kind), Some(record_id)) = (&input.source_kind, &input.record_id) {
                conditions.push("(source_kind>? OR (source_kind=? AND record_id>?))");
                binds.push(Bind::Text(kind.clone()));
                binds.push(Bind::Text(kind.clone()));
                binds.push(Bind::Text(record_id.clone()));
            }
            if !conditions.is_empty() {
                sql.push_str("WHERE ");
                sql.push_str(&conditions.join(" AND "));
            }
            sql.push_str(" ORDER BY source_kind,record_id LIMIT ?");
            binds.push(Bind::Int(input.limit as i64));
            fetch(&pool, sql, binds, true).await
        })
    }

    fn commercial_point<'a>(
        &'a self,
        holding: &'a str,
        input: &'a CommercialPointInput,
    ) -> super::application::Lane38Future<'a, Option<Value>> {
        let pool = self.pool.clone();
        let holding = holding.to_owned();
        let input = input.clone();
        Box::pin(async move {
            let sql = format!(
                "{COMMERCIAL_SELECT}\n        WHERE source_kind='{}' AND record_id=? LIMIT 1",
                input.source_kind
            );
            let mut binds = base_commercial_binds(&holding, input.now);
            binds.push(Bind::Text(input.record_id.clone()));
            let mut rows = fetch(&pool, sql, binds, true).await?;
            if rows.len() > 1 {
                return Err(evidence_unavailable());
            }
            Ok(rows.pop())
        })
    }

    fn overdue_search<'a>(
        &'a self,
        holding: &'a str,
        input: &'a OverdueSearchInput,
    ) -> super::application::Lane38Future<'a, Vec<Value>> {
        let pool = self.pool.clone();
        let holding = holding.to_owned();
        let input = input.clone();
        Box::pin(async move {
            let mut sql = OVERDUE_SOURCE.to_owned();
            let mut binds = vec![
                Bind::Text(holding.clone()),
                Bind::Int(input.from_ms),
                Bind::Int(input.end),
                Bind::Int(input.now),
            ];
            let mut conditions: Vec<&str> = Vec::new();
            if !input.query.is_empty() {
                conditions.push(
                    "position(lower(?) in lower(concat_ws(' ',i.id,i.kind,i.status,p.month)))>0",
                );
                binds.push(Bind::Text(input.query.clone()));
            }
            if let (Some(due_at), Some(id)) = (input.due_at, &input.id) {
                conditions.push("(i.due_at>? OR (i.due_at=? AND i.id>?))");
                binds.push(Bind::Int(due_at));
                binds.push(Bind::Int(due_at));
                binds.push(Bind::Text(id.clone()));
            }
            if !conditions.is_empty() {
                sql.push_str(" AND ");
                sql.push_str(&conditions.join(" AND "));
            }
            sql.push_str("\n    ORDER BY i.due_at,i.id LIMIT ?");
            binds.push(Bind::Int(input.limit as i64));
            fetch(&pool, sql, binds, false).await
        })
    }

    fn overdue_point<'a>(
        &'a self,
        holding: &'a str,
        input: &'a OverduePointInput,
    ) -> super::application::Lane38Future<'a, Option<Value>> {
        let pool = self.pool.clone();
        let holding = holding.to_owned();
        let input = input.clone();
        Box::pin(async move {
            let sql = format!("{OVERDUE_SOURCE} AND i.id=? LIMIT 1");
            let binds = vec![
                Bind::Text(holding),
                Bind::Int(input.from_ms),
                Bind::Int(input.to_exclusive_ms),
                Bind::Int(input.now),
                Bind::Text(input.id.clone()),
            ];
            let mut rows = fetch(&pool, sql, binds, false).await?;
            if rows.len() > 1 {
                return Err(evidence_unavailable());
            }
            Ok(rows.pop())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED_SQL: &str = "SELECT a.version::text account_version,\n         (SELECT md5(COALESCE(string_agg(jsonb_build_array(e.id,e.plan_id,e.started_at,e.ended_at,e.pending_end,e.pending_plan,e.effective_at)::text,',' ORDER BY e.id),'')) FROM billing_enrollments e WHERE e.holding_id=a.holding_id) enrollment_revision,\n         (SELECT md5(COALESCE(string_agg(jsonb_build_array(i.id,i.status,i.due_at,i.total_satang,i.updated_at,i.kind)::text,',' ORDER BY i.id),'')) FROM billing_invoices i WHERE i.holding_id=a.holding_id) invoice_revision,\n         (SELECT md5(COALESCE(string_agg(jsonb_build_array(b.subscription_id,b.status,b.current_period_end,b.last_event_at,b.updated_at)::text,',' ORDER BY b.holding_id),'')) FROM beta_billing b WHERE b.holding_id=a.holding_id) legacy_subscription_revision,\n         (SELECT md5(COALESCE(string_agg(jsonb_build_array(p.id,p.revision,p.name,p.effective_month)::text,',' ORDER BY p.id,p.revision),'')) FROM billing_plans p WHERE p.id IN (SELECT e.plan_id FROM billing_enrollments e WHERE e.holding_id=a.holding_id)) plan_revision\n        FROM billing_accounts a WHERE a.holding_id=$1";

    #[test]
    fn native_lane_38_sql_is_byte_identical_to_rendered_read_tail() {
        assert_eq!(WATERMARK_SQL, EXPECTED_SQL);
        assert_eq!(WATERMARK_SQL.matches("$1").count(), 1);
        assert_eq!(WATERMARK_SQL.matches("SELECT").count(), 6);
    }

    #[test]
    fn native_lane_38_evidence_sql_is_owner_local_and_select_only() {
        for sql in [COMMERCIAL_SELECT, OVERDUE_SOURCE] {
            let upper = sql.to_ascii_uppercase();
            assert!(upper.starts_with("WITH SUBSCRIPTIONS") || upper.starts_with("SELECT "));
            for forbidden in [
                "INSERT",
                "UPDATE ",
                "DELETE",
                "RETURNING",
                ";",
                "FOR UPDATE",
                "FOR SHARE",
                "LOCK TABLE",
            ] {
                assert!(
                    !sql.contains(forbidden),
                    "evidence read must not mutate or lock: {forbidden}"
                );
            }
        }
        assert!(COMMERCIAL_SELECT.contains("billing_accounts"));
        assert!(COMMERCIAL_SELECT.contains("beta_billing"));
        assert!(OVERDUE_SOURCE.contains("billing_invoices"));
        assert!(OVERDUE_SOURCE.contains("billing_periods"));
        assert_eq!(COMMERCIAL_SELECT.matches('?').count(), 5);
        assert_eq!(OVERDUE_SOURCE.matches('?').count(), 4);
    }

    #[test]
    fn native_lane_38_placeholders_follow_bind_order() {
        let (sql, count) = placeholders("WHERE a=? AND b=? AND c=?");
        assert_eq!(sql, "WHERE a=$1 AND b=$2 AND c=$3");
        assert_eq!(count, 3);
        let (sql, count) = placeholders(COMMERCIAL_SELECT);
        assert!(sql.contains("a.holding_id=$1"));
        assert!(sql.contains("e.started_at<=$2"));
        assert!(sql.contains("b.current_period_end*1000>$5"));
        assert_eq!(count, 5);
    }

    #[test]
    fn native_lane_38_sql_is_owner_local_select_only() {
        assert!(WATERMARK_SQL.starts_with("SELECT "));
        for forbidden in [
            "INSERT",
            "UPDATE ",
            "DELETE",
            "RETURNING",
            ";",
            "FOR UPDATE",
            "FOR SHARE",
            "LOCK TABLE",
        ] {
            assert!(
                !WATERMARK_SQL.contains(forbidden),
                "owner read must not mutate or lock: {forbidden}"
            );
        }
        for table in [
            "billing_accounts",
            "billing_enrollments",
            "billing_invoices",
            "beta_billing",
            "billing_plans",
        ] {
            assert!(
                WATERMARK_SQL.contains(table),
                "owner read must stay on Billing-owned table: {table}"
            );
        }
    }
}
