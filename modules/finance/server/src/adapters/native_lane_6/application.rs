//! Application port and handler for the native Finance chart-list read.
//!
//! The handler validates the Company scope, delegates SQL execution to the
//! owner adapter and pins the complete response shape. It contains no SQL.

use crate::adapters::native_lane_6::domain::{
    chart_list_wire, invalid_request, valid_company_id,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type ChartListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<Value>, BusinessError>> + Send + 'a>>;

/// Owner read port implemented by the Postgres adapter only.
pub trait ChartListRead: Send + Sync {
    fn chart_list<'a>(&'a self, company_id: &'a str) -> ChartListFuture<'a>;
}

/// Validate, read and project the complete chart list for one Company.
pub async fn handle(
    reader: &dyn ChartListRead,
    company_id: &str,
) -> Result<Value, BusinessError> {
    if !valid_company_id(company_id) {
        return Err(invalid_request());
    }
    let rows = reader.chart_list(company_id).await?;
    chart_list_wire(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeReader;

    impl ChartListRead for FakeReader {
        fn chart_list<'a>(&'a self, _company_id: &'a str) -> ChartListFuture<'a> {
            Box::pin(async { Ok(vec![serde_json::json!({"id": "c1", "name": "Operations ledger"})]) })
        }
    }

    #[tokio::test]
    async fn native_lane_6_handler_returns_the_full_chart_row() {
        let wire = handle(&FakeReader, "acme-co").await.unwrap();
        assert_eq!(wire["data"][0]["id"], "c1");
        assert_eq!(wire["data"][0]["name"], "Operations ledger");
    }

    #[tokio::test]
    async fn native_lane_6_handler_rejects_invalid_company() {
        assert!(handle(&FakeReader, "").await.is_err());
        assert!(handle(&FakeReader, "bad/company").await.is_err());
    }
}
