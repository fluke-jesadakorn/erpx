//! Billing target assignment/price read. This is the bounded assignment
//! prerequisite for the target billing period; it does not rate, approve,
//! invoice or read a Holding database.
use crate::domain::forecast_read::safe_number;
use crate::domain::target_pricing::{TargetPrice, month_bounds};
use erpx_server_shared::public::{BusinessError, HoldingId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetAssignment {
    pub effective_month: String,
    pub mode: String,
    pub price_id: String,
    pub service_started_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetAssignmentRead {
    pub assignment: Option<TargetAssignment>,
    pub price: Option<TargetPrice>,
}

#[derive(Clone, Debug, Default)]
pub struct TargetAssignmentRow {
    pub holding_id: String,
    pub effective_month: String,
    pub price_id: String,
    pub mode: String,
    pub service_started_at: String,
}

#[derive(Clone, Debug, Default)]
pub struct TargetPriceRow {
    pub id: String,
    pub effective_month: String,
    pub base_monthly_satang: String,
    pub included_original_bytes: String,
    pub storage_satang_per_gb_month: String,
    pub ai_markup_percent: i32,
    pub seat_terms_status: String,
    pub included_seats: Option<i32>,
    pub seat_monthly_satang: Option<String>,
    pub storage_allowance_mode: String,
    pub rating_enabled: bool,
}

fn invalid(code: &'static str) -> BusinessError {
    BusinessError::invalid(code, "The target billing source is invalid.")
}

pub fn map_assignment(row: &TargetAssignmentRow) -> Result<TargetAssignment, BusinessError> {
    if !matches!(row.mode.as_str(), "shadow" | "sandbox_invoice") {
        return Err(invalid("INVALID_TARGET_MODE"));
    }
    let service_started_at = safe_number(&row.service_started_at)?;
    if service_started_at < 1 {
        return Err(invalid("INVALID_SERVICE_START"));
    }
    Ok(TargetAssignment {
        effective_month: row.effective_month.clone(),
        mode: row.mode.clone(),
        price_id: row.price_id.clone(),
        service_started_at,
    })
}

pub fn map_price(row: &TargetPriceRow) -> Result<TargetPrice, BusinessError> {
    if !matches!(row.seat_terms_status.as_str(), "undefined" | "defined")
        || !matches!(
            row.storage_allowance_mode.as_str(),
            "holding_flat" | "per_paid_seat_time"
        )
    {
        return Err(invalid("INVALID_TARGET_PRICE"));
    }
    Ok(TargetPrice {
        id: row.id.clone(),
        effective_month: row.effective_month.clone(),
        base_monthly_satang: safe_number(&row.base_monthly_satang)?,
        included_original_bytes: safe_number(&row.included_original_bytes)?,
        storage_satang_per_gb_month: safe_number(&row.storage_satang_per_gb_month)?,
        ai_markup_percent: i64::from(row.ai_markup_percent),
        seat_terms_status: row.seat_terms_status.clone(),
        included_seats: row.included_seats.map(i64::from),
        seat_monthly_satang: row
            .seat_monthly_satang
            .as_deref()
            .map(safe_number)
            .transpose()?,
        storage_allowance_mode: row.storage_allowance_mode.clone(),
        rating_enabled: row.rating_enabled,
    })
}

pub fn read_target_assignment(
    holding: &HoldingId,
    month: &str,
    assignment_row: Option<TargetAssignmentRow>,
    price_row: Option<TargetPriceRow>,
) -> Result<TargetAssignmentRead, BusinessError> {
    let (month_start, month_end) = month_bounds(month)?;
    let _ = (holding, month_start, month_end);
    let assignment = assignment_row.as_ref().map(map_assignment).transpose()?;
    if let Some(assignment) = &assignment
        && assignment.effective_month.as_str() > month
    {
        return Err(invalid("INVALID_TARGET_ASSIGNMENT"));
    }
    let price = price_row.as_ref().map(map_price).transpose()?;
    if let Some(price) = &price
        && (price.effective_month.as_str() > month
            || assignment
                .as_ref()
                .is_some_and(|assignment| assignment.price_id != price.id))
    {
        return Err(invalid("INVALID_TARGET_PRICE"));
    }
    Ok(TargetAssignmentRead { assignment, price })
}
