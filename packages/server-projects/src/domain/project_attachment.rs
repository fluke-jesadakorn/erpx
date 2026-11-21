//! Native Projects attachment target policy.
//!
//! Files owns the attachment set/link ledger, quarantine, scanning and byte
//! storage. Projects owns only the target fact: the current Company, lifecycle,
//! project version and writer/reader authority for a `project` attachment
//! target. The port below is the bounded Files seam; it must run inside the
//! caller's existing transaction so the Files bind and the project version step
//! commit together. This crate never writes an attachment table.
use crate::domain::project_commands::js_trim;
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::{Deserialize, Serialize};

const LINK_LIMIT: usize = 100;
const CAPTION_LIMIT: usize = 500;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectAttachmentTargetType {
    Project,
}

impl ProjectAttachmentTargetType {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "project" => Ok(Self::Project),
            _ => Err(BusinessError::new(
                FailureKind::InvalidInput,
                "ATTACHMENT_TARGET_UNSUPPORTED",
                "This attachment target has no native Projects policy.",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectAttachmentPurpose {
    NewWork,
    SettleExisting,
    History,
}

impl ProjectAttachmentPurpose {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "new-work" => Ok(Self::NewWork),
            "settle-existing" => Ok(Self::SettleExisting),
            "history" => Ok(Self::History),
            _ => Err(BusinessError::invalid(
                "PROJECT_ATTACHMENT_PURPOSE_INVALID",
                "Choose a supported attachment purpose.",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::NewWork => "new-work",
            Self::SettleExisting => "settle-existing",
            Self::History => "history",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProjectAttachmentActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
    pub active_staff: bool,
    /// Current Commercial Projects WBS capability. Composition supplies it
    /// after its own same-transaction admission.
    pub wbs_purchased: bool,
}

impl ProjectAttachmentActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn require(&self, permission: &str, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has(permission) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this Project file action.",
            ));
        }
        if !self.modules.iter().any(|value| value == "projects") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "Your current role cannot access Projects.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        if !self.active_staff {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "STAFF_LINK_REQUIRED",
                "An active staff profile is required for Project files.",
            ));
        }
        Ok(())
    }

    pub fn require_write(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require("records.write", now)
    }

    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require("records.read", now)
    }
}

#[derive(Clone, Debug)]
pub struct ProjectAttachmentTargetFacts {
    pub company_id: String,
    pub target_id: String,
    pub owner_user_id: String,
    pub lifecycle_status: String,
    pub version: i64,
    pub is_owner: bool,
    pub is_member: bool,
}

#[derive(Clone, Debug)]
pub struct ProjectAttachmentGrant {
    pub company_id: CompanyId,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub target_type: ProjectAttachmentTargetType,
    pub target_id: String,
    pub expected_version: i64,
    pub actor_id: String,
    pub capability: &'static str,
    pub purpose: ProjectAttachmentPurpose,
    /// Opaque current target witness, for example the retained fact image.
    /// Files rechecks it after binding and after the project version step.
    pub witness: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectAttachmentBindingCommand {
    pub target_type: String,
    pub target_id: String,
    pub expected_version: i64,
    pub purpose: String,
    pub link_ids: Vec<String>,
    #[serde(default)]
    pub caption: Option<String>,
}

impl ProjectAttachmentBindingCommand {
    pub fn normalize(mut self) -> Self {
        self.caption = self
            .caption
            .map(|caption| js_trim(&caption).to_owned())
            .filter(|caption| !caption.is_empty());
        self
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.expected_version < 1 {
            return Err(BusinessError::invalid(
                "PROJECT_ATTACHMENT_INVALID",
                "Review the current project version before binding files.",
            ));
        }
        ProjectAttachmentTargetType::parse(&self.target_type)?;
        ProjectAttachmentPurpose::parse(&self.purpose)?;
        RecordId::new(self.target_id.as_str()).map_err(|_| attachment_invalid())?;
        if self.link_ids.is_empty() || self.link_ids.len() > LINK_LIMIT {
            return Err(attachment_invalid());
        }
        let mut seen = std::collections::BTreeSet::new();
        for link in &self.link_ids {
            if RecordId::new(link.as_str()).is_err() || !seen.insert(link) {
                return Err(attachment_invalid());
            }
        }
        if self
            .caption
            .as_deref()
            .is_some_and(|caption| caption.chars().count() > CAPTION_LIMIT)
        {
            return Err(attachment_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProjectAttachmentBindingReceipt {
    pub set_id: String,
    pub link_ids: Vec<String>,
    pub target_version: i64,
}

/// The Files-owned bound link identity the Projects receipt needs. No bytes,
/// storage key, checksum or media type crosses into Projects.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectAttachmentBoundLink {
    pub link_id: String,
    pub source_id: String,
}

pub fn attachment_invalid() -> BusinessError {
    BusinessError::invalid(
        "PROJECT_ATTACHMENT_INVALID",
        "Review the selected links and current project version.",
    )
}

pub fn attachment_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_TARGET_NOT_FOUND",
        "The original Project file source is unavailable.",
    )
}

pub fn attachment_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_ATTACHMENT_CHANGED",
        "The original Project file source or current authority changed.",
    )
}

