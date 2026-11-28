//! PostgreSQL execution for the native R3 RFQ workspace/detail reads.
//!
//! This is the only layer that executes SQL for these operations. Each nested
//! projection reproduces the retained TypeScript statement (same owner
//! projection source, same binds, same predicates and ordering) and the outer
//! `jsonb_build_object` only serializes the identical rows into one value so the
//! mapping cannot drift column by column. The pure `sourceBlocked`/`current`
//! comparison is applied after the fetch. No IAM authority, purchased-package
//! admission or command write happens here; composition supplies the actor and
//! the caller-owned Holding connection.
use crate::application::native_r3_rfq::{RfqFuture, RfqReadRepository};
use crate::domain::native_r3_rfq::{current_offers, not_found, source_blocked, RfqActor};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde_json::{json, Value};
use sqlx::{types::Json, PgConnection, Row as SqlRow};

type Error = PgTransactionError<BusinessError>;

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROCUREMENT_RFQ_WORKSPACE_UNAVAILABLE",
        "The Procurement quotation workspace is unavailable.",
    )
    .into()
}

// Retained projection-source oracles, asserted by the statement-shape tests.
#[allow(dead_code)]
const PARTIES_SQL: &str = r#"SELECT "id","company_id","kind","code","name","currency","tax_registration","payment_terms_days","credit_limit_minor","status","version","created_at","updated_at","person_id","notification_email","notification_consent","merged_into_id","merged_at","merged_by","phone","address_json" FROM finance_parties"#;
#[allow(dead_code)]
const PRODUCTS_SQL: &str = r#"SELECT "id","company_id","sku","name","unit","price_minor","tax_bps","status","version","tracking_mode" FROM commerce_products"#;
#[allow(dead_code)]
const WAREHOUSES_SQL: &str = r#"SELECT "id","company_id","branch_id","code","name","status","version" FROM commerce_warehouses"#;
#[allow(dead_code)]
const SETTINGS_SQL: &str = r#"SELECT "company_id","chart_id","currency","receivable_account_id","payable_account_id","revenue_account_id","inventory_account_id","clearing_account_id","cogs_account_id","input_tax_account_id","output_tax_account_id","cash_account_id" FROM commerce_settings"#;
#[allow(dead_code)]
const REQUESTS_SQL: &str = r#"SELECT "id","company_id","branch_id","department_id","kind","category","title","description","location","impact","needed_date","items_json","currency","requester_user_id","created_by","parent_request_id","state","team_id","assignee_user_id","routing_json","approval_json","blocked_reason","resolution","purchase_order_id","handover_json","version","evidence_version","created_at","updated_at","engagement_id" FROM employee_requests"#;

/// Retained workspace read: the scoped RFQ list (100 latest), active products,
/// branch-scoped active warehouses, active suppliers and the Company currency.
/// `$1` company, `$2` branch, `$3` department.
pub const WORKSPACE_SQL: &str = r#"
SELECT jsonb_build_object(
  'rfqs', COALESCE((SELECT jsonb_agg(to_jsonb(q) ORDER BY q.updated_at DESC) FROM (
    SELECT r.* FROM procurement_rfqs r WHERE r.company_id=$1 AND ($2::text IS NULL OR r.branch_id=$2) AND ($3::text IS NULL OR r.department_id=$3) ORDER BY r.updated_at DESC LIMIT 100
  ) q),'[]'::jsonb),
  'products', COALESCE((SELECT jsonb_agg(to_jsonb(q) ORDER BY q.sku) FROM (
    SELECT id,sku,name,unit FROM (SELECT "id","company_id","sku","name","unit","price_minor","tax_bps","status","version","tracking_mode" FROM commerce_products) commerce_products WHERE company_id=$1 AND status='active' ORDER BY sku
  ) q),'[]'::jsonb),
  'warehouses', COALESCE((SELECT jsonb_agg(to_jsonb(q) ORDER BY q.name) FROM (
    SELECT id,name,branch_id FROM (SELECT "id","company_id","branch_id","code","name","status","version" FROM commerce_warehouses) commerce_warehouses WHERE company_id=$1 AND ($2::text IS NULL OR branch_id=$2) AND status='active' ORDER BY name
  ) q),'[]'::jsonb),
  'suppliers', COALESCE((SELECT jsonb_agg(to_jsonb(q) ORDER BY q.name) FROM (
    SELECT id,name FROM (SELECT "id","company_id","kind","code","name","currency","tax_registration","payment_terms_days","credit_limit_minor","status","version","created_at","updated_at","person_id","notification_email","notification_consent","merged_into_id","merged_at","merged_by","phone","address_json" FROM finance_parties) finance_parties WHERE company_id=$1 AND kind='supplier' AND status='active' ORDER BY name
  ) q),'[]'::jsonb),
  'currency', COALESCE((SELECT currency FROM (SELECT "company_id","chart_id","currency","receivable_account_id","payable_account_id","revenue_account_id","inventory_account_id","clearing_account_id","cogs_account_id","input_tax_account_id","output_tax_account_id","cash_account_id" FROM commerce_settings) commerce_settings WHERE company_id=$1),'')
) AS data
"#;

