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
use erpx_server_runtime::public::postgres::DatabasePool;
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
