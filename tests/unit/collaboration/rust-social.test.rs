//! Native Social command sequencing with a caller-owned unit. PostgreSQL
//! persistence, Company isolation and readback live in the integration target.
use erpx_server_collaboration::public::social::{
    Access, Audience, Change, CommentSnapshot, CommentState, FollowTarget, MentionInput,
    MentionKind, OwnerPlan, PostSnapshot, PostState, Receipt, SocialActor, SocialUnit, execute,
    hashtags, normalize_tag, normalize_tags, post_not_found, post_tags, text,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::collections::BTreeMap;

struct Unit {
    actor: SocialActor,
    replay: Option<Receipt>,
    posts: BTreeMap<String, PostSnapshot>,
    comments: BTreeMap<String, CommentSnapshot>,
    people: BTreeMap<String, String>,
    records: BTreeMap<String, String>,
    applied: Option<OwnerPlan>,
    drift: bool,
}
impl Unit {
    fn new() -> Self {
        let actor = SocialActor {
            user_id: "user".into(),
            display_name: "User".into(),
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: None,
            can_share_company: true,
            can_moderate: false,
        };
        let post = PostSnapshot {
            id: "post-1".into(),
            author_id: "user".into(),
            author_name: "User".into(),
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: None,
            audience: Audience::Branch,
            body: "Body".into(),
            version: 2,
            state: PostState::Published,
            topics: vec![],
            mentions: vec![],
            linked_record: None,
            created_at: 1,
        };
        let comment = CommentSnapshot {
            id: "comment-1".into(),
            post_id: "post-1".into(),
            author_id: "user".into(),
            state: CommentState::Published,
            version: 2,
            body: "Body".into(),
            created_at: 1,
        };
        Self {
            actor,
            replay: None,
            posts: BTreeMap::from([("post-1".into(), post)]),
            comments: BTreeMap::from([("comment-1".into(), comment)]),
            people: BTreeMap::from([("person-1".into(), "Person One".into())]),
            records: BTreeMap::from([(
                "people_payroll/leave-1".into(),
                "LR-1 · Leave request".into(),
            )]),
            applied: None,
            drift: false,
        }
    }
}
impl SocialUnit for Unit {
    type Error = BusinessError;
    async fn authorize(&mut self, _: Access) -> Result<SocialActor, BusinessError> {
        Ok(self.actor.clone())
    }
    async fn replay(&mut self, _: &Change) -> Result<Option<Receipt>, BusinessError> {
        Ok(self.replay.clone())
    }
    async fn post(&mut self, id: &str) -> Result<PostSnapshot, BusinessError> {
        self.posts.get(id).cloned().ok_or_else(post_not_found)
    }
    async fn comment(
        &mut self,
        id: &str,
        post_id: &str,
    ) -> Result<CommentSnapshot, BusinessError> {
        self.comments
            .get(id)
            .filter(|comment| comment.post_id == post_id)
            .cloned()
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "COMMENT_NOT_FOUND",
                    "missing",
                )
            })
    }
    async fn resolve_person(&mut self, id: &str) -> Result<Option<String>, BusinessError> {
        Ok(self.people.get(id).cloned())
    }
    async fn resolve_record(
        &mut self,
        module: &str,
        id: &str,
    ) -> Result<Option<String>, BusinessError> {
        Ok(self.records.get(&format!("{module}/{id}")).cloned())
    }
    async fn apply(
        &mut self,
        _: &SocialActor,
        key: &CommandKey,
        plan: OwnerPlan,
    ) -> Result<Receipt, BusinessError> {
        let receipt = erpx_server_collaboration::public::social::plan_receipt(key, &plan)?;
        self.applied = Some(plan);
        Ok(receipt)
    }
    async fn flush_deferred(&mut self) -> Result<(), BusinessError> {
        Ok(())
    }
    async fn require_unit_current(&mut self) -> Result<(), BusinessError> {
        if self.drift {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SOCIAL_SOURCE_CHANGED",
                "changed",
            ));
        }
        Ok(())
    }
}

fn key() -> CommandKey {
    CommandKey::new("social-command-0001").unwrap()
}

