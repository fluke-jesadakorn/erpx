//! Target-neutral HR and Payroll route, projection, and display contracts.
//!
//! This module is deliberately presentation-only. It accepts already-authorized scalar facts
//! from a future adapter, rejects structured or credential-bearing values, and exposes no
//! network, database, command, token, payroll-secret, or authorization implementation.

use std::{error::Error, fmt};

use crate::{
    Locale, NavItem, ProjectionFreshness, RouteAuthRequirement, TranslationKey, UiStateKind,
    redact_deep_link,
};

/// Maximum bytes retained for any opaque HR identifier.
pub const MAX_HR_IDENTIFIER_BYTES: usize = 128;
/// Maximum bytes retained for a scalar HR display value.
pub const MAX_HR_DISPLAY_VALUE_BYTES: usize = 255;
/// Maximum number of payroll lines retained in one display model.
pub const MAX_HR_PAYROLL_LINES: usize = 1_000;

/// Stable identifiers for the allow-listed HR route set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HrRouteId {
    /// HR landing page.
    Hr,
    /// Employee list.
    Employees,
    /// Leave requests.
    Leave,
    /// Expense claims.
    Expenses,
    /// Payroll runs.
    Payroll,
    /// Expired-role workflow reassignment view.
    Reassignments,
}

/// A target-neutral HR route definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HrRouteDefinition {
    /// Stable route identifier.
    pub id: HrRouteId,
    /// Canonical route template.
    pub template: &'static str,
    /// Existing localization key for the route label.
    pub label: TranslationKey,
    /// Authentication metadata mirrored for navigation UX.
    pub auth: RouteAuthRequirement,
    /// Static icon token interpreted by the consuming target theme.
    pub icon: &'static str,
}

impl HrRouteDefinition {
    /// Return the route label in the requested locale.
    pub const fn label_for(self, locale: Locale) -> &'static str {
        self.label.text(locale)
    }

    /// Build a target-neutral navigation item.
    pub fn nav_item(self, locale: Locale) -> NavItem {
        NavItem::new(self.label_for(locale), self.template, self.icon)
    }
}

/// Canonical HR route allow-list in stable display order.
pub const HR_ROUTES: &[HrRouteDefinition] = &[
    HrRouteDefinition {
        id: HrRouteId::Hr,
        template: "/hr",
        label: TranslationKey::Hr,
        auth: RouteAuthRequirement::Authenticated,
        icon: "♙",
    },
    HrRouteDefinition {
        id: HrRouteId::Employees,
        template: "/hr/employees",
        label: TranslationKey::HrEmployees,
        auth: RouteAuthRequirement::Authenticated,
        icon: "♟",
    },
    HrRouteDefinition {
        id: HrRouteId::Leave,
        template: "/hr/leave",
        label: TranslationKey::HrLeave,
        auth: RouteAuthRequirement::Authenticated,
        icon: "◷",
    },
    HrRouteDefinition {
        id: HrRouteId::Expenses,
        template: "/hr/expenses",
        label: TranslationKey::HrExpenses,
        auth: RouteAuthRequirement::Authenticated,
        icon: "▤",
    },
    HrRouteDefinition {
        id: HrRouteId::Payroll,
        template: "/hr/payroll",
        label: TranslationKey::HrPayroll,
        auth: RouteAuthRequirement::Authenticated,
        icon: "▥",
    },
    HrRouteDefinition {
        id: HrRouteId::Reassignments,
        template: "/hr/reassignments",
        label: TranslationKey::HrReassignments,
        auth: RouteAuthRequirement::Authenticated,
        icon: "⇄",
    },
];

/// Backwards-readable alias for callers that name the exported registry directly.
pub const HR_ROUTE_REGISTRY: &[HrRouteDefinition] = HR_ROUTES;

/// Cheap, copyable handle to the HR route registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HrRouteRegistry;

impl HrRouteRegistry {
    /// Construct the canonical HR registry.
    pub const fn new() -> Self {
        Self
    }

    /// Return every allow-listed HR route definition in canonical order.
    pub const fn definitions(self) -> &'static [HrRouteDefinition] {
        HR_ROUTES
    }

    /// Resolve a normalized or browser deep link into a typed HR route.
    pub fn resolve(self, path: &str) -> Result<HrRoute, HrRouteParseError> {
        HrRoute::parse(path)
    }

    /// Resolve a path to its stable route identifier.
    pub fn resolve_id(self, path: &str) -> Result<HrRouteId, HrRouteParseError> {
        self.resolve(path).map(|route| route.id())
    }
}

/// A parsed route from the HR allow-list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HrRoute {
    /// HR landing page.
    Hr,
    /// Employee list.
    Employees,
    /// Leave requests.
    Leave,
    /// Expense claims.
    Expenses,
    /// Payroll runs.
    Payroll,
    /// Reassignment list.
    Reassignments,
}

impl HrRoute {
    /// Parse a route after removing browser-only fragments and sensitive query material.
    pub fn parse(path: &str) -> Result<Self, HrRouteParseError> {
        let normalized = redact_deep_link(path);
        let route = match normalized.as_str() {
            "/hr" => Self::Hr,
            "/hr/employees" => Self::Employees,
            "/hr/leave" => Self::Leave,
            "/hr/expenses" => Self::Expenses,
            "/hr/payroll" => Self::Payroll,
            "/hr/reassignments" => Self::Reassignments,
            _ => return Err(HrRouteParseError::new(normalized)),
        };
        Ok(route)
    }

    /// Return the stable route identifier.
    pub const fn id(self) -> HrRouteId {
        match self {
            Self::Hr => HrRouteId::Hr,
            Self::Employees => HrRouteId::Employees,
            Self::Leave => HrRouteId::Leave,
            Self::Expenses => HrRouteId::Expenses,
            Self::Payroll => HrRouteId::Payroll,
            Self::Reassignments => HrRouteId::Reassignments,
        }
    }

    /// Return the canonical route path.
    pub const fn path(self) -> &'static str {
        match self {
            Self::Hr => "/hr",
            Self::Employees => "/hr/employees",
            Self::Leave => "/hr/leave",
            Self::Expenses => "/hr/expenses",
            Self::Payroll => "/hr/payroll",
            Self::Reassignments => "/hr/reassignments",
        }
    }
}

