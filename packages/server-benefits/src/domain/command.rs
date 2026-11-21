//! Native Benefits plan, window, dependent and election command policy.
//!
//! The application owns command ordering and current actor policy. This module
//! validates exact retained input shapes, Money minor units, dependent and
//! enrollment-window facts, the retained transition matrix, independent review
//! and optimistic versions. Repository SQL, current Account/IAM resolution,
//! Commercial purchase admission, Learning consumption, Files evidence binding
//! and provider payroll delivery remain explicit ports or composition
//! responsibilities; this domain never reads another owner's tables.
use crate::domain::workspace::{
    BenefitsWorker, js_trim, valid_code, valid_currency, valid_date, valid_identifier, valid_name,
    valid_text,
};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::{Serialize, Serializer};
use serde_json::Value;

const NAME_LIMIT: usize = 160;
const PROVIDER_LIMIT: usize = 160;
const RELATIONSHIP_LIMIT: usize = 80;
const ELIGIBILITY_LIMIT: usize = 120;
const REASON_LIMIT: usize = 3_000;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// Fresh command authority supplied by composition. It is not a request
/// deserializer and grants no module or permission by itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenefitsCommandActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub assignment_id: String,
    pub role: String,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub work_context: String,
    pub expires_at: i64,
}

impl BenefitsCommandActor {
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
            return Err(forbidden(
                "BENEFITS_PERMISSION_REQUIRED",
                "Your selected role does not grant Benefits read access.",
            ));
        }
        self.require_module()?;
        self.require_current(now)
    }

    pub fn require_write(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.can_write() {
            return Err(forbidden(
                "BENEFITS_PERMISSION_REQUIRED",
                "Your selected role does not grant Benefits write access.",
            ));
        }
        self.require_module()?;
        self.require_current(now)
    }

    pub fn require_review(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.can_review() {
            return Err(forbidden(
                "BENEFITS_REVIEW_REQUIRED",
                "Your selected role does not grant Benefits review access.",
            ));
        }
        self.require_module()?;
        self.require_current(now)
    }

    fn require_module(&self) -> Result<(), BusinessError> {
        if self.has_module() {
            Ok(())
        } else {
            Err(forbidden(
                "BENEFITS_MODULE_FORBIDDEN",
                "Your current role cannot access Benefits.",
            ))
        }
    }

    fn require_current(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !valid_identifier(self.user_id.as_str())
            || !valid_identifier(self.assignment_id.as_str())
            || !valid_text(&self.role, 120)
            || self.work_context.len() != 64
            || !self
                .work_context
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(unavailable());
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }

    pub fn authority_json(&self) -> String {
        serde_json::json!({
            "role": self.role,
            "assignmentId": self.assignment_id,
            "companyId": self.company_id.as_str(),
            "branchId": self.branch_id.as_ref().map(RecordId::as_str),
            "departmentId": self.department_id.as_ref().map(RecordId::as_str),
            "workContext": self.work_context,
        })
        .to_string()
    }
}

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

fn invalid(message: &'static str) -> BusinessError {
    BusinessError::invalid("BENEFITS_VALIDATION_FAILED", message)
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BENEFITS_SOURCE_INVALID",
        "The current Benefits source is invalid.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to a different Benefits request.",
    )
}

pub fn command_incomplete() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMAND_INCOMPLETE",
        "The previous Benefits command is still being finalized. Retry the same command.",
    )
}

fn object(value: &Value) -> Result<&serde_json::Map<String, Value>, BusinessError> {
    value
        .as_object()
        .ok_or_else(|| invalid("Review the Benefits request."))
}

fn string<'a>(object: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    object.get(key).and_then(Value::as_str)
}

fn required<'a>(
    object: &'a serde_json::Map<String, Value>,
    key: &str,
    message: &'static str,
) -> Result<&'a str, BusinessError> {
    string(object, key).ok_or_else(|| invalid(message))
}

fn optional<'a>(
    object: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<&'a str>, BusinessError> {
    match object.get(key) {
        Some(Value::Null) | None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(invalid("Review the Benefits request.")),
    }
}

