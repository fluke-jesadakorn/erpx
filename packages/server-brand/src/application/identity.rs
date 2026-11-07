//! Brand identity use cases. Ports own current authority, replay, snapshots and
//! the commit; this layer never opens a transaction or reads HTTP.
use crate::domain::identity::{
    BrandCancelInput, BrandCategory, BrandCategoryLock, BrandCategoryLockInput, BrandDraft,
    BrandDraftInput, BrandIdentity, BrandPublishInput, BrandPublished, BrandRollbackInput,
    ActiveBrandIdentity,
};
use crate::domain::policy::{assert_brand_category_locks, assert_brand_independent_publisher};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::commands::{CommandActor, CommandField, CommandJson};
use serde_json::{Value, json};
use std::future::Future;

#[derive(Clone, Debug)]
pub struct BrandActor {
    pub holding_id: String,
    pub command: CommandActor,
    pub modules: Vec<String>,
    pub permissions: Vec<String>,
}

impl BrandActor {
    pub fn new(
        holding_id: impl Into<String>,
        command: CommandActor,
        modules: Vec<String>,
        permissions: Vec<String>,
    ) -> Self {
        Self {
            holding_id: holding_id.into(),
            command,
            modules,
            permissions,
        }
    }

    pub fn company_id(&self) -> &str {
        self.command.company_id.as_str()
    }

    pub fn user_id(&self) -> &str {
        self.command.user_id.as_str()
    }

    pub fn assignment_id(&self) -> &str {
        self.command.assignment_id.as_str()
    }

    pub fn role(&self) -> &str {
        &self.command.role
    }

    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn require_permission(&self, permission: &str) -> Result<(), BusinessError> {
        if self.has_permission(permission) {
            Ok(())
        } else {
            Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ))
        }
    }

    pub fn require_brand_module(&self) -> Result<(), BusinessError> {
        if self.modules.iter().any(|value| value == "brand") {
            Ok(())
        } else {
            Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "Brand identity is unavailable to the selected role.",
            ))
        }
    }

    fn scoped_scope(&self) -> bool {
        matches!(
            (&self.command.branch_id, &self.command.department_id),
            (CommandField::Omitted | CommandField::Null, CommandField::Omitted | CommandField::Null)
        )
    }

    pub fn can_manage(&self) -> bool {
        self.has_permission("organization.manage") && self.scoped_scope()
    }

    pub fn can_lock(&self) -> bool {
        self.has_permission("company.owner") && self.scoped_scope()
    }
}

#[derive(Clone, Debug)]
pub struct BrandMutationCommon {
    pub key: String,
    pub operation: &'static str,
    pub record_id: String,
    pub version: i64,
    pub input: CommandJson,
    pub now: i64,
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] // one owned mutation per command, not a hot collection
pub enum BrandMutationAction {
    Save {
        identity: BrandIdentity,
        locks: Vec<BrandCategoryLock>,
        existing: Option<BrandDraft>,
        active: Option<ActiveBrandIdentity>,
        reason: String,
        expected_published_version: i64,
        expected_version: Option<i64>,
    },
    Publish {
        identity: BrandIdentity,
        locks: Vec<BrandCategoryLock>,
        draft: BrandDraft,
        published_id: String,
        revision: i64,
        expected_published_version: i64,
        expected_version: i64,
        reason: String,
    },
    Rollback {
        identity: BrandIdentity,
        locks: Vec<BrandCategoryLock>,
        published: BrandPublished,
        expected_published_version: i64,
        reason: String,
    },
    Lock {
        category: BrandCategory,
        locked: bool,
        active: ActiveBrandIdentity,
        expected_version: i64,
        expected_published_version: i64,
        reason: String,
    },
    Cancel {
        expected_version: i64,
        reason: String,
    },
}

#[derive(Clone, Debug)]
pub struct BrandMutation {
    pub common: BrandMutationCommon,
    pub action: BrandMutationAction,
}

/// All methods are mandatory. No permissive default or serialized authority is
/// supplied; the adapter repeats every snapshot guard inside the commit.
pub trait BrandIdentityPorts {
    type Error: From<BusinessError> + Send;

