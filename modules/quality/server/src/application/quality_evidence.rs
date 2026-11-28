//! Application port for the native Executive Quality evidence search.
//!
//! Validation and the `limit + 1` pagination contract live here; no SQL text
//! or HTTP concern crosses this boundary.

use crate::domain::quality_evidence::{
    InspectionEvidenceQuery, InspectionEvidenceRow, invalid, validate, valid_row,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type QualityEvidenceFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition supplies the owner reader; the router never builds SQL and
/// never derives a Company scope itself.
pub trait QualityEvidenceReader: Send + Sync {
    fn inspection_evidence<'a>(
        &'a self,
        query: &'a InspectionEvidenceQuery,
    ) -> QualityEvidenceFuture<'a, Vec<InspectionEvidenceRow>>;
}

/// Validate, delegate and fail closed on a malformed projection. The caller
/// computes `hasMore` from the retained `limit + 1` sentinel exactly as the
/// retired TypeScript search did.
pub async fn read_inspection_evidence<R: QualityEvidenceReader + ?Sized>(
    reader: &R,
    query: &InspectionEvidenceQuery,
) -> Result<Vec<InspectionEvidenceRow>, BusinessError> {
    validate(query)?;
    let rows = reader.inspection_evidence(query).await?;
    if rows.len() > query.limit + 1 || !rows.iter().all(valid_row) {
        return Err(invalid());
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::{Context, Poll, Waker};

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

    struct Fake(usize);
    impl QualityEvidenceReader for Fake {
        fn inspection_evidence<'a>(
            &'a self,
            _: &'a InspectionEvidenceQuery,
        ) -> QualityEvidenceFuture<'a, Vec<InspectionEvidenceRow>> {
            let rows = (0..self.0)
                .map(|_| InspectionEvidenceRow {
                    id: "i1".into(),
                    hold_id: "h1".into(),
                    result: "pass".into(),
                    created_at: 1_767_225_600_000,
                    reviewed_version: 3,
                    product_name: "Rice".into(),
                    warehouse_name: "Main".into(),
                })
                .collect();
            Box::pin(async move { Ok(rows) })
        }
    }

    fn query(limit: usize) -> InspectionEvidenceQuery {
        InspectionEvidenceQuery {
            company: "co".into(),
            branch: None,
            from: "2026-01-01".into(),
            to: "2026-01-31".into(),
            currency: "THB".into(),
            query: String::new(),
            limit,
            cursor: None,
        }
    }

    #[test]
    fn keeps_limit_plus_one_sentinel_and_exact_row_shape() {
        let rows = block_on(read_inspection_evidence(&Fake(2), &query(1))).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].clone().value().as_object().unwrap().len(), 7);
    }

    #[test]
    fn rejects_oversized_page_and_invalid_request_before_reader() {
        assert!(block_on(read_inspection_evidence(&Fake(3), &query(1))).is_err());
        let mut q = query(25);
        q.currency = "thb".into();
        assert!(block_on(read_inspection_evidence(&Fake(1), &q)).is_err());
    }
}
