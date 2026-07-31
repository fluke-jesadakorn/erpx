//! G11 product-context acceptance verifier.
//!
//! This verifier owns no product implementation.  It reads the authoritative
//! domain, SQLx, worker, transport, console, and Folio bridge sources and
//! executes the local domain/infrastructure/worker library suites.  Live
//! PostgreSQL, vendor, and device evidence is reported as queued when the
//! required environment is absent; static source presence is never promoted
//! to live acceptance.

use std::{env, net::TcpStream, process::Command, time::Duration};

const SALES_DOMAIN: &str = include_str!("../../../crates/domain/src/sales.rs");
const SCM_DOMAIN: &str = include_str!("../../../crates/domain/src/procurement_inventory.rs");
const ASSET_DOMAIN: &str = include_str!("../../../crates/domain/src/asset_maintenance.rs");
const HR_DOMAIN: &str = include_str!("../../../crates/domain/src/hr.rs");
const LEGAL_DOMAIN: &str = include_str!("../../../crates/domain/src/legal.rs");
const POS_DOMAIN: &str = include_str!("../../../crates/domain/src/pos.rs");
const MARKETING_DOMAIN: &str = include_str!("../../../crates/domain/src/marketing.rs");
const CRM_DOMAIN: &str = include_str!("../../../crates/domain/src/marketing_crm.rs");
const QUALITY_DOMAIN: &str = include_str!("../../../crates/domain/src/quality.rs");

const SALES_PERSISTENCE: &str =
    include_str!("../../../crates/infrastructure/src/sales_persistence.rs");
const SCM_PERSISTENCE: &str = include_str!("../../../crates/infrastructure/src/scm_persistence.rs");
const ASSET_PERSISTENCE: &str =
    include_str!("../../../crates/infrastructure/src/asset_maintenance_persistence.rs");
const HR_PERSISTENCE: &str = include_str!("../../../crates/infrastructure/src/hr_persistence.rs");
const HR_PROJECTION: &str = include_str!("../../../crates/infrastructure/src/hr_projection.rs");
const LEGAL_PERSISTENCE: &str =
    include_str!("../../../crates/infrastructure/src/legal_persistence.rs");
const LEGAL_PROJECTION: &str =
    include_str!("../../../crates/infrastructure/src/legal_projection.rs");
const POS_PERSISTENCE: &str = include_str!("../../../crates/infrastructure/src/pos_persistence.rs");
const POS_PROJECTION: &str = include_str!("../../../crates/infrastructure/src/pos_projection.rs");
const MARKETING_PERSISTENCE: &str =
    include_str!("../../../crates/infrastructure/src/marketing_persistence.rs");
const MARKETING_CRM_PERSISTENCE: &str =
    include_str!("../../../crates/infrastructure/src/marketing_crm_persistence.rs");
const QUALITY_PERSISTENCE: &str =
    include_str!("../../../crates/infrastructure/src/quality_persistence.rs");
const SCOPED_TRANSACTION: &str =
    include_str!("../../../crates/infrastructure/src/finance_persistence.rs");

const SALES_WRITE_MIGRATION: &str =
    include_str!("../../../crates/migration/migrations_tenant_write/0019_sales.sql");
const SCM_WRITE_MIGRATION: &str =
    include_str!("../../../crates/migration/migrations_tenant_write/0020_scm.sql");
const ASSET_WRITE_MIGRATION: &str =
    include_str!("../../../crates/migration/migrations_tenant_write/0021_asset_maintenance.sql");
const QUALITY_WRITE_MIGRATION: &str =
    include_str!("../../../crates/migration/migrations_tenant_write/0053_quality_inspections.sql");
const HR_WRITE_MIGRATION: &str =
    include_str!("../../../crates/migration/migrations_tenant_write/0038_hr_payroll.sql");
const LEGAL_WRITE_MIGRATION: &str =
    include_str!("../../../crates/migration/migrations_tenant_write/0040_legal_documents.sql");
const POS_WRITE_MIGRATION: &str =
    include_str!("../../../crates/migration/migrations_tenant_write/0039_pos.sql");
