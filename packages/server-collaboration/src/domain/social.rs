//! Company conversation decisions.
//!
//! This module owns posts, comments, topics, reactions and follows only.
//! Current IAM/purchase authority and cross-owner mention targets remain owner
//! ports supplied by composition; no value here is authority.
use crate::domain::collaboration_work::js_trim;
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};

pub const MAX_SAFE_VERSION: i64 = 9_007_199_254_740_991;
pub const MAX_POST_BODY: usize = 3000;
pub const MAX_COMMENT_BODY: usize = 10_000;
pub const MAX_REPORT_REASON: usize = 2000;
pub const MAX_DRAFT_BODY: usize = 30_000;
pub const MAX_DRAFT_PARTICIPANTS: usize = 20;
pub const MAX_TAGS: usize = 5;
pub const MAX_MENTIONS: usize = 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Audience {
    Holding,
    Branch,
}
impl Audience {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "holding" => Ok(Self::Holding),
            "branch" => Ok(Self::Branch),
            _ => Err(invalid()),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Holding => "holding",
            Self::Branch => "branch",
        }
    }
    pub fn stored(self) -> &'static str {
        match self {
            Self::Holding => "company",
            Self::Branch => "branch",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MentionKind {
    Person,
    Record,
}
impl MentionKind {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "person" => Ok(Self::Person),
            "record" => Ok(Self::Record),
            _ => Err(invalid()),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Record => "record",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FollowTarget {
    Post,
    Topic,
}
impl FollowTarget {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "post" => Ok(Self::Post),
            "topic" => Ok(Self::Topic),
            _ => Err(invalid()),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Post => "post",
            Self::Topic => "topic",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostState {
    Published,
    Archived,
    Moderated,
}
impl PostState {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "published" => Ok(Self::Published),
            "archived" => Ok(Self::Archived),
            "moderated" => Ok(Self::Moderated),
            _ => Err(changed()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommentState {
    Published,
    Tombstoned,
}
impl CommentState {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "published" => Ok(Self::Published),
            "tombstoned" => Ok(Self::Tombstoned),
            _ => Err(changed()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SocialActor {
    pub user_id: String,
    pub display_name: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    /// Current role may share with the whole Company.
    pub can_share_company: bool,
    /// Current role may tombstone another author's comment.
    pub can_moderate: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MentionInput {
    pub kind: MentionKind,
    pub target_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mention {
    pub kind: MentionKind,
    pub target_id: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize)]
pub struct PostSource {
    pub kind: String,
    pub id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkedRecordInput {
    pub module: String,
    pub id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct LinkedRecord {
    pub module: String,
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostSnapshot {
    pub id: String,
    pub author_id: String,
    pub author_name: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub audience: Audience,
    pub body: String,
    pub version: i64,
    pub state: PostState,
    pub topics: Vec<String>,
    pub mentions: Vec<Mention>,
    pub linked_record: Option<LinkedRecord>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommentSnapshot {
    pub id: String,
    pub post_id: String,
    pub author_id: String,
    pub state: CommentState,
    pub version: i64,
    pub body: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Receipt {
    pub id: String,
    pub version: i64,
    pub status: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Change {
    PublishPost {
        body: String,
        audience: Audience,
        tags: Vec<String>,
        mentions: Vec<MentionInput>,
        linked_record: Option<LinkedRecordInput>,
    },
    PublishComment {
        post_id: String,
        body: String,
        parent_comment_id: Option<String>,
        mentions: Vec<MentionInput>,
    },
    EditComment {
        post_id: String,
        id: String,
        expected_version: i64,
        body: String,
        mentions: Vec<MentionInput>,
    },
    TombstoneComment {
        post_id: String,
        id: String,
        expected_version: i64,
    },
    SetReaction {
        post_id: String,
        active: bool,
    },
    SetFollow {
        target: FollowTarget,
        target_id: String,
        active: bool,
    },
    ReportPost {
        post_id: String,
        reason: String,
    },
    PublishDraft {
        id: String,
        version: i64,
    },
    EditPost {
        id: String,
        expected_version: i64,
        body: String,
        mentions: Vec<MentionInput>,
        content_json: Option<String>,
        rich_content_json: Option<String>,
    },
    RetractPost {
        id: String,
        expected_version: i64,
    },
    CommentUploadCreate {
        post_id: String,
    },
    CommentUploadCancel {
        id: String,
    },
    PostEditUploadCreate {
        post_id: String,
        expected_version: i64,
    },
    PostEditUploadCancel {
        id: String,
    },
    SavePostDraft {
        id: Option<String>,
        body: String,
        audience_kind: Option<String>,
        audience_id: Option<String>,
        participants: Vec<String>,
        sources_json: String,
        purpose: String,
        mentions: Vec<MentionInput>,
        content_json: Option<String>,
        rich_content_json: Option<String>,
        expected_version: i64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OwnerPlan {
    CreatePost {
        id: String,
        author_name: String,
        company_id: String,
        branch_id: Option<String>,
        department_id: Option<String>,
        audience: Audience,
        body: String,
        tags: Vec<String>,
        mentions: Vec<Mention>,
        linked_record: Option<LinkedRecord>,
    },
    CreateComment {
        id: String,
        post_id: String,
        company_id: String,
        author_name: String,
        body: String,
        tags: Vec<String>,
        mentions: Vec<Mention>,
        parent_comment_id: Option<String>,
    },
    UpdateComment {
        id: String,
        post_id: String,
        expected_version: i64,
        body: String,
        tags: Vec<String>,
        mentions: Vec<Mention>,
    },
    TombstoneComment {
        id: String,
        post_id: String,
        expected_version: i64,
    },
    SetReaction {
        post_id: String,
        version: i64,
        active: bool,
    },
    SetFollow {
        target: FollowTarget,
        target_id: String,
        version: i64,
        active: bool,
    },
    ReportPost {
        id: String,
        post_id: String,
        reporter_id: String,
        reason: String,
    },
    EditPost {
        id: String,
        expected_version: i64,
        body: String,
        content_json: Option<String>,
        rich_content_json: Option<String>,
        mentions_json: String,
        mentions: Vec<Mention>,
    },
    RetractPost {
        id: String,
        expected_version: i64,
    },
    UploadDraftCreate {
        id: String,
        author_id: String,
        company_id: String,
        branch_id: Option<String>,
        comment_parent_post_id: Option<String>,
        edit_post_id: Option<String>,
        edit_post_version: Option<i64>,
    },
    UploadDraftCancel {
        id: String,
        version: i64,
    },
    PublishDraftPost {
        draft_id: String,
        draft_version: i64,
        post_id: String,
        author_id: String,
        author_name: String,
        company_id: String,
        branch_id: Option<String>,
        department_id: Option<String>,
        body: String,
        audience_kind: String,
        audience_id: Option<String>,
        participants: Vec<String>,
        sources: Vec<PostSource>,
        mentions: Vec<Mention>,
        content_json: Option<String>,
        rich_content_json: Option<String>,
        purpose: String,
    },
    SaveDraft {
        id: String,
        author_id: String,
        company_id: String,
        branch_id: Option<String>,
        department_id: Option<String>,
        body: String,
        version: i64,
        audience_kind: Option<String>,
        audience_id: Option<String>,
        participants_json: String,
        sources_json: String,
        purpose: String,
        mentions_json: String,
        content_json: Option<String>,
        rich_content_json: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftSnapshot {
    pub id: String,
    pub author_id: String,
    pub company_id: String,
    pub version: i64,
    pub published_post_id: Option<String>,
    pub body: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub audience_kind: Option<String>,
    pub audience_id: Option<String>,
    pub participants_json: String,
    pub sources_json: String,
    pub purpose: String,
    pub mentions_json: String,
    pub content_json: Option<String>,
    pub rich_content_json: Option<String>,
    pub cancelled_at: Option<i64>,
    pub comment_parent_post_id: Option<String>,
    pub published_comment_id: Option<String>,
    pub edit_post_id: Option<String>,
    pub edit_post_version: Option<i64>,
    pub edit_saved_version: Option<i64>,
}

fn invalid() -> BusinessError {
    BusinessError::invalid("SOCIAL_INPUT_INVALID", "Provide a valid collaboration value.")
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SOCIAL_SOURCE_CHANGED",
        "The conversation changed. Refresh and retry.",
    )
}

pub fn post_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "POST_NOT_FOUND",
        "The update is unavailable in this work scope.",
    )
}

pub fn draft_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "DRAFT_NOT_FOUND",
        "The draft is unavailable in this work scope.",
    )
}

pub fn comment_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "COMMENT_NOT_FOUND",
        "The comment is unavailable in this work scope.",
    )
}

pub fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

pub fn text(value: &str, min: usize, max: usize) -> Result<String, BusinessError> {
    let value = js_trim(value);
    let length = utf16_len(value);
    if length < min || length > max {
        return Err(invalid());
    }
    Ok(value.into())
}

pub fn validate_expected_version(value: i64) -> Result<(), BusinessError> {
    if !(1..=MAX_SAFE_VERSION).contains(&value) {
        return Err(invalid());
    }
    Ok(())
}

fn tag_valid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 40 {
        return false;
    }
    (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-'))
}

pub fn normalize_tag(value: &str) -> Result<String, BusinessError> {
    let value = js_trim(value).to_ascii_lowercase();
    if !tag_valid(&value) {
        return Err(invalid());
    }
    Ok(value)
}

/// Hashtags follow the original `#([a-z0-9][a-z0-9_-]{0,39})` matcher.
pub fn hashtags(body: &str) -> Vec<String> {
    let bytes = body.as_bytes();
    let mut tags = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'#' {
            index += 1;
            continue;
        }
        let start = index + 1;
        let mut end = start;
        while end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'-'))
            && end - start < 40
        {
            end += 1;
        }
        if end > start && (bytes[start].is_ascii_alphanumeric()) {
            let value = std::str::from_utf8(&bytes[start..end]).unwrap_or("");
            let value = value.to_ascii_lowercase();
            if tag_valid(&value) {
                tags.push(value);
            }
        }
        index = end.max(start);
    }
    tags
}

pub fn post_tags(explicit: &[String], body: &str) -> Result<Vec<String>, BusinessError> {
    let mut tags = Vec::new();
    for tag in explicit {
        let tag = normalize_tag(tag)?;
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    for tag in hashtags(body) {
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags.truncate(MAX_TAGS);
    Ok(tags)
}

pub fn normalize_tags(tags: &[String]) -> Result<Vec<String>, BusinessError> {
    let mut normalized = Vec::new();
    for tag in tags {
        let tag = normalize_tag(tag)?;
        if !normalized.contains(&tag) {
            normalized.push(tag);
        }
    }
    normalized.truncate(MAX_TAGS);
    Ok(normalized)
}

pub fn validate_mentions(mentions: &[MentionInput]) -> Result<Vec<MentionInput>, BusinessError> {
    if mentions.len() > MAX_MENTIONS {
        return Err(invalid());
    }
    let mut unique: Vec<MentionInput> = Vec::new();
    for mention in mentions {
        if mention.target_id.is_empty() || utf16_len(&mention.target_id) > 260 {
            return Err(invalid());
        }
        let key = format!("{}:{}", mention.kind.as_str(), mention.target_id);
        if !unique
            .iter()
            .any(|item| format!("{}:{}", item.kind.as_str(), item.target_id) == key)
        {
            unique.push(mention.clone());
        }
    }
    Ok(unique)
}

pub fn require_audience(
    actor: &SocialActor,
    audience: Audience,
) -> Result<Option<String>, BusinessError> {
    match audience {
        Audience::Holding if actor.can_share_company => Ok(None),
        Audience::Branch => actor.branch_id.clone().map(Some).ok_or_else(|| {
            BusinessError::new(
                FailureKind::Forbidden,
                "AUDIENCE_FORBIDDEN",
                "Choose a current branch assignment to share with that branch.",
            )
        }),
        Audience::Holding => Err(BusinessError::new(
            FailureKind::Forbidden,
            "AUDIENCE_FORBIDDEN",
            "Your current role cannot share with the whole company.",
        )),
    }
}

pub fn can_read_post(actor: &SocialActor, post: &PostSnapshot) -> bool {
    post.company_id == actor.company_id
        && match post.audience {
            Audience::Holding => true,
            Audience::Branch => {
                post.branch_id == actor.branch_id
                    || post.department_id
                        .as_deref()
                        .zip(actor.department_id.as_deref())
                        .is_some_and(|(post, actor)| post == actor)
                    || (post.branch_id.is_none()
                        && post.department_id.is_none()
                        && post.author_id == actor.user_id)
            }
        }
}

pub fn require_published_post(actor: &SocialActor, post: &PostSnapshot) -> Result<(), BusinessError> {
    if post.state != PostState::Published || !can_read_post(actor, post) {
        return Err(post_not_found());
    }
    Ok(())
}

/// Author edit is the original person comment only; agents cannot be edited as
/// a person by invoking a command.
pub fn require_comment_edit(
    actor: &SocialActor,
    comment: &CommentSnapshot,
) -> Result<(), BusinessError> {
    if comment.state != CommentState::Published || comment.author_id != actor.user_id {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COMMENT_AUTHOR_REQUIRED",
            "Only the current author can edit this comment.",
        ));
    }
    Ok(())
}

pub fn require_comment_removal(
    actor: &SocialActor,
    comment: &CommentSnapshot,
) -> Result<(), BusinessError> {
    if comment.state != CommentState::Published {
        return Err(comment_not_found());
    }
    if comment.author_id != actor.user_id && !actor.can_moderate {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COMMENT_AUTHOR_REQUIRED",
            "Only the author or a reviewer can remove this comment.",
        ));
    }
    Ok(())
}

/// Exact owner result prepared before the Workflow ledger writes a receipt.
pub fn plan_receipt(_key: &CommandKey, plan: &OwnerPlan) -> Result<Receipt, BusinessError> {
    Ok(match plan {
        OwnerPlan::CreatePost { id, .. } => Receipt {
            id: id.clone(),
            version: 1,
            status: "published",
        },
        OwnerPlan::CreateComment { id, .. } => Receipt {
            id: id.clone(),
            version: 1,
            status: "published",
        },
        OwnerPlan::UpdateComment {
            id,
            expected_version,
            ..
        } => Receipt {
            id: id.clone(),
            version: next_version(*expected_version)?,
            status: "published",
        },
        OwnerPlan::TombstoneComment {
            id,
            expected_version,
            ..
        } => Receipt {
            id: id.clone(),
            version: next_version(*expected_version)?,
            status: "tombstoned",
        },
        OwnerPlan::SetReaction {
            post_id,
            version,
            active,
        } => Receipt {
            id: post_id.clone(),
            version: *version,
            status: if *active { "active" } else { "inactive" },
        },
        OwnerPlan::SetFollow {
            target_id,
            version,
            active,
            ..
        } => Receipt {
            id: target_id.clone(),
            version: *version,
            status: if *active { "followed" } else { "unfollowed" },
        },
        OwnerPlan::ReportPost { post_id, .. } => Receipt {
            id: post_id.clone(),
            version: 1,
            status: "reported",
        },
        OwnerPlan::SaveDraft { id, version, .. } => Receipt {
            id: id.clone(),
            version: *version,
            status: "draft",
        },
        OwnerPlan::PublishDraftPost { post_id, .. } => Receipt {
            id: post_id.clone(),
            version: 1,
            status: "published",
        },
        OwnerPlan::EditPost {
            id,
            expected_version,
            ..
        } => Receipt {
            id: id.clone(),
            version: next_version(*expected_version)?,
            status: "published",
        },
        OwnerPlan::RetractPost {
            id,
            expected_version,
        } => Receipt {
            id: id.clone(),
            version: next_version(*expected_version)?,
            status: "archived",
        },
        OwnerPlan::UploadDraftCreate { id, .. } => Receipt {
            id: id.clone(),
            version: 1,
            status: "draft",
        },
        OwnerPlan::UploadDraftCancel { id, version } => Receipt {
            id: id.clone(),
            version: *version,
            status: "cancelled",
        },
    })
}

fn next_version(version: i64) -> Result<i64, BusinessError> {
    version
        .checked_add(1)
        .filter(|next| version > 0 && *next <= MAX_SAFE_VERSION)
        .ok_or_else(changed)
}

/// Cursor is an opaque `created_at:id` pair. Command-derived ids contain only
/// ASCII alphanumerics/`:_.-`, so the first colon is unambiguous.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeedCursor {
    pub at: i64,
    pub id: String,
}
impl FeedCursor {
    pub fn encode(&self) -> String {
        format!("{}:{}", self.at, self.id)
    }
    pub fn decode(value: &str) -> Result<Self, BusinessError> {
        let (at, id) = value.split_once(':').ok_or_else(cursor_invalid)?;
        let at = at.parse::<i64>().map_err(|_| cursor_invalid())?;
        if !(0..=MAX_SAFE_VERSION).contains(&at) || id.is_empty() || id.len() > 512 {
            return Err(cursor_invalid());
        }
        Ok(Self { at, id: id.into() })
    }
}
fn cursor_invalid() -> BusinessError {
    BusinessError::invalid("INVALID_CURSOR", "Restart the conversation feed.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_and_hashtags_follow_the_original_normalization() {
        assert_eq!(text("  Hello  ", 1, 10).unwrap(), "Hello");
        assert_eq!(
            post_tags(&["Policy".into(), "policy".into()], "Hello #Bangkok #ops_1")
                .unwrap(),
            vec!["policy".to_owned(), "bangkok".to_owned(), "ops_1".to_owned()]
        );
        assert_eq!(hashtags("#one #two-three #四").len(), 2);
        assert_eq!(
            normalize_tags(&hashtags("  Reply #Ops  ")).unwrap(),
            vec!["ops".to_owned()]
        );
        assert!(post_tags(&["bad tag".into()], "body").is_err());
        assert!(text("", 1, 10).is_err());
    }

    #[test]
    fn branch_and_company_audience_are_denied_without_current_scope() {
        let mut actor = SocialActor {
            user_id: "user".into(),
            display_name: "User".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            can_share_company: false,
            can_moderate: false,
        };
        assert_eq!(
            require_audience(&actor, Audience::Holding)
                .unwrap_err()
                .code,
            "AUDIENCE_FORBIDDEN"
        );
        assert_eq!(
            require_audience(&actor, Audience::Branch).unwrap_err().code,
            "AUDIENCE_FORBIDDEN"
        );
        actor.can_share_company = true;
        actor.branch_id = Some("branch".into());
        assert_eq!(require_audience(&actor, Audience::Holding).unwrap(), None);
        assert_eq!(
            require_audience(&actor, Audience::Branch).unwrap(),
            Some("branch".into())
        );
    }

    #[test]
    fn branch_posts_are_readable_only_in_the_same_scope() {
        let actor = SocialActor {
            user_id: "user".into(),
            display_name: "User".into(),
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: None,
            can_share_company: false,
            can_moderate: false,
        };
        let post = PostSnapshot {
            id: "post".into(),
            author_id: "other".into(),
            author_name: "Other".into(),
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: None,
            audience: Audience::Branch,
            body: "Body".into(),
            version: 1,
            state: PostState::Published,
            topics: vec![],
            mentions: vec![],
            linked_record: None,
            created_at: 1,
        };
        assert!(can_read_post(&actor, &post));
        let mut other = post.clone();
        other.branch_id = Some("other".into());
        assert!(!can_read_post(&actor, &other));
        other.branch_id = None;
        assert!(!can_read_post(&actor, &other));
        other.author_id = "user".into();
        assert!(can_read_post(&actor, &other));
    }

    #[test]
    fn comment_edit_and_removal_require_current_author_or_reviewer() {
        let actor = SocialActor {
            user_id: "user".into(),
            display_name: "User".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            can_share_company: true,
            can_moderate: false,
        };
        let mut comment = CommentSnapshot {
            id: "comment".into(),
            post_id: "post".into(),
            author_id: "user".into(),
            state: CommentState::Published,
            version: 1,
            body: "Body".into(),
            created_at: 1,
        };
        assert!(require_comment_edit(&actor, &comment).is_ok());
        assert!(require_comment_removal(&actor, &comment).is_ok());
        comment.author_id = "other".into();
        assert_eq!(
            require_comment_edit(&actor, &comment).unwrap_err().code,
            "COMMENT_AUTHOR_REQUIRED"
        );
        assert_eq!(
            require_comment_removal(&actor, &comment).unwrap_err().code,
            "COMMENT_AUTHOR_REQUIRED"
        );
        let mut reviewer = actor.clone();
        reviewer.can_moderate = true;
        assert!(require_comment_removal(&reviewer, &comment).is_ok());
        assert!(require_comment_edit(&reviewer, &comment).is_err());
        comment.state = CommentState::Tombstoned;
        assert_eq!(
            require_comment_removal(&reviewer, &comment).unwrap_err().code,
            "COMMENT_NOT_FOUND"
        );
    }

    #[test]
    fn receipts_and_cursor_are_bounded() {
        let key = CommandKey::new("social-command").unwrap();
        let plan = OwnerPlan::TombstoneComment {
            id: "comment-1".into(),
            post_id: "post-1".into(),
            expected_version: 3,
        };
        assert_eq!(
            plan_receipt(&key, &plan).unwrap(),
            Receipt {
                id: "comment-1".into(),
                version: 4,
                status: "tombstoned"
            }
        );
        let cursor = FeedCursor {
            at: 1_700_000_000_000,
            id: "post-1".into(),
        };
        assert_eq!(FeedCursor::decode(&cursor.encode()).unwrap(), cursor);
        for value in ["", "x", "-1:post", "1:"] {
            assert_eq!(
                FeedCursor::decode(value).unwrap_err().code,
                "INVALID_CURSOR"
            );
        }
    }
}