    fn actor(&self) -> &BrandActor;
    fn now(&self) -> i64;
    fn authorize(
        &mut self,
        write: bool,
        owner: bool,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn replay(
        &mut self,
        key: &str,
        operation: &str,
        record_id: &str,
        input: &CommandJson,
    ) -> impl Future<Output = Result<Option<Value>, Self::Error>> + Send;
    fn active(
        &mut self,
    ) -> impl Future<Output = Result<Option<ActiveBrandIdentity>, Self::Error>> + Send;
    fn locks(&mut self) -> impl Future<Output = Result<Vec<BrandCategoryLock>, Self::Error>> + Send;
    fn draft(
        &mut self,
        id: &str,
    ) -> impl Future<Output = Result<Option<BrandDraft>, Self::Error>> + Send;
    fn published(
        &mut self,
        id: &str,
    ) -> impl Future<Output = Result<Option<BrandPublished>, Self::Error>> + Send;
    fn next_revision(&mut self) -> impl Future<Output = Result<i64, Self::Error>> + Send;
    fn is_same_person(
        &mut self,
        user_id: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn commit(
        &mut self,
        mutation: &BrandMutation,
        result: &Value,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

fn rule(kind: FailureKind, code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(kind, code, message)
}

fn input_json<T: serde::Serialize>(input: &T) -> Result<CommandJson, BusinessError> {
    CommandJson::from_serializable(input)
}

/// Save a reviewed identity draft. The first save creates a new draft; a later
/// save requires the current draft version and preserves the publication base.
pub async fn save_brand_identity_draft<P: BrandIdentityPorts>(
    ports: &mut P,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    let input = BrandDraftInput::parse(raw)?;
    ports.authorize(true, false).await?;
    let record_id = input
        .id
        .clone()
        .unwrap_or_else(|| format!("brand-draft-{key}"));
    let operation = if input.id.is_some() {
        "brand.identity.save"
    } else {
        "brand.identity.draft"
    };
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, &record_id, &canonical).await? {
        return Ok(prior);
    }
    let active = ports.active().await?;
    let locks = ports.locks().await?;
    assert_brand_category_locks(&locks, &input.identity)?;
    if input.expected_published_version
        != active.as_ref().map(|row| row.active_version).unwrap_or(0)
    {
        return Err(rule(
            FailureKind::Conflict,
            "BRAND_PUBLICATION_CHANGED",
            "The published identity changed. Review it before saving.",
        )
        .into());
    }
    let existing = match &input.id {
        Some(id) => ports.draft(id).await?,
        None => None,
    };
    if input.id.is_some() && !matches!(&existing, Some(row) if row.status == "draft") {
        return Err(rule(
            FailureKind::NotFound,
            "BRAND_DRAFT_UNAVAILABLE",
            "The editable draft is unavailable.",
        )
        .into());
    }
    if existing.is_some() && input.expected_version.is_none() {
        return Err(rule(
            FailureKind::InvalidInput,
            "EXPECTED_VERSION_REQUIRED",
            "Use the current draft version.",
        )
        .into());
    }
    let version = input.expected_version.unwrap_or(0) + 1;
    let result = json!({"id": record_id, "status": "draft", "version": version});
    ports
        .commit(
            &BrandMutation {
                common: BrandMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id,
                    version,
                    input: canonical,
                    now: ports.now(),
                },
                action: BrandMutationAction::Save {
                    identity: input.identity,
                    locks,
                    existing,
                    active,
                    reason: input.reason,
                    expected_published_version: input.expected_published_version,
                    expected_version: input.expected_version,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

/// Publish a draft under a different person than its creator or last updater.
pub async fn publish_brand_identity<P: BrandIdentityPorts>(
    ports: &mut P,
    draft_id: &str,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    let input = BrandPublishInput::parse(raw)?;
    ports.authorize(true, false).await?;
    let operation = "brand.identity.publish";
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, draft_id, &canonical).await? {
        return Ok(prior);
    }
    let draft = ports.draft(draft_id).await?;
    let Some(draft) = draft.filter(|row| row.status == "draft") else {
        return Err(rule(
            FailureKind::NotFound,
            "BRAND_DRAFT_UNAVAILABLE",
            "The editable draft is unavailable.",
        )
        .into());
    };
    let created_by_is_actor = ports.is_same_person(&draft.created_by).await?;
    let updated_by_is_actor = ports.is_same_person(&draft.updated_by).await?;
    assert_brand_independent_publisher(created_by_is_actor, updated_by_is_actor)?;
    let identity = BrandIdentity::from_json(&draft.identity_json)?;
    let active = ports.active().await?;
    let locks = ports.locks().await?;
    assert_brand_category_locks(&locks, &identity)?;
    if draft.based_revision != active.as_ref().map(|row| row.revision).unwrap_or(0) {
        return Err(rule(
            FailureKind::Conflict,
            "BRAND_DRAFT_REBASE_REQUIRED",
            "The published identity advanced. Save a reviewed draft against the current identity first.",
        )
        .into());
    }
    let now = ports.now();
    let revision = ports.next_revision().await?;
    let published_id = format!("brand-version-{key}");
    let version = input.expected_version + 1;
    let result = json!({
        "id": draft_id,
        "status": "published",
        "version": version,
        "publishedId": published_id,
        "revision": revision,
        "publishedVersion": input.expected_published_version + 1,
    });
    ports
        .commit(
            &BrandMutation {
                common: BrandMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id: draft_id.to_owned(),
                    version,
                    input: canonical,
                    now,
                },
                action: BrandMutationAction::Publish {
                    identity,
                    locks,
                    draft,
                    published_id,
                    revision,
                    expected_published_version: input.expected_published_version,
                    expected_version: input.expected_version,
                    reason: input.reason,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

/// Restore an immutable published version after the current active version and
/// every approved category lock match.
pub async fn rollback_brand_identity<P: BrandIdentityPorts>(
    ports: &mut P,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    let input = BrandRollbackInput::parse(raw)?;
    ports.authorize(true, false).await?;
    let operation = "brand.identity.rollback";
    let record_id = ports.actor().company_id().to_owned();
    let canonical = input_json(&input)?;
    if let Some(prior) = ports
        .replay(key, operation, &record_id, &canonical)
        .await?
    {
        return Ok(prior);
    }
    let published = ports.published(&input.published_id).await?;
    let Some(published) = published else {
        return Err(rule(
            FailureKind::NotFound,
            "BRAND_VERSION_UNAVAILABLE",
            "Choose a published identity from this Company.",
        )
        .into());
    };
    let identity = BrandIdentity::from_json(&published.identity_json)?;
    let locks = ports.locks().await?;
    assert_brand_category_locks(&locks, &identity)?;
    let version = input.expected_published_version + 1;
    let result = json!({
        "id": published.id,
        "revision": published.revision,
        "publishedVersion": version,
    });
    ports
        .commit(
            &BrandMutation {
                common: BrandMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id,
                    version,
                    input: canonical,
                    now: ports.now(),
                },
                action: BrandMutationAction::Rollback {
                    identity,
                    locks,
                    published,
                    expected_published_version: input.expected_published_version,
                    reason: input.reason,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

/// Change a category lock. Only a Company owner may lock or unlock, and only an
/// active independently published identity may supply the approved snapshot.
pub async fn change_brand_category_lock<P: BrandIdentityPorts>(
    ports: &mut P,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    let input = BrandCategoryLockInput::parse(raw)?;
    ports.authorize(true, true).await?;
    let record_id = format!(
        "{}:{}",
        ports.actor().company_id(),
        input.category.as_str()
    );
    let operation = if input.locked {
        "brand.category.lock"
    } else {
        "brand.category.unlock"
    };
    let canonical = input_json(&input)?;
    if let Some(prior) = ports
        .replay(key, operation, &record_id, &canonical)
        .await?
    {
        return Ok(prior);
    }
    let active = ports.active().await?;
    let Some(active) = active else {
        return Err(rule(
            FailureKind::Conflict,
            "BRAND_PUBLISHED_IDENTITY_REQUIRED",
            "Publish an independently reviewed identity before locking categories.",
        )
        .into());
    };
    let version = input.expected_version + 1;
    let result = json!({
        "category": input.category.as_str(),
        "locked": input.locked,
        "version": version,
    });
    ports
        .commit(
            &BrandMutation {
                common: BrandMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id,
                    version,
                    input: canonical,
                    now: ports.now(),
                },
                action: BrandMutationAction::Lock {
                    category: input.category,
                    locked: input.locked,
                    active,
                    expected_version: input.expected_version,
                    expected_published_version: input.expected_published_version,
                    reason: input.reason,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

/// Withdraw an editable draft without deleting its retained evidence.
pub async fn cancel_brand_draft<P: BrandIdentityPorts>(
    ports: &mut P,
    draft_id: &str,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    let input = BrandCancelInput::parse(raw)?;
    ports.authorize(true, false).await?;
    let operation = "brand.identity.cancel";
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, draft_id, &canonical).await? {
        return Ok(prior);
    }
    let version = input.expected_version + 1;
    let result = json!({"id": draft_id, "status": "cancelled", "version": version});
    ports
        .commit(
            &BrandMutation {
                common: BrandMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id: draft_id.to_owned(),
                    version,
                    input: canonical,
                    now: ports.now(),
                },
                action: BrandMutationAction::Cancel {
                    expected_version: input.expected_version,
                    reason: input.reason,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}