impl fmt::Display for HrRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.path())
    }
}

/// Error returned when a path is not in the HR allow-list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrRouteParseError {
    /// Normalized, non-query path that failed to resolve.
    pub normalized_path: String,
}

impl HrRouteParseError {
    fn new(normalized_path: String) -> Self {
        Self { normalized_path }
    }
}

impl fmt::Display for HrRouteParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "no ERPX HR route matches {}",
            self.normalized_path
        )
    }
}

impl Error for HrRouteParseError {}

/// Error returned for an unsafe opaque identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HrIdentifierError {
    /// The identifier is empty.
    Empty,
    /// The identifier exceeds its bounded size.
    TooLong,
    /// The identifier contains path, control, or whitespace characters.
    UnsafeCharacters,
    /// The identifier resembles sensitive transport material.
    SensitiveMarker,
}

impl fmt::Display for HrIdentifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "HR identifier is empty",
            Self::TooLong => "HR identifier is too long",
            Self::UnsafeCharacters => "HR identifier contains unsafe characters",
            Self::SensitiveMarker => "HR identifier contains a sensitive marker",
        })
    }
}

impl Error for HrIdentifierError {}

macro_rules! define_hr_identifier {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            /// Validate an opaque identifier without interpreting its identifier scheme.
            pub fn new(value: impl Into<String>) -> Result<Self, HrIdentifierError> {
                let value = value.into();
                validate_opaque_identifier(&value)?;
                Ok(Self(value))
            }

            /// Borrow the validated identifier.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<&str> for $name {
            type Error = HrIdentifierError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = HrIdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
    };
}

// Generic opaque identifier used for scope metadata.
define_hr_identifier!(HrOpaqueId);
// Opaque employee identifier.
define_hr_identifier!(HrEmployeeId);
// Opaque leave request identifier.
define_hr_identifier!(HrLeaveId);
// Opaque expense claim identifier.
define_hr_identifier!(HrExpenseId);
// Opaque payroll run identifier.
define_hr_identifier!(HrPayrollId);
// Opaque reassignment workflow reference.
define_hr_identifier!(HrWorkflowRef);
// Opaque principal reference used for redacted actor links.
define_hr_identifier!(HrPrincipalId);

/// Error returned for a display value that cannot cross the HR display boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HrDisplayValueError {
    /// The value is required but empty.
    Required,
    /// The value exceeds its bounded display size.
    TooLong,
    /// The value contains a control character.
    ControlCharacter,
    /// The value looks like an unstructured serialized payload.
    StructuredValue,
    /// The value contains a credential or sensitive-data marker.
    SensitiveMarker,
}

impl fmt::Display for HrDisplayValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Required => "HR display value is required",
            Self::TooLong => "HR display value is too long",
            Self::ControlCharacter => "HR display value contains a control character",
            Self::StructuredValue => "HR display value must be scalar",
            Self::SensitiveMarker => "HR display value contains a sensitive marker",
        })
    }
}

impl Error for HrDisplayValueError {}

/// Error returned when a currency code is not a bounded uppercase code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HrCurrencyError;

impl fmt::Display for HrCurrencyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HR currency must be a three-letter uppercase code")
    }
}

impl Error for HrCurrencyError {}

/// Error returned when a safe HR display model cannot be constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HrViewModelError {
    /// A typed identifier was invalid.
    InvalidIdentifier(HrIdentifierError),
    /// A scalar display value was invalid.
    InvalidDisplayValue(HrDisplayValueError),
    /// A currency code was invalid.
    InvalidCurrency,
    /// A monetary value, line number, or amount relationship was invalid.
    InvalidAmount,
    /// A payroll model exceeded the bounded line count.
    TooManyPayrollLines,
    /// Payroll line totals did not reconcile with the aggregate totals.
    InconsistentPayrollTotals,
}

impl fmt::Display for HrViewModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentifier(error) => error.fmt(formatter),
            Self::InvalidDisplayValue(error) => error.fmt(formatter),
            Self::InvalidCurrency => formatter.write_str("HR currency is invalid"),
            Self::InvalidAmount => formatter.write_str("HR amount or line number is invalid"),
            Self::TooManyPayrollLines => formatter.write_str("payroll has too many lines"),
            Self::InconsistentPayrollTotals => {
                formatter.write_str("payroll totals do not reconcile")
            }
        }
    }
}

impl Error for HrViewModelError {}

impl From<HrIdentifierError> for HrViewModelError {
    fn from(error: HrIdentifierError) -> Self {
        Self::InvalidIdentifier(error)
    }
}

impl From<HrDisplayValueError> for HrViewModelError {
    fn from(error: HrDisplayValueError) -> Self {
        Self::InvalidDisplayValue(error)
    }
}

/// A validated three-letter currency code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrCurrency(String);

impl HrCurrency {
    /// Validate and construct a currency code.
    pub fn new(value: impl Into<String>) -> Result<Self, HrCurrencyError> {
        let value = value.into();
        if value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(HrCurrencyError);
        }
        Ok(Self(value))
    }

    /// Borrow the validated currency code.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

macro_rules! define_hr_status {
    ($name:ident { $( $variant:ident => ($token:literal, $key:ident) ),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
        pub enum $name {
            $( $variant, )+
        }

        impl $name {
            /// Parse a bounded status token emitted by an authorized read adapter.
            pub fn from_transport(value: &str) -> Option<Self> {
                match value.trim().to_ascii_lowercase().as_str() {
                    $( $token => Some(Self::$variant), )+
                    _ => None,
                }
            }

            /// Return the stable transport token.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $( Self::$variant => $token, )+
                }
            }

            /// Return the localized lifecycle label.
            pub const fn label(self, locale: Locale) -> &'static str {
                let key = match self {
                    $( Self::$variant => TranslationKey::$key, )+
                };
                key.text(locale)
            }
        }
    };
}