/// Retained detail read: scoped RFQ point row, demand lines, party-joined
/// offers, offer lines, events and the raw Records source facts. `$1` id,
/// `$2` company, `$3` branch, `$4` department.
pub const DETAIL_SQL: &str = r#"
SELECT jsonb_build_object(
  'rfq', (SELECT to_jsonb(q) FROM (
    SELECT r.* FROM procurement_rfqs r WHERE r.id=$1 AND r.company_id=$2 AND ($3::text IS NULL OR r.branch_id=$3) AND ($4::text IS NULL OR r.department_id=$4)
  ) q),
  'lines', COALESCE((SELECT jsonb_agg(to_jsonb(q) ORDER BY q.ordinal) FROM (
    SELECT * FROM procurement_rfq_lines WHERE rfq_id=$1
  ) q),'[]'::jsonb),
  'offers', COALESCE((SELECT jsonb_agg(to_jsonb(q) ORDER BY q.rfq_version DESC) FROM (
    SELECT o.*,p.name supplier_name FROM procurement_rfq_offers o JOIN (SELECT "id","company_id","kind","code","name","currency","tax_registration","payment_terms_days","credit_limit_minor","status","version","created_at","updated_at","person_id","notification_email","notification_consent","merged_into_id","merged_at","merged_by","phone","address_json" FROM finance_parties) p ON p.id=o.supplier_id WHERE o.rfq_id=$1
  ) q),'[]'::jsonb),
  'offerLines', COALESCE((SELECT jsonb_agg(to_jsonb(q) ORDER BY q.id) FROM (
    SELECT l.* FROM procurement_rfq_offer_lines l JOIN procurement_rfq_offers o ON o.id=l.offer_id WHERE o.rfq_id=$1
  ) q),'[]'::jsonb),
  'events', COALESCE((SELECT jsonb_agg(to_jsonb(q) ORDER BY q.version DESC) FROM (
    SELECT * FROM procurement_rfq_events WHERE rfq_id=$1
  ) q),'[]'::jsonb),
  'sourceState', (SELECT to_jsonb(q) FROM (
    SELECT "id","company_id","branch_id","department_id","kind","category","title","description","location","impact","needed_date","items_json","currency","requester_user_id","created_by","parent_request_id","state","team_id","assignee_user_id","routing_json","approval_json","blocked_reason","resolution","purchase_order_id","handover_json","version","evidence_version","created_at","updated_at","engagement_id" FROM employee_requests WHERE id=(SELECT request_id FROM procurement_rfqs WHERE id=$1) AND company_id=$2
  ) q)
) AS data
"#;