/// Exact retained plan money shape: a nonnegative decimal with at most two
/// fractional digits, converted to integer minor units without floating point.
pub fn parse_money_minor(value: &str) -> Result<i64, BusinessError> {
    if value.is_empty() || value.trim() != value {
        return Err(invalid("Enter a nonnegative money amount."));
    }
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.is_some_and(|value| {
            value.is_empty() || value.len() > 2 || !value.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(invalid(
            "Enter a nonnegative money amount with at most two decimals.",
        ));
    }
    let whole = whole
        .parse::<i64>()
        .map_err(|_| invalid("Enter a nonnegative money amount."))?;
    let fraction = fraction.unwrap_or("0");
    let fraction = if fraction.len() == 1 {
        format!("{fraction}0")
    } else {
        fraction.to_owned()
    };
    let fraction = fraction
        .parse::<i64>()
        .map_err(|_| invalid("Enter a nonnegative money amount."))?;
    let minor = whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(fraction))
        .ok_or_else(|| invalid("The Benefits money amount is too large."))?;
    if minor > MAX_SAFE_INTEGER {
        return Err(invalid("The Benefits money amount is too large."));
    }
    Ok(minor)
}

#[derive(Clone, Debug, Serialize)]
pub struct BenefitPlanInput {
    pub name: String,
    pub provider: String,
    pub coverage_type: String,
    pub currency: String,
    pub employee_deduction_minor: i64,
    pub employer_cost_minor: i64,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub required_eligibility_code: Option<String>,
}

