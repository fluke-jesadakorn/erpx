//! Native People workforce facts: directory person create/update/status,
//! engagement placement transitions and compensation revisions.
//!
//! This module validates the exact command/read shapes that the retained
//! TypeScript `people.directory.*`, `people.person.create`,
//! `people.engagement.*` and `people.compensation.*` commands accepted. It
//! never reads or writes Identity, Organization, Operations, Workflow or
//! Finance rows; those owners are supplied through public ports at
//! composition. Contact and compensation fields are private facts and are not
//! part of any cross-owner authority witness.
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, HoldingId, RecordId, TimestampMillis,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

pub const DIRECTORY_CREATE_OPERATION: &str = "people.directory.create";
pub const DIRECTORY_UPDATE_OPERATION: &str = "people.directory.update";
pub const DIRECTORY_STATUS_OPERATION: &str = "people.directory.status";
pub const PERSON_CREATE_OPERATION: &str = "people.person.create";
pub const ENGAGEMENT_CREATE_OPERATION: &str = "people.engagement.create";
pub const ENGAGEMENT_TRANSITION_OPERATION: &str = "people.engagement.transition";
pub const COMPENSATION_CREATE_OPERATION: &str = "people.compensation.create";
pub const COMPENSATION_TRANSITION_OPERATION: &str = "people.compensation.transition";

pub const PERSON_RECORD_TYPE: &str = "hr_people";
pub const ENGAGEMENT_RECORD_TYPE: &str = "hr_engagements";
pub const COMPENSATION_RECORD_TYPE: &str = "hr_compensation_revisions";

const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
const EMAIL_LIMIT: usize = 320;

/// Fresh Account/IAM authority for one workforce command. The caller owns
/// authorization composition; this only checks the supplied current role.
#[derive(Clone, Debug)]
pub struct WorkforceCommandActor {
    pub user_id: String,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub assignment_id: String,
    pub role: String,
    pub permissions: Vec<String>,
    pub work_context: String,
    pub expires_at: i64,
    /// The retained non-beta lifecycle gate. When false the directory command
    /// must not suspend or reactivate a person outside the lifecycle workflow.
    pub beta: bool,
}