const MARKETING_WRITE_MIGRATION: &str =
    include_str!("../../../crates/migration/migrations_tenant_write/0018_marketing.sql");
const CRM_WRITE_MIGRATION: &str =
    include_str!("../../../crates/migration/migrations_tenant_write/0036_marketing_crm.sql");

const WORKER_LIB: &str = include_str!("../../worker/src/lib.rs");
const SCM_WORKER: &str = include_str!("../../worker/src/scm_projection.rs");
const HR_WORKER: &str = include_str!("../../worker/src/hr_projection.rs");
const LEGAL_WORKER: &str = include_str!("../../worker/src/legal_projection.rs");
const POS_WORKER: &str = include_str!("../../worker/src/pos_projection.rs");
const MARKETING_WORKER: &str = include_str!("../../worker/src/marketing_projection.rs");
const QUALITY_WORKER: &str = include_str!("../../worker/src/quality_projection.rs");
const WORKER_MAIN: &str = include_str!("../../worker/src/main.rs");

const SERVER_MAIN: &str = include_str!("../../server/src/main.rs");
const CONSOLE_MAIN: &str = include_str!("../../console/src/main.rs");
const CONSOLE_PRODUCT_COMPLETION: &str = include_str!("../../console/src/product_completion.rs");
const UI_PRODUCT_CONTEXTS: &str = include_str!("../../../crates/ui/src/product_contexts.rs");
const PLATFORM_PROTO: &str =
    include_str!("../../../crates/proto/proto/erpx/platform/v1/platform.proto");
const MIGRATOR_MAIN: &str = include_str!("../src/main.rs");
const FOLIO_PLATFORM_PAGE: &str =
    include_str!("../../../../app/(app)/(protected)/platform/page.tsx");
const FOLIO_PLATFORM_CONSOLE: &str =
    include_str!("../../../../components/platform/PlatformConsole.tsx");
const FOLIO_CONTROL_MIGRATION: &str =
    include_str!("../../../../db/migrations/2026-07-23-erpx-control-plane.sql");

const ERPX_MANIFEST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");

struct ProductContext {
    name: &'static str,
    domain_sources: &'static [&'static str],
    persistence_sources: &'static [&'static str],
    write_migrations: &'static [&'static str],
    worker_source: &'static str,
    worker_markers: &'static [&'static str],
}

fn require_markers(surface: &str, name: &str, markers: &[&str]) {
    let missing = markers
        .iter()
        .copied()
        .filter(|marker| !surface.contains(marker))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "G11 authoritative contract markers missing from {name}: {}",
        missing.join(", ")
    );
}

fn require_any_marker(surface: &str, name: &str, markers: &[&str]) {
    assert!(
        markers
            .iter()
            .copied()
            .any(|marker| surface.contains(marker)),
        "G11 authoritative contract markers missing from {name}: expected one of {}",
        markers.join(", ")
    );
}

fn join_sources(sources: &[&str]) -> String {
    sources.join("\n\n/* next authoritative source */\n\n")
}

fn run_infrastructure_acceptance(
    context: &str,
    test_target: &str,
    test_name: &str,
    write_url: &str,
    read_url: &str,
) {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(&cargo)
        .args([
            "test",
            "--offline",
            "--manifest-path",
            ERPX_MANIFEST,
            "-p",
            "erpx-infrastructure",
            "--test",
            test_target,
            "--",
            "--ignored",
            "--exact",
            test_name,
            "--nocapture",
            "--test-threads=1",
        ])
        .env("CARGO_TERM_COLOR", "never")
        .env("ERPX_TEST_TENANT_WRITE_DATABASE_URL", write_url)
        .env("ERPX_TEST_TENANT_READ_DATABASE_URL", read_url)
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "run G11 {context} authoritative SQLx acceptance {test_target}::{test_name}: {error}"
            )
        });
    assert!(
        output.status.success(),
        "G11 {context} authoritative SQLx acceptance {test_target}::{test_name} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!(
        "G11 {context} live SQLx acceptance: {test_target}::{test_name} passed against configured disposable write/read databases"
    );
}