impl BenefitPlanInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let raw = object(raw)?;
        let name = js_trim(required(raw, "name", "Enter a plan name.")?);
        let provider = js_trim(required(raw, "provider", "Enter a plan provider.")?);
        let coverage_type = required(raw, "coverageType", "Choose employee or family coverage.")?;
        let currency =
            required(raw, "currency", "Use a three-letter ISO currency.")?.to_ascii_uppercase();
        let employee_deduction =
            required(raw, "employeeDeduction", "Enter the employee deduction.")?;
        let employer_cost = required(raw, "employerCost", "Enter the employer cost.")?;
        let effective_from = required(raw, "effectiveFrom", "Choose the effective start date.")?;
        let effective_to = optional(raw, "effectiveTo")?;
        let required_eligibility_code = optional(raw, "requiredEligibilityCode")?.map(js_trim);
        if !valid_name(name, NAME_LIMIT)
            || !valid_name(provider, PROVIDER_LIMIT)
            || !matches!(coverage_type, "employee" | "family")
            || !valid_currency(&currency)
            || !valid_date(effective_from)
            || effective_to.is_some_and(|value| !valid_date(value))
            || required_eligibility_code.is_some_and(|value| !valid_code(value, ELIGIBILITY_LIMIT))
        {
            return Err(invalid("Review the plan terms."));
        }
        if effective_to.is_some_and(|value| value < effective_from) {
            return Err(BusinessError::invalid(
                "INVALID_EFFECTIVE_RANGE",
                "Effective end must follow start.",
            ));
        }
        Ok(Self {
            name: name.to_owned(),
            provider: provider.to_owned(),
            coverage_type: coverage_type.to_owned(),
            currency,
            employee_deduction_minor: parse_money_minor(employee_deduction)?,
            employer_cost_minor: parse_money_minor(employer_cost)?,
            effective_from: effective_from.to_owned(),
            effective_to: effective_to.map(str::to_owned),
            required_eligibility_code: required_eligibility_code.map(str::to_owned),
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct BenefitWindowInput {
    pub name: String,
    pub opens_on: String,
    pub closes_on: String,
    pub effective_on: String,
}

impl BenefitWindowInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let raw = object(raw)?;
        let name = js_trim(required(raw, "name", "Enter an enrollment window name.")?);
        let opens_on = required(raw, "opensOn", "Choose the opening date.")?;
        let closes_on = required(raw, "closesOn", "Choose the closing date.")?;
        let effective_on = required(raw, "effectiveOn", "Choose the coverage effective date.")?;
        if !valid_name(name, NAME_LIMIT)
            || !valid_date(opens_on)
            || !valid_date(closes_on)
            || !valid_date(effective_on)
        {
            return Err(invalid("Review the enrollment window."));
        }
        if closes_on < opens_on {
            return Err(BusinessError::invalid(
                "INVALID_WINDOW",
                "Close date must follow open date.",
            ));
        }
        Ok(Self {
            name: name.to_owned(),
            opens_on: opens_on.to_owned(),
            closes_on: closes_on.to_owned(),
            effective_on: effective_on.to_owned(),
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct BenefitDependentInput {
    pub name: String,
    pub relationship: String,
    pub birth_date: String,
}

impl BenefitDependentInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let raw = object(raw)?;
        let name = js_trim(required(raw, "name", "Enter the dependent name.")?);
        let relationship = js_trim(required(raw, "relationship", "Enter the relationship.")?);
        let birth_date = required(raw, "birthDate", "Choose the birth date.")?;
        if !valid_name(name, NAME_LIMIT)
            || !valid_name(relationship, RELATIONSHIP_LIMIT)
            || !valid_date(birth_date)
        {
            return Err(invalid("Review the dependent details."));
        }
        Ok(Self {
            name: name.to_owned(),
            relationship: relationship.to_owned(),
            birth_date: birth_date.to_owned(),
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct BenefitElectionInput {
    pub plan_id: String,
    pub plan_version: i64,
    pub window_id: String,
    pub window_version: i64,
    #[serde(serialize_with = "serialize_strings")]
    pub dependent_ids: Vec<String>,
}

fn serialize_strings<S>(value: &[String], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    use serde::ser::SerializeSeq;
    let mut sequence = serializer.serialize_seq(Some(value.len()))?;
    for item in value {
        sequence.serialize_element(item)?;
    }
    sequence.end()
}

fn positive_integer(raw: &serde_json::Map<String, Value>, key: &str) -> Option<i64> {
    raw.get(key)
        .and_then(Value::as_i64)
        .filter(|value| (1..=MAX_SAFE_INTEGER).contains(value))
}

impl BenefitElectionInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let raw = object(raw)?;
        let plan_id = required(raw, "planId", "Choose a published plan.")?;
        let window_id = required(raw, "windowId", "Choose an open enrollment window.")?;
        let plan_version = positive_integer(raw, "planVersion")
            .ok_or_else(|| invalid("Choose the current plan version."))?;
        let window_version = positive_integer(raw, "windowVersion")
            .ok_or_else(|| invalid("Choose the current enrollment window version."))?;
        let dependent_ids = match raw.get("dependentIds") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(values)) => {
                if values.len() > 20 {
                    return Err(invalid("Choose at most twenty verified dependents."));
                }
                values
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .filter(|value| valid_identifier(value))
                            .map(str::to_owned)
                            .ok_or_else(|| invalid("Choose valid verified dependents."))
                    })
                    .collect::<Result<Vec<_>, _>>()?
            }
            Some(_) => return Err(invalid("Choose valid verified dependents.")),
        };
        if !valid_identifier(plan_id) || !valid_identifier(window_id) {
            return Err(invalid("Choose the current Benefits sources."));
        }
        Ok(Self {
            plan_id: plan_id.to_owned(),
            plan_version,
            window_id: window_id.to_owned(),
            window_version,
            dependent_ids,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BenefitRecordKind {
    Plan,
    Window,
    Dependent,
    Election,
}

impl BenefitRecordKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "plan" => Some(Self::Plan),
            "window" => Some(Self::Window),
            "dependent" => Some(Self::Dependent),
            "election" => Some(Self::Election),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Window => "window",
            Self::Dependent => "dependent",
            Self::Election => "election",
        }
    }

    pub const fn table(self) -> &'static str {
        match self {
            Self::Plan => "benefit_plans",
            Self::Window => "benefit_enrollment_windows",
            Self::Dependent => "benefit_dependents",
            Self::Election => "benefit_elections",
        }
    }

    pub const fn event_table(self) -> &'static str {
        match self {
            Self::Plan => "benefit_plan_events",
            Self::Window => "benefit_window_events",
            Self::Dependent => "benefit_dependent_events",
            Self::Election => "benefit_election_events",
        }
    }

    pub const fn id_column(self) -> &'static str {
        match self {
            Self::Plan => "plan_id",
            Self::Window => "window_id",
            Self::Dependent => "dependent_id",
            Self::Election => "election_id",
        }
    }

    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Plan => "benefits_plan",
            Self::Window => "benefits_window",
            Self::Dependent => "benefits_dependent",
            Self::Election => "benefits_election",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BenefitAction {
    Publish,
    Reject,
    Retire,
    Open,
    Close,
    Cancel,
    Verify,
    Remove,
    Submit,
    Acknowledge,
    Release,
}

