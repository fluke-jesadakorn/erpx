//! Native R3 RFQ owner query. Read-only, no transaction boundary and no
//! idempotency key; composition supplies the actor, the purchased-package
//! admission and the caller-owned Holding connection. The adapter owns SQL.
use crate::domain::native_r3_rfq::{validate_detail, validate_workspace, RfqActor};
use erpx_platform_shared_server::public::{BusinessError, CompanyId, TimestampMillis};
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type RfqFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// The single owner read port. Each method returns the exact owner-owned JSON
/// facts; the command writes stay a composition/workflow responsibility.
pub trait RfqReadRepository: Send {
    type Error: From<BusinessError> + Send;

    fn workspace<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch: Option<&'a str>,
        department: Option<&'a str>,
    ) -> RfqFuture<'a, Value, Self::Error>;

    fn detail<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch: Option<&'a str>,
        department: Option<&'a str>,
        id: &'a str,
    ) -> RfqFuture<'a, Value, Self::Error>;
}

/// Project the bounded RFQ workspace for the selected work context.
pub async fn rfq_workspace<R: RfqReadRepository>(
    actor: &RfqActor,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    actor.require_read(now)?;
    let data = repository
        .workspace(&actor.company_id, actor.branch_id.as_deref(), actor.department_id.as_deref())
        .await?;
    validate_workspace(&data)?;
    Ok(data)
}

/// Project the bounded RFQ detail for one id in the selected work context.
pub async fn rfq_detail<R: RfqReadRepository>(
    actor: &RfqActor,
    repository: &mut R,
    now: TimestampMillis,
    id: &str,
) -> Result<Value, R::Error> {
    actor.require_read(now)?;
    let data = repository
        .detail(&actor.company_id, actor.branch_id.as_deref(), actor.department_id.as_deref(), id)
        .await?;
    validate_detail(&data)?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::native_r3_rfq::RfqActor;
    use erpx_platform_shared_server::public::FailureKind;
    use serde_json::json;

    const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

    fn actor(permissions: &[&str]) -> RfqActor {
        RfqActor {
            user_id: "owner".into(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: Some("branch".into()),
            department_id: None,
            permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
            expires_at: NOW.get() + 60_000,
        }
    }

    struct Repo {
        value: Value,
        calls: usize,
    }

    impl RfqReadRepository for Repo {
        type Error = BusinessError;

        fn workspace<'a>(
            &'a mut self,
            _: &'a CompanyId,
            _: Option<&'a str>,
            _: Option<&'a str>,
        ) -> RfqFuture<'a, Value, Self::Error> {
            self.calls += 1;
            let value = self.value.clone();
            Box::pin(async move { Ok(value) })
        }

        fn detail<'a>(
            &'a mut self,
            _: &'a CompanyId,
            _: Option<&'a str>,
            _: Option<&'a str>,
            _: &'a str,
        ) -> RfqFuture<'a, Value, Self::Error> {
            self.calls += 1;
            let value = self.value.clone();
            Box::pin(async move { Ok(value) })
        }
    }

    fn workspace_value() -> Value {
        json!({"rfqs": [], "products": [], "warehouses": [], "suppliers": [], "currency": "THB"})
    }

    #[tokio::test]
    async fn workspace_runs_after_the_read_gate() {
        let mut repo = Repo { value: workspace_value(), calls: 0 };
        let data = rfq_workspace(&actor(&["commerce.read"]), &mut repo, NOW).await.unwrap();
        assert_eq!(data, workspace_value());
        assert_eq!(repo.calls, 1);
    }

    #[tokio::test]
    async fn forbidden_actor_never_runs_sql() {
        let mut repo = Repo { value: workspace_value(), calls: 0 };
        let error = rfq_workspace(&actor(&[]), &mut repo, NOW).await.unwrap_err();
        assert_eq!(error.kind, FailureKind::Forbidden);
        assert_eq!(repo.calls, 0);
    }

    #[tokio::test]
    async fn malformed_projection_is_rejected() {
        let mut repo = Repo { value: json!({"currency": "THB"}), calls: 0 };
        let error = rfq_workspace(&actor(&["commerce.read"]), &mut repo, NOW).await.unwrap_err();
        assert_eq!(error.kind, FailureKind::Unavailable);
    }

    #[tokio::test]
    async fn detail_runs_after_the_read_gate() {
        let detail = json!({"rfq": {"id": "rfq-1"}, "lines": [], "offers": [], "offerLines": [], "events": [], "current": [], "sourceBlocked": false});
        let mut repo = Repo { value: detail.clone(), calls: 0 };
        let data = rfq_detail(&actor(&["commerce.read"]), &mut repo, NOW, "rfq-1").await.unwrap();
        assert_eq!(data, detail);
    }
}
