//! G12 UI/QA contract tests and integration-facing evidence helpers.
//!
//! These checks are intentionally executable once the coordinator registers both completion
//! modules from the console binary. They test the client contract, not live Android/iOS device
//! execution or external LDAP/AD/UCS certification.

use super::product_completion::{
    AccessSecurityPolicy, AuthoritativeProductReadModel, AuthorizationEvidence, ConnectorScope,
    ContextSurfaceModel, DIRECTORY_READ_MODEL_AUTHORITY, DIRECTORY_ROLLBACK_BOUNDARY,
    DirectoryAdministrationModel, DirectoryConnectorContract, DirectoryConnectorPolicy,
    DirectoryRoleMapping, DirectoryVendor, FIVE_MINUTE_REVOCATION_SECONDS, FullImportCapability,
    HighRiskMfaPolicy, IdentityLinkingMode, ImmutableIdentityLink, LocalFallbackMode,
    MAX_FULL_IMPORT_USERS, MAX_PROJECTION_FRESHNESS_SECONDS, MobileTargetHandoff,
    MobileTargetHandoffStatus, NativeChangeFeed, OpaqueIdentity, PRODUCT_CONTEXTS,
    PersistenceEvidence, PlatformOperationsArea, PlatformOperationsAreaEvidence,
    PlatformOperationsModel, ProductContext, ProductContextScope, ProductContractError,
    ProductPage, ProductPageCursor, ProductRouteResponse, ProductRouteStateAdapter,
    ProductRowEvidence, ProductSurface, ProductSurfaceModel, ProductSurfaceState,
    ProjectionFreshness, ReconciliationEvidence, RevocationPolicy, RoleMappingMode,
    RoleRevocationMode, SafeDisplayText, safe_return_path,
};
use erpx_ui::{ClientTarget, SecureSessionSupport, UiStateKind};

/// Contract identifiers used by the coordinator's G12 evidence roll-up.
pub const G12_QA_CONTRACTS: &[&str] = &[
    "G12-UI-DIRECTORY-ADMINISTRATION",
    "G12-UI-PLATFORM-OPERATIONS",
    "G12-UI-PLATFORM-ADAPTER-COVERAGE",
    "G12-UI-PRODUCT-CONTEXTS",
    "G12-UI-PRODUCT-ROW-EVIDENCE",
    "G12-UI-EXPLICIT-STATES",
    "G12-UI-TARGET-STATE-MATRIX",
    "G12-UI-ACCESSIBLE-NAMING",
    "G12-UI-REDACTION",
    "G12-UI-HIGH-RISK-MFA",
    "G12-UI-FIVE-MINUTE-REVOCATION",
    "G12-UI-MOBILE-HANDOFF",
];

/// External certification remains an explicit G14 blocker until authoritative vendor evidence is
/// registered. This test-only status cannot represent successful certification by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalCertificationReadiness {
    Required,
}

impl ExternalCertificationReadiness {
    pub const fn has_authoritative_evidence(self) -> bool {
        false
    }

    pub const fn claims_completion(self) -> bool {
        false
    }
}

pub const G14_EXTERNAL_CERTIFICATION_CONTRACTS: &[&str] = &[
    "G14-EXTERNAL-LDAP-CERTIFICATION-REQUIRED",
    "G14-EXTERNAL-AD-DS-CERTIFICATION-REQUIRED",
    "G14-EXTERNAL-UCS-CERTIFICATION-REQUIRED",
];

pub const G14_EXTERNAL_READINESS: ExternalCertificationReadiness =
    ExternalCertificationReadiness::Required;

/// Return whether a product state is one of the explicit non-ready states required by G12.
pub const fn is_explicit_non_ready_state(state: &ProductSurfaceState) -> bool {
    !matches!(state, ProductSurfaceState::Ready { .. })
}

fn opaque(value: &str) -> OpaqueIdentity {
    OpaqueIdentity::new(value).expect("QA fixture must use an opaque identity")
}

fn connector(vendor: DirectoryVendor, scope: ConnectorScope) -> DirectoryConnectorContract {
    DirectoryConnectorContract::new(
        opaque("connector-001"),
        vendor,
        scope,
        DirectoryConnectorPolicy::strict(vendor),
    )
    .expect("strict QA connector must satisfy the approved contract")
}

fn directory_model() -> DirectoryAdministrationModel {
    let platform = connector(DirectoryVendor::GenericLdap, ConnectorScope::Platform);
    let holding = connector(
        DirectoryVendor::ActiveDirectoryDomainServices,
        ConnectorScope::Holding {
            holding_id: opaque("holding-001"),
        },
    );
    let mapping = DirectoryRoleMapping::new(
        "ERPX-Operators",
        "operations.viewer",
        ConnectorScope::Holding {
            holding_id: opaque("holding-001"),
        },
        RoleRevocationMode::AppendOnly,
    )
    .expect("strict QA role mapping must be append-only");
    let link = ImmutableIdentityLink::new("external-subject-001", opaque("principal-001"));
    DirectoryAdministrationModel::new(vec![platform, holding], vec![mapping], vec![link])
}

fn platform_model() -> PlatformOperationsModel {
    let areas = vec![
        PlatformOperationsArea::Tenants,
        PlatformOperationsArea::Projections,
        PlatformOperationsArea::Backups,
        PlatformOperationsArea::Integrations,
        PlatformOperationsArea::Audit,
        PlatformOperationsArea::Settings,
        PlatformOperationsArea::RoleReview,
    ];
    PlatformOperationsModel::from_authoritative_area_reads(
        areas.clone(),
        vec![
            ConnectorScope::Platform,
            ConnectorScope::Holding {
                holding_id: opaque("holding-001"),
            },
        ],
        areas
            .into_iter()
            .map(|area| {
                PlatformOperationsAreaEvidence::authoritative(
                    area,
                    ReconciliationEvidence::Reconciled,
                )
            })
            .collect(),
    )
    .expect("all Platform Operations areas must have real adapter evidence")
}

fn context_model(context: ProductContext) -> ContextSurfaceModel {
    ContextSurfaceModel::new(context, ProjectionFreshness::Fresh, 4, 2)
}

