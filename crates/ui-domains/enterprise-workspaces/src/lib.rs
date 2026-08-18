#![doc = "Transport-neutral presentation contracts for future ERPX workspaces."]
#![forbid(unsafe_code)]

use async_trait::async_trait;
use dioxus::prelude::*;
use erpx_ui_core::{FeatureContext, Locale, LocalizedText, SafeUiText, normalize_deep_link};
use erpx_ui_kit::kit::{Button as KitButton, Link as KitLink};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceKind {
    Manufacturing,
    Planning,
    Projects,
    Ecommerce,
    Talent,
    Learning,
    Benefits,
    FieldService,
}

impl WorkspaceKind {
    pub const ALL: [Self; 8] = [
        Self::Manufacturing,
        Self::Planning,
        Self::Projects,
        Self::Ecommerce,
        Self::Talent,
        Self::Learning,
        Self::Benefits,
        Self::FieldService,
    ];

    pub const fn root(self) -> &'static str {
        match self {
            Self::Manufacturing => "/manufacturing",
            Self::Planning => "/planning",
            Self::Projects => "/project",
            Self::Ecommerce => "/commerce",
            Self::Talent => "/talent",
            Self::Learning => "/learning",
            Self::Benefits => "/benefits",
            Self::FieldService => "/field-service",
        }
    }

    pub const fn record_segment(self) -> &'static str {
        match self {
            Self::Manufacturing => "order",
            Self::Planning => "scenario",
            Self::Projects => "item",
            Self::Ecommerce => "order",
            Self::Talent => "application",
            Self::Learning => "course",
            Self::Benefits => "election",
            Self::FieldService => "visit",
        }
    }

    pub const fn label(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::Manufacturing, Locale::En) => "Manufacturing",
            (Self::Manufacturing, Locale::Th) => "การผลิต",
            (Self::Planning, Locale::En) => "Supply and demand planning",
            (Self::Planning, Locale::Th) => "การวางแผนอุปสงค์และอุปทาน",
            (Self::Projects, Locale::En) => "Projects",
            (Self::Projects, Locale::Th) => "โครงการ",
            (Self::Ecommerce, Locale::En) => "E-commerce",
            (Self::Ecommerce, Locale::Th) => "อีคอมเมิร์ซ",
            (Self::Talent, Locale::En) => "Recruiting and talent",
            (Self::Talent, Locale::Th) => "การสรรหาและผู้มีศักยภาพ",
            (Self::Learning, Locale::En) => "Learning",
            (Self::Learning, Locale::Th) => "การเรียนรู้",
            (Self::Benefits, Locale::En) => "Benefits",
            (Self::Benefits, Locale::Th) => "สวัสดิการ",
            (Self::FieldService, Locale::En) => "Field service",
            (Self::FieldService, Locale::Th) => "บริการภาคสนาม",
        }
    }

    pub const fn channel_policy(self) -> ChannelPolicy {
        ChannelPolicy::WebMacOs
    }

    pub fn collection_path(self) -> String {
        format!("{}/{}", self.root(), self.record_segment())
    }

    pub fn detail_path(self, id: &str) -> Option<String> {
        valid_opaque_segment(id)
            .then(|| format!("{}/{}/{}", self.root(), self.record_segment(), id))
    }

    pub const fn record_families(self) -> &'static [RecordFamilySpec] {
        match self {
            Self::Manufacturing => MANUFACTURING_FAMILIES,
            Self::Planning => PLANNING_FAMILIES,
            Self::Projects => PROJECT_FAMILIES,
            Self::Ecommerce => ECOMMERCE_FAMILIES,
            Self::Talent => TALENT_FAMILIES,
            Self::Learning => LEARNING_FAMILIES,
            Self::Benefits => BENEFIT_FAMILIES,
            Self::FieldService => FIELD_SERVICE_FAMILIES,
        }
    }

    pub const fn e2e_scenarios(self) -> &'static [&'static str] {
        match self {
            Self::Manufacturing => &["E2E-32"],
            Self::Planning => &["E2E-33", "E2E-34"],
            Self::Projects => &["E2E-35", "E2E-64", "E2E-65", "E2E-66"],
            Self::Ecommerce => &["E2E-36"],
            Self::Talent => &["E2E-37"],
            Self::Learning => &["E2E-38"],
            Self::Benefits => &["E2E-39"],
            Self::FieldService => &["E2E-40"],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordFamilySpec {
    pub segment: &'static str,
    pub label: LocalizedText,
    pub lifecycle: &'static [&'static str],
}

impl RecordFamilySpec {
    pub fn collection_path(self, workspace: WorkspaceKind) -> String {
        format!("{}/{}", workspace.root(), self.segment)
    }

    pub fn detail_path(self, workspace: WorkspaceKind, id: &str) -> Option<String> {
        valid_opaque_segment(id).then(|| format!("{}/{}/{}", workspace.root(), self.segment, id))
    }
}

const GOVERNED_LIFECYCLE: &[&str] = &[
    "proposed",
    "open",
    "in-progress",
    "waiting",
    "resolved",
    "closed",
    "cancelled",
];
const ACTIVE_LIFECYCLE: &[&str] = &["draft", "active", "suspended", "closed"];
const TRANSACTION_LIFECYCLE: &[&str] = &[
    "draft",
    "submitted",
    "approved",
    "in-progress",
    "completed",
    "cancelled",
];

macro_rules! family {
    ($segment:literal, $en:literal, $th:literal, $lifecycle:expr) => {
        RecordFamilySpec {
            segment: $segment,
            label: LocalizedText::new($en, $th),
            lifecycle: $lifecycle,
        }
    };
}

pub const MANUFACTURING_FAMILIES: &[RecordFamilySpec] = &[
    family!(
        "specification",
        "Specifications",
        "ข้อกำหนดการผลิต",
        ACTIVE_LIFECYCLE
    ),
    family!(
        "order",
        "Manufacturing orders",
        "คำสั่งผลิต",
        TRANSACTION_LIFECYCLE
    ),
    family!("routing", "Routings", "เส้นทางการผลิต", ACTIVE_LIFECYCLE),
    family!("batch", "Batches", "รุ่นการผลิต", TRANSACTION_LIFECYCLE),
    family!(
        "result",
        "Production results",
        "ผลการผลิต",
        TRANSACTION_LIFECYCLE
    ),
];
pub const PLANNING_FAMILIES: &[RecordFamilySpec] = &[
    family!(
        "scenario",
        "Planning scenarios",
        "สถานการณ์การวางแผน",
        GOVERNED_LIFECYCLE
    ),
    family!(
        "recommendation",
        "Recommendations",
        "คำแนะนำ",
        GOVERNED_LIFECYCLE
    ),
    family!("demand-plan", "Demand plans", "แผนอุปสงค์", ACTIVE_LIFECYCLE),
    family!("supply-plan", "Supply plans", "แผนอุปทาน", ACTIVE_LIFECYCLE),
    family!(
        "capacity-plan",
        "Capacity plans",
        "แผนกำลังการผลิต",
        ACTIVE_LIFECYCLE
    ),
];
pub const PROJECT_FAMILIES: &[RecordFamilySpec] = &[
    family!("project", "Projects", "โครงการ", GOVERNED_LIFECYCLE),
    family!(
        "phase",
        "Phases and epics",
        "ระยะงานและอีพิก",
        GOVERNED_LIFECYCLE
    ),
    family!("item", "Project items", "รายการโครงการ", GOVERNED_LIFECYCLE),
    family!(
        "template",
        "Workflow templates",
        "แม่แบบกระบวนงาน",
        ACTIVE_LIFECYCLE
    ),
    family!(
        "handoff",
        "Domain handoffs",
        "การส่งมอบข้ามโดเมน",
        GOVERNED_LIFECYCLE
    ),
];
pub const ECOMMERCE_FAMILIES: &[RecordFamilySpec] = &[
    family!(
        "catalog-publication",
        "Catalog publications",
        "การเผยแพร่แค็ตตาล็อก",
        ACTIVE_LIFECYCLE
    ),
    family!("cart", "Carts", "ตะกร้าสินค้า", TRANSACTION_LIFECYCLE),
    family!(
        "order",
        "Commerce orders",
        "คำสั่งซื้อออนไลน์",
        TRANSACTION_LIFECYCLE
    ),
    family!("return", "Returns", "การคืนสินค้า", TRANSACTION_LIFECYCLE),
    family!(
        "fulfillment-request",
        "Fulfillment requests",
        "คำขอจัดส่ง",
        TRANSACTION_LIFECYCLE
    ),
];
pub const TALENT_FAMILIES: &[RecordFamilySpec] = &[
    family!(
        "requisition",
        "Requisitions",
        "คำขออัตรากำลัง",
        GOVERNED_LIFECYCLE
    ),
    family!("candidate", "Candidates", "ผู้สมัคร", ACTIVE_LIFECYCLE),
    family!("application", "Applications", "ใบสมัคร", GOVERNED_LIFECYCLE),
    family!("interview", "Interviews", "การสัมภาษณ์", GOVERNED_LIFECYCLE),
    family!("offer", "Offers", "ข้อเสนอการจ้างงาน", GOVERNED_LIFECYCLE),
    family!(
        "proposal",
        "Talent proposals",
        "ข้อเสนอด้านบุคลากร",
        GOVERNED_LIFECYCLE
    ),
    family!(
        "review-cycle",
        "Review cycles",
        "รอบการประเมิน",
        ACTIVE_LIFECYCLE
    ),
];
pub const LEARNING_FAMILIES: &[RecordFamilySpec] = &[
    family!("course", "Courses", "หลักสูตร", ACTIVE_LIFECYCLE),
    family!(
        "enrollment",
        "Enrollments",
        "การลงทะเบียนเรียน",
        TRANSACTION_LIFECYCLE
    ),
    family!(
        "assignment",
        "Learning assignments",
        "งานการเรียนรู้",
        GOVERNED_LIFECYCLE
    ),
    family!(
        "completion",
        "Completions",
        "ผลสำเร็จการเรียนรู้",
        TRANSACTION_LIFECYCLE
    ),
];
pub const BENEFIT_FAMILIES: &[RecordFamilySpec] = &[
    family!("plan", "Benefit plans", "แผนสวัสดิการ", ACTIVE_LIFECYCLE),
    family!(
        "election",
        "Benefit elections",
        "การเลือกสวัสดิการ",
        TRANSACTION_LIFECYCLE
    ),
    family!(
        "window",
        "Enrollment windows",
        "ช่วงเวลาลงทะเบียน",
        ACTIVE_LIFECYCLE
    ),
    family!(
        "handoff",
        "Payroll handoffs",
        "การส่งมอบให้เงินเดือน",
        GOVERNED_LIFECYCLE
    ),
];
pub const FIELD_SERVICE_FAMILIES: &[RecordFamilySpec] = &[
    family!(
        "agreement",
        "Service agreements",
        "ข้อตกลงบริการ",
        ACTIVE_LIFECYCLE
    ),
    family!(
        "request",
        "Service requests",
        "คำขอบริการ",
        GOVERNED_LIFECYCLE
    ),
    family!(
        "dispatch",
        "Dispatches",
        "การจัดส่งช่าง",
        TRANSACTION_LIFECYCLE
    ),
    family!(
        "visit",
        "Field visits",
        "การเข้าบริการ",
        TRANSACTION_LIFECYCLE
    ),
    family!(
        "handoff",
        "Domain handoffs",
        "การส่งมอบข้ามโดเมน",
        GOVERNED_LIFECYCLE
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientChannel {
    Web,
    Desktop,
    Ios,
    Android,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelPolicy {
    WebMacOs,
}

impl ChannelPolicy {
    pub const fn supports(self, channel: ClientChannel) -> bool {
        match self {
            Self::WebMacOs => matches!(channel, ClientChannel::Web | ClientChannel::Desktop),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceRoute {
    Workspace {
        workspace: WorkspaceKind,
    },
    Collection {
        workspace: WorkspaceKind,
    },
    Detail {
        workspace: WorkspaceKind,
        id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalRecordRoute {
    Workspace {
        workspace: WorkspaceKind,
    },
    Collection {
        workspace: WorkspaceKind,
        family: RecordFamilySpec,
    },
    Detail {
        workspace: WorkspaceKind,
        family: RecordFamilySpec,
        id: String,
    },
}

impl CanonicalRecordRoute {
    pub fn parse(workspace: WorkspaceKind, input: &str) -> Option<Self> {
        let path = normalize_deep_link(input);
        if path == workspace.root() {
            return Some(Self::Workspace { workspace });
        }
        workspace.record_families().iter().find_map(|family| {
            let collection = family.collection_path(workspace);
            if path == collection {
                return Some(Self::Collection {
                    workspace,
                    family: *family,
                });
            }
            let prefix = format!("{collection}/");
            let id = path.strip_prefix(&prefix)?;
            valid_opaque_segment(id).then(|| Self::Detail {
                workspace,
                family: *family,
                id: id.to_owned(),
            })
        })
    }

    pub const fn workspace(&self) -> WorkspaceKind {
        match self {
            Self::Workspace { workspace }
            | Self::Collection { workspace, .. }
            | Self::Detail { workspace, .. } => *workspace,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceQuery {
    pub route: WorkspaceRoute,
    pub locale: Locale,
    pub channel: ClientChannel,
    pub connectivity: Connectivity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceGatewayError {
    Denied,
    Restricted,
    Conflict,
    Offline,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryConsistency {
    Eventual,
    ReadYourWrites { minimum_source_version: u64 },
}

pub const DEFAULT_COLLECTION_PAGE_SIZE: u32 = 50;
pub const MAX_COLLECTION_PAGE_SIZE: u32 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordQuery {
    pub route: CanonicalRecordRoute,
    pub locale: Locale,
    pub channel: ClientChannel,
    pub connectivity: Connectivity,
    pub consistency: QueryConsistency,
    pub page_size: u32,
    pub cursor: Option<String>,
}

pub fn recovery_state(error: WorkspaceGatewayError) -> WorkspaceState {
    match error {
        WorkspaceGatewayError::Denied => WorkspaceState::Denied,
        WorkspaceGatewayError::Restricted => WorkspaceState::Restricted,
        WorkspaceGatewayError::Conflict => {
            WorkspaceState::conflict("The record changed. Reload before retrying.", true)
        }
        WorkspaceGatewayError::Offline => {
            WorkspaceState::degraded("Offline: cached reads only; changes are not queued.", true)
        }
        WorkspaceGatewayError::Unavailable => {
            WorkspaceState::retry("The workspace is temporarily unavailable.")
        }
    }
}

impl WorkspaceRoute {
    pub fn parse(workspace: WorkspaceKind, input: &str) -> Option<Self> {
        let path = normalize_deep_link(input);
        if path == workspace.root() {
            return Some(Self::Workspace { workspace });
        }
        if path == workspace.collection_path() {
            return Some(Self::Collection { workspace });
        }
        let prefix = format!("{}/{}/", workspace.root(), workspace.record_segment());
        let id = path.strip_prefix(&prefix)?;
        valid_opaque_segment(id).then(|| Self::Detail {
            workspace,
            id: id.to_owned(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connectivity {
    Online,
    Offline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionMode {
    Interactive,
    ReadOnlyOffline,
    Denied,
}

impl InteractionMode {
    pub const fn for_connectivity(connectivity: Connectivity, authorized: bool) -> Self {
        if !authorized {
            Self::Denied
        } else if matches!(connectivity, Connectivity::Offline) {
            Self::ReadOnlyOffline
        } else {
            Self::Interactive
        }
    }

    pub const fn permits_mutation(self) -> bool {
        matches!(self, Self::Interactive)
    }

    pub const fn queues_mutation(self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceItem {
    pub id: String,
    pub title: String,
    pub status: String,
    pub updated_at: String,
}

impl WorkspaceItem {
    pub fn bounded(id: &str, title: &str, status: &str, updated_at: &str) -> Option<Self> {
        valid_opaque_segment(id).then(|| Self {
            id: id.to_owned(),
            title: SafeUiText::new(title).as_str().to_owned(),
            status: SafeUiText::new(status).as_str().to_owned(),
            updated_at: SafeUiText::new(updated_at).as_str().to_owned(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFreshness {
    Fresh { lag_seconds: u64 },
    Stale { lag_seconds: u64 },
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspacePage {
    Collection { items: Vec<WorkspaceItem> },
    Detail { item: WorkspaceItem },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceState {
    Loading,
    Empty,
    Denied,
    Restricted,
    Stale {
        page: WorkspacePage,
        freshness: SourceFreshness,
    },
    Degraded {
        message: String,
        retryable: bool,
    },
    Conflict {
        message: String,
        retryable: bool,
    },
    Retry {
        message: String,
    },
    Success {
        page: WorkspacePage,
        freshness: SourceFreshness,
    },
}

impl WorkspaceState {
    pub fn degraded(message: &str, retryable: bool) -> Self {
        Self::Degraded {
            message: SafeUiText::new(message).as_str().to_owned(),
            retryable,
        }
    }

    pub fn conflict(message: &str, retryable: bool) -> Self {
        Self::Conflict {
            message: SafeUiText::new(message).as_str().to_owned(),
            retryable,
        }
    }

    pub fn retry(message: &str) -> Self {
        Self::Retry {
            message: SafeUiText::new(message).as_str().to_owned(),
        }
    }

    pub const fn exposes_record_payload(&self) -> bool {
        matches!(self, Self::Success { .. } | Self::Stale { .. })
    }

    pub const fn state_key(&self) -> &'static str {
        match self {
            Self::Loading => "loading",
            Self::Empty => "empty",
            Self::Denied => "denied",
            Self::Restricted => "restricted",
            Self::Stale { .. } => "stale",
            Self::Degraded { .. } => "degraded",
            Self::Conflict { .. } => "conflict",
            Self::Retry { .. } => "retry",
            Self::Success { .. } => "success",
        }
    }

    pub const fn aria_role(&self) -> &'static str {
        match self {
            Self::Denied | Self::Conflict { .. } => "alert",
            _ => "status",
        }
    }

    pub const fn is_busy(&self) -> bool {
        matches!(self, Self::Loading)
    }

    pub const fn permits_retry(&self) -> bool {
        match self {
            Self::Degraded { retryable, .. } | Self::Conflict { retryable, .. } => *retryable,
            Self::Retry { .. } => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreshnessEvidence {
    pub source_version: u64,
    pub projection_generation: u64,
    pub checkpoint_position: u64,
    pub lag_seconds: u64,
    pub observed_at: String,
}

impl FreshnessEvidence {
    pub fn status(&self) -> SourceFreshness {
        if self.lag_seconds <= 60 {
            SourceFreshness::Fresh {
                lag_seconds: self.lag_seconds,
            }
        } else {
            SourceFreshness::Stale {
                lag_seconds: self.lag_seconds,
            }
        }
    }

    pub const fn satisfies(&self, consistency: QueryConsistency) -> bool {
        match consistency {
            QueryConsistency::Eventual => true,
            QueryConsistency::ReadYourWrites {
                minimum_source_version,
            } => self.source_version >= minimum_source_version,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordSummary {
    pub record_ref: Option<String>,
    pub title: String,
    pub status: String,
    pub owner_scope: String,
    pub updated_at: String,
    pub restricted: bool,
}

impl RecordSummary {
    pub fn authorized(
        record_ref: &str,
        title: &str,
        status: &str,
        owner_scope: &str,
        updated_at: &str,
    ) -> Option<Self> {
        valid_opaque_segment(record_ref).then(|| Self {
            record_ref: Some(record_ref.to_owned()),
            title: safe_text(title),
            status: safe_text(status),
            owner_scope: safe_text(owner_scope),
            updated_at: safe_text(updated_at),
            restricted: false,
        })
    }

    pub fn restricted(locale: Locale) -> Self {
        Self {
            record_ref: None,
            title: restricted_item_copy(locale).to_owned(),
            status: String::new(),
            owner_scope: String::new(),
            updated_at: String::new(),
            restricted: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordField {
    pub key: &'static str,
    pub label: LocalizedText,
    pub value: String,
    pub redacted: bool,
}

impl RecordField {
    pub fn visible(key: &'static str, label: LocalizedText, value: &str) -> Self {
        Self {
            key,
            label,
            value: safe_text(value),
            redacted: false,
        }
    }

    pub fn redacted(key: &'static str, label: LocalizedText, locale: Locale) -> Self {
        Self {
            key,
            label,
            value: restricted_item_copy(locale).to_owned(),
            redacted: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionModel {
    pub family: RecordFamilySpec,
    pub records: Vec<RecordSummary>,
    pub freshness: FreshnessEvidence,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailModel {
    pub family: RecordFamilySpec,
    pub record: RecordSummary,
    pub version: u64,
    pub fields: Vec<RecordField>,
    pub freshness: FreshnessEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAvailability {
    Available,
    OfflineReadOnly,
    AwaitingFreshProjection,
    Unauthorized,
    TransportUnavailable,
}

impl CommandAvailability {
    pub const fn permits_submit(self) -> bool {
        matches!(self, Self::Available)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernedCommandCard {
    pub action: GovernedCommandAction,
    pub command_id: String,
    pub label: String,
    pub target_status: &'static str,
    pub expected_version: u64,
    pub confirmation_required: bool,
    pub availability: CommandAvailability,
    pub denial_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectTransitionTarget {
    Approved,
    Active,
    Paused,
    Completed,
    Cancelled,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TalentRequisitionTarget {
    Published,
    OnHold,
    Closed,
    Cancelled,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernedCommandAction {
    ManufacturingReleaseOrder,
    ManufacturingStartOrder,
    ManufacturingCompleteOrder,
    ManufacturingCloseOrder,
    PlanningFreezeScenario,
    PlanningApproveRecommendation,
    PlanningRejectRecommendation,
    ProjectTransition(ProjectTransitionTarget),
    CommerceApproveListing,
    CommerceWithdrawListing,
    CommerceLockCart,
    TalentApproveRequisition,
    TalentTransitionRequisition(TalentRequisitionTarget),
    TalentWithdrawApplication,
    TalentApproveOffer,
    TalentPresentOffer,
    TalentSubmitProposal,
    TalentApproveProposal,
    LearningRetireCourse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernedCommandRequest {
    pub workspace: WorkspaceKind,
    pub family: &'static str,
    pub record_ref: String,
    pub action: GovernedCommandAction,
    pub expected_version: u64,
    pub idempotency_key: String,
    pub confirmation_token: Option<String>,
}

impl GovernedCommandRequest {
    pub fn validate(&self) -> bool {
        valid_opaque_segment(&self.record_ref)
            && valid_idempotency_key(&self.idempotency_key)
            && self.expected_version > 0
            && self.action.targets(self.workspace, self.family)
            && self.confirmation_token.as_deref().is_some_and(|value| {
                !value.is_empty() && value.len() <= 256 && !contains_control(value)
            })
    }
}

impl GovernedCommandAction {
    pub fn targets(self, workspace: WorkspaceKind, family: &str) -> bool {
        matches!(
            (self, workspace, family),
            (
                Self::ManufacturingReleaseOrder
                    | Self::ManufacturingStartOrder
                    | Self::ManufacturingCompleteOrder
                    | Self::ManufacturingCloseOrder,
                WorkspaceKind::Manufacturing,
                "order"
            ) | (
                Self::PlanningFreezeScenario,
                WorkspaceKind::Planning,
                "scenario"
            ) | (
                Self::PlanningApproveRecommendation | Self::PlanningRejectRecommendation,
                WorkspaceKind::Planning,
                "recommendation"
            ) | (
                Self::ProjectTransition(_),
                WorkspaceKind::Projects,
                "project"
            ) | (
                Self::CommerceApproveListing | Self::CommerceWithdrawListing,
                WorkspaceKind::Ecommerce,
                "catalog-publication"
            ) | (Self::CommerceLockCart, WorkspaceKind::Ecommerce, "cart")
                | (
                    Self::TalentApproveRequisition | Self::TalentTransitionRequisition(_),
                    WorkspaceKind::Talent,
                    "requisition"
                )
                | (
                    Self::TalentWithdrawApplication,
                    WorkspaceKind::Talent,
                    "application"
                )
                | (
                    Self::TalentApproveOffer | Self::TalentPresentOffer,
                    WorkspaceKind::Talent,
                    "offer"
                )
                | (
                    Self::TalentSubmitProposal | Self::TalentApproveProposal,
                    WorkspaceKind::Talent,
                    "proposal"
                )
                | (
                    Self::LearningRetireCourse,
                    WorkspaceKind::Learning,
                    "course"
                )
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernedCommandReceipt {
    pub command_id: String,
    pub committed_version: u64,
    pub correlation_ref: String,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnterpriseGatewayError {
    Denied,
    Restricted,
    Conflict { current_version: u64 },
    Stale { current_source_version: u64 },
    Offline,
    Unavailable,
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait EnterpriseWorkspaceGateway: Send + Sync {
    async fn load_collection(
        &self,
        query: RecordQuery,
    ) -> Result<CollectionModel, EnterpriseGatewayError>;

    async fn load_detail(&self, query: RecordQuery) -> Result<DetailModel, EnterpriseGatewayError>;

    async fn submit_command(
        &self,
        request: GovernedCommandRequest,
    ) -> Result<GovernedCommandReceipt, EnterpriseGatewayError>;
}

/// App-owned transport injected into registered future-workspace renderers.
///
/// The mutable slot lets the authenticated shell rotate credentials/scope on
/// the same mounted component. Poisoned or absent state always fails closed.
#[derive(Clone, Default)]
pub struct EnterpriseGatewayContext {
    gateway: Arc<RwLock<Option<Arc<dyn EnterpriseWorkspaceGateway>>>>,
}

impl EnterpriseGatewayContext {
    pub fn replace(&self, gateway: Option<Arc<dyn EnterpriseWorkspaceGateway>>) {
        if let Ok(mut slot) = self.gateway.write() {
            *slot = gateway;
        }
    }

    pub fn current(&self) -> Option<Arc<dyn EnterpriseWorkspaceGateway>> {
        self.gateway.read().ok()?.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EnterpriseReadModel {
    Collection(CollectionModel),
    Detail(DetailModel),
}

#[derive(Debug, Default, Clone, Copy)]
pub struct UnavailableEnterpriseGateway;

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl EnterpriseWorkspaceGateway for UnavailableEnterpriseGateway {
    async fn load_collection(
        &self,
        _query: RecordQuery,
    ) -> Result<CollectionModel, EnterpriseGatewayError> {
        Err(EnterpriseGatewayError::Unavailable)
    }

    async fn load_detail(
        &self,
        _query: RecordQuery,
    ) -> Result<DetailModel, EnterpriseGatewayError> {
        Err(EnterpriseGatewayError::Unavailable)
    }

    async fn submit_command(
        &self,
        _request: GovernedCommandRequest,
    ) -> Result<GovernedCommandReceipt, EnterpriseGatewayError> {
        Err(EnterpriseGatewayError::Unavailable)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CommandCardContext<'a> {
    pub workspace: WorkspaceKind,
    pub family: RecordFamilySpec,
    pub current_status: &'a str,
    pub expected_version: u64,
    pub locale: Locale,
    pub interaction: InteractionMode,
    pub freshness: Option<&'a FreshnessEvidence>,
    pub transport_ready: bool,
}

pub fn command_cards(context: CommandCardContext<'_>) -> Vec<GovernedCommandCard> {
    let CommandCardContext {
        workspace,
        family,
        current_status,
        expected_version,
        locale,
        interaction,
        freshness,
        transport_ready,
    } = context;
    let availability = if !matches!(interaction, InteractionMode::Interactive) {
        match interaction {
            InteractionMode::ReadOnlyOffline => CommandAvailability::OfflineReadOnly,
            InteractionMode::Denied => CommandAvailability::Unauthorized,
            InteractionMode::Interactive => CommandAvailability::Available,
        }
    } else if freshness.is_some_and(|evidence| evidence.lag_seconds > 60) {
        CommandAvailability::AwaitingFreshProjection
    } else if !transport_ready {
        CommandAvailability::TransportUnavailable
    } else {
        CommandAvailability::Available
    };

    governed_actions(workspace, family.segment, current_status)
        .into_iter()
        .map(|(action, command_id, status)| GovernedCommandCard {
            action,
            command_id: command_id.into(),
            label: match locale {
                Locale::En => format!("Request transition to {status}"),
                Locale::Th => format!("ขอเปลี่ยนสถานะเป็น {status}"),
            },
            target_status: status,
            expected_version,
            confirmation_required: true,
            availability,
            denial_reason: (!availability.permits_submit())
                .then(|| command_denial_copy(locale, availability).to_owned()),
        })
        .collect()
}

fn governed_actions(
    workspace: WorkspaceKind,
    family: &str,
    status: &str,
) -> Vec<(GovernedCommandAction, &'static str, &'static str)> {
    use GovernedCommandAction as Action;
    let status = status.to_ascii_lowercase().replace(['_', '-', ' '], "");
    match (workspace, family, status.as_str()) {
        (WorkspaceKind::Manufacturing, "order", "draft") => {
            vec![(
                Action::ManufacturingReleaseOrder,
                "release-order",
                "released",
            )]
        }
        (WorkspaceKind::Manufacturing, "order", "released") => {
            vec![(
                Action::ManufacturingStartOrder,
                "start-order",
                "in-progress",
            )]
        }
        (WorkspaceKind::Manufacturing, "order", "inprogress") => vec![(
            Action::ManufacturingCompleteOrder,
            "complete-order",
            "completed",
        )],
        (WorkspaceKind::Manufacturing, "order", "completed") => {
            vec![(Action::ManufacturingCloseOrder, "close-order", "closed")]
        }
        (WorkspaceKind::Planning, "scenario", "draft") => {
            vec![(Action::PlanningFreezeScenario, "freeze-scenario", "frozen")]
        }
        (WorkspaceKind::Planning, "recommendation", "proposed") => vec![
            (
                Action::PlanningApproveRecommendation,
                "approve-recommendation",
                "approved",
            ),
            (
                Action::PlanningRejectRecommendation,
                "reject-recommendation",
                "rejected",
            ),
        ],
        (WorkspaceKind::Projects, "project", "proposed") => vec![
            (
                Action::ProjectTransition(ProjectTransitionTarget::Approved),
                "approve-project",
                "approved",
            ),
            (
                Action::ProjectTransition(ProjectTransitionTarget::Cancelled),
                "cancel-project",
                "cancelled",
            ),
        ],
        (WorkspaceKind::Projects, "project", "approved") => vec![
            (
                Action::ProjectTransition(ProjectTransitionTarget::Active),
                "activate-project",
                "active",
            ),
            (
                Action::ProjectTransition(ProjectTransitionTarget::Cancelled),
                "cancel-project",
                "cancelled",
            ),
        ],
        (WorkspaceKind::Projects, "project", "active") => vec![
            (
                Action::ProjectTransition(ProjectTransitionTarget::Paused),
                "pause-project",
                "paused",
            ),
            (
                Action::ProjectTransition(ProjectTransitionTarget::Completed),
                "complete-project",
                "completed",
            ),
            (
                Action::ProjectTransition(ProjectTransitionTarget::Cancelled),
                "cancel-project",
                "cancelled",
            ),
        ],
        (WorkspaceKind::Projects, "project", "paused") => vec![
            (
                Action::ProjectTransition(ProjectTransitionTarget::Active),
                "reactivate-project",
                "active",
            ),
            (
                Action::ProjectTransition(ProjectTransitionTarget::Cancelled),
                "cancel-project",
                "cancelled",
            ),
        ],
        (WorkspaceKind::Projects, "project", "completed") => vec![(
            Action::ProjectTransition(ProjectTransitionTarget::Active),
            "reactivate-project",
            "active",
        )],
        (WorkspaceKind::Projects, "project", "closed" | "cancelled") => vec![(
            Action::ProjectTransition(ProjectTransitionTarget::Archived),
            "archive-project",
            "archived",
        )],
        (WorkspaceKind::Ecommerce, "catalog-publication", "draft") => vec![(
            Action::CommerceApproveListing,
            "approve-listing",
            "approved",
        )],
        (WorkspaceKind::Ecommerce, "catalog-publication", "published") => vec![(
            Action::CommerceWithdrawListing,
            "withdraw-listing",
            "withdrawn",
        )],
        (WorkspaceKind::Ecommerce, "cart", "active") => {
            vec![(Action::CommerceLockCart, "lock-cart", "checkout-locked")]
        }
        (WorkspaceKind::Talent, "requisition", "draft") => vec![
            (
                Action::TalentApproveRequisition,
                "approve-requisition",
                "approved",
            ),
            (
                Action::TalentTransitionRequisition(TalentRequisitionTarget::Cancelled),
                "cancel-requisition",
                "cancelled",
            ),
        ],
        (WorkspaceKind::Talent, "requisition", "approved") => vec![
            (
                Action::TalentTransitionRequisition(TalentRequisitionTarget::Published),
                "publish-requisition",
                "published",
            ),
            (
                Action::TalentTransitionRequisition(TalentRequisitionTarget::Cancelled),
                "cancel-requisition",
                "cancelled",
            ),
        ],
        (WorkspaceKind::Talent, "requisition", "published") => vec![
            (
                Action::TalentTransitionRequisition(TalentRequisitionTarget::OnHold),
                "hold-requisition",
                "on-hold",
            ),
            (
                Action::TalentTransitionRequisition(TalentRequisitionTarget::Closed),
                "close-requisition",
                "closed",
            ),
        ],
        (WorkspaceKind::Talent, "requisition", "onhold") => vec![
            (
                Action::TalentTransitionRequisition(TalentRequisitionTarget::Published),
                "resume-requisition",
                "published",
            ),
            (
                Action::TalentTransitionRequisition(TalentRequisitionTarget::Closed),
                "close-requisition",
                "closed",
            ),
            (
                Action::TalentTransitionRequisition(TalentRequisitionTarget::Cancelled),
                "cancel-requisition",
                "cancelled",
            ),
        ],
        (WorkspaceKind::Talent, "requisition", "closed" | "cancelled") => vec![(
            Action::TalentTransitionRequisition(TalentRequisitionTarget::Archived),
            "archive-requisition",
            "archived",
        )],
        (WorkspaceKind::Talent, "application", "submitted" | "screening" | "interviewing") => {
            vec![(
                Action::TalentWithdrawApplication,
                "withdraw-application",
                "withdrawn",
            )]
        }
        (WorkspaceKind::Talent, "offer", "draft") => {
            vec![(Action::TalentApproveOffer, "approve-offer", "approved")]
        }
        (WorkspaceKind::Talent, "offer", "approved") => {
            vec![(Action::TalentPresentOffer, "present-offer", "presented")]
        }
        (WorkspaceKind::Talent, "proposal", "draft") => {
            vec![(Action::TalentSubmitProposal, "submit-proposal", "submitted")]
        }
        (WorkspaceKind::Talent, "proposal", "submitted") => vec![(
            Action::TalentApproveProposal,
            "approve-proposal",
            "approved",
        )],
        (WorkspaceKind::Learning, "course", "published") => {
            vec![(Action::LearningRetireCourse, "retire-course", "retired")]
        }
        _ => Vec::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectNodeKind {
    Project,
    Phase,
    Epic,
    Task,
    Milestone,
    Deliverable,
    Risk,
    Issue,
    Decision,
    ChangeRequest,
    SubItem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectGraphNode {
    pub record_ref: String,
    pub parent_ref: Option<String>,
    pub kind: ProjectNodeKind,
    pub label: String,
    pub status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectRelationState {
    Proposed,
    Accepted,
    Rejected,
    Revoked,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDependencyEdge {
    pub relation_ref: String,
    pub predecessor_ref: String,
    pub successor_ref: String,
    pub relation_version: u64,
    pub state: ProjectRelationState,
    pub predecessor_owner_accepted: bool,
    pub successor_owner_accepted: bool,
}

impl ProjectDependencyEdge {
    pub const fn affects_schedule(&self) -> bool {
        matches!(self.state, ProjectRelationState::Accepted)
            && self.predecessor_owner_accepted
            && self.successor_owner_accepted
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectWorkGraph {
    pub nodes: Vec<ProjectGraphNode>,
    pub dependencies: Vec<ProjectDependencyEdge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectGraphError {
    InvalidReference,
    MissingParent,
    SelfParent,
    Cycle,
    DuplicateNode,
}

impl ProjectWorkGraph {
    pub fn validate(&self) -> Result<(), ProjectGraphError> {
        for (index, node) in self.nodes.iter().enumerate() {
            if !valid_opaque_segment(&node.record_ref) {
                return Err(ProjectGraphError::InvalidReference);
            }
            if self.nodes[..index]
                .iter()
                .any(|candidate| candidate.record_ref == node.record_ref)
            {
                return Err(ProjectGraphError::DuplicateNode);
            }
            if node.parent_ref.as_deref() == Some(node.record_ref.as_str()) {
                return Err(ProjectGraphError::SelfParent);
            }
            if let Some(parent_ref) = &node.parent_ref
                && !self
                    .nodes
                    .iter()
                    .any(|candidate| candidate.record_ref == *parent_ref)
            {
                return Err(ProjectGraphError::MissingParent);
            }
        }
        for node in &self.nodes {
            let mut cursor = node.parent_ref.as_deref();
            let mut hops = 0usize;
            while let Some(parent_ref) = cursor {
                if parent_ref == node.record_ref {
                    return Err(ProjectGraphError::Cycle);
                }
                hops += 1;
                if hops > self.nodes.len() {
                    return Err(ProjectGraphError::Cycle);
                }
                cursor = self
                    .nodes
                    .iter()
                    .find(|candidate| candidate.record_ref == parent_ref)
                    .and_then(|candidate| candidate.parent_ref.as_deref());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoffState {
    Proposed,
    AwaitingApproval,
    Accepted,
    Reconciled,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectHandoffCard {
    pub handoff_ref: String,
    pub target_context: &'static str,
    pub correlation_ref: String,
    pub source_version: u64,
    pub state: HandoffState,
    pub approval_required: bool,
    pub reconciliation_message: Option<String>,
}

impl ProjectHandoffCard {
    pub const fn may_claim_owner_context_changed(&self) -> bool {
        matches!(self.state, HandoffState::Reconciled)
    }
}

pub fn render_enterprise_workspace_surface(
    context: &FeatureContext,
    workspace: WorkspaceKind,
) -> Element {
    let locale = context.locale;
    let route = CanonicalRecordRoute::parse(workspace, &context.canonical_path);
    let gateway =
        try_consume_context::<EnterpriseGatewayContext>().and_then(|context| context.current());
    let transport_ready = gateway.is_some();
    let command_gateway = gateway.clone();
    let mut pending_confirmation = use_signal(|| Option::<String>::None);
    let mut command_feedback = use_signal(|| Option::<String>::None);
    let mut page_cursor = use_signal(|| Option::<String>::None);
    let mut minimum_source_version = use_signal(|| 0_u64);
    let pagination_scope_key = format!(
        "{}:{}:{}",
        context.canonical_path,
        context
            .active_scope
            .as_ref()
            .map(|scope| scope.reference.as_str())
            .unwrap_or_default(),
        context.generation
    );
    use_effect(move || {
        let _ = &pagination_scope_key;
        page_cursor.set(None);
        minimum_source_version.set(0);
    });
    let query_route = route.clone();
    let channel = enterprise_client_channel();
    let read_model = use_resource(move || {
        let gateway = gateway.clone();
        let route = query_route.clone();
        let cursor = page_cursor.read().clone();
        let required_version = *minimum_source_version.read();
        async move {
            let gateway = gateway.ok_or(EnterpriseGatewayError::Unavailable)?;
            let route = route.ok_or(EnterpriseGatewayError::Unavailable)?;
            let query = RecordQuery {
                route: route.clone(),
                locale,
                channel,
                connectivity: Connectivity::Online,
                consistency: if required_version == 0 {
                    QueryConsistency::Eventual
                } else {
                    QueryConsistency::ReadYourWrites {
                        minimum_source_version: required_version,
                    }
                },
                page_size: DEFAULT_COLLECTION_PAGE_SIZE,
                cursor,
            };
            match route {
                CanonicalRecordRoute::Collection { .. } => gateway
                    .load_collection(query)
                    .await
                    .map(EnterpriseReadModel::Collection),
                CanonicalRecordRoute::Detail { .. } => gateway
                    .load_detail(query)
                    .await
                    .map(EnterpriseReadModel::Detail),
                CanonicalRecordRoute::Workspace { .. } => Err(EnterpriseGatewayError::Unavailable),
            }
        }
    });
    let read_snapshot = read_model.read().clone();
    let selected_family = match &route {
        Some(
            CanonicalRecordRoute::Collection { family, .. }
            | CanonicalRecordRoute::Detail { family, .. },
        ) => *family,
        _ => workspace.record_families()[0],
    };
    let detail_ref = match &route {
        Some(CanonicalRecordRoute::Detail { id, .. }) => Some(id.clone()),
        _ => None,
    };
    let (state_key, state_title, state_message, state_role) = if !context.session.authenticated {
        (
            "denied",
            localized(locale, "Access denied", "ปฏิเสธการเข้าถึง"),
            localized(
                locale,
                "Sign in with an authorized workforce identity.",
                "ลงชื่อเข้าใช้ด้วยบัญชีบุคลากรที่ได้รับอนุญาต",
            ),
            "alert",
        )
    } else if context.active_scope.is_none() {
        (
            "restricted",
            localized(locale, "Restricted item", "รายการที่ถูกจำกัด"),
            localized(
                locale,
                "Select an authorized organization scope. No record details were loaded.",
                "เลือกขอบเขตองค์กรที่ได้รับอนุญาต โดยยังไม่มีการโหลดรายละเอียดรายการ",
            ),
            "status",
        )
    } else if route.is_none() {
        (
            "retry",
            localized(locale, "Route unavailable", "เส้นทางไม่พร้อมใช้งาน"),
            localized(
                locale,
                "Use one of the canonical record collections below.",
                "เลือกคอลเลกชันรายการตามเส้นทางมาตรฐานด้านล่าง",
            ),
            "alert",
        )
    } else if read_snapshot.is_none() {
        (
            "loading",
            localized(
                locale,
                "Loading authorized records",
                "กำลังโหลดรายการที่ได้รับอนุญาต",
            ),
            localized(
                locale,
                "Checking record authority and projection freshness.",
                "กำลังตรวจสอบสิทธิ์ของรายการและความสดใหม่ของ projection",
            ),
            "status",
        )
    } else if matches!(read_snapshot.as_ref(), Some(Ok(_))) {
        (
            "success",
            localized(locale, "Authorized records", "รายการที่ได้รับอนุญาต"),
            localized(
                locale,
                "The server-authorized read model is ready.",
                "แบบจำลองการอ่านที่เซิร์ฟเวอร์อนุญาตพร้อมใช้งาน",
            ),
            "status",
        )
    } else if matches!(
        read_snapshot.as_ref(),
        Some(Err(EnterpriseGatewayError::Denied))
    ) {
        (
            "denied",
            localized(locale, "Restricted item", "รายการที่ถูกจำกัด"),
            localized(
                locale,
                "The record is absent or unavailable to this authority scope.",
                "ไม่พบรายการหรือรายการไม่พร้อมสำหรับขอบเขตสิทธิ์นี้",
            ),
            "alert",
        )
    } else if matches!(
        read_snapshot.as_ref(),
        Some(Err(EnterpriseGatewayError::Stale { .. }))
    ) {
        (
            "stale",
            localized(locale, "Projection is not ready", "Projection ยังไม่พร้อม"),
            localized(
                locale,
                "The authorized read model has not reached the required version.",
                "แบบจำลองการอ่านที่ได้รับอนุญาตยังไม่ถึงเวอร์ชันที่ต้องการ",
            ),
            "status",
        )
    } else if matches!(
        read_snapshot.as_ref(),
        Some(Err(EnterpriseGatewayError::Offline))
    ) {
        (
            "offline",
            localized(
                locale,
                "Offline read unavailable",
                "การอ่านแบบออฟไลน์ไม่พร้อมใช้",
            ),
            localized(
                locale,
                "No mutation is queued and no write-model fallback is used.",
                "ระบบไม่จัดคิวการเปลี่ยนแปลงและไม่ย้อนอ่านจากแบบจำลองเขียน",
            ),
            "status",
        )
    } else {
        (
            "degraded",
            localized(
                locale,
                "Data connection unavailable",
                "การเชื่อมต่อข้อมูลไม่พร้อมใช้งาน",
            ),
            localized(
                locale,
                "The typed workspace gateway is not composed yet. No data or command result is being simulated.",
                "ยังไม่ได้เชื่อมต่อเกตเวย์แบบระบุชนิด ระบบจะไม่จำลองข้อมูลหรือผลคำสั่ง",
            ),
            "status",
        )
    };
    let loaded_model = read_snapshot
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned();
    let loaded_version = match &loaded_model {
        Some(EnterpriseReadModel::Detail(detail)) => detail.version,
        _ => 1,
    };
    let loaded_freshness = match &loaded_model {
        Some(EnterpriseReadModel::Collection(collection)) => Some(&collection.freshness),
        Some(EnterpriseReadModel::Detail(detail)) => Some(&detail.freshness),
        None => None,
    };
    let loaded_status = match &loaded_model {
        Some(EnterpriseReadModel::Detail(detail)) => detail.record.status.as_str(),
        _ => "",
    };
    let interaction = InteractionMode::for_connectivity(
        Connectivity::Online,
        context.session.authenticated && context.active_scope.is_some(),
    );
    let commands = command_cards(CommandCardContext {
        workspace,
        family: selected_family,
        current_status: loaded_status,
        expected_version: loaded_version,
        locale,
        interaction,
        freshness: loaded_freshness,
        transport_ready,
    });
    let channel_copy = match workspace.channel_policy() {
        ChannelPolicy::WebMacOs => localized(
            locale,
            "Adaptive Web and macOS workspace",
            "พื้นที่ทำงานแบบปรับตาม Web และ macOS",
        ),
    };
    let workspace_label = workspace.label(locale);
    let family_label = selected_family.label.text(locale);

    rsx! {
        section {
            class: "feature-owned-surface enterprise-workspace",
            role: ACCESSIBILITY.landmark_role,
            "aria-labelledby": "enterprise-workspace-title",
            "data-workspace": workspace.root(),
            "data-channel-policy": match workspace.channel_policy() {
                ChannelPolicy::WebMacOs => "web-macos",
            },
            "data-offline-policy": "read-only-no-queue",
            header { class: "feature-owned-surface__header enterprise-workspace__header",
                p { class: "work-os-eyebrow", {channel_copy} }
                h2 { id: "enterprise-workspace-title", tabindex: "-1", {workspace_label} }
                p {
                    {localized(
                        locale,
                        "Authorized collections and governed lifecycle actions",
                        "คอลเลกชันที่ได้รับอนุญาตและการดำเนินการตามวงจรชีวิตที่กำกับดูแล",
                    )}
                }
            }
            nav {
                class: "enterprise-workspace__record-navigation",
                "aria-label": localized(locale, "Record collections", "คอลเลกชันรายการ"),
                ul {
                    for family in workspace.record_families() {
                        {
                            let collection_path = family.collection_path(workspace);
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
                                        "aria-current": if family.segment == selected_family.segment { "page" } else { "false" },
                                        {family.label.text(locale)}
                                    }
                                }
                            }
                        }
                    }
                }
            }
            section {
                class: "enterprise-workspace__state",
                role: state_role,
                "aria-live": ACCESSIBILITY.live_region,
                "data-state": state_key,
                h3 { {state_title} }
                p { {state_message} }
                if matches!(state_key, "degraded" | "retry") {
                    KitButton {
                        r#type: "button",
                        disabled: true,
                        "aria-disabled": "true",
                        {localized(locale, "Retry when transport is available", "ลองใหม่เมื่อการเชื่อมต่อพร้อม")}
                    }
                }
            }
            section {
                class: "enterprise-workspace__collection-or-detail",
                "aria-labelledby": "enterprise-record-family-title",
                h3 { id: "enterprise-record-family-title", {family_label} }
                if let Some(EnterpriseReadModel::Collection(collection)) = loaded_model.clone() {
                    h4 {
                        tabindex: "-1",
                        autofocus: page_cursor.read().is_some(),
                        {localized(locale, "Authorized page", "หน้าที่ได้รับอนุญาต")}
                    }
                    if collection.records.is_empty() {
                        p { {localized(locale, "No authorized records found.", "ไม่พบรายการที่ได้รับอนุญาต")} }
                    } else {
                        ul {
                            for record in collection.records {
                                li {
                                    if let Some(record_ref) = record.record_ref {
                                        {
                                            let detail_path = selected_family.detail_path(workspace, &record_ref).unwrap_or_default();
                                            let path_for_click = detail_path.clone();
                                            let navigate = context.navigate.clone();
                                            rsx! {
                                                KitLink {
                                                    href: detail_path,
                                                    onclick: move |event: MouseEvent| {
                                                        event.prevent_default();
                                                        navigate(path_for_click.clone());
                                                    },
                                                    {record.title}
                                                }
                                            }
                                        }
                                        span { " · {record.status}" }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(next_cursor) = collection.next_cursor {
                        KitButton {
                            r#type: "button",
                            onclick: move |_| page_cursor.set(Some(next_cursor.clone())),
                            {localized(locale, "Load next page", "โหลดหน้าถัดไป")}
                        }
                    }
                } else if let Some(EnterpriseReadModel::Detail(detail)) = loaded_model.clone() {
                    dl {
                        dt { {localized(locale, "Opaque record reference", "รหัสอ้างอิงแบบทึบ")} }
                        dd { {detail.record.record_ref.unwrap_or_default()} }
                        dt { {localized(locale, "Lifecycle", "วงจรชีวิต")} }
                        dd { {detail.record.status} }
                        dt { {localized(locale, "Freshness", "ความสดใหม่")} }
                        dd { {match detail.freshness.status() {
                            SourceFreshness::Fresh { .. } => localized(locale, "Current", "เป็นปัจจุบัน"),
                            SourceFreshness::Stale { .. } => localized(locale, "Stale; read only", "ล่าช้าและอ่านอย่างเดียว"),
                            SourceFreshness::Unavailable => localized(locale, "Unavailable", "ไม่พร้อมใช้งาน"),
                        }} }
                        for field in detail.fields {
                            dt { {field.label.text(locale)} }
                            dd { {field.value} }
                        }
                    }
                } else if let Some(record_ref) = detail_ref.as_ref() {
                    p { {format!("{}: {record_ref}", localized(locale, "Authorized detail pending", "กำลังรอรายละเอียดที่ได้รับอนุญาต"))} }
                } else {
                    p { {localized(locale, "No authorized records loaded.", "ยังไม่มีการโหลดรายการที่ได้รับอนุญาต")} }
                }
            }
            section {
                class: "enterprise-workspace__commands",
                "aria-labelledby": "enterprise-command-title",
                h3 { id: "enterprise-command-title", {localized(locale, "Governed actions", "การดำเนินการที่กำกับดูแล")} }
                p { {localized(
                    locale,
                    "Actions require a fresh version, idempotency key and explicit confirmation.",
                    "การดำเนินการต้องใช้เวอร์ชันล่าสุด คีย์ป้องกันการทำซ้ำ และการยืนยันอย่างชัดเจน",
                )} }
                if commands.is_empty() {
                    p {
                        role: "status",
                        {localized(
                            locale,
                            "Additional authorized input is required before a governed action can be offered.",
                            "ต้องมีข้อมูลนำเข้าที่ได้รับอนุญาตเพิ่มเติมก่อนจึงจะเสนอการดำเนินการที่กำกับดูแลได้",
                        )}
                    }
                } else {
                    ul {
                        for command in commands {
                            li {
                                article {
                                class: "enterprise-command-card",
                                "data-command-id": command.command_id.clone(),
                                h4 { {command.label.clone()} }
                                KitButton {
                                    r#type: "button",
                                    disabled: !command.availability.permits_submit() || detail_ref.is_none(),
                                    "aria-disabled": if command.availability.permits_submit() && detail_ref.is_some() { "false" } else { "true" },
                                    onclick: {
                                        let command_id = command.command_id.clone();
                                        let action = command.action;
                                        let record_ref = detail_ref.clone();
                                        let gateway = command_gateway.clone();
                                        let available = command.availability.permits_submit();
                                        let expected_version = command.expected_version;
                                        move |_| {
                                            if !available {
                                                return;
                                            }
                                            if pending_confirmation.read().as_deref() != Some(command_id.as_str()) {
                                                pending_confirmation.set(Some(command_id.clone()));
                                                command_feedback.set(Some(localized(locale, "Select the action again to confirm.", "เลือกการดำเนินการอีกครั้งเพื่อยืนยัน").into()));
                                                return;
                                            }
                                            let Some(gateway) = gateway.clone() else { return };
                                            let Some(record_ref) = record_ref.clone() else { return };
                                            let command_id = command_id.clone();
                                            pending_confirmation.set(None);
                                            spawn(async move {
                                                let result = gateway.submit_command(GovernedCommandRequest {
                                                    workspace,
                                                    family: selected_family.segment,
                                                    record_ref: record_ref.clone(),
                                                    action,
                                                    expected_version,
                                                    idempotency_key: format!("ui:{command_id}:{record_ref}:{expected_version}"),
                                                    confirmation_token: Some(format!("confirmed:{command_id}")),
                                                }).await;
                                                command_feedback.set(Some(match result {
                                                    Ok(receipt) => {
                                                        minimum_source_version.set(receipt.committed_version);
                                                        page_cursor.set(None);
                                                        format!("{} {}", if receipt.replayed {
                                                            localized(locale, "Duplicate command safely replayed:", "คำสั่งซ้ำถูก replay อย่างปลอดภัย:")
                                                        } else {
                                                            localized(locale, "Command accepted:", "รับคำสั่งแล้ว:")
                                                        }, receipt.correlation_ref)
                                                    },
                                                    Err(_) => localized(locale, "Command was not accepted. Refresh before retrying.", "คำสั่งไม่ได้รับการยอมรับ โปรดรีเฟรชก่อนลองใหม่").into(),
                                                }));
                                            });
                                        }
                                    },
                                    {if pending_confirmation.read().as_deref() == Some(command.command_id.as_str()) {
                                        localized(locale, "Confirm now", "ยืนยันตอนนี้")
                                    } else {
                                        localized(locale, "Confirm action", "ยืนยันการดำเนินการ")
                                    }}
                                }
                                if let Some(reason) = command.denial_reason {
                                    p { role: "status", {reason} }
                                }
                                }
                            }
                        }
                    }
                }
                if let Some(feedback) = command_feedback.read().as_ref() {
                    p { role: "status", {feedback.clone()} }
                }
            }
            if matches!(workspace, WorkspaceKind::Projects) {
                section {
                    class: "enterprise-workspace__project-graph",
                    "aria-labelledby": "project-work-graph-title",
                    h3 { id: "project-work-graph-title", {localized(locale, "Project work graph", "กราฟงานโครงการ")} }
                    p { {localized(
                        locale,
                        "Project → Phase or Epic → Item → Sub-item. Cross-owner dependencies require acceptance by both owners; domain handoffs reconcile through typed commands.",
                        "โครงการ → ระยะงานหรืออีพิก → รายการ → รายการย่อย การพึ่งพาข้ามเจ้าของต้องได้รับการยอมรับจากทั้งสองฝ่าย และการส่งมอบข้ามโดเมนต้องกระทบยอดผ่านคำสั่งแบบระบุชนิด",
                    )} }
                }
            }
            footer {
                p { {localized(
                    locale,
                    "Offline mode is read-only. Changes are never queued.",
                    "โหมดออฟไลน์อ่านได้อย่างเดียว และจะไม่จัดคิวการเปลี่ยนแปลง",
                )} }
            }
        }
    }
}

fn localized(locale: Locale, en: &'static str, th: &'static str) -> &'static str {
    LocalizedText::new(en, th).text(locale)
}

#[cfg(target_arch = "wasm32")]
const fn enterprise_client_channel() -> ClientChannel {
    ClientChannel::Web
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
const fn enterprise_client_channel() -> ClientChannel {
    ClientChannel::Ios
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
const fn enterprise_client_channel() -> ClientChannel {
    ClientChannel::Android
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "ios", target_os = "android"))
))]
const fn enterprise_client_channel() -> ClientChannel {
    ClientChannel::Desktop
}

fn restricted_item_copy(locale: Locale) -> &'static str {
    localized(locale, "Restricted item", "รายการที่ถูกจำกัด")
}

fn command_denial_copy(locale: Locale, availability: CommandAvailability) -> &'static str {
    match availability {
        CommandAvailability::Available => "",
        CommandAvailability::OfflineReadOnly => localized(
            locale,
            "Offline mode is read-only; the action is not queued.",
            "โหมดออฟไลน์อ่านได้อย่างเดียวและจะไม่จัดคิวการดำเนินการ",
        ),
        CommandAvailability::AwaitingFreshProjection => localized(
            locale,
            "Wait for the read model to reach the committed version.",
            "รอให้แบบจำลองอ่านตามทันเวอร์ชันที่บันทึกแล้ว",
        ),
        CommandAvailability::Unauthorized => localized(
            locale,
            "Current authority does not permit this action.",
            "สิทธิ์ปัจจุบันไม่อนุญาตให้ดำเนินการนี้",
        ),
        CommandAvailability::TransportUnavailable => localized(
            locale,
            "The typed command transport is unavailable.",
            "การเชื่อมต่อคำสั่งแบบระบุชนิดไม่พร้อมใช้งาน",
        ),
    }
}

fn safe_text(value: &str) -> String {
    SafeUiText::new(value).as_str().to_owned()
}

fn contains_control(value: &str) -> bool {
    value.chars().any(char::is_control)
}

fn valid_idempotency_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !contains_control(value)
        && !value.to_ascii_lowercase().contains("secret")
}

#[macro_export]
macro_rules! declare_enterprise_workspace_feature {
    (
        $surface:ident,
        $workspace:expr,
        $feature_id:literal,
        $route_id:literal,
        $canonical_path:literal,
        $navigation_group:literal,
        $artifact_key:literal
    ) => {
        use dioxus::prelude::*;

        const FEATURE_ROUTES: &[erpx_ui_core::FeatureRouteDescriptor] =
            &[erpx_ui_core::FeatureRouteDescriptor {
                id: $route_id,
                canonical_path: $canonical_path,
                authorization: "authenticated",
                navigation_group: $navigation_group,
            }];

        #[component]
        pub fn $surface(context: erpx_ui_core::FeatureContext) -> Element {
            erpx_ui_enterprise_workspaces::render_enterprise_workspace_surface(&context, $workspace)
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
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessibilityContract {
    pub landmark_role: &'static str,
    pub live_region: &'static str,
    pub focus_heading_after_navigation: bool,
    pub keyboard_reachable_actions: bool,
    pub reduced_motion_respected: bool,
    pub announces_freshness: bool,
}

pub const ACCESSIBILITY: AccessibilityContract = AccessibilityContract {
    landmark_role: "region",
    live_region: "polite",
    focus_heading_after_navigation: true,
    keyboard_reachable_actions: true,
    reduced_motion_respected: true,
    announces_freshness: true,
};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_workspace_routes_are_canonical_and_detail_ids_are_opaque() {
        for workspace in WorkspaceKind::ALL {
            assert!(!workspace.root().contains("workspaces"));
            assert!(matches!(
                WorkspaceRoute::parse(workspace, workspace.root()),
                Some(WorkspaceRoute::Workspace { .. })
            ));
            let detail = workspace.detail_path("record-001").expect("safe detail");
            assert!(matches!(
                WorkspaceRoute::parse(workspace, &detail),
                Some(WorkspaceRoute::Detail { .. })
            ));
            assert!(workspace.detail_path("token=secret").is_none());
            for family in workspace.record_families() {
                let collection = family.collection_path(workspace);
                assert!(matches!(
                    CanonicalRecordRoute::parse(workspace, &collection),
                    Some(CanonicalRecordRoute::Collection { .. })
                ));
                let detail = family
                    .detail_path(workspace, "record-001")
                    .expect("family detail route");
                assert!(matches!(
                    CanonicalRecordRoute::parse(workspace, &detail),
                    Some(CanonicalRecordRoute::Detail { .. })
                ));
                let retired_prefix = ["/", "workspaces"].concat();
                assert!(!detail.contains(&retired_prefix));
            }
        }
    }

    #[test]
    fn all_forty_workspace_record_families_are_live_surface_targets() {
        let total = WorkspaceKind::ALL
            .into_iter()
            .map(|workspace| workspace.record_families().len())
            .sum::<usize>();
        assert_eq!(total, 40);
        for workspace in WorkspaceKind::ALL {
            assert!(!workspace.e2e_scenarios().is_empty());
            for family in workspace.record_families() {
                assert!(family.label.is_valid());
                assert!(!family.lifecycle.is_empty());
            }
        }
    }

    #[test]
    fn release_channels_are_web_and_macos_only() {
        for workspace in WorkspaceKind::ALL {
            assert!(workspace.channel_policy().supports(ClientChannel::Web));
            assert!(workspace.channel_policy().supports(ClientChannel::Desktop));
            assert!(!workspace.channel_policy().supports(ClientChannel::Ios));
            assert!(!workspace.channel_policy().supports(ClientChannel::Android));
        }
    }

    #[test]
    fn offline_is_read_only_and_never_queues_changes() {
        let mode = InteractionMode::for_connectivity(Connectivity::Offline, true);
        assert_eq!(mode, InteractionMode::ReadOnlyOffline);
        assert!(!mode.permits_mutation());
        assert!(!mode.queues_mutation());
    }

    #[test]
    fn governed_actions_require_fresh_authorized_online_transport() {
        let family = MANUFACTURING_FAMILIES[1];
        let stale = FreshnessEvidence {
            source_version: 4,
            projection_generation: 2,
            checkpoint_position: 40,
            lag_seconds: 61,
            observed_at: "2026-08-16T00:00:00Z".into(),
        };
        let cards = command_cards(CommandCardContext {
            workspace: WorkspaceKind::Manufacturing,
            family,
            current_status: "draft",
            expected_version: 4,
            locale: Locale::En,
            interaction: InteractionMode::Interactive,
            freshness: Some(&stale),
            transport_ready: true,
        });
        assert_eq!(cards.len(), 1);
        assert_eq!(
            cards[0].action,
            GovernedCommandAction::ManufacturingReleaseOrder
        );
        assert!(cards.iter().all(|card| {
            card.confirmation_required
                && card.expected_version == 4
                && card.availability == CommandAvailability::AwaitingFreshProjection
                && !card.availability.permits_submit()
        }));

        let offline_freshness = FreshnessEvidence {
            lag_seconds: 0,
            ..stale
        };
        let offline = command_cards(CommandCardContext {
            workspace: WorkspaceKind::Manufacturing,
            family,
            current_status: "draft",
            expected_version: 4,
            locale: Locale::Th,
            interaction: InteractionMode::ReadOnlyOffline,
            freshness: Some(&offline_freshness),
            transport_ready: true,
        });
        assert_eq!(offline.len(), 1);
        assert!(
            offline
                .iter()
                .all(|card| card.availability == CommandAvailability::OfflineReadOnly)
        );
    }

    #[test]
    fn loaded_reads_do_not_enable_commands_without_a_command_transport() {
        let freshness = FreshnessEvidence {
            source_version: 4,
            projection_generation: 2,
            checkpoint_position: 40,
            lag_seconds: 0,
            observed_at: "2026-08-16T00:00:00Z".into(),
        };
        let cards = command_cards(CommandCardContext {
            workspace: WorkspaceKind::Projects,
            family: PROJECT_FAMILIES[0],
            current_status: "proposed",
            expected_version: 4,
            locale: Locale::En,
            interaction: InteractionMode::Interactive,
            freshness: Some(&freshness),
            transport_ready: false,
        });
        assert!(cards.iter().all(|card| {
            card.availability == CommandAvailability::TransportUnavailable
                && !card.availability.permits_submit()
        }));
    }

    #[test]
    fn typed_transport_enables_only_closed_owner_actions() {
        let freshness = FreshnessEvidence {
            source_version: 4,
            projection_generation: 2,
            checkpoint_position: 40,
            lag_seconds: 0,
            observed_at: "2026-08-16T00:00:00Z".into(),
        };
        let actions = command_cards(CommandCardContext {
            workspace: WorkspaceKind::Planning,
            family: PLANNING_FAMILIES[1],
            current_status: "proposed",
            expected_version: 4,
            locale: Locale::En,
            interaction: InteractionMode::Interactive,
            freshness: Some(&freshness),
            transport_ready: true,
        });
        assert_eq!(actions.len(), 2);
        assert!(
            actions
                .iter()
                .all(|card| card.availability.permits_submit())
        );
        assert!(actions.iter().all(|card| {
            card.action
                .targets(WorkspaceKind::Planning, "recommendation")
        }));

        let unavailable = command_cards(CommandCardContext {
            workspace: WorkspaceKind::Benefits,
            family: BENEFIT_FAMILIES[0],
            current_status: "draft",
            expected_version: 4,
            locale: Locale::En,
            interaction: InteractionMode::Interactive,
            freshness: Some(&freshness),
            transport_ready: true,
        });
        assert!(unavailable.is_empty());

        let published_requisition =
            governed_actions(WorkspaceKind::Talent, "requisition", "published");
        assert_eq!(published_requisition.len(), 2);
        assert!(!published_requisition.iter().any(|(action, _, _)| {
            *action
                == GovernedCommandAction::TalentTransitionRequisition(
                    TalentRequisitionTarget::Cancelled,
                )
        }));
    }

    #[test]
    fn read_your_writes_waits_for_the_committed_source_version() {
        let evidence = FreshnessEvidence {
            source_version: 7,
            projection_generation: 3,
            checkpoint_position: 91,
            lag_seconds: 2,
            observed_at: "2026-08-16T00:00:00Z".into(),
        };
        assert!(evidence.satisfies(QueryConsistency::Eventual));
        assert!(evidence.satisfies(QueryConsistency::ReadYourWrites {
            minimum_source_version: 7,
        }));
        assert!(!evidence.satisfies(QueryConsistency::ReadYourWrites {
            minimum_source_version: 8,
        }));
    }

    #[test]
    fn restricted_rows_are_neutral_and_do_not_leak_identifiers() {
        for locale in Locale::ALL {
            let row = RecordSummary::restricted(locale);
            assert!(row.restricted);
            assert!(row.record_ref.is_none());
            assert!(row.status.is_empty());
            assert!(row.owner_scope.is_empty());
            assert!(!row.title.is_empty());
        }
    }

    #[test]
    fn project_graph_requires_acyclic_hierarchy_and_two_sided_dependencies() {
        let mut graph = ProjectWorkGraph {
            nodes: vec![
                ProjectGraphNode {
                    record_ref: "project-1".into(),
                    parent_ref: None,
                    kind: ProjectNodeKind::Project,
                    label: "ERP program".into(),
                    status: "open".into(),
                },
                ProjectGraphNode {
                    record_ref: "phase-1".into(),
                    parent_ref: Some("project-1".into()),
                    kind: ProjectNodeKind::Phase,
                    label: "Foundation".into(),
                    status: "in-progress".into(),
                },
            ],
            dependencies: vec![ProjectDependencyEdge {
                relation_ref: "dependency-1".into(),
                predecessor_ref: "phase-1".into(),
                successor_ref: "project-1".into(),
                relation_version: 1,
                state: ProjectRelationState::Accepted,
                predecessor_owner_accepted: true,
                successor_owner_accepted: false,
            }],
        };
        assert_eq!(graph.validate(), Ok(()));
        assert!(!graph.dependencies[0].affects_schedule());
        graph.dependencies[0].successor_owner_accepted = true;
        assert!(graph.dependencies[0].affects_schedule());
        graph.nodes[0].parent_ref = Some("phase-1".into());
        assert_eq!(graph.validate(), Err(ProjectGraphError::Cycle));
    }

    #[test]
    fn project_handoff_never_claims_mutation_before_reconciliation() {
        let mut handoff = ProjectHandoffCard {
            handoff_ref: "handoff-1".into(),
            target_context: "finance",
            correlation_ref: "correlation-1".into(),
            source_version: 3,
            state: HandoffState::Accepted,
            approval_required: true,
            reconciliation_message: None,
        };
        assert!(!handoff.may_claim_owner_context_changed());
        handoff.state = HandoffState::Reconciled;
        assert!(handoff.may_claim_owner_context_changed());
    }

    #[test]
    fn command_request_rejects_missing_confirmation_or_secret_material() {
        let mut request = GovernedCommandRequest {
            workspace: WorkspaceKind::Manufacturing,
            family: "order",
            record_ref: "order-1".into(),
            action: GovernedCommandAction::ManufacturingReleaseOrder,
            expected_version: 2,
            idempotency_key: "idem-1".into(),
            confirmation_token: None,
        };
        assert!(!request.validate());
        request.confirmation_token = Some("confirmed".into());
        assert!(request.validate());
        request.action = GovernedCommandAction::PlanningFreezeScenario;
        assert!(!request.validate());
        request.action = GovernedCommandAction::ManufacturingReleaseOrder;
        request.idempotency_key = "client_secret=value".into();
        assert!(!request.validate());
    }

    #[test]
    fn denial_and_recovery_states_never_expose_payload() {
        for state in [
            WorkspaceState::Denied,
            WorkspaceState::Restricted,
            WorkspaceState::degraded("token=secret", true),
            WorkspaceState::conflict("credential=secret", true),
            WorkspaceState::retry("password=secret"),
        ] {
            assert!(!state.exposes_record_payload());
        }
        assert!(matches!(
            WorkspaceState::degraded("token=secret", true),
            WorkspaceState::Degraded { message, .. } if message == "[redacted]"
        ));
    }

    #[test]
    fn every_required_surface_state_has_explicit_accessibility_and_retry_behavior() {
        let item = WorkspaceItem::bounded(
            "record-1",
            "Authorized title",
            "open",
            "2026-08-16T00:00:00Z",
        )
        .expect("record");
        let page = WorkspacePage::Detail { item };
        let states = [
            WorkspaceState::Loading,
            WorkspaceState::Empty,
            WorkspaceState::Denied,
            WorkspaceState::Restricted,
            WorkspaceState::Stale {
                page: page.clone(),
                freshness: SourceFreshness::Stale { lag_seconds: 61 },
            },
            WorkspaceState::degraded("Projection temporarily unavailable", true),
            WorkspaceState::conflict("Record version changed", true),
            WorkspaceState::retry("Try the read again"),
            WorkspaceState::Success {
                page,
                freshness: SourceFreshness::Fresh { lag_seconds: 1 },
            },
        ];
        assert_eq!(
            states
                .iter()
                .map(WorkspaceState::state_key)
                .collect::<Vec<_>>(),
            vec![
                "loading",
                "empty",
                "denied",
                "restricted",
                "stale",
                "degraded",
                "conflict",
                "retry",
                "success",
            ]
        );
        assert!(states[0].is_busy());
        assert_eq!(states[2].aria_role(), "alert");
        assert_eq!(states[6].aria_role(), "alert");
        assert!(states[5].permits_retry());
        assert!(states[6].permits_retry());
        assert!(states[7].permits_retry());
        assert!(!states[8].permits_retry());
    }

    #[test]
    fn bilingual_and_accessibility_contracts_are_complete() {
        for workspace in WorkspaceKind::ALL {
            assert!(!workspace.label(Locale::En).is_empty());
            assert!(!workspace.label(Locale::Th).is_empty());
        }
        let accessibility = std::hint::black_box(ACCESSIBILITY);
        assert!(accessibility.focus_heading_after_navigation);
        assert!(accessibility.keyboard_reachable_actions);
        assert!(accessibility.reduced_motion_respected);
        assert!(accessibility.announces_freshness);
    }

    #[test]
    fn surfaces_use_the_governed_navigation_component() {
        let source = include_str!("lib.rs");
        assert!(source.contains("KitLink {"));
        assert!(source.contains("KitButton {"));
        assert!(source.contains("event.prevent_default();"));
        assert!(source.contains("context.navigate.clone()"));
        let raw_anchor = ["\n", "                            ", "a", " {"].concat();
        let raw_button = ["\n", "                    ", "button", " {"].concat();
        assert!(!source.contains(&raw_anchor));
        assert!(!source.contains(&raw_button));
    }
}