impl WorkforceCommandActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn can_manage_people(&self) -> bool {
        self.has("people.manage")
    }

    pub fn can_read_compensation(&self, person_user_id: Option<&str>) -> bool {
        self.has("people.compensation.read")
            || self.has("people.compensation.payroll")
            || person_user_id.is_some_and(|user| user == self.user_id)
    }

    pub fn require_manage(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.can_manage_people() {
            return Err(forbidden(
                "PERMISSION_REQUIRED",
                "Your selected role does not grant People management access.",
            ));
        }
        self.require_current(now)
    }

    pub fn require_compensation_approve(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("people.compensation.approve") {
            return Err(forbidden(
                "COMPENSATION_APPROVE_REQUIRED",
                "Your selected role does not grant compensation approval.",
            ));
        }
        self.require_current(now)
    }

    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.can_manage_people()
            && !self.has("people.compensation.read")
            && !self.has("people.compensation.payroll")
        {
            return Err(forbidden(
                "PERMISSION_REQUIRED",
                "Your selected role does not grant People read access.",
            ));
        }
        self.require_current(now)
    }

    fn require_current(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if self.user_id.is_empty()
            || self.assignment_id.is_empty()
            || self.role.is_empty()
            || self.work_context.len() != 64
            || !self
                .work_context
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(unavailable(
                "PEOPLE_AUTHORITY_INVALID",
                "The current People authority is unavailable.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(conflict(
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }

    /// Directory placement must stay inside the selected branch/department.
    pub fn require_placement_scope(
        &self,
        branch_id: &RecordId,
        department_id: Option<&RecordId>,
    ) -> Result<(), BusinessError> {
        if self
            .branch_id
            .as_ref()
            .is_some_and(|branch| branch != branch_id)
            || self
                .department_id
                .as_ref()
                .is_some_and(|department| department_id != Some(department))
        {
            return Err(forbidden(
                "SCOPE_FORBIDDEN",
                "Choose a placement in your assigned workspace.",
            ));
        }
        Ok(())
    }

    pub fn authority_json(&self) -> String {
        json!({
            "role": self.role,
            "assignmentId": self.assignment_id,
            "companyId": self.company_id.as_str(),
            "branchId": self.branch_id.as_ref().map(RecordId::as_str),
            "departmentId": self.department_id.as_ref().map(RecordId::as_str),
            "workContext": self.work_context,
            "userId": self.user_id,
        })
        .to_string()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PersonProfile {
    pub legal_name: String,
    pub preferred_name: Option<String>,
    pub email: String,
    pub phone: Option<String>,
    /// `None` keeps the current work contact; `Some(None)` clears it.
    pub work_email: Option<Option<String>>,
    pub work_phone: Option<Option<String>>,
    pub country_code: String,
    pub branch_id: RecordId,
    pub department_id: Option<RecordId>,
    pub job_title: String,
    pub worker_type: String,
    pub employment_type: String,
    pub start_date: String,
    pub currency: String,
    pub manager_person_id: Option<RecordId>,
    pub level_id: Option<RecordId>,
    /// `None` keeps the current position; `Some(None)` clears it.
    pub position_id: Option<Option<RecordId>>,
    pub accept_recommended_pay: bool,
    pub accept_recommended_leave: bool,
    pub reason: String,
    pub linked_member_user_id: Option<String>,
}

impl PersonProfile {
    fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let legal_name = text(raw, "legalName", 2, 200)?;
        let preferred_name = optional_text(raw, "preferredName", 160)?;
        let email = email(raw, "email", true)?;
        let phone = optional_text(raw, "phone", 60)?;
        let work_email = optional_patch_text(raw, "workEmail", EMAIL_LIMIT, true)?;
        let work_phone = optional_patch_text(raw, "workPhone", 60, false)?;
        let country_code = country_code(raw)?;
        let branch_id = required_id(raw, "branchId")?;
        let department_id = optional_id(raw, "departmentId")?;
        let job_title = text(raw, "jobTitle", 2, 160)?;
        let worker_type = enumerated(raw, "workerType", &["employee", "contractor"])?;
        let employment_type = enumerated(
            raw,
            "employmentType",
            &["salaried", "daily", "hourly", "shift", "contractor"],
        )?;
        let start_date = date(raw, "startDate")?;
        let currency = enumerated(raw, "currency", &["USD", "EUR", "GBP", "THB", "SGD", "CAD"])?;
        let manager_person_id = optional_id(raw, "managerPersonId")?;
        let level_id = optional_id(raw, "levelId")?;
        let position_id = optional_patch_id(raw, "positionId")?;
        let accept_recommended_pay = flag(raw, "acceptRecommendedPay")?;
        let accept_recommended_leave = flag(raw, "acceptRecommendedLeave")?;
        let reason = match raw.get("reason") {
            None | Some(Value::Null) => "Staff profile updated".to_owned(),
            Some(Value::String(value)) => {
                let trimmed = value.trim();
                if trimmed.encode_utf16().count() < 3 || trimmed.encode_utf16().count() > 1_000 {
                    return Err(request_invalid());
                }
                trimmed.to_owned()
            }
            Some(_) => return Err(request_invalid()),
        };
        let linked_member_user_id = match raw.get("linkedMemberUserId") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if !value.trim().is_empty() => {
                let trimmed = value.trim();
                if trimmed.encode_utf16().count() > 160 {
                    return Err(request_invalid());
                }
                Some(trimmed.to_owned())
            }
            Some(Value::String(_)) => None,
            Some(_) => return Err(request_invalid()),
        };
        // Directory placement only manages employees; external access is owned
        // by the Identity/People access workflow.
        if worker_type != "employee" {
            return Err(BusinessError::new(
                FailureKind::InvalidInput,
                "EXTERNAL_ACCESS_REQUIRED",
                "Manage external users in People -> Users & Access.",
            ));
        }
        if employment_type == "contractor" {
            return Err(BusinessError::new(
                FailureKind::InvalidInput,
                "WORKER_TYPE_INVALID",
                "Contractor worker and employment types must agree.",
            ));
        }
        Ok(Self {
            legal_name,
            preferred_name,
            email,
            phone,
            work_email,
            work_phone,
            country_code,
            branch_id,
            department_id,
            job_title,
            worker_type,
            employment_type,
            start_date,
            currency,
            manager_person_id,
            level_id,
            position_id,
            accept_recommended_pay,
            accept_recommended_leave,
            reason,
            linked_member_user_id,
        })
    }

    pub fn require_valid_placement(
        &self,
        actor: &WorkforceCommandActor,
    ) -> Result<(), BusinessError> {
        actor.require_placement_scope(&self.branch_id, self.department_id.as_ref())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PersonCreateInput {
    pub profile: PersonProfile,
}

impl PersonCreateInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        Ok(Self {
            profile: PersonProfile::parse(raw)?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PersonUpdateInput {
    pub profile: PersonProfile,
    pub expected_version: i64,
    pub engagement_id: Option<RecordId>,
    pub expected_engagement_version: Option<i64>,
}

impl PersonUpdateInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let expected_version = positive(raw, "expectedVersion")?;
        let engagement_id = optional_id(raw, "engagementId")?;
        let expected_engagement_version = match raw.get("expectedEngagementVersion") {
            None | Some(Value::Null) => None,
            Some(_) => Some(positive(raw, "expectedEngagementVersion")?),
        };
        Ok(Self {
            profile: PersonProfile::parse(raw)?,
            expected_version,
            engagement_id,
            expected_engagement_version,
        })
    }
}

/// The standalone `people.person.create` command has no placement or login.
#[derive(Clone, Debug, Serialize)]
pub struct PersonRecordInput {
    pub legal_name: String,
    pub preferred_name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub country_code: String,
}

impl PersonRecordInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let legal_name = text(raw, "legalName", 1, 200)?;
        let email = match raw.get("email") {
            None | Some(Value::Null) => None,
            Some(_) => Some(email(raw, "email", true)?),
        };
        Ok(Self {
            legal_name,
            preferred_name: optional_text(raw, "preferredName", 160)?,
            email,
            phone: optional_text(raw, "phone", 60)?,
            country_code: country_code(raw)?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PersonStatusInput {
    pub status: String,
    pub expected_version: i64,
}

impl PersonStatusInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        Ok(Self {
            status: enumerated(raw, "status", &["active", "inactive", "archived"])?,
            expected_version: positive(raw, "expectedVersion")?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct EngagementCreateInput {
    pub person_id: RecordId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub worker_type: String,
    pub employment_type: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub manager_person_id: Option<RecordId>,
    pub job_title: String,
    pub currency: String,
    pub pay_group_id: Option<RecordId>,
}

impl EngagementCreateInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let worker_type = enumerated(raw, "workerType", &["employee", "contractor"])?;
        let employment_type = enumerated(
            raw,
            "employmentType",
            &["salaried", "daily", "hourly", "shift", "contractor"],
        )?;
        let start_date = date(raw, "startDate")?;
        let end_date = optional_date(raw, "endDate")?;
        if end_date
            .as_deref()
            .is_some_and(|end| end < start_date.as_str())
        {
            return Err(BusinessError::invalid(
                "INVALID_DATES",
                "End date must be on or after start date.",
            ));
        }
        if worker_type == "employee" && employment_type == "contractor" {
            return Err(BusinessError::invalid(
                "INVALID_WORKER_TYPE",
                "Employee engagements cannot use contractor employment type.",
            ));
        }
        let currency = currency(raw, "currency")?;
        Ok(Self {
            person_id: required_id(raw, "personId")?,
            branch_id: optional_id(raw, "branchId")?,
            department_id: optional_id(raw, "departmentId")?,
            worker_type,
            employment_type,
            start_date,
            end_date,
            manager_person_id: optional_id(raw, "managerPersonId")?,
            job_title: text(raw, "jobTitle", 1, 160)?,
            currency,
            pay_group_id: optional_id(raw, "payGroupId")?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct EngagementTransitionInput {
    pub status: String,
    pub expected_version: i64,
    pub end_date: Option<String>,
}

impl EngagementTransitionInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        Ok(Self {
            status: enumerated(raw, "status", &["active", "suspended", "ended"])?,
            expected_version: positive(raw, "expectedVersion")?,
            end_date: optional_date(raw, "endDate")?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CompensationComponent {
    pub code: String,
    pub label: String,
    pub kind: String,
    pub amount: String,
    pub taxable: bool,
    pub income_timing: Option<String>,
    pub social_security_wage: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompensationCreateInput {
    pub engagement_id: RecordId,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub pay_frequency: String,
    pub base_amount_minor: i64,
    pub hourly_rate_minor: i64,
    pub daily_rate_minor: i64,
    pub standard_daily_hours: f64,
    pub expected_hours: f64,
    pub currency: String,
    pub components: Vec<CompensationComponent>,
}

impl CompensationCreateInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let effective_from = date(raw, "effectiveFrom")?;
        let effective_to = optional_date(raw, "effectiveTo")?;
        if effective_to
            .as_deref()
            .is_some_and(|end| end < effective_from.as_str())
        {
            return Err(BusinessError::invalid(
                "INVALID_DATES",
                "Effective end must follow the start date.",
            ));
        }
        let base = amount(raw, "baseAmount", "0")?;
        let hourly = amount(raw, "hourlyRate", "0")?;
        let daily = amount(raw, "dailyRate", "0")?;
        let standard_daily_hours = number(raw, "standardDailyHours", 8.0, false, 24.0)?;
        let expected_hours = number(raw, "expectedHours", 0.0, true, 744.0)?;
        let components = components(raw)?;
        let mut codes = BTreeSet::new();
        for component in &components {
            if !codes.insert(component.code.as_str()) {
                return Err(BusinessError::invalid(
                    "DUPLICATE_COMPONENT",
                    "Each compensation component needs a distinct code.",
                ));
            }
        }
        Ok(Self {
            engagement_id: required_id(raw, "engagementId")?,
            effective_from,
            effective_to,
            pay_frequency: enumerated(
                raw,
                "payFrequency",
                &[
                    "weekly",
                    "fortnightly",
                    "semimonthly",
                    "monthly",
                    "off_cycle",
                ],
            )?,
            base_amount_minor: base,
            hourly_rate_minor: hourly,
            daily_rate_minor: daily,
            standard_daily_hours,
            expected_hours,
            currency: currency(raw, "currency")?,
            components,
        })
    }

    /// Amount kinds passed to the Finance money snapshot port, in the retained
    /// command order.
    pub fn money_amounts(&self) -> Vec<(String, i64)> {
        let mut amounts = vec![
            ("base".to_owned(), self.base_amount_minor),
            ("hourly_rate".to_owned(), self.hourly_rate_minor),
            ("daily_rate".to_owned(), self.daily_rate_minor),
        ];
        for component in &self.components {
            amounts.push((
                format!("component:{}", component.code),
                compensation_minor(&component.amount).unwrap_or(0),
            ));
        }
        amounts
    }

    pub fn components_json(&self) -> Result<String, BusinessError> {
        let values: Vec<Value> = self
            .components
            .iter()
            .map(|component| {
                let mut value = Map::new();
                value.insert("code".into(), json!(component.code));
                value.insert("label".into(), json!(component.label));
                value.insert("kind".into(), json!(component.kind));
                value.insert("amount".into(), json!(component.amount));
                value.insert("taxable".into(), json!(component.taxable));
                if let Some(timing) = &component.income_timing {
                    value.insert("incomeTiming".into(), json!(timing));
                }
                if let Some(social) = component.social_security_wage {
                    value.insert("socialSecurityWage".into(), json!(social));
                }
                Value::Object(value)
            })
            .collect();
        serde_json::to_string(&values).map_err(|_| request_invalid())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CompensationTransitionInput {
    pub status: String,
    pub expected_version: i64,
}

impl CompensationTransitionInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        Ok(Self {
            status: enumerated(raw, "status", &["approved", "superseded"])?,
            expected_version: positive(raw, "expectedVersion")?,
        })
    }
}

/// Person row plus the selected employee engagement. Contact and compensation
/// fields never leave this owner through authority ports.
#[derive(Clone, Debug)]
pub struct WorkforcePersonRow {
    pub id: String,
    pub company_id: String,
    pub user_id: Option<String>,
    pub legal_name: String,
    pub preferred_name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub work_email: Option<String>,
    pub work_phone: Option<String>,
    pub country_code: String,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub engagement_id: Option<String>,
    pub engagement_version: Option<i64>,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub job_title: Option<String>,
    pub worker_type: Option<String>,
    pub employment_type: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub currency: Option<String>,
    pub engagement_status: Option<String>,
    pub manager_person_id: Option<String>,
    pub pay_group_id: Option<String>,
    pub position_id: Option<String>,
    pub level_id: Option<String>,
    pub level_name: Option<String>,
    pub level_number: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkforceCompensationSummary {
    pub base_amount_minor: i64,
    pub hourly_rate_minor: i64,
    pub daily_rate_minor: i64,
    pub standard_daily_hours: f64,
    pub pay_frequency: String,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub currency: String,
    pub status: String,
    pub version: i64,
}

/// Private person detail read model. The caller redacts phone/compensation
/// before it reaches presentation code.
#[derive(Clone, Debug, Serialize)]
pub struct WorkforcePersonDetail {
    pub id: String,
    pub user_id: Option<String>,
    pub legal_name: String,
    pub preferred_name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub work_email: Option<String>,
    pub work_phone: Option<String>,
    pub country_code: String,
    pub status: String,
    pub version: i64,
    pub engagement_id: Option<String>,
    pub engagement_version: Option<i64>,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub job_title: Option<String>,
    pub worker_type: Option<String>,
    pub employment_type: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub currency: Option<String>,
    pub engagement_status: Option<String>,
    pub manager_person_id: Option<String>,
    pub pay_group_id: Option<String>,
    pub position_id: Option<String>,
    pub level_id: Option<String>,
    pub level_name: Option<String>,
    pub level_number: Option<i64>,
    pub private_access: bool,
    pub compensation: Option<WorkforceCompensationSummary>,
}

impl WorkforcePersonRow {
    pub fn detail(
        &self,
        private_access: bool,
        compensation: Option<WorkforceCompensationSummary>,
    ) -> WorkforcePersonDetail {
        WorkforcePersonDetail {
            id: self.id.clone(),
            user_id: self.user_id.clone(),
            legal_name: self.legal_name.clone(),
            preferred_name: self.preferred_name.clone(),
            email: self.email.clone(),
            phone: if private_access {
                self.phone.clone()
            } else {
                None
            },
            work_email: self.work_email.clone(),
            work_phone: self.work_phone.clone(),
            country_code: self.country_code.clone(),
            status: self.status.clone(),
            version: self.version,
            engagement_id: self.engagement_id.clone(),
            engagement_version: self.engagement_version,
            branch_id: self.branch_id.clone(),
            department_id: self.department_id.clone(),
            job_title: self.job_title.clone(),
            worker_type: self.worker_type.clone(),
            employment_type: self.employment_type.clone(),
            start_date: self.start_date.clone(),
            end_date: self.end_date.clone(),
            currency: self.currency.clone(),
            engagement_status: self.engagement_status.clone(),
            manager_person_id: self.manager_person_id.clone(),
            pay_group_id: self.pay_group_id.clone(),
            position_id: self.position_id.clone(),
            level_id: self.level_id.clone(),
            level_name: self.level_name.clone(),
            level_number: self.level_number,
            private_access,
            compensation,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EngagementRow {
    pub id: String,
    pub person_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub worker_type: String,
    pub employment_type: String,
    pub status: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub manager_person_id: Option<String>,
    pub job_title: String,
    pub currency: String,
    pub pay_group_id: Option<String>,
    pub version: i64,
    pub person_user_id: Option<String>,
    pub person_status: String,
    pub level_id: Option<String>,
    pub position_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CompensationRow {
    pub id: String,
    pub engagement_id: String,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub currency: String,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

/// The partial unique facts needed by the update/status paths.
#[derive(Clone, Debug)]
pub struct PersonStatusFacts {
    pub status: String,
    pub version: i64,
    pub user_id: Option<String>,
    pub engagement_status: Option<String>,
    pub has_non_ended_engagements: bool,
    pub has_open_access_cutoff: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PersonWriteResult {
    pub id: String,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PersonStatusResult {
    pub id: String,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EngagementWriteResult {
    pub id: String,
    pub person_id: String,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CompensationWriteResult {
    pub id: String,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct EngagementTransitionPlan {
    pub status: String,
    pub end_date: Option<String>,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct PersonStatusPlan {
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct CompensationTransitionPlan {
    pub status: String,
    pub version: i64,
}

/// Retained engagement lifecycle rules, including the non-beta lifecycle gate.
pub fn plan_engagement_transition(
    actor: &WorkforceCommandActor,
    input: &EngagementTransitionInput,
    row: &EngagementRow,
) -> Result<EngagementTransitionPlan, BusinessError> {
    if !actor.beta {
        return Err(conflict(
            "LIFECYCLE_REQUIRED",
            "Use the person lifecycle workflow to review verified accounts and the effective time.",
        ));
    }
    if row.person_status == "archived" {
        return Err(not_found());
    }
    if row.version != input.expected_version {
        return Err(conflict(
            "ENGAGEMENT_CHANGED",
            "Reload the engagement before changing it.",
        ));
    }
    let valid = (row.status == "onboarding" && input.status == "active")
        || (matches!(row.status.as_str(), "active" | "suspended")
            && input.status != row.status
            && matches!(input.status.as_str(), "active" | "suspended" | "ended"));
    if !valid {
        return Err(conflict(
            "INVALID_ENGAGEMENT_TRANSITION",
            "Engagement cannot move through that transition.",
        ));
    }
    if input.status == "ended" && input.end_date.is_none() {
        return Err(BusinessError::invalid(
            "END_DATE_REQUIRED",
            "Enter the final date of engagement.",
        ));
    }
    if input
        .end_date
        .as_deref()
        .is_some_and(|end| end < row.start_date.as_str())
    {
        return Err(BusinessError::invalid(
            "INVALID_DATES",
            "End date must be on or after start date.",
        ));
    }
    if row.person_user_id.as_deref() == Some(actor.user_id.as_str()) && input.status != "active" {
        return Err(conflict(
            "SELF_DEACTIVATION",
            "Another HR administrator must deactivate your engagement.",
        ));
    }
    Ok(EngagementTransitionPlan {
        status: input.status.clone(),
        end_date: input.end_date.clone(),
        version: input.expected_version + 1,
    })
}

/// Retained directory status rules. `beta` reflects the same lifecycle gate as
/// the TypeScript command.
pub fn plan_person_status(
    actor: &WorkforceCommandActor,
    input: &PersonStatusInput,
    facts: &PersonStatusFacts,
) -> Result<PersonStatusPlan, BusinessError> {
    if !actor.beta {
        if input.status == "inactive" {
            return Err(conflict(
                "LIFECYCLE_REQUIRED",
                "Use Suspend access or End employment to review all linked accounts and the effective time.",
            ));
        }
        if input.status == "archived" && facts.engagement_status.as_deref() != Some("ended") {
            return Err(conflict(
                "ENGAGEMENT_NOT_ENDED",
                "End employment before archiving its history.",
            ));
        }
        if input.status == "active" && facts.status != "archived" && facts.has_open_access_cutoff {
            return Err(conflict(
                "LIFECYCLE_REQUIRED",
                "Request new access through the lifecycle workflow before returning to work.",
            ));
        }
    }
    if facts.version != input.expected_version {
        return Err(conflict(
            "PERSON_CHANGED",
            "Reload the person before changing their status.",
        ));
    }
    if input.status == "archived" && facts.has_non_ended_engagements {
        return Err(conflict(
            "ENGAGEMENT_NOT_ENDED",
            "End employment before archiving its history.",
        ));
    }
    if facts.user_id.as_deref() == Some(actor.user_id.as_str()) && input.status != "active" {
        return Err(conflict(
            "SELF_DEACTIVATION",
            "Another HR administrator must deactivate your employee record.",
        ));
    }
    Ok(PersonStatusPlan {
        status: input.status.clone(),
        version: input.expected_version + 1,
    })
}

/// Retained compensation approval rules: exact version, draft -> approved,
/// approved -> superseded and a different reviewer.
pub fn plan_compensation_transition(
    actor: &WorkforceCommandActor,
    input: &CompensationTransitionInput,
    row: &CompensationRow,
    evaluation_draft: bool,
) -> Result<CompensationTransitionPlan, BusinessError> {
    if evaluation_draft {
        return Err(conflict(
            "EVALUATION_COMPENSATION_WORKFLOW_REQUIRED",
            "Approve this draft in People evaluation so the original and new pay intervals commit together.",
        ));
    }
    if row.created_by == actor.user_id {
        return Err(conflict(
            "SELF_REVIEW_FORBIDDEN",
            "A different person must approve compensation.",
        ));
    }
    let expected_status = if input.status == "approved" {
        "draft"
    } else {
        "approved"
    };
    if row.status != expected_status {
        return Err(conflict(
            "COMPENSATION_CHANGED",
            "Reload the compensation revision before changing it.",
        ));
    }
    if row.version != input.expected_version {
        return Err(conflict(
            "COMPENSATION_CHANGED",
            "Reload the compensation revision before changing it.",
        ));
    }
    if actor
        .branch_id
        .as_ref()
        .is_some_and(|branch| row.branch_id.as_deref() != Some(branch.as_str()))
        || actor
            .department_id
            .as_ref()
            .is_some_and(|department| row.department_id.as_deref() != Some(department.as_str()))
    {
        return Err(forbidden(
            "SCOPE_FORBIDDEN",
            "The compensation revision is outside your assigned workspace.",
        ));
    }
    Ok(CompensationTransitionPlan {
        status: input.status.clone(),
        version: input.expected_version + 1,
    })
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_REQUEST_INVALID",
        "Review the people request details.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PERSON_NOT_FOUND",
        "Person not found in this workspace.",
    )
}

pub fn engagement_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ENGAGEMENT_NOT_FOUND",
        "Engagement not found in your scope.",
    )
}

pub fn compensation_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "COMPENSATION_NOT_FOUND",
        "Compensation not found in your scope.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command was already used for a different request. Start a new action.",
    )
}

pub fn command_incomplete() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMAND_INCOMPLETE",
        "This command did not finish. Retry the same action.",
    )
}

pub fn staff_placement_port_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_STAFF_PLACEMENT_PORT_UNAVAILABLE",
        "The Organization staff placement port is unavailable.",
    )
}

pub fn account_link_port_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_ACCOUNT_LINK_PORT_UNAVAILABLE",
        "The Identity account link port is unavailable.",
    )
}

pub fn compensation_money_port_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_COMPENSATION_MONEY_PORT_UNAVAILABLE",
        "The Finance money snapshot port is unavailable.",
    )
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

fn text(raw: &Value, field: &'static str, min: usize, max: usize) -> Result<String, BusinessError> {
    let Some(Value::String(value)) = raw.get(field) else {
        return Err(request_invalid());
    };
    let trimmed = value.trim();
    let length = trimmed.encode_utf16().count();
    if length < min || length > max {
        return Err(request_invalid());
    }
    Ok(trimmed.to_owned())
}

fn optional_text(
    raw: &Value,
    field: &'static str,
    max: usize,
) -> Result<Option<String>, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            if trimmed.encode_utf16().count() > max {
                return Err(request_invalid());
            }
            Ok((!trimmed.is_empty()).then(|| trimmed.to_owned()))
        }
        Some(_) => Err(request_invalid()),
    }
}

fn optional_patch_text(
    raw: &Value,
    field: &'static str,
    max: usize,
    is_email: bool,
) -> Result<Option<Option<String>>, BusinessError> {
    match raw.get(field) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Ok(Some(None));
            }
            if trimmed.encode_utf16().count() > max || (is_email && !valid_email(trimmed)) {
                return Err(request_invalid());
            }
            Ok(Some(Some(if is_email {
                trimmed.to_lowercase()
            } else {
                trimmed.to_owned()
            })))
        }
        Some(_) => Err(request_invalid()),
    }
}

fn email(raw: &Value, field: &'static str, required: bool) -> Result<String, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) if !required => Ok(String::new()),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            if !valid_email(trimmed) {
                return Err(request_invalid());
            }
            Ok(trimmed.to_lowercase())
        }
        _ => Err(request_invalid()),
    }
}

fn valid_email(value: &str) -> bool {
    if value.is_empty()
        || value.encode_utf16().count() > EMAIL_LIMIT
        || value.chars().any(char::is_whitespace)
    {
        return false;
    }
    let mut parts = value.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    parts.next().is_none()
        && !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}

fn country_code(raw: &Value) -> Result<String, BusinessError> {
    let Some(Value::String(value)) = raw.get("countryCode") else {
        return Err(request_invalid());
    };
    let value = value.trim();
    if value.len() != 2 || !value.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return Err(request_invalid());
    }
    Ok(value.to_ascii_uppercase())
}

fn currency(raw: &Value, field: &'static str) -> Result<String, BusinessError> {
    let Some(Value::String(value)) = raw.get(field) else {
        return Err(request_invalid());
    };
    let value = value.trim();
    if value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return Err(request_invalid());
    }
    Ok(value.to_ascii_uppercase())
}

