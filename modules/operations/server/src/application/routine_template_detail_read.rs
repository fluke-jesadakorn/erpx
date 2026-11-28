//! Application port and handler for the native routine-template detail read.
//! No SQL is executed here: the owner reader executes the statement and the
//! composition scope port supplies only owner-owned static predicates.

use crate::application::routine_template_read::{TemplateScope, TemplateScopeFragment};
use crate::domain::routine_template_detail_read::validate_template_id;
use crate::domain::routine_template_read::{MAX_ID_BYTES, TemplateActor, TemplateRow, validate_identity};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin, sync::Arc};

pub type TemplateDetailFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader port. Implementations are the only layer allowed to execute
/// the routine-template detail statement.
pub trait TemplateDetailRead: Send + Sync {
    fn detail<'a>(
        &'a self,
        company: &'a str,
        id: &'a str,
        scope: TemplateScopeFragment,
    ) -> TemplateDetailFuture<'a, Option<TemplateRow>>;
}

/// Validation and response assembly stay in the application layer; the axum
/// handler only transports the resulting row.
#[derive(Clone)]
pub struct TemplateDetailHandler {
    reader: Arc<dyn TemplateDetailRead>,
    scope: Arc<dyn TemplateScope>,
}

impl TemplateDetailHandler {
    pub fn new(reader: Arc<dyn TemplateDetailRead>, scope: Arc<dyn TemplateScope>) -> Self {
        Self { reader, scope }
    }

    pub fn with_trusted_scope(reader: Arc<dyn TemplateDetailRead>) -> Self {
        Self::new(
            reader,
            Arc::new(crate::application::routine_template_read::TrustedTemplateScope),
        )
    }

    pub fn detail<'a>(
        &'a self,
        actor: &'a TemplateActor,
        id: &'a str,
    ) -> TemplateDetailFuture<'a, Option<TemplateRow>> {
        let company = actor.company_id.clone();
        let id = id.to_owned();
        Box::pin(async move {
            validate_identity(&company, "company")?;
            validate_identity(&actor.user_id, "user")?;
            validate_identity(&actor.assignment_id, "assignment")?;
            validate_template_id(&id)?;
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
                    .any(|value| value.is_empty() || value.len() > MAX_ID_BYTES)
            {
                return Err(BusinessError::invalid(
                    "OPERATIONS_ROUTINE_SCOPE_INVALID",
                    "Use bounded Operations scope values.",
                ));
            }
            self.reader.detail(&company, &id, scope).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::routine_template_detail_read::render_detail_sql;
    use crate::application::routine_template_read::TemplateScopeFragment;

    #[derive(Clone)]
    struct Probe;

    impl TemplateDetailRead for Probe {
        fn detail<'a>(
            &'a self,
            company: &'a str,
            id: &'a str,
            scope: TemplateScopeFragment,
        ) -> TemplateDetailFuture<'a, Option<TemplateRow>> {
            let company = company.to_owned();
            let id = id.to_owned();
            Box::pin(async move {
                assert_eq!(scope.predicate, "TRUE");
                assert_eq!(render_detail_sql(&scope.predicate).matches("$1").count(), 1);
                Ok(Some(TemplateRow {
                    id,
                    title: "Weekly close".into(),
                    description: String::new(),
                    company_id: company,
                    branch_id: None,
                    department_id: None,
                    creator_user_id: "u-1".into(),
                    assignee_user_id: "u-2".into(),
                    project_id: None,
                    start_approvers_json: "[]".into(),
                    completion_approvers_json: "[]".into(),
                    published: 1,
                    version: 2,
                    created_at: 100,
                    updated_at: 200,
                }))
            })
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
    async fn handler_validates_actor_and_template_id_before_delegating() {
        let handler = TemplateDetailHandler::with_trusted_scope(Arc::new(Probe));
        assert!(handler.detail(&actor(), "routine-1").await.unwrap().is_some());
        assert!(handler.detail(&actor(), "bad id").await.is_err());
        let mut bad = actor();
        bad.company_id = String::new();
        assert!(handler.detail(&bad, "routine-1").await.is_err());
    }
}
