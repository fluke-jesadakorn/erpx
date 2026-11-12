//! Social command sequencing on one caller-owned owner unit.
//!
//! Composition supplies current membership/module/purchase authority and the
//! exact readable person/record targets. This module never opens, commits or
//! retries a transaction and never invents an access result or mention label.
use crate::domain::social::{
    Change, CommentSnapshot, CommentState, FollowTarget, MAX_COMMENT_BODY, MAX_POST_BODY,
    MAX_REPORT_REASON, Mention, MentionInput, MentionKind, OwnerPlan, PostSnapshot, Receipt, SocialActor, comment_not_found,
    normalize_tags, plan_receipt, post_not_found, post_tags, require_audience, require_comment_edit,
    require_comment_removal, require_published_post, text, validate_expected_version,
    validate_mentions,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::future::Future;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Read,
    Write,
}

pub trait SocialUnit: Send {
    type Error: From<BusinessError> + Send;
    fn authorize(
        &mut self,
        access: Access,
    ) -> impl Future<Output = Result<SocialActor, Self::Error>> + Send;
    fn replay(
        &mut self,
        change: &Change,
    ) -> impl Future<Output = Result<Option<Receipt>, Self::Error>> + Send;
    fn post(&mut self, id: &str) -> impl Future<Output = Result<PostSnapshot, Self::Error>> + Send;
    fn comment(
        &mut self,
        id: &str,
        post_id: &str,
    ) -> impl Future<Output = Result<CommentSnapshot, Self::Error>> + Send;
    /// Resolve a current Company-readable person; composition supplies the label.
    fn resolve_person(
        &mut self,
        id: &str,
    ) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send;
    /// Resolve a current readable record through its owning port, never SQL here.
    fn resolve_record(
        &mut self,
        module: &str,
        id: &str,
    ) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send;
    /// Stage the exact owner rows; every failure must roll back the caller unit.
    fn apply(
        &mut self,
        actor: &SocialActor,
        key: &CommandKey,
        plan: OwnerPlan,
    ) -> impl Future<Output = Result<Receipt, Self::Error>> + Send;
    fn flush_deferred(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Revalidate selected IAM/purchase and every retained owner row.
    fn require_unit_current(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

fn invalid() -> BusinessError {
    BusinessError::invalid("SOCIAL_INPUT_INVALID", "Provide a valid collaboration value.")
}

fn conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "VERSION_CONFLICT",
        "Refresh the conversation before saving or removing this comment.",
    )
}

fn mention_missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "MENTION_TARGET_NOT_FOUND",
        "That person or record is not available in this Company.",
    )
}

async fn mentions<P: SocialUnit>(
    p: &mut P,
    inputs: &[MentionInput],
) -> Result<Vec<Mention>, P::Error> {
    let inputs = validate_mentions(inputs)?;
    let mut resolved = Vec::with_capacity(inputs.len());
    for mention in inputs {
        match mention.kind {
            MentionKind::Person => {
                let label = p
                    .resolve_person(&mention.target_id)
                    .await?
                    .filter(|label| !label.is_empty())
                    .ok_or_else(mention_missing)?;
                resolved.push(Mention {
                    kind: MentionKind::Person,
                    target_id: mention.target_id,
                    label,
                });
            }
            MentionKind::Record => {
                let (module, id) = mention.target_id.split_once('/').ok_or_else(invalid)?;
                if module.is_empty() || id.is_empty() {
                    return Err(invalid().into());
                }
                let label = p
                    .resolve_record(module, id)
                    .await?
                    .filter(|label| !label.is_empty())
                    .ok_or_else(mention_missing)?;
                resolved.push(Mention {
                    kind: MentionKind::Record,
                    target_id: mention.target_id,
                    label,
                });
            }
        }
    }
    Ok(resolved)
}