impl BenefitAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "publish" => Some(Self::Publish),
            "reject" => Some(Self::Reject),
            "retire" => Some(Self::Retire),
            "open" => Some(Self::Open),
            "close" => Some(Self::Close),
            "cancel" => Some(Self::Cancel),
            "verify" => Some(Self::Verify),
            "remove" => Some(Self::Remove),
            "submit" => Some(Self::Submit),
            "acknowledge" => Some(Self::Acknowledge),
            "release" => Some(Self::Release),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Publish => "publish",
            Self::Reject => "reject",
            Self::Retire => "retire",
            Self::Open => "open",
            Self::Close => "close",
            Self::Cancel => "cancel",
            Self::Verify => "verify",
            Self::Remove => "remove",
            Self::Submit => "submit",
            Self::Acknowledge => "acknowledge",
            Self::Release => "release",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct BenefitTransitionInput {
    pub action: String,
    pub expected_version: i64,
    pub reason: String,
    pub provider_reference: Option<String>,
    #[serde(serialize_with = "serialize_strings")]
    pub attachment_set_ids: Vec<String>,
}

impl BenefitTransitionInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let raw = object(raw)?;
        let action = required(raw, "action", "Choose a Benefits action.")?;
        let expected_version = positive_integer(raw, "expectedVersion")
            .ok_or_else(|| invalid("Reload the current Benefits record."))?;
        let reason = required(raw, "reason", "Explain this Benefits action.")?;
        let reason = js_trim(reason);
        if !(3..=REASON_LIMIT).contains(&reason.encode_utf16().count())
            || !valid_text(reason, REASON_LIMIT)
        {
            return Err(invalid("Enter a reason between 3 and 3000 characters."));
        }
        let provider_reference = optional(raw, "providerReference")?;
        if provider_reference.is_some_and(|value| {
            let value = js_trim(value);
            !(2..=160).contains(&value.encode_utf16().count()) || !valid_text(value, PROVIDER_LIMIT)
        }) {
            return Err(invalid("Record the provider acknowledgement reference."));
        }
        let attachment_set_ids = match raw.get("attachmentSetIds") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(values)) if values.len() <= 20 => values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .filter(|value| valid_identifier(value))
                        .map(str::to_owned)
                        .ok_or_else(|| invalid("Choose valid Benefits evidence."))
                })
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => return Err(invalid("Choose at most twenty Benefits evidence sets.")),
        };
        if BenefitAction::parse(action).is_none() {
            return Err(invalid("Choose a registered Benefits action."));
        }
        Ok(Self {
            action: action.to_owned(),
            expected_version,
            reason: reason.to_owned(),
            provider_reference: provider_reference.map(js_trim).map(str::to_owned),
            attachment_set_ids,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct BenefitTransitionChanges {
    pub submitted_by: Option<String>,
    pub provider_reference: Option<String>,
    pub acknowledged_by: Option<String>,
    pub released_by: Option<String>,
    pub reviewed_by: Option<String>,
}

pub fn transition_changes(
    input: &BenefitTransitionInput,
    actor_id: &str,
) -> BenefitTransitionChanges {
    let action = BenefitAction::parse(&input.action);
    let reviewed = matches!(
        action,
        Some(
            BenefitAction::Publish
                | BenefitAction::Reject
                | BenefitAction::Retire
                | BenefitAction::Open
                | BenefitAction::Close
                | BenefitAction::Verify
                | BenefitAction::Remove
        )
    );
    BenefitTransitionChanges {
        submitted_by: (action == Some(BenefitAction::Submit)).then(|| actor_id.to_owned()),
        provider_reference: (action == Some(BenefitAction::Acknowledge))
            .then(|| input.provider_reference.clone())
            .flatten(),
        acknowledged_by: (action == Some(BenefitAction::Acknowledge)).then(|| actor_id.to_owned()),
        released_by: (action == Some(BenefitAction::Release)).then(|| actor_id.to_owned()),
        reviewed_by: reviewed.then(|| actor_id.to_owned()),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenefitRecordState {
    pub id: String,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub acknowledged_by: Option<String>,
    pub person_id: Option<String>,
    pub effective_from: Option<String>,
}

pub fn transition_state(
    kind: BenefitRecordKind,
    state: &BenefitRecordState,
    input: &BenefitTransitionInput,
) -> Result<(&'static str, &'static str), BusinessError> {
    let action = BenefitAction::parse(&input.action)
        .ok_or_else(|| invalid("Choose a registered Benefits action."))?;
    let rule = match (kind, action) {
        (BenefitRecordKind::Plan, BenefitAction::Publish) => Some(("draft", "published")),
        (BenefitRecordKind::Plan, BenefitAction::Reject) => Some(("draft", "rejected")),
        (BenefitRecordKind::Plan, BenefitAction::Retire) => Some(("published", "retired")),
        (BenefitRecordKind::Window, BenefitAction::Open) => Some(("draft", "open")),
        (BenefitRecordKind::Window, BenefitAction::Close) => Some(("open", "closed")),
        (BenefitRecordKind::Window, BenefitAction::Cancel) => Some(("draft", "cancelled")),
        (BenefitRecordKind::Dependent, BenefitAction::Verify) => Some(("pending", "verified")),
        (BenefitRecordKind::Dependent, BenefitAction::Reject) => Some(("pending", "rejected")),
        (BenefitRecordKind::Dependent, BenefitAction::Remove) => Some(("verified", "removed")),
        (BenefitRecordKind::Election, BenefitAction::Submit) => Some(("elected", "submitted")),
        (BenefitRecordKind::Election, BenefitAction::Acknowledge) => {
            Some(("submitted", "acknowledged"))
        }
        (BenefitRecordKind::Election, BenefitAction::Release) => Some(("acknowledged", "released")),
        (BenefitRecordKind::Election, BenefitAction::Cancel) => Some(("elected", "cancelled")),
        _ => None,
    }
    .ok_or_else(|| {
        BusinessError::new(
            FailureKind::Conflict,
            "INVALID_BENEFIT_TRANSITION",
            "This transition is not valid for the current status.",
        )
    })?;
    if state.status != rule.0 {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "INVALID_BENEFIT_TRANSITION",
            "This transition is not valid for the current status.",
        ));
    }
    if state.version != input.expected_version {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CONFLICT",
            "The record changed. Reload and retry.",
        ));
    }
    Ok(rule)
}

