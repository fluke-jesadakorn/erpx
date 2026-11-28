//! Application query for the native Finance root aggregate read.
//!
//! Validates the Company and ledger scope, delegates all SELECTs to the owner
//! Postgres adapter and pins the exact `{data: {...}}` wire shape. No SQL here.

use crate::adapters::finance_root_aggregate::domain::{
    canonical_envelope, invalid_request, valid_chart_id, valid_company_id,
};
use erpx_server_shared::public::BusinessError;
use serde::Serialize;
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type AggregateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<RootAggregate, BusinessError>> + Send + 'a>>;

/// The five collections the retired `GET /finance` handler returned.
#[derive(Clone, Debug, Serialize)]
pub struct RootAggregate {
    pub charts: Vec<Value>,
    pub journals: Vec<Value>,
    pub invoices: Vec<Value>,
    pub payments: Vec<Value>,
    pub report: Value,
}

/// Owner read port implemented by the Postgres adapter only.
pub trait RootAggregateRead: Send + Sync {
    fn aggregate<'a>(
        &'a self,
        company_id: &'a str,
        chart_id: Option<&'a str>,
    ) -> AggregateFuture<'a>;
}

/// Bounded rows; an oversized reply fails closed instead of being truncated.
const MAX_CHARTS: usize = 20_001;
const MAX_JOURNALS: usize = 51;
const MAX_INVOICES: usize = 101;
const MAX_PAYMENTS: usize = 101;

fn bounded_rows(rows: &[Value], max: usize) -> Result<(), BusinessError> {
    if rows.len() > max || rows.iter().any(|row| !matches!(row, Value::Object(_))) {
        Err(invalid_request())
    } else {
        Ok(())
    }
}

/// Validate, read and project the complete Finance root payload.
pub async fn handle(
    reader: &dyn RootAggregateRead,
    company_id: &str,
    chart_id: Option<&str>,
) -> Result<Value, BusinessError> {
    if !valid_company_id(company_id) {
        return Err(invalid_request());
    }
    if chart_id.is_some_and(|id| !valid_chart_id(id)) {
        return Err(invalid_request());
    }
    let aggregate = reader.aggregate(company_id, chart_id).await?;
    bounded_rows(&aggregate.charts, MAX_CHARTS)?;
    bounded_rows(&aggregate.journals, MAX_JOURNALS)?;
    bounded_rows(&aggregate.invoices, MAX_INVOICES)?;
    bounded_rows(&aggregate.payments, MAX_PAYMENTS)?;
    Ok(canonical_envelope(serde_json::to_value(aggregate).map_err(
        |_| invalid_request(),
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeReader;

    impl RootAggregateRead for FakeReader {
        fn aggregate<'a>(
            &'a self,
            _company_id: &'a str,
            _chart_id: Option<&'a str>,
        ) -> AggregateFuture<'a> {
            Box::pin(async {
                Ok(RootAggregate {
                    charts: vec![serde_json::json!({"id": "c1", "name": "Operations ledger"})],
                    journals: vec![serde_json::json!({"id": "j1"})],
                    invoices: vec![serde_json::json!({"id": "i1"})],
                    payments: vec![serde_json::json!({"id": "p1"})],
                    report: serde_json::json!({"balanceSheet": {"assets": "1"}}),
                })
            })
        }
    }

    struct OversizedReader;

    impl RootAggregateRead for OversizedReader {
        fn aggregate<'a>(
            &'a self,
            _company_id: &'a str,
            _chart_id: Option<&'a str>,
        ) -> AggregateFuture<'a> {
            Box::pin(async {
                Ok(RootAggregate {
                    charts: vec![],
                    journals: vec![],
                    invoices: vec![],
                    payments: vec![Value::from(1); MAX_PAYMENTS + 1],
                    report: Value::Null,
                })
            })
        }
    }

    #[tokio::test]
    async fn root_aggregate_handler_returns_the_complete_envelope() {
        let wire = handle(&FakeReader, "acme-co", None).await.unwrap();
        assert_eq!(wire["data"]["charts"][0]["name"], "Operations ledger");
        assert_eq!(wire["data"]["journals"][0]["id"], "j1");
        assert_eq!(wire["data"]["invoices"][0]["id"], "i1");
        assert_eq!(wire["data"]["payments"][0]["id"], "p1");
        assert_eq!(wire["data"]["report"]["balanceSheet"]["assets"], "1");
        assert!(wire.get("data").is_some());
        assert!(wire.get("charts").is_none());
    }

    #[tokio::test]
    async fn root_aggregate_handler_rejects_invalid_scope_and_oversized_rows() {
        assert!(handle(&FakeReader, "", None).await.is_err());
        assert!(handle(&FakeReader, "bad/company", None).await.is_err());
        assert!(handle(&FakeReader, "acme-co", Some("bad/chart")).await.is_err());
        assert!(handle(&OversizedReader, "acme-co", None).await.is_err());
    }
}
