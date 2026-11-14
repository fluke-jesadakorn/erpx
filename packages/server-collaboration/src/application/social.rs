//! Social command sequencing on one caller-owned owner unit.
//!
//! Composition supplies current membership/module/purchase authority and the
//! exact readable person/record targets. This module never opens, commits or
//! retries a transaction and never invents an access result or mention label.
use crate::domain::social::{
    Change, CommentSnapshot, CommentState, DraftSnapshot, FollowTarget, MAX_COMMENT_BODY,
    MAX_DRAFT_BODY, MAX_DRAFT_PARTICIPANTS, MAX_POST_BODY,
    MAX_REPORT_REASON, Mention, MentionInput, MentionKind, OwnerPlan, PostSnapshot, PostSource, Receipt, SocialActor, comment_not_found,
    draft_not_found, normalize_tags, plan_receipt, post_not_found, post_tags, require_audience, require_comment_edit,
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
    /// Resolve the current author's retained draft for update/publish.
    fn draft(
        &mut self,
        id: &str,
    ) -> impl Future<Output = Result<Option<DraftSnapshot>, Self::Error>> + Send;
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
    if matches!(&change, Change::ModeratePost { .. }) && !actor.can_hide_posts {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "POST_MODERATOR_REQUIRED",
            "A Holding owner must moderate this post.",
        )
        .into());
    }
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
                comment_upload_id,
            } => {
                let post = p.post(&post_id).await?;
                require_published_post(&actor, &post)?;
                if let Some(upload_id) = comment_upload_id.as_deref() {
                    let draft = p
                        .draft(upload_id)
                        .await?
                        .ok_or_else(draft_not_found)?;
                    if draft.author_id != actor.user_id
                        || draft.company_id != actor.company_id
                        || draft.comment_parent_post_id.as_deref() != Some(post_id.as_str())
                        || draft.published_comment_id.is_some()
                        || draft.published_post_id.is_some()
                        || draft.edit_post_id.is_some()
                        || draft.cancelled_at.is_some()
                    {
                        return Err(draft_not_found().into());
                    }
                }
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
                    comment_upload_id,
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
            Change::EditPost {
                id,
                expected_version,
                body,
                mentions: mention_inputs,
                content_json,
                rich_content_json,
                edit_upload_id,
            } => {
                validate_expected_version(expected_version)?;
                let post = p.post(&id).await?;
                require_published_post(&actor, &post)?;
                if post.author_id != actor.user_id || post.linked_record.is_some() {
                    return Err(BusinessError::new(
                        FailureKind::Forbidden,
                        "POST_AUTHOR_REQUIRED",
                        "Only the author can change this team post.",
                    )
                    .into());
                }
                if post.version != expected_version {
                    return Err(conflict().into());
                }
                if let Some(upload_id) = edit_upload_id.as_deref() {
                    let draft = p
                        .draft(upload_id)
                        .await?
                        .ok_or_else(draft_not_found)?;
                    if draft.author_id != actor.user_id
                        || draft.company_id != actor.company_id
                        || draft.edit_post_id.as_deref() != Some(id.as_str())
                        || draft.edit_post_version != Some(expected_version)
                        || draft.edit_saved_version.is_some()
                        || draft.published_post_id.is_some()
                        || draft.published_comment_id.is_some()
                        || draft.comment_parent_post_id.is_some()
                        || draft.cancelled_at.is_some()
                    {
                        return Err(draft_not_found().into());
                    }
                }
                let body = text(&body, 0, MAX_DRAFT_BODY)?;
                if body.trim().is_empty() && content_json.is_none() {
                    return Err(invalid().into());
                }
                let resolved = mentions(p, &mention_inputs).await?;
                plan = Some(OwnerPlan::EditPost {
                    id,
                    expected_version,
                    body,
                    content_json,
                    rich_content_json,
                    mentions_json: serde_json::to_string(&resolved).map_err(|_| invalid())?,
                    mentions: resolved,
                    edit_upload_id,
                });
            }
            Change::RetractPost {
                id,
                expected_version,
            } => {
                validate_expected_version(expected_version)?;
                let post = p.post(&id).await?;
                require_published_post(&actor, &post)?;
                if post.author_id != actor.user_id || post.linked_record.is_some() {
                    return Err(BusinessError::new(
                        FailureKind::Forbidden,
                        "POST_AUTHOR_REQUIRED",
                        "Only the author can change this team post.",
                    )
                    .into());
                }
                if post.version != expected_version {
                    return Err(conflict().into());
                }
                plan = Some(OwnerPlan::RetractPost {
                    id,
                    expected_version,
                });
            }
            Change::CommentUploadCreate { post_id } => {
                let post = p.post(&post_id).await?;
                require_published_post(&actor, &post)?;
                plan = Some(OwnerPlan::UploadDraftCreate {
                    id: format!("comment-upload-{}", key.as_str()),
                    author_id: actor.user_id.clone(),
                    company_id: actor.company_id.clone(),
                    branch_id: actor.branch_id.clone(),
                    comment_parent_post_id: Some(post_id),
                    edit_post_id: None,
                    edit_post_version: None,
                });
            }
            Change::CommentUploadCancel { id } => {
                let draft = p.draft(&id).await?.ok_or_else(draft_not_found)?;
                if draft.author_id != actor.user_id
                    || draft.company_id != actor.company_id
                    || draft.cancelled_at.is_some()
                    || draft.published_post_id.is_some()
                    || draft.published_comment_id.is_some()
                    || draft.edit_post_id.is_some()
                    || draft.comment_parent_post_id.is_none()
                {
                    return Err(draft_not_found().into());
                }
                plan = Some(OwnerPlan::UploadDraftCancel {
                    id,
                    version: draft.version + 1,
                });
            }
            Change::PostEditUploadCreate {
                post_id,
                expected_version,
            } => {
                let post = p.post(&post_id).await?;
                require_published_post(&actor, &post)?;
                if post.author_id != actor.user_id || post.linked_record.is_some() {
                    return Err(BusinessError::new(
                        FailureKind::Forbidden,
                        "POST_AUTHOR_REQUIRED",
                        "Only the author can edit this team post.",
                    )
                    .into());
                }
                if post.version != expected_version {
                    return Err(conflict().into());
                }
                plan = Some(OwnerPlan::UploadDraftCreate {
                    id: format!("post-edit-upload-{}", key.as_str()),
                    author_id: actor.user_id.clone(),
                    company_id: actor.company_id.clone(),
                    branch_id: actor.branch_id.clone(),
                    comment_parent_post_id: None,
                    edit_post_id: Some(post_id),
                    edit_post_version: Some(expected_version),
                });
            }
            Change::PublishHandoverPost {
                body,
                recipient_ids,
                sources,
            } => {
                let body = text(&body, 1, MAX_DRAFT_BODY)?;
                let mut recipients = recipient_ids;
                recipients.sort();
                recipients.dedup();
                if recipients.is_empty() || recipients.len() > MAX_DRAFT_PARTICIPANTS || sources.len() > 5 {
                    return Err(invalid().into());
                }
                plan = Some(OwnerPlan::PublishHandoverPost {
                    post_id: format!("post-post-draft-{}", key.as_str()),
                    author_id: actor.user_id.clone(),
                    author_name: actor.display_name.clone(),
                    company_id: actor.company_id.clone(),
                    branch_id: actor.branch_id.clone(),
                    department_id: actor.department_id.clone(),
                    body,
                    participants: recipients,
                    sources,
                });
            }
            Change::ModeratePost { id } => {
                let post = p.post(&id).await?;
                require_published_post(&actor, &post)?;
                plan = Some(OwnerPlan::ModeratePost {
                    id,
                    version: post.version + 1,
                });
            }
            Change::PostEditUploadCancel { id } => {
                let draft = p.draft(&id).await?.ok_or_else(draft_not_found)?;
                if draft.author_id != actor.user_id
                    || draft.company_id != actor.company_id
                    || draft.edit_post_id.is_none()
                    || draft.edit_saved_version.is_some()
                    || draft.cancelled_at.is_some()
                    || draft.published_post_id.is_some()
                    || draft.published_comment_id.is_some()
                    || draft.comment_parent_post_id.is_some()
                {
                    return Err(draft_not_found().into());
                }
                plan = Some(OwnerPlan::UploadDraftCancel {
                    id,
                    version: draft.version + 1,
                });
            }
            Change::PublishDraft { id, version } => {
                validate_expected_version(version)?;
                let draft = p.draft(&id).await?.ok_or_else(draft_not_found)?;
                if draft.author_id != actor.user_id
                    || draft.company_id != actor.company_id
                    || draft.published_post_id.is_some()
                {
                    return Err(draft_not_found().into());
                }
                if draft.version != version {
                    return Err(conflict().into());
                }
                let audience_kind = draft.audience_kind.clone().ok_or_else(invalid)?;
                if !matches!(
                    audience_kind.as_str(),
                    "company" | "branch" | "department" | "participants" | "team" | "project"
                ) {
                    return Err(invalid().into());
                }
                let participants: Vec<String> =
                    serde_json::from_str(&draft.participants_json).map_err(|_| invalid())?;
                if participants.len() > MAX_DRAFT_PARTICIPANTS {
                    return Err(invalid().into());
                }
                let sources: Vec<PostSource> =
                    serde_json::from_str(&draft.sources_json).map_err(|_| invalid())?;
                if sources.len() > 5 {
                    return Err(invalid().into());
                }
                let resolved: Vec<Mention> =
                    serde_json::from_str(&draft.mentions_json).map_err(|_| invalid())?;
                let body = text(&draft.body, 0, MAX_DRAFT_BODY)?;
                if body.trim().is_empty() && draft.content_json.is_none() {
                    return Err(invalid().into());
                }
                plan = Some(OwnerPlan::PublishDraftPost {
                    draft_id: id.clone(),
                    draft_version: version,
                    post_id: format!("post-{id}"),
                    author_id: actor.user_id.clone(),
                    author_name: actor.display_name.clone(),
                    company_id: actor.company_id.clone(),
                    branch_id: draft.branch_id.clone(),
                    department_id: draft.department_id.clone(),
                    body,
                    audience_kind,
                    audience_id: draft.audience_id.clone(),
                    participants,
                    sources,
                    mentions: resolved,
                    content_json: draft.content_json.clone(),
                    rich_content_json: draft.rich_content_json.clone(),
                    purpose: draft.purpose.clone(),
                });
            }
            Change::SavePostDraft {
                id,
                body,
                audience_kind,
                audience_id,
                participants,
                sources_json,
                purpose,
                mentions: mention_inputs,
                content_json,
                rich_content_json,
                expected_version,
            } => {
                validate_expected_version(expected_version)?;
                let body = text(&body, 0, MAX_DRAFT_BODY)?;
                if participants.len() > MAX_DRAFT_PARTICIPANTS
                    || participants.iter().any(|user| !(1..=160).contains(&user.len()))
                    || sources_json.len() > 20_000
                    || !sources_json.trim_start().starts_with('[')
                {
                    return Err(invalid().into());
                }
                if let Some(kind) = audience_kind.as_deref() {
                    if !matches!(
                        kind,
                        "company" | "branch" | "department" | "participants" | "team" | "project"
                    ) {
                        return Err(invalid().into());
                    }
                }
                let mentions = mentions(p, &mention_inputs).await?;
                let record_id = id
                    .clone()
                    .unwrap_or_else(|| format!("post-draft-{}", key.as_str()));
                let version = match &id {
                    Some(current_id) => {
                        let draft = p.draft(current_id).await?.ok_or_else(draft_not_found)?;
                        if draft.author_id != actor.user_id
                            || draft.company_id != actor.company_id
                            || draft.published_post_id.is_some()
                        {
                            return Err(draft_not_found().into());
                        }
                        if draft.version != expected_version {
                            return Err(conflict().into());
                        }
                        expected_version + 1
                    }
                    None => 1,
                };
                plan = Some(OwnerPlan::SaveDraft {
                    id: record_id,
                    author_id: actor.user_id.clone(),
                    company_id: actor.company_id.clone(),
                    branch_id: actor.branch_id.clone(),
                    department_id: actor.department_id.clone(),
                    body,
                    version,
                    audience_kind,
                    audience_id,
                    participants_json: serde_json::to_string(&participants)
                        .map_err(|_| invalid())?,
                    sources_json,
                    purpose,
                    mentions_json: serde_json::to_string(&mentions).map_err(|_| invalid())?,
                    content_json,
                    rich_content_json,
                });
            }
            Change::CreateStory {
                title,
                body,
                theme,
                audience,
            } => {
                let branch_id = require_audience(&actor, audience)?;
                let title = text(&title, 1, 80)?;
                let body = text(&body, 1, 500)?;
                if !matches!(theme.as_str(), "sage" | "sunset" | "sky" | "plum") {
                    return Err(invalid().into());
                }
                plan = Some(OwnerPlan::CreateStory {
                    id: format!("story-{}", key.as_str()),
                    author_name: actor.display_name.clone(),
                    company_id: actor.company_id.clone(),
                    branch_id,
                    department_id: actor.department_id.clone(),
                    title,
                    body,
                    theme,
                });
            }
            Change::ArchiveStory {
                id,
                expected_version,
            } => {
                validate_expected_version(expected_version)?;
                plan = Some(OwnerPlan::ArchiveStory {
                    id,
                    expected_version,
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