pub fn attachment_set_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_SET_CHANGED",
        "The original file binding changed. Refresh before retrying.",
    )
}

pub fn attachment_set_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_SET_NOT_FOUND",
        "The original file set is unavailable.",
    )
}

pub fn attachment_set_locked() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_SET_LOCKED",
        "A file set was already bound or closed.",
    )
}

pub fn attachment_set_empty() -> BusinessError {
    BusinessError::invalid("ATTACHMENT_SET_EMPTY", "Bind at least one available file.")
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command key was already used for a different Project request.",
    )
}

pub fn invalid_receipt() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_ATTACHMENT_RECEIPT_INVALID",
        "The original Project file receipt is invalid.",
    )
}

pub fn attachment_owner_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PROJECT_FILE_OWNER_REQUIRED",
        "The project owner and an active project lifecycle are required to bind project files.",
    )
}

pub fn attachment_project_owner_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ATTACHMENT_PROJECT_OWNER_REQUIRED",
        "Only the current project owner can manage project documents.",
    )
}

pub fn attachment_project_frozen() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_STRUCTURE_FROZEN",
        "Reopen the project before changing its documents.",
    )
}

pub fn attachment_target_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_TARGET_CHANGED",
        "The record changed. Refresh before adding files.",
    )
}

pub fn attachment_package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "This Project file action requires its purchased capability.",
    )
}

/// Owner-side authorization: callers receive a grant only for the current
/// project version and their real writer/reader role. The target witness is
/// built by the adapter and rechecked inside the Files transaction.
pub fn authorize_binding(
    actor: &ProjectAttachmentActor,
    facts: &ProjectAttachmentTargetFacts,
    command: &ProjectAttachmentBindingCommand,
    witness: String,
    now: TimestampMillis,
) -> Result<ProjectAttachmentGrant, BusinessError> {
    actor.require_write(now)?;
    if !actor.wbs_purchased {
        return Err(attachment_package_required());
    }
    let target_type = ProjectAttachmentTargetType::parse(&command.target_type)?;
    if facts.company_id != actor.company_id.as_str()
        || facts.target_id != command.target_id
        || !facts.is_owner
        || facts.owner_user_id != actor.user_id
        || facts.lifecycle_status != "active"
        || facts.version != command.expected_version
    {
        return Err(attachment_owner_required());
    }
    Ok(ProjectAttachmentGrant {
        company_id: actor.company_id.clone(),
        branch_id: actor
            .branch_id
            .as_ref()
            .map(RecordId::as_str)
            .map(str::to_owned),
        department_id: actor
            .department_id
            .as_ref()
            .map(RecordId::as_str)
            .map(str::to_owned),
        target_type,
        target_id: facts.target_id.clone(),
        expected_version: facts.version,
        actor_id: actor.user_id.clone(),
        capability: "projects.wbs",
        purpose: ProjectAttachmentPurpose::parse(&command.purpose)?,
        witness,
    })
}

