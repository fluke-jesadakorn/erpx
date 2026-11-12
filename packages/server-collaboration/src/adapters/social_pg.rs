//! Production owner unit for Social commands and reads.
//!
//! Composition supplies the selected Work scope and a record resolver that
//! delegates to Records and the owning module. This module verifies the current
//! assignment/principal on the caller's serializable connection, reads People
//! labels, stages the exact Social rows plus receipt/audit/outbox, and never
//! commits, retries or opens a new transaction.
use crate::adapters::command_ledger::{self, Error, fingerprint, source_changed};
use crate::adapters::social::{
    CurrentSocialEffects, FeedComment, FeedFilter, FeedPost, apply_plan, read_comment,
    read_comments, read_feed, read_post, read_thread, retain_effects,
};
use crate::application::social::{Access, SocialUnit};
use crate::domain::social::{
    Change, CommentSnapshot, DraftSnapshot, FeedCursor, FollowTarget, OwnerPlan, PostSnapshot,
    Receipt, SocialActor, comment_not_found, post_not_found,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, UserId,
};
use serde_json::Value;
use sqlx::{PgConnection, Row};
use std::future::Future;
use std::pin::Pin;

type Unit = (String, i32, String, String);

pub type SocialRecordFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<String>, BusinessError>> + Send + 'a>>;

/// Cross-owner record label port. Composition binds it to Records and the
/// module owner; an absent port must fail rather than invent a mention label.
pub trait SocialRecordPort: Send {
    fn resolve_record<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        module: &'a str,
        id: &'a str,
    ) -> SocialRecordFuture<'a>;
}

/// Fail-closed resolver used when composition has not supplied a Records port.
pub struct UnavailableSocialRecords;

impl SocialRecordPort for UnavailableSocialRecords {
    fn resolve_record<'a>(
        &'a mut self,
        _connection: &'a mut PgConnection,
        _module: &'a str,
        _id: &'a str,
    ) -> SocialRecordFuture<'a> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "SOCIAL_RECORD_PORT_UNAVAILABLE",
                "This record cannot be resolved by the native Social owner.",
            ))
        })
    }
}

/// Selected Work facts supplied by composition. The unit revalidates the
/// assignment and principal on every call; the boolean policy flags remain the
/// composition's current effective-IAM result.
#[derive(Clone, Debug)]
pub struct SocialScope {
    pub user_id: UserId,
    pub company_id: CompanyId,
    pub assignment_id: AssignmentId,
    pub assignment_version: i64,
    pub display_name: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub can_share_company: bool,
    pub can_moderate: bool,
    pub now: i64,
}

impl SocialScope {
    pub fn new(
        company_id: impl Into<String>,
        user_id: impl Into<String>,
        assignment_id: impl Into<String>,
        display_name: impl Into<String>,
        now: i64,
    ) -> Result<Self, BusinessError> {
        let display_name = display_name.into();
        let user_id = UserId::new(user_id)?;
        Ok(Self {
            display_name: if display_name.trim().is_empty() {
                user_id.as_str().to_owned()
            } else {
                display_name
            },
            user_id,
            company_id: CompanyId::new(company_id)?,
            assignment_id: AssignmentId::new(assignment_id)?,
            assignment_version: 0,
            branch_id: None,
            department_id: None,
            can_share_company: false,
            can_moderate: false,
            now,
        })
    }

    pub fn branch(mut self, id: Option<&str>) -> Result<Self, BusinessError> {
        self.branch_id = id.filter(|value| !value.is_empty()).map(str::to_owned);
        Ok(self)
    }

    pub fn department(mut self, id: Option<&str>) -> Result<Self, BusinessError> {
        self.department_id = id.filter(|value| !value.is_empty()).map(str::to_owned);
        Ok(self)
    }

    pub fn assignment_version(mut self, version: i64) -> Self {
        self.assignment_version = version;
        self
    }

    pub fn share_company(mut self, allowed: bool) -> Self {
        self.can_share_company = allowed;
        self
    }

    pub fn moderate(mut self, allowed: bool) -> Self {
        self.can_moderate = allowed;
        self
    }

