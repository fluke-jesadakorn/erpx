//! Owner port and handler for the native transfer-detail read. This layer owns
//! validation and the response shape; it contains no SQL and no transport.

use crate::adapters::native_transfer_detail::domain::{
    TransferDetailRequest, TransferDetailResponse,
};
use erpx_server_runtime::public::http_support::serde_json::Value;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{future::Future, pin::Pin};

pub type TransferDetailFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the Inventory owner reader behind this port. The reader
/// owns the statement, the Company/branch scope and the row decode.
pub trait InventoryTransferDetailRead: Send + Sync {
    fn get<'a>(
        &'a self,
        request: &'a TransferDetailRequest,
    ) -> TransferDetailFuture<'a, Option<Value>>;
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "INVENTORY_TRANSFER_DETAIL_REQUEST_INVALID",
        "Use one bounded native Inventory transfer read.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "TRANSFER_NOT_FOUND",
        "Transfer not found in this workspace.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INVENTORY_TRANSFER_DETAIL_UNAVAILABLE",
        "The native Inventory transfer reader is unavailable.",
    )
}

/// Validate the request, delegate execution to the owner reader and wrap the
/// complete projected row in the fixed response envelope. A missing row becomes
/// the same not-found error the retired TypeScript adapter raised.
pub async fn read_transfer_detail(
    reader: &dyn InventoryTransferDetailRead,
    request: &TransferDetailRequest,
) -> Result<TransferDetailResponse, BusinessError> {
    request.validate().map_err(|_| invalid())?;
    let row = reader.get(request).await?;
    Ok(TransferDetailResponse { row })
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::serde_json::json;

    struct Reader(bool);
    impl InventoryTransferDetailRead for Reader {
        fn get<'a>(
            &'a self,
            _request: &'a TransferDetailRequest,
        ) -> TransferDetailFuture<'a, Option<Value>> {
            let ok = self.0;
            Box::pin(async move {
                if ok {
                    Ok(Some(json!({"id":"transfer-1","company_id":"company-1"})))
                } else {
                    Err(unavailable())
                }
            })
        }
    }

    fn request() -> TransferDetailRequest {
        TransferDetailRequest {
            company: "company-1".into(),
            branch: None,
            id: "transfer-1".into(),
        }
    }

    #[tokio::test]
    async fn handler_wraps_the_complete_owner_row() {
        let response = read_transfer_detail(&Reader(true), &request())
            .await
            .expect("read");
        assert_eq!(
            response.row,
            Some(json!({"id":"transfer-1","company_id":"company-1"}))
        );
    }

    #[tokio::test]
    async fn handler_fails_closed_on_request_and_reader_errors() {
        assert_eq!(
            read_transfer_detail(
                &Reader(true),
                &TransferDetailRequest {
                    company: String::new(),
                    ..request()
                }
            )
            .await
            .unwrap_err()
            .code,
            "INVENTORY_TRANSFER_DETAIL_REQUEST_INVALID"
        );
        assert_eq!(
            read_transfer_detail(&Reader(false), &request())
                .await
                .unwrap_err()
                .code,
            "INVENTORY_TRANSFER_DETAIL_UNAVAILABLE"
        );
    }
}
