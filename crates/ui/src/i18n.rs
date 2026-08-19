use chrono::{DateTime, Datelike, NaiveDate, Timelike};
use dioxus::prelude::*;

use crate::state::SafeUiText;

/// Supported client locales.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    /// Thai copy.
    Th,
    /// English copy.
    #[default]
    En,
}

impl Locale {
    /// Every locale supported by this client release.
    pub const ALL: [Self; 2] = [Self::En, Self::Th];

    /// The BCP-47 language tag used by the document shell.
    pub const fn language_tag(self) -> &'static str {
        match self {
            Self::Th => "th",
            Self::En => "en",
        }
    }

    /// Parse one persisted locale tag. Only the supported language subtag is
    /// authoritative; region and Unicode extensions are deliberately ignored.
    pub fn from_language_tag(value: &str) -> Option<Self> {
        let language = value.trim().split(['-', '_']).next().unwrap_or_default();
        if language.eq_ignore_ascii_case("en") {
            Some(Self::En)
        } else if language.eq_ignore_ascii_case("th") {
            Some(Self::Th)
        } else {
            None
        }
    }

    /// Parse the versioned persisted value. Storage accepts only the two
    /// canonical tags written by ERPX so malformed or future values cannot
    /// silently acquire meaning.
    pub fn from_stored_tag(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.eq_ignore_ascii_case("en") {
            Some(Self::En)
        } else if value.eq_ignore_ascii_case("th") {
            Some(Self::Th)
        } else {
            None
        }
    }

    /// Resolve a device-local preference. A valid stored value wins, then the
    /// device's ordered language list, with English as the stable fallback.
    pub fn resolve_preference<'a>(
        stored: Option<&str>,
        preferred_languages: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        if let Some(locale) = stored.and_then(Self::from_stored_tag) {
            return locale;
        }
        if preferred_languages
            .into_iter()
            .any(|language| Self::from_language_tag(language) == Some(Self::Th))
        {
            Self::Th
        } else {
            Self::En
        }
    }

    /// The locale's native display name.
    pub const fn native_name(self) -> &'static str {
        match self {
            Self::Th => "ไทย",
            Self::En => "English",
        }
    }

    /// Toggle between the supported locales.
    pub const fn toggled(self) -> Self {
        match self {
            Self::Th => Self::En,
            Self::En => Self::Th,
        }
    }

    /// Build deterministic locale-aware display formatting helpers.
    pub const fn formatter(self) -> LocaleFormatter {
        LocaleFormatter::new(self)
    }
}

/// A reviewable, compile-time English/Thai copy pair.
///
/// Product-specific copy can stay near its owning presentation module while
/// the constructor still makes omission of either supported locale impossible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalizedText {
    en: &'static str,
    th: &'static str,
}

impl LocalizedText {
    /// Maximum bytes accepted for one trusted presentation string.
    pub const MAX_BYTES: usize = 2_048;

    /// Create a bilingual copy pair.
    pub const fn new(en: &'static str, th: &'static str) -> Self {
        Self { en, th }
    }

    /// Select safe copy for the requested locale, falling back to English.
    pub const fn text(self, locale: Locale) -> &'static str {
        let selected = match locale {
            Locale::En => self.en,
            Locale::Th => self.th,
        };
        if valid_localized_copy(selected) {
            selected
        } else if valid_localized_copy(self.en) {
            self.en
        } else {
            "Unavailable"
        }
    }

    /// Return the English source copy for catalog verification.
    pub const fn english(self) -> &'static str {
        self.en
    }

    /// Return the Thai copy for catalog verification.
    pub const fn thai(self) -> &'static str {
        self.th
    }

    /// Verify both catalog entries without rendering either one.
    pub const fn is_valid(self) -> bool {
        valid_localized_copy(self.en) && valid_localized_copy(self.th)
    }
}

const fn valid_localized_copy(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > LocalizedText::MAX_BYTES {
        return false;
    }
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte < 0x20 || byte == 0x7f {
            return false;
        }
        index += 1;
    }
    !matches!(bytes[0], b' ' | b'\t') && !matches!(bytes[bytes.len() - 1], b' ' | b'\t')
}

/// Deterministic Gregorian presentation formatting for the active locale.
///
/// It intentionally does not consult the host timezone. RFC3339 timestamps
/// retain their supplied offset, command inputs remain untouched, and malformed
/// server display values pass through the existing safe-text boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocaleFormatter {
    locale: Locale,
}

impl LocaleFormatter {
    /// Create a formatter for one supported locale.
    pub const fn new(locale: Locale) -> Self {
        Self { locale }
    }

