//! Native R3 Manufacturing posting-setup prerequisites query.
//!
//! Reads only the owner-owned period and FX-rate references after the current
//! Finance read gate; the configuration/chart/account rows come from the
//! existing native posting-setup read. Company authority, purchased-package
//! admission, locking, `contextKey` and the `available`/`can*` decisions stay
//! at the composition boundary.
use crate::domain::native_r3_posting_prereq::project_prerequisites;
use crate::domain::native_r3_posting_setup::PostingSetupActor;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, TimestampMillis};
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type PrereqFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait PrereqRepository: Send {
    type Error: From<BusinessError> + Send;

    fn periods<'a>(&'a mut self, company: &'a CompanyId) -> PrereqFuture<'a, Value, Self::Error>;
    fn rates<'a>(&'a mut self, company: &'a CompanyId) -> PrereqFuture<'a, Value, Self::Error>;
}

/// Project the base posting rows plus periods and rates into the retained
/// reference snapshot. The gate runs before either repository method.
pub async fn posting_prerequisites<R: PrereqRepository>(
    actor: &PostingSetupActor,
    base: Value,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    actor.require_read(now)?;
    let periods = repository.periods(&actor.company_id).await?;
    let rates = repository.rates(&actor.company_id).await?;
    Ok(project_prerequisites(&base, &periods, &rates)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_shared_server::public::{CompanyId, FailureKind};
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
        calls: usize,
    }

    impl PrereqRepository for Repo {
        type Error = BusinessError;

        fn periods<'a>(&'a mut self, _: &'a CompanyId) -> PrereqFuture<'a, Value, Self::Error> {
            self.calls += 1;
            Box::pin(async { Ok(json!({"periods":[]})) })
        }

        fn rates<'a>(&'a mut self, _: &'a CompanyId) -> PrereqFuture<'a, Value, Self::Error> {
            self.calls += 1;
            Box::pin(async { Ok(json!({"rates":[]})) })
        }
    }

    #[tokio::test]
    async fn forbidden_actor_never_reads_references() {
        let mut repo = Repo { calls: 0 };
        let error = posting_prerequisites(
            &actor(&[]),
            json!({"configuration":null,"charts":[],"accounts":[]}),
            &mut repo,
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind, FailureKind::Forbidden);
        assert_eq!(repo.calls, 0);
    }

    #[tokio::test]
    async fn projects_exact_reference_keys() {
        let mut repo = Repo { calls: 0 };
        let rows = posting_prerequisites(
            &actor(&["finance.read", "finance.write"]),
            json!({"configuration":null,"charts":[{"id":"c","name":"THB","baseCurrency":"THB"}],"accounts":[]}),
            &mut repo,
            NOW,
        )
        .await
        .unwrap();
        assert_eq!(repo.calls, 2);
        assert_eq!(rows, json!({"charts":[{"id":"c","name":"THB","baseCurrency":"THB"}],"accounts":[],"periods":[],"rates":[]}));
    }
}
