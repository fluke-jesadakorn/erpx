use erpx_server_collaboration::public::handbook::{
    Access, ArticleSnapshot, AuthorizedHandbookActor, Category, Change, DraftContent, HandbookUnit,
    OwnerPlan, Receipt, execute,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};

struct Unit {
    steps: Vec<&'static str>,
    actor: AuthorizedHandbookActor,
    replay: bool,
    drift: bool,
    article: Option<ArticleSnapshot>,
    settings: i64,
    member: bool,
    key: CommandKey,
    applied: Option<OwnerPlan>,
}
impl Unit {
    fn new() -> Self {
        Self {
            steps: vec![],
            actor: AuthorizedHandbookActor {
                user_id: "editor".into(),
                company_id: "company".into(),
                can_read: true,
                can_edit: true,
                can_manage_editors: true,
            },
            replay: false,
            drift: false,
            article: Some(article()),
            settings: 0,
            member: true,
            key: CommandKey::new("handbook-command").unwrap(),
            applied: None,
        }
    }
}
fn article() -> ArticleSnapshot {
    ArticleSnapshot {
        id: "article-handbook-command".into(),
        version: 2,
        draft_revision: 2,
        published_revision: None,
        archived: false,
        draft: DraftContent {
            title: "Policy".into(),
            category: Category::Policy,
            body: "Body".into(),
        },
    }
}
impl HandbookUnit for Unit {
    type Error = BusinessError;
    async fn authorize(
        &mut self,
        access: Access,
    ) -> Result<AuthorizedHandbookActor, BusinessError> {
        self.steps.push("authorize");
        if !self.actor.can_read {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MEMBERSHIP_REQUIRED",
                "membership",
            ));
        }
        match access {
            Access::Read => {}
            Access::Edit if self.actor.can_edit => {}
            Access::Edit => {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "HANDBOOK_EDITOR_REQUIRED",
                    "editor",
                ));
            }
            Access::ManageEditors if self.actor.can_manage_editors => {}
            Access::ManageEditors => {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "HANDBOOK_OWNER_REQUIRED",
                    "owner",
                ));
            }
        }
        Ok(self.actor.clone())
    }
    async fn replay(&mut self, _: &Change) -> Result<Option<Receipt>, BusinessError> {
        self.steps.push("replay");
        Ok(self.replay.then(|| Receipt {
            id: "retained".into(),
            version: 1,
            revision: 1,
            status: "draft",
        }))
    }
    async fn article(&mut self, _: &str) -> Result<ArticleSnapshot, BusinessError> {
        self.steps.push("article");
        self.article.clone().ok_or_else(|| {
            BusinessError::new(FailureKind::NotFound, "ARTICLE_NOT_FOUND", "missing")
        })
    }
    async fn settings_version(&mut self) -> Result<i64, BusinessError> {
        self.steps.push("settings");
        Ok(self.settings)
    }
    async fn is_company_member(&mut self, _: &str) -> Result<bool, BusinessError> {
        self.steps.push("member");
        Ok(self.member)
    }
    async fn apply(
        &mut self,
        actor: &AuthorizedHandbookActor,
        key: &CommandKey,
        plan: OwnerPlan,
    ) -> Result<Receipt, BusinessError> {
        self.steps.push("apply");
        assert_eq!(actor.company_id, self.actor.company_id);
        assert_eq!(key, &self.key);
        let receipt = erpx_server_collaboration::public::handbook::plan_receipt(key, &plan)?;
        self.applied = Some(plan);
        Ok(receipt)
    }
    async fn flush_deferred(&mut self) -> Result<(), BusinessError> {
        self.steps.push("flush");
        Ok(())
    }
    async fn require_unit_current(&mut self) -> Result<(), BusinessError> {
        self.steps.push("authority");
        if self.drift {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "HANDBOOK_SOURCE_CHANGED",
                "changed",
            ));
        }
        Ok(())
    }
}
fn change() -> Change {
    Change::CreateArticle {
        title: "Policy".into(),
        category: Category::Policy,
        body: "Body".into(),
    }
}

