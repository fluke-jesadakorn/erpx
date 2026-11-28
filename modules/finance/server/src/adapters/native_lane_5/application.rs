//! Query port and handler for the Finance customer master read family.
//! No SQL is written here: the owner Postgres adapter owns every statement,
//! projection and page cursor.

use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;
use std::{future::Future, pin::Pin};

use super::domain::{self, CustomerReadRequest, RecordFactsRequest};

pub type CustomerReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the owner reader behind this port. The router transports
/// only bounded Holding/company identities and the registered read arguments.
pub trait CustomerRead: Send + Sync {
    fn read<'a>(
        &'a self,
        company: &'a str,
        branch: &'a str,
        request: &'a CustomerReadRequest,
    ) -> CustomerReadFuture<'a, Value>;
}

/// Validate and dispatch one registered read. Company is always bound by the
/// inbound router; branch only participates in the page cursor scope.
pub fn handle<'a, R: CustomerRead + ?Sized>(
    reader: &'a R,
    company: &'a str,
    branch: &'a str,
    request: &'a CustomerReadRequest,
) -> CustomerReadFuture<'a, Value> {
    Box::pin(async move {
        domain::validate_request(request)?;
        if !domain::valid_company(company) || !domain::valid_branch(branch) {
            return Err(domain::invalid());
        }
        reader.read(company, branch, request).await
    })
}

/// Owner reader for the four Finance-owned record supplement facts. SQL,
/// projections and row shapes stay inside the owner Postgres adapter.
pub trait RecordFactsRead: Send + Sync {
    fn read<'a>(
        &'a self,
        company: &'a str,
        request: &'a RecordFactsRequest,
    ) -> CustomerReadFuture<'a, Value>;
}

pub fn handle_record_facts<'a, R: RecordFactsRead + ?Sized>(
    reader: &'a R,
    company: &'a str,
    request: &'a RecordFactsRequest,
) -> CustomerReadFuture<'a, Value> {
    Box::pin(async move {
        domain::validate_record_facts(request)?;
        if !domain::valid_company(company) {
            return Err(domain::invalid());
        }
        reader.read(company, request).await
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CountingReader {
        seen: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl CustomerRead for CountingReader {
        fn read<'a>(
            &'a self,
            company: &'a str,
            _branch: &'a str,
            request: &'a CustomerReadRequest,
        ) -> CustomerReadFuture<'a, Value> {
            self.seen
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let company = company.to_owned();
            Box::pin(async move { Ok(serde_json::json!({"company": company, "kind": request.kind})) })
        }
    }

    fn request(kind: &str) -> CustomerReadRequest {
        CustomerReadRequest {
            kind: kind.to_owned(),
            id: Some("party-1".into()),
            currency: Some("THB".into()),
            search: None,
            status: None,
            cursor: None,
            version: None,
        }
    }

    #[tokio::test]
    async fn native_lane_5_handler_fails_closed_before_reaching_reader() {
        let seen = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let reader = CountingReader { seen: seen.clone() };
        let mut r = request("not_registered");
        assert!(handle(&reader, "company-1", "", &r).await.is_err());
        r = request(domain::KIND_CUSTOMER_PROFILE);
        assert!(handle(&reader, "", "", &r).await.is_err());
        assert!(handle(&reader, "company 1", "", &r).await.is_err());
        assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 0);
        let value = handle(&reader, "company-1", "", &r).await.unwrap();
        assert_eq!(value["kind"], domain::KIND_CUSTOMER_PROFILE);
        assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    struct FactsReader;

    impl RecordFactsRead for FactsReader {
        fn read<'a>(
            &'a self,
            company: &'a str,
            _request: &'a RecordFactsRequest,
        ) -> CustomerReadFuture<'a, Value> {
            let company = company.to_owned();
            Box::pin(async move { Ok(serde_json::json!({"company": company})) })
        }
    }

    fn facts(kind: &'static str) -> RecordFactsRequest {
        RecordFactsRequest {
            kind: kind.to_owned(),
            id: Some("record-1".into()),
            chart_id: None,
            value: Some("0".into()),
            value2: None,
            value3: None,
            value4: None,
            value5: None,
            offset: Some(0),
        }
    }

    #[tokio::test]
    async fn native_lane_5_record_facts_handler_binds_company_before_reading() {
        let mut r = facts("unknown");
        assert!(handle_record_facts(&FactsReader, "company-1", &r).await.is_err());
        r = facts(domain::RF_KIND_PARTY);
        assert!(handle_record_facts(&FactsReader, "", &r).await.is_err());
        let value = handle_record_facts(&FactsReader, "company-1", &r).await.unwrap();
        assert_eq!(value["company"], "company-1");
    }
}
