//! Application port and handler for the native overview-counts read.
//!
//! No SQL lives here: composition supplies the owner reader, and this handler
//! only enforces the Company boundary and validates the projection.

use crate::adapters::native_lane_2::domain::{
    OverviewCounts, invalid_request, valid_company_id,
};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type OverviewCountsFuture<'a> =
    Pin<Box<dyn Future<Output = Result<OverviewCounts, BusinessError>> + Send + 'a>>;

/// Owner read port. The sqlx adapter implements it; the HTTP router only
/// transports the validated result.
pub trait OverviewCountsQuery: Send + Sync {
    fn counts<'a>(&'a self, company_id: &'a str) -> OverviewCountsFuture<'a>;
}

/// Validate the Company scope, run the owner read and pin the response shape.
pub async fn load(
    query: &dyn OverviewCountsQuery,
    company_id: &str,
) -> Result<OverviewCounts, BusinessError> {
    if !valid_company_id(company_id) {
        return Err(invalid_request());
    }
    query.counts(company_id).await?.validated()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_2::domain::unavailable;

    struct Stub(bool);
    impl OverviewCountsQuery for Stub {
        fn counts<'a>(&'a self, company_id: &'a str) -> OverviewCountsFuture<'a> {
            let company_id = company_id.to_owned();
            let fail = self.0;
            Box::pin(async move {
                if fail || company_id != "acme:co" {
                    return Err(unavailable());
                }
                Ok(OverviewCounts {
                    journals: 7,
                    posting: 2,
                    invoices: 11,
                    payments: 5,
                    receivable: 3,
                    payable: 1,
                    reconcile: 4,
                })
            })
        }
    }

    #[tokio::test]
    async fn load_returns_the_pinned_projection() {
        let counts = load(&Stub(false), "acme:co").await.expect("counts");
        assert_eq!(
            counts,
            OverviewCounts {
                journals: 7,
                posting: 2,
                invoices: 11,
                payments: 5,
                receivable: 3,
                payable: 1,
                reconcile: 4,
            }
        );
    }

    #[tokio::test]
    async fn load_fails_closed_on_invalid_company() {
        assert_eq!(
            load(&Stub(false), "bad/id").await.unwrap_err(),
            invalid_request()
        );
    }

    #[tokio::test]
    async fn load_fails_closed_when_owner_read_fails() {
        assert_eq!(load(&Stub(true), "acme:co").await.unwrap_err(), unavailable());
    }
}