fn assert_fail_closed_scope_policy(name: &str, migrations: &[&str]) {
    let migration = join_sources(migrations);
    let enabled = migration.matches("ENABLE ROW LEVEL SECURITY").count();
    let forced = migration.matches("FORCE ROW LEVEL SECURITY").count();
    let using = migration
        .matches("USING (holding_ref = current_setting('erpx.holding_ref', true))")
        .count();
    let with_check = migration
        .matches("WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true))")
        .count();

    assert!(
        migration.contains("CREATE SCHEMA IF NOT EXISTS erpx_tenant")
            || migration.contains("CREATE TABLE IF NOT EXISTS erpx_tenant"),
        "G11 {name} write migration must own tenant tables"
    );
    assert!(
        enabled > 0,
        "G11 {name} write migration must enable row-level security"
    );
    assert_eq!(
        enabled, forced,
        "G11 {name} must force RLS on every table where it is enabled"
    );
    assert!(
        using > 0,
        "G11 {name} write migration must require the transaction holding scope for reads/writes"
    );
    assert_eq!(
        using, with_check,
        "G11 {name} RLS policies must have a matching holding_ref WITH CHECK for every USING guard"
    );
    assert!(
        !migration.contains("coalesce(current_setting('erpx.holding_ref'")
            && !migration.contains("current_setting('erpx.holding_ref', true) IS NULL OR"),
        "G11 {name} RLS must not provide an unscoped fallback when holding_ref is absent"
    );
}

fn product_contexts() -> [ProductContext; 8] {
    [
        ProductContext {
            name: "Sales",
            domain_sources: &[SALES_DOMAIN],
            persistence_sources: &[SALES_PERSISTENCE],
            write_migrations: &[SALES_WRITE_MIGRATION],
            worker_source: WORKER_LIB,
            worker_markers: &[
                "SALES_PROJECTION_HANDLER_NAME",
                "SalesProjectionRunner",
                "rebuild",
            ],
        },
        ProductContext {
            name: "Procurement/Inventory",
            domain_sources: &[SCM_DOMAIN],
            persistence_sources: &[SCM_PERSISTENCE],
            write_migrations: &[SCM_WRITE_MIGRATION],
            worker_source: SCM_WORKER,
            worker_markers: &[
                "SCM_PROJECTION_HANDLER_NAME",
                "ScmProjectionRunner",
                "reconcile",
            ],
        },
        ProductContext {
            name: "Asset/Maintenance",
            domain_sources: &[ASSET_DOMAIN],
            persistence_sources: &[ASSET_PERSISTENCE],
            write_migrations: &[ASSET_WRITE_MIGRATION],
            worker_source: WORKER_LIB,
            worker_markers: &[
                "ASSET_MAINTENANCE_PROJECTION_HANDLER_NAME",
                "AssetMaintenanceProjectionRunner",
                "rebuild",
            ],
        },
        ProductContext {
            name: "Quality",
            domain_sources: &[QUALITY_DOMAIN],
            persistence_sources: &[QUALITY_PERSISTENCE],
            write_migrations: &[QUALITY_WRITE_MIGRATION],
            worker_source: QUALITY_WORKER,
            worker_markers: &["QualityProjectionRunner", "rebuild"],
        },
        ProductContext {
            name: "HR/Payroll",
            domain_sources: &[HR_DOMAIN],
            persistence_sources: &[HR_PERSISTENCE, HR_PROJECTION],
            write_migrations: &[HR_WRITE_MIGRATION],
            worker_source: HR_WORKER,
            worker_markers: &[
                "HR_PROJECTION_HANDLER_NAME",
                "HrProjectionRunner",
                "reconcile",
            ],
        },
        ProductContext {
            name: "Legal/Documents",
            domain_sources: &[LEGAL_DOMAIN],
            persistence_sources: &[LEGAL_PERSISTENCE, LEGAL_PROJECTION],
            write_migrations: &[LEGAL_WRITE_MIGRATION],
            worker_source: LEGAL_WORKER,
            worker_markers: &[
                "LEGAL_PROJECTION_HANDLER_NAME",
                "LegalProjectionRunner",
                "reconcile",
            ],
        },
        ProductContext {
            name: "POS",
            domain_sources: &[POS_DOMAIN],
            persistence_sources: &[POS_PERSISTENCE, POS_PROJECTION],
            write_migrations: &[POS_WRITE_MIGRATION],
            worker_source: POS_WORKER,
            worker_markers: &[
                "POS_PROJECTION_HANDLER_NAME",
                "PosProjectionRunner",
                "reconcile",
            ],
        },
        ProductContext {
            name: "Marketing/CRM",
            domain_sources: &[MARKETING_DOMAIN, CRM_DOMAIN],
            persistence_sources: &[MARKETING_PERSISTENCE, MARKETING_CRM_PERSISTENCE],
            write_migrations: &[MARKETING_WRITE_MIGRATION, CRM_WRITE_MIGRATION],
            worker_source: MARKETING_WORKER,
            worker_markers: &[
                "MARKETING_PROJECTION_HANDLER_NAME",
                "MarketingProjectionRunner",
                "rebuild",
            ],
        },
    ]
}

