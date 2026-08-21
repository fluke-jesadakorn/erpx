//! Compile-time authority for ERPX context, workspace, route, and service IDs.
//!
//! This module deliberately contains no transport or persistence types. Code
//! generators and adapters consume the catalog; they do not own it.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Delivery lifecycle of a bounded context in the clean baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextLifecycle {
    Current,
    EnterpriseTarget,
    CollaborationTarget,
}

/// Stable owner ID for every write-authoritative ERPX bounded context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextId {
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
    Manufacturing,
    Planning,
    Projects,
    Ecommerce,
    RecruitingTalent,
    Learning,
    Benefits,
    FieldService,
    Collaboration,
    AgentOrchestration,
}

impl ContextId {
    pub const ALL: [Self; 31] = [
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
        Self::Manufacturing,
        Self::Planning,
        Self::Projects,
        Self::Ecommerce,
        Self::RecruitingTalent,
        Self::Learning,
        Self::Benefits,
        Self::FieldService,
        Self::Collaboration,
        Self::AgentOrchestration,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::IdentityAccess => "identity-access",
            Self::PlatformAdministration => "platform-administration",
            Self::Organization => "organization",
            Self::Commercial => "commercial",
            Self::Brand => "brand",
            Self::CustomerAuthority => "customer-authority",
            Self::Customer360 => "customer-360",
            Self::Support => "support",
            Self::Finance => "finance",
            Self::Sales => "sales",
            Self::PointOfSale => "point-of-sale",
            Self::Procurement => "procurement",
            Self::Inventory => "inventory",
            Self::Assets => "assets",
            Self::Maintenance => "maintenance",
            Self::Quality => "quality",
            Self::Operations => "operations",
            Self::PeoplePayroll => "people-payroll",
            Self::Legal => "legal",
            Self::Marketing => "marketing",
            Self::Crm => "crm",
            Self::Manufacturing => "manufacturing",
            Self::Planning => "planning",
            Self::Projects => "projects",
            Self::Ecommerce => "ecommerce",
            Self::RecruitingTalent => "recruiting-talent",
            Self::Learning => "learning",
            Self::Benefits => "benefits",
            Self::FieldService => "field-service",
            Self::Collaboration => "collaboration",
            Self::AgentOrchestration => "agent-orchestration",
        }
    }

    pub const fn proto_package(self) -> &'static str {
        match self {
            Self::IdentityAccess => "erpx.identity_access",
            Self::PlatformAdministration => "erpx.platform_administration",
            Self::Organization => "erpx.organization",
            Self::Commercial => "erpx.commercial",
            Self::Brand => "erpx.brand",
            Self::CustomerAuthority => "erpx.customer_authority",
            Self::Customer360 => "erpx.customer_360",
            Self::Support => "erpx.support",
            Self::Finance => "erpx.finance",
            Self::Sales => "erpx.sales",
            Self::PointOfSale => "erpx.pos",
            Self::Procurement => "erpx.procurement",
            Self::Inventory => "erpx.inventory",
            Self::Assets => "erpx.assets",
            Self::Maintenance => "erpx.maintenance",
            Self::Quality => "erpx.quality",
            Self::Operations => "erpx.operations",
            Self::PeoplePayroll => "erpx.people_payroll",
            Self::Legal => "erpx.legal",
            Self::Marketing => "erpx.marketing",
            Self::Crm => "erpx.crm",
            Self::Manufacturing => "erpx.manufacturing",
            Self::Planning => "erpx.planning",
            Self::Projects => "erpx.projects",
            Self::Ecommerce => "erpx.ecommerce",
            Self::RecruitingTalent => "erpx.talent",
            Self::Learning => "erpx.learning",
            Self::Benefits => "erpx.benefits",
            Self::FieldService => "erpx.field_service",
            Self::Collaboration => "erpx.collaboration",
            Self::AgentOrchestration => "erpx.agent",
        }
    }

    pub const fn lifecycle(self) -> ContextLifecycle {
        match self {
            Self::Manufacturing
            | Self::Planning
            | Self::Projects
            | Self::Ecommerce
            | Self::RecruitingTalent
            | Self::Learning
            | Self::Benefits
            | Self::FieldService => ContextLifecycle::EnterpriseTarget,
            Self::Collaboration | Self::AgentOrchestration => ContextLifecycle::CollaborationTarget,
            _ => ContextLifecycle::Current,
        }
    }
}

