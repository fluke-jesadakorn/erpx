//! Server-only composition root for the Web SSR/BFF process.

use crate::bff::{
    BffState, accounts, activate_account, activate_role, delete_account, delete_session,
    development_proxy, directory_login, grpc_proxy, identity_proxy, navigation_manifest,
    passkey_authentication_begin, passkey_authentication_finish, password_login,
    password_login_form, public_branding_proxy, public_marketing_proxy, recovery_login,
    role_options, session_middleware, session_snapshot, upgrade_legacy_session,
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

const DATABASE_URL_ENV: &str = "ERPX_CONTROL_WRITE_DATABASE_URL";
const AUTH_HTTP_ORIGIN_ENV: &str = "ERPX_CONSOLE_AUTH_HTTP_ORIGIN";
const GRPC_ORIGIN_ENV: &str = "ERPX_CONSOLE_GRPC_ENDPOINT";

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
    let state = BffState::new(store, auth_http_origin, grpc_origin)?;

    Ok(dioxus_router(crate::App)
        .route("/api/bff/v1/session", get(session_snapshot))
        .route("/api/bff/v1/navigation", post(navigation_manifest))
        .route("/api/bff/v1/auth/password", post(password_login))
        .route("/api/bff/v1/auth/password/form", post(password_login_form))
        .route("/api/bff/v1/auth/directory", post(directory_login))
        .route(
            "/api/bff/v1/auth/passkeys/authentication/begin",
            post(passkey_authentication_begin),
        )
        .route(
            "/api/bff/v1/auth/passkeys/authentication/finish",
            post(passkey_authentication_finish),
        )
        .route("/api/bff/v1/auth/recovery", post(recovery_login))
        .route("/api/bff/v1/accounts", get(accounts))
        .route("/api/bff/v1/accounts/activate", post(activate_account))
        .route("/api/bff/v1/accounts/{session_id}", delete(delete_account))
        .route("/api/bff/v1/roles", get(role_options))
        .route("/api/bff/v1/roles/activate", post(activate_role))
        .route("/api/bff/v1/session", delete(delete_session))
        .route("/api/bff/v1/grpc/{*grpc_path}", post(grpc_proxy))
        .route(
            "/api/bff/v1/identity/{*identity_path}",
            post(identity_proxy).get(identity_proxy),
        )
        .route(
            "/api/bff/v1/dev-access/{*development_path}",
            post(development_proxy).get(development_proxy),
        )
        .route("/api/bff/v1/public/branding", get(public_branding_proxy))
        .route(
            "/api/bff/v1/public/marketing/{*public_path}",
            post(public_marketing_proxy).get(public_marketing_proxy),
        )
        .route("/auth/bff/upgrade", post(upgrade_legacy_session))
        .layer(middleware::from_fn(session_middleware))
        .layer(Extension(state)))
}