fn scoped_context_model(
    context: ProductContext,
    freshness: ProjectionFreshness,
    returned_count: u32,
    redacted_field_count: u32,
    reconciliation: ReconciliationEvidence,
) -> ContextSurfaceModel {
    let scope = ProductContextScope::new(opaque("tenant-001"), opaque("holding-001"));
    let page = ProductPage::new(returned_count, returned_count.max(1), None)
        .expect("QA page must satisfy product pagination contract");
    ContextSurfaceModel::from_authoritative_scoped_read_model(
        context,
        freshness,
        scope,
        page,
        redacted_field_count,
        reconciliation,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AUTH_HTTP_ORIGIN, GRPC_ENDPOINT};
    use dioxus::{
        dioxus_core::{AttributeValue, Mutation},
        prelude::*,
    };

    const CONSOLE_MAIN_SOURCE: &str = include_str!("main.rs");

    #[test]
    fn directory_contract_covers_all_approved_vendors_and_native_feeds() {
        let vendors = [
            DirectoryVendor::GenericLdap,
            DirectoryVendor::ActiveDirectoryDomainServices,
            DirectoryVendor::UniventionCorporateServer,
        ];
        let feeds = [
            NativeChangeFeed::Rfc4533,
            NativeChangeFeed::AdDirSync,
            NativeChangeFeed::UcsProvisioningService,
        ];
        assert_eq!(
            vendors.len(),
            feeds.len(),
            "every approved vendor must have exactly one native-feed fixture"
        );

        for (vendor, feed) in vendors.into_iter().zip(feeds) {
            let contract = connector(vendor, ConnectorScope::Platform);
            assert_eq!(contract.vendor(), vendor);
            assert_eq!(contract.policy().change_feed, feed);
            assert_eq!(
                contract.policy().full_import.max_users(),
                MAX_FULL_IMPORT_USERS
            );
            assert_eq!(
                contract.policy().role_mapping,
                RoleMappingMode::DirectoryManaged
            );
            assert_eq!(
                contract.policy().identity_linking,
                IdentityLinkingMode::Immutable
            );
            assert_eq!(
                contract.policy().role_revocation,
                RoleRevocationMode::AppendOnly
            );
            assert_eq!(
                contract.policy().security.revocation,
                RevocationPolicy::FiveMinutes
            );
            assert_eq!(
                contract.policy().security.local_fallback,
                LocalFallbackMode::BreakGlassOnly
            );
            assert!(contract.policy().security.tls_only);
            assert!(
                contract
                    .policy()
                    .security
                    .high_risk_mfa
                    .satisfies_high_risk_requirement()
            );
            assert_eq!(
                contract.policy().security.revocation.max_seconds(),
                FIVE_MINUTE_REVOCATION_SECONDS
            );
        }
    }

    #[test]
    fn every_approved_directory_vendor_is_tls_mfa_and_break_glass_only() {
        for vendor in [
            DirectoryVendor::GenericLdap,
            DirectoryVendor::ActiveDirectoryDomainServices,
            DirectoryVendor::UniventionCorporateServer,
        ] {
            let policy = DirectoryConnectorPolicy::strict(vendor);
            assert!(policy.security.tls_only, "TLS is required for {vendor:?}");
            assert!(
                policy
                    .security
                    .high_risk_mfa
                    .satisfies_high_risk_requirement(),
                "fresh backend MFA is required for {vendor:?}"
            );
            assert_eq!(
                policy.security.revocation.max_seconds(),
                FIVE_MINUTE_REVOCATION_SECONDS,
                "five-minute revocation is required for {vendor:?}"
            );
            assert_eq!(
                policy.security.local_fallback,
                LocalFallbackMode::BreakGlassOnly,
                "local fallback must remain break-glass-only for {vendor:?}"
            );
        }
    }

    #[test]
    fn directory_administration_has_platform_and_per_holding_scopes() {
        let model = directory_model();
        assert_eq!(model.connectors().len(), 2);
        assert!(
            model
                .connectors()
                .iter()
                .any(|connector| connector.scope().is_platform())
        );
        assert!(
            model
                .connectors()
                .iter()
                .any(|connector| matches!(connector.scope(), ConnectorScope::Holding { .. }))
        );
        assert_eq!(model.role_mappings().len(), 1);
        assert_eq!(
            model.role_mappings()[0].revocation(),
            RoleRevocationMode::AppendOnly
        );
        assert!(model.identity_links()[0].is_immutable());
    }

    #[test]
    fn directory_identity_link_page_exposes_truncation_instead_of_claiming_completeness() {
        let base = directory_model();
        let page = DirectoryAdministrationModel::with_identity_link_page(
            base.connectors().to_vec(),
            base.role_mappings().to_vec(),
            base.identity_links().to_vec(),
            true,
        );
        assert!(page.identity_links_truncated());
        assert_eq!(page.identity_links().len(), 1);
        assert!(page.identity_links()[0].is_immutable());
    }

    #[test]
    fn directory_read_model_is_complete_read_only_and_scope_aware() {
        let model = directory_model();

        assert!(model.is_read_only());
        assert!(model.is_contract_compliant());
        assert!(model.has_platform_connector());
        assert_eq!(model.holding_connector_count(), 1);
        assert_eq!(
            model.connectors()[0].policy().change_feed,
            NativeChangeFeed::Rfc4533
        );
        assert_eq!(
            model.connectors()[1].policy().change_feed,
            NativeChangeFeed::AdDirSync
        );
        assert_eq!(
            model.connectors()[1]
                .policy()
                .security
                .revocation
                .max_seconds(),
            FIVE_MINUTE_REVOCATION_SECONDS
        );
        assert!(
            model
                .connectors()
                .iter()
                .all(|connector| connector.policy().security.tls_only)
        );
        assert!(model.connectors().iter().all(|connector| {
            connector
                .policy()
                .security
                .high_risk_mfa
                .satisfies_high_risk_requirement()
        }));

        let debug = format!("{model:?}");
        assert!(!debug.contains("password"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("ldap://"));
    }

    #[test]
    fn directory_snapshot_requires_platform_connector_and_matching_mapping_scope() {
        let empty = DirectoryAdministrationModel::new(Vec::new(), Vec::new(), Vec::new());
        assert!(!empty.is_contract_compliant());

        let holding_scope = ConnectorScope::Holding {
            holding_id: opaque("holding-001"),
        };
        let holding_only = DirectoryAdministrationModel::new(
            vec![connector(
                DirectoryVendor::ActiveDirectoryDomainServices,
                holding_scope.clone(),
            )],
            Vec::new(),
            Vec::new(),
        );
        assert!(!holding_only.is_contract_compliant());

        let mismatched_mapping = DirectoryRoleMapping::new(
            "ERPX-Operators",
            "operations.viewer",
            ConnectorScope::Holding {
                holding_id: opaque("holding-002"),
            },
            RoleRevocationMode::AppendOnly,
        )
        .expect("mapping fixture must be append-only");
        let scoped = DirectoryAdministrationModel::new(
            vec![
                connector(DirectoryVendor::GenericLdap, ConnectorScope::Platform),
                connector(
                    DirectoryVendor::ActiveDirectoryDomainServices,
                    holding_scope,
                ),
            ],
            vec![mismatched_mapping],
            Vec::new(),
        );
        assert!(!scoped.is_contract_compliant());
    }

    #[test]
    fn directory_render_boundary_blocks_invalid_snapshots_and_drift() {
        let invalid =
            ProductSurfaceState::ReadOnly {
                model: ProductSurfaceModel::DirectoryAdministration(
                    DirectoryAdministrationModel::new(Vec::new(), Vec::new(), Vec::new()),
                ),
                reason: SafeDisplayText::new("untrusted directory snapshot"),
            }
            .fail_closed_for_render();
        assert_eq!(invalid.state_key(), "conflict");
        assert!(!invalid.has_model());
        assert_eq!(invalid.authorization(), AuthorizationEvidence::Blocked);
        assert_eq!(invalid.persistence(), PersistenceEvidence::Unavailable);
        assert_eq!(
            invalid.reconciliation(),
            ReconciliationEvidence::NotReported
        );

        let drifted = DirectoryAdministrationModel::from_authoritative_read_model(
            directory_model().connectors().to_vec(),
            directory_model().role_mappings().to_vec(),
            directory_model().identity_links().to_vec(),
            ReconciliationEvidence::DriftDetected,
        );
        let drifted = ProductSurfaceState::ReadOnly {
            model: ProductSurfaceModel::DirectoryAdministration(drifted),
            reason: SafeDisplayText::new("directory reconciliation returned"),
        }
        .fail_closed_for_render();
        assert_eq!(drifted.state_key(), "conflict");
        assert!(!drifted.has_model());
        assert_eq!(drifted.authorization(), AuthorizationEvidence::Blocked);
        assert!(
            drifted
                .message()
                .as_str()
                .contains("reconciliation detected drift")
        );
    }

    #[test]
    fn directory_pending_reconciliation_is_visible_but_remains_read_only() {
        let base = directory_model();
        let pending = DirectoryAdministrationModel::from_authoritative_read_model(
            base.connectors().to_vec(),
            base.role_mappings().to_vec(),
            base.identity_links().to_vec(),
            ReconciliationEvidence::Pending,
        );
        assert_eq!(
            pending.persistence(),
            PersistenceEvidence::AuthoritativeReadModel
        );
        assert_eq!(
            pending.authorization(),
            AuthorizationEvidence::ServerAuthorized
        );
        assert_eq!(pending.reconciliation(), ReconciliationEvidence::Pending);
        assert_eq!(pending.reconciliation().key(), "pending");

        let state = ProductSurfaceState::ReadOnly {
            model: ProductSurfaceModel::DirectoryAdministration(pending),
            reason: SafeDisplayText::new("directory facts are pending reconciliation"),
        }
        .fail_closed_for_render();
        assert_eq!(state.state_key(), "read-only");
        assert!(state.has_model());
        assert_eq!(state.reconciliation(), ReconciliationEvidence::Pending);
        assert!(!state.mutations_enabled());
        assert_eq!(
            DIRECTORY_ROLLBACK_BOUNDARY,
            "Server/worker-owned rollback and replay; no client action is available."
        );
    }

    #[test]
    fn safe_display_text_redacts_common_transport_and_api_credentials() {
        for value in [
            "api_key=secret",
            "api-key=secret",
            "postgres://user:password@example.invalid/db",
            "ldapi://directory.example.invalid",
            "ssh://operator@example.invalid",
        ] {
            assert!(
                SafeDisplayText::new(value).is_redacted(),
                "sensitive provider material must not render: {value}"
            );
        }
    }

    #[test]
    fn connector_rejects_insecure_or_non_authoritative_policy() {
        let cases = [
            (
                DirectoryConnectorPolicy {
                    change_feed: NativeChangeFeed::AdDirSync,
                    ..DirectoryConnectorPolicy::strict(DirectoryVendor::GenericLdap)
                },
                ProductContractError::ConnectorMustUseNativeFeed,
            ),
            (
                DirectoryConnectorPolicy {
                    role_mapping: RoleMappingMode::LocalOnly,
                    ..DirectoryConnectorPolicy::strict(DirectoryVendor::GenericLdap)
                },
                ProductContractError::DirectoryRoleMappingRequired,
            ),
            (
                DirectoryConnectorPolicy {
                    identity_linking: IdentityLinkingMode::Mutable,
                    ..DirectoryConnectorPolicy::strict(DirectoryVendor::GenericLdap)
                },
                ProductContractError::ImmutableIdentityLinkRequired,
            ),
            (
                DirectoryConnectorPolicy {
                    role_revocation: RoleRevocationMode::Replaceable,
                    ..DirectoryConnectorPolicy::strict(DirectoryVendor::GenericLdap)
                },
                ProductContractError::AppendOnlyRevocationRequired,
            ),
            (
                DirectoryConnectorPolicy {
                    security: AccessSecurityPolicy {
                        tls_only: false,
                        ..AccessSecurityPolicy::strict()
                    },
                    ..DirectoryConnectorPolicy::strict(DirectoryVendor::GenericLdap)
                },
                ProductContractError::TlsOnlyRequired,
            ),
            (
                DirectoryConnectorPolicy {
                    security: AccessSecurityPolicy {
                        revocation: RevocationPolicy::Custom { max_seconds: 301 },
                        ..AccessSecurityPolicy::strict()
                    },
                    ..DirectoryConnectorPolicy::strict(DirectoryVendor::GenericLdap)
                },
                ProductContractError::FiveMinuteRevocationRequired,
            ),
            (
                DirectoryConnectorPolicy {
                    security: AccessSecurityPolicy {
                        high_risk_mfa: HighRiskMfaPolicy::NotRequired,
                        ..AccessSecurityPolicy::strict()
                    },
                    ..DirectoryConnectorPolicy::strict(DirectoryVendor::GenericLdap)
                },
                ProductContractError::HighRiskMfaRequired,
            ),
            (
                DirectoryConnectorPolicy {
                    security: AccessSecurityPolicy {
                        local_fallback: LocalFallbackMode::GeneralLocalFallback,
                        ..AccessSecurityPolicy::strict()
                    },
                    ..DirectoryConnectorPolicy::strict(DirectoryVendor::GenericLdap)
                },
                ProductContractError::BreakGlassOnlyFallbackRequired,
            ),
        ];

        for (policy, expected_error) in cases {
            let error = DirectoryConnectorContract::new(
                opaque("connector-001"),
                DirectoryVendor::GenericLdap,
                ConnectorScope::Platform,
                policy,
            )
            .expect_err("non-compliant connector policy must fail closed");
            assert_eq!(error, expected_error);
        }
    }

    #[test]
    fn platform_operations_covers_all_safe_read_areas_without_provider_payloads() {
        let model = platform_model();
        assert_eq!(model.areas(), PlatformOperationsArea::all());
        assert!(model.has_complete_adapter_coverage());
        assert_eq!(
            model.area_evidence().len(),
            PlatformOperationsArea::all().len()
        );
        assert!(
            model
                .connector_scopes()
                .iter()
                .any(ConnectorScope::is_platform)
        );
        assert!(
            model
                .connector_scopes()
                .iter()
                .any(|scope| matches!(scope, ConnectorScope::Holding { .. }))
        );
        let debug = format!("{model:?}");
        assert!(!debug.contains("password"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("database_url"));
        assert_eq!(
            model.persistence(),
            PersistenceEvidence::AuthoritativeReadModel
        );
        assert_eq!(model.reconciliation(), ReconciliationEvidence::Reconciled);
        assert_eq!(
            model.authorization(),
            AuthorizationEvidence::ServerAuthorized
        );
    }

    #[test]
    fn platform_operations_adapter_attestations_bind_each_area_to_the_real_client_boundary() {
        let model = platform_model();
        let adapter_ids = model
            .area_evidence()
            .iter()
            .map(|evidence| evidence.adapter().id())
            .collect::<Vec<_>>();

        assert_eq!(
            adapter_ids,
            vec![
                "platform.list_tenants",
                "platform.list_projection_statuses",
                "platform.list_tenant_backup_jobs",
                "platform.list_integrations",
                "platform.list_audit",
                "platform.get_settings",
                "role.list_review",
            ]
        );
        for evidence in model.area_evidence() {
            assert_eq!(
                evidence.area().path(),
                match evidence.area() {
                    PlatformOperationsArea::Tenants => "/platform/tenants",
                    PlatformOperationsArea::Projections => "/platform/projections",
                    PlatformOperationsArea::Backups => "/platform/backups",
                    PlatformOperationsArea::Integrations => "/platform/integrations",
                    PlatformOperationsArea::Audit => "/platform/audit",
                    PlatformOperationsArea::Settings => "/platform/settings",
                    PlatformOperationsArea::RoleReview => "/platform/role-review",
                }
            );
            assert_eq!(
                evidence.persistence(),
                PersistenceEvidence::AuthoritativeReadModel
            );
            assert_eq!(
                evidence.authorization(),
                AuthorizationEvidence::ServerAuthorized
            );
        }
    }

    #[test]
    fn platform_operations_incomplete_or_mismatched_adapter_evidence_fails_closed() {
        let areas = PlatformOperationsArea::all().to_vec();
        let incomplete = PlatformOperationsModel::from_authoritative_area_reads(
            areas.clone(),
            vec![ConnectorScope::Platform],
            areas[..areas.len() - 1]
                .iter()
                .copied()
                .map(|area| {
                    PlatformOperationsAreaEvidence::authoritative(
                        area,
                        ReconciliationEvidence::Reconciled,
                    )
                })
                .collect(),
        );
        assert_eq!(
            incomplete,
            Err(ProductContractError::PlatformAdapterCoverageIncomplete)
        );

        let mut mismatched = areas
            .iter()
            .copied()
            .map(|area| {
                PlatformOperationsAreaEvidence::authoritative(
                    area,
                    ReconciliationEvidence::Reconciled,
                )
            })
            .collect::<Vec<_>>();
        mismatched.swap(0, 1);
        let result = PlatformOperationsModel::from_authoritative_area_reads(
            areas,
            vec![ConnectorScope::Platform],
            mismatched,
        );
        assert_eq!(
            result,
            Err(ProductContractError::PlatformAdapterEvidenceMismatch)
        );

        let metadata_only = PlatformOperationsModel::new(
            PlatformOperationsArea::all().to_vec(),
            vec![ConnectorScope::Platform],
        );
        assert!(!metadata_only.has_complete_adapter_coverage());
    }

    #[test]
    fn platform_operations_drift_is_not_renderable_as_authenticated_evidence() {
        let areas = PlatformOperationsArea::all().to_vec();
        let model = PlatformOperationsModel::from_authoritative_area_reads(
            areas.clone(),
            vec![ConnectorScope::Platform],
            areas
                .iter()
                .copied()
                .map(|area| {
                    PlatformOperationsAreaEvidence::authoritative(
                        area,
                        ReconciliationEvidence::DriftDetected,
                    )
                })
                .collect(),
        )
        .expect("complete drifted evidence is still a valid server response");

        assert!(model.has_complete_adapter_coverage());
        assert_eq!(
            model.reconciliation(),
            ReconciliationEvidence::DriftDetected
        );
        let state = ProductSurfaceState::ReadOnly {
            model: ProductSurfaceModel::PlatformOperations(model),
            reason: SafeDisplayText::new("Platform Operations response returned."),
        }
        .fail_closed_for_render();
        assert_eq!(state.state_key(), "conflict");
        assert!(!state.has_model());
        assert_eq!(state.authorization(), AuthorizationEvidence::Blocked);
    }

    #[test]
    fn platform_metadata_without_authoritative_evidence_fails_closed() {
        let model = PlatformOperationsModel::new(
            PlatformOperationsArea::all().to_vec(),
            vec![ConnectorScope::Platform],
        );
        assert_eq!(model.persistence(), PersistenceEvidence::Unavailable);
        assert_eq!(model.reconciliation(), ReconciliationEvidence::NotReported);
        assert_eq!(model.authorization(), AuthorizationEvidence::Unavailable);
    }

    #[test]
    fn every_product_context_has_a_typed_read_only_snapshot() {
        assert_eq!(PRODUCT_CONTEXTS.len(), 12);
        for context in PRODUCT_CONTEXTS {
            let model = ProductSurfaceModel::Context(context_model(*context));
            assert_eq!(model.surface(), ProductSurface::Context(*context));
            assert!(model.is_read_only());
            assert_eq!(
                model.product_row_evidence(),
                ProductRowEvidence::Unavailable
            );
            assert_eq!(
                model.summary(),
                "4 authoritative projection status row(s); product rows unavailable"
            );
        }
    }

    #[test]
    fn product_rows_are_holding_bound_and_page_validated_before_render() {
        let cursor = ProductPageCursor::new("cursor-001").expect("safe cursor fixture");
        let page = ProductPage::new(25, 100, Some(cursor)).expect("page fixture must validate");
        let model = ContextSurfaceModel::from_authoritative_scoped_read_model(
            ProductContext::Sales,
            ProjectionFreshness::Fresh,
            ProductContextScope::new(opaque("tenant-001"), opaque("holding-001")),
            page,
            0,
            ReconciliationEvidence::Reconciled,
        );
        assert_eq!(
            model
                .scope()
                .expect("scope must be bound")
                .tenant_id()
                .as_str(),
            "tenant-001"
        );
        assert_eq!(
            model
                .scope()
                .expect("scope must be bound")
                .holding_id()
                .as_str(),
            "holding-001"
        );
        assert_eq!(
            model
                .pagination()
                .expect("page must be bound")
                .returned_count(),
            25
        );
        assert!(
            model
                .pagination()
                .expect("page must be bound")
                .has_next_page()
        );
        assert!(!format!("{model:?}").contains("cursor-001"));

        let unscoped =
            ProductSurfaceModel::Context(ContextSurfaceModel::from_authoritative_read_model(
                ProductContext::Sales,
                ProjectionFreshness::Fresh,
                25,
                0,
                ReconciliationEvidence::Reconciled,
            ));
        assert_eq!(
            AuthoritativeProductReadModel::new(unscoped, None),
            Err(ProductContractError::ProductContextScopeRequired)
        );
    }

    #[test]
    fn projection_status_route_fails_closed_for_product_rows_but_keeps_read_evidence() {
        let model = ProductSurfaceModel::Context(context_model(ProductContext::Finance));
        let state = ProductSurfaceState::ReadOnly {
            model,
            reason: SafeDisplayText::new("Authorized projection status read model."),
        };

        assert_eq!(
            state.product_row_evidence(),
            ProductRowEvidence::Unavailable
        );
        assert_eq!(
            state.persistence(),
            PersistenceEvidence::AuthoritativeReadModel
        );
        assert_eq!(state.reconciliation(), ReconciliationEvidence::NotReported);
        assert_eq!(
            state.authorization(),
            AuthorizationEvidence::ServerAuthorized
        );
        assert_eq!(state.state_key(), "read-only");
        assert!(!state.mutations_enabled());

        let product_model = ProductSurfaceModel::Context(scoped_context_model(
            ProductContext::Finance,
            ProjectionFreshness::Fresh,
            3,
            1,
            ReconciliationEvidence::Reconciled,
        ));
        assert_eq!(
            product_model.product_row_evidence(),
            ProductRowEvidence::Authoritative
        );
        assert_eq!(
            product_model.summary(),
            "3 visible row(s), 1 redacted field(s)"
        );
    }

    #[test]
    fn operations_route_adapter_consumes_authoritative_response_states() {
        let adapter = ProductRouteStateAdapter;

        let fresh = adapter.adapt_operations_projection_status(2, "fresh", Some(120));
        assert_eq!(fresh.surface().path(), "/operations");
        assert_eq!(fresh.state_key(), "read-only");
        assert_eq!(fresh.projection_freshness(), ProjectionFreshness::Fresh);
        assert_eq!(
            fresh.persistence(),
            PersistenceEvidence::AuthoritativeReadModel
        );
        assert_eq!(
            fresh.authorization(),
            AuthorizationEvidence::ServerAuthorized
        );
        assert!(!fresh.mutations_enabled());
        assert!(fresh.model().is_some_and(|model| {
            model
                .summary()
                .contains("2 authoritative projection status")
        }));

        let stale = adapter.adapt_operations_projection_status(2, "stale", Some(301));
        assert_eq!(stale.state_key(), "stale");
        assert_eq!(stale.kind(), Some(UiStateKind::Stale));
        assert_eq!(stale.stale_age_seconds(), Some(301));
        assert!(!stale.mutations_enabled());

        let empty = adapter.adapt_operations_projection_status(0, "fresh", None);
        assert_eq!(empty.state_key(), "empty");
        assert!(!empty.has_model());
        assert_eq!(empty.persistence(), PersistenceEvidence::Unavailable);

        let contradictory = adapter.adapt_operations_projection_status(2, "fresh", Some(301));
        assert_eq!(contradictory.state_key(), "conflict");
        assert_eq!(contradictory.kind(), Some(UiStateKind::Conflict));
        assert!(!contradictory.has_model());
        assert_eq!(
            contradictory.authorization(),
            AuthorizationEvidence::Blocked
        );
    }

    #[test]
    fn route_adapter_redacts_unavailable_and_conflict_reasons_without_fabricating_rows() {
        let adapter = ProductRouteStateAdapter;
        let surface = ProductSurface::Context(ProductContext::Operations);

        let unavailable = adapter.adapt(ProductRouteResponse::unavailable(
            surface,
            "access_token=secret backend response unavailable",
        ));
        assert_eq!(unavailable.state_key(), "unavailable");
        assert_eq!(unavailable.message().as_str(), "[redacted]");
        assert!(!unavailable.has_model());
        assert_eq!(
            unavailable.product_row_evidence(),
            ProductRowEvidence::Unavailable
        );

        let conflict = adapter.adapt(ProductRouteResponse::conflict(
            surface,
            "database_url=postgres://user:secret@host/db response conflict",
        ));
        assert_eq!(conflict.state_key(), "conflict");
        assert_eq!(conflict.message().as_str(), "[redacted]");
        assert!(!conflict.has_model());
        assert_eq!(conflict.authorization(), AuthorizationEvidence::Blocked);
        assert!(!format!("{conflict:?}").contains("postgres://"));
    }

    #[test]
    fn authoritative_read_wrapper_rejects_cache_only_and_contradictory_freshness() {
        let unavailable_model =
            ProductSurfaceModel::PlatformOperations(PlatformOperationsModel::new(
                PlatformOperationsArea::all().to_vec(),
                vec![ConnectorScope::Platform],
            ));
        assert_eq!(
            AuthoritativeProductReadModel::new(unavailable_model, None),
            Err(ProductContractError::PersistedReadModelRequired)
        );

        let fresh_model = ProductSurfaceModel::Context(ContextSurfaceModel::new(
            ProductContext::Operations,
            ProjectionFreshness::Fresh,
            1,
            0,
        ));
        assert_eq!(
            AuthoritativeProductReadModel::new(fresh_model.clone(), Some(301)),
            Err(ProductContractError::FreshnessAgeConflict)
        );

        let stale_model = ProductSurfaceModel::Context(ContextSurfaceModel::new(
            ProductContext::Operations,
            ProjectionFreshness::Stale,
            1,
            0,
        ));
        assert_eq!(
            AuthoritativeProductReadModel::new(stale_model, Some(300)),
            Err(ProductContractError::StaleFreshnessAgeRequired)
        );
        assert!(AuthoritativeProductReadModel::new(fresh_model, Some(300)).is_ok());
    }

    #[test]
    fn directory_platform_and_product_surfaces_are_all_read_only_boundary_models() {
        let mut models = vec![
            ProductSurfaceModel::DirectoryAdministration(directory_model()),
            ProductSurfaceModel::PlatformOperations(platform_model()),
        ];
        models.extend(
            PRODUCT_CONTEXTS
                .iter()
                .copied()
                .map(|context| ProductSurfaceModel::Context(context_model(context))),
        );

        for model in models {
            let surface = model.surface();
            let state = ProductSurfaceState::ReadOnly {
                model,
                reason: SafeDisplayText::new("Server-authorized read model."),
            };
            assert_eq!(state.surface(), surface);
            assert_eq!(state.state_key(), "read-only");
            assert!(!state.mutations_enabled());
            assert!(state.has_model());
            assert!(state.message().as_str().contains("Server-authorized"));
            assert!(surface.path().starts_with('/'));
            assert!(!surface.path().contains("?"));
            assert!(!surface.path().contains("#"));
            assert!(!format!("{state:?}").contains("password"));
            assert!(!format!("{state:?}").contains("database_url"));
        }
    }

    #[test]
    fn directory_route_is_authorized_read_only_and_g9_g11_paths_remain_represented() {
        let directory_surface = ProductSurface::DirectoryAdministration;
        let directory_model = ProductSurfaceModel::DirectoryAdministration(directory_model());
        let directory_state = ProductSurfaceState::ReadOnly {
            model: directory_model.clone(),
            reason: SafeDisplayText::new("Server-authorized directory read model."),
        };

        assert_eq!(directory_surface.path(), "/platform/directories");
        assert_eq!(directory_surface.key(), "directory-administration");
        assert_eq!(
            DIRECTORY_READ_MODEL_AUTHORITY,
            "authoritative-server-read-model"
        );
        assert!(directory_model.is_read_only());
        assert!(directory_state.has_model());
        assert_eq!(directory_state.surface(), directory_surface);
        assert_eq!(directory_state.state_key(), "read-only");
        assert!(!directory_state.mutations_enabled());

        let expected_context_paths = [
            (ProductContext::Operations, "/operations"),
            (ProductContext::Finance, "/finance"),
            (ProductContext::Sales, "/sales"),
            (ProductContext::Procurement, "/procurement"),
            (ProductContext::Inventory, "/inventory"),
            (ProductContext::Maintenance, "/maintenance"),
            (ProductContext::Quality, "/quality"),
            (ProductContext::Support, "/support"),
            (ProductContext::MarketingCrm, "/marketing"),
            (ProductContext::HrPayroll, "/hr"),
            (ProductContext::Pos, "/pos"),
            (ProductContext::Legal, "/legal"),
        ];

        assert_eq!(PRODUCT_CONTEXTS.len(), expected_context_paths.len());
        for (context, expected_path) in expected_context_paths {
            assert!(PRODUCT_CONTEXTS.contains(&context));
            let surface = ProductSurface::Context(context);
            let model = ProductSurfaceModel::Context(context_model(context));
            let state = ProductSurfaceState::ReadOnly {
                model,
                reason: SafeDisplayText::new("Server-authorized product read model."),
            };

            assert_eq!(surface.path(), expected_path);
            assert_eq!(state.surface(), surface);
            assert!(state.has_model());
            assert!(!state.mutations_enabled());
        }
    }

    #[test]
    fn all_required_explicit_states_map_to_shared_ui_state_kinds() {
        let model = ProductSurfaceModel::Context(context_model(ProductContext::Operations));
        let surface = ProductSurface::Context(ProductContext::Operations);
        let cases = [
            (
                ProductSurfaceState::Loading { surface },
                UiStateKind::Loading,
            ),
            (
                ProductSurfaceState::Empty {
                    surface,
                    message: SafeDisplayText::new("No authorized rows."),
                },
                UiStateKind::Empty,
            ),
            (
                ProductSurfaceState::Unavailable {
                    surface,
                    reason: SafeDisplayText::new(
                        "The authoritative route response is unavailable.",
                    ),
                },
                UiStateKind::Offline,
            ),
            (
                ProductSurfaceState::Denied {
                    surface,
                    reason: SafeDisplayText::new("Denied by server policy."),
                },
                UiStateKind::Denied,
            ),
            (
                ProductSurfaceState::Stale {
                    model: model.clone(),
                    age_seconds: 301,
                    reason: SafeDisplayText::new("Projection is stale."),
                },
                UiStateKind::Stale,
            ),
            (
                ProductSurfaceState::ReadOnly {
                    model: model.clone(),
                    reason: SafeDisplayText::new("Read-only context."),
                },
                UiStateKind::ReadOnly,
            ),
            (
                ProductSurfaceState::Conflict {
                    surface,
                    reason: SafeDisplayText::new("Authoritative response conflict."),
                },
                UiStateKind::Conflict,
            ),
            (
                ProductSurfaceState::Error {
                    surface,
                    reason: SafeDisplayText::new("Categorical request failure."),
                },
                UiStateKind::Error,
            ),
            (
                ProductSurfaceState::Offline {
                    surface,
                    reason: SafeDisplayText::new("Backend unavailable."),
                },
                UiStateKind::Offline,
            ),
        ];

        for (state, expected_kind) in cases {
            assert_eq!(state.kind(), Some(expected_kind));
            assert!(is_explicit_non_ready_state(&state));
            assert!(!state.mutations_enabled());
        }
        let ready = ProductSurfaceState::Ready { model };
        assert_eq!(ready.kind(), None);
        assert!(!is_explicit_non_ready_state(&ready));
        assert!(!ready.mutations_enabled());
    }

    #[test]
    fn every_product_context_preserves_the_full_read_acceptance_matrix() {
        assert_eq!(PRODUCT_CONTEXTS.len(), 12);

        for context in PRODUCT_CONTEXTS.iter().copied() {
            let surface = ProductSurface::Context(context);
            let fresh_model = ProductSurfaceModel::Context(context_model(context));
            let stale_model = ProductSurfaceModel::Context(ContextSurfaceModel::new(
                context,
                ProjectionFreshness::Stale,
                4,
                0,
            ));
            let states = [
                (
                    ProductSurfaceState::Loading { surface },
                    UiStateKind::Loading,
                    false,
                ),
                (
                    ProductSurfaceState::Empty {
                        surface,
                        message: SafeDisplayText::new("No authorized product rows."),
                    },
                    UiStateKind::Empty,
                    false,
                ),
                (
                    ProductSurfaceState::Unavailable {
                        surface,
                        reason: SafeDisplayText::new("The authoritative response is unavailable."),
                    },
                    UiStateKind::Offline,
                    false,
                ),
                (
                    ProductSurfaceState::Denied {
                        surface,
                        reason: SafeDisplayText::new("The server denied this product read."),
                    },
                    UiStateKind::Denied,
                    false,
                ),
                (
                    ProductSurfaceState::Stale {
                        model: stale_model,
                        age_seconds: 301,
                        reason: SafeDisplayText::new("The product projection is stale."),
                    },
                    UiStateKind::Stale,
                    true,
                ),
                (
                    ProductSurfaceState::ReadOnly {
                        model: fresh_model.clone(),
                        reason: SafeDisplayText::new("The product read is server-authorized."),
                    },
                    UiStateKind::ReadOnly,
                    true,
                ),
                (
                    ProductSurfaceState::Conflict {
                        surface,
                        reason: SafeDisplayText::new("Reconciliation drift blocks this read."),
                    },
                    UiStateKind::Conflict,
                    false,
                ),
                (
                    ProductSurfaceState::Error {
                        surface,
                        reason: SafeDisplayText::new("The product read failed categorically."),
                    },
                    UiStateKind::Error,
                    false,
                ),
                (
                    ProductSurfaceState::Offline {
                        surface,
                        reason: SafeDisplayText::new("The product service is unavailable."),
                    },
                    UiStateKind::Offline,
                    false,
                ),
                (
                    ProductSurfaceState::Blocked {
                        surface,
                        reason: SafeDisplayText::new("A product read dependency is blocked."),
                    },
                    UiStateKind::Denied,
                    false,
                ),
            ];

            for (state, expected_kind, has_model) in states {
                assert_eq!(state.surface(), surface);
                assert_eq!(state.kind(), Some(expected_kind));
                assert_eq!(
                    state.accessibility_contract(),
                    expected_kind.accessibility()
                );
                assert_eq!(state.has_model(), has_model);
                assert!(!state.mutations_enabled());
                assert!(!state.message().as_str().contains("secret"));
            }

            let ready = ProductSurfaceState::Ready { model: fresh_model };
            assert_eq!(ready.surface(), surface);
            assert_eq!(ready.kind(), None);
            assert_eq!(
                ready.accessibility_contract(),
                UiStateKind::ReadOnly.accessibility()
            );
            assert!(!ready.mutations_enabled());

            for reconciliation in [
                ReconciliationEvidence::Reconciled,
                ReconciliationEvidence::Pending,
            ] {
                let model = ProductSurfaceModel::Context(scoped_context_model(
                    context,
                    ProjectionFreshness::Fresh,
                    1,
                    0,
                    reconciliation,
                ));
                let state = ProductSurfaceState::ReadOnly {
                    model,
                    reason: SafeDisplayText::new("Reconciliation evidence is server-supplied."),
                };
                assert_eq!(state.surface(), surface);
                assert_eq!(state.reconciliation(), reconciliation);
                assert_eq!(
                    state.authorization(),
                    AuthorizationEvidence::ServerAuthorized
                );
                assert_eq!(
                    state.accessibility_contract(),
                    UiStateKind::ReadOnly.accessibility()
                );
                assert!(!state.mutations_enabled());
            }

            let drifted = ProductSurfaceState::ReadOnly {
                model: ProductSurfaceModel::Context(scoped_context_model(
                    context,
                    ProjectionFreshness::Fresh,
                    1,
                    0,
                    ReconciliationEvidence::DriftDetected,
                )),
                reason: SafeDisplayText::new("Reconciliation drift was detected."),
            };
            assert_eq!(
                drifted.reconciliation(),
                ReconciliationEvidence::DriftDetected
            );
            let blocked = drifted.fail_closed_for_render();
            assert_eq!(blocked.surface(), surface);
            assert_eq!(blocked.state_key(), "conflict");
            assert_eq!(blocked.kind(), Some(UiStateKind::Conflict));
            assert_eq!(blocked.authorization(), AuthorizationEvidence::Blocked);
            assert_eq!(
                blocked.accessibility_contract(),
                UiStateKind::Conflict.accessibility()
            );
            assert!(!blocked.mutations_enabled());
        }
    }

    #[test]
    fn rendered_evidence_preserves_freshness_age_and_blocked_authorization() {
        let surface = ProductSurface::Context(ProductContext::Finance);
        let fresh_model = ProductSurfaceModel::Context(scoped_context_model(
            ProductContext::Finance,
            ProjectionFreshness::Fresh,
            12,
            1,
            ReconciliationEvidence::Reconciled,
        ));
        let fresh = ProductSurfaceState::ReadOnly {
            model: fresh_model,
            reason: SafeDisplayText::new("Fresh authorized projection."),
        };
        assert_eq!(fresh.projection_freshness(), ProjectionFreshness::Fresh);
        assert_eq!(fresh.projection_freshness().key(), "fresh");
        assert_eq!(fresh.stale_age_seconds(), None);
        assert_eq!(fresh.persistence().key(), "authoritative-read-model");
        assert_eq!(fresh.reconciliation().key(), "reconciled");
        assert_eq!(fresh.authorization().key(), "server-authorized");
        assert!(!fresh.mutations_enabled());

        let stale_model = ProductSurfaceModel::Context(scoped_context_model(
            ProductContext::Finance,
            ProjectionFreshness::Stale,
            12,
            1,
            ReconciliationEvidence::DriftDetected,
        ));
        let stale = ProductSurfaceState::Stale {
            model: stale_model,
            age_seconds: 301,
            reason: SafeDisplayText::new("Projection freshness is outside its budget."),
        };
        assert_eq!(stale.surface(), surface);
        assert_eq!(stale.projection_freshness(), ProjectionFreshness::Stale);
        assert_eq!(stale.projection_freshness().label(), "Stale projection");
        assert_eq!(stale.stale_age_seconds(), Some(301));
        assert_eq!(stale.reconciliation().key(), "drift-detected");
        assert_eq!(stale.authorization().key(), "server-authorized");
        assert!(!stale.mutations_enabled());

        let blocked = ProductSurfaceState::Blocked {
            surface,
            reason: SafeDisplayText::new("Reconciliation drift blocks this read."),
        };
        assert_eq!(blocked.state_key(), "blocked");
        assert_eq!(blocked.projection_freshness(), ProjectionFreshness::Unknown);
        assert_eq!(blocked.projection_freshness().key(), "unknown");
        assert_eq!(blocked.persistence().key(), "unavailable");
        assert_eq!(blocked.reconciliation().key(), "not-reported");
        assert_eq!(blocked.authorization().key(), "blocked");
        assert!(!blocked.has_model());
        assert!(!blocked.mutations_enabled());
    }

    #[test]
    fn state_reasons_and_return_paths_are_redacted_before_display() {
        let reason = SafeDisplayText::new("Authorization: Bearer refresh-secret");
        assert!(reason.is_redacted());
        assert_eq!(reason.as_str(), "[redacted]");

        let path = safe_return_path(
            "/platform/directories?access_token=secret&refresh_token=secret#password=secret",
        );
        assert!(!path.as_str().contains("access_token"));
        assert!(!path.as_str().contains("refresh_token"));
        assert!(!path.as_str().contains("secret"));

        let debug = format!("{reason:?} {path:?}");
        assert!(!debug.contains("refresh-secret"));
        assert!(!debug.contains("access_token=secret"));

        for hash_path in [
            "/#/platform/customers?access_token=secret",
            "/#/platform/customers?refresh_token=secret",
            "/#/platform/customers?client_secret=secret",
            "/#/platform/customers#password=secret",
        ] {
            assert_eq!(
                safe_return_path(hash_path).as_str(),
                "/",
                "credential-bearing hash return path must fail closed: {hash_path}"
            );
        }
    }

    #[test]
    fn high_risk_access_requires_fresh_mfa_and_revocation_stays_at_five_minutes() {
        let policy = AccessSecurityPolicy::strict();
        assert!(policy.high_risk_mfa.satisfies_high_risk_requirement());
        assert_eq!(
            policy.high_risk_mfa.max_age_seconds(),
            Some(FIVE_MINUTE_REVOCATION_SECONDS)
        );
        assert_eq!(
            policy.revocation.max_seconds(),
            FIVE_MINUTE_REVOCATION_SECONDS
        );
        assert!(policy.tls_only);
        assert_eq!(policy.local_fallback, LocalFallbackMode::BreakGlassOnly);

        let too_old = HighRiskMfaPolicy::Required {
            max_age_seconds: FIVE_MINUTE_REVOCATION_SECONDS + 1,
        };
        assert!(!too_old.satisfies_high_risk_requirement());
    }

    #[test]
    fn mobile_handoff_is_target_neutral_and_makes_no_certification_claim() {
        for target in [ClientTarget::Android, ClientTarget::Ios] {
            let handoff = MobileTargetHandoff::new(target, SecureSessionSupport::Unsupported);
            assert_eq!(
                handoff.status(),
                MobileTargetHandoffStatus::NativeCertificationRequired
            );
            assert_eq!(handoff.secure_session(), SecureSessionSupport::Unsupported);
            assert!(!handoff.claims_certification());
        }

        let desktop =
            MobileTargetHandoff::new(ClientTarget::Desktop, SecureSessionSupport::Unsupported);
        assert_eq!(
            desktop.status(),
            MobileTargetHandoffStatus::TargetNeutralSurfaceReady
        );
        assert!(!desktop.claims_certification());
    }

    #[test]
    fn qa_contract_registry_names_every_g12_obligation() {
        for contract in G12_QA_CONTRACTS {
            assert!(!contract.is_empty());
            assert!(contract.starts_with("G12-UI-"));
        }
        assert!(G12_QA_CONTRACTS.iter().any(|id| id.contains("DIRECTORY")));
        assert!(G12_QA_CONTRACTS.iter().any(|id| id.contains("PLATFORM")));
        assert!(G12_QA_CONTRACTS.iter().any(|id| id.contains("PRODUCT")));
        assert!(G12_QA_CONTRACTS.iter().any(|id| id.contains("REDACTION")));
        assert!(G12_QA_CONTRACTS.iter().any(|id| id.contains("MOBILE")));
    }

    #[test]
    fn g14_external_readiness_is_source_bound_and_never_claims_completion() {
        assert_eq!(G14_EXTERNAL_CERTIFICATION_CONTRACTS.len(), 3);
        assert!(
            G14_EXTERNAL_CERTIFICATION_CONTRACTS
                .iter()
                .all(|id| id.starts_with("G14-EXTERNAL-") && id.ends_with("-REQUIRED"))
        );
        assert!(!G14_EXTERNAL_READINESS.has_authoritative_evidence());
        assert!(!G14_EXTERNAL_READINESS.claims_completion());
    }

    #[test]
    fn public_limits_are_explicit_and_not_schema_only() {
        assert_eq!(
            FullImportCapability::UpToOneMillionUsers.max_users(),
            1_000_000
        );
        assert_eq!(MAX_FULL_IMPORT_USERS, 1_000_000);
        assert_eq!(FIVE_MINUTE_REVOCATION_SECONDS, 300);
        assert!(
            DirectoryConnectorPolicy::strict(DirectoryVendor::UniventionCorporateServer)
                .security
                .tls_only
        );
    }

    #[test]
    fn g14_console_deployment_inputs_are_explicit_and_non_secret() {
        for marker in [
            "const GRPC_ENDPOINT",
            "option_env!(\"ERPX_CONSOLE_GRPC_ENDPOINT\")",
            "const AUTH_HTTP_ORIGIN",
            "option_env!(\"ERPX_CONSOLE_AUTH_HTTP_ORIGIN\")",
            "Endpoint::from_static(GRPC_ENDPOINT)",
            "credentials: \"omit\"",
            "cache: \"no-store\"",
        ] {
            assert!(
                CONSOLE_MAIN_SOURCE.contains(marker),
                "G14 deployment/access marker missing from apps/console/src/main.rs: {marker}"
            );
        }

        for (name, value) in [
            ("ERPX_CONSOLE_GRPC_ENDPOINT", GRPC_ENDPOINT),
            ("ERPX_CONSOLE_AUTH_HTTP_ORIGIN", AUTH_HTTP_ORIGIN),
        ] {
            assert!(
                !value.contains('@')
                    && !value.to_ascii_lowercase().contains("password")
                    && !value.to_ascii_lowercase().contains("secret")
                    && !value.to_ascii_lowercase().contains("token"),
                "configured {name} must not contain credentials"
            );
            if value.starts_with("http://127.0.0.1:") {
                eprintln!(
                    "G14 blocker queued: {name} is using the local-development loopback default; packaged deployment must provide HTTPS"
                );
            } else {
                assert!(
                    value.starts_with("https://"),
                    "configured {name} must use HTTPS outside the local-development loopback default"
                );
            }
        }
    }

    #[test]
    fn g14_console_startup_hash_deep_links_restore_and_delegate_redaction() {
        for marker in [
            "let mut active_route = use_signal",
            "let mut deep_link_applied = use_signal",
            "window.location.pathname",
            "window.location.hash.startsWith('#/')",
            "window.location.hash.slice(1)",
            "PlatformRoute::parse",
            "active_route.set(route)",
        ] {
            assert!(
                CONSOLE_MAIN_SOURCE.contains(marker),
                "G14 deep-link startup marker missing from apps/console/src/main.rs: {marker}"
            );
        }
        let credential_bearing_hash =
            "/#/platform/customers?access_token=should-not-enter-the-route-registry";
        assert_eq!(
            erpx_ui::redact_deep_link(credential_bearing_hash),
            "/",
            "hash-style credential-bearing links must be redacted before route restoration"
        );
        assert!(
            erpx_ui::PlatformRoute::parse(credential_bearing_hash).is_err(),
            "console startup must delegate hash links to the credential-redacting route parser"
        );
    }

    #[test]
    fn g14_product_state_render_contract_is_identical_for_web_desktop_and_mobile_targets() {
        let surface = ProductSurface::Context(ProductContext::Sales);
        let states = [
            ProductSurfaceState::Loading { surface },
            ProductSurfaceState::Empty {
                surface,
                message: SafeDisplayText::new("No authorized rows were returned."),
            },
            ProductSurfaceState::Unavailable {
                surface,
                reason: SafeDisplayText::new("The authoritative read is unavailable."),
            },
            ProductSurfaceState::Denied {
                surface,
                reason: SafeDisplayText::new("The server denied this read."),
            },
            ProductSurfaceState::Conflict {
                surface,
                reason: SafeDisplayText::new("The authoritative response needs reconciliation."),
            },
            ProductSurfaceState::Error {
                surface,
                reason: SafeDisplayText::new("The authoritative read failed."),
            },
            ProductSurfaceState::Offline {
                surface,
                reason: SafeDisplayText::new("The authoritative backend is offline."),
            },
            ProductSurfaceState::Ready {
                model: ProductSurfaceModel::Context(context_model(ProductContext::Sales)),
            },
            ProductSurfaceState::Stale {
                model: ProductSurfaceModel::Context(ContextSurfaceModel::new(
                    ProductContext::Sales,
                    ProjectionFreshness::Stale,
                    1,
                    0,
                )),
                age_seconds: MAX_PROJECTION_FRESHNESS_SECONDS + 1,
                reason: SafeDisplayText::new("The projection is stale and read-only."),
            },
        ];

        for target in [
            ClientTarget::Web,
            ClientTarget::Desktop,
            ClientTarget::Android,
            ClientTarget::Ios,
        ] {
            for state in states.clone() {
                let state_key = state.clone().fail_closed_for_render().state_key();
                let mut dom = VirtualDom::new_with_props(
                    super::super::product_completion::ProductCompletionSurface,
                    super::super::product_completion::ProductCompletionSurfaceProps { state },
                );
                let edits = dom.rebuild_to_vec().edits;
                let attr = |name: &str| {
                    edits.iter().find_map(|mutation| match mutation {
                        Mutation::SetAttribute {
                            name: attribute,
                            value: AttributeValue::Text(value),
                            ..
                        } if *attribute == name => Some(value.clone()),
                        _ => None,
                    })
                };

                assert_eq!(attr("data-state").as_deref(), Some(state_key));
                assert_eq!(attr("data-mutations").as_deref(), Some("disabled"));
                let title_id = attr("aria-labelledby").expect("surface needs an accessible name");
                let description_id = attr("aria-describedby").expect("surface needs a description");
                assert!(edits.iter().any(|mutation| matches!(
                    mutation,
                    Mutation::SetAttribute { name: "id", value: AttributeValue::Text(value), .. }
                        if value == &title_id || value == &description_id
                )), "{target:?} state {state_key} references missing accessible text targets");
                assert!(!format!("{edits:?}").contains("access_token"));
                assert!(!format!("{edits:?}").contains("password"));
            }
        }
    }
}