    /// Format an ISO Gregorian date (`YYYY-MM-DD`).
    pub fn format_date(self, value: &str) -> String {
        let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") else {
            return safe_passthrough(value);
        };
        format!(
            "{:02} {} {:04}",
            date.day(),
            month_name(self.locale, date.month()),
            date.year()
        )
    }

    /// Format an RFC3339 timestamp while retaining its original fixed offset.
    pub fn format_timestamp(self, value: &str) -> String {
        let Ok(timestamp) = DateTime::parse_from_rfc3339(value) else {
            return safe_passthrough(value);
        };
        let separator = match self.locale {
            Locale::En => ", ",
            Locale::Th => " ",
        };
        format!(
            "{:02} {} {:04}{separator}{:02}:{:02}:{:02} {}",
            timestamp.day(),
            month_name(self.locale, timestamp.month()),
            timestamp.year(),
            timestamp.hour(),
            timestamp.minute(),
            timestamp.second(),
            offset_label(timestamp.offset().local_minus_utc())
        )
    }

    /// Group an integer with ASCII digits and comma thousands separators.
    pub fn format_integer(self, value: i64) -> String {
        let _ = self;
        grouped_integer(value)
    }

    /// Format a read-only monetary amount supplied in minor units.
    ///
    /// ERPX currently presents THB and USD as two-decimal currencies. Unknown
    /// bounded codes are never assigned an invented exponent and stay labelled
    /// explicitly as minor units.
    pub fn format_minor_amount(self, currency: &str, amount_minor: i64) -> String {
        let currency = currency.trim();
        let currency = if !currency.is_empty()
            && currency.len() <= 12
            && currency
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            currency
        } else {
            match self.locale {
                Locale::En => "UNKNOWN",
                Locale::Th => "ไม่ทราบสกุล",
            }
        };
        if matches!(currency, "THB" | "USD") {
            let negative = amount_minor.is_negative();
            let absolute = i128::from(amount_minor).abs();
            let major = absolute / 100;
            let minor = absolute % 100;
            format!(
                "{currency} {}{}.{minor:02}",
                if negative { "-" } else { "" },
                grouped_unsigned(major as u128)
            )
        } else {
            let unit = match self.locale {
                Locale::En => "minor units",
                Locale::Th => "หน่วยย่อย",
            };
            format!("{currency} {} {unit}", grouped_integer(amount_minor))
        }
    }
}

fn safe_passthrough(value: &str) -> String {
    SafeUiText::new(value).as_str().to_owned()
}

const fn month_name(locale: Locale, month: u32) -> &'static str {
    match (locale, month) {
        (Locale::En, 1) => "Jan",
        (Locale::En, 2) => "Feb",
        (Locale::En, 3) => "Mar",
        (Locale::En, 4) => "Apr",
        (Locale::En, 5) => "May",
        (Locale::En, 6) => "Jun",
        (Locale::En, 7) => "Jul",
        (Locale::En, 8) => "Aug",
        (Locale::En, 9) => "Sep",
        (Locale::En, 10) => "Oct",
        (Locale::En, 11) => "Nov",
        (Locale::En, 12) => "Dec",
        (Locale::Th, 1) => "ม.ค.",
        (Locale::Th, 2) => "ก.พ.",
        (Locale::Th, 3) => "มี.ค.",
        (Locale::Th, 4) => "เม.ย.",
        (Locale::Th, 5) => "พ.ค.",
        (Locale::Th, 6) => "มิ.ย.",
        (Locale::Th, 7) => "ก.ค.",
        (Locale::Th, 8) => "ส.ค.",
        (Locale::Th, 9) => "ก.ย.",
        (Locale::Th, 10) => "ต.ค.",
        (Locale::Th, 11) => "พ.ย.",
        (Locale::Th, 12) => "ธ.ค.",
        _ => "?",
    }
}

fn offset_label(seconds: i32) -> String {
    if seconds == 0 {
        return "UTC".to_owned();
    }
    let sign = if seconds.is_negative() { '-' } else { '+' };
    let absolute = seconds.unsigned_abs();
    format!(
        "UTC{sign}{:02}:{:02}",
        absolute / 3_600,
        absolute % 3_600 / 60
    )
}

fn grouped_integer(value: i64) -> String {
    let negative = value.is_negative();
    let absolute = i128::from(value).unsigned_abs();
    format!(
        "{}{}",
        if negative { "-" } else { "" },
        grouped_unsigned(absolute)
    )
}

fn grouped_unsigned(value: u128) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.bytes().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(char::from(digit));
    }
    grouped
}