    pub fn actor(&self) -> SocialActor {
        SocialActor {
            user_id: self.user_id.as_str().to_owned(),
            display_name: self.display_name.clone(),
            company_id: self.company_id.as_str().to_owned(),
            branch_id: self.branch_id.clone(),
            department_id: self.department_id.clone(),
            can_share_company: self.can_share_company,
            can_moderate: self.can_moderate,
        }
    }
}

pub struct PgSocialUnit<'c> {
    connection: &'c mut PgConnection,
    scope: SocialScope,
    records: &'c mut dyn SocialRecordPort,
    key: CommandKey,
    actor: Option<SocialActor>,
    current: Option<CurrentSocialEffects>,
    unit: Option<Unit>,
    change: Option<Change>,
    request_fingerprint: Option<String>,
    receipt: Option<Receipt>,
    before_apply: bool,
}

impl<'c> PgSocialUnit<'c> {
    /// The caller must already hold its serializable transaction, all common
    /// IAM/People/Records source locks and
    /// [`crate::adapters::social::lock_social_sources`].
    pub fn new(
        connection: &'c mut PgConnection,
        scope: SocialScope,
        key: CommandKey,
        records: &'c mut dyn SocialRecordPort,
    ) -> Self {
        Self {
            connection,
            scope,
            records,
            key,
            actor: None,
            current: None,
            unit: None,
            change: None,
            request_fingerprint: None,
            receipt: None,
            before_apply: true,
        }
    }

    /// Read-only composition convenience. No receipt or ledger row is written.
    pub fn read_only(
        connection: &'c mut PgConnection,
        scope: SocialScope,
        records: &'c mut dyn SocialRecordPort,
    ) -> Self {
        Self::new(
            connection,
            scope,
            CommandKey::new("social-read").expect("static read key"),
            records,
        )
    }

    async fn unit(&mut self) -> Result<Unit, Error> {
        command_ledger::require_serializable(self.connection, "SOCIAL_SERIALIZABLE_REQUIRED")
            .await?;
        if let Some(unit) = &self.unit {
            return Ok(unit.clone());
        }
        let unit = sqlx::query_as::<_, Unit>(
            "SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text",
        )
        .fetch_one(&mut *self.connection)
        .await?;
        self.unit = Some(unit.clone());
        Ok(unit)
    }

    async fn verify_authority(&mut self) -> Result<SocialActor, Error> {
        self.unit().await?;
        let row = sqlx::query(
            "SELECT a.version FROM assignments a \
             WHERE a.id=$1 AND a.company_id=$2 AND a.user_id=$3 AND a.revoked_at IS NULL \
             AND a.valid_from<=$4 AND a.valid_until>$4 \
             AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4)",
        )
        .bind(self.scope.assignment_id.as_str())
        .bind(self.scope.company_id.as_str())
        .bind(self.scope.user_id.as_str())
        .bind(self.scope.now)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MEMBERSHIP_REQUIRED",
                "Current Company membership is required.",
            )
            .into());
        };
        let version: i64 = row.try_get(0)?;
        if self.scope.assignment_version > 0 && version != self.scope.assignment_version {
            return Err(source_changed(
                "SOCIAL_SOURCE_CHANGED",
                "Your selected assignment changed. Refresh and retry.",
            ));
        }
        let actor = self.scope.actor();
        self.actor = Some(actor.clone());
        Ok(actor)
    }

    fn operation(change: &Change) -> &'static str {
        match change {
            Change::PublishPost { .. } => "social.post.publish",
            Change::PublishComment { .. } => "social.comment.publish",
            Change::EditComment { .. } => "social.comment.edit",
            Change::TombstoneComment { .. } => "social.comment.remove",
            Change::SetReaction { .. } => "social.post.appreciate",
            Change::SetFollow {
                target: FollowTarget::Post,
                ..
            } => "social.post.follow",
            Change::SetFollow {
                target: FollowTarget::Topic,
                ..
            } => "social.topic.follow",
            Change::ReportPost { .. } => "social.post.report",
            Change::SavePostDraft { .. } => "social.draft.save",
        }
    }

    fn record_id(key: &CommandKey, change: &Change) -> Result<String, Error> {
        Ok(match change {
            Change::PublishPost { .. } => format!("post-{}", key.as_str()),
            Change::PublishComment { .. } => format!("comment-{}", key.as_str()),
            Change::EditComment { id, .. } | Change::TombstoneComment { id, .. } => id.clone(),
            Change::SetReaction { post_id, .. } => post_id.clone(),
            Change::SetFollow {
                target: FollowTarget::Post,
                target_id,
                ..
            } => target_id.clone(),
            Change::SetFollow {
                target: FollowTarget::Topic,
                target_id,
                ..
            } => crate::domain::social::normalize_tag(target_id)?,
            Change::ReportPost { post_id, .. } => post_id.clone(),
            Change::SavePostDraft { id, .. } => id
                .clone()
                .unwrap_or_else(|| format!("post-draft-{}", key.as_str())),
        })
    }

    /// Existing Company feed. The caller must have used the same transaction.
    pub async fn feed(
        &mut self,
        filter: FeedFilter,
        cursor: Option<&FeedCursor>,
        limit: usize,
    ) -> Result<Vec<FeedPost>, Error> {
        let actor = self.verify_authority().await?;
        read_feed(&mut *self.connection, &actor, filter, cursor, limit).await
    }

    /// Feed page plus the exact `has_more` flag expected by `SocialFeedData`.
    pub async fn feed_page(
        &mut self,
        filter: FeedFilter,
        cursor: Option<&FeedCursor>,
        limit: usize,
    ) -> Result<(Vec<FeedPost>, bool), Error> {
        let actor = self.verify_authority().await?;
        crate::adapters::social::read_feed_page(
            &mut *self.connection,
            &actor,
            filter,
            cursor,
            limit,
        )
        .await
    }

    pub async fn thread(&mut self, post_id: &str) -> Result<Vec<FeedPost>, Error> {
        let actor = self.verify_authority().await?;
        read_thread(&mut *self.connection, &actor, post_id).await
    }

    pub async fn comments(
        &mut self,
        post_id: &str,
        limit: usize,
    ) -> Result<Vec<FeedComment>, Error> {
        self.verify_authority().await?;
        read_comments(&mut *self.connection, post_id, limit).await
    }

    pub async fn post(&mut self, id: &str) -> Result<Option<PostSnapshot>, Error> {
        read_post(&mut *self.connection, &self.scope.company_id, id).await
    }
}

