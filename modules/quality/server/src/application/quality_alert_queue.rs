//! Quality escalation queue query, admission policy and owner port.
//!
//! Transport-independent mirror of the retained `qualityAlerts` application
//! operation: read permission and Company/Branch scope are checked before any
//! SQL, and the ordered row set is bounded here, not in the HTTP adapter.
use crate::domain::quality_alert_queue::{assert_bounded, QualityAlertRow};
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, RecordId, UserId};
use std::{future::Future, pin::Pin};

pub const QUALITY_ALERT_QUEUE_READ: &str = "quality.alert.queue.read";

pub type QualityAlertQueueFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityAlertQueueQuery {
    pub company: CompanyId,
    pub branch: Option<RecordId>,
    pub actor: UserId,
    /// The caller's current assignment must not be a department scope.
    pub department_scope: bool,
    /// `quality.read` on the caller's current assignment.
    pub read_allowed: bool,
}

impl QualityAlertQueueQuery {
    pub fn operation(&self) -> &'static str {
        QUALITY_ALERT_QUEUE_READ
    }
}

/// The owner read port. Composition binds this to the sqlx adapter inside the
/// caller's already-open Holding unit; the port itself never sees SQL.
pub trait QualityAlertQueuePort: Send + Sync {
    fn alerts<'a>(
        &'a mut self,
        query: &'a QualityAlertQueueQuery,
    ) -> QualityAlertQueueFuture<'a, Vec<QualityAlertRow>>;
}

fn denied() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "QUALITY_SCOPE_REQUIRED",
        "Use a current Company or Branch quality assignment.",
    )
}

/// Admit and return the complete escalation queue. Authorization facts are
/// supplied by the caller's current Account/IAM unit; no permission is inferred
/// from the request body or headers inside this operation.
pub fn quality_alert_queue<'a, R: QualityAlertQueuePort>(
    query: &'a QualityAlertQueueQuery,
    repository: &'a mut R,
) -> QualityAlertQueueFuture<'a, Vec<QualityAlertRow>> {
    Box::pin(async move {
        if query.department_scope || !query.read_allowed {
            return Err(denied());
        }
        let rows = repository.alerts(query).await?;
        assert_bounded(&rows)?;
        Ok(rows)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Port {
        calls: usize,
    }

    impl QualityAlertQueuePort for Port {
        fn alerts<'a>(
            &'a mut self,
            _query: &'a QualityAlertQueueQuery,
        ) -> QualityAlertQueueFuture<'a, Vec<QualityAlertRow>> {
            Box::pin(async move {
                self.calls += 1;
                Ok(Vec::new())
            })
        }
    }

    fn query() -> QualityAlertQueueQuery {
        QualityAlertQueueQuery {
            company: CompanyId::new("company").unwrap(),
            branch: Some(RecordId::new("branch").unwrap()),
            actor: UserId::new("user").unwrap(),
            department_scope: false,
            read_allowed: true,
        }
    }

    #[tokio::test]
    async fn admits_current_company_read() {
        let mut port = Port::default();
        let rows = quality_alert_queue(&query(), &mut port).await.unwrap();
        assert!(rows.is_empty());
        assert_eq!(port.calls, 1);
    }

    #[tokio::test]
    async fn refuses_department_scope_before_sql() {
        let mut port = Port::default();
        let mut query = query();
        query.department_scope = true;
        assert_eq!(
            quality_alert_queue(&query, &mut port).await.unwrap_err().code,
            "QUALITY_SCOPE_REQUIRED"
        );
        assert_eq!(port.calls, 0);
    }

    #[tokio::test]
    async fn refuses_missing_read_permission_before_sql() {
        let mut port = Port::default();
        let mut query = query();
        query.read_allowed = false;
        assert_eq!(
            quality_alert_queue(&query, &mut port).await.unwrap_err().code,
            "QUALITY_SCOPE_REQUIRED"
        );
        assert_eq!(port.calls, 0);
    }
}
