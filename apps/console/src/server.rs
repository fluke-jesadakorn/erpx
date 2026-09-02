//! Server-only composition root for the Web SSR/BFF process.

use crate::bff::{
    BffState, accounts, activate_account, activate_role, delete_account, directory_login,
    passkey_authentication_begin, passkey_authentication_finish, password_login,
    password_login_form, password_login_form_entry, recovery_login, role_options,
    session_middleware,
};
use dioxus::server::{
    axum::{
        Extension, Router,
        extract::{OriginalUri, Request},
        http::{Method, StatusCode, header},
        middleware::{self, Next},
        response::{IntoResponse, Redirect, Response},
        routing::{delete, get, post},
    },
    router as dioxus_router,
};
use erpx_web_session::WebSessionStore;
use erpx_web_session_sqlx::{SqlxWebSessionStore, WebSessionCipher};
use sqlx::postgres::PgPoolOptions;
use std::{env, io, sync::Arc};
use tower_http::compression::CompressionLayer;

const DATABASE_URL_ENV: &str = "ERPX_CONTROL_WRITE_DATABASE_URL";
const AUTH_HTTP_ORIGIN_ENV: &str = "ERPX_CONSOLE_AUTH_HTTP_ORIGIN";
const GRPC_ORIGIN_ENV: &str = "ERPX_CONSOLE_GRPC_ENDPOINT";
const TRUSTED_PUBLIC_ORIGINS_ENV: &str = "ERPX_WEB_TRUSTED_ORIGINS";
const PUBLIC_ORIGIN_ENV: &str = "ERPX_PUBLIC_ORIGIN";
const PLATFORM_ORIGIN_ENV: &str = "ERPX_PLATFORM_ORIGIN";
const DEVELOPMENT_TAILSCALE_FQDN_ENV: &str = "ERPX_DEV_TAILSCALE_FQDN";
const DEFAULT_PUBLIC_ORIGIN: &str = "http://127.0.0.1:3004";
const DEVELOPMENT_PLATFORM_ORIGIN: &str = "http://platform.localhost:3004";
const DEVELOPMENT_PLATFORM_HOST: &str = "platform.localhost";
const PRODUCTION_PLATFORM_ORIGIN: &str = "https://console.tailb48369.ts.net";
const PRODUCTION_PLATFORM_HOST: &str = "console.tailb48369.ts.net";
const PRODUCTION_PROFILE_ENV: &str = "ERPX_WEB_PRODUCTION_PROFILE";
const DIOXUS_PROXY_HEADER: &str = "x-proxied-by-dioxus";
const TAILSCALE_IDENTITY_HEADER: &str = "tailscale-user-login";

#[derive(Clone)]
struct PlatformHostPolicy {
    origin: Arc<str>,
    host: Arc<str>,
    production_profile: bool,
}

async fn root_relative_auth_asset(OriginalUri(uri): OriginalUri) -> Redirect {
    let location = uri
        .path_and_query()
        .and_then(|value| value.as_str().strip_prefix("/auth"))
        .unwrap_or("/");
    Redirect::temporary(location)
}

fn is_platform_path(path: &str) -> bool {
    matches!(path, "/platform/holdings" | "/platform/holdings/")
        || path.starts_with("/platform/holdings/")
}

fn platform_host_allowed(path: &str, host: Option<&str>, expected_host: &str) -> bool {
    !is_platform_path(path)
        || host.is_some_and(|value| {
            let hostname = value
                .rsplit_once(':')
                .filter(|(_, port)| {
                    !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit())
                })
                .map_or(value, |(hostname, _)| hostname);
            hostname.eq_ignore_ascii_case(expected_host)
        })
}

fn development_tailnet_hostname(value: &str) -> Option<String> {
    let hostname = value.trim().trim_end_matches('.').to_ascii_lowercase();
    let labels = hostname.split('.').collect::<Vec<_>>();
    (hostname.len() <= 253
        && labels.len() >= 4
        && hostname.ends_with(".ts.net")
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        }))
    .then_some(hostname)
}

fn dioxus_development_proxy_supplies_platform_host(
    production_profile: bool,
    forwarded_host: Option<&str>,
    dioxus_proxied: bool,
    tailscale_identity: bool,
    expected_host: &str,
) -> bool {
    !production_profile
        && forwarded_host.is_none()
        && dioxus_proxied
        && (!tailscale_identity || expected_host.ends_with(".ts.net"))
}

