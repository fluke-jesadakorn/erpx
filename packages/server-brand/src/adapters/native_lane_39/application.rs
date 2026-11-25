//! Lane 39 Brand application: the query port and the SQL-free handler.
//!
//! Composition injects the owner reader behind `WorkspaceChoiceRead`. The
//! handler owns validation, projection and the response shape; the reader owns
//! the query and its Company scope. No SQL and no HTTP live here.

use crate::adapters::native_lane_39::domain::{
    project, validate_company_ids, WorkspaceChoice,
};
use erpx_server_runtime::public::http_support::serde_json::{json, Value};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type WorkspaceChoiceFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner query port. The Brand crate owns the relation, the Company scope and
/// the projection; the router only transports bounded Company identities.
pub trait WorkspaceChoiceRead: Send + Sync {
    fn workspace_choices<'a>(
        &'a self,
        company_ids: &'a [String],
    ) -> WorkspaceChoiceFuture<'a, Vec<WorkspaceChoice>>;
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "BRAND_WORKSPACE_CHOICE_UNAVAILABLE",
        "The native Brand workspace choice service is unavailable.",
    )
}

/// Validate, read and render one bounded workspace-choice response.
pub async fn read_workspace_choices(
    reader: &dyn WorkspaceChoiceRead,
    company_ids: Vec<String>,
) -> Result<Value, BusinessError> {
    validate_company_ids(&company_ids)?;
    let rows = reader.workspace_choices(&company_ids).await.map_err(|error| {
        if error.kind == erpx_server_shared::public::FailureKind::InvalidInput {
            error
        } else {
            unavailable()
        }
    })?;
    let rows = project(rows)?;
    let brands = rows
        .into_iter()
        .map(|row| {
            json!({
                "scope_kind": row.scope_kind.as_str(),
                "scope_id": row.scope_id,
                "primary_publication_id": row.primary_publication_id,
            })
        })
        .collect::<Vec<Value>>();
    Ok(json!({ "brands": brands }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_39::domain::ScopeKind;
    use std::sync::{Arc, Mutex};

    struct Fake {
        seen: Arc<Mutex<Vec<String>>>,
        rows: Vec<WorkspaceChoice>,
    }

    impl WorkspaceChoiceRead for Fake {
        fn workspace_choices<'a>(
            &'a self,
            company_ids: &'a [String],
        ) -> WorkspaceChoiceFuture<'a, Vec<WorkspaceChoice>> {
            *self.seen.lock().unwrap() = company_ids.to_vec();
            let rows = self.rows.clone();
            Box::pin(async move { Ok(rows) })
        }
    }

    fn choice(kind: ScopeKind, id: &str, publication: Option<&str>) -> WorkspaceChoice {
        WorkspaceChoice {
            scope_kind: kind,
            scope_id: id.into(),
            primary_publication_id: publication.map(str::to_owned),
        }
    }

    #[tokio::test]
    async fn handler_projects_the_exact_response_shape_and_binds_are_passed_through() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Fake {
            seen: seen.clone(),
            rows: vec![
                choice(ScopeKind::Holding, "holding-1", Some("pub-1")),
                choice(ScopeKind::Branch, "branch-1", None),
            ],
        };
        let value = read_workspace_choices(&reader, vec!["co-a".into(), "co-b".into()])
            .await
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), vec!["co-a".to_owned(), "co-b".to_owned()]);
        assert_eq!(
            value,
            json!({"brands":[
                {"scope_kind":"holding","scope_id":"holding-1","primary_publication_id":"pub-1"},
                {"scope_kind":"branch","scope_id":"branch-1","primary_publication_id":null}
            ]})
        );
    }

    #[tokio::test]
    async fn handler_rejects_unbounded_or_malformed_company_sets_before_reading() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Fake { seen: seen.clone(), rows: vec![] };
        assert_eq!(
            read_workspace_choices(&reader, vec![])
                .await
                .unwrap_err()
                .code,
            "BRAND_WORKSPACE_CHOICE_REQUEST_INVALID"
        );
        assert!(seen.lock().unwrap().is_empty());
    }
}