fn enumerated(raw: &Value, field: &'static str, allowed: &[&str]) -> Result<String, BusinessError> {
    let Some(Value::String(value)) = raw.get(field) else {
        return Err(request_invalid());
    };
    let value = value.trim();
    if !allowed.contains(&value) {
        return Err(request_invalid());
    }
    Ok(value.to_owned())
}

fn flag(raw: &Value, field: &'static str) -> Result<bool, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(request_invalid()),
    }
}

fn positive(raw: &Value, field: &'static str) -> Result<i64, BusinessError> {
    let Some(Value::Number(value)) = raw.get(field) else {
        return Err(request_invalid());
    };
    value
        .as_i64()
        .filter(|value| *value >= 1)
        .ok_or_else(request_invalid)
}

fn date(raw: &Value, field: &'static str) -> Result<String, BusinessError> {
    let Some(Value::String(value)) = raw.get(field) else {
        return Err(request_invalid());
    };
    if !valid_date(value) {
        return Err(request_invalid());
    }
    Ok(value.clone())
}

fn optional_date(raw: &Value, field: &'static str) -> Result<Option<String>, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if valid_date(value) => Ok(Some(value.clone())),
        Some(_) => Err(request_invalid()),
    }
}

fn required_id(raw: &Value, field: &'static str) -> Result<RecordId, BusinessError> {
    let Some(Value::String(value)) = raw.get(field) else {
        return Err(request_invalid());
    };
    if value.trim() != value || value.is_empty() {
        return Err(request_invalid());
    }
    RecordId::new(value.clone()).map_err(|_| request_invalid())
}

