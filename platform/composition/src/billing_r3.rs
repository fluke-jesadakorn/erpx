//! Self-contained L05 Billing composition slice for the R3 native reads.
//!
//! The target read is owner-local SQL in the Billing crate; this module only
//! binds the two exact registered Platform hosts and the Control pool. The
//! forecast read reuses the existing `BillingService`/`BillingBackend`
//! authority through a lazily resolved backend factory registered by the
//! already assembled Billing backend, so no owner SQL or session policy is
//! duplicated here. Lead integration line:
//!
//! `router = crate::billing_r3::mount(router, config, pools)?;`

use crate::private_development_api::DevelopmentApiConfiguration;
use axum::Router;
use erpx_billing_server::public::forecast_http::{
    BillingForecastHttpBackend, BillingForecastHttpFuture, BillingForecastHttpRequest,
    BillingForecastHttpResponse, BillingForecastHttpTarget, billing_forecast_router,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind, HoldingId};
use std::{collections::BTreeSet, sync::Arc};

/// Exact registered Platform surfaces for the target read. The owner router
/// refuses every other Host with `404 NOT_FOUND`.
pub const TARGET_HOSTS: [&str; 2] = [
    "admin.jesadakorn.com",
    "erpx-internal.jesadakorn.com",
];

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BILLING_FORECAST_BACKEND_UNAVAILABLE",
        "The native Billing forecast backend is unavailable.",
    )
}

fn target_hosts() -> Vec<String> {
    TARGET_HOSTS.iter().map(|host| (*host).to_owned()).collect()
}

#[derive(Clone, Debug)]
struct ForecastSpec {
    host: String,
    holding_slug: String,
    holding_id: HoldingId,
    origin: String,
}

fn forecast_specs(config: &DevelopmentApiConfiguration) -> Vec<ForecastSpec> {
    config
        .holdings
        .iter()
        .map(|holding| ForecastSpec {
            host: holding.host.clone(),
            holding_slug: holding.registration.slug.clone(),
            holding_id: holding.registration.holding_id.clone(),
            origin: format!("https://{}", holding.host),
        })
        .collect()
}

/// Transport-only validation; the owner router repeats it on every request.
fn validate_forecast_specs(specs: &[ForecastSpec]) -> Result<(), &'static str> {
    let mut targets = BTreeSet::new();
    for spec in specs {
        if spec.host.is_empty()
            || spec.holding_slug.is_empty()
            || spec.holding_slug.len() > 128
            || !spec.holding_slug.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'_' | b'-')
            })
            || spec.origin != format!("https://{}", spec.host)
            || !targets.insert((spec.host.clone(), spec.holding_slug.clone()))
        {
            return Err("PRIVATE_API_BILLING_R3_FORECAST_INVALID");
        }
    }
    Ok(())
}

/// Resolves the already assembled Billing backend by Holding. The factory is
/// registered by `crate::billing::BillingBackend::new`, which owns the Account
/// policy, scope headers and `BillingService`; this adapter owns no policy and
/// executes no SQL.
struct LazyForecastBackend {
    holding: HoldingId,
}

impl BillingForecastHttpBackend for LazyForecastBackend {
    fn forecast<'a>(
        &'a self,
        request: BillingForecastHttpRequest<'a>,
    ) -> BillingForecastHttpFuture<'a, BillingForecastHttpResponse> {
        Box::pin(async move {
            let backend = crate::billing::forecast_backend(self.holding.as_str())
                .ok_or_else(unavailable)?;
            backend.forecast(request).await
        })
    }
}