pub fn require_independent_review(
    state: &BenefitRecordState,
    input: &BenefitTransitionInput,
    actor_id: &str,
) -> Result<(), BusinessError> {
    let action = BenefitAction::parse(&input.action)
        .ok_or_else(|| invalid("Choose a registered Benefits action."))?;
    if matches!(
        action,
        BenefitAction::Publish
            | BenefitAction::Reject
            | BenefitAction::Open
            | BenefitAction::Verify
    ) && state.created_by == actor_id
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CONFLICT",
            "Independent review is required.",
        ));
    }
    if action == BenefitAction::Acknowledge && input.provider_reference.is_none() {
        return Err(BusinessError::invalid(
            "PROVIDER_REFERENCE_REQUIRED",
            "Record the provider acknowledgement reference.",
        ));
    }
    if action == BenefitAction::Release && state.acknowledged_by.as_deref() == Some(actor_id) {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CONFLICT",
            "Payroll release must be independent from provider acknowledgement.",
        ));
    }
    Ok(())
}

pub fn election_sources<'a>(
    plan: Option<&'a crate::domain::workspace::BenefitPlanRow>,
    window: Option<&'a crate::domain::workspace::BenefitWindowRow>,
    today: &str,
    input: &BenefitElectionInput,
) -> Result<
    (
        &'a crate::domain::workspace::BenefitPlanRow,
        &'a crate::domain::workspace::BenefitWindowRow,
    ),
    BusinessError,
