//! Application port and handler for the native beta-facts read family.
//!
//! No SQL lives here. Composition supplies the owner query adapter; this
//! handler validates the request, enforces the Company boundary on the header
//! and pins the four tagged response shapes.

use crate::adapters::native_lane_15::domain::{
    capped, invalid_request, valid_id, validate, BetaFactKind, BetaFactRequest, BetaFacts,
    PostDraft, PostReport, TaskOrigin,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type BetaFactsFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner read port implemented by the sqlx adapter. The HTTP router only
/// transports the validated result.
pub trait BetaFactsQuery: Send + Sync {
    fn post_drafts<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
    ) -> BetaFactsFuture<'a, Vec<PostDraft>>;
    fn reports<'a>(&'a self, company: &'a str) -> BetaFactsFuture<'a, Vec<PostReport>>;
    /// The one byte-identical statement behind both task read kinds.
    fn task_origin<'a>(&'a self, post: &'a str) -> BetaFactsFuture<'a, Option<TaskOrigin>>;
}

/// Validate the request, then dispatch to exactly one owner read. `context`
/// is the value of the `x-erpx-context-company` header; on Company-scoped
/// kinds it must match the body's Company and be a valid bounded id.
pub async fn load(
    query: &dyn BetaFactsQuery,
    request: &BetaFactRequest,
    context: Option<&str>,
) -> Result<BetaFacts, BusinessError> {
    validate(request)?;
    if request.kind.company_scoped() {
        let company = request.company_id.as_deref().ok_or_else(invalid_request)?;
        if context != Some(company) || !valid_id(context.unwrap_or_default()) {
            return Err(invalid_request());
        }
    }
    let facts = match request.kind {
        BetaFactKind::PostDrafts => {
            let company = request.company_id.as_deref().ok_or_else(invalid_request)?;
            let user = request.user_id.as_deref().ok_or_else(invalid_request)?;
            BetaFacts::PostDrafts {
                drafts: query.post_drafts(company, user).await?,
            }
        }
        BetaFactKind::Reports => {
            let company = request.company_id.as_deref().ok_or_else(invalid_request)?;
            BetaFacts::Reports {
                reports: query.reports(company).await?,
            }
        }
        BetaFactKind::PostTask => {
            let post = request.post_id.as_deref().ok_or_else(invalid_request)?;
            BetaFacts::PostTask {
                task: query.task_origin(post).await?,
            }
        }
        BetaFactKind::RecoverPostTask => {
            let post = request.post_id.as_deref().ok_or_else(invalid_request)?;
            BetaFacts::RecoverPostTask {
                task: query.task_origin(post).await?,
            }
        }
    };
    capped(facts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_15::domain::unavailable;

    struct Stub {
        fail: bool,
    }

    impl BetaFactsQuery for Stub {
        fn post_drafts<'a>(
            &'a self,
            company: &'a str,
            user: &'a str,
        ) -> BetaFactsFuture<'a, Vec<PostDraft>> {
            let ok = !self.fail && company == "c1" && user == "u1";
            Box::pin(async move {
                if !ok {
                    return Err(unavailable());
                }
                Ok(vec![])
            })
        }

        fn reports<'a>(&'a self, company: &'a str) -> BetaFactsFuture<'a, Vec<PostReport>> {
            let ok = !self.fail && company == "c1";
            Box::pin(async move {
                if !ok {
                    return Err(unavailable());
                }
                Ok(vec![])
            })
        }

        fn task_origin<'a>(&'a self, post: &'a str) -> BetaFactsFuture<'a, Option<TaskOrigin>> {
            let ok = !self.fail && post == "p1";
            Box::pin(async move {
                if !ok {
                    return Err(unavailable());
                }
                Ok(Some(TaskOrigin {
                    task_id: "t1".into(),
                }))
            })
        }
    }

    #[tokio::test]
    async fn dispatches_each_kind_to_the_owner_port() {
        let query = Stub { fail: false };
        let drafts = load(
            &query,
            &BetaFactRequest {
                kind: BetaFactKind::PostDrafts,
                company_id: Some("c1".into()),
                user_id: Some("u1".into()),
                ..BetaFactRequest::default()
            },
            Some("c1"),
        )
        .await
        .expect("drafts");
        assert_eq!(drafts, BetaFacts::PostDrafts { drafts: vec![] });

        let task = load(
            &query,
            &BetaFactRequest {
                kind: BetaFactKind::PostTask,
                post_id: Some("p1".into()),
                ..BetaFactRequest::default()
            },
            None,
        )
        .await
        .expect("task");
        assert_eq!(
            task,
            BetaFacts::PostTask {
                task: Some(TaskOrigin {
                    task_id: "t1".into()
                })
            }
        );
    }

    #[tokio::test]
    async fn fails_closed_when_company_header_disagrees() {
        let query = Stub { fail: false };
        let error = load(
            &query,
            &BetaFactRequest {
                kind: BetaFactKind::Reports,
                company_id: Some("c1".into()),
                ..BetaFactRequest::default()
            },
            Some("c2"),
        )
        .await
        .unwrap_err();
        assert_eq!(error, invalid_request());
    }

    #[tokio::test]
    async fn fails_closed_when_owner_read_fails() {
        let query = Stub { fail: true };
        let error = load(
            &query,
            &BetaFactRequest {
                kind: BetaFactKind::Reports,
                company_id: Some("c1".into()),
                ..BetaFactRequest::default()
            },
            Some("c1"),
        )
        .await
        .unwrap_err();
        assert_eq!(error, unavailable());
    }
}
