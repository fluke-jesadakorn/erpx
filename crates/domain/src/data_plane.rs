//! Three-layer configuration, customer-hosted data-plane, and Root-access rules.
//!
//! The control plane may propose configuration for every layer, but a customer-hosted
//! agent applies tenant configuration and admits interactive ERPX Root sessions only
//! after customer approval. Secret values and generic database/shell access are not
//! representable by these contracts.

use std::{collections::BTreeSet, fmt};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::access::Scope;

pub const PLATFORM_INFRASTRUCTURE_READ: &str = "platform.infrastructure.read";
pub const PLATFORM_INFRASTRUCTURE_MANAGE: &str = "platform.infrastructure.manage";
pub const TENANT_INFRASTRUCTURE_READ: &str = "tenant.infrastructure.read";
pub const TENANT_INFRASTRUCTURE_PROPOSE: &str = "tenant.infrastructure.propose";
pub const TENANT_INFRASTRUCTURE_APPROVE: &str = "tenant.infrastructure.approve";
pub const TENANT_INFRASTRUCTURE_APPLY: &str = "tenant.infrastructure.apply";
pub const TENANT_AGENT_ENROLL: &str = "tenant.agent.enroll";
pub const TENANT_AGENT_ROTATE: &str = "tenant.agent.rotate";
pub const TENANT_ROOT_ACCESS_READ: &str = "tenant.root-access.read";
pub const TENANT_ROOT_ACCESS_REQUEST: &str = "tenant.root-access.request";
pub const TENANT_ROOT_ACCESS_APPROVE: &str = "tenant.root-access.approve";
pub const TENANT_ROOT_ACCESS_REVOKE: &str = "tenant.root-access.revoke";
pub const BUSINESS_UNIT_CONFIGURATION_READ: &str = "business-unit.configuration.read";
pub const BUSINESS_UNIT_CONFIGURATION_MANAGE: &str = "business-unit.configuration.manage";

const MAX_REFERENCE_BYTES: usize = 255;
const MAX_REASON_BYTES: usize = 512;
const MAX_PERMISSION_BYTES: usize = 128;
const MAX_ROOT_ACTIONS: usize = 64;

/// The one canonical business-unit catalog shared by routing, entitlements, roles,
/// configuration, telemetry, and audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BusinessUnitId {
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
}

/// Stable route and authorization metadata for one ERPX business unit.
///
/// This catalog is the single source of truth shared by server navigation,
/// client route resolution, entitlements, localization, and contract tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusinessUnitRouteDescriptor {
    pub unit: BusinessUnitId,
    pub route_id: &'static str,
    pub canonical_path: &'static str,
    pub read_permission: &'static str,
    pub entitlement_key: &'static str,
    pub label_key: &'static str,
    pub label_en: &'static str,
    pub label_th: &'static str,
    pub icon: &'static str,
}

pub const BUSINESS_UNIT_ROUTE_CATALOG: [BusinessUnitRouteDescriptor; 12] = [
    business_unit_route(BusinessUnitId::Operations, "operations", "ปฏิบัติการ"),
    business_unit_route(BusinessUnitId::Finance, "finance", "การเงิน"),
    business_unit_route(BusinessUnitId::Sales, "sales", "การขาย"),
    business_unit_route(BusinessUnitId::Procurement, "procurement", "จัดซื้อ"),
    business_unit_route(BusinessUnitId::Inventory, "inventory", "สินค้าคงคลัง"),
    business_unit_route(BusinessUnitId::Assets, "assets", "สินทรัพย์"),
    business_unit_route(BusinessUnitId::Maintenance, "maintenance", "บำรุงรักษา"),
    business_unit_route(BusinessUnitId::Quality, "quality", "คุณภาพ"),
    business_unit_route(
        BusinessUnitId::MarketingCrm,
        "marketing-crm",
        "การตลาดและ CRM",
    ),
    business_unit_route(
        BusinessUnitId::HrPayroll,
        "hr-payroll",
        "ทรัพยากรบุคคลและเงินเดือน",
    ),
    business_unit_route(BusinessUnitId::PointOfSale, "point-of-sale", "จุดขาย"),
    business_unit_route(BusinessUnitId::Legal, "legal", "กฎหมาย"),
];

const fn business_unit_route(
    unit: BusinessUnitId,
    entitlement_key: &'static str,
    label_th: &'static str,
) -> BusinessUnitRouteDescriptor {
    BusinessUnitRouteDescriptor {
        unit,
        route_id: entitlement_key,
        canonical_path: match unit {
            BusinessUnitId::Operations => "/workspaces/operations",
            BusinessUnitId::Finance => "/workspaces/finance",
            BusinessUnitId::Sales => "/workspaces/sales",
            BusinessUnitId::Procurement => "/workspaces/procurement",
            BusinessUnitId::Inventory => "/workspaces/inventory",
            BusinessUnitId::Assets => "/workspaces/assets",
            BusinessUnitId::Maintenance => "/workspaces/maintenance",
            BusinessUnitId::Quality => "/workspaces/quality",
            BusinessUnitId::MarketingCrm => "/workspaces/marketing-crm",
            BusinessUnitId::HrPayroll => "/workspaces/hr-payroll",
            BusinessUnitId::PointOfSale => "/workspaces/point-of-sale",
            BusinessUnitId::Legal => "/workspaces/legal",
        },
        read_permission: match unit {
            BusinessUnitId::Operations => "operations.read",
            BusinessUnitId::Finance => "finance.read",
            BusinessUnitId::Sales => "sales.read",
            BusinessUnitId::Procurement => "procurement.read",
            BusinessUnitId::Inventory => "inventory.read",
            BusinessUnitId::Assets => "assets.read",
            BusinessUnitId::Maintenance => "maintenance.read",
            BusinessUnitId::Quality => "quality.read",
            BusinessUnitId::MarketingCrm => "marketing-crm.read",
            BusinessUnitId::HrPayroll => "hr-payroll.read",
            BusinessUnitId::PointOfSale => "point-of-sale.read",
            BusinessUnitId::Legal => "legal.read",
        },
        entitlement_key,
        label_key: match unit {
            BusinessUnitId::Operations => "nav.business_unit.operations",
            BusinessUnitId::Finance => "nav.business_unit.finance",
            BusinessUnitId::Sales => "nav.business_unit.sales",
            BusinessUnitId::Procurement => "nav.business_unit.procurement",
            BusinessUnitId::Inventory => "nav.business_unit.inventory",
            BusinessUnitId::Assets => "nav.business_unit.assets",
            BusinessUnitId::Maintenance => "nav.business_unit.maintenance",
            BusinessUnitId::Quality => "nav.business_unit.quality",
            BusinessUnitId::MarketingCrm => "nav.business_unit.marketing_crm",
            BusinessUnitId::HrPayroll => "nav.business_unit.hr_payroll",
            BusinessUnitId::PointOfSale => "nav.business_unit.point_of_sale",
            BusinessUnitId::Legal => "nav.business_unit.legal",
        },
        label_en: unit.display_name(),
        label_th,
        icon: "module",
    }
}

