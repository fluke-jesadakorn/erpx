//! Server-only composition root for the Web SSR/BFF process.

use crate::bff::{
    BffState, accounts, activate_account, activate_role, dashboard_read_model, delete_account,
    delete_session, development_proxy, directory_login, grpc_proxy, identity_proxy,
    navigation_manifest, passkey_authentication_begin, passkey_authentication_finish,
    password_login, password_login_form, password_login_form_entry, public_brand_asset_proxy,
    public_branding_proxy, public_marketing_proxy, recovery_login, role_options,
    session_middleware, session_snapshot, upgrade_legacy_session, work_queue_read_model,
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
        .route(erpx_console_bff::SESSION_PATH, get(session_snapshot))
        .route(erpx_console_bff::NAVIGATION_PATH, post(navigation_manifest))
        .route(erpx_console_bff::DASHBOARD_PATH, get(dashboard_read_model))
        .route(
            erpx_console_bff::WORK_ITEMS_PATH,
            get(work_queue_read_model),
        )
        .route("/api/v1/auth/password", post(password_login))
        .route(
            "/api/v1/auth/password/form",
            get(password_login_form_entry).post(password_login_form),
        )
        .route("/api/v1/auth/directory", post(directory_login))
        .route(
            "/api/v1/auth/passkeys/authentication/begin",
            post(passkey_authentication_begin),
        )
        .route(
            "/api/v1/auth/passkeys/authentication/finish",
            post(passkey_authentication_finish),
        )
        .route("/api/v1/auth/recovery", post(recovery_login))
        .route("/api/v1/accounts", get(accounts))
        .route("/api/v1/accounts/activate", post(activate_account))
        .route("/api/v1/accounts/{session_id}", delete(delete_account))
        .route("/api/v1/roles", get(role_options))
        .route("/api/v1/roles/activate", post(activate_role))
        .route(erpx_console_bff::SESSION_PATH, delete(delete_session))
        .route(erpx_console_bff::GRPC_PROXY_PATH, post(grpc_proxy))
        .route(
            "/api/v1/identity/{*identity_path}",
            post(identity_proxy).get(identity_proxy),
        )
        .route(
            "/api/v1/dev-access/{*development_path}",
            post(development_proxy).get(development_proxy),
        )
        .route("/api/v1/public/branding", get(public_branding_proxy))
        .route(
            "/api/v1/public/branding/assets/{kind}",
            get(public_brand_asset_proxy),
        )
        .route(
            "/api/v1/public/marketing/{*public_path}",
            post(public_marketing_proxy).get(public_marketing_proxy),
        )
        .route("/auth/bff/upgrade", post(upgrade_legacy_session))
        .layer(CompressionLayer::new())
        .layer(middleware::from_fn(session_middleware))
        .layer(Extension(state)))
}