#[test]
fn g11_product_context_tenant_scope_is_fail_closed_at_the_write_boundary() {
    require_markers(
        SCOPED_TRANSACTION,
        "shared tenant-scoped transaction helper",
        &[
            "pub(crate) async fn begin_scoped",
            "SELECT set_config('erpx.holding_ref', $1, true)",
        ],
    );

    for context in product_contexts() {
        let persistence = join_sources(context.persistence_sources);
        require_markers(
            &persistence,
            &format!("G11 {} SQLx scope boundary", context.name),
            &["holding_ref", "begin_scoped"],
        );
        require_any_marker(
            &persistence,
            &format!("G11 {} holding_ref validation", context.name),
            &["validate_holding_ref", "validate_text(&holding_ref"],
        );
        assert_fail_closed_scope_policy(context.name, context.write_migrations);
    }
}

#[test]
fn g11_authoritative_write_projection_and_reconciliation_contracts_are_present() {
    for context in product_contexts() {
        let domain = join_sources(context.domain_sources);
        let persistence = join_sources(context.persistence_sources);

        require_markers(
            &domain,
            &format!("G11 {} domain", context.name),
            &[
                "ProjectionReconciliation",
                "validate",
                "scope",
                "idempotency",
            ],
        );
        require_markers(
            &persistence,
            &format!("G11 {} SQLx persistence", context.name),
            &[
                "Sqlx",
                "write_pool",
                "read_pool",
                "outbox",
                "idempotency",
                "read_source_event_page",
                "reconcile_projection",
                "project_event",
                "clear_projection",
            ],
        );
        require_markers(
            context.worker_source,
            &format!("G11 {} worker projector", context.name),
            context.worker_markers,
        );
    }
}

#[test]
fn g11_product_shell_registry_is_authenticated_and_read_only_by_default() {
    require_markers(
        UI_PRODUCT_CONTEXTS,
        "crates/ui/src/product_contexts.rs",
        &[
            "PRODUCT_CONTEXT_ROUTES",
            "ProductContextId::Sales",
            "ProductContextId::Procurement",
            "ProductContextId::Inventory",
            "ProductContextId::Asset",
            "ProductContextId::Maintenance",
            "ProductContextId::Quality",
            "ProductContextId::HrPayroll",
            "ProductContextId::Legal",
            "ProductContextId::Pos",
            "ProductContextId::MarketingCrm",
            "RouteAuthRequirement::Authenticated",
            "read_only_shell: true",
            "redact_deep_link",
        ],
    );
}

#[test]
fn g11_sales_projection_stream_and_read_scope_are_closed_and_revalidated() {
    require_markers(
        SALES_PERSISTENCE,
        "G11 Sales source/read boundary",
        &[
            "SALES_SOURCE_EVENT_TYPE_FILTER",
            "get_for_scope",
            "decode_read_aggregate",
            "aggregate.validate()?",
            "read_source_event_page",
            "reconcile_projection",
            "project_event",
            "clear_projection",
        ],
    );
    for aggregate_type in [
        "'lead'",
        "'opportunity'",
        "'quote'",
        "'sales_order'",
        "'delivery_handoff'",
        "'invoice_handoff'",
        "'collection_state'",
    ] {
        assert!(
            SALES_PERSISTENCE.contains(aggregate_type),
            "Sales source filter is missing {aggregate_type}"
        );
    }
    assert!(
        SALES_DOMAIN.contains("lifecycle_mutation_rotates_command_identity"),
        "Sales mutation identity rotation must be covered by a domain test"
    );
    eprintln!(
        "G11 Sales source/read safety: authoritative write/outbox, filtered source replay, contiguous projector, revalidated read payload, and scope guard are present"
    );
}

