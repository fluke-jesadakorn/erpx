//! Read-only preparation; this plan is neither a write capability nor authority.
//! The consuming Maintenance/Field Service unit must retain source authorization,
//! reservations, Finance/Workflow guards and exact after-images before commit.
use super::consumption::ConsumptionPostingConfig;
use crate::domain::consumption::{
    ConsumptionValuation, LotAllocation, LotValuation, StockValuation, TrackingMode, cost_lot,
    finish_consumption, require_stock,
};
use crate::domain::valuation::safe;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use std::collections::BTreeSet;
use std::future::Future;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldServiceConsumptionSelection {
    pub company_id: CompanyId,
    pub branch_id: RecordId,
    pub warehouse_id: String,
    pub product_id: String,
    pub entry_date: String,
    pub quantity_milli: i64,
    pub allocations: Vec<LotAllocation>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldServiceConsumptionPlan {
    pub selection: FieldServiceConsumptionSelection,
    pub config: ConsumptionPostingConfig,
    pub period_id: RecordId,
    pub valuation: ConsumptionValuation,
}
/// Scoped owner queries on an already retained transaction. No cross-owner SQL,
/// provider calls, environment reads, effects or independent transaction here.
pub trait FieldServiceConsumptionPlanningPorts: Send {
    type Error: From<BusinessError> + Send;
    fn stock(
        &mut self,
        selection: &FieldServiceConsumptionSelection,
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
        selection: &FieldServiceConsumptionSelection,
        allocation: &LotAllocation,
    ) -> impl Future<Output = Result<Option<LotValuation>, Self::Error>> + Send;
}

pub async fn prepare_field_service_consumption<P: FieldServiceConsumptionPlanningPorts>(
    ports: &mut P,
    selection: FieldServiceConsumptionSelection,
) -> Result<FieldServiceConsumptionPlan, P::Error> {
    Ok(
        prepare_field_service_consumption_observed(ports, selection, || ())
            .await?
            .0,
    )
}
/// Observe the original consumption clock position after the unique period
/// check and before allocation validation/lot reads. An observation is data,
/// not a grant or effect. Read-only callers need no clock or runtime adapter.
pub async fn prepare_field_service_consumption_observed<P, O, Observe>(
    ports: &mut P,
    selection: FieldServiceConsumptionSelection,
    observe: Observe,
) -> Result<(FieldServiceConsumptionPlan, O), P::Error>
where
    P: FieldServiceConsumptionPlanningPorts,
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
    require_raw_lot_allocations(&stock, selection.quantity_milli, &selection.allocations)?;
    let mut lots = Vec::with_capacity(selection.allocations.len());
    for allocation in &selection.allocations {
        let lot = ports.lot(&selection, allocation).await?;
        lots.push(cost_lot(allocation, lot, &selection.entry_date)?);
    }
    let valuation = finish_consumption(stock, selection.quantity_milli, lots)?;
    Ok((
        FieldServiceConsumptionPlan {
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

// Field Service accepts raw persisted lot references; all nonidentity checks
// retain the shared planner's order and errors at the same observation point.
fn require_raw_lot_allocations(
    stock: &StockValuation,
    quantity: i64,
    allocations: &[LotAllocation],
) -> Result<(), BusinessError> {
    let required = || {
        BusinessError::invalid(
            "PART_LOT_ALLOCATION_REQUIRED",
            "Select the full spare quantity from distinct available lots or serials.",
        )
    };
    if allocations.len() > 500 {
        return Err(required());
    }
    if stock.tracking_mode == TrackingMode::None {
        if !allocations.is_empty() {
            return Err(BusinessError::invalid(
                "PART_LOT_NOT_ALLOWED",
                "This spare does not use lot or serial tracking.",
            ));
        }
        return Ok(());
    }
    let mut ids = BTreeSet::new();
    let mut sum = 0_i64;
    for lot in allocations {
        safe(lot.quantity_milli)?;
        safe(lot.balance_version)?;
        if lot.lot_id.is_empty()
            || lot.quantity_milli <= 0
            || lot.balance_version <= 0
            || !ids.insert(&lot.lot_id)
        {
            return Err(required());
        }
        sum = safe(sum.checked_add(lot.quantity_milli).ok_or_else(required)?)?;
    }
    if allocations.is_empty() || sum != quantity {
        return Err(required());
    }
    Ok(())
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
