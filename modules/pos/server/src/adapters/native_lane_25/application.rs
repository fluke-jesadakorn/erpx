//! Application port for the native POS evidence search. No SQL and no HTTP:
//! the owner reader executes the query and the HTTP layer only transports it.

use crate::adapters::native_lane_25::domain::{
    DayClosesQuery, DayClosesRow, EvidenceDetailQuery, EvidenceDetailRow, EvidenceQuery,
    EvidenceRow, invalid, validate, validate_day_closes, validate_detail,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type EvidenceFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition supplies the owner reader; the router never builds SQL or
/// derives a Company scope itself.
pub trait PosEvidenceReader: Send + Sync {
    fn search<'a>(&'a self, query: &'a EvidenceQuery) -> EvidenceFuture<'a, Vec<EvidenceRow>>;

    /// Wave 2 detail read. The default keeps single-purpose test readers
    /// compiling; the pool-backed owner reader overrides it.
    fn detail<'a>(
        &'a self,
        _: &'a EvidenceDetailQuery,
    ) -> EvidenceFuture<'a, Option<EvidenceDetailRow>> {
        Box::pin(async {
            Err(BusinessError::new(
                erpx_platform_shared_server::public::FailureKind::Unavailable,
                "POS_EVIDENCE_DETAIL_UNAVAILABLE",
                "The native POS evidence detail reader is unavailable.",
            ))
        })
    }

    /// Wave 3 day-close report list. The default keeps single-purpose test
    /// readers compiling; the pool-backed owner reader overrides it.
    fn day_closes<'a>(&'a self, _: &'a DayClosesQuery) -> EvidenceFuture<'a, Vec<DayClosesRow>> {
        Box::pin(async {
            Err(BusinessError::new(
                erpx_platform_shared_server::public::FailureKind::Unavailable,
                "POS_DAY_CLOSES_UNAVAILABLE",
                "The native POS day-close reader is unavailable.",
            ))
        })
    }
}

/// Validate, then delegate to the injected owner reader. Every call passes the
/// same bounded request the TypeScript client is required to send.
pub async fn read<R: PosEvidenceReader + ?Sized>(
    reader: &R,
    query: &EvidenceQuery,
) -> Result<Vec<EvidenceRow>, BusinessError> {
    validate(query)?;
    let rows = reader.search(query).await?;
    // One extra row is the retained `hasMore` sentinel; the application
    // truncates the page to `limit` itself.
    if rows.len() > query.limit + 1 {
        return Err(invalid());
    }
    Ok(rows)
}

/// Validate and delegate the bounded detail read to the owner reader.
pub async fn read_detail<R: PosEvidenceReader + ?Sized>(
    reader: &R,
    query: &EvidenceDetailQuery,
) -> Result<Option<EvidenceDetailRow>, BusinessError> {
    validate_detail(query)?;
    reader.detail(query).await
}