/// Stable keys for shell-level copy. Domain-specific wording stays in the consuming app until
/// the product localization catalog is frozen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslationKey {
    Language,
    Navigation,
    SwitchLanguage,
    Loading,
    Empty,
    AccessDenied,
    Stale,
    ReadOnly,
    Conflict,
    Offline,
    Error,
    Scope,
    Customer360Overview,
    Customer360Organization,
    Customer360Contacts,
    Customer360Commercial,
    Customer360Entitlements,
    Customer360Onboarding,
    Customer360Health,
    Customer360Support,
    Customer360AccessAudit,
    Customer360Notes,
    OperationsOverview,
    OperationsLeadToCash,
    OperationsProcureToPay,
    OperationsInventoryFlow,
    OperationsExceptions,
    OperationsCommitments,
    OperationsFulfillment,
    OperationsDispatch,
    OperationsCapacity,
    OperationsCalendar,
    OperationsShifts,
    OperationsHandover,
    OperationsSops,
    OperationsAnalytics,
    OperationsSourceSales,
    OperationsSourceProcurement,
    OperationsSourceInventory,
    OperationsSourceFinance,
    OperationsSourceMaintenance,
    OperationsSourceSupport,
    OperationsSourceIntegrations,
    PlatformHoldings,
    PlatformHolding,
    PlatformProjections,
    PlatformBackups,
    PlatformProviders,
    PlatformIntegrations,
    PlatformAudit,
    PlatformSettings,
    PlatformRoleReview,
    Marketing,
    MarketingProfiles,
    MarketingConsent,
    MarketingAudiences,
    MarketingCampaigns,
    MarketingCalendar,
    MarketingJourneys,
    MarketingContent,
    MarketingTemplates,
    MarketingLandingPages,
    MarketingForms,
    MarketingLeads,
    MarketingLeadScoring,
    MarketingBudget,
    MarketingAttribution,
    MarketingAnalytics,
    MarketingChannels,
    MarketingDomains,
    CrmAccounts,
    CrmContacts,
    CrmLeads,
    CrmOpportunities,
    CrmActivities,
    Support,
    SupportNew,
    SupportAccess,
    SupportDiagnostics,
    SupportSessions,
    Incidents,
    SupportApproval,
    SupportPlatformAccess,
    SupportModeDiagnosticRead,
    SupportModeHoldingMutation,
    SupportModeImpersonation,
    SupportStatusNew,
    SupportStatusTriaged,
    SupportStatusWaitingForCustomer,
    SupportStatusAccessRequested,
    SupportStatusInProgress,
    SupportStatusApproved,
    SupportStatusResolved,
    SupportStatusCustomerConfirmed,
    SupportStatusClosed,
    IncidentStatusInvestigating,
    IncidentStatusIdentified,
    IncidentStatusMonitoring,
    IncidentStatusResolved,
    IncidentStatusPostmortemComplete,
    SupportApprovalNotRequired,
    SupportApprovalPending,
    SupportApprovalApproved,
    SupportApprovalRejected,
    SupportApprovalExpired,
    SupportApprovalRevoked,
    Hr,
    HrEmployees,
    HrLeave,
    HrExpenses,
    HrPayroll,
    HrReassignments,
    HrEmployeeStatusPending,
    HrEmployeeStatusActive,
    HrEmployeeStatusOnLeave,
    HrEmployeeStatusInactive,
    HrEmployeeStatusTerminated,
    HrLeaveStatusDraft,
    HrLeaveStatusRequested,
    HrLeaveStatusApproved,
    HrLeaveStatusRejected,
    HrLeaveStatusCancelled,
    HrLeaveStatusTaken,
    HrExpenseStatusDraft,
    HrExpenseStatusSubmitted,
    HrExpenseStatusApproved,
    HrExpenseStatusRejected,
    HrExpenseStatusPaid,
    HrExpenseStatusReimbursed,
    HrPayrollStatusDraft,
    HrPayrollStatusPrepared,
    HrPayrollStatusReviewed,
    HrPayrollStatusApproved,
    HrPayrollStatusPosted,
    HrPayrollStatusPaid,
    HrPayrollStatusVoided,
    HrReassignmentPending,
    HrReassignmentAssigned,
    HrReassignmentCompleted,
    HrReassignmentCancelled,
    Pos,
    PosSales,
    PosSale,
    PosRegisters,
    PosReceipts,
    PosRefunds,
    PosShiftClose,
    PosStatusDraft,
    PosStatusOpen,
    PosStatusHeld,
    PosStatusCompleted,
    PosStatusVoided,
    PosStatusRefunded,
    PosPaymentCash,
    PosPaymentCard,
    PosPaymentBankTransfer,
    PosPaymentMobileWallet,
    PosPaymentVoucher,
    PosPaymentOther,
}

impl TranslationKey {
    /// Maximum UTF-8 byte length for a shell label.
    ///
    /// This leaves room for the longest Thai labels in the catalog while preventing an
    /// accidentally interpolated or unbounded value from becoming an accessible name.
    const MAX_LABEL_BYTES: usize = 128;

    const SAFE_LABEL_FALLBACK: &'static str = "Unavailable";

