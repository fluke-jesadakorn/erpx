//! Billing target billing-period read over Control evidence.
pub use crate::adapters::target::{PgTargetBillingRepository, read_target_billing_period};
pub use crate::application::target::{
    TargetBillingRepository, read_target_billing_period as read_target_billing_period_with,
};
pub use crate::domain::target::{
    ApprovedExtrasRow, ApprovedInvoiceRow, ApprovedRatingRow, AssignmentRow, CostRow,
    CurrentSeatsRow, EvidenceMeterRow, MeasuredActiveNow, MeasuredAi, MeasuredAiSource,
    MeasuredSeatIntervalsRow, MeasuredSeats, MeasuredStorage, MeterRow, OperatingCategories,
    OperatingCategory, OperatingCostsRow, PaymentConsentRow, PeriodApproval, PeriodAssignment,
    PeriodCompleteness, PeriodInvoice, PriceRow, RatingSeatIntervalsRow, SnapshotRow,
    StatementReconciliation, StatementRow, TargetBillingPeriodView, TargetMeasuredEvidence,
    TargetMeasuredEvidenceSnapshot, TargetPeriodSource, TargetPriceView, TargetRatedLine,
    calculate_target_period, measured_evidence,
};
