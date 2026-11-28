//! Query port and handler for the native lane 38 Billing watermark read.
//! This layer owns validation and the response projection; it never touches SQL.
use super::domain::{
    BillingWatermark, WatermarkRow, invalid, project, response_body, valid_holding,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::{Value, json};
use std::{future::Future, pin::Pin};

pub type Lane38Future<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader port. Composition injects the PostgreSQL-backed reader; the
/// handler below is the only caller and applies authorization-safe scope
/// validation before the reader ever executes a statement.
pub trait BillingWatermarkRead: Send + Sync {
    fn watermarks<'a>(&'a self, holding: &'a str) -> Lane38Future<'a, Option<WatermarkRow>>;
}

/// Validate the company/holding scope, execute the owner read and return the
/// bounded wire body. Any reader failure is mapped to a fail-closed 503.
pub async fn read_watermark_body(
    reader: &dyn BillingWatermarkRead,
    holding: &str,
) -> Result<Value, BusinessError> {
    if !valid_holding(holding) {
        return Err(invalid());
    }
    let row = reader
        .watermarks(holding)
        .await
        .map_err(|_| super::domain::unavailable())?;
    let watermarks: Vec<BillingWatermark> = project(row);
    Ok(response_body(&watermarks))
}

// ---- Executive-main evidence family (wave 2) ------------------------------

use super::domain::{
    COMMERCIAL_ROW_KEYS, CommercialPointInput, CommercialSearchInput, OVERDUE_ROW_KEYS,
    OverduePointInput, OverdueSearchInput, exact_row, validate_commercial_point,
    validate_commercial_search, validate_overdue_point, validate_overdue_search,
};

/// Owner reader port for the four Billing-owned evidence reads. SQL execution
/// stays in the PostgreSQL adapter; this layer owns validation and shape.
pub trait BillingEvidenceRead: Send + Sync {
    fn commercial_search<'a>(
        &'a self,
        holding: &'a str,
        input: &'a CommercialSearchInput,
    ) -> Lane38Future<'a, Vec<Value>>;

    fn commercial_point<'a>(
        &'a self,
        holding: &'a str,
        input: &'a CommercialPointInput,
    ) -> Lane38Future<'a, Option<Value>>;

    fn overdue_search<'a>(
        &'a self,
        holding: &'a str,
        input: &'a OverdueSearchInput,
    ) -> Lane38Future<'a, Vec<Value>>;

    fn overdue_point<'a>(
        &'a self,
        holding: &'a str,
        input: &'a OverduePointInput,
    ) -> Lane38Future<'a, Option<Value>>;
}

fn shape(rows: Vec<Value>, keys: &[&str]) -> Result<Value, BusinessError> {
    if !rows.iter().all(|row| exact_row(row, keys)) {
        return Err(super::domain::unavailable());
    }
    Ok(json!({ "rows": rows }))
}

fn scope(holding: &str) -> Result<(), BusinessError> {
    if !valid_holding(holding) {
        return Err(invalid());
    }
    Ok(())
}

pub async fn read_commercial_search(
    reader: &dyn BillingEvidenceRead,
    holding: &str,
    input: &CommercialSearchInput,
) -> Result<Value, BusinessError> {
    scope(holding)?;
    validate_commercial_search(input)?;
    shape(
        reader.commercial_search(holding, input).await?,
        &COMMERCIAL_ROW_KEYS,
    )
}

pub async fn read_commercial_point(
    reader: &dyn BillingEvidenceRead,
    holding: &str,
    input: &CommercialPointInput,
) -> Result<Value, BusinessError> {
    scope(holding)?;
    validate_commercial_point(input)?;
    let row = reader.commercial_point(holding, input).await?;
    match row {
        Some(row) if exact_row(&row, &COMMERCIAL_ROW_KEYS) => Ok(json!({ "row": row })),
        None => Ok(json!({ "row": Value::Null })),
        Some(_) => Err(super::domain::unavailable()),
    }
}

pub async fn read_overdue_search(
    reader: &dyn BillingEvidenceRead,
    holding: &str,
    input: &OverdueSearchInput,
) -> Result<Value, BusinessError> {
    scope(holding)?;
    validate_overdue_search(input)?;
    shape(
        reader.overdue_search(holding, input).await?,
        &OVERDUE_ROW_KEYS,
    )
}

pub async fn read_overdue_point(
    reader: &dyn BillingEvidenceRead,
    holding: &str,
    input: &OverduePointInput,
) -> Result<Value, BusinessError> {
    scope(holding)?;
    validate_overdue_point(input)?;
    let row = reader.overdue_point(holding, input).await?;
    match row {
        Some(row) if exact_row(&row, &OVERDUE_ROW_KEYS) => Ok(json!({ "row": row })),
        None => Ok(json!({ "row": Value::Null })),
        Some(_) => Err(super::domain::unavailable()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(Option<WatermarkRow>);
    impl BillingWatermarkRead for Fixed {
        fn watermarks<'a>(&'a self, holding: &'a str) -> Lane38Future<'a, Option<WatermarkRow>> {
            let holding = holding.to_owned();
            let row = self.0.clone();
            Box::pin(async move {
                assert_eq!(holding, "acme");
                Ok(row)
            })
        }
    }

    #[tokio::test]
    async fn native_lane_38_handler_projects_owner_row() {
        let body = read_watermark_body(
            &Fixed(Some(WatermarkRow {
                account_version: "1".to_owned(),
                enrollment_revision: "e".to_owned(),
                invoice_revision: "i".to_owned(),
                legacy_subscription_revision: "l".to_owned(),
                plan_revision: "p".to_owned(),
            })),
            "acme",
        )
        .await
        .expect("bounded body");
        assert_eq!(body["watermarks"][0]["table_name"], "billing_snapshot");
        assert_eq!(body["watermarks"].as_array().map(Vec::len), Some(1));
    }

    #[tokio::test]
    async fn native_lane_38_handler_rejects_unbounded_holding() {
        let err = read_watermark_body(&Fixed(None), "bad scope")
            .await
            .expect_err("invalid scope fails closed");
        assert_eq!(err.code, "NATIVE_LANE_38_BILLING_WATERMARK_REQUEST_INVALID");
    }
}