#[test]
fn public_port_exposes_bounded_topic_and_audience_values() {
    assert_eq!(
        post_tags(&["Policy".into()], "Hello #Ops").unwrap(),
        vec!["policy".to_owned(), "ops".to_owned()]
    );
    assert_eq!(normalize_tag(" Ops ").unwrap(), "ops");
    assert_eq!(normalize_tag("bad tag").unwrap_err().code, "SOCIAL_INPUT_INVALID");
    assert_eq!(
        normalize_tags(&hashtags("  Reply #Ops  ")).unwrap(),
        vec!["ops".to_owned()]
    );
    assert_eq!(text("  Hello  ", 1, 10).unwrap(), "Hello");
    assert_eq!(Audience::parse("branch").unwrap(), Audience::Branch);
    assert_eq!(FollowTarget::parse("topic").unwrap(), FollowTarget::Topic);
    assert_eq!(MentionKind::parse("person").unwrap(), MentionKind::Person);
    assert_eq!(PostState::parse("published").unwrap(), PostState::Published);
    assert_eq!(CommentState::parse("tombstoned").unwrap(), CommentState::Tombstoned);
}

#[tokio::test]
async fn publishes_a_post_with_normalized_topics_resolved_mentions_and_record() {
    let mut unit = Unit::new();
    let receipt = execute(
        &mut unit,
        &key(),
        Change::PublishPost {
            body: "  Hello #Policy  ".into(),
            audience: Audience::Holding,
            tags: vec!["Policy".into(), "ops".into()],
            mentions: vec![
                MentionInput {
                    kind: MentionKind::Person,
                    target_id: "person-1".into(),
                },
                MentionInput {
                    kind: MentionKind::Record,
                    target_id: "people_payroll/leave-1".into(),
                },
            ],
            linked_record: Some(
                erpx_server_collaboration::public::social::LinkedRecordInput {
                    module: "people_payroll".into(),
                    id: "leave-1".into(),
                },
            ),
        },
    )
    .await
    .unwrap();
    assert_eq!((receipt.version, receipt.status), (1, "published"));
    match unit.applied.unwrap() {
        OwnerPlan::CreatePost {
            branch_id,
            body,
            tags,
            mentions,
            linked_record,
            ..
        } => {
            assert_eq!(branch_id, None);
            assert_eq!(body, "Hello #Policy");
            assert_eq!(tags, vec!["policy".to_owned(), "ops".to_owned()]);
            assert_eq!(mentions[0].label, "Person One");
            assert_eq!(mentions[1].label, "LR-1 · Leave request");
            assert_eq!(linked_record.unwrap().label, "LR-1 · Leave request");
        }
        other => panic!("unexpected plan {other:?}"),
    }
}

