//! Bounded per-Company AI quota reservation/finish contracts.
pub use crate::adapters::quota_postgres::{PgCompanyQuota, QuotaError};
pub use crate::application::quota::{
    CompanyQuotaPorts, finish_company_quota, reserve_company_quota,
};
pub use crate::domain::quota::{
    FundingSource, QuotaFinish, QuotaOutcome, QuotaReservation, quota_exhausted,
    reservation_unavailable,
};