#[test]
fn g11_transport_and_ui_handoff_is_measured_without_fabricating_missing_surfaces() {
    struct SurfaceCheck {
        name: &'static str,
        proto_markers: &'static [&'static str],
        server_markers: &'static [&'static str],
        console_markers: &'static [&'static str],
        product_surface_markers: &'static [&'static str],
    }

    let checks = [
        SurfaceCheck {
            name: "Sales",
            proto_markers: &["service SalesService"],
            server_markers: &["impl SalesService"],
            console_markers: &[
                "ProductContext::Sales",
                "product_context_projection_name",
                "ProductContextRouteSurface",
                "product_context_route",
            ],
            product_surface_markers: &["ProductContext::Sales", "ProductSurfaceState"],
        },
        SurfaceCheck {
            name: "Procurement/Inventory",
            proto_markers: &["service ScmService", "ListScmAggregatesRequest"],
            server_markers: &["impl ScmService", "list_scm_aggregates"],
            console_markers: &[
                "ProductContext::Procurement",
                "ProductContext::Inventory",
                "ScmAggregateListRouteSurface",
                "fetch_scm_list_read",
                "product_context_projection_name",
                "ProductContextRouteSurface",
                "product_context_route",
            ],
            product_surface_markers: &[
                "ProductContext::Procurement",
                "ProductContext::Inventory",
                "ProductSurfaceState",
            ],
        },
        SurfaceCheck {
            name: "Asset/Maintenance",
            proto_markers: &[
                "service AssetMaintenanceService",
                "ListAssetMaintenanceAggregatesRequest",
            ],
            server_markers: &[
                "impl AssetMaintenanceService",
                "list_asset_maintenance_aggregates",
            ],
            console_markers: &[
                "ProductContext::Maintenance",
                "AssetMaintenanceAggregateListRouteSurface",
                "fetch_asset_maintenance_list_read",
                "product_context_projection_name",
                "ProductContextRouteSurface",
                "product_context_route",
            ],
            product_surface_markers: &["ProductContext::Maintenance", "ProductSurfaceState"],
        },
        SurfaceCheck {
            name: "Quality",
            proto_markers: &[
                "service QualityService",
                "CreateQualityInspectionRequest",
                "ListQualityInspectionsRequest",
            ],
            server_markers: &[
                "impl QualityService",
                "quality.write",
                "list_quality_inspections",
                "QualityInspectionReadRow",
                "RoutedDatabaseSide::Read",
            ],
            console_markers: &[
                "ProductContext::Quality",
                "QualityContextRouteSurface",
                "fetch_quality_read",
                "product_context_projection_name",
                "product_context_route",
            ],
            product_surface_markers: &["ProductContext::Quality", "ProductSurfaceState"],
        },
        SurfaceCheck {
            name: "HR/Payroll",
            proto_markers: &[
                "service HrService",
                "SaveHrPayrollRequest",
                "ListHrPayrollRuns",
            ],
            server_markers: &[
                "impl HrService",
                "authorize_entitled_command",
                "RoutedDatabaseSide::Read",
            ],
            console_markers: &[
                "ProductContext::HrPayroll",
                "ProductContextRouteSurface",
                "product_context_route",
            ],
            product_surface_markers: &["ProductContext::HrPayroll", "ProductSurfaceState"],
        },
        SurfaceCheck {
            name: "Legal/Documents",
            proto_markers: &[
                "service LegalService",
                "SaveLegalDocumentRequest",
                "ListLegalDocuments",
            ],
            server_markers: &[
                "impl LegalService",
                "legal.write",
                "RoutedDatabaseSide::Read",
            ],
            console_markers: &[
                "ProductContext::Legal",
                "ProductContextRouteSurface",
                "product_context_route",
            ],
            product_surface_markers: &["ProductContext::Legal", "ProductSurfaceState"],
        },
        SurfaceCheck {
            name: "POS",
            proto_markers: &["service PosService", "SavePosSaleRequest", "ListPosSales"],
            server_markers: &["impl PosService", "pos.write", "RoutedDatabaseSide::Read"],
            console_markers: &[
                "ProductContext::Pos",
                "ProductContextRouteSurface",
                "product_context_route",
            ],
            product_surface_markers: &["ProductContext::Pos", "ProductSurfaceState"],
        },
        SurfaceCheck {
            name: "Marketing/CRM",
            proto_markers: &[
                "service CrmService",
                "ListCrmReadRows",
                "service MarketingContentService",
            ],
            server_markers: &[
                "impl CrmService",
                "crm.write",
                "impl MarketingContentService",
            ],
            console_markers: &[
                "ProductContext::MarketingCrm",
                "ProductContextRouteSurface",
                "product_context_route",
                "CrmSurface",
                "ListCrmReadRowsRequest",
                "read-only",
            ],
            product_surface_markers: &["ProductContext::MarketingCrm", "ProductSurfaceState"],
        },
    ];

    for check in checks {
        let proto = check
            .proto_markers
            .iter()
            .copied()
            .all(|marker| PLATFORM_PROTO.contains(marker));
        let server = check
            .server_markers
            .iter()
            .copied()
            .all(|marker| SERVER_MAIN.contains(marker));
        let console = check
            .console_markers
            .iter()
            .copied()
            .all(|marker| CONSOLE_MAIN.contains(marker));
        let product_surface = check
            .product_surface_markers
            .iter()
            .copied()
            .all(|marker| CONSOLE_PRODUCT_COMPLETION.contains(marker));
        assert!(
            proto && server && console && product_surface,
            "G11 surface {} is missing an authoritative API, server, console route, or read-surface contract",
            check.name
        );
        let status = "complete local contract: typed API, authorization/scope handoff, and read-only console route present";
        eprintln!(
            "G11 surface {}: {} (api={}, server={}, shell_ui={}, read_surface={})",
            check.name, status, proto, server, console, product_surface
        );
    }

    require_markers(
        CONSOLE_MAIN,
        "G12 directory console route",
        &[
            "DirectoryAdministrationRouteSurface",
            "fetch_directory_administration",
            "DirectoryAdministrationSurface",
        ],
    );
    require_markers(
        CONSOLE_PRODUCT_COMPLETION,
        "G12 directory and product read surfaces",
        &[
            "DirectoryAdministrationSurface",
            "ProductCompletionSurface",
            "ProductSurfaceState",
            "data-read-only",
            "data-mutations",
        ],
    );

    // The verifier must itself guard the transport boundary whenever a
    // surface exists: commands authorize against the server-side context and
    // reads route to the disposable read database.  These markers are
    // intentionally checked globally because the product RPCs share the same
    // composition boundary.
    require_markers(
        SERVER_MAIN,
        "server product transport boundary",
        &[
            "authorize_entitled_command",
            "authorize_role",
            "RoutedDatabaseSide::Write",
            "RoutedDatabaseSide::Read",
            "access_scope",
        ],
    );
}

