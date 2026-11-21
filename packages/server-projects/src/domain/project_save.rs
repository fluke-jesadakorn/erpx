//! Native Projects create/edit command facts.
//!
//! The owner command writes the project brief and its complete membership in
//! one serializable unit. Composition supplies the current active-staff facts
//! (the actor placement and one row per requested member) and the current
//! Commercial `projects.wbs` admission; a client can never set out-of-scope
//! fields, a foreign owner or an unearned capability.
use crate::domain::project_commands::js_trim;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const TITLE_MIN: usize = 2;
pub const TITLE_LIMIT: usize = 200;
pub const DESCRIPTION_LIMIT: usize = 10_000;
pub const MEMBER_LIMIT: usize = 200;
pub const MEMBER_ID_LIMIT: usize = 160;

/// The retained `projectSchema`: every field is optional except `title`, the
/// defaults match the Zod defaults and the expected version is a positive int.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveProjectCommand {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub member_user_ids: Vec<String>,
    #[serde(default = "default_version")]
    pub expected_version: i64,
}

const fn default_version() -> i64 {
    1
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SaveProjectReceipt {
    pub id: String,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct StoredProjectReceipt {
    pub operation: String,
    pub record_id: String,
    pub request_fingerprint: String,
    pub result_json: Option<String>,
}

/// Current Commercial admission facts for the project command. Composition
/// resolves each flag from the caller's Company purchase snapshot; a replay is
/// admitted by retained history rather than by new work.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectSavePurchase {
    pub new_work: bool,
    pub settle_existing: bool,
    pub history: bool,
}

/// The current actor plus the same-transaction People staff facts.
#[derive(Clone, Debug)]
pub struct SaveProjectActor {
    pub command: crate::domain::project_commands::ProjectCommandActor,
    /// Current active-staff placement of the actor (`activeStaff`).
    pub staff_branch_id: Option<String>,
    pub staff_department_id: Option<String>,
    /// Users with a current active People staff row in this Company.
    pub active_staff_users: BTreeSet<String>,
    pub purchase: ProjectSavePurchase,
}

#[derive(Clone, Debug)]
pub struct SaveProjectPlan {
    pub key: erpx_server_shared::public::CommandKey,
    pub operation: String,
    pub company_id: erpx_server_shared::public::CompanyId,
    pub actor_id: String,
    pub assignment_id: String,
    pub authority_json: String,
    pub project_id: String,
    /// Current project version for an edit; `0` for a fresh project.
    pub project_version: i64,
    pub title: String,
    pub description: String,
    /// Retained create used the actor's active-staff branch/department rather
    /// than the selected work-context scope.
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub member_user_ids: Vec<String>,
    pub version: i64,
    pub request_fingerprint: String,
    pub now: i64,
}

pub fn missing_project() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "PROJECT_NOT_FOUND", "Project not found.")
}

/// A concurrent change between the use-case read and the locked write.
pub fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_SOURCE_CHANGED",
        "The current project source changed. Reload it and try again.",
    )
}

pub fn owner_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PROJECT_OWNER_REQUIRED",
        "Only the project owner manages this project and its membership.",
    )
}

pub fn structure_frozen() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROJECT_STRUCTURE_FROZEN",
        "Reopen the project before changing its brief or membership.",
    )
}

/// The retained `guardCurrentOwner(project-version, …)` is a command assertion;
/// its failure surfaces as the shared exact-source CONFLICT.
pub fn version_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONFLICT",
        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
    )
}

/// The retained `activeStaff` failure for the actor or any requested member.
pub fn staff_link_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "STAFF_LINK_REQUIRED",
        "An active employee or contractor engagement and account are required in this company.",
    )
}

pub fn package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "This Project work requires its purchased capability.",
    )
}

pub fn package_history_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "This Project work requires its purchased capability or retained history.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMAND_KEY_CONFLICT",
        "This command key was already used for a different Project request.",
    )
}

pub fn incomplete_receipt() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_COMMAND_RECEIPT_INCOMPLETE",
        "The original Project command receipt is incomplete.",
    )
}

pub fn invalid_receipt() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECT_COMMAND_RECEIPT_INVALID",
        "The original Project command receipt is invalid.",
    )
}

fn cp_len(value: &str) -> usize {
    value.chars().count()
}

fn valid_title(value: &str) -> bool {
    let length = cp_len(value);
    (TITLE_MIN..=TITLE_LIMIT).contains(&length) && value == js_trim(value)
}

fn valid_description(value: &str) -> bool {
    cp_len(value) <= DESCRIPTION_LIMIT
}

fn valid_member(value: &str) -> bool {
    let length = cp_len(value);
    (1..=MEMBER_ID_LIMIT).contains(&length)
}

impl SaveProjectCommand {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_title(&self.title)
            || !valid_description(&self.description)
            || self.expected_version < 1
            || self.member_user_ids.len() > MEMBER_LIMIT
            || self.member_user_ids.iter().any(|user| !valid_member(user))
        {
            return Err(BusinessError::invalid(
                "PROJECT_INVALID",
                "Review the project title, brief, members and version.",
            ));
        }
        Ok(())
    }

    /// Retained `[...new Set(input.memberUserIds)]`: first occurrence wins and
    /// the owner may not be repeated as a separate member insert.
    pub fn unique_member_user_ids(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        self.member_user_ids
            .iter()
            .filter(|user| seen.insert((*user).clone()))
            .cloned()
            .collect()
    }
}

pub fn fingerprint(
    operation: &str,
    record_id: &str,
    input: &serde_json::Value,
) -> Result<String, BusinessError> {
    serde_json::to_string(&serde_json::json!({
        "operation": operation,
        "recordId": record_id,
        "input": input,
    }))
    .map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "PROJECT_COMMAND_INVALID",
            "The current Project command could not be retained.",
        )
    })
}

pub fn authority_json(
    actor: &crate::domain::project_commands::ProjectCommandActor,
) -> Result<String, BusinessError> {
    crate::domain::project_commands::authority_json(actor)
}
