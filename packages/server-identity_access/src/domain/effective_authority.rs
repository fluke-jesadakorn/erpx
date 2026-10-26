use crate::domain::work_context::CurrentWorkSelection;
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct WorkAuthorizationRequest {
    pub permission: String,
    pub module: Option<String>,
    /// The router's matched owner route; not a forwarded browser header.
    pub destination: String,
    pub api: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct EffectiveRoleFact {
    pub id: String,
    pub revision: i64,
    pub permissions: Vec<String>,
    pub grantable_permissions: Vec<String>,
    pub scopes: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoginPolicy {
    Local,
    Directory,
    Emergency,
}
#[derive(Clone, Debug)]
pub(crate) struct EffectiveMemberFact {
    pub active: bool,
    pub login_policy: LoginPolicy,
    pub version: i64,
    pub principal_id: String,
    pub person_id: Option<RecordId>,
    pub directory_enabled: bool,
    pub directory_connection_active: bool,
    pub directory_observed_at: i64,
}
#[derive(Clone, Debug)]
pub(crate) struct HrAppointmentFact {
    pub version: i64,
    pub active_person: bool,
    pub unmerged_person: bool,
    pub active_engagement: bool,
    pub person_user_id: Option<UserId>,
    pub same_person_as_principal: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct IamAuthorityQuery {
    pub company_id: CompanyId,
    pub user_id: UserId,
    pub role: String,
    pub session_id: Option<RecordId>,
}
#[derive(Clone, Debug)]
pub(crate) struct CurrentIamAuthorityFacts {
    pub role: EffectiveRoleFact,
    pub member: Option<EffectiveMemberFact>,
    pub fallback_person_id: Option<RecordId>,
    pub aliases: Vec<UserId>,
    pub hr: Option<HrAppointmentFact>,
    pub emergency_until: Option<i64>,
}
pub(crate) struct EvaluatedIamAuthority {
    pub permissions: Vec<String>,
    pub hr_appointment_version: Option<i64>,
    pub emergency_until: Option<i64>,
    pub fresh_until: Option<i64>,
}
#[derive(Clone, Debug)]
pub(crate) struct EffectiveAuthorityFacts {
    pub selection: CurrentWorkSelection,
    pub iam: CurrentIamAuthorityFacts,
    pub beta: bool,
}
/// Only the owning PostgreSQL coordinator can construct this context. It lends
/// a reference during the retained Control/Holding transaction; no constructor,
/// serialization, deserialization or cloning can move it into a queued command.
/// Owner-specific record, commercial, MFA and independent-review policy remains required.
#[derive(Debug)]
pub struct AuthorizedWorkContext {
    pub(crate) holding_id: HoldingId,
    pub(crate) user_id: UserId,
    pub(crate) session_id: RecordId,
    pub(crate) selection: CurrentWorkSelection,
    pub(crate) permissions: BTreeSet<String>,
    pub(crate) grantable_permissions: Vec<String>,
    pub(crate) modules: Vec<String>,
    pub(crate) person_user_ids: BTreeSet<UserId>,
    pub(crate) person_id: Option<RecordId>,
    pub(crate) principal_id: String,
    pub(crate) identity_version: i64,
    pub(crate) hr_appointment_version: Option<i64>,
    pub(crate) emergency_until: Option<i64>,
    pub(crate) expires_at: i64,
}
impl AuthorizedWorkContext {
    pub fn holding_id(&self) -> &HoldingId {
        &self.holding_id
    }
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }
    pub fn session_id(&self) -> &RecordId {
        &self.session_id
    }
    pub fn selection(&self) -> &CurrentWorkSelection {
        &self.selection
    }
    pub fn person_id(&self) -> Option<&RecordId> {
        self.person_id.as_ref()
    }
    pub fn principal_id(&self) -> &str {
        &self.principal_id
    }
    pub fn identity_version(&self) -> i64 {
        self.identity_version
    }
    pub fn hr_appointment_version(&self) -> Option<i64> {
        self.hr_appointment_version
    }
    pub fn emergency_until(&self) -> Option<i64> {
        self.emergency_until
    }
    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }
    pub fn modules(&self) -> &[String] {
        &self.modules
    }
    pub fn grantable_permissions(&self) -> &[String] {
        &self.grantable_permissions
    }
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }
    pub fn same_person_user(&self, user: &UserId) -> bool {
        self.person_user_ids.contains(user)
    }
    pub fn require_permission(&self, permission: &str) -> Result<(), BusinessError> {
        if self.has_permission(permission) {
            Ok(())
        } else {
            Err(authority_error(
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ))
        }
    }
    pub(crate) fn require_current_time(&self, now: i64) -> Result<(), BusinessError> {
        if now < self.expires_at {
            Ok(())
        } else {
            Err(authority_error(
                "WORK_AUTHORITY_EXPIRED",
                "Your work authority expired. Review your current access.",
            ))
        }
    }
}
pub(crate) fn authority_error(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
pub fn effective_authority_permissions(
    role: &str,
    permissions: &[String],
    hr_appointment: bool,
) -> Vec<String> {
    let appointment = [
        "people.read",
        "people.manage",
        "leave.policy.manage",
        "module.people_payroll.read",
    ];
    let mut result: Vec<String> = permissions
        .iter()
        .filter(|p| role != "hr_admin" || hr_appointment || !appointment.contains(&p.as_str()))
        .cloned()
        .collect();
    if hr_appointment {
        for id in appointment {
            if !result.iter().any(|p| p == id) {
                result.push(id.into());
            }
        }
    }
    result
}
