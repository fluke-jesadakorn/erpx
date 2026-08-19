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
        http::{StatusCode, header},
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
const DEFAULT_PUBLIC_ORIGIN: &str = "http://127.0.0.1:3004";
const PLATFORM_ORIGIN: &str = "https://console.tailb48369.ts.net";
const PLATFORM_HOST: &str = "console.tailb48369.ts.net";
const PRODUCTION_PROFILE_ENV: &str = "ERPX_WEB_PRODUCTION_PROFILE";

async fn root_relative_auth_asset(OriginalUri(uri): OriginalUri) -> Redirect {
    let location = uri
        .path_and_query()
        .and_then(|value| value.as_str().strip_prefix("/auth"))
        .unwrap_or("/");
    Redirect::temporary(location)
}

fn platform_host_allowed(path: &str, host: Option<&str>) -> bool {
    !matches!(path, "/platform/holdings" | "/platform/holdings/")
        && !path.starts_with("/platform/holdings/")
        || host.is_some_and(|value| value.eq_ignore_ascii_case(PLATFORM_HOST))
}

async fn platform_host_fence(request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    if !platform_host_allowed(request.uri().path(), host) {
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
    let store: Arc<dyn WebSessionStore> = Arc::new(SqlxWebSessionStore::new(pool, cipher));
    let auth_http_origin =
        env::var(AUTH_HTTP_ORIGIN_ENV).unwrap_or_else(|_| "http://127.0.0.1:50052".to_owned());
    let grpc_origin =
        env::var(GRPC_ORIGIN_ENV).unwrap_or_else(|_| "http://127.0.0.1:50051".to_owned());
    let public_origin =
        env::var(PUBLIC_ORIGIN_ENV).unwrap_or_else(|_| DEFAULT_PUBLIC_ORIGIN.into());
    let platform_origin = env::var(PLATFORM_ORIGIN_ENV).unwrap_or_else(|_| PLATFORM_ORIGIN.into());
    if platform_origin.trim_end_matches('/') != PLATFORM_ORIGIN {
        return Err(io::Error::other(
            "ERPX_PLATFORM_ORIGIN must be the tailnet-only Platform origin",
        ));
    }
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
    let production_profile = env::var(PRODUCTION_PROFILE_ENV).as_deref() == Ok("enabled");
    if production_profile
        && (env::var("ERPX_DEV_PERSONA_MODE").as_deref() == Ok("enabled")
            || env::var("ERPX_DEVELOPMENT_PROFILE").is_ok_and(|value| value.starts_with("local")))
    {
        return Err(io::Error::other(
            "development access is forbidden in the Web production profile",
        ));
    }
    let state = BffState::new(
        store,
        auth_http_origin,
        grpc_origin,
        trusted_public_origins,
        production_profile,
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
        .layer(middleware::from_fn(platform_host_fence)))
}

#[cfg(test)]
mod tests {
    use super::{PLATFORM_HOST, platform_host_allowed};

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
    fn platform_holding_routes_require_the_exact_platform_host() {
        assert!(platform_host_allowed(
            "/platform/holdings",
            Some(PLATFORM_HOST)
        ));
        assert!(platform_host_allowed(
            "/platform/holdings/00000000-0000-0000-0000-000000000001",
            Some(PLATFORM_HOST)
        ));
        assert!(!platform_host_allowed(
            "/platform/holdings",
            Some("127.0.0.1:3004")
        ));
        assert!(!platform_host_allowed("/platform/holdings", None));
        assert!(platform_host_allowed("/work", Some("127.0.0.1:3004")));
    }
}