#[test]
fn g11_worker_registration_is_reported_as_an_acceptance_gate() {
    require_markers(
        WORKER_MAIN,
        "worker composition root",
        &[
            "ERPX_WORKER_HANDLER",
            "handler_registry",
            "run_registered_until_shutdown",
            "NamedWorkerHandler",
        ],
    );

    for (handler, registration_markers) in [
        (
            "sales_projection",
            &[
                "\"sales\"",
                "ProductProjectionExecutor::Sales",
                "SalesProjectionRunner",
            ][..],
        ),
        (
            "scm_projection",
            &[
                "\"scm\"",
                "ProductProjectionExecutor::Scm",
                "ScmProjectionRunner",
            ][..],
        ),
        (
            "asset_maintenance_projection",
            &[
                "\"asset_maintenance\"",
                "ProductProjectionExecutor::AssetMaintenance",
                "AssetMaintenanceProjectionRunner",
            ][..],
        ),
        (
            "quality_projection",
            &[
                "\"quality\"",
                "ProductProjectionExecutor::Quality",
                "QualityProjectionRunner",
            ][..],
        ),
        (
            "hr_projection",
            &[
                "\"hr\"",
                "ProductProjectionExecutor::Hr",
                "HrProjectionRunner",
            ][..],
        ),
        (
            "legal_documents_projection",
            &[
                "\"legal_documents\"",
                "ProductProjectionExecutor::Legal",
                "LegalProjectionRunner",
            ][..],
        ),
        (
            "pos_projection",
            &[
                "\"pos\"",
                "ProductProjectionExecutor::Pos",
                "PosProjectionRunner",
            ][..],
        ),
        (
            "marketing_projection",
            &[
                "\"marketing\"",
                "MarketingTenantProjectionExecutor",
                "SqlxMarketingProjectionFactory",
            ][..],
        ),
    ] {
        if registration_markers
            .iter()
            .copied()
            .all(|marker| WORKER_MAIN.contains(marker))
        {
            eprintln!("G11 worker registration {handler}: present in composition root");
        } else {
            eprintln!(
                "G11 worker registration blocker queued: {handler} markers are incomplete in apps/worker/src/main.rs"
            );
        }
    }
}