> {
    let (plan, window) = plan.zip(window).ok_or_else(|| {
        BusinessError::new(
            FailureKind::Conflict,
            "BENEFIT_SOURCE_CHANGED",
            "Choose current published plan and open window versions.",
        )
    })?;
    if today < window.opens_on.as_str() || today > window.closes_on.as_str() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "ENROLLMENT_WINDOW_CLOSED",
            "The enrollment window is not active today.",
        ));
    }
    if plan.coverage_type == "family" && input.dependent_ids.is_empty() {
        return Err(BusinessError::invalid(
            "VERIFIED_DEPENDENT_REQUIRED",
            "Family coverage requires a verified dependent.",
        ));
    }
    Ok((plan, window))
}

pub fn require_verified_dependents(
    input: &BenefitElectionInput,
    verified_ids: &[String],
) -> Result<(), BusinessError> {
    let verified = verified_ids
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut selected = std::collections::BTreeSet::new();
    for id in &input.dependent_ids {
        if !selected.insert(id.clone()) || !verified.contains(id) {
            return Err(BusinessError::invalid(
                "VERIFIED_DEPENDENT_REQUIRED",
                "Every selected dependent must be verified.",
            ));
        }
    }
    Ok(())
}

pub fn require_submit_owner(
    worker: Option<&BenefitsWorker>,
    state: &BenefitRecordState,
) -> Result<(), BusinessError> {
    if worker.is_none_or(|worker| Some(worker.person_id.as_str()) != state.person_id.as_deref()) {
        return Err(forbidden(
            "BENEFIT_ELECTION_OWNER_REQUIRED",
            "Only the employee who owns this election can submit it.",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BenefitEvidencePolicy {
    pub target_type: &'static str,
    pub purpose: &'static str,
    pub component_ref: &'static str,
}

/// Retained evidence policy: only dependent verification and election
/// submission/provider acknowledgement bind Files evidence. Other actions
/// ignore `attachmentSetIds`, matching the TypeScript owner.
pub fn benefit_evidence_policy(
    kind: BenefitRecordKind,
    action: &str,
) -> Option<BenefitEvidencePolicy> {
    match (kind, action) {
        (BenefitRecordKind::Dependent, "verify") => Some(BenefitEvidencePolicy {
            target_type: "benefit_dependent",
            purpose: "dependent_proof",
            component_ref: "verification",
        }),
        (BenefitRecordKind::Election, "submit") => Some(BenefitEvidencePolicy {
            target_type: "benefit_election",
            purpose: "election_document",
            component_ref: "employee-submit",
        }),
        (BenefitRecordKind::Election, "acknowledge") => Some(BenefitEvidencePolicy {
            target_type: "benefit_election",
            purpose: "provider_proof",
            component_ref: "provider-acknowledgement",
        }),
        _ => None,
    }
}

pub fn benefit_evidence_binding_type(kind: BenefitRecordKind, action: &str) -> String {
    format!("benefits_{}_{}", kind.as_str(), action)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenefitEvidenceRequest {
    pub attachment_set_ids: Vec<String>,
    pub target_type: String,
    pub purpose: String,
    pub component_ref: String,
    pub target_id: String,
    pub expected_target_version: i64,
    pub command_id: String,
    pub binding_type: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenefitLearningRequest {
    pub company_id: String,
    pub person_id: String,
    pub required_code: String,
    pub effective_date: String,
    pub election_id: String,
    pub command_id: String,
    pub today: String,
}

/// Prepared Learning consumption receipt retained by the owner unit. The
/// Learning owner writes the receipt and re-reads it after the Benefits effect;
/// Benefits never writes a Learning row itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedBenefitLearning {
    pub certificate_id: String,
    pub election_id: String,
    pub command_id: String,
}

impl PreparedBenefitLearning {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_identifier(&self.certificate_id)
            || !valid_identifier(&self.election_id)
            || self.command_id.is_empty()
            || !valid_text(&self.command_id, 200)
        {
            return Err(unavailable());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenefitCommandResult {
    pub id: String,
    pub status: String,
    pub version: i64,
}

impl BenefitCommandResult {
    pub fn new(id: impl Into<String>, status: impl Into<String>, version: i64) -> Self {
        Self {
            id: id.into(),
            status: status.into(),
            version,
        }
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_identifier(&self.id)
            || !matches!(
                self.status.as_str(),
                "draft"
                    | "published"
                    | "rejected"
                    | "retired"
                    | "open"
                    | "closed"
                    | "cancelled"
                    | "pending"
                    | "verified"
                    | "removed"
                    | "elected"
                    | "submitted"
                    | "acknowledged"
                    | "released"
            )
            || self.version < 1
            || self.version > MAX_SAFE_INTEGER
        {
            return Err(unavailable());
        }
        Ok(())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[derive(Clone, Debug)]
pub struct StoredBenefitReceipt {
    pub request_hash: String,
    pub result: Option<BenefitCommandResult>,
}

pub fn validate_hash(value: &str) -> Result<(), BusinessError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(unavailable());
    }
    Ok(())
}

pub fn current_date(now: TimestampMillis) -> String {
    let seconds = now.get().div_euclid(1_000);
    let days = seconds.div_euclid(86_400);
    // Howard Hinnant's civil-from-days algorithm, kept exact without a timezone
    // dependency. The retained Benefits command uses the UTC calendar date.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[derive(Clone, Debug)]
pub struct CreatePlan<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub operation: &'a str,
    pub request_hash: &'a str,
    pub input: &'a BenefitPlanInput,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct CreateWindow<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub operation: &'a str,
    pub request_hash: &'a str,
    pub input: &'a BenefitWindowInput,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct CreateDependent<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub operation: &'a str,
    pub request_hash: &'a str,
    pub input: &'a BenefitDependentInput,
    pub person_id: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct CreateElection<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub operation: &'a str,
    pub request_hash: &'a str,
    pub input: &'a BenefitElectionInput,
    pub worker: &'a BenefitsWorker,
    pub plan: &'a crate::domain::workspace::BenefitPlanRow,
    pub window: &'a crate::domain::workspace::BenefitWindowRow,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ChangeBenefit<'a> {
    pub kind: BenefitRecordKind,
    pub id: &'a str,
    pub key: &'a str,
    pub operation: &'a str,
    pub request_hash: &'a str,
    pub input: &'a BenefitTransitionInput,
    pub from: &'a str,
    pub to: &'a str,
    pub changes: &'a BenefitTransitionChanges,
    pub release_payroll: bool,
    pub now: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::workspace::BenefitsEntitlement;

    #[test]
    fn exact_money_shapes_convert_without_float_rounding() {
        assert_eq!(parse_money_minor("125.50").unwrap(), 12_550);
        assert_eq!(parse_money_minor("125.5").unwrap(), 12_550);
        assert_eq!(parse_money_minor("0").unwrap(), 0);
        assert_eq!(parse_money_minor("1.00").unwrap(), 100);
        for value in ["", "-1", "1.234", ".5", "1.", " 1", "1 ", "1e2", "01."] {
            assert!(parse_money_minor(value).is_err(), "{value}");
        }
    }

    #[test]
    fn current_date_is_the_exact_utc_calendar_date() {
        assert_eq!(current_date(TimestampMillis::new(0)), "1970-01-01");
        assert_eq!(
            current_date(TimestampMillis::new(1_791_331_200_000)),
            "2026-10-07"
        );
        assert_eq!(
            current_date(TimestampMillis::new(1_791_417_600_000)),
            "2026-10-08"
        );
    }

    #[test]
    fn transition_matrix_and_independent_review_match_the_retained_owner() {
        let state = BenefitRecordState {
            id: "plan".into(),
            status: "draft".into(),
            version: 1,
            created_by: "author".into(),
            acknowledged_by: None,
            person_id: None,
            effective_from: None,
        };
        let input = BenefitTransitionInput {
            action: "publish".into(),
            expected_version: 1,
            reason: "Independent terms review".into(),
            provider_reference: None,
            attachment_set_ids: Vec::new(),
        };
        assert_eq!(
            transition_state(BenefitRecordKind::Plan, &state, &input).unwrap(),
            ("draft", "published")
        );
        assert!(require_independent_review(&state, &input, "author").is_err());
        assert!(require_independent_review(&state, &input, "reviewer").is_ok());
        assert!(
            transition_state(
                BenefitRecordKind::Plan,
                &BenefitRecordState {
                    version: 2,
                    ..state
                },
                &input
            )
            .is_err()
        );
    }

    #[test]
    fn family_election_requires_selected_verified_dependents() {
        let plan = crate::domain::workspace::BenefitPlanRow {
            id: "plan".into(),
            company_id: "company".into(),
            name: "Plan".into(),
            provider: "Provider".into(),
            coverage_type: "family".into(),
            currency: "THB".into(),
            employee_deduction_minor: 100,
            employer_cost_minor: 200,
            effective_from: "2026-01-01".into(),
            effective_to: None,
            status: "published".into(),
            version: 1,
            created_by: "owner".into(),
            reviewed_by: None,
            created_at: 1,
            updated_at: 1,
            required_eligibility_code: None,
        };
        let window = crate::domain::workspace::BenefitWindowRow {
            id: "window".into(),
            company_id: "company".into(),
            name: "Window".into(),
            opens_on: "2026-01-01".into(),
            closes_on: "2026-12-31".into(),
            effective_on: "2026-02-01".into(),
            status: "open".into(),
            version: 1,
            created_by: "owner".into(),
            reviewed_by: None,
            created_at: 1,
            updated_at: 1,
        };
        let input = BenefitElectionInput {
            plan_id: "plan".into(),
            plan_version: 1,
            window_id: "window".into(),
            window_version: 1,
            dependent_ids: Vec::new(),
        };
        assert_eq!(
            election_sources(Some(&plan), Some(&window), "2026-06-01", &input)
                .unwrap_err()
                .code,
            "VERIFIED_DEPENDENT_REQUIRED"
        );
        let with_dependent = BenefitElectionInput {
            dependent_ids: vec!["dependent".into()],
            ..input
        };
        assert!(require_verified_dependents(&with_dependent, &["dependent".into()]).is_ok());
        assert!(require_verified_dependents(&with_dependent, &["other".into()]).is_err());
    }

    #[test]
    fn entitlement_gates_new_work_and_settlement_separately() {
        let active = BenefitsEntitlement {
            enforced: true,
            history_allowed: true,
            new_work: true,
            settle_existing: true,
            revision: "1".into(),
        };
        let draining = BenefitsEntitlement {
            new_work: false,
            ..active.clone()
        };
        let inactive = BenefitsEntitlement {
            settle_existing: false,
            ..draining.clone()
        };
        assert!(active.require_new_work().is_ok());
        assert_eq!(
            draining.require_new_work().unwrap_err().code,
            "PACKAGE_DRAINING"
        );
        assert!(draining.require_settle_existing().is_ok());
        assert_eq!(
            inactive.require_settle_existing().unwrap_err().code,
            "PACKAGE_INACTIVE"
        );
    }

    #[test]
    fn actor_policy_rechecks_current_company_module_and_expiry() {
        let actor = BenefitsCommandActor {
            user_id: "owner".into(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: None,
            department_id: None,
            assignment_id: "assignment".into(),
            role: "owner".into(),
            permissions: vec!["records.read".into(), "records.write".into()],
            modules: vec!["benefits".into()],
            work_context: "a".repeat(64),
            expires_at: 2_000,
        };
        assert!(actor.require_write(TimestampMillis::new(1_000)).is_ok());
        assert_eq!(
            actor
                .require_review(TimestampMillis::new(1_000))
                .unwrap_err()
                .code,
            "BENEFITS_REVIEW_REQUIRED"
        );
        assert_eq!(
            actor
                .require_write(TimestampMillis::new(2_000))
                .unwrap_err()
                .code,
            "WORK_CONTEXT_CHANGED"
        );
    }
}