/// Validate and delegate the bounded day-close report list to the owner reader.
pub async fn read_day_closes<R: PosEvidenceReader + ?Sized>(
    reader: &R,
    query: &DayClosesQuery,
) -> Result<Vec<DayClosesRow>, BusinessError> {
    validate_day_closes(query)?;
    reader.day_closes(query).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_25::domain::{METRIC_POS_SALES, ROW_KEYS};
    use serde_json::Value;
    use std::task::{Context, Poll, Waker};

    /// The fake reader is immediately ready; no reactor is needed. The crate
    /// intentionally declares no direct tokio dev-dependency, so tests use a
    /// std-only executor rather than widening Cargo.toml outside the lane.
    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = Box::pin(future);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    struct Fake(Vec<Value>);
    impl PosEvidenceReader for Fake {
        fn search<'a>(&'a self, _: &'a EvidenceQuery) -> EvidenceFuture<'a, Vec<EvidenceRow>> {
            let rows = self
                .0
                .iter()
                .map(|v| {
                    let o = v.as_object().unwrap();
                    Ok(EvidenceRow {
                        id: o["id"].as_str().unwrap().into(),
                        source_id: o["source_id"].as_str().unwrap().into(),
                        source_type: o["source_type"].as_str().unwrap().into(),
                        sale_id: o["sale_id"].as_str().unwrap().into(),
                        event_date: o["event_date"].as_str().unwrap().into(),
                        receipt_number: o["receipt_number"].as_str().unwrap().into(),
                        status: o["status"].as_str().unwrap().into(),
                        sale_status: o["sale_status"].as_str().unwrap().into(),
                        currency: o["currency"].as_str().unwrap().into(),
                        method: o["method"].as_str().map(str::to_owned),
                        amount_minor: o["amount_minor"].as_str().unwrap().into(),
                        contribution_minor: o["contribution_minor"].as_str().unwrap().into(),
                        refunded_minor: o["refunded_minor"].as_str().unwrap().into(),
                        revision: o["revision"].as_str().unwrap().into(),
                        sale_version: o["sale_version"].as_str().unwrap().into(),
                    })
                })
                .collect::<Result<Vec<_>, BusinessError>>();
            Box::pin(async move { rows })
        }
    }

    fn query(limit: usize) -> EvidenceQuery {
        EvidenceQuery {
            company: "co".into(),
            branch: None,
            metric: METRIC_POS_SALES.into(),
            from: "2026-01-01".into(),
            to: "2026-01-31".into(),
            currency: "USD".into(),
            query: String::new(),
            limit,
            cursor: None,
        }
    }

    fn value() -> Value {
        serde_json::json!({
            "id":"sale:s1","source_id":"s1","source_type":"sale","sale_id":"s1",
            "event_date":"2026-01-02","receipt_number":"R1","status":"completed",
            "sale_status":"completed","currency":"USD","method":null,
            "amount_minor":"100","contribution_minor":"100","refunded_minor":"0",
            "revision":"1","sale_version":"1"
        })
    }

    #[test]
    fn read_returns_owner_rows_with_exact_shape() {
        let reader = Fake(vec![value()]);
        let rows = block_on(read(&reader, &query(25))).unwrap();
        assert_eq!(rows.len(), 1);
        let object = rows[0].clone().value();
        assert_eq!(object.as_object().unwrap().len(), ROW_KEYS.len());
    }

    #[test]
    fn read_rejects_invalid_request_before_calling_reader() {
        let mut q = query(25);
        q.metric = "unknown".into();
        let reader = Fake(vec![value()]);
        assert!(block_on(read(&reader, &q)).is_err());
    }

    #[test]
    fn read_rejects_oversized_result() {
        let reader = Fake(vec![value(), value()]);
        // limit + 1 is the retained hasMore sentinel and stays valid.
        assert!(block_on(read(&reader, &query(1))).is_ok());
        let reader = Fake(vec![value(), value(), value()]);
        assert!(block_on(read(&reader, &query(1))).is_err());
    }

    struct DayClosesFake;
    impl PosEvidenceReader for DayClosesFake {
        fn search<'a>(&'a self, _: &'a EvidenceQuery) -> EvidenceFuture<'a, Vec<EvidenceRow>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn day_closes<'a>(&'a self, _: &'a DayClosesQuery) -> EvidenceFuture<'a, Vec<DayClosesRow>> {
            use crate::adapters::native_lane_25::domain::DAY_CLOSES_KEYS;
            Box::pin(async move {
                Ok(vec![DayClosesRow {
                    values: DAY_CLOSES_KEYS
                        .iter()
                        .map(|key| {
                            let value = match *key {
                                "id" => "dc1".to_owned(),
                                "business_date" => "2026-01-31".to_owned(),
                                "currency" => "USD".to_owned(),
                                _ => "v".to_owned(),
                            };
                            ((*key).to_owned(), Some(value))
                        })
                        .collect(),
                }])
            })
        }
    }

    #[test]
    fn read_day_closes_returns_owner_rows_and_validates_scope() {
        let reader = DayClosesFake;
        let q = DayClosesQuery {
            company: "co".into(),
            branch: Some("br".into()),
        };
        let rows = block_on(read_day_closes(&reader, &q)).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].clone().value().as_object().unwrap().len(),
            crate::adapters::native_lane_25::domain::DAY_CLOSES_KEYS.len()
        );
        let mut bad = q;
        bad.company.clear();
        assert!(block_on(read_day_closes(&reader, &bad)).is_err());
    }
}
