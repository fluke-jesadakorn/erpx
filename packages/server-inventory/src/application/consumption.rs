//! Read-only preparation; this plan is neither a write capability nor authority.
//! The consuming Maintenance/Field Service unit must retain source authorization,
//! reservations, Finance/Workflow guards and exact after-images before commit.
use crate::domain::consumption::{
    ConsumptionValuation, LotAllocation, LotValuation, StockValuation, cost_lot,
    finish_consumption, require_lot_allocations, require_stock,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use std::future::Future;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsumptionSelection {
    pub company_id: CompanyId,
    pub branch_id: RecordId,
    pub warehouse_id: RecordId,
    pub product_id: RecordId,
    pub entry_date: String,
    pub quantity_milli: i64,
    pub allocations: Vec<LotAllocation>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsumptionPostingConfig {
    pub chart_id: RecordId,
    pub currency: String,
    pub base_currency: String,
    pub inventory_account_id: RecordId,
    pub expense_account_id: RecordId,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsumptionPlan {
    pub selection: ConsumptionSelection,
    pub config: ConsumptionPostingConfig,
    pub period_id: RecordId,
    pub valuation: ConsumptionValuation,
}
/// Scoped owner queries on an already retained transaction. No cross-owner SQL,
/// provider calls, environment reads, effects or independent transaction here.
pub trait ConsumptionPlanningPorts: Send {
    type Error: From<BusinessError> + Send;
    fn stock(
        &mut self,
        selection: &ConsumptionSelection,
    ) -> impl Future<Output = Result<Option<StockValuation>, Self::Error>> + Send;
    fn posting_config(
        &mut self,
        company: &CompanyId,
    ) -> impl Future<Output = Result<Option<ConsumptionPostingConfig>, Self::Error>> + Send;
    fn posting_periods(
        &mut self,
        company: &CompanyId,
        date: &str,
    ) -> impl Future<Output = Result<Vec<RecordId>, Self::Error>> + Send;
    fn lot(
        &mut self,
        selection: &ConsumptionSelection,
        allocation: &LotAllocation,
    ) -> impl Future<Output = Result<Option<LotValuation>, Self::Error>> + Send;
}

pub async fn prepare_consumption<P: ConsumptionPlanningPorts>(
    ports: &mut P,
    selection: ConsumptionSelection,
) -> Result<ConsumptionPlan, P::Error> {
    Ok(prepare_consumption_observed(ports, selection, || ())
        .await?
        .0)
}
/// Observe the original consumption clock position after the unique period
/// check and before allocation validation/lot reads. An observation is data,
/// not a grant or effect. Read-only callers need no clock or runtime adapter.
pub async fn prepare_consumption_observed<P, O, Observe>(
    ports: &mut P,
    selection: ConsumptionSelection,
    observe: Observe,
) -> Result<(ConsumptionPlan, O), P::Error>
where
    P: ConsumptionPlanningPorts,
    O: Send,
    Observe: FnOnce() -> O + Send,
{
    if !iso_date(&selection.entry_date) {
        return Err(BusinessError::invalid(
            "INVALID_CONSUMPTION_DATE",
            "Use a valid ISO inventory entry date.",
        )
        .into());
    }
    let stock = ports.stock(&selection).await?;
    let stock = require_stock(stock.as_ref(), selection.quantity_milli)?.clone();
    let config = ports.posting_config(&selection.company_id).await?;
    let Some(config) = config.filter(|c| c.currency == c.base_currency) else {
        return Err(BusinessError::invalid("PART_FUNCTIONAL_CURRENCY_REQUIRED", "Review the functional-currency inventory and service expense adapter before consuming parts.").into());
    };
    let periods = ports
        .posting_periods(&selection.company_id, &selection.entry_date)
        .await?;
    if periods.len() != 1 {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "PART_POSTING_PERIOD_REQUIRED",
            "Choose a date in one open Finance period.",
        )
        .into());
    }
    let observation = observe();
    require_lot_allocations(&stock, selection.quantity_milli, &selection.allocations)?;
    let mut lots = Vec::with_capacity(selection.allocations.len());
    for allocation in &selection.allocations {
        let lot = ports.lot(&selection, allocation).await?;
        lots.push(cost_lot(allocation, lot, &selection.entry_date)?);
    }
    let valuation = finish_consumption(stock, selection.quantity_milli, lots)?;
    Ok((
        ConsumptionPlan {
            selection,
            config,
            period_id: periods
                .into_iter()
                .next()
                .expect("exactly one checked period"),
            valuation,
        },
        observation,
    ))
}

fn iso_date(date: &str) -> bool {
    let b = date.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let year: u32 = date[..4].parse().unwrap_or(0);
    let month: u32 = date[5..7].parse().unwrap_or(0);
    let day: u32 = date[8..].parse().unwrap_or(0);
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    day >= 1 && day <= days
}