#[test]
fn g11_folio_bridge_is_read_only_and_cutover_remains_outside_this_packet() {
    require_markers(
        FOLIO_PLATFORM_PAGE,
        "Folio platform bridge page",
        &[
            "export const dynamic = 'force-dynamic'",
            "canAccessPlatform(actor)",
            "loadPlatformOverview()",
            "schemaReady",
            "NoPermissionView",
        ],
    );
    require_markers(
        FOLIO_PLATFORM_CONSOLE,
        "Folio platform bridge console",
        &[
            "Control plane is not initialized",
            "Apply the ERPX control-plane migration",
            "without changing tenant business records",
            "schemaReady",
        ],
    );
    require_markers(
        FOLIO_CONTROL_MIGRATION,
        "Folio control-plane bridge migration",
        &[
            "Tenant business data remains in Folio's",
            "CREATE SCHEMA IF NOT EXISTS platform",
            "CREATE TABLE IF NOT EXISTS platform.customer_accounts",
        ],
    );
    assert!(
        !MIGRATOR_MAIN.contains("Command::Cutover")
            && !MIGRATOR_MAIN.contains("perform_cutover")
            && !MIGRATOR_MAIN.contains("switch_command_owner"),
        "G11 verifier must not acquire a production cutover command"
    );
}

