//! Human-workflow routing and transport classification for the target service catalog.
//!
//! The compile-time service authority lives in `erpx-integration-contracts`. This
//! module adds presentation metadata without re-declaring legacy transports or
//! accepting generic payloads. The server remains the authorization authority.

use erpx_integration_contracts::catalog::{ACTIVE_SERVICES, ServiceKind};

/// The audience and ingress boundary for an RPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcClassification {
    /// An authenticated person invokes the RPC from an ERPX application.
    HumanInteractive,
    /// An anonymous person invokes a deliberately public, abuse-controlled RPC.
    PublicHuman,
    /// An ERPX data-plane agent invokes the RPC without a human UI.
    MachineOnly,
    /// An external provider invokes the RPC or equivalent ingress boundary.
    ProviderIngress,
    /// An ERPX worker invokes the RPC or equivalent internal boundary.
    WorkerOnly,
}

impl RpcClassification {
    /// Whether route/action/state parity is required for this classification.
    pub const fn requires_human_surface(self) -> bool {
        matches!(self, Self::HumanInteractive | Self::PublicHuman)
    }
}

/// The reviewed client adapter used to construct or consume the wire message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HumanAdapterKind {
    /// A bounded typed read DTO is rendered through a domain view model.
    TypedRead,
    /// A bounded typed command form creates a typed protobuf command directly.
    TypedCommand,
}

/// Every human route must represent these states without raw transport errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HumanUiState {
    Loading,
    Empty,
    Denied,
    Offline,
    Stale,
    Conflict,
    Submitting,
    Success,
    RetryableError,
    Unavailable,
}

/// Canonical state contract shared by every human workflow.
pub const HUMAN_UI_STATES: &[HumanUiState] = &[
    HumanUiState::Loading,
    HumanUiState::Empty,
    HumanUiState::Denied,
    HumanUiState::Offline,
    HumanUiState::Stale,
    HumanUiState::Conflict,
    HumanUiState::Submitting,
    HumanUiState::Success,
    HumanUiState::RetryableError,
    HumanUiState::Unavailable,
];

/// Presentation metadata for one active service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RpcServiceWorkflow {
    pub service: &'static str,
    /// RPC methods are descriptor-owned and intentionally not duplicated here.
    pub methods: &'static [&'static str],
    pub classification: RpcClassification,
    pub routes: &'static [&'static str],
}

/// The resolved parity contract for one target RPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RpcWorkflow<'a> {
    pub service: &'a str,
    pub method: &'a str,
    pub classification: RpcClassification,
    pub routes: &'static [&'static str],
    /// Stable server-owned capability key. The RPC method is deliberately the
    /// source of truth so capability drift is visible in descriptor review.
    pub action_capability: Option<&'a str>,
    pub adapter: Option<HumanAdapterKind>,
    pub states: &'static [HumanUiState],
}

const NO_METHODS: &[&str] = &[];
const NO_ROUTES: &[&str] = &[];
const IDENTITY_ROUTES: &[&str] = &["/platform/role-review"];
const PLATFORM_ROUTES: &[&str] = &["/platform"];
const ORGANIZATION_ROUTES: &[&str] = &["/holding/organization"];
const COMMERCIAL_ROUTES: &[&str] = &["/billing"];
const BRAND_ROUTES: &[&str] = &["/holding/corporate-identity"];
const CUSTOMER_ROUTES: &[&str] = &["/platform/holdings"];
const SUPPORT_ROUTES: &[&str] = &["/support"];
const FINANCE_ROUTES: &[&str] = &["/finance"];
const SALES_ROUTES: &[&str] = &["/sales"];
const POS_ROUTES: &[&str] = &["/pos"];
const PROCUREMENT_ROUTES: &[&str] = &["/procurement"];
const INVENTORY_ROUTES: &[&str] = &["/inventory"];
const ASSETS_ROUTES: &[&str] = &["/asset"];
const MAINTENANCE_ROUTES: &[&str] = &["/maintenance"];
const QUALITY_ROUTES: &[&str] = &["/quality"];
const OPERATIONS_ROUTES: &[&str] = &["/operations"];
const PEOPLE_PAYROLL_ROUTES: &[&str] = &["/people"];
const LEGAL_ROUTES: &[&str] = &["/legal"];
const MARKETING_ROUTES: &[&str] = &["/marketing"];
const CRM_ROUTES: &[&str] = &["/crm"];
const MANUFACTURING_ROUTES: &[&str] = &["/manufacturing"];
const PLANNING_ROUTES: &[&str] = &["/planning"];
const PROJECT_ROUTES: &[&str] = &["/project"];
const COMMERCE_ROUTES: &[&str] = &["/commerce"];
const TALENT_ROUTES: &[&str] = &["/talent"];
const LEARNING_ROUTES: &[&str] = &["/learning"];
const BENEFITS_ROUTES: &[&str] = &["/benefits"];
const FIELD_SERVICE_ROUTES: &[&str] = &["/field-service"];
const AI_ROUTES: &[&str] = &["/ai"];