impl fmt::Display for ContextId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.slug())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChannelId {
    Web,
    Desktop,
    Ios,
    Android,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelDescriptor {
    pub id: ChannelId,
    pub slug: &'static str,
    /// Business and collaboration mutations require current online authority.
    pub online_mutations_required: bool,
    pub offline_read_only: bool,
}

pub const CHANNELS: [ChannelDescriptor; 4] = [
    ChannelDescriptor {
        id: ChannelId::Web,
        slug: "web",
        online_mutations_required: true,
        offline_read_only: true,
    },
    ChannelDescriptor {
        id: ChannelId::Desktop,
        slug: "desktop",
        online_mutations_required: true,
        offline_read_only: true,
    },
    ChannelDescriptor {
        id: ChannelId::Ios,
        slug: "ios",
        online_mutations_required: true,
        offline_read_only: true,
    },
    ChannelDescriptor {
        id: ChannelId::Android,
        slug: "android",
        online_mutations_required: true,
        offline_read_only: true,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelSet(u8);

impl ChannelSet {
    const WEB: u8 = 1;
    const DESKTOP: u8 = 1 << 1;
    const IOS: u8 = 1 << 2;
    const ANDROID: u8 = 1 << 3;

    pub const WEB_ONLY: Self = Self(Self::WEB);
    pub const FOUR_CHANNEL: Self = Self(Self::WEB | Self::DESKTOP | Self::IOS | Self::ANDROID);

    pub const fn contains(self, channel: ChannelId) -> bool {
        let flag = match channel {
            ChannelId::Web => Self::WEB,
            ChannelId::Desktop => Self::DESKTOP,
            ChannelId::Ios => Self::IOS,
            ChannelId::Android => Self::ANDROID,
        };
        self.0 & flag == flag
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RealmId {
    InternalWorkforce,
    PlatformAdministration,
    CustomerPortal,
    CommercePortal,
    CandidatePortal,
    SignatoryPortal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RealmDescriptor {
    pub id: RealmId,
    pub slug: &'static str,
    pub external_portal: bool,
    pub internal_ai_launcher: bool,
    pub internal_notes_visible: bool,
}

pub const REALMS: [RealmDescriptor; 6] = [
    RealmDescriptor {
        id: RealmId::InternalWorkforce,
        slug: "internal-workforce",
        external_portal: false,
        internal_ai_launcher: true,
        internal_notes_visible: true,
    },
    RealmDescriptor {
        id: RealmId::PlatformAdministration,
        slug: "platform-administration",
        external_portal: false,
        internal_ai_launcher: true,
        internal_notes_visible: true,
    },
    RealmDescriptor {
        id: RealmId::CustomerPortal,
        slug: "customer-portal",
        external_portal: true,
        internal_ai_launcher: false,
        internal_notes_visible: false,
    },
    RealmDescriptor {
        id: RealmId::CommercePortal,
        slug: "commerce-portal",
        external_portal: true,
        internal_ai_launcher: false,
        internal_notes_visible: false,
    },
    RealmDescriptor {
        id: RealmId::CandidatePortal,
        slug: "candidate-portal",
        external_portal: true,
        internal_ai_launcher: false,
        internal_notes_visible: false,
    },
    RealmDescriptor {
        id: RealmId::SignatoryPortal,
        slug: "signatory-portal",
        external_portal: true,
        internal_ai_launcher: false,
        internal_notes_visible: false,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RealmSet(u8);

impl RealmSet {
    const INTERNAL_WORKFORCE_FLAG: u8 = 1;
    const PLATFORM_FLAG: u8 = 1 << 1;
    const CUSTOMER_FLAG: u8 = 1 << 2;
    const COMMERCE_FLAG: u8 = 1 << 3;
    const CANDIDATE_FLAG: u8 = 1 << 4;
    const SIGNATORY_FLAG: u8 = 1 << 5;

    pub const INTERNAL: Self = Self(Self::INTERNAL_WORKFORCE_FLAG | Self::PLATFORM_FLAG);
    pub const INTERNAL_AND_COMMERCE: Self =
        Self(Self::INTERNAL_WORKFORCE_FLAG | Self::PLATFORM_FLAG | Self::COMMERCE_FLAG);
    pub const INTERNAL_AND_CANDIDATE: Self =
        Self(Self::INTERNAL_WORKFORCE_FLAG | Self::PLATFORM_FLAG | Self::CANDIDATE_FLAG);
    pub const CUSTOMER_PORTAL: Self = Self(Self::CUSTOMER_FLAG);
    pub const SIGNATORY_PORTAL: Self = Self(Self::SIGNATORY_FLAG);

    pub const fn contains(self, realm: RealmId) -> bool {
        let flag = match realm {
            RealmId::InternalWorkforce => Self::INTERNAL_WORKFORCE_FLAG,
            RealmId::PlatformAdministration => Self::PLATFORM_FLAG,
            RealmId::CustomerPortal => Self::CUSTOMER_FLAG,
            RealmId::CommercePortal => Self::COMMERCE_FLAG,
            RealmId::CandidatePortal => Self::CANDIDATE_FLAG,
            RealmId::SignatoryPortal => Self::SIGNATORY_FLAG,
        };
        self.0 & flag == flag
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WorkspaceId {
    Operations,
    Finance,
    Sales,
    Procurement,
    Inventory,
    Assets,
    Maintenance,
    Quality,
    MarketingCrm,
    HrPayroll,
    PointOfSale,
    Legal,
    Manufacturing,
    Planning,
    Projects,
    Ecommerce,
    RecruitingTalent,
    Learning,
    Benefits,
    FieldService,
    AiCollaboration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceDescriptor {
    pub id: WorkspaceId,
    pub slug: &'static str,
    pub route: &'static str,
    pub channels: ChannelSet,
    pub realms: RealmSet,
}

macro_rules! workspace {
    ($id:ident, $slug:literal, $channels:expr, $realms:expr) => {
        WorkspaceDescriptor {
            id: WorkspaceId::$id,
            slug: $slug,
            route: concat!("/", $slug),
            channels: $channels,
            realms: $realms,
        }
    };
}

pub const WORKSPACES: [WorkspaceDescriptor; 21] = [
    workspace!(
        Operations,
        "operations",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
    workspace!(
        Finance,
        "finance",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
    workspace!(Sales, "sales", ChannelSet::FOUR_CHANNEL, RealmSet::INTERNAL),
    workspace!(
        Procurement,
        "procurement",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
    workspace!(
        Inventory,
        "inventory",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
    workspace!(
        Assets,
        "asset",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
    workspace!(
        Maintenance,
        "maintenance",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
    workspace!(
        Quality,
        "quality",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
    workspace!(
        MarketingCrm,
        "marketing",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
    workspace!(
        HrPayroll,
        "people",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
    workspace!(
        PointOfSale,
        "pos",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
    workspace!(Legal, "legal", ChannelSet::FOUR_CHANNEL, RealmSet::INTERNAL),
    workspace!(
        Manufacturing,
        "manufacturing",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
    workspace!(
        Planning,
        "planning",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
    workspace!(
        Projects,
        "project",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
    workspace!(
        Ecommerce,
        "commerce",
        ChannelSet::WEB_ONLY,
        RealmSet::INTERNAL_AND_COMMERCE
    ),
    workspace!(
        RecruitingTalent,
        "talent",
        ChannelSet::WEB_ONLY,
        RealmSet::INTERNAL_AND_CANDIDATE
    ),
    workspace!(
        Learning,
        "learning",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
    workspace!(
        Benefits,
        "benefits",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
    workspace!(
        FieldService,
        "field-service",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
    workspace!(
        AiCollaboration,
        "ai",
        ChannelSet::FOUR_CHANNEL,
        RealmSet::INTERNAL
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServiceId(u16);

impl ServiceId {
    const fn catalog(value: u16) -> Self {
        Self(value)
    }

    pub const fn number(self) -> u16 {
        self.0
    }
}

impl fmt::Display for ServiceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "IFC-SVC-{:03}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceKind {
    Command,
    Query,
    Stream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceOwner {
    Context(ContextId),
    FederatedItemDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceDescriptor {
    pub id: ServiceId,
    pub name: &'static str,
    pub package: &'static str,
    pub owner: ServiceOwner,
    pub kind: ServiceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ServicePair {
    command: ServiceDescriptor,
    query: ServiceDescriptor,
}

macro_rules! context_services {
    ($command_id:literal, $query_id:literal, $context:ident, $command:literal, $query:literal) => {
        ServicePair {
            command: ServiceDescriptor {
                id: ServiceId::catalog($command_id),
                name: $command,
                package: ContextId::$context.proto_package(),
                owner: ServiceOwner::Context(ContextId::$context),
                kind: ServiceKind::Command,
            },
            query: ServiceDescriptor {
                id: ServiceId::catalog($query_id),
                name: $query,
                package: ContextId::$context.proto_package(),
                owner: ServiceOwner::Context(ContextId::$context),
                kind: ServiceKind::Query,
            },
        }
    };
}

const CONTEXT_SERVICE_PAIRS: [ServicePair; 31] = [
    context_services!(
        201,
        202,
        IdentityAccess,
        "IdentityAccessCommandService",
        "IdentityAccessQueryService"
    ),
    context_services!(
        203,
        204,
        PlatformAdministration,
        "PlatformAdministrationCommandService",
        "PlatformAdministrationQueryService"
    ),
    context_services!(
        205,
        206,
        Organization,
        "OrganizationCommandService",
        "OrganizationQueryService"
    ),
    context_services!(
        207,
        208,
        Commercial,
        "CommercialCommandService",
        "CommercialQueryService"
    ),
    context_services!(209, 210, Brand, "BrandCommandService", "BrandQueryService"),
    context_services!(
        211,
        212,
        CustomerAuthority,
        "CustomerAuthorityCommandService",
        "CustomerAuthorityQueryService"
    ),
    context_services!(
        213,
        214,
        Customer360,
        "Customer360CommandService",
        "Customer360QueryService"
    ),
    context_services!(
        215,
        216,
        Support,
        "SupportCommandService",
        "SupportQueryService"
    ),
    context_services!(
        217,
        218,
        Finance,
        "FinanceCommandService",
        "FinanceQueryService"
    ),
    context_services!(219, 220, Sales, "SalesCommandService", "SalesQueryService"),
    context_services!(
        221,
        222,
        PointOfSale,
        "PointOfSaleCommandService",
        "PointOfSaleQueryService"
    ),
    context_services!(
        223,
        224,
        Procurement,
        "ProcurementCommandService",
        "ProcurementQueryService"
    ),
    context_services!(
        225,
        226,
        Inventory,
        "InventoryCommandService",
        "InventoryQueryService"
    ),
    context_services!(
        227,
        228,
        Assets,
        "AssetsCommandService",
        "AssetsQueryService"
    ),
    context_services!(
        229,
        230,
        Maintenance,
        "MaintenanceCommandService",
        "MaintenanceQueryService"
    ),
    context_services!(
        231,
        232,
        Quality,
        "QualityCommandService",
        "QualityQueryService"
    ),
    context_services!(
        233,
        234,
        Operations,
        "OperationsCommandService",
        "OperationsQueryService"
    ),
    context_services!(
        235,
        236,
        PeoplePayroll,
        "PeoplePayrollCommandService",
        "PeoplePayrollQueryService"
    ),
    context_services!(237, 238, Legal, "LegalCommandService", "LegalQueryService"),
    context_services!(
        239,
        240,
        Marketing,
        "MarketingCommandService",
        "MarketingQueryService"
    ),
    context_services!(241, 242, Crm, "CrmCommandService", "CrmQueryService"),
    context_services!(
        243,
        244,
        Manufacturing,
        "ManufacturingCommandService",
        "ManufacturingQueryService"
    ),
    context_services!(
        245,
        246,
        Planning,
        "PlanningCommandService",
        "PlanningQueryService"
    ),
    context_services!(
        247,
        248,
        Projects,
        "ProjectsCommandService",
        "ProjectsQueryService"
    ),
    context_services!(
        249,
        250,
        Ecommerce,
        "EcommerceCommandService",
        "EcommerceQueryService"
    ),
    context_services!(
        251,
        252,
        RecruitingTalent,
        "TalentCommandService",
        "TalentQueryService"
    ),
    context_services!(
        253,
        254,
        Learning,
        "LearningCommandService",
        "LearningQueryService"
    ),
    context_services!(
        255,
        256,
        Benefits,
        "BenefitsCommandService",
        "BenefitsQueryService"
    ),
    context_services!(
        257,
        258,
        FieldService,
        "FieldServiceCommandService",
        "FieldServiceQueryService"
    ),
    context_services!(
        259,
        260,
        Collaboration,
        "CollaborationCommandService",
        "CollaborationQueryService"
    ),
    context_services!(
        261,
        262,
        AgentOrchestration,
        "AgentOrchestrationCommandService",
        "AgentOrchestrationQueryService"
    ),
];

const COLLABORATION_STREAM_SERVICE: ServiceDescriptor = ServiceDescriptor {
    id: ServiceId::catalog(263),
    name: "CollaborationStreamService",
    package: ContextId::Collaboration.proto_package(),
    owner: ServiceOwner::Context(ContextId::Collaboration),
    kind: ServiceKind::Stream,
};
const REFERENCE_QUERY_SERVICE: ServiceDescriptor = ServiceDescriptor {
    id: ServiceId::catalog(264),
    name: "ReferenceQueryService",
    package: "erpx.reference",
    owner: ServiceOwner::FederatedItemDirectory,
    kind: ServiceKind::Query,
};
const DATA_PLANE_STREAM_SERVICE: ServiceDescriptor = ServiceDescriptor {
    id: ServiceId::catalog(265),
    name: "DataPlaneStreamService",
    package: ContextId::PlatformAdministration.proto_package(),
    owner: ServiceOwner::Context(ContextId::PlatformAdministration),
    kind: ServiceKind::Stream,
};

const fn active_services() -> [ServiceDescriptor; 65] {
    let mut services = [COLLABORATION_STREAM_SERVICE; 65];
    let mut pair_index = 0;
    while pair_index < CONTEXT_SERVICE_PAIRS.len() {
        let service_index = pair_index * 2;
        services[service_index] = CONTEXT_SERVICE_PAIRS[pair_index].command;
        services[service_index + 1] = CONTEXT_SERVICE_PAIRS[pair_index].query;
        pair_index += 1;
    }
    services[62] = COLLABORATION_STREAM_SERVICE;
    services[63] = REFERENCE_QUERY_SERVICE;
    services[64] = DATA_PLANE_STREAM_SERVICE;
    services
}

pub const ACTIVE_SERVICES: [ServiceDescriptor; 65] = active_services();

const fn legacy_service_ids() -> [ServiceId; 65] {
    let mut ids = [ServiceId::catalog(101); 65];
    let mut index = 0;
    while index < ids.len() {
        ids[index] = ServiceId::catalog(index as u16 + 101);
        index += 1;
    }
    ids
}

/// Permanently retired legacy service identifiers. They remain cataloged so
/// generators and validators can reject accidental reuse.
pub const RETIRED_SERVICE_IDS: [ServiceId; 65] = legacy_service_ids();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordTypeDescriptor {
    pub id: &'static str,
    pub owner: ContextId,
    pub module: &'static str,
    pub record: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordPublicationSource {
    /// The authoritative context appends the typed fact in the same command
    /// transaction as its aggregate, audit, and outbox rows.
    OwnerOutbox,
    /// A typed owner-owned projection source emits the fact from normalized
    /// authority when a legacy command path has no shared outbox yet.
    OwnerProjectionSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordPublisherMapping {
    pub owner: ContextId,
    pub source: RecordPublicationSource,
    pub event_version: u32,
    /// Owner payload capability. Version 2 carries a separately validated
    /// display label; it never reuses authorized search text as response data.
    pub descriptor_contract_version: u32,
}

/// Publication ownership for the 21 current contexts. This registry maps
/// capability only; it never fabricates a descriptor from catalog metadata.
/// Runtime publishers must supply a validated, domain-owned fact.
pub const CURRENT_CONTEXT_PUBLISHERS: [RecordPublisherMapping; 21] = [
    publisher(
        ContextId::IdentityAccess,
        RecordPublicationSource::OwnerOutbox,
    ),
    publisher(
        ContextId::PlatformAdministration,
        RecordPublicationSource::OwnerOutbox,
    ),
    publisher(
        ContextId::Organization,
        RecordPublicationSource::OwnerOutbox,
    ),
    publisher(ContextId::Commercial, RecordPublicationSource::OwnerOutbox),
    publisher(ContextId::Brand, RecordPublicationSource::OwnerOutbox),
    publisher(
        ContextId::CustomerAuthority,
        RecordPublicationSource::OwnerOutbox,
    ),
    publisher(ContextId::Customer360, RecordPublicationSource::OwnerOutbox),
    publisher(ContextId::Support, RecordPublicationSource::OwnerOutbox),
    publisher(
        ContextId::Finance,
        RecordPublicationSource::OwnerProjectionSource,
    ),
    publisher(ContextId::Sales, RecordPublicationSource::OwnerOutbox),
    publisher(ContextId::PointOfSale, RecordPublicationSource::OwnerOutbox),
    publisher(ContextId::Procurement, RecordPublicationSource::OwnerOutbox),
    publisher(ContextId::Inventory, RecordPublicationSource::OwnerOutbox),
    publisher(ContextId::Assets, RecordPublicationSource::OwnerOutbox),
    publisher(ContextId::Maintenance, RecordPublicationSource::OwnerOutbox),
    publisher(ContextId::Quality, RecordPublicationSource::OwnerOutbox),
    publisher(ContextId::Operations, RecordPublicationSource::OwnerOutbox),
    publisher(
        ContextId::PeoplePayroll,
        RecordPublicationSource::OwnerOutbox,
    ),
    publisher(ContextId::Legal, RecordPublicationSource::OwnerOutbox),
    publisher(
        ContextId::Marketing,
        RecordPublicationSource::OwnerProjectionSource,
    ),
    publisher(ContextId::Crm, RecordPublicationSource::OwnerOutbox),
];

const fn publisher(owner: ContextId, source: RecordPublicationSource) -> RecordPublisherMapping {
    RecordPublisherMapping {
        owner,
        source,
        event_version: crate::reference::ITEM_DIRECTORY_FACT_VERSION,
        descriptor_contract_version: 2,
    }
}

pub fn record_publisher(record_type_id: &str) -> Option<&'static RecordPublisherMapping> {
    let owner = record_type(record_type_id)?.owner;
    CURRENT_CONTEXT_PUBLISHERS
        .iter()
        .find(|mapping| mapping.owner == owner)
}

/// Current-context record families backed by a live owner UoW publisher.
///
/// Every current-context family enters this list only after its owning
/// aggregate appends an `ItemDescriptorDraft` atomically with the authoritative
/// commit and its read model is wired to the trusted projection runtime.
pub const LIVE_CURRENT_RECORD_PUBLISHERS: [&str; 93] = [
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
    "finance.vendor-invoice",
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

/// Ordered Phase 2 activation batches. The first batch is the cross-cutting
/// canary; later batches may be activated only after the Item Directory
/// generation containing labels and collaboration policies is swapped in
/// atomically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordCollaborationRolloutBatch {
    pub name: &'static str,
    pub contexts: &'static [ContextId],
}

pub const CURRENT_RECORD_COLLABORATION_ROLLOUT: [RecordCollaborationRolloutBatch; 4] = [
    RecordCollaborationRolloutBatch {
        name: "canary",
        contexts: &[
            ContextId::IdentityAccess,
            ContextId::Legal,
            ContextId::Procurement,
            ContextId::Finance,
        ],
    },
    RecordCollaborationRolloutBatch {
        name: "authority-and-customer",
        contexts: &[
            ContextId::PlatformAdministration,
            ContextId::Organization,
            ContextId::Commercial,
            ContextId::Brand,
            ContextId::CustomerAuthority,
            ContextId::Customer360,
        ],
    },
    RecordCollaborationRolloutBatch {
        name: "service-and-supply",
        contexts: &[
            ContextId::Support,
            ContextId::Sales,
            ContextId::PointOfSale,
            ContextId::Inventory,
            ContextId::Assets,
            ContextId::Maintenance,
        ],
    },
    RecordCollaborationRolloutBatch {
        name: "operations-and-growth",
        contexts: &[
            ContextId::Quality,
            ContextId::Operations,
            ContextId::PeoplePayroll,
            ContextId::Marketing,
            ContextId::Crm,
        ],
    },
];

/// None of the live aggregate families currently exposes a domain-owned
/// tombstone transition. Their archive, cancel, close, disable, or retire
/// commands remain ordinary lifecycle updates and therefore publish another
/// descriptor rather than an `ItemTombstoned` fact. A family must be added
/// here only with a real owner-domain deletion transition and atomic outbox
/// publisher; absence is the explicit "no tombstone transition" declaration.
pub const LIVE_CURRENT_TOMBSTONE_PUBLISHERS: [&str; 0] = [];

/// Exact catalog families whose current context still lacks a matching live
/// aggregate publisher. Catalog metadata must never be used to fabricate one.
pub fn current_record_family_gaps() -> Vec<&'static str> {
    RECORD_TYPES
        .iter()
        .filter(|record| record.owner.lifecycle() == ContextLifecycle::Current)
        .map(|record| record.id)
        .filter(|id| !LIVE_CURRENT_RECORD_PUBLISHERS.contains(id))
        .collect()
}

impl RecordTypeDescriptor {
    pub fn collection_route(self) -> String {
        format!("/{}/{}", self.module, self.record)
    }

    pub fn detail_route_template(self) -> String {
        format!("/{}/{}/:id", self.module, self.record)
    }
}

macro_rules! records {
    ($($owner:ident, $module:literal => [$($record:literal),+ $(,)?]);+ $(;)?) => {
        &[
            $(
                $(RecordTypeDescriptor {
                    id: concat!($module, ".", $record),
                    owner: ContextId::$owner,
                    module: $module,
                    record: $record,
                },)+
            )+
        ]
    };
}

/// First-class aggregate-root route families in the clean baseline.
pub const RECORD_TYPES: &[RecordTypeDescriptor] = records! {
    IdentityAccess, "identity" => ["principal", "role", "grant", "access-review", "break-glass-session"];
    PlatformAdministration, "platform" => ["holding", "data-plane-operation", "migration", "provider", "release"];
    Organization, "organization" => ["holding", "company", "branch", "sharing-agreement"];
    Commercial, "commercial" => ["subscription", "entitlement", "account", "adjustment"];
    Brand, "brand" => ["profile", "asset", "publication"];
    CustomerAuthority, "customer" => ["account", "authority-grant", "support-access"];
    Customer360, "customer-360" => ["profile", "note", "merge"];
    Support, "support" => ["customer-case", "platform-case", "incident", "access-session"];
    Finance, "finance" => ["journal", "invoice", "vendor-invoice", "credit-note", "debit-note", "payment", "reconciliation", "close-cycle"];
    Sales, "sales" => ["quote", "order", "return", "fulfillment-handoff"];
    PointOfSale, "pos" => ["register-session", "sale", "refund", "cash-movement"];
    Procurement, "procurement" => ["requisition", "rfq", "purchase-order", "receipt", "supplier-return"];
    Inventory, "inventory" => ["document", "transfer", "count", "reservation", "lot", "serial"];
    Assets, "asset" => ["asset", "transfer", "depreciation-run", "disposal"];
    Maintenance, "maintenance" => ["plan", "work-order", "inspection", "request"];
    Quality, "quality" => ["plan", "inspection", "nonconformance", "corrective-action"];
    Operations, "operations" => ["operation", "exception", "case", "approval-work"];
    PeoplePayroll, "people" => ["employee", "employment", "leave", "time-record"];
    PeoplePayroll, "payroll" => ["run", "payslip"];
    Legal, "legal" => ["contract", "case", "matter", "hold", "signature-envelope"];
    Marketing, "marketing" => ["campaign", "content", "audience-activation", "consent-purpose"];
    Crm, "crm" => ["lead", "opportunity", "activity", "forecast"];
    Manufacturing, "manufacturing" => ["specification", "routing", "order", "batch", "result"];
    Planning, "planning" => ["scenario", "demand-plan", "supply-plan", "capacity-plan", "recommendation"];
    Projects, "project" => ["project", "phase", "item", "template", "handoff"];
    Ecommerce, "commerce" => ["catalog-publication", "cart", "order", "fulfillment-request", "return"];
    RecruitingTalent, "talent" => ["requisition", "candidate", "application", "interview", "offer", "review-cycle", "proposal"];
    Learning, "learning" => ["course", "assignment", "enrollment", "completion"];
    Benefits, "benefits" => ["plan", "window", "election", "handoff"];
    FieldService, "field-service" => ["agreement", "request", "dispatch", "visit", "handoff"];
    Collaboration, "collaboration" => ["thread", "moderation-case", "promotion"];
    AgentOrchestration, "agent" => ["definition", "version", "assignment", "delegation-tree", "session", "budget-decision"];
};

pub fn record_type(id: &str) -> Option<&'static RecordTypeDescriptor> {
    RECORD_TYPES.iter().find(|descriptor| descriptor.id == id)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn context_catalog_has_exact_unique_baseline() {
        assert_eq!(ContextId::ALL.len(), 31);
        let slugs = ContextId::ALL
            .iter()
            .map(|context| context.slug())
            .collect::<BTreeSet<_>>();
        let packages = ContextId::ALL
            .iter()
            .map(|context| context.proto_package())
            .collect::<BTreeSet<_>>();
        assert_eq!(slugs.len(), 31);
        assert_eq!(packages.len(), 31);
        assert_eq!(
            ContextId::ALL
                .iter()
                .filter(|context| context.lifecycle() == ContextLifecycle::Current)
                .count(),
            21
        );
    }

    #[test]
    fn workspace_catalog_has_exact_routes_and_channel_policy() {
        assert_eq!(WORKSPACES.len(), 21);
        assert_eq!(
            WORKSPACES
                .iter()
                .map(|workspace| workspace.slug)
                .collect::<BTreeSet<_>>()
                .len(),
            21
        );
        assert!(WORKSPACES.iter().all(|workspace| {
            workspace.route == format!("/{}", workspace.slug)
                && !workspace.route.starts_with("/workspaces")
        }));
        let web_only = WORKSPACES
            .iter()
            .filter(|workspace| !workspace.channels.contains(ChannelId::Desktop))
            .map(|workspace| workspace.id)
            .collect::<Vec<_>>();
        assert_eq!(
            web_only,
            vec![WorkspaceId::Ecommerce, WorkspaceId::RecruitingTalent]
        );
        assert!(
            WORKSPACES
                .iter()
                .filter(|w| !web_only.contains(&w.id))
                .all(|workspace| [
                    ChannelId::Web,
                    ChannelId::Desktop,
                    ChannelId::Ios,
                    ChannelId::Android
                ]
                .into_iter()
                .all(|channel| workspace.channels.contains(channel)))
        );
    }

    #[test]
    fn realm_and_channel_policies_fail_closed_for_external_surfaces() {
        assert_eq!(CHANNELS.len(), 4);
        assert!(
            CHANNELS
                .iter()
                .all(|channel| channel.online_mutations_required && channel.offline_read_only)
        );
        assert_eq!(
            REALMS
                .iter()
                .map(|realm| realm.slug)
                .collect::<BTreeSet<_>>()
                .len(),
            6
        );
        assert!(REALMS.iter().all(|realm| {
            !realm.external_portal || (!realm.internal_ai_launcher && !realm.internal_notes_visible)
        }));
    }

    #[test]
    fn active_service_ids_are_exact_and_retired_ids_are_never_reused() {
        assert_eq!(ACTIVE_SERVICES.len(), 65);
        let active_ids = ACTIVE_SERVICES
            .iter()
            .map(|service| service.id.number())
            .collect::<BTreeSet<_>>();
        let active_names = ACTIVE_SERVICES
            .iter()
            .map(|service| (service.package, service.name))
            .collect::<BTreeSet<_>>();
        let retired_ids = RETIRED_SERVICE_IDS
            .iter()
            .map(|id| id.number())
            .collect::<BTreeSet<_>>();
        assert_eq!(active_ids, (201..=265).collect());
        assert_eq!(active_names.len(), 65);
        assert_eq!(retired_ids, (101..=165).collect());
        assert!(active_ids.is_disjoint(&retired_ids));
        assert_eq!(ACTIVE_SERVICES[0].id.to_string(), "IFC-SVC-201");
        assert_eq!(ACTIVE_SERVICES[64].id.to_string(), "IFC-SVC-265");
    }

    #[test]
    fn every_context_has_one_command_and_one_query_service() {
        for context in ContextId::ALL {
            assert_eq!(
                ACTIVE_SERVICES
                    .iter()
                    .filter(|service| {
                        service.owner == ServiceOwner::Context(context)
                            && service.kind == ServiceKind::Command
                    })
                    .count(),
                1,
                "missing command service for {context}"
            );
            assert_eq!(
                ACTIVE_SERVICES
                    .iter()
                    .filter(|service| {
                        service.owner == ServiceOwner::Context(context)
                            && service.kind == ServiceKind::Query
                    })
                    .count(),
                1,
                "missing query service for {context}"
            );
        }
    }

    #[test]
    fn record_catalog_is_unique_and_routes_are_singular() {
        let ids = RECORD_TYPES
            .iter()
            .map(|record| record.id)
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), RECORD_TYPES.len());
        assert!(RECORD_TYPES.iter().all(|record| {
            !record.module.is_empty()
                && !record.record.is_empty()
                && record.detail_route_template()
                    == format!("/{}/{}/:id", record.module, record.record)
                && record_type(record.id) == Some(record)
        }));
        assert_eq!(
            record_type("finance.invoice")
                .expect("finance invoice")
                .detail_route_template(),
            "/finance/invoice/:id"
        );
    }

    #[test]
    fn every_current_record_family_has_one_owner_publisher_mapping() {
        assert_eq!(CURRENT_CONTEXT_PUBLISHERS.len(), 21);
        assert_eq!(
            CURRENT_CONTEXT_PUBLISHERS
                .iter()
                .map(|mapping| mapping.owner)
                .collect::<BTreeSet<_>>()
                .len(),
            21
        );
        let current_records = RECORD_TYPES
            .iter()
            .filter(|record| record.owner.lifecycle() == ContextLifecycle::Current)
            .collect::<Vec<_>>();
        assert_eq!(current_records.len(), 93);
        for record in current_records {
            let mapping = record_publisher(record.id)
                .unwrap_or_else(|| panic!("missing publisher for {}", record.id));
            assert_eq!(mapping.owner, record.owner);
            assert_eq!(
                mapping.event_version,
                crate::reference::ITEM_DIRECTORY_FACT_VERSION
            );
            assert_eq!(mapping.descriptor_contract_version, 2);
        }
        assert!(record_publisher("project.item").is_none());
    }

    #[test]
    fn live_current_publishers_are_exact_and_gaps_are_explicit() {
        assert_eq!(LIVE_CURRENT_RECORD_PUBLISHERS.len(), 93);
        assert_eq!(
            LIVE_CURRENT_RECORD_PUBLISHERS
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len(),
            LIVE_CURRENT_RECORD_PUBLISHERS.len()
        );
        assert!(LIVE_CURRENT_RECORD_PUBLISHERS.iter().all(|id| {
            record_type(id).is_some_and(|record| {
                record.owner.lifecycle() == ContextLifecycle::Current
                    && record_publisher(id).is_some()
            })
        }));
        assert_eq!(
            current_record_family_gaps().len(),
            93 - LIVE_CURRENT_RECORD_PUBLISHERS.len()
        );
        assert!(current_record_family_gaps().is_empty());
        assert!(
            current_record_family_gaps()
                .iter()
                .all(|id| !LIVE_CURRENT_RECORD_PUBLISHERS.contains(id))
        );
        assert!(LIVE_CURRENT_TOMBSTONE_PUBLISHERS.is_empty());
    }

    #[test]
    fn collaboration_rollout_covers_each_current_context_once_with_the_canary_first() {
        let canary = CURRENT_RECORD_COLLABORATION_ROLLOUT[0];
        assert_eq!(canary.name, "canary");
        assert_eq!(
            canary.contexts,
            &[
                ContextId::IdentityAccess,
                ContextId::Legal,
                ContextId::Procurement,
                ContextId::Finance,
            ]
        );
        let activated = CURRENT_RECORD_COLLABORATION_ROLLOUT
            .iter()
            .flat_map(|batch| batch.contexts.iter().copied())
            .collect::<Vec<_>>();
        assert_eq!(activated.len(), 21);
        assert_eq!(activated.iter().copied().collect::<BTreeSet<_>>().len(), 21);
        assert!(
            activated
                .iter()
                .all(|context| context.lifecycle() == ContextLifecycle::Current)
        );
        assert_eq!(
            activated.iter().copied().collect::<BTreeSet<_>>(),
            LIVE_CURRENT_RECORD_PUBLISHERS
                .iter()
                .map(|record_type_id| {
                    record_type(record_type_id)
                        .expect("live publisher record")
                        .owner
                })
                .collect::<BTreeSet<_>>()
        );
    }

    #[test]
    fn every_current_owner_source_publishes_v2_labels_while_future_sources_remain_legacy() {
        let owner_sources = vec![
            (
                ContextId::IdentityAccess,
                vec![
                    include_str!("../../adapters/identity-access-sqlx/src/command.rs"),
                    include_str!("../../adapters/identity-access-sqlx/src/governance.rs"),
                ],
            ),
            (
                ContextId::PlatformAdministration,
                vec![
                    include_str!("../../adapters/platform-administration-sqlx/src/command.rs"),
                    include_str!("../../adapters/platform-administration-sqlx/src/governance.rs"),
                ],
            ),
            (
                ContextId::Organization,
                vec![
                    include_str!("../../adapters/organization-sqlx/src/command.rs"),
                    include_str!("../../adapters/organization-sqlx/src/sharing.rs"),
                ],
            ),
            (
                ContextId::Commercial,
                vec![
                    include_str!("../../adapters/commercial-sqlx/src/command.rs"),
                    include_str!("../../adapters/commercial-sqlx/src/records.rs"),
                ],
            ),
            (
                ContextId::Brand,
                vec![include_str!("../../adapters/brand-sqlx/src/command.rs")],
            ),
            (
                ContextId::CustomerAuthority,
                vec![
                    include_str!("../../adapters/customer-authority-sqlx/src/command.rs"),
                    include_str!("../../adapters/customer-authority-sqlx/src/authority_records.rs"),
                ],
            ),
            (
                ContextId::Customer360,
                vec![include_str!(
                    "../../adapters/customer-360-sqlx/src/owned_records.rs"
                )],
            ),
            (
                ContextId::Support,
                vec![
                    include_str!("../../adapters/support-sqlx/src/command.rs"),
                    include_str!("../../adapters/support-sqlx/src/platform_case.rs"),
                ],
            ),
            (
                ContextId::Finance,
                vec![
                    include_str!("../../adapters/finance-sqlx/src/command.rs"),
                    include_str!("../../adapters/finance-sqlx/src/records.rs"),
                ],
            ),
            (
                ContextId::Sales,
                vec![
                    include_str!("../../adapters/sales-sqlx/src/command.rs"),
                    include_str!("../../adapters/sales-sqlx/src/records.rs"),
                ],
            ),
            (
                ContextId::PointOfSale,
                vec![
                    include_str!("../../adapters/pos-sqlx/src/command.rs"),
                    include_str!("../../adapters/pos-sqlx/src/records.rs"),
                ],
            ),
            (
                ContextId::Procurement,
                vec![
                    include_str!("../../adapters/procurement-sqlx/src/command.rs"),
                    include_str!("../../adapters/procurement-sqlx/src/records.rs"),
                ],
            ),
            (
                ContextId::Inventory,
                vec![
                    include_str!("../../adapters/inventory-sqlx/src/command.rs"),
                    include_str!("../../adapters/inventory-sqlx/src/records.rs"),
                ],
            ),
            (
                ContextId::Assets,
                vec![
                    include_str!("../../adapters/assets-sqlx/src/command.rs"),
                    include_str!("../../adapters/assets-sqlx/src/records.rs"),
                ],
            ),
            (
                ContextId::Maintenance,
                vec![
                    include_str!("../../adapters/maintenance-sqlx/src/command.rs"),
                    include_str!("../../adapters/maintenance-sqlx/src/records.rs"),
                ],
            ),
            (
                ContextId::Quality,
                vec![
                    include_str!("../../adapters/quality-sqlx/src/command.rs"),
                    include_str!("../../adapters/quality-sqlx/src/records.rs"),
                ],
            ),
            (
                ContextId::Operations,
                vec![
                    include_str!("../../adapters/operations-sqlx/src/command.rs"),
                    include_str!("../../adapters/operations-sqlx/src/records.rs"),
                ],
            ),
            (
                ContextId::PeoplePayroll,
                vec![
                    include_str!("../../adapters/people-payroll-sqlx/src/command.rs"),
                    include_str!("../../adapters/people-payroll-sqlx/src/time_record.rs"),
                ],
            ),
            (
                ContextId::Legal,
                vec![
                    include_str!("../../adapters/legal-sqlx/src/command.rs"),
                    include_str!("../../adapters/legal-sqlx/src/legal_case.rs"),
                ],
            ),
            (
                ContextId::Marketing,
                vec![include_str!("../../adapters/marketing-sqlx/src/records.rs")],
            ),
            (
                ContextId::Crm,
                vec![
                    include_str!("../../adapters/crm-sqlx/src/command.rs"),
                    include_str!("../../adapters/crm-sqlx/src/records.rs"),
                ],
            ),
        ];
        assert_eq!(owner_sources.len(), 21);
        assert_eq!(
            owner_sources
                .iter()
                .map(|(owner, _)| *owner)
                .collect::<BTreeSet<_>>(),
            CURRENT_CONTEXT_PUBLISHERS
                .iter()
                .map(|publisher| publisher.owner)
                .collect::<BTreeSet<_>>()
        );
        for (owner, sources) in owner_sources {
            assert!(
                sources
                    .iter()
                    .all(|source| source.contains("ItemDescriptorDraftV2")),
                "{owner} retains a legacy current publisher"
            );
        }
        assert!(LIVE_CURRENT_RECORD_PUBLISHERS.iter().all(|record_type_id| {
            record_publisher(record_type_id)
                .is_some_and(|publisher| publisher.descriptor_contract_version == 2)
        }));

        let future_source = include_str!("../../adapters/planning-sqlx/src/directory.rs");
        assert!(future_source.contains("ItemDescriptorDraft"));
        assert!(!future_source.contains("ItemDescriptorDraftV2"));
        assert!(record_publisher("planning.scenario").is_none());
    }
}