/// Execute a Social change. The caller owns the serializable transaction and
/// the Workflow ledger; this function never opens or commits one. Exact replay
/// is resolved after current authority and still revalidates retained sources.
pub async fn execute<P: SocialUnit>(
    p: &mut P,
    key: &CommandKey,
    change: Change,
) -> Result<Receipt, P::Error> {
    let actor = p.authorize(Access::Write).await?;
    let prior = p.replay(&change).await?;
    let mut plan = None;
    if prior.is_none() {
        match change {
            Change::PublishPost {
                body,
                audience,
                tags,
                mentions: mention_inputs,
                linked_record,
            } => {
                let branch_id = require_audience(&actor, audience)?;
                let body = text(&body, 1, MAX_POST_BODY)?;
                let tags = post_tags(&tags, &body)?;
                let mentions = mentions(p, &mention_inputs).await?;
                let linked_record = match linked_record {
                    None => None,
                    Some(record) => {
                        if record.module.is_empty() || record.id.is_empty() {
                            return Err(invalid().into());
                        }
                        let label = p
                            .resolve_record(&record.module, &record.id)
                            .await?
                            .filter(|label| !label.is_empty())
                            .ok_or_else(mention_missing)?;
                        Some(crate::domain::social::LinkedRecord {
                            module: record.module,
                            id: record.id,
                            label,
                        })
                    }
                };
                plan = Some(OwnerPlan::CreatePost {
                    id: format!("post-{}", key.as_str()),
                    author_name: actor.display_name.clone(),
                    company_id: actor.company_id.clone(),
                    branch_id,
                    department_id: actor.department_id.clone(),
                    audience,
                    body,
                    tags,
                    mentions,
                    linked_record,
                });
            }
            Change::PublishComment {
                post_id,
                body,
                parent_comment_id,
                mentions: mention_inputs,
            } => {
                let post = p.post(&post_id).await?;
                require_published_post(&actor, &post)?;
                let body = text(&body, 1, MAX_COMMENT_BODY)?;
                if let Some(parent) = parent_comment_id.as_deref() {
                    let parent = p.comment(parent, &post_id).await?;
                    if parent.state != CommentState::Published {
                        return Err(comment_not_found().into());
                    }
                }
                let tags = normalize_tags(&crate::domain::social::hashtags(&body))?;
                let mentions = mentions(p, &mention_inputs).await?;
                plan = Some(OwnerPlan::CreateComment {
                    id: format!("comment-{}", key.as_str()),
                    post_id,
                    company_id: actor.company_id.clone(),
                    author_name: actor.display_name.clone(),
                    body,
                    tags,
                    mentions,
                    parent_comment_id,
                });
            }
            Change::EditComment {
                post_id,
                id,
                expected_version,
                body,
                mentions: mention_inputs,
            } => {
                validate_expected_version(expected_version)?;
                let post = p.post(&post_id).await?;
                require_published_post(&actor, &post)?;
                let comment = p.comment(&id, &post_id).await?;
                require_comment_edit(&actor, &comment)?;
                if comment.version != expected_version {
                    return Err(conflict().into());
                }
                let body = text(&body, 1, MAX_COMMENT_BODY)?;
                let tags = normalize_tags(&crate::domain::social::hashtags(&body))?;
                let mentions = mentions(p, &mention_inputs).await?;
                plan = Some(OwnerPlan::UpdateComment {
                    id,
                    post_id,
                    expected_version,
                    body,
                    tags,
                    mentions,
                });
            }
            Change::TombstoneComment {
                post_id,
                id,
                expected_version,
            } => {
                validate_expected_version(expected_version)?;
                let post = p.post(&post_id).await?;
                require_published_post(&actor, &post)?;
                let comment = p.comment(&id, &post_id).await?;
                require_comment_removal(&actor, &comment)?;
                if comment.version != expected_version {
                    return Err(conflict().into());
                }
                plan = Some(OwnerPlan::TombstoneComment {
                    id,
                    post_id,
                    expected_version,
                });
            }
            Change::SetReaction { post_id, active } => {
                let post = p.post(&post_id).await?;
                require_published_post(&actor, &post)?;
                plan = Some(OwnerPlan::SetReaction {
                    post_id,
                    version: post.version,
                    active,
                });
            }
            Change::SetFollow {
                target,
                target_id,
                active,
            } => {
                let (target_id, version) = match target {
                    FollowTarget::Post => {
                        let post = p.post(&target_id).await?;
                        require_published_post(&actor, &post)?;
                        (target_id, post.version)
                    }
                    FollowTarget::Topic => (
                        crate::domain::social::normalize_tag(&target_id)?,
                        1,
                    ),
                };
                plan = Some(OwnerPlan::SetFollow {
                    target,
                    target_id,
                    version,
                    active,
                });
            }
            Change::ReportPost { post_id, reason } => {
                let post = p.post(&post_id).await?;
                require_published_post(&actor, &post)?;
                let reason = text(&reason, 1, MAX_REPORT_REASON)?;
                plan = Some(OwnerPlan::ReportPost {
                    id: format!("report-{}", key.as_str()),
                    post_id,
                    reporter_id: actor.user_id.clone(),
                    reason,
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
pub async fn authorize_read<P: SocialUnit>(p: &mut P) -> Result<SocialActor, P::Error> {
    let actor = p.authorize(Access::Read).await?;
    if actor.company_id.is_empty() {
        return Err(post_not_found().into());
    }
    Ok(actor)
}

