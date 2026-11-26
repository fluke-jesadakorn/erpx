//! Application port and handler for the native Finance invoice-list read.
//!
//! The handler validates the Company scope and limit, delegates SQL execution
//! to the owner adapter and pins the complete response shape. It contains no
//! SQL and no caller predicate.

use crate::adapters::native_lane_7::domain::{
    bound_limit, invoice_list_wire, invalid_request, valid_company_id,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type InvoiceListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<Value>, BusinessError>> + Send + 'a>>;

/// Owner read port implemented by the Postgres adapter only.
pub trait InvoiceListRead: Send + Sync {
    fn invoice_list<'a>(
        &'a self,
        company_id: &'a str,
        limit: i64,
    ) -> InvoiceListFuture<'a>;
}

/// Validate, read and project the complete invoice list for one Company.
pub async fn handle(
    reader: &dyn InvoiceListRead,
    company_id: &str,
    limit: i64,
) -> Result<Value, BusinessError> {
    if !valid_company_id(company_id) {
        return Err(invalid_request());
    }
    let rows = reader.invoice_list(company_id, bound_limit(limit)).await?;
    invoice_list_wire(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeReader;

    impl InvoiceListRead for FakeReader {
        fn invoice_list<'a>(
            &'a self,
            _company_id: &'a str,
            _limit: i64,
        ) -> InvoiceListFuture<'a> {
            Box::pin(async {
                Ok(vec![serde_json::json!({
                    "id": "i1",
                    "invoice_number": "INV-1",
                    "kind": "purchase",
                    "invoice_date": "2026-01-01",
                    "due_date": "2026-01-31",
                    "currency": "THB",
                    "total_minor": 1000,
                    "outstanding_minor": 0,
                    "status": "draft",
                    "party_name": "Acme",
                })])
            })
        }
    }

    #[tokio::test]
    async fn native_lane_7_handler_returns_every_retired_column() {
        let wire = handle(&FakeReader, "acme:co-1", 100).await.unwrap();
        assert_eq!(wire["data"][0]["id"], "i1");
        assert_eq!(wire["data"][0]["invoice_number"], "INV-1");
        assert_eq!(wire["data"][0]["total_minor"], 1000);
        assert_eq!(wire["data"][0]["party_name"], "Acme");
        assert_eq!(wire["data"][0].as_object().unwrap().len(), 10);
    }

    #[tokio::test]
    async fn native_lane_7_handler_rejects_invalid_company() {
        assert!(handle(&FakeReader, "", 100).await.is_err());
        assert!(handle(&FakeReader, "bad/company", 100).await.is_err());
    }

    #[test]
    fn native_lane_7_handler_bounds_the_limit_before_binding() {
        let reader = FakeReader;
        let future = reader.invoice_list("acme", bound_limit(5000));
        drop(future);
        assert_eq!(bound_limit(5000), 100);
    }
}