define_hr_status!(HrEmployeeStatus {
    Pending => ("pending", HrEmployeeStatusPending),
    Active => ("active", HrEmployeeStatusActive),
    OnLeave => ("on_leave", HrEmployeeStatusOnLeave),
    Inactive => ("inactive", HrEmployeeStatusInactive),
    Terminated => ("terminated", HrEmployeeStatusTerminated),
});

define_hr_status!(HrLeaveStatus {
    Draft => ("draft", HrLeaveStatusDraft),
    Requested => ("requested", HrLeaveStatusRequested),
    Approved => ("approved", HrLeaveStatusApproved),
    Rejected => ("rejected", HrLeaveStatusRejected),
    Cancelled => ("cancelled", HrLeaveStatusCancelled),
    Taken => ("taken", HrLeaveStatusTaken),
});

define_hr_status!(HrExpenseStatus {
    Draft => ("draft", HrExpenseStatusDraft),
    Submitted => ("submitted", HrExpenseStatusSubmitted),
    Approved => ("approved", HrExpenseStatusApproved),
    Rejected => ("rejected", HrExpenseStatusRejected),
    Paid => ("paid", HrExpenseStatusPaid),
    Reimbursed => ("reimbursed", HrExpenseStatusReimbursed),
});

define_hr_status!(HrPayrollStatus {
    Draft => ("draft", HrPayrollStatusDraft),
    Prepared => ("prepared", HrPayrollStatusPrepared),
    Reviewed => ("reviewed", HrPayrollStatusReviewed),
    Approved => ("approved", HrPayrollStatusApproved),
    Posted => ("posted", HrPayrollStatusPosted),
    Paid => ("paid", HrPayrollStatusPaid),
    Voided => ("voided", HrPayrollStatusVoided),
});

define_hr_status!(HrReassignmentStatus {
    Pending => ("pending", HrReassignmentPending),
    Assigned => ("assigned", HrReassignmentAssigned),
    Completed => ("completed", HrReassignmentCompleted),
    Cancelled => ("cancelled", HrReassignmentCancelled),
});

/// Opaque holding/company/branch scope metadata for an HR projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrScope {
    holding_id: HrOpaqueId,
    company_id: Option<HrOpaqueId>,
    branch_id: Option<HrOpaqueId>,
}

impl HrScope {
    /// Construct a scope from opaque identifiers. Empty optional identifiers are omitted.
    pub fn new(
        holding_id: impl Into<String>,
        company_id: Option<String>,
        branch_id: Option<String>,
    ) -> Result<Self, HrIdentifierError> {
        let company_id = optional_opaque_id(company_id)?;
        let branch_id = optional_opaque_id(branch_id)?;
        Ok(Self {
            holding_id: HrOpaqueId::new(holding_id)?,
            company_id,
            branch_id,
        })
    }

    /// Borrow the opaque holding identifier.
    pub const fn holding_id(&self) -> &HrOpaqueId {
        &self.holding_id
    }

    /// Borrow the optional opaque company identifier.
    pub const fn company_id(&self) -> Option<&HrOpaqueId> {
        self.company_id.as_ref()
    }

    /// Borrow the optional opaque branch identifier.
    pub const fn branch_id(&self) -> Option<&HrOpaqueId> {
        self.branch_id.as_ref()
    }
}

/// Safe projection metadata shared by every HR page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrProjectionMetadata {
    /// Opaque holding/company/branch scope of the projection.
    pub scope: HrScope,
    /// Server-provided display timestamp, retained only as safe scalar text.
    pub projection_updated_at: Option<String>,
    /// Freshness classification supplied by the read boundary.
    pub freshness: ProjectionFreshness,
    /// Whether the current context is explicitly non-mutating.
    pub read_only: bool,
}

impl HrProjectionMetadata {
    /// Build metadata for a fresh projection.
    pub fn fresh(scope: HrScope, projection_updated_at: impl Into<String>) -> Self {
        Self::present(scope, projection_updated_at, ProjectionFreshness::Fresh)
    }

    /// Build metadata for a stale but displayable projection.
    pub fn stale(scope: HrScope, projection_updated_at: impl Into<String>) -> Self {
        Self::present(scope, projection_updated_at, ProjectionFreshness::Stale)
    }

    /// Build metadata when no projection timestamp is available.
    pub fn unknown(scope: HrScope) -> Self {
        Self {
            scope,
            projection_updated_at: None,
            freshness: ProjectionFreshness::Unknown,
            read_only: true,
        }
    }

    /// Build present metadata with an explicit freshness classification.
    pub fn present(
        scope: HrScope,
        projection_updated_at: impl Into<String>,
        freshness: ProjectionFreshness,
    ) -> Self {
        let projection_updated_at = safe_optional_scalar(Some(projection_updated_at.into()))
            .ok()
            .flatten();
        Self {
            freshness: if projection_updated_at.is_some() {
                freshness
            } else {
                ProjectionFreshness::Unknown
            },
            scope,
            projection_updated_at,
            read_only: true,
        }
    }

    /// Set the explicit read-only flag without changing freshness metadata.
    pub const fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Whether this projection is explicitly read-only.
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Whether this projection is outside its freshness budget.
    pub const fn is_stale(&self) -> bool {
        matches!(self.freshness, ProjectionFreshness::Stale)
    }

    /// Whether this projection is within its freshness budget.
    pub const fn is_fresh(&self) -> bool {
        matches!(self.freshness, ProjectionFreshness::Fresh)
    }
}

/// A redacted employee display model containing only scalar facts and opaque IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrEmployeeDisplayModel {
    employee_id: HrEmployeeId,
    employee_ref: String,
    display_name: String,
    status: HrEmployeeStatus,
    hired_on: String,
    terminated_on: Option<String>,
}

impl HrEmployeeDisplayModel {
    /// Construct an employee model from normalized scalar projection facts.
    pub fn new(
        employee_id: HrEmployeeId,
        employee_ref: impl Into<String>,
        display_name: impl Into<String>,
        status: HrEmployeeStatus,
        hired_on: impl Into<String>,
        terminated_on: Option<String>,
    ) -> Result<Self, HrViewModelError> {
        Ok(Self {
            employee_id,
            employee_ref: safe_scalar(employee_ref.into())?,
            display_name: safe_scalar(display_name.into())?,
            status,
            hired_on: safe_scalar(hired_on.into())?,
            terminated_on: safe_optional_scalar(terminated_on)?,
        })
    }