macro_rules! human_service {
    ($service:literal, $routes:ident) => {
        RpcServiceWorkflow {
            service: $service,
            methods: NO_METHODS,
            classification: RpcClassification::HumanInteractive,
            routes: $routes,
        }
    };
}

/// Presentation metadata for exactly the 67 active target services.
///
/// Method names remain descriptor-owned. `rpc_workflow` first proves service
/// membership against `ACTIVE_SERVICES`; descriptor tests then exercise every
/// declared RPC method through this service-level policy.
pub const RPC_WORKFLOW_SERVICES: &[RpcServiceWorkflow] = &[
    human_service!("IdentityAccessCommandService", IDENTITY_ROUTES),
    human_service!("IdentityAccessQueryService", IDENTITY_ROUTES),
    human_service!("PlatformAdministrationCommandService", PLATFORM_ROUTES),
    human_service!("PlatformAdministrationQueryService", PLATFORM_ROUTES),
    human_service!("OrganizationCommandService", ORGANIZATION_ROUTES),
    human_service!("OrganizationQueryService", ORGANIZATION_ROUTES),
    human_service!("CommercialCommandService", COMMERCIAL_ROUTES),
    human_service!("CommercialQueryService", COMMERCIAL_ROUTES),
    human_service!("BrandCommandService", BRAND_ROUTES),
    human_service!("BrandQueryService", BRAND_ROUTES),
    human_service!("CustomerAuthorityCommandService", CUSTOMER_ROUTES),
    human_service!("CustomerAuthorityQueryService", CUSTOMER_ROUTES),
    human_service!("Customer360CommandService", CUSTOMER_ROUTES),
    human_service!("Customer360QueryService", CUSTOMER_ROUTES),
    human_service!("SupportCommandService", SUPPORT_ROUTES),
    human_service!("SupportQueryService", SUPPORT_ROUTES),
    human_service!("FinanceCommandService", FINANCE_ROUTES),
    human_service!("FinanceQueryService", FINANCE_ROUTES),
    human_service!("FinancePayablesCommandService", FINANCE_ROUTES),
    human_service!("FinancePayablesQueryService", FINANCE_ROUTES),
    human_service!("SalesCommandService", SALES_ROUTES),
    human_service!("SalesQueryService", SALES_ROUTES),
    human_service!("PointOfSaleCommandService", POS_ROUTES),
    human_service!("PointOfSaleQueryService", POS_ROUTES),
    human_service!("ProcurementCommandService", PROCUREMENT_ROUTES),
    human_service!("ProcurementQueryService", PROCUREMENT_ROUTES),
    human_service!("InventoryCommandService", INVENTORY_ROUTES),
    human_service!("InventoryQueryService", INVENTORY_ROUTES),
    human_service!("AssetsCommandService", ASSETS_ROUTES),
    human_service!("AssetsQueryService", ASSETS_ROUTES),
    human_service!("MaintenanceCommandService", MAINTENANCE_ROUTES),
    human_service!("MaintenanceQueryService", MAINTENANCE_ROUTES),
    human_service!("QualityCommandService", QUALITY_ROUTES),
    human_service!("QualityQueryService", QUALITY_ROUTES),
    human_service!("OperationsCommandService", OPERATIONS_ROUTES),
    human_service!("OperationsQueryService", OPERATIONS_ROUTES),
    human_service!("PeoplePayrollCommandService", PEOPLE_PAYROLL_ROUTES),
    human_service!("PeoplePayrollQueryService", PEOPLE_PAYROLL_ROUTES),
    human_service!("LegalCommandService", LEGAL_ROUTES),
    human_service!("LegalQueryService", LEGAL_ROUTES),
    human_service!("MarketingCommandService", MARKETING_ROUTES),
    human_service!("MarketingQueryService", MARKETING_ROUTES),
    human_service!("CrmCommandService", CRM_ROUTES),
    human_service!("CrmQueryService", CRM_ROUTES),
    human_service!("ManufacturingCommandService", MANUFACTURING_ROUTES),
    human_service!("ManufacturingQueryService", MANUFACTURING_ROUTES),
    human_service!("PlanningCommandService", PLANNING_ROUTES),
    human_service!("PlanningQueryService", PLANNING_ROUTES),
    human_service!("ProjectsCommandService", PROJECT_ROUTES),
    human_service!("ProjectsQueryService", PROJECT_ROUTES),
    human_service!("EcommerceCommandService", COMMERCE_ROUTES),
    human_service!("EcommerceQueryService", COMMERCE_ROUTES),
    human_service!("TalentCommandService", TALENT_ROUTES),
    human_service!("TalentQueryService", TALENT_ROUTES),
    human_service!("LearningCommandService", LEARNING_ROUTES),
    human_service!("LearningQueryService", LEARNING_ROUTES),
    human_service!("BenefitsCommandService", BENEFITS_ROUTES),
    human_service!("BenefitsQueryService", BENEFITS_ROUTES),
    human_service!("FieldServiceCommandService", FIELD_SERVICE_ROUTES),
    human_service!("FieldServiceQueryService", FIELD_SERVICE_ROUTES),
    human_service!("CollaborationCommandService", AI_ROUTES),
    human_service!("CollaborationQueryService", AI_ROUTES),
    human_service!("AgentOrchestrationCommandService", AI_ROUTES),
    human_service!("AgentOrchestrationQueryService", AI_ROUTES),
    human_service!("CollaborationStreamService", AI_ROUTES),
    human_service!("ReferenceQueryService", AI_ROUTES),
    RpcServiceWorkflow {
        service: "DataPlaneStreamService",
        methods: NO_METHODS,
        classification: RpcClassification::MachineOnly,
        routes: NO_ROUTES,
    },
];