    const fn english(self) -> &'static str {
        match self {
            Self::Language => "Language",
            Self::Navigation => "Navigation",
            Self::SwitchLanguage => "Switch language",
            Self::Loading => "Loading",
            Self::Empty => "Nothing to show",
            Self::AccessDenied => "Access denied",
            Self::Stale => "Data may be stale",
            Self::ReadOnly => "Read-only mode",
            Self::Conflict => "Changes need review",
            Self::Offline => "Offline",
            Self::Error => "Error",
            Self::Scope => "Scope",
            Self::Customer360Overview => "Overview",
            Self::Customer360Organization => "Organization",
            Self::Customer360Contacts => "Contacts",
            Self::Customer360Commercial => "Commercial",
            Self::Customer360Entitlements => "Entitlements",
            Self::Customer360Onboarding => "Onboarding",
            Self::Customer360Health => "Health",
            Self::Customer360Support => "Support",
            Self::Customer360AccessAudit => "Access audit",
            Self::Customer360Notes => "Notes",
            Self::OperationsOverview => "Operations",
            Self::OperationsLeadToCash => "Lead to Cash",
            Self::OperationsProcureToPay => "Procure to Pay",
            Self::OperationsInventoryFlow => "Inventory Flow",
            Self::OperationsExceptions => "Exceptions",
            Self::OperationsCommitments => "Commitments",
            Self::OperationsFulfillment => "Fulfillment",
            Self::OperationsDispatch => "Dispatch",
            Self::OperationsCapacity => "Capacity",
            Self::OperationsCalendar => "Calendar",
            Self::OperationsShifts => "Shifts",
            Self::OperationsHandover => "Handover",
            Self::OperationsSops => "SOPs",
            Self::OperationsAnalytics => "Analytics",
            Self::OperationsSourceSales => "Sales",
            Self::OperationsSourceProcurement => "Procurement",
            Self::OperationsSourceInventory => "Inventory",
            Self::OperationsSourceFinance => "Finance",
            Self::OperationsSourceMaintenance => "Maintenance",
            Self::OperationsSourceSupport => "Support",
            Self::OperationsSourceIntegrations => "Integrations",
            Self::PlatformHoldings => "Holdings",
            Self::PlatformHolding => "Holding",
            Self::PlatformProjections => "Projections",
            Self::PlatformBackups => "Backups",
            Self::PlatformProviders => "Providers",
            Self::PlatformIntegrations => "Integrations",
            Self::PlatformAudit => "Audit",
            Self::PlatformSettings => "Settings",
            Self::PlatformRoleReview => "Role review",
            Self::Marketing => "Marketing",
            Self::MarketingProfiles => "Profiles",
            Self::MarketingConsent => "Consent",
            Self::MarketingAudiences => "Audiences",
            Self::MarketingCampaigns => "Campaigns",
            Self::MarketingCalendar => "Calendar",
            Self::MarketingJourneys => "Journeys",
            Self::MarketingContent => "Content",
            Self::MarketingTemplates => "Templates",
            Self::MarketingLandingPages => "Landing pages",
            Self::MarketingForms => "Forms",
            Self::MarketingLeads => "Leads",
            Self::MarketingLeadScoring => "Lead scoring",
            Self::MarketingBudget => "Budget",
            Self::MarketingAttribution => "Attribution",
            Self::MarketingAnalytics => "Analytics",
            Self::MarketingChannels => "Channels",
            Self::MarketingDomains => "Domains",
            Self::CrmAccounts => "Accounts",
            Self::CrmContacts => "Contacts",
            Self::CrmLeads => "Leads",
            Self::CrmOpportunities => "Opportunities",
            Self::CrmActivities => "Activities",
            Self::Support => "Support",
            Self::SupportNew => "New support request",
            Self::SupportAccess => "Support access",
            Self::SupportDiagnostics => "Diagnostics",
            Self::SupportSessions => "Support sessions",
            Self::Incidents => "Incidents",
            Self::SupportApproval => "Support access approval",
            Self::SupportPlatformAccess => "Platform access",
            Self::SupportModeDiagnosticRead => "Diagnostic read",
            Self::SupportModeHoldingMutation => "Holding mutation",
            Self::SupportModeImpersonation => "Impersonation",
            Self::SupportStatusNew => "New",
            Self::SupportStatusTriaged => "Triaged",
            Self::SupportStatusWaitingForCustomer => "Waiting for customer",
            Self::SupportStatusAccessRequested => "Access requested",
            Self::SupportStatusInProgress => "In progress",
            Self::SupportStatusApproved => "Approved",
            Self::SupportStatusResolved => "Resolved",
            Self::SupportStatusCustomerConfirmed => "Customer confirmed",
            Self::SupportStatusClosed => "Closed",
            Self::IncidentStatusInvestigating => "Investigating",
            Self::IncidentStatusIdentified => "Identified",
            Self::IncidentStatusMonitoring => "Monitoring",
            Self::IncidentStatusResolved => "Resolved",
            Self::IncidentStatusPostmortemComplete => "Postmortem complete",
            Self::SupportApprovalNotRequired => "Approval not required",
            Self::SupportApprovalPending => "Pending customer approval",
            Self::SupportApprovalApproved => "Customer approved",
            Self::SupportApprovalRejected => "Customer rejected",
            Self::SupportApprovalExpired => "Approval expired",
            Self::SupportApprovalRevoked => "Access revoked",
            Self::Hr => "Human resources",
            Self::HrEmployees => "Employees",
            Self::HrLeave => "Leave",
            Self::HrExpenses => "Expenses",
            Self::HrPayroll => "Payroll",
            Self::HrReassignments => "Reassignments",
            Self::HrEmployeeStatusPending => "Pending",
            Self::HrEmployeeStatusActive => "Active",
            Self::HrEmployeeStatusOnLeave => "On leave",
            Self::HrEmployeeStatusInactive => "Inactive",
            Self::HrEmployeeStatusTerminated => "Terminated",
            Self::HrLeaveStatusDraft => "Draft",
            Self::HrLeaveStatusRequested => "Requested",
            Self::HrLeaveStatusApproved => "Approved",
            Self::HrLeaveStatusRejected => "Rejected",
            Self::HrLeaveStatusCancelled => "Cancelled",
            Self::HrLeaveStatusTaken => "Taken",
            Self::HrExpenseStatusDraft => "Draft",
            Self::HrExpenseStatusSubmitted => "Submitted",
            Self::HrExpenseStatusApproved => "Approved",
            Self::HrExpenseStatusRejected => "Rejected",
            Self::HrExpenseStatusPaid => "Paid",
            Self::HrExpenseStatusReimbursed => "Reimbursed",
            Self::HrPayrollStatusDraft => "Draft",
            Self::HrPayrollStatusPrepared => "Prepared",
            Self::HrPayrollStatusReviewed => "Reviewed",
            Self::HrPayrollStatusApproved => "Approved",
            Self::HrPayrollStatusPosted => "Posted",
            Self::HrPayrollStatusPaid => "Paid",
            Self::HrPayrollStatusVoided => "Voided",
            Self::HrReassignmentPending => "Pending",
            Self::HrReassignmentAssigned => "Assigned",
            Self::HrReassignmentCompleted => "Completed",
            Self::HrReassignmentCancelled => "Cancelled",
            Self::Pos => "Point of sale",
            Self::PosSales => "Sales",
            Self::PosSale => "Sale",
            Self::PosRegisters => "Registers",
            Self::PosReceipts => "Receipts",
            Self::PosRefunds => "Refunds",
            Self::PosShiftClose => "Shift close",
            Self::PosStatusDraft => "Draft",
            Self::PosStatusOpen => "Open",
            Self::PosStatusHeld => "Held",
            Self::PosStatusCompleted => "Completed",
            Self::PosStatusVoided => "Voided",
            Self::PosStatusRefunded => "Refunded",
            Self::PosPaymentCash => "Cash",
            Self::PosPaymentCard => "Card",
            Self::PosPaymentBankTransfer => "Bank transfer",
            Self::PosPaymentMobileWallet => "Mobile wallet",
            Self::PosPaymentVoucher => "Voucher",
            Self::PosPaymentOther => "Other",
        }
    }

    const fn thai(self) -> &'static str {
        match self {
            Self::Language => "ภาษา",
            Self::Navigation => "เมนูนำทาง",
            Self::SwitchLanguage => "เปลี่ยนภาษา",
            Self::Loading => "กำลังโหลด",
            Self::Empty => "ไม่มีข้อมูลที่แสดง",
            Self::AccessDenied => "ไม่มีสิทธิ์เข้าถึง",
            Self::Stale => "ข้อมูลอาจไม่เป็นปัจจุบัน",
            Self::ReadOnly => "โหมดอ่านอย่างเดียว",
            Self::Conflict => "ต้องตรวจสอบการเปลี่ยนแปลง",
            Self::Offline => "ออฟไลน์",
            Self::Error => "ข้อผิดพลาด",
            Self::Scope => "ขอบเขต",
            Self::Customer360Overview => "ภาพรวม",
            Self::Customer360Organization => "องค์กร",
            Self::Customer360Contacts => "ผู้ติดต่อ",
            Self::Customer360Commercial => "การค้า",
            Self::Customer360Entitlements => "สิทธิ์การใช้งาน",
            Self::Customer360Onboarding => "การเริ่มใช้งาน",
            Self::Customer360Health => "สุขภาพระบบ",
            Self::Customer360Support => "การสนับสนุน",
            Self::Customer360AccessAudit => "ประวัติการเข้าถึง",
            Self::Customer360Notes => "บันทึก",
            Self::OperationsOverview => "การปฏิบัติการ",
            Self::OperationsLeadToCash => "ตั้งแต่โอกาสขายถึงรับเงิน",
            Self::OperationsProcureToPay => "ตั้งแต่จัดซื้อถึงชำระเงิน",
            Self::OperationsInventoryFlow => "การไหลของสินค้าคงคลัง",
            Self::OperationsExceptions => "ข้อยกเว้น",
            Self::OperationsCommitments => "ภาระผูกพัน",
            Self::OperationsFulfillment => "การจัดส่งตามแผน",
            Self::OperationsDispatch => "การจ่ายงาน",
            Self::OperationsCapacity => "ขีดความสามารถ",
            Self::OperationsCalendar => "ปฏิทินการปฏิบัติงาน",
            Self::OperationsShifts => "กะการทำงาน",
            Self::OperationsHandover => "การส่งต่องาน",
            Self::OperationsSops => "ขั้นตอนปฏิบัติงานมาตรฐาน",
            Self::OperationsAnalytics => "การวิเคราะห์",
            Self::OperationsSourceSales => "ฝ่ายขาย",
            Self::OperationsSourceProcurement => "จัดซื้อ",
            Self::OperationsSourceInventory => "สินค้าคงคลัง",
            Self::OperationsSourceFinance => "การเงิน",
            Self::OperationsSourceMaintenance => "ซ่อมบำรุง",
            Self::OperationsSourceSupport => "สนับสนุน",
            Self::OperationsSourceIntegrations => "การเชื่อมต่อระบบ",
            Self::PlatformHoldings => "ผู้เช่า",
            Self::PlatformHolding => "ผู้เช่า",
            Self::PlatformProjections => "การฉายข้อมูล",
            Self::PlatformBackups => "การสำรองข้อมูล",
            Self::PlatformProviders => "ผู้ให้บริการ",
            Self::PlatformIntegrations => "การเชื่อมต่อระบบ",
            Self::PlatformAudit => "การตรวจสอบ",
            Self::PlatformSettings => "การตั้งค่า",
            Self::PlatformRoleReview => "การทบทวนสิทธิ์",
            Self::Marketing => "การตลาด",
            Self::MarketingProfiles => "โปรไฟล์การตลาด",
            Self::MarketingConsent => "ความยินยอม",
            Self::MarketingAudiences => "กลุ่มเป้าหมาย",
            Self::MarketingCampaigns => "แคมเปญ",
            Self::MarketingCalendar => "ปฏิทิน",
            Self::MarketingJourneys => "เส้นทางลูกค้า",
            Self::MarketingContent => "เนื้อหา",
            Self::MarketingTemplates => "เทมเพลต",
            Self::MarketingLandingPages => "หน้า Landing Page",
            Self::MarketingForms => "แบบฟอร์ม",
            Self::MarketingLeads => "ลีด",
            Self::MarketingLeadScoring => "การให้คะแนนลีด",
            Self::MarketingBudget => "งบประมาณ",
            Self::MarketingAttribution => "การระบุแหล่งที่มา",
            Self::MarketingAnalytics => "การวิเคราะห์",
            Self::MarketingChannels => "ช่องทาง",
            Self::MarketingDomains => "โดเมน",
            Self::CrmAccounts => "บัญชีลูกค้า",
            Self::CrmContacts => "ผู้ติดต่อ",
            Self::CrmLeads => "ลีด CRM",
            Self::CrmOpportunities => "โอกาสการขาย",
            Self::CrmActivities => "กิจกรรม CRM",
            Self::Support => "การสนับสนุน",
            Self::SupportNew => "คำขอสนับสนุนใหม่",
            Self::SupportAccess => "สิทธิ์เข้าถึงเพื่อสนับสนุน",
            Self::SupportDiagnostics => "การวินิจฉัย",
            Self::SupportSessions => "เซสชันสนับสนุน",
            Self::Incidents => "เหตุขัดข้อง",
            Self::SupportApproval => "อนุมัติสิทธิ์สนับสนุน",
            Self::SupportPlatformAccess => "การเข้าถึงแพลตฟอร์ม",
            Self::SupportModeDiagnosticRead => "อ่านเพื่อวินิจฉัย",
            Self::SupportModeHoldingMutation => "แก้ไขข้อมูลผู้เช่า",
            Self::SupportModeImpersonation => "แสดงแทนผู้ใช้",
            Self::SupportStatusNew => "ใหม่",
            Self::SupportStatusTriaged => "คัดแยกแล้ว",
            Self::SupportStatusWaitingForCustomer => "รอลูกค้า",
            Self::SupportStatusAccessRequested => "ขอสิทธิ์เข้าถึงแล้ว",
            Self::SupportStatusInProgress => "กำลังดำเนินการ",
            Self::SupportStatusApproved => "อนุมัติแล้ว",
            Self::SupportStatusResolved => "แก้ไขแล้ว",
            Self::SupportStatusCustomerConfirmed => "ลูกค้ายืนยันแล้ว",
            Self::SupportStatusClosed => "ปิดแล้ว",
            Self::IncidentStatusInvestigating => "กำลังตรวจสอบ",
            Self::IncidentStatusIdentified => "ระบุสาเหตุและผลกระทบแล้ว",
            Self::IncidentStatusMonitoring => "กำลังติดตามผล",
            Self::IncidentStatusResolved => "แก้ไขเหตุขัดข้องแล้ว",
            Self::IncidentStatusPostmortemComplete => "เสร็จสิ้นการทบทวนเหตุการณ์",
            Self::SupportApprovalNotRequired => "ไม่ต้องอนุมัติ",
            Self::SupportApprovalPending => "รอการอนุมัติจากลูกค้า",
            Self::SupportApprovalApproved => "ลูกค้าอนุมัติแล้ว",
            Self::SupportApprovalRejected => "ลูกค้าปฏิเสธ",
            Self::SupportApprovalExpired => "การอนุมัติหมดอายุ",
            Self::SupportApprovalRevoked => "เพิกถอนสิทธิ์แล้ว",
            Self::Hr => "ทรัพยากรบุคคล",
            Self::HrEmployees => "พนักงาน",
            Self::HrLeave => "การลา",
            Self::HrExpenses => "ค่าใช้จ่าย",
            Self::HrPayroll => "เงินเดือน",
            Self::HrReassignments => "การมอบหมายงานใหม่",
            Self::HrEmployeeStatusPending => "รอดำเนินการ",
            Self::HrEmployeeStatusActive => "ปฏิบัติงานอยู่",
            Self::HrEmployeeStatusOnLeave => "อยู่ระหว่างลา",
            Self::HrEmployeeStatusInactive => "ไม่ใช้งาน",
            Self::HrEmployeeStatusTerminated => "สิ้นสุดการจ้างงาน",
            Self::HrLeaveStatusDraft => "ร่าง",
            Self::HrLeaveStatusRequested => "ยื่นคำขอแล้ว",
            Self::HrLeaveStatusApproved => "อนุมัติแล้ว",
            Self::HrLeaveStatusRejected => "ไม่อนุมัติ",
            Self::HrLeaveStatusCancelled => "ยกเลิกแล้ว",
            Self::HrLeaveStatusTaken => "ใช้สิทธิ์แล้ว",
            Self::HrExpenseStatusDraft => "ร่าง",
            Self::HrExpenseStatusSubmitted => "ส่งแล้ว",
            Self::HrExpenseStatusApproved => "อนุมัติแล้ว",
            Self::HrExpenseStatusRejected => "ไม่อนุมัติ",
            Self::HrExpenseStatusPaid => "จ่ายแล้ว",
            Self::HrExpenseStatusReimbursed => "เบิกคืนแล้ว",
            Self::HrPayrollStatusDraft => "ร่าง",
            Self::HrPayrollStatusPrepared => "จัดเตรียมแล้ว",
            Self::HrPayrollStatusReviewed => "ตรวจทานแล้ว",
            Self::HrPayrollStatusApproved => "อนุมัติแล้ว",
            Self::HrPayrollStatusPosted => "บันทึกบัญชีแล้ว",
            Self::HrPayrollStatusPaid => "จ่ายแล้ว",
            Self::HrPayrollStatusVoided => "ยกเลิกแล้ว",
            Self::HrReassignmentPending => "รอดำเนินการ",
            Self::HrReassignmentAssigned => "มอบหมายแล้ว",
            Self::HrReassignmentCompleted => "เสร็จสิ้น",
            Self::HrReassignmentCancelled => "ยกเลิกแล้ว",
            Self::Pos => "จุดขาย",
            Self::PosSales => "การขาย",
            Self::PosSale => "รายการขาย",
            Self::PosRegisters => "เครื่องบันทึกเงินสด",
            Self::PosReceipts => "ใบเสร็จ",
            Self::PosRefunds => "การคืนเงิน",
            Self::PosShiftClose => "ปิดกะ",
            Self::PosStatusDraft => "ร่าง",
            Self::PosStatusOpen => "เปิด",
            Self::PosStatusHeld => "พักไว้",
            Self::PosStatusCompleted => "เสร็จสิ้น",
            Self::PosStatusVoided => "ยกเลิกแล้ว",
            Self::PosStatusRefunded => "คืนเงินแล้ว",
            Self::PosPaymentCash => "เงินสด",
            Self::PosPaymentCard => "บัตร",
            Self::PosPaymentBankTransfer => "โอนเงินผ่านธนาคาร",
            Self::PosPaymentMobileWallet => "กระเป๋าเงินมือถือ",
            Self::PosPaymentVoucher => "บัตรกำนัล",
            Self::PosPaymentOther => "อื่นๆ",
        }
    }

    pub const fn text(self, locale: Locale) -> &'static str {
        let english = self.english();
        let localized = match locale {
            Locale::Th => self.thai(),
            Locale::En => english,
        };

        Self::bounded_accessible_label(localized, english)
    }

    /// Keep localized copy safe for visible and assistive labels.
    ///
    /// The catalog is statically typed, but this boundary still fails closed if a translation is
    /// later introduced with control characters, edge whitespace, or an excessive length. Thai
    /// text is UTF-8, so the bound is deliberately expressed in bytes rather than characters.
    const fn bounded_accessible_label<'a>(primary: &'a str, fallback: &'a str) -> &'a str {
        if Self::is_accessible_label(primary) {
            primary
        } else if Self::is_accessible_label(fallback) {
            fallback
        } else {
            Self::SAFE_LABEL_FALLBACK
        }
    }

    const fn is_accessible_label(label: &str) -> bool {
        let bytes = label.as_bytes();
        if bytes.is_empty() || bytes.len() > Self::MAX_LABEL_BYTES {
            return false;
        }

        let mut index = 0;
        while index < bytes.len() {
            let byte = bytes[index];
            if byte < 0x20 || byte == 0x7f {
                return false;
            }
            index += 1;
        }

        !matches!(bytes[0], b' ' | b'\t') && !matches!(bytes[bytes.len() - 1], b' ' | b'\t')
    }
}