fn decode_receipt(value: &Value) -> Result<Receipt, Error> {
    let status = match value["status"].as_str() {
        Some("published") => "published",
        Some("active") => "active",
        Some("inactive") => "inactive",
        Some("followed") => "followed",
        Some("unfollowed") => "unfollowed",
        Some("tombstoned") => "tombstoned",
        _ => {
            return Err(source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The stored Social receipt has an unknown status.",
            ));
        }
    };
    let id = value["id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The stored Social receipt has no identifier.",
            )
        })?;
    let version = value["version"]
        .as_i64()
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The stored Social receipt has no version.",
            )
        })?;
    Ok(Receipt {
        id: id.into(),
        version,
        status,
    })
}

impl SocialUnit for PgSocialUnit<'_> {
    type Error = Error;

    async fn authorize(&mut self, _access: Access) -> Result<SocialActor, Error> {
        if let Some(actor) = &self.actor {
            return Ok(actor.clone());
        }
        self.verify_authority().await
    }

    async fn replay(&mut self, change: &Change) -> Result<Option<Receipt>, Error> {
        let actor = self.verify_authority().await?;
        let operation = Self::operation(change);
        let record_id = Self::record_id(&self.key, change)?;
        let request = fingerprint(operation, change);
        let value = command_ledger::replay(
            &mut *self.connection,
            self.key.as_str(),
            operation,
            actor.user_id.as_str(),
            &record_id,
            &request,
        )
        .await?;
        self.change = Some(change.clone());
        self.request_fingerprint = Some(request);
        self.before_apply = true;
        match value {
            Some(value) => {
                let receipt = decode_receipt(&value)?;
                self.receipt = Some(receipt.clone());
                Ok(Some(receipt))
            }
            None => {
                self.receipt = None;
                Ok(None)
            }
        }
    }

    async fn post(&mut self, id: &str) -> Result<PostSnapshot, Error> {
        self.post(id).await?.ok_or_else(|| post_not_found().into())
    }

    async fn draft(&mut self, id: &str) -> Result<Option<DraftSnapshot>, Error> {
        let row = sqlx::query(
            "SELECT id,author_id,company_id,version,published_post_id FROM social_post_drafts WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(DraftSnapshot {
                id: row.try_get(0)?,
                author_id: row.try_get(1)?,
                company_id: row.try_get(2)?,
                version: row.try_get(3)?,
                published_post_id: row.try_get(4)?,
            })
        })
        .transpose()
    }
    async fn comment(&mut self, id: &str, post_id: &str) -> Result<CommentSnapshot, Error> {
        read_comment(&mut *self.connection, id, post_id)
            .await?
            .ok_or_else(|| comment_not_found().into())
    }

    async fn resolve_person(&mut self, id: &str) -> Result<Option<String>, Error> {
        Ok(sqlx::query_scalar(
            "SELECT COALESCE(NULLIF(p.preferred_name,''),p.legal_name) FROM hr_people p \
             WHERE p.company_id=$1 AND p.user_id=$2 AND p.status='active' AND p.merged_into_id IS NULL \
             AND EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=p.company_id AND a.user_id=p.user_id \
               AND a.revoked_at IS NULL AND a.valid_from<=$3 AND a.valid_until>$3 \
               AND iam_assignment_active(a.id,$3) AND iam_principal_active(a.user_id,a.company_id,$3)) LIMIT 1",
        )
        .bind(self.scope.company_id.as_str())
        .bind(id)
        .bind(self.scope.now)
        .fetch_optional(&mut *self.connection)
        .await?)
    }

    async fn resolve_record(&mut self, module: &str, id: &str) -> Result<Option<String>, Error> {
        self.records
            .resolve_record(&mut *self.connection, module, id)
            .await
            .map_err(Into::into)
    }

    async fn apply(
        &mut self,
        actor: &SocialActor,
        key: &CommandKey,
        plan: OwnerPlan,
    ) -> Result<Receipt, Error> {
        self.verify_authority().await?;
        if actor.company_id != self.scope.company_id.as_str()
            || actor.user_id != self.scope.user_id.as_str()
        {
            return Err(source_changed(
                "SOCIAL_SOURCE_CHANGED",
                "The Social actor changed inside the retained unit.",
            ));
        }
        if key.as_str() != self.key.as_str() {
            return Err(source_changed(
                "SOCIAL_SOURCE_CHANGED",
                "The Social command key changed inside the retained unit.",
            ));
        }
        let request_fingerprint = self.request_fingerprint.clone().ok_or_else(|| {
            source_changed(
                "SOCIAL_REPLAY_REQUIRED",
                "The Social command must resolve its receipt before applying.",
            )
        })?;
        let receipt = apply_plan(&mut *self.connection, actor, key, &plan, self.scope.now).await?;
        let effects = retain_effects(&mut *self.connection, &plan, &receipt).await?;

        let (record_type, purpose, event_type) = match &plan {
            OwnerPlan::CreatePost { .. } => (
                "social_post",
                "Publish workplace update",
                "social.post.published",
            ),
            OwnerPlan::CreateComment { .. } => (
                "social_comment",
                "Comment on workplace update",
                "social.comment.published",
            ),
            OwnerPlan::UpdateComment { .. } => (
                "social_comment",
                "Edit workplace comment",
                "social.comment.edited",
            ),
            OwnerPlan::TombstoneComment { .. } => (
                "social_comment",
                "Remove workplace comment",
                "social.comment.removed",
            ),
            OwnerPlan::SetReaction { active, .. } => (
                "social_post",
                if *active {
                    "Appreciate workplace update"
                } else {
                    "Remove appreciation"
                },
                if *active {
                    "social.post.appreciated"
                } else {
                    "social.post.unappreciated"
                },
            ),
            OwnerPlan::SetFollow { target, active, .. } => (
                if *target == FollowTarget::Topic {
                    "social_topic"
                } else {
                    "social_post"
                },
                if *active {
                    "Follow workplace conversation"
                } else {
                    "Unfollow workplace conversation"
                },
                match (*target, *active) {
                    (FollowTarget::Topic, true) => "social.topic.followed",
                    (FollowTarget::Topic, false) => "social.topic.unfollowed",
                    (FollowTarget::Post, true) => "social.post.followed",
                    (FollowTarget::Post, false) => "social.post.unfollowed",
                },
            ),
            OwnerPlan::ReportPost { .. } => (
                "social_post",
                "Report workplace update",
                "social.post.reported",
            ),
            OwnerPlan::SaveDraft { .. } => (
                "social_post",
                "Save workplace draft",
                "social.draft.saved",
            ),
        };
        let value = serde_json::to_value(&receipt).map_err(|_| {
            source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The Social result could not be encoded.",
            )
        })?;
        command_ledger::write_ledger(
            &mut *self.connection,
            command_ledger::LedgerRows {
                key: key.as_str(),
                operation: Self::operation(self.change.as_ref().ok_or_else(|| {
                    source_changed(
                        "SOCIAL_REPLAY_REQUIRED",
                        "The Social command lost its retained change.",
                    )
                })?),
                actor: actor.user_id.as_str(),
                assignment: self.scope.assignment_id.as_str(),
                record_id: &receipt.id,
                record_version: receipt.version,
                fingerprint: &request_fingerprint,
                result: &value,
                record_type,
                purpose,
                event_type,
                now: self.scope.now,
            },
        )
        .await?;
        self.current = Some(effects);
        self.actor = Some(actor.clone());
        self.receipt = Some(receipt.clone());
        self.before_apply = false;
        Ok(receipt)
    }

    async fn flush_deferred(&mut self) -> Result<(), Error> {
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn require_unit_current(&mut self) -> Result<(), Error> {
        self.actor = None;
        self.verify_authority().await?;
        if let Some(current) = &self.current {
            current.require_current(&mut *self.connection).await?;
        }
        if self.before_apply {
            return Ok(());
        }
        let change = self.change.as_ref().ok_or_else(|| {
            source_changed(
                "SOCIAL_SOURCE_CHANGED",
                "The Social command lost its retained change.",
            )
        })?;
        let operation = Self::operation(change);
        let record_id = Self::record_id(&self.key, change)?;
        let request = self.request_fingerprint.as_deref().ok_or_else(|| {
            source_changed(
                "SOCIAL_SOURCE_CHANGED",
                "The Social command lost its request fingerprint.",
            )
        })?;
        let value = command_ledger::replay(
            &mut *self.connection,
            self.key.as_str(),
            operation,
            self.scope.user_id.as_str(),
            &record_id,
            request,
        )
        .await?
        .ok_or_else(|| {
            source_changed("SOCIAL_SOURCE_CHANGED", "The Social receipt disappeared.")
        })?;
        if let Some(expected) = &self.receipt {
            if decode_receipt(&value)? != *expected {
                return Err(source_changed(
                    "SOCIAL_SOURCE_CHANGED",
                    "The Social receipt changed.",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_requires_current_identifiers_and_bounds_optional_scope() {
        let scope = SocialScope::new("company", "user", "assignment", "  ", 10)
            .unwrap()
            .branch(Some("branch"))
            .unwrap()
            .department(None)
            .unwrap()
            .share_company(true)
            .moderate(true);
        let actor = scope.actor();
        assert_eq!(actor.display_name, "user");
        assert_eq!(actor.branch_id.as_deref(), Some("branch"));
        assert_eq!(actor.department_id, None);
        assert!(actor.can_share_company && actor.can_moderate);
        assert!(SocialScope::new("", "user", "assignment", "User", 10).is_err());
    }

    #[test]
    fn replay_record_ids_follow_the_command_key_for_new_rows() {
        let key = CommandKey::new("social-key-0001").unwrap();
        assert_eq!(
            PgSocialUnit::record_id(
                &key,
                &Change::PublishPost {
                    body: "Body".into(),
                    audience: crate::domain::social::Audience::Holding,
                    tags: vec![],
                    mentions: vec![],
                    linked_record: None,
                }
            )
            .unwrap(),
            "post-social-key-0001"
        );
        assert_eq!(
            PgSocialUnit::record_id(
                &key,
                &Change::TombstoneComment {
                    post_id: "post-1".into(),
                    id: "comment-1".into(),
                    expected_version: 1,
                }
            )
            .unwrap(),
            "comment-1"
        );
        assert_eq!(
            PgSocialUnit::record_id(
                &key,
                &Change::SetFollow {
                    target: crate::domain::social::FollowTarget::Topic,
                    target_id: "Policy".into(),
                    active: true,
                }
            )
            .unwrap(),
            "policy"
        );
    }
}
