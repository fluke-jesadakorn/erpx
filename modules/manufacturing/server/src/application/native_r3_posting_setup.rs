//! Native R3 Manufacturing posting-setup owner query. Read-only: one owner
//! query owns no transaction boundary and no idempotency key. Composition
//! supplies the actor, Finance authority, purchased-package admission and the
//! caller-owned Holding transaction; the adapter owns SQL.
use crate::domain::native_r3_posting_setup::{validate_rows, PostingSetupActor};
use erpx_platform_shared_server::public::{BusinessError, CompanyId, TimestampMillis};
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type PostingSetupFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait PostingSetupRepository: Send {
    type Error: From<BusinessError> + Send;

    fn rows<'a>(&'a mut self, company: &'a CompanyId) -> PostingSetupFuture<'a, Value, Self::Error>;
}

/// Project the owner-owned posting configuration and its active THB references.
pub async fn manufacturing_posting_setup<R: PostingSetupRepository>(
    actor: &PostingSetupActor,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    actor.require_read(now)?;
    let rows = repository.rows(&actor.company_id).await?;
    validate_rows(&rows)?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_shared_server::public::FailureKind;
    use serde_json::json;

    const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

    fn actor(permissions: &[&str]) -> PostingSetupActor {
        PostingSetupActor {
            user_id: "owner".into(),
            company_id: CompanyId::new("company").unwrap(),
            permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
            expires_at: NOW.get() + 60_000,
        }
    }

    struct Repo {
        value: Value,
        calls: usize,
    }

    impl PostingSetupRepository for Repo {
        type Error = BusinessError;

        fn rows<'a>(&'a mut self, _: &'a CompanyId) -> PostingSetupFuture<'a, Value, Self::Error> {
            self.calls += 1;
            let value = self.value.clone();
            Box::pin(async move { Ok(value) })
        }
    }

    #[tokio::test]
    async fn projects_rows_after_the_read_gate() {
        let value = json!({"configuration": null, "charts": [], "accounts": []});
        let mut repo = Repo { value: value.clone(), calls: 0 };
        let rows = manufacturing_posting_setup(&actor(&["finance.read", "finance.write"]), &mut repo, NOW)
            .await
            .unwrap();
        assert_eq!(rows, value);
        assert_eq!(repo.calls, 1);
    }

    #[tokio::test]
    async fn forbidden_actor_never_runs_sql() {
        let mut repo = Repo { value: json!({"configuration": null, "charts": [], "accounts": []}), calls: 0 };
        let error = manufacturing_posting_setup(&actor(&[]), &mut repo, NOW).await.unwrap_err();
        assert_eq!(error.kind, FailureKind::Forbidden);
        assert_eq!(repo.calls, 0);
    }

    #[tokio::test]
    async fn malformed_projection_is_rejected() {
        let mut repo = Repo { value: json!({"charts": []}), calls: 0 };
        let error = manufacturing_posting_setup(&actor(&["finance.read", "finance.write"]), &mut repo, NOW)
            .await
            .unwrap_err();
        assert_eq!(error.kind, FailureKind::Unavailable);
    }
}