    /// Borrow the opaque employee identifier.
    pub const fn employee_id(&self) -> &HrEmployeeId {
        &self.employee_id
    }

    /// Borrow the safe employee reference.
    pub fn employee_ref(&self) -> &str {
        &self.employee_ref
    }

    /// Borrow the safe display name.
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Return the typed employment status.
    pub const fn status(&self) -> HrEmployeeStatus {
        self.status
    }

    /// Borrow the opaque/scalar hire date supplied by the projection.
    pub fn hired_on(&self) -> &str {
        &self.hired_on
    }

    /// Borrow the optional termination date.
    pub fn terminated_on(&self) -> Option<&str> {
        self.terminated_on.as_deref()
    }
}

/// A redacted leave-request display model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrLeaveDisplayModel {
    leave_id: HrLeaveId,
    employee_id: HrEmployeeId,
    starts_on: String,
    ends_on: String,
    reason: Option<String>,
    status: HrLeaveStatus,
    requested_by: HrPrincipalId,
    approved_by: Option<HrPrincipalId>,
}

impl HrLeaveDisplayModel {
    /// Construct a leave model from normalized scalar projection facts.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        leave_id: HrLeaveId,
        employee_id: HrEmployeeId,
        starts_on: impl Into<String>,
        ends_on: impl Into<String>,
        reason: Option<String>,
        status: HrLeaveStatus,
        requested_by: HrPrincipalId,
        approved_by: Option<HrPrincipalId>,
    ) -> Result<Self, HrViewModelError> {
        Ok(Self {
            leave_id,
            employee_id,
            starts_on: safe_scalar(starts_on.into())?,
            ends_on: safe_scalar(ends_on.into())?,
            reason: safe_optional_scalar(reason)?,
            status,
            requested_by,
            approved_by,
        })
    }

    /// Borrow the opaque leave identifier.
    pub const fn leave_id(&self) -> &HrLeaveId {
        &self.leave_id
    }

    /// Borrow the opaque employee identifier.
    pub const fn employee_id(&self) -> &HrEmployeeId {
        &self.employee_id
    }

    /// Borrow the start date text.
    pub fn starts_on(&self) -> &str {
        &self.starts_on
    }

    /// Borrow the end date text.
    pub fn ends_on(&self) -> &str {
        &self.ends_on
    }

    /// Borrow the optional safe reason.
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    /// Return the typed leave status.
    pub const fn status(&self) -> HrLeaveStatus {
        self.status
    }

    /// Borrow the opaque requester principal identifier.
    pub const fn requested_by(&self) -> &HrPrincipalId {
        &self.requested_by
    }

    /// Borrow the optional approver principal identifier.
    pub const fn approved_by(&self) -> Option<&HrPrincipalId> {
        self.approved_by.as_ref()
    }
}

/// A redacted expense-claim display model. It contains no receipt, account, or payment data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrExpenseDisplayModel {
    expense_id: HrExpenseId,
    employee_id: HrEmployeeId,
    currency: HrCurrency,
    amount_minor: i64,
    status: HrExpenseStatus,
    submitted_by: HrPrincipalId,
    approved_by: Option<HrPrincipalId>,
}

impl HrExpenseDisplayModel {
    /// Construct an expense model from normalized scalar projection facts.
    pub fn new(
        expense_id: HrExpenseId,
        employee_id: HrEmployeeId,
        currency: impl Into<String>,
        amount_minor: i64,
        status: HrExpenseStatus,
        submitted_by: HrPrincipalId,
        approved_by: Option<HrPrincipalId>,
    ) -> Result<Self, HrViewModelError> {
        if amount_minor < 0 {
            return Err(HrViewModelError::InvalidAmount);
        }
        Ok(Self {
            expense_id,
            employee_id,
            currency: HrCurrency::new(currency.into())
                .map_err(|_| HrViewModelError::InvalidCurrency)?,
            amount_minor,
            status,
            submitted_by,
            approved_by,
        })
    }

    /// Borrow the opaque expense identifier.
    pub const fn expense_id(&self) -> &HrExpenseId {
        &self.expense_id
    }

    /// Borrow the opaque employee identifier.
    pub const fn employee_id(&self) -> &HrEmployeeId {
        &self.employee_id
    }

    /// Borrow the validated currency.
    pub const fn currency(&self) -> &HrCurrency {
        &self.currency
    }

    /// Return the expense amount in minor currency units.
    pub const fn amount_minor(&self) -> i64 {
        self.amount_minor
    }

    /// Return the typed expense status.
    pub const fn status(&self) -> HrExpenseStatus {
        self.status
    }

    /// Borrow the opaque submitter principal identifier.
    pub const fn submitted_by(&self) -> &HrPrincipalId {
        &self.submitted_by
    }

    /// Borrow the optional approver principal identifier.
    pub const fn approved_by(&self) -> Option<&HrPrincipalId> {
        self.approved_by.as_ref()
    }
}

/// A redacted payroll line. It contains no bank, tax, account, or payment instrument fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrPayrollLineDisplayModel {
    line_no: u32,
    employee_id: HrEmployeeId,
    gross_minor: i64,
    deductions_minor: i64,
    net_minor: i64,
}

impl HrPayrollLineDisplayModel {
    /// Construct a payroll line from reconciled scalar amounts.
    pub fn new(
        line_no: u32,
        employee_id: HrEmployeeId,
        gross_minor: i64,
        deductions_minor: i64,
        net_minor: i64,
    ) -> Result<Self, HrViewModelError> {
        if line_no == 0
            || gross_minor < 0
            || deductions_minor < 0
            || net_minor < 0
            || deductions_minor > gross_minor
            || gross_minor - deductions_minor != net_minor
        {
            return Err(HrViewModelError::InvalidAmount);
        }
        Ok(Self {
            line_no,
            employee_id,
            gross_minor,
            deductions_minor,
            net_minor,
        })
    }

    /// Return the one-based payroll line number.
    pub const fn line_no(&self) -> u32 {
        self.line_no
    }

