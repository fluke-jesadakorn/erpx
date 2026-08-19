//! Shared presentation contracts for the current ERPX business workspaces.

use dioxus::prelude::*;
use erpx_ui_core::{
    EnterpriseState, EnterpriseStateKind, FeatureContext, Locale, LocalizedText, MutationMode,
    ReadModelFreshness, SafeUiText, normalize_deep_link,
};

use crate::kit::{
    Link as KitLink,
    button::{Button as KitButton, ButtonSize, ButtonVariant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentFeatureKind {
    IdentityAccess,
    PlatformAdministration,
    Organization,
    Commercial,
    Brand,
    CustomerAuthority,
    Customer360,
    Support,
    Finance,
    Sales,
    PointOfSale,
    Procurement,
    Inventory,
    Assets,
    Maintenance,
    Quality,
    Operations,
    PeoplePayroll,
    Legal,
    Marketing,
    Crm,
}

impl CurrentFeatureKind {
    pub const ALL: [Self; 21] = [
        Self::IdentityAccess,
        Self::PlatformAdministration,
        Self::Organization,
        Self::Commercial,
        Self::Brand,
        Self::CustomerAuthority,
        Self::Customer360,
        Self::Support,
        Self::Finance,
        Self::Sales,
        Self::PointOfSale,
        Self::Procurement,
        Self::Inventory,
        Self::Assets,
        Self::Maintenance,
        Self::Quality,
        Self::Operations,
        Self::PeoplePayroll,
        Self::Legal,
        Self::Marketing,
        Self::Crm,
    ];

    pub const fn root(self) -> &'static str {
        match self {
            Self::IdentityAccess => "/identity",
            Self::PlatformAdministration => "/platform",
            Self::Organization => "/organization",
            Self::Commercial => "/commercial",
            Self::Brand => "/brand",
            Self::CustomerAuthority => "/customer",
            Self::Customer360 => "/customer-360",
            Self::Support => "/support",
            Self::Finance => "/finance",
            Self::Sales => "/sales",
            Self::PointOfSale => "/pos",
            Self::Procurement => "/procurement",
            Self::Inventory => "/inventory",
            Self::Assets => "/asset",
            Self::Maintenance => "/maintenance",
            Self::Quality => "/quality",
            Self::Operations => "/operations",
            Self::PeoplePayroll => "/people",
            Self::Legal => "/legal",
            Self::Marketing => "/marketing",
            Self::Crm => "/crm",
        }
    }

    pub const fn label(self) -> LocalizedText {
        match self {
            Self::IdentityAccess => LocalizedText::new("Identity and access", "ข้อมูลประจำตัวและสิทธิ์"),
            Self::PlatformAdministration => {
                LocalizedText::new("Platform administration", "การดูแลแพลตฟอร์ม")
            }
            Self::Organization => LocalizedText::new("Organization", "องค์กร"),
            Self::Commercial => LocalizedText::new("Commercial governance", "การกำกับดูแลเชิงพาณิชย์"),
            Self::Brand => LocalizedText::new("Brand governance", "การกำกับดูแลแบรนด์"),
            Self::CustomerAuthority => LocalizedText::new("Customer authority", "ข้อมูลหลักลูกค้า"),
            Self::Customer360 => LocalizedText::new("Customer 360", "มุมมองลูกค้า 360"),
            Self::Support => {
                LocalizedText::new("Customer and platform support", "การสนับสนุนลูกค้าและแพลตฟอร์ม")
            }
            Self::Finance => LocalizedText::new("Finance", "การเงิน"),
            Self::Sales => LocalizedText::new("Sales", "การขาย"),
            Self::PointOfSale => LocalizedText::new("Point of sale", "จุดขาย"),
            Self::Procurement => LocalizedText::new("Procurement", "จัดซื้อ"),
            Self::Inventory => LocalizedText::new("Inventory", "สินค้าคงคลัง"),
            Self::Assets => LocalizedText::new("Assets", "สินทรัพย์"),
            Self::Maintenance => LocalizedText::new("Maintenance", "บำรุงรักษา"),
            Self::Quality => LocalizedText::new("Quality", "คุณภาพ"),
            Self::Operations => LocalizedText::new("Operations", "ปฏิบัติการ"),
            Self::PeoplePayroll => LocalizedText::new("People and payroll", "บุคลากรและเงินเดือน"),
            Self::Legal => LocalizedText::new("Legal", "กฎหมาย"),
            Self::Marketing => LocalizedText::new("Marketing", "การตลาด"),
            Self::Crm => LocalizedText::new("CRM", "CRM"),
        }
    }

    pub const fn record_families(self) -> &'static [CurrentRecordFamilySpec] {
        match self {
            Self::IdentityAccess => IDENTITY_FAMILIES,
            Self::PlatformAdministration => PLATFORM_FAMILIES,
            Self::Organization => ORGANIZATION_FAMILIES,
            Self::Commercial => COMMERCIAL_FAMILIES,
            Self::Brand => BRAND_FAMILIES,
            Self::CustomerAuthority => CUSTOMER_AUTHORITY_FAMILIES,
            Self::Customer360 => CUSTOMER_360_FAMILIES,
            Self::Support => SUPPORT_FAMILIES,
            Self::Finance => FINANCE_FAMILIES,
            Self::Sales => SALES_FAMILIES,
            Self::PointOfSale => POS_FAMILIES,
            Self::Procurement => PROCUREMENT_FAMILIES,
            Self::Inventory => INVENTORY_FAMILIES,
            Self::Assets => ASSET_FAMILIES,
            Self::Maintenance => MAINTENANCE_FAMILIES,
            Self::Quality => QUALITY_FAMILIES,
            Self::Operations => OPERATIONS_FAMILIES,
            Self::PeoplePayroll => PEOPLE_FAMILIES,
            Self::Legal => LEGAL_FAMILIES,
            Self::Marketing => MARKETING_FAMILIES,
            Self::Crm => CRM_FAMILIES,
        }
    }

    pub const fn e2e_scenarios(self) -> &'static [&'static str] {
        match self {
            Self::IdentityAccess => &["E2E-01", "E2E-03", "E2E-26"],
            Self::PlatformAdministration => &["E2E-01", "E2E-02", "E2E-13", "E2E-15"],
            Self::Organization => &["E2E-01", "E2E-02", "E2E-17"],
            Self::Commercial => &["E2E-01", "E2E-18"],
            Self::Brand => &["E2E-01", "E2E-17"],
            Self::CustomerAuthority | Self::Customer360 | Self::Support => &["E2E-04"],
            Self::Finance => &["E2E-06", "E2E-07", "E2E-10"],
            Self::Sales => &["E2E-06", "E2E-08"],
            Self::PointOfSale => &["E2E-11"],
            Self::Procurement => &["E2E-07"],
            Self::Inventory => &["E2E-07", "E2E-08"],
            Self::Assets | Self::Maintenance | Self::Quality | Self::Operations => &["E2E-09"],
            Self::PeoplePayroll => &["E2E-10", "E2E-26"],
            Self::Legal => &["E2E-12"],
            Self::Marketing => &["E2E-05", "E2E-06"],
            Self::Crm => &["E2E-06"],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordPublicationState {
    Live,
    TargetPending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentRecordFamilySpec {
    pub record_type: &'static str,
    pub route_root: &'static str,
    pub segment: &'static str,
    pub label: LocalizedText,
    pub lifecycle: &'static [&'static str],
    pub publication: RecordPublicationState,
}

impl CurrentRecordFamilySpec {
    pub fn collection_path(self) -> String {
        format!("{}/{}", self.route_root, self.segment)
    }

    pub fn detail_path(self, id: &str) -> Option<String> {
        valid_opaque_segment(id).then(|| format!("{}/{}/{}", self.route_root, self.segment, id))
    }
}

const GOVERNED: &[&str] = &[
    "proposed",
    "open",
    "in-progress",
    "waiting",
    "resolved",
    "closed",
    "cancelled",
];
const ACTIVE: &[&str] = &["draft", "active", "suspended", "retired"];
const TRANSACTION: &[&str] = &[
    "draft",
    "submitted",
    "approved",
    "posted",
    "completed",
    "cancelled",
];

macro_rules! live_family {
    ($record_type:literal, $root:literal, $segment:literal, $en:literal, $th:literal, $lifecycle:expr) => {
        CurrentRecordFamilySpec {
            record_type: $record_type,
            route_root: $root,
            segment: $segment,
            label: LocalizedText::new($en, $th),
            lifecycle: $lifecycle,
            publication: RecordPublicationState::Live,
        }
    };
}

pub const IDENTITY_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "identity.principal",
        "/identity",
        "principal",
        "Principals",
        "ผู้ใช้งานและตัวตน",
        ACTIVE
    ),
    live_family!(
        "identity.role",
        "/identity",
        "role",
        "Roles",
        "บทบาท",
        ACTIVE
    ),
    live_family!(
        "identity.grant",
        "/identity",
        "grant",
        "Access grants",
        "สิทธิ์การเข้าถึง",
        GOVERNED
    ),
    live_family!(
        "identity.access-review",
        "/identity",
        "access-review",
        "Access reviews",
        "การทบทวนสิทธิ์",
        GOVERNED
    ),
    live_family!(
        "identity.break-glass-session",
        "/identity",
        "break-glass-session",
        "Break-glass sessions",
        "เซสชันฉุกเฉิน",
        GOVERNED
    ),
];
pub const PLATFORM_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "platform.holding",
        "/platform",
        "holding",
        "Holdings",
        "ผู้เช่า",
        ACTIVE
    ),
    live_family!(
        "platform.provider",
        "/platform",
        "provider",
        "Providers",
        "ผู้ให้บริการ",
        ACTIVE
    ),
    live_family!(
        "platform.data-plane-operation",
        "/platform",
        "data-plane-operation",
        "Data-plane operations",
        "ปฏิบัติการดาต้าเพลน",
        GOVERNED
    ),
    live_family!(
        "platform.migration",
        "/platform",
        "migration",
        "Platform migrations",
        "การย้ายแพลตฟอร์ม",
        GOVERNED
    ),
    live_family!(
        "platform.release",
        "/platform",
        "release",
        "Platform releases",
        "รีลีสแพลตฟอร์ม",
        GOVERNED
    ),
];
pub const ORGANIZATION_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "organization.holding",
        "/organization",
        "holding",
        "Holdings",
        "กลุ่มบริษัท",
        ACTIVE
    ),
    live_family!(
        "organization.company",
        "/organization",
        "company",
        "Companies",
        "บริษัท",
        ACTIVE
    ),
    live_family!(
        "organization.branch",
        "/organization",
        "branch",
        "Branches",
        "สาขา",
        ACTIVE
    ),
    live_family!(
        "organization.sharing-agreement",
        "/organization",
        "sharing-agreement",
        "Sharing agreements",
        "ข้อตกลงการแบ่งปัน",
        GOVERNED
    ),
];
pub const COMMERCIAL_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "commercial.subscription",
        "/commercial",
        "subscription",
        "Subscriptions",
        "การสมัครใช้บริการ",
        TRANSACTION
    ),
    live_family!(
        "commercial.entitlement",
        "/commercial",
        "entitlement",
        "Entitlements",
        "สิทธิ์การใช้งาน",
        ACTIVE
    ),
    live_family!(
        "commercial.account",
        "/commercial",
        "account",
        "Commercial accounts",
        "บัญชีการค้า",
        ACTIVE
    ),
    live_family!(
        "commercial.adjustment",
        "/commercial",
        "adjustment",
        "Commercial adjustments",
        "รายการปรับปรุงการค้า",
        TRANSACTION
    ),
];
pub const BRAND_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "brand.profile",
        "/brand",
        "profile",
        "Brand profiles",
        "โปรไฟล์แบรนด์",
        ACTIVE
    ),
    live_family!(
        "brand.asset",
        "/brand",
        "asset",
        "Brand assets",
        "สินทรัพย์แบรนด์",
        ACTIVE
    ),
    live_family!(
        "brand.publication",
        "/brand",
        "publication",
        "Brand publications",
        "การเผยแพร่แบรนด์",
        GOVERNED
    ),
];
pub const CUSTOMER_AUTHORITY_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "customer.account",
        "/customer",
        "account",
        "Customer accounts",
        "บัญชีลูกค้า",
        ACTIVE
    ),
    live_family!(
        "customer.authority-grant",
        "/customer",
        "authority-grant",
        "Customer authority grants",
        "สิทธิอำนาจข้อมูลลูกค้า",
        GOVERNED
    ),
    live_family!(
        "customer.support-access",
        "/customer",
        "support-access",
        "Customer support access",
        "สิทธิ์เข้าถึงเพื่อบริการลูกค้า",
        GOVERNED
    ),
];
pub const CUSTOMER_360_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "customer-360.profile",
        "/customer-360",
        "profile",
        "Customer profiles",
        "โปรไฟล์ลูกค้า",
        ACTIVE
    ),
    live_family!(
        "customer-360.note",
        "/customer-360",
        "note",
        "Customer notes",
        "บันทึกลูกค้า",
        GOVERNED
    ),
    live_family!(
        "customer-360.merge",
        "/customer-360",
        "merge",
        "Customer merge cases",
        "กรณีรวมข้อมูลลูกค้า",
        GOVERNED
    ),
];
pub const SUPPORT_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "support.customer-case",
        "/support",
        "customer-case",
        "Customer cases",
        "เคสลูกค้า",
        GOVERNED
    ),
    live_family!(
        "support.incident",
        "/support",
        "incident",
        "Incidents",
        "เหตุขัดข้อง",
        GOVERNED
    ),
    live_family!(
        "support.access-session",
        "/support",
        "access-session",
        "Approved access sessions",
        "เซสชันเข้าถึงที่อนุมัติ",
        TRANSACTION
    ),
    live_family!(
        "support.platform-case",
        "/support",
        "platform-case",
        "Platform support cases",
        "เคสสนับสนุนแพลตฟอร์ม",
        GOVERNED
    ),
];
pub const FINANCE_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "finance.journal",
        "/finance",
        "journal",
        "Journals",
        "สมุดรายวัน",
        TRANSACTION
    ),
    live_family!(
        "finance.invoice",
        "/finance",
        "invoice",
        "Invoices",
        "ใบแจ้งหนี้",
        TRANSACTION
    ),
    live_family!(
        "finance.close-cycle",
        "/finance",
        "close-cycle",
        "Close cycles",
        "รอบปิดบัญชี",
        GOVERNED
    ),
    live_family!(
        "finance.credit-note",
        "/finance",
        "credit-note",
        "Credit notes",
        "ใบลดหนี้",
        TRANSACTION
    ),
    live_family!(
        "finance.debit-note",
        "/finance",
        "debit-note",
        "Debit notes",
        "ใบเพิ่มหนี้",
        TRANSACTION
    ),
    live_family!(
        "finance.payment",
        "/finance",
        "payment",
        "Payments",
        "การชำระเงิน",
        TRANSACTION
    ),
    live_family!(
        "finance.reconciliation",
        "/finance",
        "reconciliation",
        "Reconciliations",
        "การกระทบยอด",
        GOVERNED
    ),
];
pub const SALES_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "sales.quote",
        "/sales",
        "quote",
        "Quotes",
        "ใบเสนอราคา",
        TRANSACTION
    ),
    live_family!(
        "sales.order",
        "/sales",
        "order",
        "Sales orders",
        "คำสั่งขาย",
        TRANSACTION
    ),
    live_family!(
        "sales.return",
        "/sales",
        "return",
        "Sales returns",
        "การคืนสินค้าขาย",
        TRANSACTION
    ),
    live_family!(
        "sales.fulfillment-handoff",
        "/sales",
        "fulfillment-handoff",
        "Fulfillment handoffs",
        "การส่งมอบงานจัดสินค้า",
        GOVERNED
    ),
];
pub const POS_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "pos.sale",
        "/pos",
        "sale",
        "Sales",
        "รายการขาย",
        TRANSACTION
    ),
    live_family!(
        "pos.refund",
        "/pos",
        "refund",
        "Refunds",
        "การคืนเงิน",
        TRANSACTION
    ),
    live_family!(
        "pos.register-session",
        "/pos",
        "register-session",
        "Register sessions",
        "รอบเปิดเครื่องขาย",
        GOVERNED
    ),
    live_family!(
        "pos.cash-movement",
        "/pos",
        "cash-movement",
        "Cash movements",
        "การเคลื่อนไหวเงินสด",
        TRANSACTION
    ),
];
pub const PROCUREMENT_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "procurement.purchase-order",
        "/procurement",
        "purchase-order",
        "Purchase orders",
        "ใบสั่งซื้อ",
        TRANSACTION
    ),
    live_family!(
        "procurement.requisition",
        "/procurement",
        "requisition",
        "Purchase requisitions",
        "ใบขอซื้อ",
        GOVERNED
    ),
    live_family!(
        "procurement.rfq",
        "/procurement",
        "rfq",
        "Requests for quotation",
        "คำขอใบเสนอราคา",
        GOVERNED
    ),
    live_family!(
        "procurement.receipt",
        "/procurement",
        "receipt",
        "Goods receipts",
        "ใบรับสินค้า",
        TRANSACTION
    ),
    live_family!(
        "procurement.supplier-return",
        "/procurement",
        "supplier-return",
        "Supplier returns",
        "การคืนสินค้าให้ผู้ขาย",
        TRANSACTION
    ),
];
pub const INVENTORY_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "inventory.document",
        "/inventory",
        "document",
        "Inventory documents",
        "เอกสารสินค้าคงคลัง",
        TRANSACTION
    ),
    live_family!(
        "inventory.transfer",
        "/inventory",
        "transfer",
        "Inventory transfers",
        "การโอนสินค้าคงคลัง",
        TRANSACTION
    ),
    live_family!(
        "inventory.count",
        "/inventory",
        "count",
        "Inventory counts",
        "การตรวจนับสินค้าคงคลัง",
        GOVERNED
    ),
    live_family!(
        "inventory.reservation",
        "/inventory",
        "reservation",
        "Inventory reservations",
        "การจองสินค้าคงคลัง",
        ACTIVE
    ),
    live_family!(
        "inventory.lot",
        "/inventory",
        "lot",
        "Inventory lots",
        "ล็อตสินค้าคงคลัง",
        ACTIVE
    ),
    live_family!(
        "inventory.serial",
        "/inventory",
        "serial",
        "Inventory serials",
        "หมายเลขประจำสินค้า",
        ACTIVE
    ),
];
pub const ASSET_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!("asset.asset", "/asset", "asset", "Assets", "สินทรัพย์", ACTIVE),
    live_family!(
        "asset.transfer",
        "/asset",
        "transfer",
        "Asset transfers",
        "การโอนย้ายสินทรัพย์",
        TRANSACTION
    ),
    live_family!(
        "asset.depreciation-run",
        "/asset",
        "depreciation-run",
        "Depreciation runs",
        "รอบคำนวณค่าเสื่อมราคา",
        GOVERNED
    ),
    live_family!(
        "asset.disposal",
        "/asset",
        "disposal",
        "Asset disposals",
        "การจำหน่ายสินทรัพย์",
        GOVERNED
    ),
];
pub const MAINTENANCE_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "maintenance.work-order",
        "/maintenance",
        "work-order",
        "Work orders",
        "ใบสั่งงานบำรุงรักษา",
        GOVERNED
    ),
    live_family!(
        "maintenance.plan",
        "/maintenance",
        "plan",
        "Maintenance plans",
        "แผนบำรุงรักษา",
        GOVERNED
    ),
    live_family!(
        "maintenance.inspection",
        "/maintenance",
        "inspection",
        "Maintenance inspections",
        "การตรวจสภาพเพื่อบำรุงรักษา",
        GOVERNED
    ),
    live_family!(
        "maintenance.request",
        "/maintenance",
        "request",
        "Maintenance requests",
        "คำขอบำรุงรักษา",
        TRANSACTION
    ),
];
pub const QUALITY_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "quality.inspection",
        "/quality",
        "inspection",
        "Inspections",
        "การตรวจสอบ",
        GOVERNED
    ),
    live_family!(
        "quality.plan",
        "/quality",
        "plan",
        "Quality plans",
        "แผนคุณภาพ",
        GOVERNED
    ),
    live_family!(
        "quality.nonconformance",
        "/quality",
        "nonconformance",
        "Nonconformances",
        "ข้อบกพร่องไม่เป็นไปตามข้อกำหนด",
        GOVERNED
    ),
    live_family!(
        "quality.corrective-action",
        "/quality",
        "corrective-action",
        "Corrective actions",
        "การดำเนินการแก้ไข",
        GOVERNED
    ),
];
pub const OPERATIONS_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "operations.exception",
        "/operations",
        "exception",
        "Operational exceptions",
        "ข้อยกเว้นการปฏิบัติงาน",
        GOVERNED
    ),
    live_family!(
        "operations.operation",
        "/operations",
        "operation",
        "Operations",
        "การปฏิบัติการ",
        ACTIVE
    ),
    live_family!(
        "operations.case",
        "/operations",
        "case",
        "Operations cases",
        "กรณีงานปฏิบัติการ",
        GOVERNED
    ),
    live_family!(
        "operations.approval-work",
        "/operations",
        "approval-work",
        "Approval work",
        "งานอนุมัติ",
        GOVERNED
    ),
];
pub const PEOPLE_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "people.employee",
        "/people",
        "employee",
        "Employees",
        "พนักงาน",
        ACTIVE
    ),
    live_family!(
        "people.employment",
        "/people",
        "employment",
        "Employment records",
        "ประวัติการจ้างงาน",
        GOVERNED
    ),
    live_family!(
        "people.leave",
        "/people",
        "leave",
        "Leave cases",
        "คำขอลา",
        GOVERNED
    ),
    live_family!(
        "people.time-record",
        "/people",
        "time-record",
        "Time records",
        "บันทึกเวลาทำงาน",
        TRANSACTION
    ),
    live_family!(
        "payroll.run",
        "/payroll",
        "run",
        "Payroll runs",
        "รอบเงินเดือน",
        TRANSACTION
    ),
    live_family!(
        "payroll.payslip",
        "/payroll",
        "payslip",
        "Payslips",
        "สลิปเงินเดือน",
        TRANSACTION
    ),
];
pub const LEGAL_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "legal.contract",
        "/legal",
        "contract",
        "Contracts",
        "สัญญา",
        GOVERNED
    ),
    live_family!(
        "legal.matter",
        "/legal",
        "matter",
        "Legal matters",
        "เรื่องทางกฎหมาย",
        GOVERNED
    ),
    live_family!(
        "legal.hold",
        "/legal",
        "hold",
        "Legal holds",
        "คำสั่งระงับการทำลายข้อมูล",
        GOVERNED
    ),
    live_family!(
        "legal.case",
        "/legal",
        "case",
        "Legal cases",
        "คดีและกรณีกฎหมาย",
        GOVERNED
    ),
    live_family!(
        "legal.signature-envelope",
        "/legal",
        "signature-envelope",
        "Signature envelopes",
        "ชุดเอกสารลงนาม",
        GOVERNED
    ),
];
pub const MARKETING_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!(
        "marketing.campaign",
        "/marketing",
        "campaign",
        "Campaigns",
        "แคมเปญ",
        GOVERNED
    ),
    live_family!(
        "marketing.content",
        "/marketing",
        "content",
        "Marketing content",
        "เนื้อหาการตลาด",
        GOVERNED
    ),
    live_family!(
        "marketing.audience-activation",
        "/marketing",
        "audience-activation",
        "Audience activations",
        "การเปิดใช้งานกลุ่มเป้าหมาย",
        GOVERNED
    ),
    live_family!(
        "marketing.consent-purpose",
        "/marketing",
        "consent-purpose",
        "Consent purposes",
        "วัตถุประสงค์ความยินยอม",
        GOVERNED
    ),
];
pub const CRM_FAMILIES: &[CurrentRecordFamilySpec] = &[
    live_family!("crm.lead", "/crm", "lead", "Leads", "ลีด", GOVERNED),
    live_family!(
        "crm.opportunity",
        "/crm",
        "opportunity",
        "Opportunities",
        "โอกาสการขาย",
        GOVERNED
    ),
    live_family!(
        "crm.activity",
        "/crm",
        "activity",
        "CRM activities",
        "กิจกรรมลูกค้าสัมพันธ์",
        TRANSACTION
    ),
    live_family!(
        "crm.forecast",
        "/crm",
        "forecast",
        "Sales forecasts",
        "การคาดการณ์ยอดขาย",
        GOVERNED
    ),
];

