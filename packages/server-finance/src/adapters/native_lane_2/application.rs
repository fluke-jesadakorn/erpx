//! Application ports and handlers for the native Finance reads.
//!
//! No SQL lives here: composition supplies the owner readers, and these
//! handlers only enforce the Company boundary and validate inputs/results.

use crate::adapters::native_lane_2::domain::{
    AgingRequest, CompanyTool, InvoicePageRequest, InvoicePageWire, OverviewCounts, invalid_request,
    valid_company_id, validate_aging_request, validate_page_request,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
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

pub type CompanyToolsFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<CompanyTool>, BusinessError>> + Send + 'a>>;

/// Owner port for the standalone Company tool-list read. The `enabled` check
/// and the commit version guard stay in the TypeScript write transaction.
pub trait CompanyToolsQuery: Send + Sync {
    fn company_tools<'a>(&'a self, company_id: &'a str) -> CompanyToolsFuture<'a>;
}

/// Validate the Company scope and return the owner-projected tool list.
pub async fn load_tools(
    query: &dyn CompanyToolsQuery,
    company_id: &str,
) -> Result<Vec<CompanyTool>, BusinessError> {
    if !valid_company_id(company_id) {
        return Err(invalid_request());
    }
    query.company_tools(company_id).await
}

pub type InvoicePageFuture<'a> =
    Pin<Box<dyn Future<Output = Result<InvoicePageWire, BusinessError>> + Send + 'a>>;
pub type AgingFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<Value>, BusinessError>> + Send + 'a>>;

/// Owner invoice read port. The SQL and serializable unit stay in the adapter.
pub trait InvoiceReadQuery: Send + Sync {
    fn page<'a>(
        &'a self,
        company_id: &'a str,
        request: &'a InvoicePageRequest,
    ) -> InvoicePageFuture<'a>;
    fn aging<'a>(
        &'a self,
        company_id: &'a str,
        request: &'a AgingRequest,
    ) -> AgingFuture<'a>;
}

/// Validate before the owner executes the page query.
pub async fn load_page(
    query: &dyn InvoiceReadQuery,
    company_id: &str,
    request: &InvoicePageRequest,
) -> Result<InvoicePageWire, BusinessError> {
    if !valid_company_id(company_id) {
        return Err(invalid_request());
    }
    validate_page_request(request)?;
    query.page(company_id, request).await
}

/// Validate before the owner executes the historical aging query.
pub async fn load_aging(
    query: &dyn InvoiceReadQuery,
    company_id: &str,
    request: &AgingRequest,
) -> Result<Vec<Value>, BusinessError> {
    if !valid_company_id(company_id) {
        return Err(invalid_request());
    }
    validate_aging_request(request)?;
    query.aging(company_id, request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_2::domain::unavailable;

    struct InvoiceStub(bool);
    impl InvoiceReadQuery for InvoiceStub {
        fn page<'a>(
            &'a self,
            company_id: &'a str,
            _request: &'a InvoicePageRequest,
        ) -> InvoicePageFuture<'a> {
            let company_id = company_id.to_owned();
            let fail = self.0;
            Box::pin(async move {
                if fail || company_id != "acme:co" {
                    return Err(unavailable());
                }
                Ok(InvoicePageWire {
                    items: vec![serde_json::json!({"id": "i1", "page_key": "2026-01-01:INV-1"})],
                    next_cursor: None,
                    previous_cursor: None,
                    has_more: false,
                })
            })
        }
        fn aging<'a>(
            &'a self,
            company_id: &'a str,
            _request: &'a AgingRequest,
        ) -> AgingFuture<'a> {
            let company_id = company_id.to_owned();
            let fail = self.0;
            Box::pin(async move {
                if fail || company_id != "acme:co" {
                    return Err(unavailable());
                }
                Ok(vec![serde_json::json!({"id": "i1", "due_date": "2026-01-01"})])
            })
        }
    }

    #[tokio::test]
    async fn invoice_reads_validate_and_project() {
        let stub = InvoiceStub(false);
        let page = load_page(&stub, "acme:co", &InvoicePageRequest::default())
            .await
            .expect("page");
        assert_eq!(page.items.len(), 1);
        assert!(!page.has_more);
        let aging = load_aging(
            &stub,
            "acme:co",
            &AgingRequest { kind: "receivable".into(), as_of: "2026-02-01".into(), chart_id: None },
        )
        .await
        .expect("aging");
        assert_eq!(aging, vec![serde_json::json!({"id": "i1", "due_date": "2026-01-01"})]);
    }

    #[tokio::test]
    async fn invoice_reads_fail_closed_on_bad_input_or_owner_failure() {
        let stub = InvoiceStub(false);
        assert_eq!(
            load_page(&stub, "bad/id", &InvoicePageRequest::default())
                .await
                .unwrap_err(),
            invalid_request()
        );
        let bad = InvoicePageRequest { kind: Some("all".into()), ..Default::default() };
        assert_ne!(
            load_page(&stub, "acme:co", &bad).await.unwrap_err(),
            invalid_request()
        );
        assert_eq!(
            load_page(&InvoiceStub(true), "acme:co", &InvoicePageRequest::default())
                .await
                .unwrap_err(),
            unavailable()
        );
        let bad_age = AgingRequest { kind: "all".into(), as_of: "2026-02-01".into(), chart_id: None };
        assert_ne!(
            load_aging(&stub, "acme:co", &bad_age).await.unwrap_err(),
            invalid_request()
        );
    }

    struct ToolsStub(bool);
    impl CompanyToolsQuery for ToolsStub {
        fn company_tools<'a>(&'a self, company_id: &'a str) -> CompanyToolsFuture<'a> {
            let company_id = company_id.to_owned();
            let fail = self.0;
            Box::pin(async move {
                if fail || company_id != "acme:co" {
                    return Err(unavailable());
                }
                Ok(vec![CompanyTool { tool: "planning".into(), enabled: 1, version: 2 }])
            })
        }
    }

    #[tokio::test]
    async fn company_tools_validate_and_project() {
        let tools = load_tools(&ToolsStub(false), "acme:co").await.expect("tools");
        assert_eq!(
            tools,
            vec![CompanyTool { tool: "planning".into(), enabled: 1, version: 2 }]
        );
        assert_eq!(load_tools(&ToolsStub(false), "bad/id").await.unwrap_err(), invalid_request());
        assert_eq!(load_tools(&ToolsStub(true), "acme:co").await.unwrap_err(), unavailable());
    }

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