    /// Borrow the opaque employee identifier.
    pub const fn employee_id(&self) -> &HrEmployeeId {
        &self.employee_id
    }

    /// Return gross pay in minor currency units.
    pub const fn gross_minor(&self) -> i64 {
        self.gross_minor
    }

    /// Return deductions in minor currency units.
    pub const fn deductions_minor(&self) -> i64 {
        self.deductions_minor
    }

    /// Return net pay in minor currency units.
    pub const fn net_minor(&self) -> i64 {
        self.net_minor
    }
}

/// A redacted payroll-run display model containing only approved aggregate/line scalars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrPayrollDisplayModel {
    payroll_id: HrPayrollId,
    starts_on: String,
    ends_on: String,
    status: HrPayrollStatus,
    prepared_by: Option<HrPrincipalId>,
    reviewed_by: Option<HrPrincipalId>,
    approved_by: Option<HrPrincipalId>,
    total_gross_minor: i64,
    total_deductions_minor: i64,
    total_net_minor: i64,
    lines: Vec<HrPayrollLineDisplayModel>,
    lines_truncated: bool,
}

impl HrPayrollDisplayModel {
    /// Construct a payroll model from normalized aggregate and bounded line facts.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        payroll_id: HrPayrollId,
        starts_on: impl Into<String>,
        ends_on: impl Into<String>,
        status: HrPayrollStatus,
        prepared_by: Option<HrPrincipalId>,
        reviewed_by: Option<HrPrincipalId>,
        approved_by: Option<HrPrincipalId>,
        total_gross_minor: i64,
        total_deductions_minor: i64,
        total_net_minor: i64,
        lines: Vec<HrPayrollLineDisplayModel>,
        lines_truncated: bool,
    ) -> Result<Self, HrViewModelError> {
        if lines.len() > MAX_HR_PAYROLL_LINES
            || total_gross_minor < 0
            || total_deductions_minor < 0
            || total_net_minor < 0
            || total_deductions_minor > total_gross_minor
            || total_gross_minor - total_deductions_minor != total_net_minor
        {
            return Err(if lines.len() > MAX_HR_PAYROLL_LINES {
                HrViewModelError::TooManyPayrollLines
            } else {
                HrViewModelError::InvalidAmount
            });
        }
        if !lines_truncated && !lines.is_empty() {
            let gross = lines.iter().try_fold(0_i64, |total, line| {
                total
                    .checked_add(line.gross_minor)
                    .ok_or(HrViewModelError::InconsistentPayrollTotals)
            })?;
            let deductions = lines.iter().try_fold(0_i64, |total, line| {
                total
                    .checked_add(line.deductions_minor)
                    .ok_or(HrViewModelError::InconsistentPayrollTotals)
            })?;
            let net = lines.iter().try_fold(0_i64, |total, line| {
                total
                    .checked_add(line.net_minor)
                    .ok_or(HrViewModelError::InconsistentPayrollTotals)
            })?;
            if (gross, deductions, net)
                != (total_gross_minor, total_deductions_minor, total_net_minor)
            {
                return Err(HrViewModelError::InconsistentPayrollTotals);
            }
        }
        Ok(Self {
            payroll_id,
            starts_on: safe_scalar(starts_on.into())?,
            ends_on: safe_scalar(ends_on.into())?,
            status,
            prepared_by,
            reviewed_by,
            approved_by,
            total_gross_minor,
            total_deductions_minor,
            total_net_minor,
            lines,
            lines_truncated,
        })
    }

    /// Borrow the opaque payroll identifier.
    pub const fn payroll_id(&self) -> &HrPayrollId {
        &self.payroll_id
    }

    /// Borrow the payroll period start.
    pub fn starts_on(&self) -> &str {
        &self.starts_on
    }

    /// Borrow the payroll period end.
    pub fn ends_on(&self) -> &str {
        &self.ends_on
    }

    /// Return the typed payroll status.
    pub const fn status(&self) -> HrPayrollStatus {
        self.status
    }

    /// Borrow the optional preparer identifier.
    pub const fn prepared_by(&self) -> Option<&HrPrincipalId> {
        self.prepared_by.as_ref()
    }

    /// Borrow the optional reviewer identifier.
    pub const fn reviewed_by(&self) -> Option<&HrPrincipalId> {
        self.reviewed_by.as_ref()
    }

    /// Borrow the optional approver identifier.
    pub const fn approved_by(&self) -> Option<&HrPrincipalId> {
        self.approved_by.as_ref()
    }

    /// Return aggregate gross pay in minor currency units.
    pub const fn total_gross_minor(&self) -> i64 {
        self.total_gross_minor
    }

    /// Return aggregate deductions in minor currency units.
    pub const fn total_deductions_minor(&self) -> i64 {
        self.total_deductions_minor
    }

    /// Return aggregate net pay in minor currency units.
    pub const fn total_net_minor(&self) -> i64 {
        self.total_net_minor
    }

    /// Borrow bounded redacted payroll lines.
    pub fn lines(&self) -> &[HrPayrollLineDisplayModel] {
        &self.lines
    }

    /// Whether the source projection omitted additional lines.
    pub const fn lines_truncated(&self) -> bool {
        self.lines_truncated
    }
}

/// A redacted expired-role workflow reassignment display model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrReassignmentDisplayModel {
    workflow_ref: HrWorkflowRef,
    expired_principal_ref: HrPrincipalId,
    replacement_principal_ref: Option<HrPrincipalId>,
    reason: Option<String>,
    reassigned_at: String,
}

impl HrReassignmentDisplayModel {
    /// Construct a reassignment model from normalized scalar projection facts.
    pub fn new(
        workflow_ref: HrWorkflowRef,
        expired_principal_ref: HrPrincipalId,
        replacement_principal_ref: Option<HrPrincipalId>,
        reason: Option<String>,
        reassigned_at: impl Into<String>,
    ) -> Result<Self, HrViewModelError> {
        Ok(Self {
            workflow_ref,
            expired_principal_ref,
            replacement_principal_ref,
            reason: safe_optional_scalar(reason)?,
            reassigned_at: safe_scalar(reassigned_at.into())?,
        })
    }