/// Client-local locale state. It contains no credentials or server authority.
#[derive(Clone, Copy)]
pub struct LocaleContext {
    locale: Signal<Locale>,
}

impl LocaleContext {
    /// Create a locale context with the supplied initial locale.
    pub fn new(locale: Locale) -> Self {
        Self {
            locale: Signal::new(locale),
        }
    }

    /// Read the active locale and track it as a reactive dependency.
    pub fn current(self) -> Locale {
        *self.locale.read()
    }

    /// Change the active locale.
    pub fn set(mut self, locale: Locale) {
        self.locale.set(locale);
    }

    /// Translate a stable shell key.
    pub fn text(self, key: TranslationKey) -> &'static str {
        key.text(self.current())
    }

    /// Build deterministic display formatting helpers for the active locale.
    pub fn formatter(self) -> LocaleFormatter {
        self.current().formatter()
    }
}

/// Provide locale state to descendants. Call once near the application root.
pub fn provide_locale(locale: Locale) {
    use_context_provider(|| LocaleContext::new(locale));
}

/// Read locale state from the nearest provider.
pub fn use_locale() -> LocaleContext {
    use_context::<LocaleContext>()
}

#[cfg(test)]
mod tests {
    use super::{Locale, LocalizedText, TranslationKey};

    #[test]
    fn locale_metadata_is_stable() {
        assert_eq!(Locale::En.language_tag(), "en");
        assert_eq!(Locale::Th.language_tag(), "th");
        assert_eq!(Locale::En.native_name(), "English");
        assert_eq!(Locale::Th.native_name(), "ไทย");
        assert_eq!(Locale::En.toggled(), Locale::Th);
        assert_eq!(Locale::Th.toggled(), Locale::En);
        assert_eq!(Locale::from_language_tag("th-TH"), Some(Locale::Th));
        assert_eq!(Locale::from_language_tag("EN_us"), Some(Locale::En));
        assert_eq!(Locale::from_language_tag("this"), None);
        assert_eq!(Locale::from_stored_tag(" th "), Some(Locale::Th));
        assert_eq!(Locale::from_stored_tag("th-TH"), None);
        assert_eq!(
            Locale::resolve_preference(Some("th"), ["en-US"].into_iter()),
            Locale::Th
        );
        assert_eq!(
            Locale::resolve_preference(Some("invalid"), ["th-TH", "en"].into_iter()),
            Locale::Th
        );
        assert_eq!(
            Locale::resolve_preference(None, ["en-US", "th-TH"].into_iter()),
            Locale::Th
        );
        assert_eq!(
            Locale::resolve_preference(Some("th-TH"), ["en-US"].into_iter()),
            Locale::En
        );
        assert_eq!(
            Locale::resolve_preference(None, ["ja-JP"].into_iter()),
            Locale::En
        );
    }

