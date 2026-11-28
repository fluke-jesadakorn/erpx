use crate::business_http::BusinessScopeSlugs;
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use erpx_commercial_server::public::company_plugins::{
    GithubPluginCatalogue, PluginSource, install_company_plugin, list_company_plugins, withdraw_company_plugin,
};
use erpx_identity_access_server::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::{CapturedHoldingMutation, parse_inbound_account_request},
    effective_authority::{AuthorizedWorkContext, HoldingWorkAuthorizer, WorkAuthorizationRequest},
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

pub(crate) struct PluginRouterState {
    pub policy: Arc<AccountPolicy>,
    pub targets: BTreeMap<(String, String), Arc<HoldingWorkAuthorizer>>,
    pub scopes: Arc<dyn BusinessScopeSlugs>,
    pub catalogue: GithubPluginCatalogue,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLUGINS_UNAVAILABLE",
        "The plugin catalogue is unavailable.",
    )
}

fn unauthorized() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PLUGIN_ADMIN_REQUIRED",
        "Use a Company administrator assignment.",
    )
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PLUGIN_COMMAND_INVALID",
        "Choose a valid plugin and version.",
    )
}

fn db_error(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

fn reply(result: Result<Value, BusinessError>) -> Response {
    match result {
        Ok(data) => Json(json!({"data": data})).into_response(),
        Err(error) => {
            let status = match error.kind {
                FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
                FailureKind::Forbidden => StatusCode::FORBIDDEN,
                FailureKind::NotFound => StatusCode::NOT_FOUND,
                FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
                FailureKind::Conflict => StatusCode::CONFLICT,
                FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            };
            (
                status,
                Json(json!({"error":{"code":error.code,"message":error.message}})),
            )
                .into_response()
        }
    }
}

impl PluginRouterState {
    fn session(
        &self,
        headers: &HeaderMap,
        slug: String,
        mutation: bool,
    ) -> Result<
        (
            String,
            Arc<HoldingWorkAuthorizer>,
            DeviceProof,
            HoldingWorkspaceRequest,
        ),
        BusinessError,
    > {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        let host = inbound.host().to_owned();
        let target = self
            .targets
            .get(&(host.clone(), slug))
            .filter(|_| matches!(inbound.audience(), RequestAudience::Holding { .. }))
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
        if mutation {
            let mut origins = headers.get_all("origin").iter();
            if origins.next().and_then(|origin| origin.to_str().ok())
                != Some(inbound.expected_origin())
                || origins.next().is_some()
            {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "ORIGIN_FORBIDDEN",
                    "Use the application origin.",
                ));
            }
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        let captured = CapturedHoldingMutation::capture(headers, branch, department);
        let selection = if mutation {
            captured.resolve()?
        } else {
            captured.resolve_rendered_read()?
        };
        Ok((host, target.clone(), inbound.into_proof(), selection))
    }
}

fn access(admin: bool) -> WorkAuthorizationRequest {
    WorkAuthorizationRequest {
        destination: "plugins".into(),
        api: true,
        module: None,
        permission: if admin {
            "organization.manage".into()
        } else {
            "records.read".into()
        },
    }
}

fn can_manage(context: &AuthorizedWorkContext) -> bool {
    let selected = context.selection();
    selected.assignment().scope_kind == WorkScopeKind::Holding
        && selected.workspace().branch_id.is_none()
        && selected.workspace().department_id.is_none()
        && context.has_permission("organization.manage")
}

#[derive(Deserialize)]
struct ReadQuery {
    catalogue: Option<u8>,
}

