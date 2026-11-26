//! Owner query port and use case for the native Platform usage read. No SQL
//! and no transport here. The composition backend resolves the Account session,
//! Platform role and `billing.usage.read` permission before the query runs.

use crate::adapters::native_platform_usage::domain::{
    ExecutiveUsage, request_invalid, valid_company, valid_month,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{future::Future, pin::Pin, sync::Arc};

pub type UsageFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The owner executes the single `executive_generation_jobs` count on its own
/// pool. The router transports only a bounded Company and UTC month.
pub trait ExecutiveUsageRead: Send + Sync {
    fn generation_jobs<'a>(
        &'a self,
        company: &'a str,
        month: &'a str,
    ) -> UsageFuture<'a, i64>;
}

pub type SharedExecutiveUsageRead = Arc<dyn ExecutiveUsageRead>;

pub fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PLATFORM_USAGE_FORBIDDEN",
        "Platform billing permission required.",
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutiveUsageQuery<'a> {
    pub company: &'a str,
    pub month: &'a str,
    pub role: &'a str,
    pub permissions: &'a [String],
}

impl<'a> ExecutiveUsageQuery<'a> {
    pub fn parse(
        company: &'a str,
        month: &'a str,
        role: &'a str,
        permissions: &'a [String],
    ) -> Result<Self, BusinessError> {
        if !valid_company(company) || !valid_month(month) {
            return Err(request_invalid());
        }
        Ok(Self {
            company,
            month,
            role,
            permissions,
        })
    }

    fn authorized(&self) -> bool {
        matches!(self.role, "platform_admin" | "platform_auditor")
            && self
                .permissions
                .iter()
                .any(|value| value == "billing.usage.read")
    }
}

/// Read-only use case: validate the request, check Platform authority and
/// return the complete payload with no TypeScript projection.
pub async fn executive_usage<S: ExecutiveUsageRead + ?Sized>(
    query: &ExecutiveUsageQuery<'_>,
    store: &S,
) -> Result<ExecutiveUsage, BusinessError> {
    if !query.authorized() {
        return Err(forbidden());
    }
    let count = store.generation_jobs(query.company, query.month).await?;
    ExecutiveUsage::new(query.company.to_owned(), query.month.to_owned(), count)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counter(i64);

    impl ExecutiveUsageRead for Counter {
        fn generation_jobs<'a>(
            &'a self,
            _company: &'a str,
            _month: &'a str,
        ) -> UsageFuture<'a, i64> {
            Box::pin(async move { Ok(self.0) })
        }
    }

    #[tokio::test]
    async fn query_returns_complete_payload_only_for_the_platform_billing_role() {
        let store = Counter(7);
        let permissions = vec!["billing.usage.read".to_owned()];
        let query =
            ExecutiveUsageQuery::parse("company-001", "2026-06", "platform_auditor", &permissions)
                .unwrap();
        let usage = executive_usage(&query, &store).await.unwrap();
        assert_eq!(usage.company_id, "company-001");
        assert_eq!(usage.month, "2026-06");
        assert_eq!(usage.generation_jobs, 7);
        let auditor =
            ExecutiveUsageQuery::parse("company-001", "2026-06", "platform_owner", &permissions)
                .unwrap();
        assert_eq!(executive_usage(&auditor, &store).await.unwrap_err().code, "PLATFORM_USAGE_FORBIDDEN");
        let no_permission = ExecutiveUsageQuery::parse("company-001", "2026-06", "platform_admin", &[]).unwrap();
        assert_eq!(executive_usage(&no_permission, &store).await.unwrap_err().kind, FailureKind::Forbidden);
    }

    #[test]
    fn query_validation_is_fail_closed() {
        let permissions = vec!["billing.usage.read".to_owned()];
        assert!(ExecutiveUsageQuery::parse("bad'. DROP", "2026-06", "platform_admin", &permissions).is_err());
        assert!(ExecutiveUsageQuery::parse("company-001", "2026-13", "platform_admin", &permissions).is_err());
    }
}
