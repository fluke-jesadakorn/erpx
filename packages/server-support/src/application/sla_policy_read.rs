//! Support SLA policy read queries. Ports supply owner rows; this layer owns
//! validation and the response shape and opens no transaction or HTTP request.
use crate::domain::sla_policy_read::{
    point, work_items, SlaPolicyPointView, SlaPolicyReadScope, SlaPolicyRow, SlaPolicyWorkItem,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::future::Future;
use std::pin::Pin;

pub type SlaPolicyReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Every port is a read; no receipt, audit or outbox ledger is involved and no
/// caller SQL crosses the boundary.
pub trait SlaPolicyReadPorts: Send {
    fn policy<'a>(&'a mut self, id: &'a str) -> SlaPolicyReadFuture<'a, Option<SlaPolicyRow>>;
    fn work_policies<'a>(&'a mut self) -> SlaPolicyReadFuture<'a, Vec<SlaPolicyRow>>;
}

fn authorize(scope: &SlaPolicyReadScope) -> Result<(), BusinessError> {
    if !scope.support_read {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "SUPPORT_READ_REQUIRED",
            "A current Support read assignment is required.",
        ));
    }
    if scope.company_id.is_empty() || scope.user_id.is_empty() {
        return Err(BusinessError::invalid(
            "SUPPORT_SLA_SCOPE_INVALID",
            "Provide a selected Company and actor.",
        ));
    }
    Ok(())
}

/// One bounded point read: the complete retained policy row or 404.
pub async fn read_policy_point<P: SlaPolicyReadPorts>(
    ports: &mut P,
    scope: &SlaPolicyReadScope,
    id: &str,
) -> Result<SlaPolicyPointView, BusinessError> {
    authorize(scope)?;
    if id.is_empty() || id.len() > 200 || id.chars().any(char::is_control) {
        return Err(BusinessError::invalid(
            "SUPPORT_POLICY_ID_INVALID",
            "Use the current Support policy selector.",
        ));
    }
    let Some(row) = ports.policy(id).await? else {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SUPPORT_POLICY_NOT_FOUND",
            "Policy not found in this Company.",
        ));
    };
    point(&row)
}

/// Bounded work read: independent-reviewer policy work items in this scope.
pub async fn read_policy_work<P: SlaPolicyReadPorts>(
    ports: &mut P,
    scope: &SlaPolicyReadScope,
) -> Result<Vec<SlaPolicyWorkItem>, BusinessError> {
    if !scope.support_read {
        return Ok(Vec::new());
    }
    authorize(scope)?;
    let rows = ports.work_policies().await?;
    work_items(scope, rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn scope() -> SlaPolicyReadScope {
        SlaPolicyReadScope {
            company_id: "company".into(),
            user_id: "reviewer".into(),
            support_manage: true,
            support_read: true,
            new_work: true,
        }
    }

    fn row() -> SlaPolicyRow {
        SlaPolicyRow {
            id: "support-policy-1".into(),
            company_id: "company".into(),
            name: "Standard support".into(),
            policy_json: json!({"targetMinutes": 60}),
            status: "draft".into(),
            version: 1,
            created_by: "creator".into(),
            approved_by: None,
            approved_at: None,
            created_at: 1,
            updated_at: 2,
            actor_eligible: 1,
            reviewers: 1,
        }
    }

    #[derive(Default)]
    struct Fake {
        policy: Option<SlaPolicyRow>,
        work: Vec<SlaPolicyRow>,
        calls: BTreeMap<&'static str, usize>,
    }

    impl SlaPolicyReadPorts for Fake {
        fn policy<'a>(&'a mut self, _id: &'a str) -> SlaPolicyReadFuture<'a, Option<SlaPolicyRow>> {
            *self.calls.entry("point").or_insert(0) += 1;
            let row = self.policy.clone();
            Box::pin(async move { Ok(row) })
        }
        fn work_policies<'a>(&'a mut self) -> SlaPolicyReadFuture<'a, Vec<SlaPolicyRow>> {
            *self.calls.entry("work").or_insert(0) += 1;
            let rows = self.work.clone();
            Box::pin(async move { Ok(rows) })
        }
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        let mut future = Box::pin(future);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => panic!("test future unexpectedly pending"),
        }
    }

    #[test]
    fn point_reads_once_and_maps_missing_rows_to_not_found() {
        let mut ports = Fake {
            policy: Some(row()),
            ..Default::default()
        };
        let view = block_on(read_policy_point(&mut ports, &scope(), "support-policy-1")).unwrap();
        assert_eq!(view.id, "support-policy-1");
        assert_eq!(ports.calls["point"], 1);
        let mut missing = Fake::default();
        assert_eq!(
            block_on(read_policy_point(&mut missing, &scope(), "x"))
                .unwrap_err()
                .code,
            "SUPPORT_POLICY_NOT_FOUND"
        );
    }

    #[test]
    fn work_read_skips_sql_without_support_read() {
        let mut ports = Fake::default();
        let mut denied = scope();
        denied.support_read = false;
        assert!(block_on(read_policy_work(&mut ports, &denied))
            .unwrap()
            .is_empty());
        assert_eq!(ports.calls.get("work"), None);
    }

    #[test]
    fn authority_and_selector_fail_closed() {
        let mut ports = Fake::default();
        let mut denied = scope();
        denied.support_read = false;
        assert_eq!(
            block_on(read_policy_point(&mut ports, &denied, "x"))
                .unwrap_err()
                .code,
            "SUPPORT_READ_REQUIRED"
        );
        assert_eq!(
            block_on(read_policy_point(&mut ports, &scope(), ""))
                .unwrap_err()
                .code,
            "SUPPORT_POLICY_ID_INVALID"
        );
    }
}
