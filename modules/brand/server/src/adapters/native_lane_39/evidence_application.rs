//! Lane 39 Brand evidence application: the SQL-free query port and handler.
//!
//! The owner reader executes the exact registered evidence statement with
//! owner-controlled predicates; this module validates the declared families,
//! projects the rows and owns the response shape.

use crate::adapters::native_lane_39::evidence_domain::{
    BrandProfileRow, EvidenceSearchRequest, project, validate,
};
use erpx_platform_runtime_server::public::http_support::serde_json::{json, Value};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{future::Future, pin::Pin};

pub type BrandEvidenceFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub trait BrandEvidenceRead: Send + Sync {
    fn search<'a>(
        &'a self,
        request: &'a EvidenceSearchRequest,
    ) -> BrandEvidenceFuture<'a, Vec<BrandProfileRow>>;
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BRAND_EVIDENCE_SEARCH_UNAVAILABLE",
        "The native Brand evidence service is unavailable.",
    )
}

pub async fn read_brand_evidence(
    reader: &dyn BrandEvidenceRead,
    request: EvidenceSearchRequest,
) -> Result<Value, BusinessError> {
    validate(&request)?;
    let rows = reader.search(&request).await.map_err(|error| {
        if error.kind == FailureKind::InvalidInput {
            error
        } else {
            unavailable()
        }
    })?;
    let rows = project(rows)?;
    let rows = rows
        .into_iter()
        .map(|row| {
            json!({
                "id": row.id,
                "branch_name": row.branch_name,
                "document_mode": row.document_mode,
                "updated_at": row.updated_at,
                "primary_attachment_id": row.primary_attachment_id,
                "inverse_attachment_id": row.inverse_attachment_id,
            })
        })
        .collect::<Vec<Value>>();
    Ok(json!({ "rows": rows }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_39::evidence_domain::EvidenceCursor;
    use std::sync::{Arc, Mutex};

    struct Fake {
        seen: Arc<Mutex<usize>>,
    }

    impl BrandEvidenceRead for Fake {
        fn search<'a>(
            &'a self,
            request: &'a EvidenceSearchRequest,
        ) -> BrandEvidenceFuture<'a, Vec<BrandProfileRow>> {
            *self.seen.lock().unwrap() += 1;
            let rows = vec![BrandProfileRow {
                id: "profile-1".into(),
                branch_name: "North".into(),
                document_mode: "branch".into(),
                updated_at: 7,
                primary_attachment_id: Some("image-1".into()),
                inverse_attachment_id: None,
            }];
            let _ = request;
            Box::pin(async move { Ok(rows) })
        }
    }

    fn request() -> EvidenceSearchRequest {
        EvidenceSearchRequest {
            company_id: "co-1".into(),
            branch_id: Some("branch-1".into()),
            from: Some(1),
            to_exclusive: Some(2),
            query: Some("north".into()),
            cursor: Some(EvidenceCursor {
                branch_name: "North".into(),
                id: "profile-0".into(),
            }),
            limit: 25,
        }
    }

    #[tokio::test]
    async fn handler_pins_the_exact_evidence_row_shape() {
        let seen = Arc::new(Mutex::new(0));
        let reader = Fake { seen: seen.clone() };
        let value = read_brand_evidence(&reader, request()).await.unwrap();
        assert_eq!(*seen.lock().unwrap(), 1);
        assert_eq!(
            value,
            json!({"rows":[{
                "id":"profile-1","branch_name":"North","document_mode":"branch",
                "updated_at":7,"primary_attachment_id":"image-1","inverse_attachment_id":null
            }]})
        );
    }

    #[tokio::test]
    async fn handler_fails_closed_before_reading_an_invalid_request() {
        let seen = Arc::new(Mutex::new(0));
        let reader = Fake { seen: seen.clone() };
        let mut bad = request();
        bad.limit = 0;
        assert_eq!(
            read_brand_evidence(&reader, bad).await.unwrap_err().code,
            "BRAND_EVIDENCE_SEARCH_REQUEST_INVALID"
        );
        assert_eq!(*seen.lock().unwrap(), 0);
    }
}