async fn read(
    State(state): State<Arc<PluginRouterState>>,
    Path(slug): Path<String>,
    Query(query): Query<ReadQuery>,
    request: Request,
) -> Response {
    reply(
        async move {
            let (host, target, proof, selection) = state.session(request.headers(), slug, false)?;
            let (installed, can_manage, site_measurement) = target
                .execute(
                    &host,
                    proof,
                    selection,
                    access(false),
                    |connection, context| {
                        Box::pin(async move {
                            let installed = list_company_plugins(
                                connection,
                                &context.selection().workspace().company_id,
                            )
                            .await?;
                            let site_measurement = if installed.iter().any(|installation| {
                                matches!(installation.manifest.tile.source, PluginSource::NextSiteMeasurement)
                            }) {
                                crate::crm_activity_workspace::plugin_site_measurement(connection, context).await?
                            } else {
                                None
                            };
                            Ok((installed, can_manage(context), site_measurement))
                        })
                    },
                )
                .await
                .map_err(db_error)?;
            let versions = if query.catalogue == Some(0) {
                Vec::new()
            } else {
                state.catalogue.versions().await?
            };
            Ok(json!({
                "installed": installed, "canManage": can_manage,
                "versions": versions, "siteMeasurement": site_measurement
            }))
        }
        .await,
    )
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
enum Command {
    Install {
        id: String,
        version: String,
    },
    Withdraw {
        id: String,
        #[serde(rename = "installationVersion")]
        installation_version: i64,
    },
}

async fn command(
    State(state): State<Arc<PluginRouterState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    reply(
        async {
            let (host, target, proof, selection) = state.session(request.headers(), slug, true)?;
            let body = to_bytes(request.into_body(), 4096)
                .await
                .map_err(|_| invalid())?;
            let command: Command = serde_json::from_slice(&body).map_err(|_| invalid())?;
            let id = match &command {
                Command::Install { id, .. } | Command::Withdraw { id, .. } => id,
            };
            if id.is_empty()
                || id.len() > 60
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            {
                return Err(invalid());
            }
            let selected = selection.clone();
            let mut reading = selection;
            reading.mutation = false;
            target
                .execute(&host, proof.clone(), reading, access(true), |_, context| {
                    Box::pin(async {
                        if !can_manage(context) {
                            return Err(unauthorized().into());
                        }
                        Ok(())
                    })
                })
                .await
                .map_err(db_error)?;
            let verified = match &command {
                Command::Install { id, version } => Some(Arc::new(
                    state.catalogue.approved_version(id, version).await?,
                )),
                Command::Withdraw {
                    installation_version,
                    ..
                } if *installation_version > 0 => None,
                _ => return Err(invalid()),
            };
            let (withdraw_id, withdraw_version) = match command {
                Command::Withdraw {
                    id,
                    installation_version,
                } => (id, installation_version),
                Command::Install { .. } => (String::new(), 0),
            };
            target
                .execute(
                    &host,
                    proof,
                    selected,
                    access(true),
                    move |connection, context| {
                        let verified = verified.clone();
                        let withdraw_id = withdraw_id.clone();
                        Box::pin(async move {
                            if !can_manage(context) {
                                return Err(unauthorized().into());
                            }
                            let company = &context.selection().workspace().company_id;
                            let now = sqlx::query_scalar::<_, i64>(
                                "SELECT FLOOR(EXTRACT(EPOCH FROM clock_timestamp())*1000)::bigint",
                            )
                            .fetch_one(&mut *connection)
                            .await?;
                            match verified {
                                Some(verified) => Ok(json!(
                                    install_company_plugin(
                                        connection,
                                        company,
                                        context.user_id(),
                                        &verified,
                                        now
                                    )
                                    .await?
                                )),
                                None => {
                                    withdraw_company_plugin(
                                        connection,
                                        company,
                                        context.user_id(),
                                        &withdraw_id,
                                        withdraw_version,
                                        now,
                                    )
                                    .await?;
                                    Ok(json!({"withdrawn": true}))
                                }
                            }
                        })
                    },
                )
                .await
                .map_err(db_error)
        }
        .await,
    )
}

pub(crate) fn plugin_router(state: PluginRouterState) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/plugins",
            get(read).post(command),
        )
        .with_state(Arc::new(state))
}