    /// Borrow the opaque workflow reference.
    pub const fn workflow_ref(&self) -> &HrWorkflowRef {
        &self.workflow_ref
    }

    /// Borrow the expired principal reference.
    pub const fn expired_principal_ref(&self) -> &HrPrincipalId {
        &self.expired_principal_ref
    }

    /// Borrow the optional replacement principal reference.
    pub const fn replacement_principal_ref(&self) -> Option<&HrPrincipalId> {
        self.replacement_principal_ref.as_ref()
    }

    /// Borrow the optional safe workflow reason.
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    /// Borrow the reassignment timestamp text.
    pub fn reassigned_at(&self) -> &str {
        &self.reassigned_at
    }

    /// Derive the safe workflow status from whether a replacement is present.
    pub const fn status(&self) -> HrReassignmentStatus {
        if self.replacement_principal_ref.is_some() {
            HrReassignmentStatus::Assigned
        } else {
            HrReassignmentStatus::Pending
        }
    }
}

/// Explicit HR page state kinds, including the successful ready state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HrPageStateKind {
    /// A read is in progress.
    Loading,
    /// The read completed with no rows/content.
    Empty,
    /// Safe display data is available.
    Ready,
    /// The principal cannot view the page.
    Denied,
    /// Cached data is outside the freshness budget.
    Stale,
    /// The page is explicitly non-mutating.
    ReadOnly,
    /// A revision or write-boundary conflict needs review.
    Conflict,
    /// The backend is unavailable.
    Offline,
    /// An unexpected read failure occurred.
    Error,
}

impl HrPageStateKind {
    /// Map a non-ready HR state to the shared UI state kind.
    pub const fn shared_kind(self) -> Option<UiStateKind> {
        match self {
            Self::Loading => Some(UiStateKind::Loading),
            Self::Empty => Some(UiStateKind::Empty),
            Self::Ready => None,
            Self::Denied => Some(UiStateKind::Denied),
            Self::Stale => Some(UiStateKind::Stale),
            Self::ReadOnly => Some(UiStateKind::ReadOnly),
            Self::Conflict => Some(UiStateKind::Conflict),
            Self::Offline => Some(UiStateKind::Offline),
            Self::Error => Some(UiStateKind::Error),
        }
    }

    /// Return the localized label for a state kind.
    pub const fn label(self, locale: Locale) -> Option<&'static str> {
        let key = match self {
            Self::Loading => TranslationKey::Loading,
            Self::Empty => TranslationKey::Empty,
            Self::Ready => return None,
            Self::Denied => TranslationKey::AccessDenied,
            Self::Stale => TranslationKey::Stale,
            Self::ReadOnly => TranslationKey::ReadOnly,
            Self::Conflict => TranslationKey::Conflict,
            Self::Offline => TranslationKey::Offline,
            Self::Error => TranslationKey::Error,
        };
        Some(key.text(locale))
    }
}

/// Explicit HR page state for future authorized read adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HrPageState<T> {
    /// A read is in progress.
    Loading {
        /// Previously known metadata, if any.
        metadata: Option<HrProjectionMetadata>,
    },
    /// The read completed with no rows/content.
    Empty {
        /// Metadata accompanying the empty result.
        metadata: HrProjectionMetadata,
    },
    /// Current safe display data is available.
    Ready {
        /// Redacted display payload.
        data: T,
        /// Projection metadata.
        metadata: HrProjectionMetadata,
    },
    /// The principal cannot view this page.
    Denied {
        /// Metadata may be absent when denial precedes a read.
        metadata: Option<HrProjectionMetadata>,
        /// Safe policy/message text.
        reason: String,
    },
    /// Cached safe display data is outside the freshness budget.
    Stale {
        /// Last safe display payload.
        data: T,
        /// Stale projection metadata.
        metadata: HrProjectionMetadata,
        /// Age supplied by the read boundary.
        age_seconds: u64,
        /// Safe freshness explanation.
        reason: String,
    },
    /// Data is available but this presentation surface cannot mutate it.
    ReadOnly {
        /// Last safe display payload.
        data: T,
        /// Projection metadata.
        metadata: HrProjectionMetadata,
        /// Safe read-only explanation.
        reason: String,
    },
    /// A concurrent revision/write-boundary conflict needs review.
    Conflict {
        /// Last safe display payload, when available.
        data: Option<T>,
        /// Projection metadata, when available.
        metadata: Option<HrProjectionMetadata>,
        /// Safe conflict explanation.
        reason: String,
    },
    /// The backend is unavailable; cached safe data may remain visible.
    Offline {
        /// Cached safe display payload, when available.
        data: Option<T>,
        /// Cached projection metadata, when available.
        metadata: Option<HrProjectionMetadata>,
        /// Safe connectivity explanation.
        reason: String,
    },
    /// An unexpected read failure occurred.
    Error {
        /// Safe error explanation.
        reason: String,
    },
}

impl<T> HrPageState<T> {
    /// Return the explicit HR state kind.
    pub const fn state_kind(&self) -> HrPageStateKind {
        match self {
            Self::Loading { .. } => HrPageStateKind::Loading,
            Self::Empty { .. } => HrPageStateKind::Empty,
            Self::Ready { .. } => HrPageStateKind::Ready,
            Self::Denied { .. } => HrPageStateKind::Denied,
            Self::Stale { .. } => HrPageStateKind::Stale,
            Self::ReadOnly { .. } => HrPageStateKind::ReadOnly,
            Self::Conflict { .. } => HrPageStateKind::Conflict,
            Self::Offline { .. } => HrPageStateKind::Offline,
            Self::Error { .. } => HrPageStateKind::Error,
        }
    }

    /// Return the shared state kind for a non-ready state.
    pub const fn kind(&self) -> Option<UiStateKind> {
        self.state_kind().shared_kind()
    }