impl BusinessUnitId {
    pub const ALL: [Self; 12] = [
        Self::Operations,
        Self::Finance,
        Self::Sales,
        Self::Procurement,
        Self::Inventory,
        Self::Assets,
        Self::Maintenance,
        Self::Quality,
        Self::MarketingCrm,
        Self::HrPayroll,
        Self::PointOfSale,
        Self::Legal,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Operations => "operations",
            Self::Finance => "finance",
            Self::Sales => "sales",
            Self::Procurement => "procurement",
            Self::Inventory => "inventory",
            Self::Assets => "assets",
            Self::Maintenance => "maintenance",
            Self::Quality => "quality",
            Self::MarketingCrm => "marketing-crm",
            Self::HrPayroll => "hr-payroll",
            Self::PointOfSale => "point-of-sale",
            Self::Legal => "legal",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Operations => "Operations",
            Self::Finance => "Finance",
            Self::Sales => "Sales",
            Self::Procurement => "Procurement",
            Self::Inventory => "Inventory",
            Self::Assets => "Assets",
            Self::Maintenance => "Maintenance",
            Self::Quality => "Quality",
            Self::MarketingCrm => "Marketing & CRM",
            Self::HrPayroll => "HR & Payroll",
            Self::PointOfSale => "Point of Sale",
            Self::Legal => "Legal",
        }
    }

    pub const fn route(self) -> &'static BusinessUnitRouteDescriptor {
        &BUSINESS_UNIT_ROUTE_CATALOG[match self {
            Self::Operations => 0,
            Self::Finance => 1,
            Self::Sales => 2,
            Self::Procurement => 3,
            Self::Inventory => 4,
            Self::Assets => 5,
            Self::Maintenance => 6,
            Self::Quality => 7,
            Self::MarketingCrm => 8,
            Self::HrPayroll => 9,
            Self::PointOfSale => 10,
            Self::Legal => 11,
        }]
    }

    pub fn parse(value: &str) -> Result<Self, DataPlaneContractError> {
        Self::ALL
            .into_iter()
            .find(|unit| unit.as_str() == value)
            .ok_or(DataPlaneContractError::UnsupportedBusinessUnit)
    }

    pub fn permission(self, action: &str) -> Result<String, DataPlaneContractError> {
        validate_permission_segment(action)?;
        Ok(format!("{}.{action}", self.as_str()))
    }

    pub fn role_key(self, level: BusinessUnitRoleLevel) -> String {
        format!("{}_{}", self.as_str(), level.as_str())
    }

    pub fn role_permissions(
        self,
        level: BusinessUnitRoleLevel,
    ) -> Result<Vec<String>, DataPlaneContractError> {
        let read = self.permission("read")?;
        Ok(match level {
            BusinessUnitRoleLevel::Viewer => vec![read],
            BusinessUnitRoleLevel::Operator => vec![read, self.permission("write")?],
            BusinessUnitRoleLevel::Approver => vec![read, self.permission("approve")?],
            BusinessUnitRoleLevel::Administrator => {
                vec![read, self.permission("write")?, self.permission("admin")?]
            }
            BusinessUnitRoleLevel::Auditor => vec![read, "audit.read".into()],
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BusinessUnitRoleLevel {
    Viewer,
    Operator,
    Approver,
    Administrator,
    Auditor,
}

impl BusinessUnitRoleLevel {
    pub const ALL: [Self; 5] = [
        Self::Viewer,
        Self::Operator,
        Self::Approver,
        Self::Administrator,
        Self::Auditor,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Operator => "operator",
            Self::Approver => "approver",
            Self::Administrator => "admin",
            Self::Auditor => "auditor",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusinessUnitRoleTemplate {
    pub business_unit: BusinessUnitId,
    pub level: BusinessUnitRoleLevel,
    pub role_key: String,
    pub permissions: Vec<String>,
}

impl BusinessUnitRoleTemplate {
    pub fn new(
        business_unit: BusinessUnitId,
        level: BusinessUnitRoleLevel,
    ) -> Result<Self, DataPlaneContractError> {
        Ok(Self {
            business_unit,
            level,
            role_key: business_unit.role_key(level),
            permissions: business_unit.role_permissions(level)?,
        })
    }

    pub fn parse(role_key: &str) -> Result<Self, DataPlaneContractError> {
        for business_unit in BusinessUnitId::ALL {
            for level in BusinessUnitRoleLevel::ALL {
                if business_unit.role_key(level) == role_key {
                    return Self::new(business_unit, level);
                }
            }
        }
        Err(DataPlaneContractError::UnsupportedRoleTemplate)
    }
}

pub const PLATFORM_DIRECTORY_ROLE_TEMPLATES: [&str; 8] = [
    "platform_customer_operations",
    "platform_directory_administrator",
    "platform_support_agent",
    "platform_billing_administrator",
    "platform_operations_administrator",
    "platform_auditor",
    "platform_role_reviewer",
    "platform_tenant_administrator",
];

pub fn directory_role_template_is_allowed(layer: ConfigurationLayer, role_key: &str) -> bool {
    match layer {
        ConfigurationLayer::Platform => PLATFORM_DIRECTORY_ROLE_TEMPLATES.contains(&role_key),
        ConfigurationLayer::Tenant => {
            matches!(role_key, "holding_owner" | "customer_security_head")
        }
        ConfigurationLayer::BusinessUnit(unit) => BusinessUnitRoleTemplate::parse(role_key)
            .is_ok_and(|template| template.business_unit == unit),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusinessUnitDirectoryRoleMapping {
    pub group_ref: String,
    pub role_key: String,
    pub target_scope: Scope,
    pub requires_mfa: bool,
    pub assignment_duration_hours: u32,
    pub approval_limit_minor: u64,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BusinessUnitConfiguration {
    pub tenant_id: String,
    pub holding_id: Uuid,
    pub business_unit: BusinessUnitId,
    pub configuration_version: u64,
    pub active: bool,
    pub organization_scope: Scope,
    pub directory_role_mappings: Vec<BusinessUnitDirectoryRoleMapping>,
    pub integration_references: Vec<String>,
    pub retention_policy: serde_json::Value,
    pub projection_policy: serde_json::Value,
    pub workflow_policy: serde_json::Value,
    pub updated_by: String,
    pub updated_at: DateTime<Utc>,
}

impl BusinessUnitConfiguration {
    pub fn validate(&self) -> Result<(), DataPlaneContractError> {
        validate_reference(&self.tenant_id, "tenant_id")?;
        validate_reference(&self.updated_by, "updated_by")?;
        if self.holding_id.is_nil() || self.configuration_version == 0 {
            return Err(DataPlaneContractError::InvalidBusinessUnitConfiguration);
        }
        let holding_scope = Scope::Holding {
            holding_id: self.holding_id,
        };
        if !holding_scope.contains(&self.organization_scope)
            || matches!(self.organization_scope, Scope::Platform)
            || !self.retention_policy.is_object()
            || !self.projection_policy.is_object()
            || !self.workflow_policy.is_object()
        {
            return Err(DataPlaneContractError::InvalidBusinessUnitConfiguration);
        }
        let mut integration_refs = BTreeSet::new();
        for reference in &self.integration_references {
            validate_reference(reference, "integration_reference")?;
            if !integration_refs.insert(reference) {
                return Err(DataPlaneContractError::DuplicateConfigurationValue);
            }
        }
        let mut mapping_keys = BTreeSet::new();
        for mapping in &self.directory_role_mappings {
            validate_reference(&mapping.group_ref, "directory_group_ref")?;
            validate_reference(&mapping.role_key, "role_key")?;
            if mapping.assignment_duration_hours == 0
                || mapping.assignment_duration_hours > 2_160
                || !self.organization_scope.contains(&mapping.target_scope)
                || !directory_role_template_is_allowed(
                    ConfigurationLayer::BusinessUnit(self.business_unit),
                    &mapping.role_key,
                )
            {
                return Err(DataPlaneContractError::InvalidBusinessUnitConfiguration);
            }
            let key = format!(
                "{}|{}|{}",
                mapping.group_ref,
                mapping.role_key,
                mapping.target_scope.canonical_key()
            );
            if !mapping_keys.insert(key) {
                return Err(DataPlaneContractError::DuplicateConfigurationValue);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenantDeploymentMode {
    ErpxManaged,
    CustomerDataPlane,
}

impl TenantDeploymentMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ErpxManaged => "erpx_managed",
            Self::CustomerDataPlane => "customer_data_plane",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigurationLayer {
    Platform,
    Tenant,
    BusinessUnit(BusinessUnitId),
}

impl ConfigurationLayer {
    pub fn canonical_key(self) -> String {
        match self {
            Self::Platform => "platform".into(),
            Self::Tenant => "tenant".into(),
            Self::BusinessUnit(unit) => format!("business-unit:{}", unit.as_str()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigurationLifecycle {
    Draft,
    AwaitingApproval,
    Approved,
    Applying,
    Verified,
    Active,
    Rejected,
    Drifted,
    Failed,
    Suspended,
}

impl ConfigurationLifecycle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Approved => "approved",
            Self::Applying => "applying",
            Self::Verified => "verified",
            Self::Active => "active",
            Self::Rejected => "rejected",
            Self::Drifted => "drifted",
            Self::Failed => "failed",
            Self::Suspended => "suspended",
        }
    }

    pub const fn can_transition_to(self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Draft, Self::AwaitingApproval)
                | (Self::AwaitingApproval, Self::Approved | Self::Rejected)
                | (Self::Approved, Self::Applying)
                | (Self::Applying, Self::Verified | Self::Failed)
                | (Self::Verified, Self::Active | Self::Failed)
                | (Self::Active, Self::Drifted | Self::Suspended)
                | (Self::Drifted, Self::AwaitingApproval | Self::Suspended)
                | (Self::Failed, Self::AwaitingApproval | Self::Suspended)
                | (Self::Suspended, Self::AwaitingApproval)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataPlaneAgentState {
    Unenrolled,
    Enrolling,
    Connected,
    Healthy,
    Drifted,
    Degraded,
    Offline,
    Suspended,
    UpgradeRequired,
}

impl DataPlaneAgentState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unenrolled => "unenrolled",
            Self::Enrolling => "enrolling",
            Self::Connected => "connected",
            Self::Healthy => "healthy",
            Self::Drifted => "drifted",
            Self::Degraded => "degraded",
            Self::Offline => "offline",
            Self::Suspended => "suspended",
            Self::UpgradeRequired => "upgrade_required",
        }
    }

    pub const fn permits_root_session(self) -> bool {
        matches!(self, Self::Connected | Self::Healthy)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCapability {
    TypedTenantQueries,
    TypedTenantCommands,
    DirectorySync,
    ProjectionWorker,
    Backup,
    Restore,
    Migration,
    ConfigurationReconciliation,
    HealthTelemetry,
}

impl AgentCapability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TypedTenantQueries => "typed_tenant_queries",
            Self::TypedTenantCommands => "typed_tenant_commands",
            Self::DirectorySync => "directory_sync",
            Self::ProjectionWorker => "projection_worker",
            Self::Backup => "backup",
            Self::Restore => "restore",
            Self::Migration => "migration",
            Self::ConfigurationReconciliation => "configuration_reconciliation",
            Self::HealthTelemetry => "health_telemetry",
        }
    }
}

/// A secret-manager locator. Its value is intentionally hidden from `Debug`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretReference(String);

impl SecretReference {
    pub fn new(value: impl Into<String>) -> Result<Self, DataPlaneContractError> {
        let value = value.into();
        validate_reference(&value, "secret reference")?;
        let accepted = [
            "customer-secret:",
            "erpx-managed:",
            "vault:",
            "aws-secrets-manager:",
            "gcp-secret-manager:",
            "azure-key-vault:",
        ]
        .iter()
        .any(|prefix| value.starts_with(prefix));
        if !accepted || value.contains("://") || value.contains('@') || value.contains('=') {
            return Err(DataPlaneContractError::InvalidSecretReference);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretReference(<redacted>)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataPlaneAgentBinding {
    pub tenant_id: String,
    pub holding_id: Uuid,
    pub agent_id: Uuid,
    pub certificate_fingerprint: String,
    pub capabilities: Vec<AgentCapability>,
    pub state: DataPlaneAgentState,
    pub observed_version: u64,
}

impl DataPlaneAgentBinding {
    pub fn validate(&self) -> Result<(), DataPlaneContractError> {
        validate_reference(&self.tenant_id, "tenant_id")?;
        if self.holding_id.is_nil() || self.agent_id.is_nil() {
            return Err(DataPlaneContractError::NilIdentifier);
        }
        if self.certificate_fingerprint.len() != 64
            || !self
                .certificate_fingerprint
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            return Err(DataPlaneContractError::InvalidCertificateFingerprint);
        }
        if self.capabilities.is_empty()
            || self.capabilities.iter().collect::<BTreeSet<_>>().len() != self.capabilities.len()
        {
            return Err(DataPlaneContractError::InvalidAgentCapabilities);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantInfrastructureProfile {
    pub tenant_id: String,
    pub holding_id: Uuid,
    pub deployment_mode: TenantDeploymentMode,
    pub write_database_ref: SecretReference,
    pub read_database_ref: SecretReference,
    pub storage_ref: SecretReference,
    pub directory_credential_ref: Option<SecretReference>,
    pub backup_profile_ref: SecretReference,
    pub version: u64,
}

impl TenantInfrastructureProfile {
    pub fn validate(&self) -> Result<(), DataPlaneContractError> {
        validate_reference(&self.tenant_id, "tenant_id")?;
        if self.holding_id.is_nil() || self.version == 0 {
            return Err(DataPlaneContractError::NilIdentifier);
        }
        if self.write_database_ref == self.read_database_ref {
            return Err(DataPlaneContractError::WriteReadDatabaseCollision);
        }
        let expected_prefix = match self.deployment_mode {
            TenantDeploymentMode::ErpxManaged => "erpx-managed:",
            TenantDeploymentMode::CustomerDataPlane => "customer-secret:",
        };
        for reference in [
            &self.write_database_ref,
            &self.read_database_ref,
            &self.storage_ref,
            &self.backup_profile_ref,
        ] {
            if !reference.as_str().starts_with(expected_prefix) {
                return Err(DataPlaneContractError::InfrastructureOwnerMismatch);
            }
        }
        if let Some(reference) = &self.directory_credential_ref
            && !reference.as_str().starts_with(expected_prefix)
        {
            return Err(DataPlaneContractError::InfrastructureOwnerMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootAccessMode {
    HealthDiagnostic,
    InfrastructureMaintenance,
    BusinessDiagnostic,
    BusinessMutation,
    EmergencyRecovery,
}

impl RootAccessMode {
    pub const fn maximum_duration(self) -> Duration {
        match self {
            Self::HealthDiagnostic | Self::BusinessDiagnostic => Duration::hours(8),
            Self::InfrastructureMaintenance | Self::BusinessMutation => Duration::hours(2),
            Self::EmergencyRecovery => Duration::hours(1),
        }
    }

    pub const fn requires_business_units(self) -> bool {
        matches!(self, Self::BusinessDiagnostic | Self::BusinessMutation)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootAccessRequest {
    pub id: Uuid,
    pub requester: String,
    pub device_ref: String,
    pub tenant_id: String,
    pub agent_id: Uuid,
    pub tenant_scope: Scope,
    pub mode: RootAccessMode,
    pub allowed_actions: Vec<String>,
    pub business_units: Vec<BusinessUnitId>,
    pub reason: String,
    pub ticket_reference: String,
    pub requested_at: DateTime<Utc>,
    pub requested_until: DateTime<Utc>,
}

impl RootAccessRequest {
    pub fn validate(&self) -> Result<(), DataPlaneContractError> {
        if self.id.is_nil() || self.agent_id.is_nil() {
            return Err(DataPlaneContractError::NilIdentifier);
        }
        validate_reference(&self.requester, "requester")?;
        validate_reference(&self.device_ref, "device_ref")?;
        validate_reference(&self.tenant_id, "tenant_id")?;
        validate_reason(&self.reason)?;
        validate_reference(&self.ticket_reference, "ticket_reference")?;
        if matches!(self.tenant_scope, Scope::Platform) {
            return Err(DataPlaneContractError::RootAccessMustBeTenantScoped);
        }
        let duration = self.requested_until - self.requested_at;
        if duration <= Duration::zero() || duration > self.mode.maximum_duration() {
            return Err(DataPlaneContractError::InvalidRootAccessWindow);
        }
        if self.allowed_actions.is_empty() || self.allowed_actions.len() > MAX_ROOT_ACTIONS {
            return Err(DataPlaneContractError::InvalidAllowedActions);
        }
        let mut actions = BTreeSet::new();
        for action in &self.allowed_actions {
            validate_permission(action)?;
            if !actions.insert(action) {
                return Err(DataPlaneContractError::InvalidAllowedActions);
            }
        }
        let units: BTreeSet<_> = self.business_units.iter().collect();
        if units.len() != self.business_units.len()
            || (self.mode.requires_business_units() && units.is_empty())
        {
            return Err(DataPlaneContractError::InvalidBusinessUnitSelection);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootAccessApproval {
    pub request_id: Uuid,
    pub approver: String,
    pub approved_at: DateTime<Utc>,
    pub fresh_mfa: bool,
}

/// Separation-of-duty policy applied by the persistence boundary. The local
/// exception is deliberately explicit so production callers cannot acquire it
/// through an omitted or permissive default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPolicy {
    #[default]
    Strict,
    LocalDevelopmentException,
}

impl RootAccessApproval {
    pub fn validate_for(&self, request: &RootAccessRequest) -> Result<(), DataPlaneContractError> {
        self.validate_for_with_policy(request, ApprovalPolicy::Strict)
    }

    pub fn validate_for_with_policy(
        &self,
        request: &RootAccessRequest,
        policy: ApprovalPolicy,
    ) -> Result<(), DataPlaneContractError> {
        request.validate()?;
        if self.request_id != request.id {
            return Err(DataPlaneContractError::ApprovalRequestMismatch);
        }
        validate_reference(&self.approver, "approver")?;
        if self.approver == request.requester && matches!(policy, ApprovalPolicy::Strict) {
            return Err(DataPlaneContractError::ApproverMustDifferFromRequester);
        }
        if !self.fresh_mfa {
            return Err(DataPlaneContractError::FreshMfaRequired);
        }
        if self.approved_at < request.requested_at || self.approved_at >= request.requested_until {
            return Err(DataPlaneContractError::ApprovalOutsideRequestWindow);
        }
        Ok(())
    }
}

/// Hash-only authorization delivered to an agent after customer approval.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootAccessCapability {
    pub id: Uuid,
    pub request_id: Uuid,
    pub token_hash: [u8; 32],
    pub root_principal: String,
    pub device_ref: String,
    pub tenant_id: String,
    pub agent_id: Uuid,
    pub scope: Scope,
    pub allowed_actions: Vec<String>,
    pub business_units: Vec<BusinessUnitId>,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl fmt::Debug for RootAccessCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RootAccessCapability")
            .field("id", &self.id)
            .field("request_id", &self.request_id)
            .field("token_hash", &"<redacted>")
            .field("root_principal", &self.root_principal)
            .field("device_ref", &"<opaque>")
            .field("tenant_id", &"<opaque>")
            .field("agent_id", &self.agent_id)
            .field("scope", &self.scope)
            .field("allowed_actions", &self.allowed_actions)
            .field("business_units", &self.business_units)
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .field("revoked_at", &self.revoked_at)
            .finish()
    }
}

impl RootAccessCapability {
    #[allow(clippy::too_many_arguments)]
    pub fn authorize(
        &self,
        at: DateTime<Utc>,
        root_principal: &str,
        device_ref: &str,
        tenant_id: &str,
        agent_id: Uuid,
        requested_scope: &Scope,
        action: &str,
        business_unit: Option<BusinessUnitId>,
        agent_state: DataPlaneAgentState,
    ) -> Result<(), DataPlaneContractError> {
        if self.id.is_nil()
            || self.request_id.is_nil()
            || self.agent_id.is_nil()
            || self.token_hash == [0; 32]
        {
            return Err(DataPlaneContractError::InvalidCapability);
        }
        if self.revoked_at.is_some_and(|revoked| revoked <= at)
            || at < self.issued_at
            || at >= self.expires_at
        {
            return Err(DataPlaneContractError::CapabilityInactive);
        }
        if !agent_state.permits_root_session() {
            return Err(DataPlaneContractError::AgentUnavailable);
        }
        if self.root_principal != root_principal
            || self.device_ref != device_ref
            || self.tenant_id != tenant_id
            || self.agent_id != agent_id
        {
            return Err(DataPlaneContractError::CapabilityBindingMismatch);
        }
        if !self.scope.contains(requested_scope) {
            return Err(DataPlaneContractError::CapabilityScopeDenied);
        }
        if !self.allowed_actions.iter().any(|allowed| allowed == action) {
            return Err(DataPlaneContractError::CapabilityActionDenied);
        }
        if business_unit.is_some_and(|unit| !self.business_units.contains(&unit)) {
            return Err(DataPlaneContractError::CapabilityBusinessUnitDenied);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesiredConfiguration {
    pub id: Uuid,
    pub tenant_id: String,
    pub holding_id: Uuid,
    pub deployment_mode: TenantDeploymentMode,
    pub target_agent_id: Option<Uuid>,
    pub layer: ConfigurationLayer,
    pub expected_observed_version: u64,
    pub desired_version: u64,
    pub lifecycle: ConfigurationLifecycle,
    pub configuration_hash: [u8; 32],
    pub secret_references: Vec<SecretReference>,
    pub required_capabilities: Vec<AgentCapability>,
    pub requester: String,
    pub reason: String,
    pub expires_at: DateTime<Utc>,
}

impl DesiredConfiguration {
    pub fn validate(&self, at: DateTime<Utc>) -> Result<(), DataPlaneContractError> {
        if self.id.is_nil() || self.holding_id.is_nil() || self.configuration_hash == [0; 32] {
            return Err(DataPlaneContractError::NilIdentifier);
        }
        validate_reference(&self.tenant_id, "tenant_id")?;
        validate_reference(&self.requester, "requester")?;
        validate_reason(&self.reason)?;
        if self.expires_at <= at {
            return Err(DataPlaneContractError::ConfigurationExpired);
        }
        if self.expected_observed_version.checked_add(1) != Some(self.desired_version) {
            return Err(DataPlaneContractError::StaleConfigurationVersion);
        }
        match (self.deployment_mode, self.target_agent_id) {
            (TenantDeploymentMode::ErpxManaged, None)
            | (TenantDeploymentMode::CustomerDataPlane, Some(_)) => {}
            _ => return Err(DataPlaneContractError::InvalidConfigurationTarget),
        }
        if self.target_agent_id.is_some_and(|id| id.is_nil()) {
            return Err(DataPlaneContractError::NilIdentifier);
        }
        if self.secret_references.iter().collect::<BTreeSet<_>>().len()
            != self.secret_references.len()
            || self
                .required_capabilities
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != self.required_capabilities.len()
        {
            return Err(DataPlaneContractError::DuplicateConfigurationValue);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedDataPlaneState {
    pub tenant_id: String,
    pub agent_id: Uuid,
    pub observed_version: u64,
    pub desired_version: u64,
    pub write_schema_version: String,
    pub read_schema_version: String,
    pub write_healthy: bool,
    pub read_healthy: bool,
    pub storage_healthy: bool,
    pub directory_ready: bool,
    pub projections_healthy: bool,
    pub backup_verified: bool,
    pub failure_category: Option<String>,
    pub correlation_id: Uuid,
    pub observed_at: DateTime<Utc>,
}

impl ObservedDataPlaneState {
    pub fn is_drifted(&self) -> bool {
        self.observed_version != self.desired_version
    }

    pub fn is_healthy(&self) -> bool {
        !self.is_drifted()
            && self.write_healthy
            && self.read_healthy
            && self.storage_healthy
            && self.projections_healthy
            && self.backup_verified
            && self.failure_category.is_none()
    }

    pub fn validate(&self) -> Result<(), DataPlaneContractError> {
        validate_reference(&self.tenant_id, "tenant_id")?;
        if self.agent_id.is_nil() || self.correlation_id.is_nil() {
            return Err(DataPlaneContractError::NilIdentifier);
        }
        validate_reference(&self.write_schema_version, "write schema version")?;
        validate_reference(&self.read_schema_version, "read schema version")?;
        if let Some(category) = &self.failure_category {
            validate_reference(category, "failure category")?;
            if category.contains("password")
                || category.contains("secret")
                || category.contains("token")
                || category.contains("credential")
            {
                return Err(DataPlaneContractError::SensitiveDiagnostic);
            }
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TenantExecutionRoute {
    ErpxManaged {
        write_database_key: String,
        read_database_key: String,
        storage_namespace: String,
    },
    CustomerAgent {
        agent_id: Uuid,
    },
}

impl fmt::Debug for TenantExecutionRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ErpxManaged { .. } => formatter
                .debug_struct("TenantExecutionRoute::ErpxManaged")
                .field("resources", &"<redacted>")
                .finish(),
            Self::CustomerAgent { agent_id } => formatter
                .debug_struct("TenantExecutionRoute::CustomerAgent")
                .field("agent_id", agent_id)
                .finish(),
        }
    }
}

impl TenantExecutionRoute {
    pub fn validate(&self) -> Result<(), DataPlaneContractError> {
        match self {
            Self::ErpxManaged {
                write_database_key,
                read_database_key,
                storage_namespace,
            } => {
                validate_reference(write_database_key, "write_database_key")?;
                validate_reference(read_database_key, "read_database_key")?;
                validate_reference(storage_namespace, "storage_namespace")?;
                if write_database_key == read_database_key {
                    return Err(DataPlaneContractError::WriteReadDatabaseCollision);
                }
            }
            Self::CustomerAgent { agent_id } if agent_id.is_nil() => {
                return Err(DataPlaneContractError::NilIdentifier);
            }
            Self::CustomerAgent { .. } => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MandatoryPlatformPolicy {
    pub tls_required: bool,
    pub encryption_at_rest_required: bool,
    pub audit_required: bool,
    pub expiring_roles_required: bool,
    pub high_risk_mfa_required: bool,
    pub separation_of_duties_required: bool,
    pub customer_approval_required: bool,
    pub holding_isolation_required: bool,
    pub credential_redaction_required: bool,
    pub signed_packages_required: bool,
    pub backup_restore_verification_required: bool,
}

impl MandatoryPlatformPolicy {
    pub const fn secure_default() -> Self {
        Self {
            tls_required: true,
            encryption_at_rest_required: true,
            audit_required: true,
            expiring_roles_required: true,
            high_risk_mfa_required: true,
            separation_of_duties_required: true,
            customer_approval_required: true,
            holding_isolation_required: true,
            credential_redaction_required: true,
            signed_packages_required: true,
            backup_restore_verification_required: true,
        }
    }

    pub fn validate(self) -> Result<(), DataPlaneContractError> {
        let fields = [
            (self.tls_required, "tls_required"),
            (
                self.encryption_at_rest_required,
                "encryption_at_rest_required",
            ),
            (self.audit_required, "audit_required"),
            (self.expiring_roles_required, "expiring_roles_required"),
            (self.high_risk_mfa_required, "high_risk_mfa_required"),
            (
                self.separation_of_duties_required,
                "separation_of_duties_required",
            ),
            (
                self.customer_approval_required,
                "customer_approval_required",
            ),
            (
                self.holding_isolation_required,
                "holding_isolation_required",
            ),
            (
                self.credential_redaction_required,
                "credential_redaction_required",
            ),
            (self.signed_packages_required, "signed_packages_required"),
            (
                self.backup_restore_verification_required,
                "backup_restore_verification_required",
            ),
        ];
        fields
            .into_iter()
            .find(|(enabled, _)| !enabled)
            .map_or(Ok(()), |(_, field)| {
                Err(DataPlaneContractError::MandatoryPolicyDisabled(field))
            })
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DataPlaneContractError {
    #[error("business unit is not part of the canonical catalog")]
    UnsupportedBusinessUnit,
    #[error("role key is not an allow-listed ERPX role template")]
    UnsupportedRoleTemplate,
    #[error("identifier is nil")]
    NilIdentifier,
    #[error("reference is empty, oversized, or unsafe: {0}")]
    InvalidReference(&'static str),
    #[error("secret reference is not an approved secret-manager locator")]
    InvalidSecretReference,
    #[error("certificate fingerprint must be a SHA-256 hexadecimal value")]
    InvalidCertificateFingerprint,
    #[error("agent capabilities must be non-empty and unique")]
    InvalidAgentCapabilities,
    #[error("write and read database references must be different")]
    WriteReadDatabaseCollision,
    #[error("infrastructure reference owner does not match deployment mode")]
    InfrastructureOwnerMismatch,
    #[error("Root access must target a customer tenant scope")]
    RootAccessMustBeTenantScoped,
    #[error("Root access window is empty or exceeds its mode maximum")]
    InvalidRootAccessWindow,
    #[error("Root access actions must be bounded, explicit, and unique")]
    InvalidAllowedActions,
    #[error("business-unit selection is invalid for this Root access mode")]
    InvalidBusinessUnitSelection,
    #[error("approval does not reference the access request")]
    ApprovalRequestMismatch,
    #[error("customer approver must differ from the ERPX Root requester")]
    ApproverMustDifferFromRequester,
    #[error("fresh customer MFA is required")]
    FreshMfaRequired,
    #[error("approval is outside the requested access window")]
    ApprovalOutsideRequestWindow,
    #[error("Root capability is malformed")]
    InvalidCapability,
    #[error("Root capability is expired, revoked, or not yet active")]
    CapabilityInactive,
    #[error("customer data-plane agent is not connected")]
    AgentUnavailable,
    #[error("Root capability actor, device, tenant, or agent binding differs")]
    CapabilityBindingMismatch,
    #[error("Root capability does not contain the requested organization scope")]
    CapabilityScopeDenied,
    #[error("Root capability does not contain the requested action")]
    CapabilityActionDenied,
    #[error("Root capability does not contain the requested business unit")]
    CapabilityBusinessUnitDenied,
    #[error("desired configuration has expired")]
    ConfigurationExpired,
    #[error("desired configuration does not increment the observed version")]
    StaleConfigurationVersion,
    #[error("deployment mode and configuration target are inconsistent")]
    InvalidConfigurationTarget,
    #[error("desired configuration contains a duplicate reference or capability")]
    DuplicateConfigurationValue,
    #[error("diagnostic category contains sensitive material")]
    SensitiveDiagnostic,
    #[error("mandatory platform policy cannot be disabled: {0}")]
    MandatoryPolicyDisabled(&'static str),
    #[error("business-unit configuration is outside its holding, unit, or policy limits")]
    InvalidBusinessUnitConfiguration,
}

fn validate_reference(value: &str, field: &'static str) -> Result<(), DataPlaneContractError> {
    if value.is_empty()
        || value.len() > MAX_REFERENCE_BYTES
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(DataPlaneContractError::InvalidReference(field));
    }
    Ok(())
}

fn validate_reason(value: &str) -> Result<(), DataPlaneContractError> {
    if value.is_empty()
        || value.len() > MAX_REASON_BYTES
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(DataPlaneContractError::InvalidReference("reason"));
    }
    Ok(())
}

fn validate_permission(value: &str) -> Result<(), DataPlaneContractError> {
    if value.is_empty()
        || value.len() > MAX_PERMISSION_BYTES
        || value.contains('*')
        || value.starts_with('.')
        || value.ends_with('.')
        || value.split('.').any(|segment| {
            segment.is_empty()
                || !segment.chars().all(|character| {
                    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
                })
        })
    {
        return Err(DataPlaneContractError::InvalidAllowedActions);
    }
    Ok(())
}

fn validate_permission_segment(value: &str) -> Result<(), DataPlaneContractError> {
    if value.is_empty()
        || value.len() > 32
        || !value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err(DataPlaneContractError::InvalidAllowedActions);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 1, hour, 0, 0)
            .single()
            .expect("valid time")
    }

    fn branch_scope() -> Scope {
        Scope::Branch {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: Uuid::from_u128(3),
        }
    }

    fn root_request() -> RootAccessRequest {
        RootAccessRequest {
            id: Uuid::from_u128(10),
            requester: "platform-root-operator".into(),
            device_ref: "device-1".into(),
            tenant_id: "tenant-1".into(),
            agent_id: Uuid::from_u128(20),
            tenant_scope: Scope::Holding {
                holding_id: Uuid::from_u128(1),
            },
            mode: RootAccessMode::InfrastructureMaintenance,
            allowed_actions: vec![TENANT_INFRASTRUCTURE_APPLY.into()],
            business_units: vec![],
            reason: "Apply an approved database migration".into(),
            ticket_reference: "CHG-100".into(),
            requested_at: at(8),
            requested_until: at(10),
        }
    }

    #[test]
    fn canonical_business_unit_catalog_is_exact_and_support_free() {
        assert_eq!(BusinessUnitId::ALL.len(), 12);
        assert_eq!(BusinessUnitId::ALL[0].as_str(), "operations");
        assert_eq!(BusinessUnitId::ALL[11].as_str(), "legal");
        assert!(BusinessUnitId::parse("assets").is_ok());
        assert!(BusinessUnitId::parse("support").is_err());
        assert!(BusinessUnitId::parse("hr").is_err());
        assert_eq!(BUSINESS_UNIT_ROUTE_CATALOG.len(), BusinessUnitId::ALL.len());
        for (unit, route) in BusinessUnitId::ALL
            .into_iter()
            .zip(BUSINESS_UNIT_ROUTE_CATALOG)
        {
            assert_eq!(route.unit, unit);
            assert_eq!(route.route_id, unit.as_str());
            assert_eq!(route.entitlement_key, unit.as_str());
            assert_eq!(route.read_permission, unit.permission("read").unwrap());
            assert!(route.canonical_path.starts_with("/workspaces/"));
            assert!(route.label_key.starts_with("nav.business_unit."));
            assert!(!route.label_en.is_empty());
            assert!(!route.label_th.is_empty());
        }
        assert_eq!(
            BusinessUnitId::MarketingCrm.route().canonical_path,
            "/workspaces/marketing-crm"
        );
    }

    #[test]
    fn business_role_templates_keep_approval_separate_from_administration() {
        let admin = BusinessUnitId::Finance
            .role_permissions(BusinessUnitRoleLevel::Administrator)
            .expect("permissions");
        assert!(admin.contains(&"finance.admin".into()));
        assert!(!admin.contains(&"finance.approve".into()));
        assert_eq!(
            BusinessUnitId::PointOfSale.role_key(BusinessUnitRoleLevel::Approver),
            "point-of-sale_approver"
        );
        let templates = BusinessUnitId::ALL
            .into_iter()
            .flat_map(|unit| {
                BusinessUnitRoleLevel::ALL
                    .into_iter()
                    .map(move |level| BusinessUnitRoleTemplate::new(unit, level).unwrap())
            })
            .collect::<Vec<_>>();
        assert_eq!(templates.len(), 60);
        assert!(BusinessUnitRoleTemplate::parse("finance_operator").is_ok());
        assert!(BusinessUnitRoleTemplate::parse("finance_owner").is_err());
        assert!(directory_role_template_is_allowed(
            ConfigurationLayer::BusinessUnit(BusinessUnitId::Finance),
            "finance_operator"
        ));
        assert!(!directory_role_template_is_allowed(
            ConfigurationLayer::BusinessUnit(BusinessUnitId::Sales),
            "finance_operator"
        ));
        assert!(!directory_role_template_is_allowed(
            ConfigurationLayer::Platform,
            "platform_root"
        ));
    }

    #[test]
    fn secret_references_are_locators_and_debug_is_redacted() {
        let reference =
            SecretReference::new("customer-secret:erpx/write-database").expect("valid reference");
        assert_eq!(format!("{reference:?}"), "SecretReference(<redacted>)");
        assert!(SecretReference::new("postgres://user:password@db").is_err());
        assert!(SecretReference::new("customer-secret:value=raw").is_err());
    }

    #[test]
    fn customer_profiles_require_distinct_customer_owned_references() {
        let profile = TenantInfrastructureProfile {
            tenant_id: "tenant-1".into(),
            holding_id: Uuid::from_u128(1),
            deployment_mode: TenantDeploymentMode::CustomerDataPlane,
            write_database_ref: SecretReference::new("customer-secret:erpx/write").unwrap(),
            read_database_ref: SecretReference::new("customer-secret:erpx/read").unwrap(),
            storage_ref: SecretReference::new("customer-secret:erpx/storage").unwrap(),
            directory_credential_ref: None,
            backup_profile_ref: SecretReference::new("customer-secret:erpx/backup").unwrap(),
            version: 1,
        };
        profile.validate().expect("valid profile");
        let mut wrong_owner = profile.clone();
        wrong_owner.storage_ref = SecretReference::new("erpx-managed:storage").unwrap();
        assert_eq!(
            wrong_owner.validate(),
            Err(DataPlaneContractError::InfrastructureOwnerMismatch)
        );
    }

    #[test]
    fn business_unit_configuration_stays_logical_and_role_allow_listed() {
        let configuration = BusinessUnitConfiguration {
            tenant_id: "tenant-1".into(),
            holding_id: Uuid::from_u128(1),
            business_unit: BusinessUnitId::Finance,
            configuration_version: 1,
            active: true,
            organization_scope: Scope::Holding {
                holding_id: Uuid::from_u128(1),
            },
            directory_role_mappings: vec![BusinessUnitDirectoryRoleMapping {
                group_ref: "AD-FINANCE-BKK-OPERATORS".into(),
                role_key: "finance_operator".into(),
                target_scope: branch_scope(),
                requires_mfa: true,
                assignment_duration_hours: 720,
                approval_limit_minor: 0,
                active: true,
            }],
            integration_references: vec!["integration:bank-import".into()],
            retention_policy: serde_json::json!({"days": 2555}),
            projection_policy: serde_json::json!({"maximum_lag_seconds": 30}),
            workflow_policy: serde_json::json!({"approval_limit_minor": 100000}),
            updated_by: "holding-owner".into(),
            updated_at: at(8),
        };
        configuration.validate().expect("valid logical unit config");

        let mut wrong_unit = configuration.clone();
        wrong_unit.directory_role_mappings[0].role_key = "sales_operator".into();
        assert_eq!(
            wrong_unit.validate(),
            Err(DataPlaneContractError::InvalidBusinessUnitConfiguration)
        );

        let mut sibling_holding = configuration;
        sibling_holding.directory_role_mappings[0].target_scope = Scope::Holding {
            holding_id: Uuid::from_u128(99),
        };
        assert_eq!(
            sibling_holding.validate(),
            Err(DataPlaneContractError::InvalidBusinessUnitConfiguration)
        );
    }

    #[test]
    fn configuration_lifecycle_is_closed_and_customer_approval_is_mandatory() {
        assert!(
            ConfigurationLifecycle::Draft
                .can_transition_to(ConfigurationLifecycle::AwaitingApproval)
        );
        assert!(!ConfigurationLifecycle::Draft.can_transition_to(ConfigurationLifecycle::Active));
        assert!(
            ConfigurationLifecycle::AwaitingApproval
                .can_transition_to(ConfigurationLifecycle::Rejected)
        );
    }

    #[test]
    fn agent_binding_requires_unique_capabilities_and_sha256_fingerprint() {
        let mut binding = DataPlaneAgentBinding {
            tenant_id: "tenant-1".into(),
            holding_id: Uuid::from_u128(1),
            agent_id: Uuid::from_u128(2),
            certificate_fingerprint: "a".repeat(64),
            capabilities: vec![AgentCapability::HealthTelemetry],
            state: DataPlaneAgentState::Healthy,
            observed_version: 1,
        };
        binding.validate().expect("valid binding");
        binding.capabilities.push(AgentCapability::HealthTelemetry);
        assert_eq!(
            binding.validate(),
            Err(DataPlaneContractError::InvalidAgentCapabilities)
        );
    }

    #[test]
    fn root_access_is_tenant_scoped_expiring_and_customer_approved() {
        let request = root_request();
        request.validate().expect("valid request");
        RootAccessApproval {
            request_id: request.id,
            approver: "customer-security-head".into(),
            approved_at: at(9),
            fresh_mfa: true,
        }
        .validate_for(&request)
        .expect("valid approval");

        let mut platform_request = request.clone();
        platform_request.tenant_scope = Scope::Platform;
        assert_eq!(
            platform_request.validate(),
            Err(DataPlaneContractError::RootAccessMustBeTenantScoped)
        );
    }

    #[test]
    fn requester_cannot_approve_and_fresh_mfa_is_required() {
        let request = root_request();
        let mut approval = RootAccessApproval {
            request_id: request.id,
            approver: request.requester.clone(),
            approved_at: at(9),
            fresh_mfa: true,
        };
        assert_eq!(
            approval.validate_for(&request),
            Err(DataPlaneContractError::ApproverMustDifferFromRequester)
        );
        approval
            .validate_for_with_policy(&request, ApprovalPolicy::LocalDevelopmentException)
            .expect("explicit local policy permits self-approval");
        approval.approver = "holding-owner".into();
        approval.fresh_mfa = false;
        assert_eq!(
            approval.validate_for(&request),
            Err(DataPlaneContractError::FreshMfaRequired)
        );
    }

    #[test]
    fn business_root_modes_require_explicit_units_and_no_wildcards() {
        let mut request = root_request();
        request.mode = RootAccessMode::BusinessDiagnostic;
        request.requested_until = at(12);
        assert_eq!(
            request.validate(),
            Err(DataPlaneContractError::InvalidBusinessUnitSelection)
        );
        request.business_units.push(BusinessUnitId::Finance);
        request.allowed_actions = vec!["finance.*".into()];
        assert_eq!(
            request.validate(),
            Err(DataPlaneContractError::InvalidAllowedActions)
        );
    }

    #[test]
    fn capability_is_exactly_bound_and_fails_closed_when_agent_is_offline() {
        let capability = RootAccessCapability {
            id: Uuid::from_u128(30),
            request_id: Uuid::from_u128(10),
            token_hash: [7; 32],
            root_principal: "root-operator".into(),
            device_ref: "device-1".into(),
            tenant_id: "tenant-1".into(),
            agent_id: Uuid::from_u128(20),
            scope: Scope::Holding {
                holding_id: Uuid::from_u128(1),
            },
            allowed_actions: vec![TENANT_INFRASTRUCTURE_APPLY.into()],
            business_units: vec![BusinessUnitId::Finance],
            issued_at: at(8),
            expires_at: at(10),
            revoked_at: None,
        };
        capability
            .authorize(
                at(9),
                "root-operator",
                "device-1",
                "tenant-1",
                Uuid::from_u128(20),
                &branch_scope(),
                TENANT_INFRASTRUCTURE_APPLY,
                Some(BusinessUnitId::Finance),
                DataPlaneAgentState::Healthy,
            )
            .expect("authorized");
        assert_eq!(
            capability.authorize(
                at(9),
                "root-operator",
                "device-1",
                "tenant-1",
                Uuid::from_u128(20),
                &branch_scope(),
                TENANT_INFRASTRUCTURE_APPLY,
                Some(BusinessUnitId::Finance),
                DataPlaneAgentState::Offline,
            ),
            Err(DataPlaneContractError::AgentUnavailable)
        );
    }

    #[test]
    fn desired_configuration_is_versioned_and_target_bound() {
        let desired = DesiredConfiguration {
            id: Uuid::from_u128(40),
            tenant_id: "tenant-1".into(),
            holding_id: Uuid::from_u128(1),
            deployment_mode: TenantDeploymentMode::CustomerDataPlane,
            target_agent_id: Some(Uuid::from_u128(20)),
            layer: ConfigurationLayer::BusinessUnit(BusinessUnitId::Finance),
            expected_observed_version: 4,
            desired_version: 5,
            lifecycle: ConfigurationLifecycle::Draft,
            configuration_hash: [3; 32],
            secret_references: vec![],
            required_capabilities: vec![AgentCapability::ConfigurationReconciliation],
            requester: "root-operator".into(),
            reason: "Change Finance projection configuration".into(),
            expires_at: at(12),
        };
        desired.validate(at(8)).expect("valid configuration");
        let mut stale = desired.clone();
        stale.desired_version = 6;
        assert_eq!(
            stale.validate(at(8)),
            Err(DataPlaneContractError::StaleConfigurationVersion)
        );
    }

    #[test]
    fn observed_state_reports_drift_without_exposing_sensitive_failure_text() {
        let mut observed = ObservedDataPlaneState {
            tenant_id: "tenant-1".into(),
            agent_id: Uuid::from_u128(20),
            observed_version: 4,
            desired_version: 5,
            write_schema_version: "70".into(),
            read_schema_version: "45".into(),
            write_healthy: true,
            read_healthy: true,
            storage_healthy: true,
            directory_ready: true,
            projections_healthy: true,
            backup_verified: true,
            failure_category: None,
            correlation_id: Uuid::from_u128(50),
            observed_at: at(9),
        };
        assert!(observed.is_drifted());
        assert!(!observed.is_healthy());
        observed.failure_category = Some("database-password-leaked".into());
        assert_eq!(
            observed.validate(),
            Err(DataPlaneContractError::SensitiveDiagnostic)
        );
    }

    #[test]
    fn tenant_execution_route_debug_redacts_managed_resources() {
        let route = TenantExecutionRoute::ErpxManaged {
            write_database_key: "write-secret".into(),
            read_database_key: "read-secret".into(),
            storage_namespace: "storage-secret".into(),
        };
        route.validate().expect("valid route");
        let debug = format!("{route:?}");
        assert!(!debug.contains("write-secret"));
        assert!(debug.contains("redacted"));
    }

    #[test]
    fn mandatory_policy_cannot_be_weakened_by_lower_layers() {
        MandatoryPlatformPolicy::secure_default()
            .validate()
            .expect("secure policy");
        let mut weakened = MandatoryPlatformPolicy::secure_default();
        weakened.customer_approval_required = false;
        assert_eq!(
            weakened.validate(),
            Err(DataPlaneContractError::MandatoryPolicyDisabled(
                "customer_approval_required"
            ))
        );
    }
}
