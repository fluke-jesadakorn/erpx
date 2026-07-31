use dioxus::prelude::*;

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
    /// The BCP-47 language tag used by the document shell.
    pub const fn language_tag(self) -> &'static str {
        match self {
            Self::Th => "th",
            Self::En => "en",
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
    PlatformTenants,
    PlatformTenant,
    PlatformProjections,
    PlatformBackups,
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
    SupportModeTenantMutation,
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
            Self::PlatformTenants => "Tenants",
            Self::PlatformTenant => "Tenant",
            Self::PlatformProjections => "Projections",
            Self::PlatformBackups => "Backups",
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
            Self::SupportModeTenantMutation => "Tenant mutation",
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
            Self::PlatformTenants => "ผู้เช่า",
            Self::PlatformTenant => "ผู้เช่า",
            Self::PlatformProjections => "การฉายข้อมูล",
            Self::PlatformBackups => "การสำรองข้อมูล",
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
            Self::SupportModeTenantMutation => "แก้ไขข้อมูลผู้เช่า",
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

    pub(crate) const fn text(self, locale: Locale) -> &'static str {
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
    use super::{Locale, TranslationKey};

    #[test]
    fn locale_metadata_is_stable() {
        assert_eq!(Locale::En.language_tag(), "en");
        assert_eq!(Locale::Th.language_tag(), "th");
        assert_eq!(Locale::En.native_name(), "English");
        assert_eq!(Locale::Th.native_name(), "ไทย");
        assert_eq!(Locale::En.toggled(), Locale::Th);
        assert_eq!(Locale::Th.toggled(), Locale::En);
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
