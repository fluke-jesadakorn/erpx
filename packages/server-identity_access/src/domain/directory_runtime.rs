//! Directory runtime ingress contract.
//!
//! The native directory source preview/approve router and the Account directory
//! sign-in router are mounted by the private development API composition when
//! its profiles, provenance ring and Account security keys are configured. This
//! module publishes the exact route, transport and backend-supply contract
//! composition must satisfy before the routers are mounted. It changes no
//! sign-in security semantics and does not replace the retained TypeScript
//! Better Auth transport.
use erpx_server_shared::public::{BusinessError, FailureKind};

pub const SOURCE_PREVIEW_METHOD: &str = "POST";
pub const SOURCE_PREVIEW_ROUTE: &str =
    "/api/v1/holdings/{holding_slug}/iam/connections/{connection_id}/source/preview";
pub const SOURCE_APPROVE_METHOD: &str = "POST";
pub const SOURCE_APPROVE_ROUTE: &str = "/api/v1/holdings/{holding_slug}/iam/connections/{connection_id}/source/reviews/{review_id}/approve";
pub const DIRECTORY_SIGN_IN_METHOD: &str = "POST";
pub const DIRECTORY_SIGN_IN_ROUTE: &str = "/api/auth/directory/sign-in";
pub const DIRECTORY_SOURCE_BODY_LIMIT: usize = 16 * 1024;

/// Source changes are high-risk and require one exact registered Origin plus an
/// idempotency key before any body/readback error is surfaced.
pub const SOURCE_REQUIRED_HEADERS: &[&str] = &["host", "origin", "idempotency-key", "content-type"];
pub const SIGN_IN_REQUIRED_HEADERS: &[&str] = &["host", "content-type"];
/// Account-owned cookie transport; sign-in keeps its existing cookie names and
/// clearing behavior, and no new authority is minted here.
pub const SIGN_IN_COOKIE_TRANSPORT_OWNER: &str = "account_http";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryRuntimeRouterSpec {
    pub name: &'static str,
    pub method: &'static str,
    pub path: &'static str,
    pub required_headers: &'static [&'static str],
    pub body_limit: usize,
    /// Owner-local native router function that composition must mount.
    pub router_function: &'static str,
    /// Backend/session facts composition must supply in addition to the target
    /// registry; none may be invented by the router.
    pub required_composition: &'static [&'static str],
}

pub const DIRECTORY_RUNTIME_ROUTERS: &[DirectoryRuntimeRouterSpec] = &[
    DirectoryRuntimeRouterSpec {
        name: "directory_source_preview",
        method: SOURCE_PREVIEW_METHOD,
        path: SOURCE_PREVIEW_ROUTE,
        required_headers: SOURCE_REQUIRED_HEADERS,
        body_limit: DIRECTORY_SOURCE_BODY_LIMIT,
        router_function: "src/composition/src/directory_source_http.rs::directory_source_router",
        required_composition: &[
            "AccountPolicy",
            "ConfiguredHoldingRegistry",
            "BusinessScopeSlugs",
            "DirectorySourceAdministration<C: Clock>",
        ],
    },
    DirectoryRuntimeRouterSpec {
        name: "directory_source_approve",
        method: SOURCE_APPROVE_METHOD,
        path: SOURCE_APPROVE_ROUTE,
        required_headers: SOURCE_REQUIRED_HEADERS,
        body_limit: DIRECTORY_SOURCE_BODY_LIMIT,
        router_function: "src/composition/src/directory_source_http.rs::directory_source_router",
        required_composition: &[
            "AccountPolicy",
            "ConfiguredHoldingRegistry",
            "BusinessScopeSlugs",
            "DirectorySourceAdministration<C: Clock>",
        ],
    },
    DirectoryRuntimeRouterSpec {
        name: "directory_account_sign_in",
        method: DIRECTORY_SIGN_IN_METHOD,
        path: DIRECTORY_SIGN_IN_ROUTE,
        required_headers: SIGN_IN_REQUIRED_HEADERS,
        body_limit: 16 * 1024,
        router_function: "packages/server-identity_access/src/adapters/directory_http.rs::directory_account_router",
        required_composition: &[
            "AccountAuthentication",
            "DirectoryAuthentication",
            "DirectoryRoutes",
            "DirectoryProviderRead (LDAP or OIDC feature)",
            "DirectorySynchronization",
            "DirectoryAttemptCounter",
            "DirectoryAccountIssuance",
            "AccountPolicy/Audience proof",
        ],
    },
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DirectoryRuntimeComposition {
    pub registered_holdings: usize,
    pub account_policy: bool,
    pub source_administration: bool,
    pub source_scope_slugs: bool,
    pub account_authentication: bool,
    pub directory_authentication: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryRuntimeContract {
    pub routers: &'static [DirectoryRuntimeRouterSpec],
    pub source_body_limit: usize,
    pub source_required_headers: &'static [&'static str],
    pub sign_in_required_headers: &'static [&'static str],
    pub sign_in_cookie_transport_owner: &'static str,
    /// Sign-in must keep the `AccountAuthentication`/`DirectoryAuthentication`
    /// router and the existing Better Auth transport until its own switch.
    pub retained_typescript_transport: bool,
}

pub const DIRECTORY_RUNTIME_CONTRACT: DirectoryRuntimeContract = DirectoryRuntimeContract {
    routers: DIRECTORY_RUNTIME_ROUTERS,
    source_body_limit: DIRECTORY_SOURCE_BODY_LIMIT,
    source_required_headers: SOURCE_REQUIRED_HEADERS,
    sign_in_required_headers: SIGN_IN_REQUIRED_HEADERS,
    sign_in_cookie_transport_owner: SIGN_IN_COOKIE_TRANSPORT_OWNER,
    retained_typescript_transport: true,
};

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

/// Composition calls this before constructing either router. A missing supply
/// fails closed with the exact missing binding rather than mounting a partial
/// directory surface.
pub fn validate_directory_runtime(
    composition: DirectoryRuntimeComposition,
) -> Result<&'static DirectoryRuntimeContract, BusinessError> {
    if composition.registered_holdings == 0 || !composition.account_policy {
        return Err(unavailable(
            "DIRECTORY_RUNTIME_CONFIGURATION_INVALID",
            "Configure registered Holding hosts and the Account policy first.",
        ));
    }
    if !composition.source_administration {
        return Err(unavailable(
            "DIRECTORY_RUNTIME_SOURCE_ADMIN_MISSING",
            "Supply the native Directory source administration adapter.",
        ));
    }
    if !composition.source_scope_slugs {
        return Err(unavailable(
            "DIRECTORY_RUNTIME_SOURCE_SCOPE_MISSING",
            "Supply the Company/branch/department scope resolver.",
        ));
    }
    if !composition.account_authentication || !composition.directory_authentication {
        return Err(unavailable(
            "DIRECTORY_RUNTIME_SIGN_IN_MISSING",
            "Supply both native Account authentication and Directory authentication.",
        ));
    }
    Ok(&DIRECTORY_RUNTIME_CONTRACT)
}

pub fn directory_runtime_contract() -> &'static DirectoryRuntimeContract {
    &DIRECTORY_RUNTIME_CONTRACT
}