/// Resolve one descriptor RPC through the active target registry.
pub fn rpc_workflow<'a>(service: &'a str, method: &'a str) -> Option<RpcWorkflow<'a>> {
    if method.is_empty()
        || !ACTIVE_SERVICES
            .iter()
            .any(|candidate| candidate.name == service)
    {
        return None;
    }
    let registered = RPC_WORKFLOW_SERVICES
        .iter()
        .find(|candidate| candidate.service == service)?;
    let classification = if service == "CommercialCommandService"
        && matches!(method, "ApplyBillingEvent" | "ApplyEntitlementChange")
    {
        RpcClassification::ProviderIngress
    } else {
        registered.classification
    };
    let adapter = classification
        .requires_human_surface()
        .then(|| adapter_for(service, method));
    Some(RpcWorkflow {
        service,
        method,
        classification,
        routes: if classification.requires_human_surface() {
            registered.routes
        } else {
            NO_ROUTES
        },
        action_capability: classification.requires_human_surface().then_some(method),
        adapter,
        states: if classification.requires_human_surface() {
            HUMAN_UI_STATES
        } else {
            &[]
        },
    })
}

fn adapter_for(service: &str, method: &str) -> HumanAdapterKind {
    let query_service = ACTIVE_SERVICES
        .iter()
        .any(|candidate| candidate.name == service && matches!(candidate.kind, ServiceKind::Query));
    if query_service
        || method.starts_with("Get")
        || method.starts_with("List")
        || method.starts_with("Search")
        || method.starts_with("Resolve")
        || method.starts_with("Stream")
    {
        HumanAdapterKind::TypedRead
    } else {
        HumanAdapterKind::TypedCommand
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const ACTIVE_PROTO: &str = concat!(
        include_str!("../../proto/proto/erpx/identity_access/service.proto"),
        include_str!("../../proto/proto/erpx/platform_administration/service.proto"),
        include_str!("../../proto/proto/erpx/organization/service.proto"),
        include_str!("../../proto/proto/erpx/commercial/service.proto"),
        include_str!("../../proto/proto/erpx/brand/service.proto"),
        include_str!("../../proto/proto/erpx/customer_authority/service.proto"),
        include_str!("../../proto/proto/erpx/customer_360/service.proto"),
        include_str!("../../proto/proto/erpx/support/service.proto"),
        include_str!("../../proto/proto/erpx/finance/service.proto"),
        include_str!("../../proto/proto/erpx/sales/service.proto"),
        include_str!("../../proto/proto/erpx/pos/service.proto"),
        include_str!("../../proto/proto/erpx/procurement/service.proto"),
        include_str!("../../proto/proto/erpx/inventory/service.proto"),
        include_str!("../../proto/proto/erpx/assets/service.proto"),
        include_str!("../../proto/proto/erpx/maintenance/service.proto"),
        include_str!("../../proto/proto/erpx/quality/service.proto"),
        include_str!("../../proto/proto/erpx/operations/service.proto"),
        include_str!("../../proto/proto/erpx/people_payroll/service.proto"),
        include_str!("../../proto/proto/erpx/legal/service.proto"),
        include_str!("../../proto/proto/erpx/marketing/service.proto"),
        include_str!("../../proto/proto/erpx/crm/service.proto"),
        include_str!("../../proto/proto/erpx/manufacturing/service.proto"),
        include_str!("../../proto/proto/erpx/planning/service.proto"),
        include_str!("../../proto/proto/erpx/projects/service.proto"),
        include_str!("../../proto/proto/erpx/ecommerce/service.proto"),
        include_str!("../../proto/proto/erpx/talent/service.proto"),
        include_str!("../../proto/proto/erpx/learning/service.proto"),
        include_str!("../../proto/proto/erpx/benefits/service.proto"),
        include_str!("../../proto/proto/erpx/field_service/service.proto"),
        include_str!("../../proto/proto/erpx/collaboration/service.proto"),
        include_str!("../../proto/proto/erpx/agent/service.proto"),
        include_str!("../../proto/proto/erpx/reference/service.proto"),
    );

    fn descriptor_rpcs() -> BTreeSet<(String, String)> {
        let mut service = None::<String>;
        let mut result = BTreeSet::new();
        for line in ACTIVE_PROTO.lines().map(str::trim) {
            if let Some(rest) = line.strip_prefix("service ") {
                service = rest.split_whitespace().next().map(str::to_owned);
            } else if line == "}" {
                service = None;
            } else if let Some(rest) = line.strip_prefix("rpc ") {
                let method = rest.split('(').next().unwrap_or_default().trim();
                if let Some(service) = service.as_ref() {
                    result.insert((service.clone(), method.to_owned()));
                }
            }
        }
        result
    }

    #[test]
    fn active_catalog_and_workflow_registry_match_exactly() {
        assert_eq!(RPC_WORKFLOW_SERVICES.len(), 67);
        let catalog = ACTIVE_SERVICES
            .iter()
            .map(|service| service.name)
            .collect::<BTreeSet<_>>();
        let workflow = RPC_WORKFLOW_SERVICES
            .iter()
            .map(|service| service.service)
            .collect::<BTreeSet<_>>();
        assert_eq!(catalog.len(), 67);
        assert_eq!(workflow.len(), 67);
        assert_eq!(catalog, workflow);
    }

    #[test]
    fn every_active_descriptor_rpc_has_an_explicit_target_policy() {
        let descriptor = descriptor_rpcs();
        assert!(!descriptor.is_empty());
        for (service, method) in &descriptor {
            assert!(
                rpc_workflow(service, method).is_some(),
                "{service}.{method} has no target RPC policy"
            );
        }
    }

    #[test]
    fn every_human_rpc_has_routes_adapter_action_and_visible_states() {
        for (service, method) in descriptor_rpcs() {
            let workflow = rpc_workflow(&service, &method).expect("active RPC");
            if workflow.classification.requires_human_surface() {
                assert!(
                    !workflow.routes.is_empty(),
                    "{service}.{method} has no route"
                );
                assert!(
                    workflow.adapter.is_some(),
                    "{service}.{method} has no adapter"
                );
                assert!(
                    workflow.action_capability.is_some(),
                    "{service}.{method} has no action"
                );
                assert_eq!(workflow.states, HUMAN_UI_STATES);
                assert!(
                    workflow
                        .routes
                        .iter()
                        .all(|route| route.starts_with('/') && !route.contains(['?', '#']))
                );
            } else {
                assert!(workflow.routes.is_empty());
                assert!(workflow.adapter.is_none());
                assert!(workflow.action_capability.is_none());
                assert!(workflow.states.is_empty());
            }
        }
    }

    #[test]
    fn provider_and_data_plane_boundaries_cannot_be_confused_with_human_actions() {
        for method in ["ApplyBillingEvent", "ApplyEntitlementChange"] {
            let provider = rpc_workflow("CommercialCommandService", method).expect("provider RPC");
            assert_eq!(provider.classification, RpcClassification::ProviderIngress);
            assert!(!provider.classification.requires_human_surface());
        }
        let machine = rpc_workflow("DataPlaneStreamService", "Connect").expect("agent stream");
        assert_eq!(machine.classification, RpcClassification::MachineOnly);
        assert!(!machine.classification.requires_human_surface());

        // Public forms and worker consumers are HTTP/outbox boundaries, not
        // members of the internal service catalog, but remain explicit vocabulary.
        assert!(RpcClassification::PublicHuman.requires_human_surface());
        assert!(!RpcClassification::WorkerOnly.requires_human_surface());
        assert!(rpc_workflow("PlatformOperationsService", "ListHoldings").is_none());
    }
}