fn optional_id(raw: &Value, field: &'static str) -> Result<Option<RecordId>, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            if value.trim() != value || value.is_empty() {
                return Err(request_invalid());
            }
            RecordId::new(value.clone())
                .map(Some)
                .map_err(|_| request_invalid())
        }
        Some(_) => Err(request_invalid()),
    }
}

fn optional_patch_id(
    raw: &Value,
    field: &'static str,
) -> Result<Option<Option<RecordId>>, BusinessError> {
    match raw.get(field) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(value)) => {
            if value.trim() != value || value.is_empty() {
                return Err(request_invalid());
            }
            RecordId::new(value.clone())
                .map(|id| Some(Some(id)))
                .map_err(|_| request_invalid())
        }
        Some(_) => Err(request_invalid()),
    }
}

fn number(
    raw: &Value,
    field: &'static str,
    default: f64,
    allow_zero: bool,
    max: f64,
) -> Result<f64, BusinessError> {
    let value = match raw.get(field) {
        None | Some(Value::Null) => default,
        Some(Value::Number(value)) => value.as_f64().ok_or_else(request_invalid)?,
        Some(_) => return Err(request_invalid()),
    };
    if !value.is_finite() || value < 0.0 || value > max || (!allow_zero && value == 0.0) {
        return Err(request_invalid());
    }
    Ok(value)
}

