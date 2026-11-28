//! Application port and handler for the native routine-template list read.
//! No SQL is executed here: the owner reader executes the statement and the
//! scope port supplies only owner-owned static predicates.

use crate::domain::routine_template_read::{
    TemplateActor, TemplateRow, validate_identity,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin, sync::Arc};

pub type TemplateListFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Trusted static predicate plus its bound values. Both are produced by Rust
/// owner code, never by a request body, query string or header value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateScopeFragment {
    pub predicate: String,
    pub binds: Vec<String>,
}

/// Composition supplies the purchased-capability / actor-visibility predicate.
/// The default port keeps the canonical unscoped read and can be replaced in
/// composition without changing the HTTP or Postgres layers.
pub trait TemplateScope: Send + Sync {
    fn fragment(&self, actor: &TemplateActor) -> TemplateScopeFragment;
}

#[derive(Clone, Copy, Default)]
pub struct TrustedTemplateScope;

impl TemplateScope for TrustedTemplateScope {
    fn fragment(&self, _actor: &TemplateActor) -> TemplateScopeFragment {
        TemplateScopeFragment {
            predicate: "TRUE".to_owned(),
            binds: Vec::new(),
        }
    }
}

/// Owner reader port. Implementations are the only layer allowed to execute
/// the routine-template statement.
pub trait TemplateRead: Send + Sync {
    fn list<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
        assignment: &'a str,
        scope: TemplateScopeFragment,
    ) -> TemplateListFuture<'a, Vec<TemplateRow>>;
}

/// Validation and response assembly stay in the application layer; the axum
/// handler only transports the resulting rows.
#[derive(Clone)]
pub struct TemplateReadHandler {
    reader: Arc<dyn TemplateRead>,
    scope: Arc<dyn TemplateScope>,
}

impl TemplateReadHandler {
    pub fn new(reader: Arc<dyn TemplateRead>, scope: Arc<dyn TemplateScope>) -> Self {
        Self { reader, scope }
    }

    pub fn with_trusted_scope(reader: Arc<dyn TemplateRead>) -> Self {
        Self::new(reader, Arc::new(TrustedTemplateScope))
    }

    pub fn list<'a>(&'a self, actor: &'a TemplateActor) -> TemplateListFuture<'a, Vec<TemplateRow>> {
        let company = actor.company_id.clone();
        let user = actor.user_id.clone();
        let assignment = actor.assignment_id.clone();
        Box::pin(async move {
            validate_identity(&company, "company")?;
            validate_identity(&user, "user")?;
            validate_identity(&assignment, "assignment")?;
            let scope = self.scope.fragment(actor);
            if scope.predicate.is_empty()
                || scope.predicate.len() > 4096
                || !scope
                    .predicate
                    .bytes()
                    .all(|byte| byte.is_ascii_graphic() || byte == b' ' || byte == b'\n')
            {
                return Err(BusinessError::invalid(
                    "OPERATIONS_ROUTINE_SCOPE_INVALID",
                    "Use one bounded Operations read scope.",
                ));
            }
            if scope.binds.len() > 32
                || scope
                    .binds
                    .iter()
                    .any(|value| value.is_empty() || value.len() > crate::domain::routine_template_read::MAX_ID_BYTES)
            {
                return Err(BusinessError::invalid(
                    "OPERATIONS_ROUTINE_SCOPE_INVALID",
                    "Use bounded Operations scope values.",
                ));
            }
            self.reader
                .list(&company, &user, &assignment, scope)
                .await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Probe;

    impl TemplateRead for Probe {
        fn list<'a>(
            &'a self,
            company: &'a str,
            _user: &'a str,
            _assignment: &'a str,
            scope: TemplateScopeFragment,
        ) -> TemplateListFuture<'a, Vec<TemplateRow>> {
            let company = company.to_owned();
            Box::pin(async move {
                assert_eq!(scope.predicate, "TRUE");
                Ok(vec![TemplateRow {
                    id: format!("{company}-routine"),
                    title: "Close".into(),
                    description: String::new(),
                    company_id: company,
                    branch_id: None,
                    department_id: None,
                    creator_user_id: "u-1".into(),
                    assignee_user_id: "u-1".into(),
                    project_id: None,
                    start_approvers_json: "[]".into(),
                    completion_approvers_json: "[]".into(),
                    published: 1,
                    version: 1,
                    created_at: 10,
                    updated_at: 20,
                }])
            })
        }
    }

    struct EmptyScope;
    impl TemplateScope for EmptyScope {
        fn fragment(&self, _actor: &TemplateActor) -> TemplateScopeFragment {
            TemplateScopeFragment {
                predicate: String::new(),
                binds: Vec::new(),
            }
        }
    }

    fn actor() -> TemplateActor {
        TemplateActor {
            company_id: "acme:001".into(),
            user_id: "user-1".into(),
            assignment_id: "assign-1".into(),
            revision: 7,
            assignment_version: 3,
        }
    }

    #[tokio::test]
    async fn handler_validates_actor_and_scope_before_delegating() {
        let handler = TemplateReadHandler::with_trusted_scope(Arc::new(Probe));
        assert_eq!(handler.list(&actor()).await.unwrap().len(), 1);
        let mut bad = actor();
        bad.company_id = String::new();
        assert!(handler.list(&bad).await.is_err());
        bad = actor();
        bad.assignment_id = "bad assignment".into();
        assert!(handler.list(&bad).await.is_err());
        let scoped = TemplateReadHandler::new(Arc::new(Probe), Arc::new(EmptyScope));
        assert!(scoped.list(&actor()).await.is_err());
    }
}