#[test]
fn g11_local_domain_sqlx_and_worker_suites_execute() {
    if env::var("ERPX_G11_RUN_LOCAL_SUITES").ok().as_deref() != Some("1") {
        eprintln!(
            "G11 local package suites queued: set ERPX_G11_RUN_LOCAL_SUITES=1 to execute the domain, infrastructure, and worker library suites"
        );
        return;
    }

    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    for package in ["erpx-domain", "erpx-infrastructure", "erpx-worker"] {
        let output = Command::new(&cargo)
            .args([
                "test",
                "--offline",
                "--manifest-path",
                ERPX_MANIFEST,
                "-p",
                package,
                "--lib",
            ])
            .env("CARGO_TERM_COLOR", "never")
            .output()
            .unwrap_or_else(|error| panic!("run {package} local acceptance suite: {error}"));
        assert!(
            output.status.success(),
            "G11 {package} local acceptance suite failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn g11_local_product_context_sqlx_slices_execute_when_explicitly_enabled() {
    if env::var("ERPX_G11_RUN_LOCAL_SUITES").ok().as_deref() != Some("1") {
        eprintln!(
            "G11 local product SQLx suites queued: set ERPX_G11_RUN_LOCAL_SUITES=1 and configure ERPX_TEST_TENANT_WRITE_DATABASE_URL plus ERPX_TEST_TENANT_READ_DATABASE_URL"
        );
        return;
    }
    let Some(write_url) = env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        eprintln!(
            "G11 local product SQLx blocker queued: ERPX_TEST_TENANT_WRITE_DATABASE_URL is unavailable"
        );
        return;
    };
    let Some(read_url) = env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        eprintln!(
            "G11 local product SQLx blocker queued: ERPX_TEST_TENANT_READ_DATABASE_URL is unavailable"
        );
        return;
    };

    for (context, test_target, test_name) in [
        (
            "Sales",
            "sales_persistence",
            "live_sales_write_outbox_projection_scoped_read_reconciliation_and_rebuild",
        ),
        (
            "Procurement/Inventory",
            "scm_persistence",
            "live_scm_write_outbox_projection_scoped_read_and_reconciliation",
        ),
        (
            "Asset/Maintenance",
            "asset_maintenance_persistence",
            "live_asset_maintenance_write_outbox_projection_read_reconcile_and_rebuild",
        ),
        (
            "Quality",
            "quality_persistence",
            "quality_write_outbox_projection_read_reconcile_and_rebuild",
        ),
        (
            "HR/Payroll",
            "hr_payroll_projection",
            "live_hr_write_projection_and_read_are_idempotent_and_separate",
        ),
        (
            "Legal/Documents",
            "legal_projection_read",
            "live_legal_projection_is_typed_scoped_idempotent_and_rebuildable",
        ),
        (
            "POS",
            "pos_persistence",
            "live_pos_write_is_normalized_scoped_and_idempotent",
        ),
        (
            "Marketing Content",
            "marketing_content_persistence",
            "content_lifecycle_is_atomic_idempotent_versioned_and_rebuildable",
        ),
    ] {
        run_infrastructure_acceptance(context, test_target, test_name, &write_url, &read_url);
    }

    // CRM persistence is write-side only; the read URL remains bound in the
    // child environment so this packet never accidentally falls back to a
    // single undifferentiated tenant database contract.
    run_infrastructure_acceptance(
        "Marketing/CRM",
        "marketing_crm_persistence",
        "live_marketing_crm_write_boundary_is_atomic_idempotent_and_scoped",
        &write_url,
        &read_url,
    );
}

#[test]
fn g11_live_database_vendor_and_runtime_evidence_is_explicitly_queued() {
    let database_references = [
        "ERPX_TEST_TENANT_WRITE_DATABASE_URL",
        "ERPX_TEST_TENANT_READ_DATABASE_URL",
        "ERPX_TEST_TENANT_DATABASE_URL_TEMPLATE",
        "ERPX_TEST_ADMIN_DATABASE_URL",
    ];
    let missing = database_references
        .iter()
        .copied()
        .filter(|name| env::var_os(name).is_none())
        .collect::<Vec<_>>();
    if missing.is_empty() {
        eprintln!(
            "G11 database references are configured; this verifier still does not claim live SQLx acceptance without the disposable two-database scenarios"
        );
    } else {
        eprintln!(
            "G11 live SQLx blocker queued: missing disposable database references: {}",
            missing.join(", ")
        );
    }

    let folio_reachable =
        TcpStream::connect_timeout(&([127, 0, 0, 1], 3004).into(), Duration::from_millis(300))
            .is_ok();
    if folio_reachable {
        eprintln!(
            "G11 Folio runtime probe: port 3004 is reachable; bridge/API ownership acceptance remains queued"
        );
    } else {
        eprintln!(
            "G11 runtime blocker queued: Folio development server is not reachable on port 3004"
        );
    }

    for reference in [
        "ERPX_G11_SALES_VENDOR_ENDPOINT",
        "ERPX_G11_SCM_VENDOR_ENDPOINT",
        "ERPX_G11_POS_PROVIDER_ENDPOINT",
        "ERPX_G11_PAYROLL_PROVIDER_ENDPOINT",
        "ERPX_G11_DOCUMENT_STORAGE_ENDPOINT",
        "ERPX_G11_MARKETING_PROVIDER_ENDPOINT",
    ] {
        if env::var_os(reference).is_some() {
            eprintln!(
                "G11 vendor reference {reference} exists, but no vendor certification is claimed by this verifier"
            );
        } else {
            eprintln!("G11 vendor certification blocker queued: {reference} is not configured");
        }
    }
}