fn amount(raw: &Value, field: &'static str, default: &str) -> Result<i64, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => compensation_minor(default),
        Some(Value::String(value)) => compensation_minor(value),
        Some(_) => Err(request_invalid()),
    }
}

/// Accept the retained six-decimal input, truncating trailing zeros to the
/// two-decimal minor unit and rejecting any lost precision.
pub fn compensation_minor(value: &str) -> Result<i64, BusinessError> {
    let value = value.trim();
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.is_some_and(|value| {
            value.is_empty() || value.len() > 6 || !value.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(BusinessError::invalid(
            "INVALID_MONEY",
            "Enter a money amount with at most six decimals.",
        ));
    }
    let fraction = fraction.unwrap_or("");
    if fraction.len() > 2 && fraction[2..].bytes().any(|byte| byte != b'0') {
        return Err(BusinessError::invalid(
            "INVALID_MONEY",
            "Money supports at most two decimal places.",
        ));
    }
    let fraction = fraction.get(..2).unwrap_or(fraction);
    let fraction = if fraction.is_empty() {
        "0".to_owned()
    } else if fraction.len() == 1 {
        format!("{fraction}0")
    } else {
        fraction.to_owned()
    };
    let whole = whole
        .parse::<i64>()
        .map_err(|_| BusinessError::invalid("MONEY_OVERFLOW", "Money amount is too large."))?;
    let fraction = fraction
        .parse::<i64>()
        .map_err(|_| BusinessError::invalid("INVALID_MONEY", "Enter a valid money amount."))?;
    whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(fraction))
        .filter(|value| *value <= MAX_SAFE_INTEGER)
        .ok_or_else(|| BusinessError::invalid("MONEY_OVERFLOW", "Money amount is too large."))
}

