//! Owner query port and handler. No SQL lives in this layer.

use super::domain::{InvoiceLine, InvoiceLineRequest};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin, sync::Arc};

pub type InvoiceLineReadFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<InvoiceLine>, BusinessError>> + Send + 'a>>;

/// The owner owns the query and the projection. Composition injects the
/// PostgreSQL implementation; the HTTP adapter only transports bounded facts.
pub trait InvoiceLineRead: Send + Sync {
    fn lines<'a>(&'a self, request: &'a InvoiceLineRequest) -> InvoiceLineReadFuture<'a>;
}

/// Validate before crossing the owner boundary, then delegate to the injected
/// reader. A missing or failed reader surfaces as the owner's unavailable error.
pub async fn current_invoice_lines(
    reader: &dyn InvoiceLineRead,
    request: &InvoiceLineRequest,
) -> Result<Vec<InvoiceLine>, BusinessError> {
    request.validate()?;
    reader.lines(request).await
}

/// Convenience for composition-owned static readers.
pub async fn current_invoice_lines_arc(
    reader: Arc<dyn InvoiceLineRead>,
    request: &InvoiceLineRequest,
) -> Result<Vec<InvoiceLine>, BusinessError> {
    current_invoice_lines(reader.as_ref(), request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_3::domain::{fixture, invalid};

    struct Fake {
        lines: Vec<InvoiceLine>,
        reject: bool,
    }

    impl InvoiceLineRead for Fake {
        fn lines<'a>(&'a self, request: &'a InvoiceLineRequest) -> InvoiceLineReadFuture<'a> {
            let lines = self.lines.clone();
            let reject = self.reject;
            let company = request.company_id.clone();
            Box::pin(async move {
                if reject || company != "company-1" {
                    return Err(invalid());
                }
                Ok(lines)
            })
        }
    }

    #[tokio::test]
    async fn handler_returns_owner_projection() {
        let reader = Fake { lines: fixture(), reject: false };
        let request = InvoiceLineRequest {
            company_id: "company-1".into(),
            invoice_id: "inv-1".into(),
            revision: 3,
        };
        let lines = current_invoice_lines(&reader, &request)
            .await
            .expect("read succeeds");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].id, "line-1");
        assert_eq!(lines[1].total_minor, 10700);
    }

    #[tokio::test]
    async fn handler_rejects_before_delegation() {
        let reader = Fake { lines: fixture(), reject: false };
        let request = InvoiceLineRequest {
            company_id: "company/1".into(),
            invoice_id: "inv-1".into(),
            revision: 3,
        };
        assert_eq!(
            current_invoice_lines(&reader, &request)
                .await
                .err()
                .expect("rejected"),
            invalid()
        );
    }

    #[tokio::test]
    async fn handler_surfaces_owner_failure_closed() {
        let reader: Arc<dyn InvoiceLineRead> = Arc::new(Fake { lines: fixture(), reject: true });
        let request = InvoiceLineRequest {
            company_id: "company-1".into(),
            invoice_id: "inv-1".into(),
            revision: 3,
        };
        let error = current_invoice_lines_arc(reader, &request)
            .await
            .err()
            .expect("fails closed");
        assert_eq!(error.code, "LANE3_INVOICE_LINES_INPUT_INVALID");
    }
}
