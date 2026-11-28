//! PostgreSQL execution for the native R3 Manufacturing posting-setup read.
//!
//! This is the only layer that executes SQL for this operation. The nested
//! projections reproduce the retained statements (the Commerce posting-mapping
//! state and the active THB Finance chart/account references); the outer
//! `jsonb_build_object` only serializes the identical rows. The Finance
//! authority snapshot, purchased-capability admission, source-image hashing and
//! the command write remain composition responsibilities.
use crate::application::native_r3_posting_setup::{PostingSetupFuture, PostingSetupRepository};
use crate::domain::native_r3_posting_setup::PostingSetupActor;
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde_json::Value;
use sqlx::{types::Json, PgConnection, Row as SqlRow};

type Error = PgTransactionError<BusinessError>;

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANUFACTURING_POSTING_SETUP_UNAVAILABLE",
        "The Manufacturing posting setup is unavailable.",
    )
    .into()
}

// Retained projection-source oracles, asserted by the statement-shape test.
#[allow(dead_code)]
const SETTINGS_SQL: &str = r#"SELECT "company_id","chart_id","currency","receivable_account_id","payable_account_id","revenue_account_id","inventory_account_id","clearing_account_id","cogs_account_id","input_tax_account_id","output_tax_account_id","cash_account_id" FROM commerce_settings"#;
#[allow(dead_code)]
const CHARTS_SQL: &str = r#"SELECT "id","company_id","name","base_currency","accounting_basis","status","version","created_by","created_at","updated_at" FROM finance_charts"#;
#[allow(dead_code)]
const ACCOUNTS_SQL: &str = r#"SELECT "id","chart_id","company_id","code","name","category","parent_id","currency","is_control","status","version","created_at","updated_at","control_kind" FROM finance_accounts"#;

/// `$1` company. The configuration projection is the retained 11-column
/// `commerce_rd_posting_mapping_state` statement; charts and accounts reproduce
/// the retained active/THB filters and field mapping.
pub const POSTING_SETUP_SQL: &str = r#"
SELECT jsonb_build_object(
  'configuration', (SELECT to_jsonb(q) FROM (
    SELECT chart_id "chartId",currency,receivable_account_id "receivableAccountId",payable_account_id "payableAccountId",revenue_account_id "revenueAccountId",inventory_account_id "inventoryAccountId",clearing_account_id "clearingAccountId",cogs_account_id "cogsAccountId",input_tax_account_id "inputTaxAccountId",output_tax_account_id "outputTaxAccountId",cash_account_id "cashAccountId" FROM (SELECT "company_id","chart_id","currency","receivable_account_id","payable_account_id","revenue_account_id","inventory_account_id","clearing_account_id","cogs_account_id","input_tax_account_id","output_tax_account_id","cash_account_id" FROM commerce_settings) commerce_settings WHERE company_id=$1
  ) q),
  'charts', COALESCE((SELECT jsonb_agg(jsonb_build_object('id',q.id,'name',q.name,'baseCurrency',q.base_currency)) FROM (
    SELECT id,name,base_currency FROM (SELECT "id","company_id","name","base_currency","accounting_basis","status","version","created_by","created_at","updated_at" FROM finance_charts) finance_charts WHERE company_id=$1 AND status='active' AND base_currency='THB'
  ) q),'[]'::jsonb),
  'accounts', COALESCE((SELECT jsonb_agg(jsonb_build_object('id',q.id,'chartId',q.chart_id,'code',q.code,'name',q.name,'category',q.category,'currency',q.currency,'controlKind',q.control_kind)) FROM (
    SELECT id,chart_id,code,name,category,currency,control_kind FROM (SELECT "id","chart_id","company_id","code","name","category","parent_id","currency","is_control","status","version","created_at","updated_at","control_kind" FROM finance_accounts) finance_accounts WHERE company_id=$1 AND status='active' AND chart_id IN (SELECT id FROM (SELECT "id","company_id","name","base_currency","accounting_basis","status","version","created_by","created_at","updated_at" FROM finance_charts) finance_charts WHERE company_id=$1 AND status='active' AND base_currency='THB')
  ) q),'[]'::jsonb)
) AS data
"#;

pub struct PgPostingSetupRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgPostingSetupRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn rows(&mut self, company: &CompanyId) -> Result<Value, Error> {
        if company.as_str().is_empty() {
            return Err(unavailable());
        }
        let row = sqlx::query(POSTING_SETUP_SQL)
            .bind(company.as_str())
            .fetch_one(&mut *self.connection)
            .await?;
        let data: Json<Value> = row.try_get("data")?;
        Ok(data.0)
    }
}

impl PostingSetupRepository for PgPostingSetupRead<'_> {
    type Error = Error;

    fn rows<'a>(&'a mut self, company: &'a CompanyId) -> PostingSetupFuture<'a, Value, Self::Error> {
        Box::pin(async move { PgPostingSetupRead::rows(self, company).await })
    }
}

/// Runs the retained owner read on the caller-owned Holding connection.
/// Composition supplies the Finance authority, purchased admission and the
/// serializable transaction; this helper never opens a transaction or writes.
pub async fn posting_setup_read_in_unit(
    connection: &mut PgConnection,
    actor: &PostingSetupActor,
    now: TimestampMillis,
) -> Result<Value, Error> {
    let mut repository = PgPostingSetupRead::new(connection);
    crate::application::native_r3_posting_setup::manufacturing_posting_setup(actor, &mut repository, now).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_matches_the_retained_reads() {
        assert!(POSTING_SETUP_SQL.contains("chart_id \"chartId\""));
        assert!(POSTING_SETUP_SQL.contains(SETTINGS_SQL));
        assert!(POSTING_SETUP_SQL.contains(CHARTS_SQL));
        assert!(POSTING_SETUP_SQL.contains(ACCOUNTS_SQL));
        assert!(POSTING_SETUP_SQL.contains("status='active' AND base_currency='THB'"));
        assert!(POSTING_SETUP_SQL.contains("'baseCurrency',q.base_currency"));
        assert!(POSTING_SETUP_SQL.contains("'controlKind',q.control_kind"));
        assert!(POSTING_SETUP_SQL.contains("WHERE company_id=$1"));
    }
}
