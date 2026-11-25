//! Application port and handler for the native beta feedback read.
//!
//! No SQL lives here: the sqlx adapter implements the owner port and this
//! handler only enforces the Company boundary and the response invariants.

use crate::adapters::native_lane_34::domain::{FeedbackPage, invalid_request, valid_company_id};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type FeedbackFuture<'a> =
    Pin<Box<dyn Future<Output = Result<FeedbackPage, BusinessError>> + Send + 'a>>;

/// Owner read port. The Postgres adapter owns the SQL, the bind order and the
/// transaction; the HTTP router only transports the validated result.
pub trait BetaFeedbackQuery: Send + Sync {
    fn list<'a>(&'a self, company: &'a str) -> FeedbackFuture<'a>;
}

/// Validate the Company scope, run the owner read and pin the projection.
pub async fn load(
    query: &dyn BetaFeedbackQuery,
    company: &str,
) -> Result<FeedbackPage, BusinessError> {
    if !valid_company_id(company) {
        return Err(invalid_request());
    }
    query.list(company).await?.validated()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_34::domain::{FeedbackItem, unavailable};

    struct Stub {
        fail: bool,
    }

    impl BetaFeedbackQuery for Stub {
        fn list<'a>(&'a self, company: &'a str) -> FeedbackFuture<'a> {
            let company = company.to_owned();
            let fail = self.fail;
            Box::pin(async move {
                if fail {
                    return Err(unavailable());
                }
                Ok(FeedbackPage {
                    items: vec![FeedbackItem {
                        id: format!("fb-{company}"),
                        body: "hello".to_owned(),
                        created_at: 1_700_000_000_000,
                    }],
                })
            })
        }
    }

    fn stub(fail: bool) -> Stub {
        Stub { fail }
    }

    #[tokio::test]
    async fn load_returns_the_pinned_owner_projection() {
        let page = load(&stub(false), "acme:co").await.expect("page");
        assert_eq!(
            page,
            FeedbackPage {
                items: vec![FeedbackItem {
                    id: "fb-acme:co".to_owned(),
                    body: "hello".to_owned(),
                    created_at: 1_700_000_000_000,
                }],
            }
        );
    }

    #[tokio::test]
    async fn load_fails_closed_on_invalid_company() {
        assert_eq!(
            load(&stub(false), "bad/id").await.unwrap_err(),
            invalid_request()
        );
        assert_eq!(load(&stub(false), "").await.unwrap_err(), invalid_request());
    }

    #[tokio::test]
    async fn load_fails_closed_when_owner_read_fails() {
        assert_eq!(
            load(&stub(true), "acme:co").await.unwrap_err(),
            unavailable()
        );
    }
}