async fn platform_host_fence(
    Extension(policy): Extension<PlatformHostPolicy>,
    request: Request,
    next: Next,
) -> Response {
    let forwarded_host = if policy.production_profile {
        None
    } else {
        request
            .headers()
            .get("x-forwarded-host")
            .and_then(|value| value.to_str().ok())
    };
    let host = forwarded_host
        .or_else(|| {
            request
                .headers()
                .get(header::HOST)
                .and_then(|value| value.to_str().ok())
        })
        .or_else(|| request.uri().authority().map(|value| value.as_str()));
    // The Dioxus development proxy intentionally rewrites Host to its ephemeral
    // SSR backend and does not preserve the original authority. Its marker
    // cannot be supplied through the public dev edge because the proxy rejects
    // already-marked requests. Keep external/Tailscale requests on the normal
    // host fence while allowing the configured local or Tailnet development
    // origin to reach the SSR process.
    let dioxus_development_hop = dioxus_development_proxy_supplies_platform_host(
        policy.production_profile,
        forwarded_host,
        request
            .headers()
            .get(DIOXUS_PROXY_HEADER)
            .and_then(|value| value.to_str().ok())
            == Some("true"),
        request.headers().contains_key(TAILSCALE_IDENTITY_HEADER),
        &policy.host,
    );
    if !platform_host_allowed(request.uri().path(), host, &policy.host) && !dioxus_development_hop {
        if matches!(*request.method(), Method::GET | Method::HEAD) {
            let path_and_query = request
                .uri()
                .path_and_query()
                .map_or(request.uri().path(), |value| value.as_str());
            return Redirect::temporary(&format!("{}{path_and_query}", policy.origin))
                .into_response();
        }
        return StatusCode::NOT_FOUND.into_response();
    }
    next.run(request).await
}

pub(crate) async fn router() -> Result<Router, io::Error> {
    let database_url = env::var(DATABASE_URL_ENV)
        .map_err(|_| io::Error::other(format!("{DATABASE_URL_ENV} is required")))?;
    let cipher = WebSessionCipher::from_env()
        .map_err(|_| io::Error::other("web session encryption key is unavailable"))?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .map_err(|_| io::Error::other("web session persistence is unavailable"))?;
    let ai_pool = pool.clone();
    let store: Arc<dyn WebSessionStore> = Arc::new(SqlxWebSessionStore::new(pool, cipher));
    let auth_http_origin =
        env::var(AUTH_HTTP_ORIGIN_ENV).unwrap_or_else(|_| "http://127.0.0.1:50052".to_owned());
    let grpc_origin =
        env::var(GRPC_ORIGIN_ENV).unwrap_or_else(|_| "http://127.0.0.1:50051".to_owned());
    let public_origin =
        env::var(PUBLIC_ORIGIN_ENV).unwrap_or_else(|_| DEFAULT_PUBLIC_ORIGIN.into());
    let production_profile = env::var(PRODUCTION_PROFILE_ENV).as_deref() == Ok("enabled");
    let (expected_platform_origin, expected_platform_host) = if production_profile {
        (
            PRODUCTION_PLATFORM_ORIGIN.to_owned(),
            PRODUCTION_PLATFORM_HOST.to_owned(),
        )
    } else if let Some(configured_tailnet_hostname) = env::var(DEVELOPMENT_TAILSCALE_FQDN_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        let hostname =
            development_tailnet_hostname(&configured_tailnet_hostname).ok_or_else(|| {
                io::Error::other("ERPX_DEV_TAILSCALE_FQDN must be an exact hostname below .ts.net")
            })?;
        (format!("https://{hostname}"), hostname)
    } else {
        (
            DEVELOPMENT_PLATFORM_ORIGIN.to_owned(),
            DEVELOPMENT_PLATFORM_HOST.to_owned(),
        )
    };
    let platform_origin =
        env::var(PLATFORM_ORIGIN_ENV).unwrap_or_else(|_| expected_platform_origin.clone());
    if platform_origin.trim_end_matches('/') != expected_platform_origin {
        return Err(io::Error::other(if production_profile {
            "ERPX_PLATFORM_ORIGIN must be the exact tailnet-only production Platform origin"
        } else {
            "ERPX_PLATFORM_ORIGIN must match the configured development Platform origin"
        }));
    }
    let platform_policy = PlatformHostPolicy {
        origin: Arc::from(expected_platform_origin),
        host: Arc::from(expected_platform_host),
        production_profile,
    };
    let configured_trusted_origins = env::var(TRUSTED_PUBLIC_ORIGINS_ENV).unwrap_or_default();
    let trusted_public_origins = [
        public_origin.trim_end_matches('/'),
        platform_origin.trim_end_matches('/'),
        configured_trusted_origins.trim(),
    ]
    .into_iter()
    .filter(|value| !value.is_empty())
    .collect::<Vec<_>>()
    .join(",");
    if production_profile
        && (env::var("ERPX_DEV_PERSONA_MODE").as_deref() == Ok("enabled")
            || env::var("ERPX_DEVELOPMENT_PROFILE").is_ok_and(|value| value.starts_with("local")))
    {
        return Err(io::Error::other(
            "development access is forbidden in the Web production profile",
        ));
    }
    let state = BffState::new_with_pool(
        store,
        auth_http_origin,
        grpc_origin,
        trusted_public_origins,
        platform_origin,
        production_profile,
        Some(ai_pool),
    )?;

    Ok(dioxus_router(crate::App)
        .route("/auth/assets/{*path}", get(root_relative_auth_asset))
        .route("/auth/wasm/{*path}", get(root_relative_auth_asset))
        .route("/api/auth/password", post(password_login))
        .route(
            "/api/auth/password/form",
            get(password_login_form_entry).post(password_login_form),
        )
        .route("/api/auth/directory", post(directory_login))
        .route(
            "/api/auth/passkeys/authentication/begin",
            post(passkey_authentication_begin),
        )
        .route(
            "/api/auth/passkeys/authentication/finish",
            post(passkey_authentication_finish),
        )
        .route("/api/auth/recovery", post(recovery_login))
        .route("/api/accounts", get(accounts))
        .route("/api/accounts/activate", post(activate_account))
        .route("/api/accounts/{session_id}", delete(delete_account))
        .route("/api/roles", get(role_options))
        .route("/api/roles/activate", post(activate_role))
        .layer(CompressionLayer::new())
        .layer(middleware::from_fn(session_middleware))
        .layer(Extension(state))
        .layer(middleware::from_fn(platform_host_fence))
        .layer(Extension(platform_policy)))
}

