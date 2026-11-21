//! Handbook command sequencing on one caller-owned owner unit.
//!
//! Composition supplies current membership/editor/owner authority, purchase
//! access and the Workflow ledger. This module never opens, commits or retries a
//! transaction and never invents an access result.
use crate::domain::handbook::{
    ArticleAction, ArticleSnapshot, AuthorizedHandbookActor, Category, Change, DraftContent,
    OwnerPlan, Receipt, article_body, article_title, changed, plan_receipt, publication_ready,
    require_edit, require_manage_editors, validate_editor_user, validate_expected_version,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::future::Future;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Read,
    Edit,
    ManageEditors,
}

pub trait HandbookUnit: Send {
    type Error: From<BusinessError> + Send;
    fn authorize(
        &mut self,
        access: Access,
    ) -> impl Future<Output = Result<AuthorizedHandbookActor, Self::Error>> + Send;
    fn replay(
        &mut self,
        change: &Change,
    ) -> impl Future<Output = Result<Option<Receipt>, Self::Error>> + Send;
    fn article(
        &mut self,
        id: &str,
    ) -> impl Future<Output = Result<ArticleSnapshot, Self::Error>> + Send;
    fn settings_version(&mut self) -> impl Future<Output = Result<i64, Self::Error>> + Send;
    fn is_company_member(
        &mut self,
        user_id: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Stage the exact owner rows; every failure must roll back the caller unit.
    fn apply(
        &mut self,
        actor: &AuthorizedHandbookActor,
        key: &CommandKey,
        plan: OwnerPlan,
    ) -> impl Future<Output = Result<Receipt, Self::Error>> + Send;
    /// Flush deferred database constraints before currentness is revalidated.
    fn flush_deferred(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Revalidate selected IAM, purchase, editor authority and every retained owner row.
    fn require_unit_current(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

fn invalid() -> BusinessError {
    BusinessError::invalid("HANDBOOK_INPUT_INVALID", "Provide a valid handbook value.")
}

fn conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "VERSION_CONFLICT",
        "Review the current article before saving or changing availability.",
    )
}

fn validated(title: &str, category: Category, body: &str) -> Result<DraftContent, BusinessError> {
    Ok(DraftContent {
        title: article_title(title)?,
        category,
        body: article_body(body)?,
    })
}

/// Execute a Handbook article/editor change. The caller owns the serializable
/// transaction and the Workflow ledger; this function never opens or commits one.
/// Exact replay is resolved only after current authority is established, and a
/// replay still revalidates every retained source before returning.
pub async fn execute<P: HandbookUnit>(
    p: &mut P,
    key: &CommandKey,
    change: Change,
) -> Result<Receipt, P::Error> {
    let access = match &change {
        Change::CreateArticle { .. }
        | Change::UpdateArticle { .. }
        | Change::ArticleAction { .. } => Access::Edit,
        Change::SetEditor { .. } => Access::ManageEditors,
    };
    let actor = p.authorize(access).await?;
    match access {
        Access::Edit => require_edit(&actor)?,
        Access::ManageEditors => require_manage_editors(&actor)?,
        Access::Read => return Err(invalid().into()),
    }
    let prior = p.replay(&change).await?;
    let mut plan = None;
    if prior.is_none() {
        match &change {
            Change::CreateArticle {
                title,
                category,
                body,
            } => {
                plan = Some(OwnerPlan::CreateArticle {
                    id: format!("article-{}", key.as_str()),
                    content: validated(title, *category, body)?,
                });
            }
            Change::UpdateArticle {
                id,
                expected_version,
                title,
                category,
                body,
            } => {
                validate_expected_version(*expected_version, false)?;
                let current = p.article(id).await?;
                if current.version != *expected_version {
                    return Err(conflict().into());
                }
                let revision = current
                    .draft_revision
                    .checked_add(1)
                    .filter(|next| *next <= crate::domain::handbook::MAX_SAFE_VERSION)
                    .ok_or_else(changed)?;
                plan = Some(OwnerPlan::UpdateArticle {
                    id: id.clone(),
                    expected_version: *expected_version,
                    revision,
                    content: validated(title, *category, body)?,
                });
            }
            Change::ArticleAction {
                id,
                expected_version,
                action,
            } => {
                validate_expected_version(*expected_version, false)?;
                let current = p.article(id).await?;
                if current.version != *expected_version {
                    return Err(conflict().into());
                }
                plan = Some(match action {
                    ArticleAction::Publish => {
                        if !publication_ready(&current.draft.title, &current.draft.body) {
                            return Err(BusinessError::invalid(
                                "ARTICLE_CONTENT_REQUIRED",
                                "Add a title and article content before publishing.",
                            )
                            .into());
                        }
                        OwnerPlan::PublishArticle {
                            id: id.clone(),
                            expected_version: *expected_version,
                            revision: current.draft_revision,
                        }
                    }
                    ArticleAction::Archive | ArticleAction::Restore => OwnerPlan::SetArchived {
                        id: id.clone(),
                        expected_version: *expected_version,
                        archived: *action == ArticleAction::Archive,
                    },
                });
            }
            Change::SetEditor {
                user_id,
                active,
                expected_version,
            } => {
                validate_editor_user(user_id)?;
                validate_expected_version(*expected_version, true)?;
                let settings = p.settings_version().await?;
                if settings != *expected_version {
                    return Err(conflict().into());
                }
                if *active && !p.is_company_member(user_id).await? {
                    return Err(BusinessError::invalid(
                        "EDITOR_MEMBER_REQUIRED",
                        "Invite or activate this company member in People first.",
                    )
                    .into());
                }
                plan = Some(OwnerPlan::SetEditor {
                    user_id: user_id.clone(),
                    active: *active,
                    expected_version: *expected_version,
                });
            }
        }
    }
    let result = match prior {
        Some(receipt) => receipt,
        None => {
            let plan = plan.ok_or_else(invalid)?;
            plan_receipt(key, &plan)?;
            p.apply(&actor, key, plan).await?
        }
    };
    p.flush_deferred().await?;
    p.require_unit_current().await?;
    Ok(result)
}

/// Read-side authorization. The caller must already hold the same Company
/// transaction it will use for the returned native rows.
pub async fn authorize_read<P: HandbookUnit>(
    p: &mut P,
) -> Result<AuthorizedHandbookActor, P::Error> {
    let actor = p.authorize(Access::Read).await?;
    crate::domain::handbook::require_read(&actor)?;
    Ok(actor)
}
