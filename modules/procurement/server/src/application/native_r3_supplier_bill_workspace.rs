//! Native R3 supplier-bill workspace query. Read-only: one owner query owns no
//! transaction boundary and no idempotency key. Composition supplies the actor
//! (including the current Company/branch/department scope) and the commercial
//! purchased-package admission; the owner only gates `finance.read` and context
//! currency before projecting the retained rows.
use crate::domain::native_r3_supplier_bill_workspace::{
    validate_rows, Row, WorkspaceActor,
};
use erpx_platform_shared_server::public::{BusinessError, CompanyId, TimestampMillis};
use std::{future::Future, pin::Pin};

pub type WorkspaceFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// The single owner read. The adapter executes the exact retained statement on
/// the caller-owned Holding unit; no SQL is planned for TypeScript to run.
pub trait WorkspaceRepository: Send {
    type Error: From<BusinessError> + Send;

    fn rows<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch: Option<&'a str>,
        department: Option<&'a str>,
    ) -> WorkspaceFuture<'a, Vec<Row>, Self::Error>;
}

/// Project the bounded supplier-bill workspace for the selected work context.
/// The `purchase` overlay consumed by the browser caller stays a composition
/// fact and is never inferred from these rows.
pub async fn supplier_bill_workspace<R: WorkspaceRepository>(
    actor: &WorkspaceActor,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Vec<Row>, R::Error> {
    actor.require_read(now)?;
    let rows = repository
        .rows(
            &actor.company_id,
            actor.branch_id.as_deref(),
            actor.department_id.as_deref(),
        )
        .await?;
    validate_rows(&rows)?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_shared_server::public::FailureKind;
    use serde_json::json;

    const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

    fn actor(permissions: &[&str]) -> WorkspaceActor {
        WorkspaceActor {
            user_id: "owner".into(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: Some("branch".into()),
            department_id: None,
            permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
            expires_at: NOW.get() + 60_000,
        }
    }

    struct Repo {
        rows: Vec<Row>,
        calls: usize,
    }

    impl WorkspaceRepository for Repo {
        type Error = BusinessError;

        fn rows<'a>(
            &'a mut self,
            _: &'a CompanyId,
            _: Option<&'a str>,
            _: Option<&'a str>,
        ) -> WorkspaceFuture<'a, Vec<Row>, Self::Error> {
            self.calls += 1;
            let rows = self.rows.clone();
            Box::pin(async move { Ok(rows) })
        }
    }

    #[tokio::test]
    async fn projects_rows_after_the_read_gate() {
        let mut repo = Repo {
            rows: vec![json!({"id": "bill-1", "status": "review"})],
            calls: 0,
        };
        let rows = supplier_bill_workspace(&actor(&["finance.read"]), &mut repo, NOW)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(repo.calls, 1);
    }

    #[tokio::test]
    async fn forbidden_actor_never_runs_sql() {
        let mut repo = Repo {
            rows: vec![json!({"id": "bill-1"})],
            calls: 0,
        };
        let error = supplier_bill_workspace(&actor(&[]), &mut repo, NOW)
            .await
            .unwrap_err();
        assert_eq!(error.kind, FailureKind::Forbidden);
        assert_eq!(repo.calls, 0);
    }

    #[tokio::test]
    async fn malformed_projection_is_rejected() {
        let mut repo = Repo {
            rows: vec![json!("not-an-object")],
            calls: 0,
        };
        let error = supplier_bill_workspace(&actor(&["finance.read"]), &mut repo, NOW)
            .await
            .unwrap_err();
        assert_eq!(error.kind, FailureKind::Unavailable);
    }
}
