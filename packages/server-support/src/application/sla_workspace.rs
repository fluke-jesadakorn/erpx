//! Support SLA workspace read query. Ports supply authority rows and the clock;
//! this layer owns validation and the response shape and opens no transaction.
use crate::domain::sla_workspace::{
    project, SlaPolicyRow, SlaTicketRow, SlaWorkspaceScope, SlaWorkspaceView,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type SlaWorkspaceFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner ports. Every method is a read; no receipt, audit or outbox ledger is
/// involved and no caller SQL crosses the boundary.
pub trait SlaWorkspacePorts: Send {
    fn now(&self) -> i64;
    fn policies<'a>(&'a mut self) -> SlaWorkspaceFuture<'a, Vec<SlaPolicyRow>>;
    fn tickets<'a>(&'a mut self) -> SlaWorkspaceFuture<'a, Vec<SlaTicketRow>>;
    fn alerts<'a>(&'a mut self) -> SlaWorkspaceFuture<'a, Vec<Value>>;
}

/// The bounded read: complete SLA workspace in the selected Support scope.
pub async fn read_sla_workspace<P: SlaWorkspacePorts>(
    ports: &mut P,
    scope: &SlaWorkspaceScope,
) -> Result<SlaWorkspaceView, BusinessError> {
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
    let policies = ports.policies().await?;
    let tickets = ports.tickets().await?;
    let alerts = ports.alerts().await?;
    project(scope, policies, tickets, alerts, ports.now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn scope() -> SlaWorkspaceScope {
        SlaWorkspaceScope {
            company_id: "company".into(),
            user_id: "reviewer".into(),
            branch_id: None,
            department_id: None,
            support_manage: true,
            support_read: true,
            new_work: true,
            settle_existing: true,
        }
    }

    #[derive(Default)]
    struct Fake {
        policies: Vec<SlaPolicyRow>,
        tickets: Vec<SlaTicketRow>,
        alerts: Vec<Value>,
        calls: BTreeMap<&'static str, usize>,
    }

    impl Fake {
        fn count(&self, name: &'static str) -> usize {
            *self.calls.get(name).unwrap_or(&0)
        }
    }

    impl SlaWorkspacePorts for Fake {
        fn now(&self) -> i64 {
            1_793_757_600_000
        }
        fn policies<'a>(&'a mut self) -> SlaWorkspaceFuture<'a, Vec<SlaPolicyRow>> {
            *self.calls.entry("policies").or_insert(0) += 1;
            let rows = self.policies.clone();
            Box::pin(async move { Ok(rows) })
        }
        fn tickets<'a>(&'a mut self) -> SlaWorkspaceFuture<'a, Vec<SlaTicketRow>> {
            *self.calls.entry("tickets").or_insert(0) += 1;
            let rows = self.tickets.clone();
            Box::pin(async move { Ok(rows) })
        }
        fn alerts<'a>(&'a mut self) -> SlaWorkspaceFuture<'a, Vec<Value>> {
            *self.calls.entry("alerts").or_insert(0) += 1;
            let rows = self.alerts.clone();
            Box::pin(async move { Ok(rows) })
        }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = Box::pin(future);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => panic!("test future unexpectedly pending"),
        }
    }

    #[test]
    fn reads_every_owner_source_once() {
        let mut ports = Fake {
            alerts: vec![Value::Null],
            ..Fake::default()
        };
        let view = block_on(read_sla_workspace(&mut ports, &scope())).unwrap();
        assert_eq!(ports.count("policies"), 1);
        assert_eq!(ports.count("tickets"), 1);
        assert_eq!(ports.count("alerts"), 1);
        assert_eq!(view.alerts, vec![Value::Null]);
        assert_eq!(view.now, 1_793_757_600_000);
    }

    #[test]
    fn rejects_a_scope_without_support_read() {
        let mut ports = Fake::default();
        let mut denied = scope();
        denied.support_read = false;
        assert_eq!(
            block_on(read_sla_workspace(&mut ports, &denied))
                .unwrap_err()
                .code,
            "SUPPORT_READ_REQUIRED"
        );
        assert_eq!(ports.count("policies"), 0);
    }
}
