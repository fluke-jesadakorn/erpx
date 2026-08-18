//! Server-only composition root for the Web SSR/BFF process.

use crate::bff::{
    BffState, accounts, activate_account, activate_role, delete_account, directory_login,
    passkey_authentication_begin, passkey_authentication_finish, password_login,
    password_login_form, password_login_form_entry, recovery_login, role_options,
    session_middleware,
};
use dioxus::server::{
    axum::{
        Extension, Router, middleware,
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
const PRODUCTION_PROFILE_ENV: &str = "ERPX_WEB_PRODUCTION_PROFILE";

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
    let trusted_public_origins = env::var(TRUSTED_PUBLIC_ORIGINS_ENV).unwrap_or_default();
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
        .route("/api/v2/auth/password", post(password_login))
        .route(
            "/api/v2/auth/password/form",
            get(password_login_form_entry).post(password_login_form),
        )
        .route("/api/v2/auth/directory", post(directory_login))
        .route(
            "/api/v2/auth/passkeys/authentication/begin",
            post(passkey_authentication_begin),
        )
        .route(
            "/api/v2/auth/passkeys/authentication/finish",
            post(passkey_authentication_finish),
        )
        .route("/api/v2/auth/recovery", post(recovery_login))
        .route("/api/v2/accounts", get(accounts))
        .route("/api/v2/accounts/activate", post(activate_account))
        .route("/api/v2/accounts/{session_id}", delete(delete_account))
        .route("/api/v2/roles", get(role_options))
        .route("/api/v2/roles/activate", post(activate_role))
        .layer(CompressionLayer::new())
        .layer(middleware::from_fn(session_middleware))
        .layer(Extension(state)))
}

#[cfg(test)]
mod tests {
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
}