fn components(raw: &Value) -> Result<Vec<CompensationComponent>, BusinessError> {
    let Some(Value::Array(values)) = raw.get("components") else {
        if raw.get("components").is_none() || raw.get("components") == Some(&Value::Null) {
            return Ok(Vec::new());
        }
        return Err(request_invalid());
    };
    if values.len() > 50 {
        return Err(request_invalid());
    }
    let mut components = Vec::with_capacity(values.len());
    for value in values {
        if !value.is_object() {
            return Err(request_invalid());
        }
        let code = text(value, "code", 1, 80)?;
        let label = text(value, "label", 1, 160)?;
        let kind = enumerated(value, "kind", &["earning", "deduction", "employer"])?;
        let amount = match value.get("amount") {
            Some(Value::String(value)) => {
                compensation_minor(value)?;
                value.trim().to_owned()
            }
            _ => return Err(request_invalid()),
        };
        let taxable = flag(value, "taxable")?;
        let income_timing = match value.get("incomeTiming") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) => {
                if !["regular", "irregular", "non_taxable"].contains(&value.as_str()) {
                    return Err(request_invalid());
                }
                Some(value.clone())
            }
            Some(_) => return Err(request_invalid()),
        };
        let social_security_wage = match value.get("socialSecurityWage") {
            None | Some(Value::Null) => None,
            Some(Value::Bool(value)) => Some(*value),
            Some(_) => return Err(request_invalid()),
        };
        components.push(CompensationComponent {
            code,
            label,
            kind,
            amount,
            taxable,
            income_timing,
            social_security_wage,
        });
    }
    Ok(components)
}

