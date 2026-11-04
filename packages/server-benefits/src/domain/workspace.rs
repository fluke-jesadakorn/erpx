//! Native Benefits enrollment workspace facts.
//!
//! These are owner read projections only. They never create, publish, retire,
//! verify, submit, acknowledge or release a plan, window, dependent or
//! enrollment. Composition retains the normal Account/Holding session, current
//! authority, package admission and serializable owner transaction; this
//! domain enforces the current Company scope, `records.read`, the `benefits`
//! module, work-context expiry, exact plan/window/dependent/enrollment row
//! shapes, the current worker's dependent and self-enrollment scope, reviewer
//! scope, deterministic ordering and bounded page sizes before presentation.
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::Serialize;
use std::collections::BTreeSet;

pub const PLAN_LIMIT: usize = 500;
pub const WINDOW_LIMIT: usize = 500;
pub const DEPENDENT_LIMIT: usize = 200;
pub const ELECTION_LIMIT: usize = 1_000;
const NAME_LIMIT: usize = 160;
const PROVIDER_LIMIT: usize = 160;
const RELATIONSHIP_LIMIT: usize = 80;
const ELIGIBILITY_LIMIT: usize = 120;
const WORK_CONTEXT_LIMIT: usize = 64;

#[derive(Clone, Debug)]
pub struct BenefitsActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    /// Opaque current-context fingerprint rendered by composition. It never
    /// grants authority; it only keeps a client retry bound to the page that
    /// rendered the current command boundary.
    pub work_context: String,
    pub expires_at: i64,
}

impl BenefitsActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self) -> bool {
        self.modules.iter().any(|value| value == "benefits")
    }

    pub fn can_write(&self) -> bool {
        self.has("records.write")
    }

    pub fn can_review(&self) -> bool {
        self.has("benefits.review")
    }

    pub fn can_manage_documents(&self) -> bool {
        self.has("benefits.documents.manage")
    }

    pub fn require_workspace(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("records.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "BENEFITS_PERMISSION_REQUIRED",
                "Your selected role does not grant Benefits read access.",
            ));
        }
        if !self.has_module() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "BENEFITS_MODULE_FORBIDDEN",
                "Your current role cannot access Benefits.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        if !valid_work_context(&self.work_context) {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenefitsWorker {
    pub person_id: String,
    pub engagement_id: String,
}

impl BenefitsWorker {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_identifier(&self.person_id) || !valid_identifier(&self.engagement_id) {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenefitsPurchaseControls {
    pub new_work: bool,
    pub settle_existing: bool,
    pub history: bool,
    pub revision: String,
}

#[derive(Clone, Debug)]
pub struct BenefitsEntitlement {
    pub enforced: bool,
    pub history_allowed: bool,
    pub new_work: bool,
    pub settle_existing: bool,
    pub revision: String,
}

impl BenefitsEntitlement {
    pub fn require_history(&self) -> Result<(), BusinessError> {
        if !self.enforced {
            return Ok(());
        }
        if !self.history_allowed {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PACKAGE_REQUIRED",
                "This Benefits work requires its purchased capability or retained history.",
            ));
        }
        if self.revision.is_empty() || !valid_text(&self.revision, 200) {
            return Err(source_invalid());
        }
        Ok(())
    }

    pub fn purchase(&self) -> Option<BenefitsPurchaseControls> {
        self.enforced.then(|| BenefitsPurchaseControls {
            new_work: self.new_work,
            settle_existing: self.settle_existing,
            history: self.history_allowed,
            revision: self.revision.clone(),
        })
    }
}

pub(crate) fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BENEFITS_SOURCE_INVALID",
        "The current Benefits source is invalid.",
    )
}

fn too_large() -> BusinessError {
    BusinessError::invalid(
        "BENEFITS_WORKSPACE_TOO_LARGE",
        "Narrow the Benefits workspace before reading it.",
    )
}

fn valid_identifier(value: &str) -> bool {
    RecordId::new(value).is_ok()
}

fn valid_text(value: &str, limit: usize) -> bool {
    let length = value.chars().count();
    length > 0 && length <= limit && value.trim() == value && !value.chars().any(char::is_control)
}

fn valid_name(value: &str, limit: usize) -> bool {
    valid_text(value, limit) && value.trim().chars().count() >= 2
}

fn valid_code(value: &str, limit: usize) -> bool {
    let length = value.chars().count();
    length >= 2 && length <= limit && value.trim() == value && !value.chars().any(char::is_control)
}

fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn valid_work_context(value: &str) -> bool {
    value.len() == WORK_CONTEXT_LIMIT
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_date(value: &str) -> bool {
    if value.len() != 10 {
        return false;
    }
    let bytes = value.as_bytes();
    if !bytes
        .iter()
        .all(|byte| byte.is_ascii_digit() || *byte == b'-')
        || bytes[4] != b'-'
        || bytes[7] != b'-'
    {
        return false;
    }
    let Some(year) = value[0..4].parse::<u32>().ok() else {
        return false;
    };
    let Some(month) = value[5..7].parse::<u32>().ok() else {
        return false;
    };
    let Some(day) = value[8..10].parse::<u32>().ok() else {
        return false;
    };
    if !(1900..=9999).contains(&year) || !(1..=12).contains(&month) {
        return false;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=days).contains(&day)
}

fn valid_effective_to(effective_from: &str, effective_to: Option<&str>) -> bool {
    effective_to.is_none_or(|value| valid_date(value) && value >= effective_from)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BenefitPlanRow {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub provider: String,
    pub coverage_type: String,
    pub currency: String,
    pub employee_deduction_minor: i64,
    pub employer_cost_minor: i64,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub required_eligibility_code: Option<String>,
}

impl BenefitPlanRow {
    pub fn validate(&self, actor: &BenefitsActor) -> Result<(), BusinessError> {
        let eligibility = self
            .required_eligibility_code
            .as_deref()
            .is_none_or(|value| valid_code(value, ELIGIBILITY_LIMIT));
        if self.company_id != actor.company_id.as_str()
            || !valid_identifier(&self.id)
            || !valid_name(&self.name, NAME_LIMIT)
            || !valid_name(&self.provider, PROVIDER_LIMIT)
            || !matches!(self.coverage_type.as_str(), "employee" | "family")
            || !valid_currency(&self.currency)
            || self.employee_deduction_minor < 0
            || self.employer_cost_minor < 0
            || !valid_date(&self.effective_from)
            || !valid_effective_to(&self.effective_from, self.effective_to.as_deref())
            || !matches!(
                self.status.as_str(),
                "draft" | "published" | "rejected" | "retired"
            )
            || self.version < 1
            || !valid_identifier(&self.created_by)
            || self
                .reviewed_by
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || self.created_at < 0
            || self.updated_at < 0
            || !eligibility
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BenefitWindowRow {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub opens_on: String,
    pub closes_on: String,
    pub effective_on: String,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl BenefitWindowRow {
    pub fn validate(&self, actor: &BenefitsActor) -> Result<(), BusinessError> {
        if self.company_id != actor.company_id.as_str()
            || !valid_identifier(&self.id)
            || !valid_name(&self.name, NAME_LIMIT)
            || !valid_date(&self.opens_on)
            || !valid_date(&self.closes_on)
            || self.opens_on > self.closes_on
            || !valid_date(&self.effective_on)
            || !matches!(
                self.status.as_str(),
                "draft" | "open" | "closed" | "cancelled"
            )
            || self.version < 1
            || !valid_identifier(&self.created_by)
            || self
                .reviewed_by
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || self.created_at < 0
            || self.updated_at < 0
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BenefitDependentRow {
    pub id: String,
    pub company_id: String,
    pub person_id: String,
    pub name: String,
    pub relationship: String,
    pub birth_date: String,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl BenefitDependentRow {
    pub fn validate(
        &self,
        actor: &BenefitsActor,
        worker: Option<&BenefitsWorker>,
    ) -> Result<(), BusinessError> {
        if self.company_id != actor.company_id.as_str()
            || !valid_identifier(&self.id)
            || !valid_identifier(&self.person_id)
            || worker.is_none_or(|value| value.person_id != self.person_id)
            || !valid_name(&self.name, NAME_LIMIT)
            || !valid_name(&self.relationship, RELATIONSHIP_LIMIT)
            || !valid_date(&self.birth_date)
            || !matches!(
                self.status.as_str(),
                "pending" | "verified" | "rejected" | "removed"
            )
            || self.version < 1
            || !valid_identifier(&self.created_by)
            || self
                .reviewed_by
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || self.created_at < 0
            || self.updated_at < 0
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BenefitElectionRow {
    pub id: String,
    pub company_id: String,
    pub person_id: String,
    pub engagement_id: String,
    pub plan_id: String,
    pub plan_name: String,
    pub employee_name: String,
    pub plan_version: i64,
    pub window_id: String,
    pub window_version: i64,
    pub coverage_type: String,
    pub currency: String,
    pub employee_deduction_minor: i64,
    pub employer_cost_minor: i64,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub status: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl BenefitElectionRow {
    pub fn validate(
        &self,
        actor: &BenefitsActor,
        worker: Option<&BenefitsWorker>,
        reviewer: bool,
    ) -> Result<(), BusinessError> {
        if self.company_id != actor.company_id.as_str()
            || !valid_identifier(&self.id)
            || !valid_identifier(&self.person_id)
            || (!reviewer && worker.is_none_or(|value| value.person_id != self.person_id))
            || !valid_identifier(&self.engagement_id)
            || !valid_identifier(&self.plan_id)
            || !valid_name(&self.plan_name, NAME_LIMIT)
            || !valid_name(&self.employee_name, NAME_LIMIT)
            || self.plan_version < 1
            || !valid_identifier(&self.window_id)
            || self.window_version < 1
            || !matches!(self.coverage_type.as_str(), "employee" | "family")
            || !valid_currency(&self.currency)
            || self.employee_deduction_minor < 0
            || self.employer_cost_minor < 0
            || !valid_date(&self.effective_from)
            || !valid_effective_to(&self.effective_from, self.effective_to.as_deref())
            || !matches!(
                self.status.as_str(),
                "elected" | "submitted" | "acknowledged" | "released" | "cancelled"
            )
            || self.version < 1
            || self.created_at < 0
            || self.updated_at < 0
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenefitsWorkspace {
    pub actor_id: String,
    pub can_write: bool,
    pub can_review: bool,
    pub can_manage_documents: bool,
    pub work_context: String,
    pub plans: Vec<BenefitPlanRow>,
    pub windows: Vec<BenefitWindowRow>,
    pub dependents: Vec<BenefitDependentRow>,
    pub elections: Vec<BenefitElectionRow>,
    pub purchase: Option<BenefitsPurchaseControls>,
}

fn ordered<T>(rows: &[T], updated_at: impl Fn(&T) -> i64, id: impl Fn(&T) -> &str) -> bool {
    let mut previous: Option<(i64, &str)> = None;
    for row in rows {
        let current = (updated_at(row), id(row));
        if previous.as_ref().is_some_and(|value| {
            current.0 > value.0 || (current.0 == value.0 && current.1 <= value.1)
        }) {
            return false;
        }
        previous = Some(current);
    }
    true
}

fn unique(ids: impl Iterator<Item = String>) -> bool {
    let mut seen = BTreeSet::new();
    ids.into_iter().all(|value| seen.insert(value))
}

pub(crate) fn apply_employee_names(
    rows: &mut [BenefitElectionRow],
    names: Vec<(String, String)>,
) -> Result<(), BusinessError> {
    let mut requested = BTreeSet::new();
    for row in rows.iter() {
        requested.insert(row.person_id.clone());
    }
    if names.len() != requested.len() {
        return Err(source_invalid());
    }
    let mut resolved = std::collections::BTreeMap::new();
    for (person_id, name) in names {
        if !requested.contains(&person_id)
            || !valid_name(&name, NAME_LIMIT)
            || resolved.insert(person_id, name).is_some()
        {
            return Err(source_invalid());
        }
    }
    for row in rows.iter_mut() {
        let Some(name) = resolved.get(&row.person_id) else {
            return Err(source_invalid());
        };
        row.employee_name = name.clone();
    }
    Ok(())
}

pub(crate) fn validate_workspace(
    actor: &BenefitsActor,
    worker: Option<&BenefitsWorker>,
    reviewer: bool,
    plans: Vec<BenefitPlanRow>,
    windows: Vec<BenefitWindowRow>,
    dependents: Vec<BenefitDependentRow>,
    elections: Vec<BenefitElectionRow>,
) -> Result<BenefitsWorkspace, BusinessError> {
    if plans.len() > PLAN_LIMIT
        || windows.len() > WINDOW_LIMIT
        || dependents.len() > DEPENDENT_LIMIT
        || elections.len() > ELECTION_LIMIT
    {
        return Err(too_large());
    }
    if !ordered(&plans, |row| row.updated_at, |row| row.id.as_str())
        || !unique(plans.iter().map(|row| row.id.clone()))
        || !ordered(&windows, |row| row.updated_at, |row| row.id.as_str())
        || !unique(windows.iter().map(|row| row.id.clone()))
        || !ordered(&dependents, |row| row.updated_at, |row| row.id.as_str())
        || !unique(dependents.iter().map(|row| row.id.clone()))
        || !ordered(&elections, |row| row.updated_at, |row| row.id.as_str())
        || !unique(elections.iter().map(|row| row.id.clone()))
    {
        return Err(source_invalid());
    }
    for row in &plans {
        row.validate(actor)?;
    }
    for row in &windows {
        row.validate(actor)?;
    }
    for row in &dependents {
        row.validate(actor, worker)?;
    }
    for row in &elections {
        row.validate(actor, worker, reviewer)?;
    }
    Ok(BenefitsWorkspace {
        actor_id: actor.user_id.clone(),
        can_write: actor.can_write(),
        can_review: actor.can_review(),
        can_manage_documents: actor.can_manage_documents(),
        work_context: actor.work_context.clone(),
        plans,
        windows,
        dependents,
        elections,
        purchase: None,
    })
}