pub const LIVE_CURRENT_RECORD_TYPES: [&str; 92] = [
    "identity.principal",
    "identity.role",
    "identity.grant",
    "identity.access-review",
    "identity.break-glass-session",
    "platform.holding",
    "platform.provider",
    "platform.data-plane-operation",
    "platform.migration",
    "platform.release",
    "organization.holding",
    "organization.company",
    "organization.branch",
    "organization.sharing-agreement",
    "commercial.subscription",
    "commercial.entitlement",
    "commercial.account",
    "commercial.adjustment",
    "brand.profile",
    "brand.asset",
    "brand.publication",
    "customer.account",
    "customer.authority-grant",
    "customer.support-access",
    "customer-360.profile",
    "customer-360.note",
    "customer-360.merge",
    "support.customer-case",
    "support.incident",
    "support.access-session",
    "support.platform-case",
    "finance.journal",
    "finance.invoice",
    "finance.close-cycle",
    "finance.credit-note",
    "finance.debit-note",
    "finance.payment",
    "finance.reconciliation",
    "sales.quote",
    "sales.order",
    "sales.return",
    "sales.fulfillment-handoff",
    "pos.sale",
    "pos.refund",
    "pos.register-session",
    "pos.cash-movement",
    "procurement.purchase-order",
    "procurement.requisition",
    "procurement.rfq",
    "procurement.receipt",
    "procurement.supplier-return",
    "inventory.document",
    "inventory.transfer",
    "inventory.count",
    "inventory.reservation",
    "inventory.lot",
    "inventory.serial",
    "asset.asset",
    "asset.transfer",
    "asset.depreciation-run",
    "asset.disposal",
    "maintenance.work-order",
    "maintenance.plan",
    "maintenance.inspection",
    "maintenance.request",
    "quality.inspection",
    "quality.plan",
    "quality.nonconformance",
    "quality.corrective-action",
    "operations.exception",
    "operations.operation",
    "operations.case",
    "operations.approval-work",
    "people.employee",
    "people.employment",
    "people.leave",
    "people.time-record",
    "payroll.run",
    "payroll.payslip",
    "legal.contract",
    "legal.matter",
    "legal.hold",
    "legal.case",
    "legal.signature-envelope",
    "marketing.campaign",
    "marketing.content",
    "marketing.audience-activation",
    "marketing.consent-purpose",
    "crm.lead",
    "crm.opportunity",
    "crm.activity",
    "crm.forecast",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrentRecordRoute {
    Workspace {
        feature: CurrentFeatureKind,
    },
    Collection {
        feature: CurrentFeatureKind,
        family: CurrentRecordFamilySpec,
    },
    Detail {
        feature: CurrentFeatureKind,
        family: CurrentRecordFamilySpec,
        id: String,
    },
}

impl CurrentRecordRoute {
    pub fn parse(feature: CurrentFeatureKind, input: &str) -> Option<Self> {
        let path = normalize_deep_link(input);
        if path == feature.root() {
            return Some(Self::Workspace { feature });
        }
        feature.record_families().iter().find_map(|family| {
            let collection = family.collection_path();
            if path == collection {
                return Some(Self::Collection {
                    feature,
                    family: *family,
                });
            }
            let prefix = format!("{collection}/");
            let id = path.strip_prefix(&prefix)?;
            valid_opaque_segment(id).then(|| Self::Detail {
                feature,
                family: *family,
                id: id.to_owned(),
            })
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadYourWritesEvidence {
    pub committed_version: u64,
    pub projected_version: u64,
    pub lag_seconds: u64,
}

impl ReadYourWritesEvidence {
    pub const fn satisfied(self) -> bool {
        self.projected_version >= self.committed_version
    }

    pub const fn freshness(self) -> ReadModelFreshness {
        if self.lag_seconds > 60 {
            ReadModelFreshness::Stale
        } else if self.satisfied() {
            ReadModelFreshness::Live
        } else {
            ReadModelFreshness::Current
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentCommandAvailability {
    Available,
    OfflineReadOnly,
    Denied,
    Stale,
    TransportUnavailable,
    TargetPending,
}

impl CurrentCommandAvailability {
    pub const fn permits_submit(self) -> bool {
        matches!(self, Self::Available)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentCommandCard {
    pub command_id: String,
    pub label: String,
    pub expected_version: u64,
    pub target_state: &'static str,
    pub confirmation_required: bool,
    pub approval_hook: bool,
    pub activity_hook: bool,
    pub reference_hook: bool,
    pub availability: CurrentCommandAvailability,
    pub reason: Option<String>,
}

pub fn current_command_cards(
    family: CurrentRecordFamilySpec,
    current_state: &str,
    expected_version: u64,
    locale: Locale,
    mutation_mode: MutationMode,
    freshness: ReadModelFreshness,
    transport_ready: bool,
) -> Vec<CurrentCommandCard> {
    let availability = if family.publication == RecordPublicationState::TargetPending {
        CurrentCommandAvailability::TargetPending
    } else if matches!(mutation_mode, MutationMode::Denied) {
        CurrentCommandAvailability::Denied
    } else if matches!(mutation_mode, MutationMode::ReadOnlyOffline) {
        CurrentCommandAvailability::OfflineReadOnly
    } else if matches!(
        freshness,
        ReadModelFreshness::Stale | ReadModelFreshness::Unknown
    ) {
        CurrentCommandAvailability::Stale
    } else if !transport_ready {
        CurrentCommandAvailability::TransportUnavailable
    } else {
        CurrentCommandAvailability::Available
    };
    family
        .lifecycle
        .iter()
        .copied()
        .filter(|state| *state != current_state)
        .map(|state| CurrentCommandCard {
            command_id: format!(
                "{}-transition-to-{state}",
                family.record_type.replace('.', "-")
            ),
            label: match locale {
                Locale::En => format!("Request transition to {state}"),
                Locale::Th => format!("ขอเปลี่ยนสถานะเป็น {state}"),
            },
            expected_version,
            target_state: state,
            confirmation_required: true,
            approval_hook: true,
            activity_hook: true,
            reference_hook: true,
            availability,
            reason: (!availability.permits_submit())
                .then(|| command_unavailable_copy(locale, availability).to_owned()),
        })
        .collect()
}

pub fn current_workspace_state(kind: EnterpriseStateKind, locale: Locale) -> EnterpriseState {
    let (title, detail, retryable) = match kind {
        EnterpriseStateKind::Loading => ("Loading", "กำลังโหลด", false),
        EnterpriseStateKind::Empty => ("No authorized records", "ไม่พบรายการที่ได้รับอนุญาต", false),
        EnterpriseStateKind::Denied => ("Access denied", "ปฏิเสธการเข้าถึง", false),
        EnterpriseStateKind::Restricted => ("Restricted item", "รายการที่ถูกจำกัด", false),
        EnterpriseStateKind::Stale => ("Data is stale", "ข้อมูลล้าสมัย", true),
        EnterpriseStateKind::Degraded => ("Data source degraded", "แหล่งข้อมูลทำงานแบบลดระดับ", true),
        EnterpriseStateKind::Conflict => ("Record changed", "รายการมีการเปลี่ยนแปลง", true),
        EnterpriseStateKind::Retry => ("Retry available", "สามารถลองใหม่ได้", true),
        EnterpriseStateKind::Success => ("Current data", "ข้อมูลเป็นปัจจุบัน", false),
    };
    let selected = match locale {
        Locale::En => (title, english_state_detail(kind), retryable),
        Locale::Th => (detail, thai_state_detail(kind), retryable),
    };
    EnterpriseState::bounded(kind, selected.0, selected.1, selected.2)
}

pub fn render_current_workspace_surface(
    context: &FeatureContext,
    feature: CurrentFeatureKind,
) -> Element {
    let locale = context.locale;
    let route = CurrentRecordRoute::parse(feature, &context.canonical_path);
    let family = match route.as_ref() {
        Some(
            CurrentRecordRoute::Collection { family, .. }
            | CurrentRecordRoute::Detail { family, .. },
        ) => *family,
        _ => feature.record_families()[0],
    };
    let detail_ref = match route.as_ref() {
        Some(CurrentRecordRoute::Detail { id, .. }) => Some(id.clone()),
        _ => None,
    };
    let state_kind = if !context.session.authenticated {
        EnterpriseStateKind::Denied
    } else if context.active_scope.is_none() {
        EnterpriseStateKind::Restricted
    } else if route.is_none() {
        EnterpriseStateKind::Retry
    } else {
        EnterpriseStateKind::Degraded
    };
    let state = current_workspace_state(state_kind, locale);
    let commands = current_command_cards(
        family,
        "",
        1,
        locale,
        if context.session.authenticated && context.active_scope.is_some() {
            MutationMode::Enabled
        } else {
            MutationMode::Denied
        },
        ReadModelFreshness::Unknown,
        false,
    );
    let accessibility_role = if matches!(
        state.kind,
        EnterpriseStateKind::Denied
            | EnterpriseStateKind::Restricted
            | EnterpriseStateKind::Conflict
    ) {
        "alert"
    } else {
        "status"
    };
    let publication_key = match family.publication {
        RecordPublicationState::Live => "live",
        RecordPublicationState::TargetPending => "target-pending",
    };

    rsx! {
        section {
            class: "feature-owned-surface current-workspace",
            role: "region",
            "aria-labelledby": "current-workspace-title",
            "data-feature-root": feature.root(),
            "data-channel-policy": "web-desktop-ios-android",
            "data-offline-policy": "read-only-no-queue",
            "data-reduced-motion": "respected",
            "data-supported-states": "loading empty denied restricted stale degraded conflict retry success",
            header {
                class: "feature-owned-surface__header",
                p { class: "work-os-eyebrow", {localized(locale, "Business workspace", "พื้นที่ทำงานธุรกิจ")} }
                h2 { id: "current-workspace-title", tabindex: "-1", {feature.label().text(locale)} }
                p { {localized(
                    locale,
                    "Authorized records, source freshness and governed actions",
                    "รายการที่ได้รับอนุญาต ความสดใหม่ของแหล่งข้อมูล และการดำเนินการที่กำกับดูแล",
                )} }
            }
            nav {
                class: "current-workspace__navigation",
                "aria-label": localized(locale, "Record collections", "คอลเลกชันรายการ"),
                ul {
                    for item in feature.record_families() {
                        {
                            let collection_path = item.collection_path();
                            let path_for_click = collection_path.clone();
                            let navigate = context.navigate.clone();
                            rsx! {
                                li {
                                    KitLink {
                                        href: collection_path,
                                        onclick: move |event: MouseEvent| {
                                            event.prevent_default();
                                            navigate(path_for_click.clone());
                                        },
                                        "aria-current": if item.record_type == family.record_type { "page" } else { "false" },
                                        "data-publication": match item.publication {
                                            RecordPublicationState::Live => "live",
                                            RecordPublicationState::TargetPending => "target-pending",
                                        },
                                        {item.label.text(locale)}
                                    }
                                }
                            }
                        }
                    }
                }
            }
            section {
                class: "current-workspace__state",
                role: accessibility_role,
                aria_live: if accessibility_role == "alert" { "assertive" } else { "polite" },
                aria_atomic: "true",
                aria_busy: if state.kind == EnterpriseStateKind::Loading { "true" } else { "false" },
                "data-state": state_key(state.kind),
                h3 { {state.title} }
                p { {state.detail} }
                if state.retryable {
                    KitButton {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Sm,
                        r#type: "button",
                        disabled: true,
                        "aria-disabled": "true",
                        {localized(locale, "Retry when the typed transport is ready", "ลองใหม่เมื่อการเชื่อมต่อแบบระบุชนิดพร้อม")}
                    }
                }
            }
            section {
                class: "current-workspace__record",
                "aria-labelledby": "current-record-title",
                "data-record-type": family.record_type,
                "data-publication": publication_key,
                h3 { id: "current-record-title", {family.label.text(locale)} }
                if let Some(record_ref) = detail_ref {
                    dl {
                        dt { {localized(locale, "Opaque reference", "รหัสอ้างอิงแบบทึบ")} }
                        dd { {SafeUiText::new(record_ref).as_str().to_owned()} }
                        dt { {localized(locale, "Read-your-writes freshness", "ความสดใหม่หลังการบันทึก")} }
                        dd { {localized(locale, "Unavailable; no write-model fallback", "ไม่พร้อมใช้งานและไม่ย้อนอ่านแบบจำลองเขียน")} }
                    }
                } else {
                    p { {localized(locale, "No authorized records loaded.", "ยังไม่มีการโหลดรายการที่ได้รับอนุญาต")} }
                }
            }
            section {
                class: "current-workspace__actions",
                "aria-labelledby": "current-actions-title",
                h3 { id: "current-actions-title", {localized(locale, "Governed action cards", "การดำเนินการที่กำกับดูแล")} }
                ul {
                    for command in commands {
                        li {
                            article {
                                class: "current-workspace__action-card",
                                "data-command-id": command.command_id,
                                h4 { {command.label} }
                                p { {localized(
                                    locale,
                                    "Requires current version, idempotency key and explicit confirmation.",
                                    "ต้องใช้เวอร์ชันปัจจุบัน คีย์ป้องกันการทำซ้ำ และการยืนยันอย่างชัดเจน",
                                )} }
                                KitButton {
                                    variant: ButtonVariant::Secondary,
                                    size: ButtonSize::Sm,
                                    r#type: "button",
                                    disabled: true,
                                    "aria-disabled": "true",
                                    {localized(locale, "Confirm action", "ยืนยันการดำเนินการ")}
                                }
                                if let Some(reason) = command.reason {
                                    p { role: "status", {reason} }
                                }
                            }
                        }
                    }
                }
            }
            aside {
                class: "current-workspace__collaboration",
                "aria-label": localized(locale, "Record collaboration hooks", "จุดเชื่อมการทำงานร่วมกันของรายการ"),
                section {
                    h3 { {localized(locale, "Approvals", "การอนุมัติ")} }
                    p { {localized(locale, "Owner-authorized typed approvals only.", "อนุมัติผ่านคำสั่งแบบระบุชนิดของเจ้าของเท่านั้น")} }
                }
                section {
                    h3 { {localized(locale, "Activity", "กิจกรรม")} }
                    p { {localized(locale, "Versioned activity and audit facts appear when authorized.", "แสดงกิจกรรมและหลักฐานตรวจสอบแบบมีเวอร์ชันเมื่อได้รับอนุญาต")} }
                }
                section {
                    h3 { {localized(locale, "References", "การอ้างอิง")} }
                    p { {localized(locale, "Hidden references and counts remain undisclosed.", "ไม่เปิดเผยการอ้างอิงและจำนวนที่ถูกซ่อน")} }
                }
            }
            footer {
                class: "current-workspace__footer",
                p { {localized(
                    locale,
                    "Adaptive on Web, Desktop, iOS and Android. Offline is read-only and changes are not queued.",
                    "ปรับตาม Web, Desktop, iOS และ Android โหมดออฟไลน์อ่านได้อย่างเดียวและไม่จัดคิวการเปลี่ยนแปลง",
                )} }
            }
        }
    }
}

fn state_key(kind: EnterpriseStateKind) -> &'static str {
    match kind {
        EnterpriseStateKind::Loading => "loading",
        EnterpriseStateKind::Empty => "empty",
        EnterpriseStateKind::Denied => "denied",
        EnterpriseStateKind::Restricted => "restricted",
        EnterpriseStateKind::Stale => "stale",
        EnterpriseStateKind::Degraded => "degraded",
        EnterpriseStateKind::Conflict => "conflict",
        EnterpriseStateKind::Retry => "retry",
        EnterpriseStateKind::Success => "success",
    }
}

fn english_state_detail(kind: EnterpriseStateKind) -> &'static str {
    match kind {
        EnterpriseStateKind::Loading => "Waiting for an authorized projection read.",
        EnterpriseStateKind::Empty => "The authorized query returned no records.",
        EnterpriseStateKind::Denied => "No record existence or payload is disclosed.",
        EnterpriseStateKind::Restricted => {
            "The selected organization scope cannot reveal this item."
        }
        EnterpriseStateKind::Stale => "Cached data is labeled and mutations remain disabled.",
        EnterpriseStateKind::Degraded => {
            "The typed gateway is unavailable; no result is simulated."
        }
        EnterpriseStateKind::Conflict => "Reload the current version before confirming again.",
        EnterpriseStateKind::Retry => "The same read intent may be retried safely.",
        EnterpriseStateKind::Success => {
            "The authorized read model satisfies the requested version."
        }
    }
}

fn thai_state_detail(kind: EnterpriseStateKind) -> &'static str {
    match kind {
        EnterpriseStateKind::Loading => "กำลังรอการอ่านแบบจำลองที่ได้รับอนุญาต",
        EnterpriseStateKind::Empty => "คำค้นที่ได้รับอนุญาตไม่พบรายการ",
        EnterpriseStateKind::Denied => "ไม่เปิดเผยการมีอยู่หรือข้อมูลของรายการ",
        EnterpriseStateKind::Restricted => "ขอบเขตองค์กรที่เลือกไม่สามารถเปิดเผยรายการนี้",
        EnterpriseStateKind::Stale => "ข้อมูลแคชถูกระบุว่าล้าสมัยและยังไม่อนุญาตให้แก้ไข",
        EnterpriseStateKind::Degraded => "เกตเวย์แบบระบุชนิดไม่พร้อมและไม่มีการจำลองผลลัพธ์",
        EnterpriseStateKind::Conflict => "โหลดเวอร์ชันปัจจุบันก่อนยืนยันอีกครั้ง",
        EnterpriseStateKind::Retry => "สามารถลองอ่านด้วยเจตนาเดิมได้อย่างปลอดภัย",
        EnterpriseStateKind::Success => "แบบจำลองอ่านที่ได้รับอนุญาตเป็นไปตามเวอร์ชันที่ร้องขอ",
    }
}

fn command_unavailable_copy(
    locale: Locale,
    availability: CurrentCommandAvailability,
) -> &'static str {
    match availability {
        CurrentCommandAvailability::Available => "",
        CurrentCommandAvailability::OfflineReadOnly => localized(
            locale,
            "Offline mode is read-only; changes are not queued.",
            "โหมดออฟไลน์อ่านได้อย่างเดียวและไม่จัดคิวการเปลี่ยนแปลง",
        ),
        CurrentCommandAvailability::Denied => localized(
            locale,
            "Current authority does not permit this action.",
            "สิทธิ์ปัจจุบันไม่อนุญาตการดำเนินการนี้",
        ),
        CurrentCommandAvailability::Stale => localized(
            locale,
            "Wait for the read model to reach the committed version.",
            "รอให้แบบจำลองอ่านตามทันเวอร์ชันที่บันทึกแล้ว",
        ),
        CurrentCommandAvailability::TransportUnavailable => localized(
            locale,
            "The typed command transport is unavailable.",
            "การเชื่อมต่อคำสั่งแบบระบุชนิดไม่พร้อมใช้งาน",
        ),
        CurrentCommandAvailability::TargetPending => localized(
            locale,
            "This target record family has no accepted live publication policy yet.",
            "กลุ่มรายการเป้าหมายนี้ยังไม่มีนโยบายเผยแพร่ที่ใช้งานจริง",
        ),
    }
}

fn localized(locale: Locale, en: &'static str, th: &'static str) -> &'static str {
    LocalizedText::new(en, th).text(locale)
}

fn valid_opaque_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && !["token", "secret", "password", "credential"]
            .iter()
            .any(|marker| value.to_ascii_lowercase().contains(marker))
}

#[macro_export]
macro_rules! declare_current_workspace_feature {
    (
        $surface:ident,
        $feature:expr,
        $feature_id:literal,
        [$(($route_id:literal, $canonical_path:literal, $navigation_group:literal)),+ $(,)?],
        $artifact_key:literal
    ) => {
        use dioxus::prelude::*;

        const FEATURE_ROUTES: &[erpx_ui_core::FeatureRouteDescriptor] = &[
            $(erpx_ui_core::FeatureRouteDescriptor {
                id: $route_id,
                canonical_path: $canonical_path,
                authorization: "authenticated",
                navigation_group: $navigation_group,
            }),+
        ];

        #[component]
        pub fn $surface(context: erpx_ui_core::FeatureContext) -> Element {
            erpx_ui_kit::render_current_workspace_surface(&context, $feature)
        }

        pub use $surface as FeatureSurface;

        fn render_feature_surface(context: &erpx_ui_core::FeatureContext) -> Element {
            rsx! { $surface { context: context.clone() } }
        }

        pub static REGISTRATION: erpx_ui_core::FeatureRegistration =
            erpx_ui_core::FeatureRegistration::with_renderer_and_artifact(
                $feature_id,
                FEATURE_ROUTES,
                $artifact_key,
                render_feature_surface,
            );

        #[cfg(test)]
        mod current_workspace_contract_tests {
            use super::*;

            #[test]
            fn canonical_record_routes_and_e2e_evidence_are_registered() {
                let feature = $feature;
                assert!(!feature.record_families().is_empty());
                assert!(!feature.e2e_scenarios().is_empty());
                assert!(REGISTRATION.routes.iter().all(|route| {
                    !route.canonical_path.contains(concat!("/work", "spaces"))
                }));
                for family in feature.record_families() {
                    assert!(erpx_ui_kit::CurrentRecordRoute::parse(
                        feature,
                        &family.collection_path(),
                    )
                    .is_some());
                    let detail = family.detail_path("record-001").expect("detail route");
                    assert!(matches!(
                        erpx_ui_kit::CurrentRecordRoute::parse(feature, &detail),
                        Some(erpx_ui_kit::CurrentRecordRoute::Detail { .. })
                    ));
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_live_current_record_family_has_a_detail_presenter() {
        let families = CurrentFeatureKind::ALL
            .into_iter()
            .flat_map(CurrentFeatureKind::record_families)
            .collect::<Vec<_>>();
        assert_eq!(
            families
                .iter()
                .filter(|family| family.publication == RecordPublicationState::Live)
                .count(),
            LIVE_CURRENT_RECORD_TYPES.len()
        );
        assert_eq!(
            families
                .iter()
                .filter(|family| family.publication == RecordPublicationState::Live)
                .map(|family| family.record_type)
                .collect::<Vec<_>>(),
            LIVE_CURRENT_RECORD_TYPES
        );
        assert_eq!(
            erpx_integration_contracts::catalog::LIVE_CURRENT_RECORD_PUBLISHERS,
            LIVE_CURRENT_RECORD_TYPES
        );
        for family in families {
            assert!(family.label.is_valid());
            assert!(!family.lifecycle.is_empty());
            let path = family.detail_path("opaque-1").expect("detail path");
            assert!(!path.contains(concat!("/work", "spaces")));
        }
    }

    #[test]
    fn every_presented_current_policy_surface_is_live() {
        let pending = CurrentFeatureKind::ALL
            .into_iter()
            .flat_map(CurrentFeatureKind::record_families)
            .filter(|family| family.publication == RecordPublicationState::TargetPending)
            .map(|family| family.record_type)
            .collect::<Vec<_>>();
        assert!(pending.is_empty());
    }

    #[test]
    fn every_state_is_bilingual_bounded_and_accessible() {
        for kind in [
            EnterpriseStateKind::Loading,
            EnterpriseStateKind::Empty,
            EnterpriseStateKind::Denied,
            EnterpriseStateKind::Restricted,
            EnterpriseStateKind::Stale,
            EnterpriseStateKind::Degraded,
            EnterpriseStateKind::Conflict,
            EnterpriseStateKind::Retry,
            EnterpriseStateKind::Success,
        ] {
            for locale in Locale::ALL {
                let state = current_workspace_state(kind, locale);
                assert!(!state.title.is_empty());
                assert!(!state.detail.is_empty());
                assert_ne!(state.title, "[redacted]");
                assert_ne!(state.detail, "[redacted]");
            }
        }
    }

    #[test]
    fn command_cards_are_versioned_confirmed_and_fail_closed() {
        let family = FINANCE_FAMILIES[1];
        let cards = current_command_cards(
            family,
            "draft",
            7,
            Locale::En,
            MutationMode::Enabled,
            ReadModelFreshness::Live,
            false,
        );
        assert!(cards.iter().all(|card| {
            card.expected_version == 7
                && card.confirmation_required
                && card.approval_hook
                && card.activity_hook
                && card.reference_hook
                && card.availability == CurrentCommandAvailability::TransportUnavailable
                && !card.availability.permits_submit()
        }));
    }

    #[test]
    fn offline_never_queues_or_enables_mutation() {
        let cards = current_command_cards(
            SALES_FAMILIES[1],
            "draft",
            2,
            Locale::Th,
            MutationMode::ReadOnlyOffline,
            ReadModelFreshness::Live,
            true,
        );
        assert!(
            cards
                .iter()
                .all(|card| card.availability == CurrentCommandAvailability::OfflineReadOnly)
        );
        assert!(!MutationMode::ReadOnlyOffline.queues_offline());
    }

    #[test]
    fn read_your_writes_and_stale_threshold_are_explicit() {
        let waiting = ReadYourWritesEvidence {
            committed_version: 8,
            projected_version: 7,
            lag_seconds: 2,
        };
        assert!(!waiting.satisfied());
        assert_eq!(waiting.freshness(), ReadModelFreshness::Current);
        assert_eq!(
            ReadYourWritesEvidence {
                lag_seconds: 61,
                ..waiting
            }
            .freshness(),
            ReadModelFreshness::Stale
        );
    }

    #[test]
    fn navigation_uses_governed_link_and_no_raw_anchor() {
        let source = include_str!("current_workspace.rs");
        assert!(source.contains("KitLink {"));
        assert!(source.contains("event.prevent_default();"));
        assert!(source.contains("context.navigate.clone()"));
        let raw_anchor = ["\n", "                            ", "a", " {"].concat();
        assert!(!source.contains(&raw_anchor));
    }
}