/// Merge both native L05 reads into the private API router. `pools[0]` is the
/// Control pool; the forecast targets are resolved from the registered
/// Holdings by the owner router on each request.
pub fn mount(
    router: Router,
    config: &DevelopmentApiConfiguration,
    pools: &[DatabasePool],
) -> Result<Router, &'static str> {
    let control = pools
        .first()
        .ok_or("PRIVATE_API_BILLING_R3_CONTROL_POOL_REQUIRED")?;
    let specs = forecast_specs(config);
    validate_forecast_specs(&specs)?;
    let forecast_targets = specs
        .into_iter()
        .map(|spec| BillingForecastHttpTarget {
            host: spec.host,
            holding_slug: spec.holding_slug,
            origin: spec.origin,
            backend: Arc::new(LazyForecastBackend {
                holding: spec.holding_id,
            }) as Arc<dyn BillingForecastHttpBackend>,
        })
        .collect();
    let target = erpx_billing_server::public::native_r3_platform_billing_target::router(
        target_hosts(),
        control.clone(),
    );
    let forecast = billing_forecast_router(forecast_targets)
        .map_err(|_| "PRIVATE_API_BILLING_R3_FORECAST_INVALID")?;
    Ok(router.merge(target).merge(forecast))
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::HeaderMap;
    use std::future::Future;

    fn spec(host: &str, slug: &str) -> ForecastSpec {
        ForecastSpec {
            host: host.to_owned(),
            holding_slug: slug.to_owned(),
            holding_id: HoldingId::new(slug).unwrap(),
            origin: format!("https://{host}"),
        }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(future)
    }

    #[test]
    fn target_hosts_are_the_two_registered_platform_surfaces() {
        assert_eq!(target_hosts(), [
            "admin.jesadakorn.com".to_owned(),
            "erpx-internal.jesadakorn.com".to_owned(),
        ]);
    }

    #[test]
    fn forecast_targets_are_distinct_and_exactly_bound() {
        assert!(validate_forecast_specs(&[
            spec("acme.jesadakorn.com", "acme"),
            spec("erpx-internal.jesadakorn.com", "erpx"),
        ])
        .is_ok());
        for invalid in [
            vec![spec("acme.jesadakorn.com", "acme"); 2],
            vec![spec("", "acme")],
            vec![spec("acme.jesadakorn.com", "ACME")],
            vec![ForecastSpec {
                origin: "https://other.test".to_owned(),
                ..spec("acme.jesadakorn.com", "acme")
            }],
        ] {
            assert_eq!(
                validate_forecast_specs(&invalid),
                Err("PRIVATE_API_BILLING_R3_FORECAST_INVALID")
            );
        }
    }

    #[test]
    fn unresolved_forecast_backend_fails_closed() {
        let backend = LazyForecastBackend {
            holding: HoldingId::new("missing-holding").unwrap(),
        };
        let headers = HeaderMap::new();
        let request = BillingForecastHttpRequest {
            host: "acme.jesadakorn.com",
            holding_slug: "acme",
            headers: &headers,
            platform_customer: None,
        };
        let error = block_on(backend.forecast(request)).err().unwrap();
        assert_eq!(error.code, "BILLING_FORECAST_BACKEND_UNAVAILABLE");
        assert_eq!(error.kind, FailureKind::Unavailable);
    }

    #[test]
    fn holding_owner_authority_predicate_rejects_non_owners() {
        use erpx_identity_access_server::public::work_context::WorkScopeKind;
        assert!(crate::billing::forecast_holding_scope(
            "holding_owner",
            WorkScopeKind::Holding,
            false,
            false,
        )
        .is_ok());
        assert!(crate::billing::forecast_company_authority(true).is_ok());
        for role in ["company_user", "it_admin", "platform_admin"] {
            let error = crate::billing::forecast_holding_scope(
                role,
                WorkScopeKind::Holding,
                false,
                false,
            )
            .err()
            .unwrap();
            assert_eq!(error.code, "FORBIDDEN");
        }
        for (scope, branch, department) in [
            (WorkScopeKind::Branch, true, false),
            (WorkScopeKind::Department, false, true),
        ] {
            assert_eq!(
                crate::billing::forecast_holding_scope(
                    "holding_owner",
                    scope,
                    branch,
                    department,
                )
                .err()
                .unwrap()
                .code,
                "FORBIDDEN"
            );
        }
        assert_eq!(
            crate::billing::forecast_company_authority(false)
                .err()
                .unwrap()
                .code,
            "COMPANY_AUTHORITY_REQUIRED"
        );
    }
}