    #[test]
    fn localized_text_requires_safe_copy_for_both_locales() {
        let copy = LocalizedText::new("Device preferences", "การตั้งค่าอุปกรณ์");
        assert!(copy.is_valid());
        assert_eq!(copy.text(Locale::En), "Device preferences");
        assert_eq!(copy.text(Locale::Th), "การตั้งค่าอุปกรณ์");
        assert_eq!(
            LocalizedText::new("Fallback", "\ninvalid").text(Locale::Th),
            "Fallback"
        );
    }

    #[test]
    fn locale_formatter_is_gregorian_offset_preserving_and_bounded() {
        let en = Locale::En.formatter();
        let th = Locale::Th.formatter();
        assert_eq!(en.format_date("2026-08-03"), "03 Aug 2026");
        assert_eq!(th.format_date("2026-08-03"), "03 ส.ค. 2026");
        assert_eq!(
            en.format_timestamp("2026-08-03T14:05:06+07:00"),
            "03 Aug 2026, 14:05:06 UTC+07:00"
        );
        assert_eq!(
            th.format_timestamp("2026-08-03T14:05:06+07:00"),
            "03 ส.ค. 2026 14:05:06 UTC+07:00"
        );
        assert_eq!(
            en.format_timestamp("2026-08-03T07:05:06Z"),
            "03 Aug 2026, 07:05:06 UTC"
        );
        assert_eq!(en.format_date("not-a-date"), "not-a-date");
        assert_eq!(en.format_date("password=unsafe"), "[redacted]");
    }