pub struct PgRfqRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgRfqRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn workspace(
        &mut self,
        company: &CompanyId,
        branch: Option<&str>,
        department: Option<&str>,
    ) -> Result<Value, Error> {
        if company.as_str().is_empty() {
            return Err(unavailable());
        }
        let row = sqlx::query(WORKSPACE_SQL)
            .bind(company.as_str())
            .bind(branch)
            .bind(department)
            .fetch_one(&mut *self.connection)
            .await?;
        let data: Json<Value> = row.try_get("data")?;
        Ok(data.0)
    }

    pub async fn detail(
        &mut self,
        company: &CompanyId,
        branch: Option<&str>,
        department: Option<&str>,
        id: &str,
    ) -> Result<Value, Error> {
        if company.as_str().is_empty() || id.is_empty() {
            return Err(unavailable());
        }
        let row = sqlx::query(DETAIL_SQL)
            .bind(id)
            .bind(company.as_str())
            .bind(branch)
            .bind(department)
            .fetch_one(&mut *self.connection)
            .await?;
        let data: Json<Value> = row.try_get("data")?;
        let mut data = data.0;
        let rfq = data.get("rfq").cloned().filter(Value::is_object).ok_or_else(not_found)?;
        let blocked = source_blocked(&rfq, data.get("sourceState"));
        let offers: Vec<Value> = data.get("offers").and_then(Value::as_array).cloned().unwrap_or_default();
        let object = data.as_object_mut().ok_or_else(unavailable)?;
        object.insert("current".into(), json!(current_offers(&offers)));
        object.insert("sourceBlocked".into(), json!(blocked));
        // `currentRequestVersion` needs the Records requestQuotationSource
        // authority read; composition supplies it until that port is native.
        object.insert("currentRequestVersion".into(), Value::Null);
        Ok(data)
    }
}

impl RfqReadRepository for PgRfqRead<'_> {
    type Error = Error;

    fn workspace<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch: Option<&'a str>,
        department: Option<&'a str>,
    ) -> RfqFuture<'a, Value, Self::Error> {
        Box::pin(async move { PgRfqRead::workspace(self, company, branch, department).await })
    }

    fn detail<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch: Option<&'a str>,
        department: Option<&'a str>,
        id: &'a str,
    ) -> RfqFuture<'a, Value, Self::Error> {
        Box::pin(async move { PgRfqRead::detail(self, company, branch, department, id).await })
    }
}

/// Runs the retained owner read on the caller-owned Holding connection.
/// Composition supplies the Account-derived actor and enforces the commercial
/// purchased-package admission; this helper never opens a transaction.
pub async fn rfq_read_in_unit(
    connection: &mut PgConnection,
    actor: &RfqActor,
    now: TimestampMillis,
    id: Option<&str>,
) -> Result<Value, Error> {
    let mut repository = PgRfqRead::new(connection);
    match id {
        Some(id) => crate::application::native_r3_rfq::rfq_detail(actor, &mut repository, now, id).await,
        None => crate::application::native_r3_rfq::rfq_workspace(actor, &mut repository, now).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_statement_matches_the_retained_read() {
        assert!(WORKSPACE_SQL.contains("SELECT r.* FROM procurement_rfqs r WHERE r.company_id=$1"));
        assert!(WORKSPACE_SQL.contains("ORDER BY r.updated_at DESC LIMIT 100"));
        assert!(WORKSPACE_SQL.contains(PRODUCTS_SQL));
        assert!(WORKSPACE_SQL.contains(WAREHOUSES_SQL));
        assert!(WORKSPACE_SQL.contains(PARTIES_SQL));
        assert!(WORKSPACE_SQL.contains(SETTINGS_SQL));
        assert!(WORKSPACE_SQL.contains("kind='supplier' AND status='active'"));
        assert!(WORKSPACE_SQL.contains("($2::text IS NULL OR branch_id=$2)"));
    }

    #[test]
    fn detail_statement_matches_the_retained_read() {
        assert!(DETAIL_SQL.contains("r.id=$1 AND r.company_id=$2"));
        assert!(DETAIL_SQL.contains("FROM procurement_rfq_lines WHERE rfq_id=$1"));
        assert!(DETAIL_SQL.contains("supplier_name"));
        assert!(DETAIL_SQL.contains("JOIN procurement_rfq_offers o ON o.id=l.offer_id"));
        assert!(DETAIL_SQL.contains("FROM procurement_rfq_events WHERE rfq_id=$1"));
        assert!(DETAIL_SQL.contains(REQUESTS_SQL));
        assert!(DETAIL_SQL.contains("ORDER BY q.rfq_version DESC"));
        assert!(DETAIL_SQL.contains("ORDER BY q.version DESC"));
    }
}