#[tokio::test]
async fn create_requires_editor_before_replay_or_owner_effects() {
    let mut unit = Unit::new();
    unit.actor.can_edit = false;
    let error = execute(
        &mut unit,
        &CommandKey::new("handbook-command").unwrap(),
        change(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "HANDBOOK_EDITOR_REQUIRED");
    assert_eq!(unit.steps, ["authorize"]);
    assert!(unit.applied.is_none());
}

#[tokio::test]
async fn create_plans_the_first_revision_with_the_exact_stored_content() {
    let mut unit = Unit::new();
    let receipt = execute(
        &mut unit,
        &CommandKey::new("handbook-command").unwrap(),
        change(),
    )
    .await
    .unwrap();
    assert_eq!(
        receipt,
        Receipt {
            id: "article-handbook-command".into(),
            version: 1,
            revision: 1,
            status: "draft"
        }
    );
    assert_eq!(
        unit.applied,
        Some(OwnerPlan::CreateArticle {
            id: "article-handbook-command".into(),
            content: DraftContent {
                title: "Policy".into(),
                category: Category::Policy,
                body: "Body".into()
            }
        })
    );
    assert_eq!(
        unit.steps,
        ["authorize", "replay", "apply", "flush", "authority"]
    );
}

#[tokio::test]
async fn update_rejects_a_stale_version_without_staging_effects() {
    let mut unit = Unit::new();
    unit.article.as_mut().unwrap().version = 3;
    let error = execute(
        &mut unit,
        &CommandKey::new("handbook-command").unwrap(),
        Change::UpdateArticle {
            id: "article-handbook-command".into(),
            expected_version: 2,
            title: "Changed".into(),
            category: Category::Procedure,
            body: "Changed body".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "VERSION_CONFLICT");
    assert_eq!(unit.steps, ["authorize", "replay", "article"]);
    assert!(unit.applied.is_none());
}

#[tokio::test]
async fn publishing_requires_both_title_and_body_before_the_plan() {
    let mut unit = Unit::new();
    unit.article.as_mut().unwrap().draft.body = " a ".into();
    let error = execute(
        &mut unit,
        &CommandKey::new("handbook-command").unwrap(),
        Change::ArticleAction {
            id: "article-handbook-command".into(),
            expected_version: 2,
            action: erpx_server_collaboration::public::handbook::ArticleAction::Publish,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "ARTICLE_CONTENT_REQUIRED");
    assert!(unit.applied.is_none());
}

#[tokio::test]
async fn publish_and_availability_actions_use_the_current_revision_and_version() {
    let key = CommandKey::new("handbook-command").unwrap();
    let mut unit = Unit::new();
    let receipt = execute(
        &mut unit,
        &key,
        Change::ArticleAction {
            id: "article-handbook-command".into(),
            expected_version: 2,
            action: erpx_server_collaboration::public::handbook::ArticleAction::Publish,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        (receipt.version, receipt.revision, receipt.status),
        (3, 2, "published")
    );
    assert_eq!(
        unit.applied,
        Some(OwnerPlan::PublishArticle {
            id: "article-handbook-command".into(),
            expected_version: 2,
            revision: 2
        })
    );
    for (action, archived, status) in [
        (
            erpx_server_collaboration::public::handbook::ArticleAction::Archive,
            true,
            "archived",
        ),
        (
            erpx_server_collaboration::public::handbook::ArticleAction::Restore,
            false,
            "restored",
        ),
    ] {
        let mut unit = Unit::new();
        let receipt = execute(
            &mut unit,
            &key,
            Change::ArticleAction {
                id: "article-handbook-command".into(),
                expected_version: 2,
                action,
            },
        )
        .await
        .unwrap();
        assert_eq!(receipt.status, status);
        assert_eq!(
            unit.applied,
            Some(OwnerPlan::SetArchived {
                id: "article-handbook-command".into(),
                expected_version: 2,
                archived
            })
        );
    }
}

#[tokio::test]
async fn replay_skips_state_but_still_revalidates_the_retained_unit() {
    let mut unit = Unit::new();
    unit.replay = true;
    unit.article = None;
    let receipt = execute(
        &mut unit,
        &CommandKey::new("handbook-command").unwrap(),
        change(),
    )
    .await
    .unwrap();
    assert_eq!(receipt.id, "retained");
    assert_eq!(unit.steps, ["authorize", "replay", "flush", "authority"]);
    assert!(unit.applied.is_none());

    unit.drift = true;
    let error = execute(
        &mut unit,
        &CommandKey::new("handbook-command").unwrap(),
        change(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "HANDBOOK_SOURCE_CHANGED");
}

#[tokio::test]
async fn editor_updates_require_the_owner_exact_settings_version_and_membership() {
    let command = Change::SetEditor {
        user_id: "target".into(),
        active: true,
        expected_version: 0,
    };
    let mut unit = Unit::new();
    unit.actor.can_manage_editors = false;
    assert_eq!(
        execute(
            &mut unit,
            &CommandKey::new("handbook-command").unwrap(),
            command.clone()
        )
        .await
        .unwrap_err()
        .code,
        "HANDBOOK_OWNER_REQUIRED"
    );

    let mut unit = Unit::new();
    unit.settings = 2;
    assert_eq!(
        execute(
            &mut unit,
            &CommandKey::new("handbook-command").unwrap(),
            command.clone()
        )
        .await
        .unwrap_err()
        .code,
        "VERSION_CONFLICT"
    );

    let mut unit = Unit::new();
    unit.member = false;
    assert_eq!(
        execute(
            &mut unit,
            &CommandKey::new("handbook-command").unwrap(),
            command.clone()
        )
        .await
        .unwrap_err()
        .code,
        "EDITOR_MEMBER_REQUIRED"
    );

    let mut unit = Unit::new();
    let receipt = execute(
        &mut unit,
        &CommandKey::new("handbook-command").unwrap(),
        command,
    )
    .await
    .unwrap();
    assert_eq!(
        receipt,
        Receipt {
            id: "target".into(),
            version: 1,
            revision: 0,
            status: "active"
        }
    );
    assert_eq!(
        unit.applied,
        Some(OwnerPlan::SetEditor {
            user_id: "target".into(),
            active: true,
            expected_version: 0
        })
    );
}

#[tokio::test]
async fn invalid_article_text_and_editor_identity_fail_before_owner_effects() {
    let mut unit = Unit::new();
    let error = execute(
        &mut unit,
        &CommandKey::new("handbook-command").unwrap(),
        Change::CreateArticle {
            title: "a".repeat(201),
            category: Category::Policy,
            body: "Body".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "HANDBOOK_INPUT_INVALID");
    assert!(unit.applied.is_none());

    let error = execute(
        &mut unit,
        &CommandKey::new("handbook-command").unwrap(),
        Change::SetEditor {
            user_id: String::new(),
            active: true,
            expected_version: 0,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "HANDBOOK_INPUT_INVALID");
}
