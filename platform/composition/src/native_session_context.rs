//! Composition for the loopback session-context bridge.
//!
//! The dev gateway calls `GET /internal/session-context` with the browser's
//! cookies and then injects the six returned private context headers into the
//! proxied native request. Account proof, selected work role and the Holding
//! unit are resolved here; the owner crates keep their authority and SQL.
use crate::business_http::BusinessScopeSlugs;
use erpx_identity_access_server::public::{
    AccountPolicy, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::HoldingWorkSourcePreparation,
    native_lane_44::{
        ActiveWorkFn, ActiveWorkRoleView, RouteFuture, SessionContextTarget, WorkScopeLease,
        session_context_router,
    },
    work_context::{HoldingWorkContextInspector, HoldingWorkspaceRequest},
};
use erpx_platform_runtime_server::public::http_support::{HeaderMap, Router, header};
use erpx_platform_runtime_server::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_platform_shared_server::public::{BusinessError, Clock, CompanyId, FailureKind};
use sqlx::PgConnection;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone)]
struct SessionSources;

impl HoldingWorkSourcePreparation for SessionSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ACCOUNT_ACCESS_UNAVAILABLE",
        "Unable to read current account access.",
    )
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your work role changed. Reload and review before retrying.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

struct Entry {
    holding_id: String,
    inspector: Arc<HoldingWorkContextInspector>,
}

pub struct SessionContextLease {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    entries: BTreeMap<String, Entry>,
}

impl SessionContextLease {
    fn company(&self, headers: &HeaderMap, holding_id: &str) -> Option<CompanyId> {
        let cookie = format!("erpx_company_{holding_id}=");
        let from_cookie = headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(';'))
            .map(str::trim)
            .find_map(|pair| pair.strip_prefix(cookie.as_str()))
            .filter(|value| !value.is_empty());
        let value = from_cookie.or_else(|| {
            headers
                .get("x-erpx-requested-company")
                .and_then(|value| value.to_str().ok())
        })?;
        CompanyId::new(value).ok()
    }
}

impl WorkScopeLease for SessionContextLease {
    fn load_active_roles(
        &self,
        host: &str,
        headers: &HeaderMap,
        work: ActiveWorkFn,
    ) -> RouteFuture<'_, ActiveWorkRoleView> {
        let entry = self.entries.get(host);
        let policy = self.policy.clone();
        let scopes = self.scopes.clone();
        let host = host.to_owned();
        let headers = headers.clone();
        Box::pin(async move {
            let entry = entry.ok_or_else(|| {
                BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
            })?;
            let inbound = parse_inbound_account_request(&policy, &headers)?;
            if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
                return Err(BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."));
            }
            let company = self
                .company(&headers, &entry.holding_id)
                .ok_or_else(changed)?;
            let (branch, department) = scopes.slugs(&headers)?;
            let request = HoldingWorkspaceRequest {
                company_id: company,
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            };
            entry
                .inspector
                .with_account_work(&host, inbound.into_proof(), request, SessionSources, move |c, scope| {
                    let work = work.clone();
                    Box::pin(async move {
                        work(c, scope).await.map_err(PgTransactionError::Business)
                    })
                })
                .await
                .map_err(business)
        })
    }
}

/// Build the loopback context router from exact Holding entries. Each entry is
/// `(host, holding_slug, holding_id, inspector)`; the host must be unique.
pub fn session_context_router_for_holdings(
    entries: Vec<(
        String,
        String,
        String,
        Arc<HoldingWorkContextInspector>,
    )>,
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    clock: Arc<dyn Clock + Send + Sync>,
) -> Result<Router, BusinessError> {
    let mut targets = Vec::with_capacity(entries.len());
    let mut lease = BTreeMap::new();
    for (host, holding_slug, holding_id, inspector) in entries {
        if lease
            .insert(
                host.clone(),
                Entry {
                    holding_id,
                    inspector,
                },
            )
            .is_some()
        {
            return Err(BusinessError::invalid(
                "SESSION_CONTEXT_CONFIG_INVALID",
                "Use exact distinct loopback Holding targets.",
            ));
        }
        targets.push(SessionContextTarget {
            host,
            holding_slug,
        });
    }
    session_context_router(
        targets,
        Arc::new(SessionContextLease {
            policy,
            scopes,
            entries: lease,
        }),
        clock,
    )
}