/// Owner-side intake authorization for a new draft set or reservation. The
/// retained Projects policy requires the current project owner, an active
/// lifecycle, the exact current version and the purchased WBS capability.
/// Files still owns the set/link ledger and applies this check in its own unit.
pub fn authorize_staging(
    actor: &ProjectAttachmentActor,
    facts: &ProjectAttachmentTargetFacts,
    expected_version: i64,
    now: TimestampMillis,
) -> Result<(), BusinessError> {
    actor.require_write(now)?;
    if !actor.wbs_purchased {
        return Err(attachment_package_required());
    }
    if facts.company_id != actor.company_id.as_str()
        || facts.target_id.is_empty()
        || !facts.is_owner
        || facts.owner_user_id != actor.user_id
    {
        return Err(attachment_project_owner_required());
    }
    if facts.lifecycle_status != "active" {
        return Err(attachment_project_frozen());
    }
    if expected_version < 1 || facts.version != expected_version {
        return Err(attachment_target_changed());
    }
    Ok(())
}

/// Owner-side read authorization used by the Files read path. Outsiders and
/// former members are denied as not found; the Files owner still performs its
/// own set/link/scan checks.
pub fn authorize_read(
    actor: &ProjectAttachmentActor,
    facts: &ProjectAttachmentTargetFacts,
    now: TimestampMillis,
) -> Result<(), BusinessError> {
    actor.require_read(now)?;
    if facts.company_id != actor.company_id.as_str() || (!facts.is_owner && !facts.is_member) {
        return Err(attachment_not_found());
    }
    Ok(())
}

pub fn witness(facts: &ProjectAttachmentTargetFacts) -> String {
    serde_json::json!({
        "companyId": facts.company_id,
        "targetType": "project",
        "targetId": facts.target_id,
        "ownerUserId": facts.owner_user_id,
        "lifecycleStatus": facts.lifecycle_status,
        "version": facts.version,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> ProjectAttachmentActor {
        ProjectAttachmentActor {
            user_id: "owner".into(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: None,
            department_id: None,
            permissions: vec!["records.write".into(), "records.read".into()],
            modules: vec!["projects".into()],
            expires_at: 2_000,
            active_staff: true,
            wbs_purchased: true,
        }
    }

    fn facts() -> ProjectAttachmentTargetFacts {
        ProjectAttachmentTargetFacts {
            company_id: "company".into(),
            target_id: "project".into(),
            owner_user_id: "owner".into(),
            lifecycle_status: "active".into(),
            version: 3,
            is_owner: true,
            is_member: true,
        }
    }

    fn command() -> ProjectAttachmentBindingCommand {
        ProjectAttachmentBindingCommand {
            target_type: "project".into(),
            target_id: "project".into(),
            expected_version: 3,
            purpose: "new-work".into(),
            link_ids: vec!["link-1".into()],
            caption: None,
        }
    }

    #[test]
    fn member_is_not_a_writer() {
        let mut actor = actor();
        actor.user_id = "member".into();
        let mut facts = facts();
        facts.is_owner = false;
        facts.owner_user_id = "owner".into();
        let error = authorize_binding(
            &actor,
            &facts,
            &command(),
            witness(&facts),
            TimestampMillis::new(1_000),
        )
        .unwrap_err();
        assert_eq!(error.code, "PROJECT_FILE_OWNER_REQUIRED");
    }

    #[test]
    fn stale_project_version_is_rejected() {
        let mut command = command();
        command.expected_version = 2;
        let error = authorize_binding(
            &actor(),
            &facts(),
            &command,
            witness(&facts()),
            TimestampMillis::new(1_000),
        )
        .unwrap_err();
        assert_eq!(error.code, "PROJECT_FILE_OWNER_REQUIRED");
    }

    #[test]
    fn outsider_read_is_hidden() {
        let mut actor = actor();
        actor.user_id = "outsider".into();
        let mut facts = facts();
        facts.is_owner = false;
        facts.is_member = false;
        let error = authorize_read(&actor, &facts, TimestampMillis::new(1_000)).unwrap_err();
        assert_eq!(error.code, "ATTACHMENT_TARGET_NOT_FOUND");
    }
}