#[tokio::test]
async fn denies_company_audience_without_current_scope_before_writing() {
    let mut unit = Unit::new();
    unit.actor.can_share_company = false;
    let error = execute(
        &mut unit,
        &key(),
        Change::PublishPost {
            body: "Hello".into(),
            audience: Audience::Holding,
            tags: vec![],
            mentions: vec![],
            linked_record: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "AUDIENCE_FORBIDDEN");
    assert!(unit.applied.is_none());
}

#[tokio::test]
async fn comment_publish_edit_and_moderation_keep_cas_and_authority() {
    let mut unit = Unit::new();
    let publish = execute(
        &mut unit,
        &key(),
        Change::PublishComment {
            post_id: "post-1".into(),
            body: "  Reply #Ops  ".into(),
            parent_comment_id: None,
            mentions: vec![],
        },
    )
    .await
    .unwrap();
    assert_eq!((publish.version, publish.status), (1, "published"));
    match unit.applied.as_ref().unwrap() {
        OwnerPlan::CreateComment { tags, .. } => {
            assert_eq!(tags, &vec!["ops".to_owned()]);
        }
        other => panic!("unexpected plan {other:?}"),
    }
    let edit = execute(
        &mut unit,
        &CommandKey::new("social-command-0002").unwrap(),
        Change::EditComment {
            post_id: "post-1".into(),
            id: "comment-1".into(),
            expected_version: 2,
            body: "Edited".into(),
            mentions: vec![],
        },
    )
    .await
    .unwrap();
    assert_eq!((edit.version, edit.status), (3, "published"));
    let stale = execute(
        &mut unit,
        &CommandKey::new("social-command-0003").unwrap(),
        Change::EditComment {
            post_id: "post-1".into(),
            id: "comment-1".into(),
            expected_version: 1,
            body: "Stale".into(),
            mentions: vec![],
        },
    )
    .await
    .unwrap_err();
    assert_eq!(stale.code, "VERSION_CONFLICT");
    unit.comments.get_mut("comment-1").unwrap().author_id = "other".into();
    let denied = execute(
        &mut unit,
        &CommandKey::new("social-command-0004").unwrap(),
        Change::TombstoneComment {
            post_id: "post-1".into(),
            id: "comment-1".into(),
            expected_version: 2,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(denied.code, "COMMENT_AUTHOR_REQUIRED");
    unit.actor.can_moderate = true;
    let moderated = execute(
        &mut unit,
        &CommandKey::new("social-command-0005").unwrap(),
        Change::TombstoneComment {
            post_id: "post-1".into(),
            id: "comment-1".into(),
            expected_version: 2,
        },
    )
    .await
    .unwrap();
    assert_eq!(moderated.status, "tombstoned");
}

#[tokio::test]
async fn reaction_and_follow_are_idempotent_flags_not_audience_grants() {
    let mut unit = Unit::new();
    let reaction = execute(
        &mut unit,
        &key(),
        Change::SetReaction {
            post_id: "post-1".into(),
            active: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(reaction.version, 2);
    assert_eq!(reaction.status, "active");
    assert!(matches!(
        unit.applied,
        Some(OwnerPlan::SetReaction { active: true, .. })
    ));
    let follow = execute(
        &mut unit,
        &CommandKey::new("social-command-0002").unwrap(),
        Change::SetFollow {
            target: FollowTarget::Topic,
            target_id: "Policy".into(),
            active: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(follow.status, "followed");
    assert!(matches!(
        unit.applied,
        Some(OwnerPlan::SetFollow {
            target_id,
            active: true,
            ..
        }) if target_id == "policy"
    ));
}

#[tokio::test]
async fn report_requires_a_published_readable_post_and_bounded_reason() {
    let mut unit = Unit::new();
    let receipt = execute(
        &mut unit,
        &key(),
        Change::ReportPost {
            post_id: "post-1".into(),
            reason: " Spam ".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!((receipt.id.as_str(), receipt.status), ("post-1", "reported"));
    match unit.applied.as_ref().unwrap() {
        OwnerPlan::ReportPost {
            id,
            post_id,
            reporter_id,
            reason,
        } => {
            assert_eq!(id, "report-social-command-0001");
            assert_eq!(post_id, "post-1");
            assert_eq!(reporter_id, "user");
            assert_eq!(reason, "Spam");
        }
        other => panic!("unexpected plan {other:?}"),
    }
    let missing = execute(
        &mut unit,
        &CommandKey::new("social-command-0002").unwrap(),
        Change::ReportPost {
            post_id: "post-x".into(),
            reason: "Spam".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(missing.code, "POST_NOT_FOUND");
    let empty = execute(
        &mut unit,
        &CommandKey::new("social-command-0003").unwrap(),
        Change::ReportPost {
            post_id: "post-1".into(),
            reason: "   ".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(empty.code, "SOCIAL_INPUT_INVALID");
}

#[tokio::test]
async fn exact_replay_skips_apply_but_still_revalidates_current_sources() {
    let mut unit = Unit::new();
    unit.replay = Some(Receipt {
        id: "post-old".into(),
        version: 1,
        status: "published",
    });
    let receipt = execute(
        &mut unit,
        &key(),
        Change::PublishPost {
            body: "Hello".into(),
            audience: Audience::Holding,
            tags: vec![],
            mentions: vec![],
            linked_record: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(receipt.id, "post-old");
    assert!(unit.applied.is_none());
    unit.drift = true;
    let error = execute(
        &mut unit,
        &CommandKey::new("social-command-0002").unwrap(),
        Change::SetReaction {
            post_id: "post-1".into(),
            active: true,
        },
    )
    .await;
    assert!(error.is_err());
}

#[tokio::test]
async fn unreadable_mention_target_fails_before_apply() {
    let mut unit = Unit::new();
    let error = execute(
        &mut unit,
        &key(),
        Change::PublishComment {
            post_id: "post-1".into(),
            body: "Hello".into(),
            parent_comment_id: None,
            mentions: vec![MentionInput {
                kind: MentionKind::Person,
                target_id: "absent".into(),
            }],
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "MENTION_TARGET_NOT_FOUND");
    assert!(unit.applied.is_none());
}