pub fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || !bytes[8..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    let year = value[..4].parse::<i32>().unwrap_or(0);
    let month = value[5..7].parse::<u32>().unwrap_or(0);
    let day = value[8..].parse::<u32>().unwrap_or(0);
    month >= 1 && month <= 12 && day >= 1 && day <= days_in_month(year, month)
}

fn leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn actor() -> WorkforceCommandActor {
        WorkforceCommandActor {
            user_id: "owner".into(),
            holding_id: HoldingId::new("holding").unwrap(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: None,
            department_id: None,
            assignment_id: "assignment".into(),
            role: "people".into(),
            permissions: vec!["people.manage".into()],
            work_context: "a".repeat(64),
            expires_at: 1_800_000_100_000,
            beta: true,
        }
    }

    fn profile() -> Value {
        json!({
            "legalName": "Somchai Jaidee",
            "email": "Somchai@Example.COM",
            "countryCode": "th",
            "branchId": "branch",
            "jobTitle": "Engineer",
            "workerType": "employee",
            "employmentType": "salaried",
            "startDate": "2026-01-01",
            "currency": "THB",
        })
    }

    #[test]
    fn directory_profile_normalizes_email_country_and_defaults() {
        let input = PersonCreateInput::parse(&profile()).unwrap();
        assert_eq!(input.profile.email, "somchai@example.com");
        assert_eq!(input.profile.country_code, "TH");
        assert_eq!(input.profile.reason, "Staff profile updated");
        assert!(!input.profile.accept_recommended_pay);
        assert!(input.profile.department_id.is_none());
    }

    #[test]
    fn directory_profile_rejects_external_workers_and_bad_dates() {
        let mut raw = profile();
        raw["workerType"] = json!("contractor");
        let error = PersonCreateInput::parse(&raw).unwrap_err();
        assert_eq!(error.code, "EXTERNAL_ACCESS_REQUIRED");
        let mut raw = profile();
        raw["employmentType"] = json!("contractor");
        assert_eq!(
            PersonCreateInput::parse(&raw).unwrap_err().code,
            "WORKER_TYPE_INVALID"
        );
        let mut raw = profile();
        raw["startDate"] = json!("2026-02-30");
        assert_eq!(
            PersonCreateInput::parse(&raw).unwrap_err().code,
            "PEOPLE_REQUEST_INVALID"
        );
        let mut raw = profile();
        raw["email"] = json!("not-an-email");
        assert_eq!(
            PersonCreateInput::parse(&raw).unwrap_err().code,
            "PEOPLE_REQUEST_INVALID"
        );
    }

    #[test]
    fn update_preserves_work_contact_until_explicitly_cleared() {
        let mut raw = profile();
        raw["expectedVersion"] = json!(3);
        let input = PersonUpdateInput::parse(&raw).unwrap();
        assert_eq!(input.expected_version, 3);
        assert!(input.profile.work_email.is_none());
        raw["workEmail"] = json!("");
        let input = PersonUpdateInput::parse(&raw).unwrap();
        assert_eq!(input.profile.work_email, Some(None));
        raw["workEmail"] = json!("Work@Example.com");
        let input = PersonUpdateInput::parse(&raw).unwrap();
        assert_eq!(
            input.profile.work_email,
            Some(Some("work@example.com".into()))
        );
    }

    #[test]
    fn compensation_amounts_truncate_trailing_zeroes_and_reject_precision_loss() {
        assert_eq!(compensation_minor("1000.5").unwrap(), 100_050);
        assert_eq!(compensation_minor("1000.500000").unwrap(), 100_050);
        assert_eq!(
            compensation_minor("1000.505").unwrap_err().code,
            "INVALID_MONEY"
        );
        let mut raw = json!({
            "engagementId": "engagement",
            "effectiveFrom": "2026-01-01",
            "payFrequency": "monthly",
            "currency": "THB",
            "components": [
                {"code":"BONUS","label":"Bonus","kind":"earning","amount":"1000.00"},
                {"code":"BONUS","label":"Other","kind":"earning","amount":"1.00"}
            ]
        });
        assert_eq!(
            CompensationCreateInput::parse(&raw).unwrap_err().code,
            "DUPLICATE_COMPONENT"
        );
        raw["components"] = json!([]);
        let input = CompensationCreateInput::parse(&raw).unwrap();
        assert_eq!(input.base_amount_minor, 0);
        assert_eq!(input.standard_daily_hours, 8.0);
        assert_eq!(input.components_json().unwrap(), "[]");
    }

    #[test]
    fn placement_scope_rejects_another_branch_or_department() {
        let mut scoped = actor();
        scoped.branch_id = Some(RecordId::new("branch").unwrap());
        let input = PersonCreateInput::parse(&profile()).unwrap();
        input.profile.require_valid_placement(&scoped).unwrap();
        let mut raw = profile();
        raw["branchId"] = json!("other-branch");
        assert_eq!(
            PersonCreateInput::parse(&raw)
                .unwrap()
                .profile
                .require_valid_placement(&scoped)
                .unwrap_err()
                .code,
            "SCOPE_FORBIDDEN"
        );
    }

    #[test]
    fn engagement_transition_requires_lifecycle_and_end_date() {
        let mut row = EngagementRow {
            id: "engagement".into(),
            person_id: "person".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            worker_type: "employee".into(),
            employment_type: "salaried".into(),
            status: "onboarding".into(),
            start_date: "2026-01-01".into(),
            end_date: None,
            manager_person_id: None,
            job_title: "Engineer".into(),
            currency: "THB".into(),
            pay_group_id: None,
            version: 1,
            person_user_id: Some("owner".into()),
            person_status: "active".into(),
            level_id: Some("level".into()),
            position_id: None,
        };
        let input =
            EngagementTransitionInput::parse(&json!({"status":"active","expectedVersion":1}))
                .unwrap();
        let mut non_beta = actor();
        non_beta.beta = false;
        assert_eq!(
            plan_engagement_transition(&non_beta, &input, &row)
                .unwrap_err()
                .code,
            "LIFECYCLE_REQUIRED"
        );
        assert_eq!(
            plan_engagement_transition(&actor(), &input, &row)
                .unwrap()
                .status,
            "active"
        );
        row.status = "active".into();
        let self_end = EngagementTransitionInput::parse(
            &json!({"status":"ended","expectedVersion":1,"endDate":"2026-06-30"}),
        )
        .unwrap();
        assert_eq!(
            plan_engagement_transition(&actor(), &self_end, &row)
                .unwrap_err()
                .code,
            "SELF_DEACTIVATION"
        );
        let mut other = actor();
        other.user_id = "hr".into();
        let missing_end =
            EngagementTransitionInput::parse(&json!({"status":"ended","expectedVersion":1}))
                .unwrap();
        assert_eq!(
            plan_engagement_transition(&other, &missing_end, &row)
                .unwrap_err()
                .code,
            "END_DATE_REQUIRED"
        );
    }

    #[test]
    fn compensation_transition_requires_an_independent_reviewer() {
        let row = CompensationRow {
            id: "compensation".into(),
            engagement_id: "engagement".into(),
            effective_from: "2026-01-01".into(),
            effective_to: None,
            currency: "THB".into(),
            status: "draft".into(),
            version: 1,
            created_by: "owner".into(),
            branch_id: None,
            department_id: None,
        };
        let input =
            CompensationTransitionInput::parse(&json!({"status":"approved","expectedVersion":1}))
                .unwrap();
        assert_eq!(
            plan_compensation_transition(&actor(), &input, &row, false)
                .unwrap_err()
                .code,
            "SELF_REVIEW_FORBIDDEN"
        );
        let mut other = actor();
        other.user_id = "reviewer".into();
        other.permissions.push("people.compensation.approve".into());
        let plan = plan_compensation_transition(&other, &input, &row, false).unwrap();
        assert_eq!(plan.version, 2);
        assert_eq!(
            plan_compensation_transition(&other, &input, &row, true)
                .unwrap_err()
                .code,
            "EVALUATION_COMPENSATION_WORKFLOW_REQUIRED"
        );
    }
}