    /// Return the localized label for the current state.
    pub const fn state_label(&self, locale: Locale) -> Option<&'static str> {
        self.state_kind().label(locale)
    }

    /// Return safe current or cached display data, when available.
    pub fn data(&self) -> Option<&T> {
        match self {
            Self::Ready { data, .. } | Self::Stale { data, .. } | Self::ReadOnly { data, .. } => {
                Some(data)
            }
            Self::Conflict { data, .. } | Self::Offline { data, .. } => data.as_ref(),
            Self::Loading { .. }
            | Self::Empty { .. }
            | Self::Denied { .. }
            | Self::Error { .. } => None,
        }
    }

    /// Return projection metadata, if available.
    pub fn metadata(&self) -> Option<&HrProjectionMetadata> {
        match self {
            Self::Loading { metadata }
            | Self::Denied { metadata, .. }
            | Self::Conflict { metadata, .. }
            | Self::Offline { metadata, .. } => metadata.as_ref(),
            Self::Empty { metadata }
            | Self::Ready { metadata, .. }
            | Self::Stale { metadata, .. }
            | Self::ReadOnly { metadata, .. } => Some(metadata),
            Self::Error { .. } => None,
        }
    }

    /// Whether the state or projection metadata explicitly disables writes.
    pub fn is_read_only(&self) -> bool {
        matches!(self.state_kind(), HrPageStateKind::ReadOnly)
            || self
                .metadata()
                .is_some_and(HrProjectionMetadata::is_read_only)
    }

    /// This contract never exposes a mutation surface; authoritative writes remain elsewhere.
    pub const fn can_mutate(&self) -> bool {
        false
    }
}

/// Concrete state type for employee display models.
pub type HrEmployeeState = HrPageState<HrEmployeeDisplayModel>;
/// Concrete state type for leave display models.
pub type HrLeaveState = HrPageState<HrLeaveDisplayModel>;
/// Concrete state type for expense display models.
pub type HrExpenseState = HrPageState<HrExpenseDisplayModel>;
/// Concrete state type for payroll display models.
pub type HrPayrollState = HrPageState<HrPayrollDisplayModel>;
/// Concrete state type for reassignment display models.
pub type HrReassignmentState = HrPageState<HrReassignmentDisplayModel>;
/// Generic alias for consumers that use view-state terminology.
pub type HrViewState<T = HrEmployeeDisplayModel> = HrPageState<T>;
/// Generic alias for consumers that use state terminology.
pub type HrState<T = HrEmployeeDisplayModel> = HrPageState<T>;

/// Backwards-readable employee view-model name for consuming target adapters.
#[allow(dead_code)]
pub type HrEmployeeViewModel = HrEmployeeDisplayModel;
/// Backwards-readable leave view-model name for consuming target adapters.
#[allow(dead_code)]
pub type HrLeaveViewModel = HrLeaveDisplayModel;
/// Backwards-readable expense view-model name for consuming target adapters.
#[allow(dead_code)]
pub type HrExpenseViewModel = HrExpenseDisplayModel;
/// Backwards-readable payroll view-model name for consuming target adapters.
#[allow(dead_code)]
pub type HrPayrollViewModel = HrPayrollDisplayModel;
/// Backwards-readable payroll-run identifier name.
#[allow(dead_code)]
pub type HrPayrollRunId = HrPayrollId;
/// Backwards-readable reassignment identifier name.
#[allow(dead_code)]
pub type HrReassignmentId = HrWorkflowRef;
/// Backwards-readable reassignment view-model name.
#[allow(dead_code)]
pub type HrReassignmentViewModel = HrReassignmentDisplayModel;
/// Maximum rows a future target adapter should request for one HR page.
#[allow(dead_code)]
pub const MAX_HR_PAGE_ROWS: usize = 100;

fn validate_opaque_identifier(value: &str) -> Result<(), HrIdentifierError> {
    if value.is_empty() {
        return Err(HrIdentifierError::Empty);
    }
    if value.len() > MAX_HR_IDENTIFIER_BYTES {
        return Err(HrIdentifierError::TooLong);
    }
    if value == "."
        || value == ".."
        || value.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '/' | '\\' | '?' | '#')
        })
    {
        return Err(HrIdentifierError::UnsafeCharacters);
    }
    if contains_sensitive_marker(value) {
        return Err(HrIdentifierError::SensitiveMarker);
    }
    Ok(())
}

fn optional_opaque_id(value: Option<String>) -> Result<Option<HrOpaqueId>, HrIdentifierError> {
    match value {
        None => Ok(None),
        Some(value) if value.trim().is_empty() => Ok(None),
        Some(value) => HrOpaqueId::new(value).map(Some),
    }
}

fn safe_scalar(value: String) -> Result<String, HrDisplayValueError> {
    if value.trim().is_empty() {
        return Err(HrDisplayValueError::Required);
    }
    if value.len() > MAX_HR_DISPLAY_VALUE_BYTES {
        return Err(HrDisplayValueError::TooLong);
    }
    if value.chars().any(char::is_control) {
        return Err(HrDisplayValueError::ControlCharacter);
    }
    if value.contains(['{', '}', '[', ']']) {
        return Err(HrDisplayValueError::StructuredValue);
    }
    if contains_sensitive_marker(&value) {
        return Err(HrDisplayValueError::SensitiveMarker);
    }
    Ok(value)
}

fn safe_optional_scalar(value: Option<String>) -> Result<Option<String>, HrViewModelError> {
    match value {
        None => Ok(None),
        Some(value) if value.trim().is_empty() => Ok(None),
        Some(value) => safe_scalar(value).map(Some).map_err(Into::into),
    }
}

