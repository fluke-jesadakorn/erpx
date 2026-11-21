//! Per-Company AI quota reservation/finish. The port owns the Company advisory
//! lock and ledger; this layer only validates facts and maps decisions.
use crate::domain::quota::{
    QuotaFinish, QuotaReservation, quota_exhausted, reservation_unavailable,
};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use std::future::Future;

pub trait CompanyQuotaPorts {
    type Error: From<BusinessError> + Send;

    /// Acquire the retained `ai-request-budget:<company>` transaction lock and
    /// insert one reservation when the Company still has request capacity.
    fn reserve(
        &mut self,
        reservation: &QuotaReservation,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    /// Complete a reservation exactly once. This never changes the quota lock
    /// or creates a second ledger row.
    fn finish(
        &mut self,
        finish: &QuotaFinish,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}

pub async fn reserve_company_quota<P: CompanyQuotaPorts>(
    ports: &mut P,
    reservation: &QuotaReservation,
) -> Result<Value, P::Error> {
    reservation.validate()?;
    if !ports.reserve(reservation).await? {
        return Err(quota_exhausted().into());
    }
    Ok(json!({
        "id": reservation.request_id,
        "status": "reserved",
        "reservationSatang": reservation.reservation_satang,
    }))
}

pub async fn finish_company_quota<P: CompanyQuotaPorts>(
    ports: &mut P,
    finish: &QuotaFinish,
) -> Result<Value, P::Error> {
    finish.validate()?;
    if !ports.finish(finish).await? {
        return Err(reservation_unavailable().into());
    }
    Ok(json!({
        "id": finish.request_id,
        "status": finish.outcome.as_str(),
        "inputTokens": finish.input_tokens,
        "outputTokens": finish.output_tokens,
    }))
}
