//! Concrete public-facade bridge, not a synthetic Finance allowance.
use erpx_server_commerce::public::inventory_consumption::CommercePostingSettings;
use erpx_server_finance::public::inventory_consumption::*;
use erpx_server_inventory::public::{consumption::*, postgres_consumption::*};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, HoldingId, RecordId};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
pub struct FinanceFacts;
impl PgConsumptionFinancePort for FinanceFacts {
    type Source = CurrentConsumptionFinance;
    async fn lock_sources(&mut self, c: &mut PgConnection) -> Result<(), Error> {
        lock_inventory_consumption_finance_sources(c).await
    }
    async fn inspect(
        &mut self,
        c: &mut PgConnection,
        s: &CommercePostingSettings,
        date: &str,
    ) -> Result<Self::Source, Error> {
        read_inventory_consumption_finance(
            c,
            &HoldingId::new("holding").unwrap(),
            &InventoryConsumptionMapping {
                company_id: s.company_id.clone(),
                chart_id: s.chart_id.clone(),
                inventory_account_id: s.inventory_account_id.clone(),
                cogs_account_id: s.cogs_account_id.clone(),
            },
            date,
        )
        .await
    }
    fn config(s: &Self::Source) -> Option<ConsumptionPostingConfig> {
        s.config().map(|c| ConsumptionPostingConfig {
            chart_id: c.chart_id.clone(),
            currency: c.currency.clone(),
            base_currency: c.base_currency.clone(),
            inventory_account_id: c.inventory_account_id.clone(),
            expense_account_id: c.cogs_account_id.clone(),
        })
    }
    fn periods(s: &Self::Source) -> Vec<RecordId> {
        s.period_ids().to_vec()
    }
    async fn require_current(
        &mut self,
        c: &mut PgConnection,
        s: &Self::Source,
    ) -> Result<(), Error> {
        s.require_current(c).await
    }
}
