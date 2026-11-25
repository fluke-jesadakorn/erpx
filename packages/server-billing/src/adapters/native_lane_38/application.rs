//! Query port and handler for the native lane 38 Billing watermark read.
//! This layer owns validation and the response projection; it never touches SQL.
use super::domain::{
    BillingWatermark, WatermarkRow, invalid, project, response_body, valid_holding,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
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
