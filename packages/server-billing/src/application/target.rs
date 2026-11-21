//! Billing target billing-period read. The repository supplies only Control
//! evidence rows in the installed read order; the domain performs the exact
//! measured-evidence and target-period judgement. No approval or write occurs.
use crate::domain::forecast_read::billing_month;
use crate::domain::target::{
    ApprovedExtrasRow, ApprovedInvoiceRow, ApprovedRatingRow, AssignmentRow, CostRow,
    CurrentSeatsRow, EvidenceMeterRow, MeasuredSeatIntervalsRow, MeterRow, OperatingCostsRow,
    PaymentConsentRow, PriceRow, RatingSeatIntervalsRow, SnapshotRow, StatementRow,
    TargetBillingPeriodView, TargetPeriodSource, calculate_target_period,
};
use crate::domain::target_pricing::month_bounds;
use erpx_server_shared::public::{BusinessError, HoldingId, TimestampMillis};
use std::future::Future;

pub trait TargetBillingRepository {
    type Error: From<BusinessError> + Send;

    fn operating_costs(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<OperatingCostsRow>, Self::Error>> + Send;

    fn measured_snapshot(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Option<SnapshotRow>, Self::Error>> + Send;

    fn current_seats(
        &mut self,
        holding: &HoldingId,
        billing_month: &str,
    ) -> impl Future<Output = Result<Option<CurrentSeatsRow>, Self::Error>> + Send;

    fn measured_meters(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<EvidenceMeterRow>, Self::Error>> + Send;

    fn measured_seat_intervals(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<MeasuredSeatIntervalsRow>, Self::Error>> + Send;

    fn assignment(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Option<AssignmentRow>, Self::Error>> + Send;

    fn assigned_price(
        &mut self,
        price_id: &str,
        month: &str,
    ) -> impl Future<Output = Result<Option<PriceRow>, Self::Error>> + Send;

    fn approved_rating(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Option<ApprovedRatingRow>, Self::Error>> + Send;

    fn approved_invoice(
        &mut self,
        invoice_id: &str,
    ) -> impl Future<Output = Result<Option<ApprovedInvoiceRow>, Self::Error>> + Send;

    fn approved_statements(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<StatementRow>, Self::Error>> + Send;

    fn rollout_eligibility(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    fn legacy_enrollment_overlap(
        &mut self,
        holding: &HoldingId,
        start: i64,
        end: i64,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    fn month_invoices(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    fn payment_consent(
        &mut self,
        holding: &HoldingId,
    ) -> impl Future<Output = Result<Option<PaymentConsentRow>, Self::Error>> + Send;

    fn rating_snapshot(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Option<SnapshotRow>, Self::Error>> + Send;

    fn rating_meters(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<MeterRow>, Self::Error>> + Send;

    fn provider_costs(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<CostRow>, Self::Error>> + Send;

    fn provider_statements(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<StatementRow>, Self::Error>> + Send;

    fn approved_extras(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<ApprovedExtrasRow>, Self::Error>> + Send;

    fn rating_seat_intervals(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<RatingSeatIntervalsRow>, Self::Error>> + Send;
}

/// Read one target billing period in the same order as the installed
/// TypeScript owner so every source read shares the caller's snapshot.
pub async fn read_target_billing_period<R: TargetBillingRepository>(
    repository: &mut R,
    holding: &HoldingId,
    month: &str,
    now: TimestampMillis,
) -> Result<TargetBillingPeriodView, R::Error> {
    let bounds = month_bounds(month)?;
    let operating_costs = repository.operating_costs(holding, month).await?;
    let measured_snapshot = repository.measured_snapshot(holding, month).await?;
    let current_month = billing_month(now.get())?;
    let current_seats = repository.current_seats(holding, &current_month).await?;
    let (measured_meters, measured_seat_intervals) = if measured_snapshot.is_some() {
        (
            repository.measured_meters(holding, month).await?,
            repository.measured_seat_intervals(holding, month).await?,
        )
    } else {
        (Vec::new(), Vec::new())
    };
    let assignment = repository.assignment(holding, month).await?;
    let assigned_price = match assignment.as_ref() {
        Some(assignment) => {
            repository
                .assigned_price(&assignment.price_id, month)
                .await?
        }
        None => None,
    };
    let approved_rating = repository.approved_rating(holding, month).await?;
    let mut source = TargetPeriodSource {
        operating_costs,
        measured_snapshot,
        current_seats,
        measured_meters,
        measured_seat_intervals,
        assignment,
        assigned_price,
        ..TargetPeriodSource::default()
    };
    if let Some(rating) = approved_rating.as_ref() {
        source.approved_invoice = match rating.invoice_id.as_deref() {
            Some(invoice_id) => repository.approved_invoice(invoice_id).await?,
            None => None,
        };
        source.approved_statements = repository.approved_statements(holding, month).await?;
        source.approved_rating = Some(rating.clone());
    } else {
        source.rollout_eligible = repository.rollout_eligibility(holding, month).await?;
        source.legacy_enrollment_overlap = repository
            .legacy_enrollment_overlap(holding, bounds.0, bounds.1)
            .await?;
        source.month_invoiced = repository.month_invoices(holding, month).await?;
        source.payment_consent = repository.payment_consent(holding).await?;
        source.rating_snapshot = repository.rating_snapshot(holding, month).await?;
        source.rating_meters = repository.rating_meters(holding, month).await?;
        source.provider_costs = repository.provider_costs(holding, month).await?;
        source.provider_statements = repository.provider_statements(holding, month).await?;
        source.approved_extras = repository.approved_extras(holding, month).await?;
        source.rating_seat_intervals = repository.rating_seat_intervals(holding, month).await?;
    }
    calculate_target_period(holding.as_str(), month, now.get(), &source).map_err(Into::into)
}