#[cfg(test)]
mod tests {
    use super::{
        DEVELOPMENT_PLATFORM_HOST, PRODUCTION_PLATFORM_HOST, development_tailnet_hostname,
        dioxus_development_proxy_supplies_platform_host, platform_host_allowed,
    };

    #[test]
    fn axum_extends_the_single_dioxus_renderer_with_bff_routes_only() {
        let source = include_str!("server.rs");
        assert!(source.contains("Ok(dioxus_router(crate::App)"));
        for forbidden in [
            concat!(".fall", "back("),
            concat!("routing::get", "_service"),
            concat!("Serve", "Dir"),
            concat!("Html", "("),
            concat!("include_str!(\"../", "index.html\")"),
        ] {
            assert!(
                !source.contains(forbidden),
                "Axum must not own an HTML UI fallback: {forbidden}"
            );
        }
    }

    #[test]
    fn relative_auth_assets_are_normalized_without_an_html_fallback() {
        let source = include_str!("server.rs");
        assert!(source.contains("/auth/assets/{*path}"));
        assert!(source.contains("/auth/wasm/{*path}"));
        assert!(source.contains("strip_prefix(\"/auth\")"));
    }

    #[test]
    fn platform_holding_routes_require_the_environment_specific_exact_host() {
        assert!(platform_host_allowed(
            "/platform/holdings",
            Some("platform.localhost:3004"),
            DEVELOPMENT_PLATFORM_HOST,
        ));
        assert!(platform_host_allowed(
            "/platform/holdings/00000000-0000-0000-0000-000000000001",
            Some(PRODUCTION_PLATFORM_HOST),
            PRODUCTION_PLATFORM_HOST,
        ));
        assert!(!platform_host_allowed(
            "/platform/holdings",
            Some("127.0.0.1:3004"),
            DEVELOPMENT_PLATFORM_HOST,
        ));
        assert!(!platform_host_allowed(
            "/platform/holdings",
            Some(PRODUCTION_PLATFORM_HOST),
            DEVELOPMENT_PLATFORM_HOST,
        ));
        assert!(!platform_host_allowed(
            "/platform/holdings",
            None,
            DEVELOPMENT_PLATFORM_HOST,
        ));
        assert!(platform_host_allowed(
            "/work",
            Some("127.0.0.1:3004"),
            DEVELOPMENT_PLATFORM_HOST,
        ));
    }

    #[test]
    fn only_an_internal_dioxus_development_hop_can_supply_the_hidden_host() {
        assert!(dioxus_development_proxy_supplies_platform_host(
            false,
            None,
            true,
            false,
            DEVELOPMENT_PLATFORM_HOST,
        ));
        assert!(!dioxus_development_proxy_supplies_platform_host(
            true,
            None,
            true,
            false,
            PRODUCTION_PLATFORM_HOST,
        ));
        assert!(!dioxus_development_proxy_supplies_platform_host(
            false,
            Some("fluke-macbook16.tailb48369.ts.net"),
            true,
            false,
            "fluke-macbook16.tailb48369.ts.net",
        ));
        assert!(!dioxus_development_proxy_supplies_platform_host(
            false,
            None,
            true,
            true,
            DEVELOPMENT_PLATFORM_HOST,
        ));
        assert!(dioxus_development_proxy_supplies_platform_host(
            false,
            None,
            true,
            true,
            "fluke-macbook16.tailb48369.ts.net",
        ));
        assert!(!dioxus_development_proxy_supplies_platform_host(
            false,
            None,
            false,
            false,
            DEVELOPMENT_PLATFORM_HOST,
        ));
    }

    #[test]
    fn development_tailnet_platform_host_is_strictly_validated() {
        assert_eq!(
            development_tailnet_hostname("Fluke-MacBook16.tailb48369.ts.net."),
            Some("fluke-macbook16.tailb48369.ts.net".to_owned())
        );
        for invalid in [
            "https://fluke-macbook16.tailb48369.ts.net",
            "tailb48369.ts.net",
            "fluke-macbook16.tailb48369.ts.net.example.com",
            "-fluke.tailb48369.ts.net",
        ] {
            assert_eq!(development_tailnet_hostname(invalid), None);
        }
    }
}
