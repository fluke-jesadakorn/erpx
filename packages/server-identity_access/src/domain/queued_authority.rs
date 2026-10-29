use super::{
    effective_authority::AuthorizedWorkContext,
    work_context::{ExpectedWorkSelection, WorkContextRequest},
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, HoldingId, RecordId, UserId,
};
use serde::{Deserialize, Serialize};

/// Historical owner-job input, NOT authorization. The mandatory owner source
/// port must bind these bytes to its current private job/source/lease. There is
/// no compatibility default for missing native producer witnesses.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueuedWorkBinding {
    pub version: u32,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub user_id: UserId,
    #[serde(deserialize_with = "present_optional")]
    pub branch_id: Option<RecordId>,
    #[serde(deserialize_with = "present_optional")]
    pub department_id: Option<RecordId>,
    #[serde(deserialize_with = "present_optional")]
    pub branch_slug: Option<String>,
    #[serde(deserialize_with = "present_optional")]
    pub department_slug: Option<String>,
    pub assignment_id: AssignmentId,
    pub assignment_version: i64,
    pub role: String,
    pub session_id: RecordId,
    pub selection_revision: i64,
    pub identity_version: i64,
    pub role_revision: i64,
    #[serde(deserialize_with = "present_optional")]
    pub hr_appointment_version: Option<i64>,
    #[serde(deserialize_with = "present_optional")]
    pub emergency_until: Option<i64>,
    pub expires_at: i64,
}
fn present_optional<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}
impl QueuedWorkBinding {
    pub fn parse(source: &str) -> Result<Self, BusinessError> {
        if source.len() > 16384 {
            return Err(changed());
        }
        serde_json::from_str(source).map_err(|_| changed())
    }
    pub fn to_json(&self) -> Result<String, BusinessError> {
        let json = serde_json::to_string(self).map_err(|_| changed())?;
        if json.len() > 16384 {
            return Err(changed());
        }
        Ok(json)
    }
    /// Only a current borrowed context supplies the native producer snapshot.
    /// Capturing it does not extend a grant, session or selected-context expiry.
    pub fn from_current(context: &AuthorizedWorkContext) -> Self {
        let selection = context.selection();
        let workspace = selection.workspace();
        Self {
            version: 1,
            holding_id: context.holding_id().clone(),
            company_id: workspace.company_id.clone(),
            user_id: context.user_id().clone(),
            branch_id: workspace.branch_id.clone(),
            department_id: workspace.department_id.clone(),
            branch_slug: workspace.branch_slug.clone(),
            department_slug: workspace.department_slug.clone(),
            assignment_id: selection.assignment().id.clone(),
            assignment_version: selection.assignment().version,
            role: selection.assignment().role.clone(),
            session_id: context.session_id().clone(),
            selection_revision: selection.revision(),
            identity_version: context.identity_version(),
            role_revision: selection.role_revision(),
            hr_appointment_version: context.hr_appointment_version(),
            emergency_until: context.emergency_until(),
            expires_at: selection.expires_at(),
        }
    }
    pub(crate) fn request(
        &self,
        holding: &HoldingId,
        now: i64,
    ) -> Result<WorkContextRequest, BusinessError> {
        if self.version != 1
            || &self.holding_id != holding
            || self.assignment_version < 1
            || self.role_revision < 1
            || self.identity_version < 0
            || self.selection_revision < 0
            || self.role.is_empty()
            || self.expires_at <= now
            || self.hr_appointment_version.is_some_and(|v| v < 1)
            || self.department_slug.is_some() && self.department_id.is_none()
            || self.branch_slug.is_some() && self.branch_id.is_none()
        {
            return Err(changed());
        }
        Ok(WorkContextRequest {
            holding_id: self.holding_id.clone(),
            company_id: self.company_id.clone(),
            user_id: self.user_id.clone(),
            session_id: self.session_id.clone(),
            branch_slug: self.branch_slug.clone(),
            department_slug: self.department_slug.clone(),
            session_expires_at: self.expires_at,
            expected: Some(ExpectedWorkSelection {
                revision: self.selection_revision,
                assignment_id: self.assignment_id.clone(),
                assignment_version: self.assignment_version,
            }),
            mutation: true,
        })
    }
    pub(crate) fn matches_current(&self, current: &AuthorizedWorkContext) -> bool {
        let s = current.selection();
        let w = s.workspace();
        self.holding_id == *current.holding_id()
            && self.user_id == *current.user_id()
            && self.session_id == *current.session_id()
            && self.company_id == w.company_id
            && self.branch_id == w.branch_id
            && self.department_id == w.department_id
            && self.assignment_id == s.assignment().id
            && self.assignment_version == s.assignment().version
            && self.role == s.assignment().role
            && self.role_revision == s.role_revision()
            && self.selection_revision == s.revision()
            && self.identity_version == current.identity_version()
            && self.emergency_until == current.emergency_until()
            && self
                .hr_appointment_version
                .is_none_or(|v| Some(v) == current.hr_appointment_version())
    }
}
pub(crate) fn changed() -> BusinessError {
    super::effective_authority::authority_error(
        "QUEUED_AUTHORITY_CHANGED",
        "The original queued authority is no longer current.",
    )
}
