//! Owner port and handler. This layer owns validation and the response shape;
//! it contains no SQL and no transport concern.

use super::domain::{Scope, StockAvailabilityResponse, project, validate_scope};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type StockAvailabilityFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the Inventory owner reader behind this port. The reader
/// owns the statement, the Company scope and the row decode.
pub trait StockAvailabilityRead: Send + Sync {
    fn rows<'a>(
        &'a self,
        scope: Scope<'a>,
    ) -> StockAvailabilityFuture<'a, Vec<super::domain::StockAvailabilityRow>>;
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "INVENTORY_LANE18_REQUEST_INVALID",
        "Use one bounded Company stock-availability read.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "INVENTORY_LANE18_READ_UNAVAILABLE",
        "The native Inventory stock service is unavailable.",
    )
}

/// Validate the scope, delegate execution to the owner reader, then project a
/// strictly bounded response. Every failure fails closed.
pub async fn read_stock_availability(
    reader: &dyn StockAvailabilityRead,
    scope: Scope<'_>,
) -> Result<StockAvailabilityResponse, BusinessError> {
    validate_scope(scope).map_err(|_| invalid())?;
    let rows = reader.rows(scope).await?;
    project(rows).map_err(|_| unavailable())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_18::domain::StockAvailabilityRow;

    struct Reader(bool);
    impl StockAvailabilityRead for Reader {
        fn rows<'a>(
            &'a self,
            _scope: Scope<'a>,
        ) -> StockAvailabilityFuture<'a, Vec<StockAvailabilityRow>> {
            let ok = self.0;
            Box::pin(async move {
                if ok {
                    Ok(vec![StockAvailabilityRow {
                        warehouse_id: "w1".into(),
                        product_id: "p1".into(),
                        quantity_milli: 1,
                        reserved_milli: 0,
                        quality_held: 1,
                    }])
                } else {
                    Err(unavailable())
                }
            })
        }
    }

    #[tokio::test]
    async fn handler_projects_owner_rows() {
        let response = read_stock_availability(
            &Reader(true),
            Scope {
                company: "c1",
                branch: None,
            },
        )
        .await
        .expect("read");
        assert_eq!(response.count, 1);
        assert!(response.rows[0].held());
    }

    #[tokio::test]
    async fn handler_fails_closed_on_scope_and_reader_errors() {
        assert_eq!(
            read_stock_availability(
                &Reader(true),
                Scope {
                    company: "",
                    branch: None
                }
            )
            .await
            .unwrap_err()
            .code,
            "INVENTORY_LANE18_REQUEST_INVALID"
        );
        assert_eq!(
            read_stock_availability(
                &Reader(false),
                Scope {
                    company: "c1",
                    branch: None
                }
            )
            .await
            .unwrap_err()
            .code,
            "INVENTORY_LANE18_READ_UNAVAILABLE"
        );
    }
}