    #[test]
    fn locale_formatter_groups_numbers_and_does_not_invent_currency_exponents() {
        let en = Locale::En.formatter();
        let th = Locale::Th.formatter();
        assert_eq!(en.format_integer(0), "0");
        assert_eq!(en.format_integer(1_234_567), "1,234,567");
        assert_eq!(en.format_integer(i64::MIN), "-9,223,372,036,854,775,808");
        assert_eq!(en.format_minor_amount("THB", 125_050), "THB 1,250.50");
        assert_eq!(en.format_minor_amount("USD", -125), "USD -1.25");
        assert_eq!(en.format_minor_amount("JPY", 125), "JPY 125 minor units");
        assert_eq!(th.format_minor_amount("JPY", 125), "JPY 125 หน่วยย่อย");
        assert_eq!(
            en.format_minor_amount("password=unsafe", 125),
            "UNKNOWN 125 minor units"
        );
    }

    #[test]
    fn shell_translation_keys_have_both_locales() {
        assert_eq!(TranslationKey::Loading.english(), "Loading");
        assert_eq!(TranslationKey::Loading.thai(), "กำลังโหลด");
        assert_eq!(TranslationKey::Offline.english(), "Offline");
        assert_eq!(TranslationKey::Offline.thai(), "ออฟไลน์");
        assert_eq!(TranslationKey::Loading.text(Locale::En), "Loading");
        assert_eq!(TranslationKey::Loading.text(Locale::Th), "กำลังโหลด");
    }

    #[test]
    fn invalid_localized_copy_falls_back_to_safe_english() {
        assert_eq!(
            TranslationKey::bounded_accessible_label("\nเสียหาย", "Fallback"),
            "Fallback"
        );
        assert_eq!(
            TranslationKey::bounded_accessible_label("unsafe\u{7f}", "Fallback"),
            "Fallback"
        );
        assert_eq!(
            TranslationKey::bounded_accessible_label("  unsafe", "Fallback"),
            "Fallback"
        );
    }

    #[test]
    fn localized_copy_is_bounded_and_utf8_aware() {
        let too_long = "ก".repeat(TranslationKey::MAX_LABEL_BYTES);
        assert!(too_long.len() > TranslationKey::MAX_LABEL_BYTES);
        assert_eq!(
            TranslationKey::bounded_accessible_label(&too_long, "Fallback"),
            "Fallback"
        );
        assert_eq!(
            TranslationKey::bounded_accessible_label("ไทย", "Fallback"),
            "ไทย"
        );
    }

    #[test]
    fn invalid_fallback_uses_non_empty_safe_label() {
        assert_eq!(
            TranslationKey::bounded_accessible_label("\u{0}", "\t"),
            TranslationKey::SAFE_LABEL_FALLBACK
        );
    }
}