fn contains_sensitive_marker(value: &str) -> bool {
    const MARKERS: &[&str] = &[
        "access_token",
        "refresh_token",
        "id_token",
        "authorization",
        "bearer ",
        "client_secret",
        "password",
        "private_key",
        "api_key",
        "signed_url",
        "presigned_url",
        "payroll_secret",
        "bank_account",
        "payment_instrument",
        "national_id",
        "tax_id",
    ];
    let lower = value.to_ascii_lowercase();
    MARKERS.iter().any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> HrScope {
        HrScope::new(
            "holding-1",
            Some("company-1".to_string()),
            Some("branch-1".to_string()),
        )
        .expect("valid scope")
    }

    fn metadata() -> HrProjectionMetadata {
        HrProjectionMetadata::fresh(scope(), "2026-07-25T00:00:00Z")
    }

    fn employee() -> HrEmployeeDisplayModel {
        HrEmployeeDisplayModel::new(
            HrEmployeeId::new("employee-1").expect("valid employee id"),
            "EMP-001",
            "Somchai",
            HrEmployeeStatus::Active,
            "2026-01-01",
            None,
        )
        .expect("valid employee")
    }

    #[test]
    fn route_registry_is_allow_listed_and_bilingual() {
        let expected = [
            (HrRouteId::Hr, "/hr"),
            (HrRouteId::Employees, "/hr/employees"),
            (HrRouteId::Leave, "/hr/leave"),
            (HrRouteId::Expenses, "/hr/expenses"),
            (HrRouteId::Payroll, "/hr/payroll"),
            (HrRouteId::Reassignments, "/hr/reassignments"),
        ];
        let registry = HrRouteRegistry::new();
        assert_eq!(registry.definitions().len(), expected.len());
        for (definition, (id, path)) in registry.definitions().iter().zip(expected) {
            assert_eq!(definition.id, id);
            assert_eq!(definition.template, path);
            assert_eq!(definition.auth, RouteAuthRequirement::Authenticated);
            assert_ne!(
                definition.label_for(Locale::En),
                definition.label_for(Locale::Th)
            );
            assert_eq!(definition.nav_item(Locale::En).path, path);
            assert_eq!(registry.resolve_id(&format!("{path}/?tab=summary")), Ok(id));
        }
        assert_eq!(registry.resolve("/#/hr/payroll"), Ok(HrRoute::Payroll));
        assert!(registry.resolve("/hr/payroll/detail").is_err());
        assert!(registry.resolve("/hr/payroll?access_token=hidden").is_err());
    }

    #[test]
    fn opaque_ids_are_bounded_and_not_interpreted() {
        let id = HrEmployeeId::new("external-opaque-ref").expect("opaque IDs are supported");
        assert_eq!(id.as_str(), "external-opaque-ref");
        assert!(HrEmployeeId::new("").is_err());
        assert!(HrEmployeeId::new("employee/id").is_err());
        assert!(HrEmployeeId::new("employee id").is_err());
        assert!(HrEmployeeId::new("refresh_token-value").is_err());
        assert!(HrEmployeeId::new("x".repeat(MAX_HR_IDENTIFIER_BYTES + 1)).is_err());
    }

    #[test]
    fn all_explicit_page_states_remain_distinct_and_non_mutating() {
        let display = employee();
        let states = [
            HrEmployeeState::Loading {
                metadata: Some(metadata()),
            },
            HrEmployeeState::Empty {
                metadata: metadata(),
            },
            HrEmployeeState::Denied {
                metadata: None,
                reason: "policy".into(),
            },
            HrEmployeeState::Ready {
                data: display.clone(),
                metadata: metadata(),
            },
            HrEmployeeState::Stale {
                data: display.clone(),
                metadata: HrProjectionMetadata::stale(scope(), "2026-07-24T00:00:00Z"),
                age_seconds: 301,
                reason: "projection lag".into(),
            },
            HrEmployeeState::ReadOnly {
                data: display.clone(),
                metadata: metadata(),
                reason: "billing state".into(),
            },
            HrEmployeeState::Conflict {
                data: Some(display.clone()),
                metadata: Some(metadata()),
                reason: "revision changed".into(),
            },
            HrEmployeeState::Offline {
                data: Some(display),
                metadata: Some(metadata()),
                reason: "backend unavailable".into(),
            },
            HrEmployeeState::Error {
                reason: "backend failure".into(),
            },
        ];
        let kinds = states
            .iter()
            .map(HrPageState::state_kind)
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                HrPageStateKind::Loading,
                HrPageStateKind::Empty,
                HrPageStateKind::Denied,
                HrPageStateKind::Ready,
                HrPageStateKind::Stale,
                HrPageStateKind::ReadOnly,
                HrPageStateKind::Conflict,
                HrPageStateKind::Offline,
                HrPageStateKind::Error,
            ]
        );
        assert_eq!(states[0].kind(), Some(UiStateKind::Loading));
        assert!(states[3].data().is_some());
        assert!(states[4].is_read_only());
        assert!(states[5].is_read_only());
        assert!(states[6].data().is_some());
        assert!(states[7].data().is_some());
        assert!(states.iter().all(|state| !state.can_mutate()));
    }

    #[test]
    fn models_are_typed_scalar_only_and_reject_sensitive_payloads() {
        assert_ne!(
            HrEmployeeStatus::Active.label(Locale::En),
            HrEmployeeStatus::Active.label(Locale::Th)
        );
        assert_ne!(
            HrPayrollStatus::Approved.label(Locale::En),
            HrPayrollStatus::Approved.label(Locale::Th)
        );

        assert!(
            HrEmployeeDisplayModel::new(
                HrEmployeeId::new("employee-1").expect("valid id"),
                "EMP-001",
                "password=secret",
                HrEmployeeStatus::Active,
                "2026-01-01",
                None,
            )
            .is_err()
        );

        let line = HrPayrollLineDisplayModel::new(
            1,
            HrEmployeeId::new("employee-1").expect("valid id"),
            10_000,
            1_000,
            9_000,
        )
        .expect("valid payroll line");
        let payroll = HrPayrollDisplayModel::new(
            HrPayrollId::new("payroll-1").expect("valid payroll id"),
            "2026-01-01",
            "2026-01-31",
            HrPayrollStatus::Approved,
            None,
            None,
            None,
            10_000,
            1_000,
            9_000,
            vec![line],
            false,
        )
        .expect("valid payroll");
        assert_eq!(payroll.lines().len(), 1);
        assert!(!format!("{payroll:?}").contains("bank_account"));
        assert!(
            HrPayrollDisplayModel::new(
                HrPayrollId::new("payroll-1").expect("valid payroll id"),
                "2026-01-01",
                "2026-01-31",
                HrPayrollStatus::Approved,
                None,
                None,
                None,
                10_000,
                1_000,
                8_999,
                Vec::new(),
                true,
            )
            .is_err()
        );
    }
}
