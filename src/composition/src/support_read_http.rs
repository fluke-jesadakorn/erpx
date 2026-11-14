//! Private Support ticket read HTTP. The Account/Holding cookie and rendered
//! work-context CAS are verified again inside the owner read; this layer only
//! supplies requested workspace selectors and the typed read query.
use crate::{
    http_scope::{BusinessScopeSlugs, selected_company_target},
    support_ticket_read::SupportTicketReadService,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use erpx_server_identity_access::public::{
    AccountPolicy, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use serde::Deserialize;
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};

pub struct SupportReadHttpTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub service: Arc<SupportTicketReadService<C>>,
}

struct ReadState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String, String), SupportReadHttpTarget<C>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn support_read_http_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<SupportReadHttpTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut configured = BTreeMap::new();
    for target in targets {
        let valid = policy
            .audience(&target.host)
            .and_then(|audience| registry.expected_for_audience(&audience).ok())
            .is_some_and(|holding| holding.slug == target.holding_slug);
        let key = (
            target.host.clone(),
            target.holding_slug.clone(),
            target.service.company_id().as_str().to_owned(),
        );
        if !valid
            || target.host.is_empty()
            || target.holding_slug.is_empty()
            || configured.insert(key, target).is_some()
        {
            return Err(BusinessError::invalid(
                "SUPPORT_READ_HTTP_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    if configured.is_empty() {
        return Err(BusinessError::invalid(
            "SUPPORT_READ_HTTP_CONFIGURATION_INVALID",
            "Configure at least one Support Holding target.",
        ));
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/support",
            get(read_ticket::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/support/incidents",
            get(read_incidents::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/support/sla",
            get(read_sla::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/support/sessions",
            get(read_sessions::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/support/evidence",
            get(read_evidence::<C>),
        )
        .with_state(Arc::new(ReadState {
            policy,
            scopes,
            targets: configured,
        })))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct SupportReadQuery {
    id: Option<String>,
    order_id: Option<String>,
}

fn selector(value: Option<String>) -> Result<Option<String>, BusinessError> {
    value
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value.len() > 180 || value.chars().any(char::is_control) || value.contains('\0') {
                return Err(BusinessError::invalid(
                    "SUPPORT_READ_SELECTOR_INVALID",
                    "Use the current Support record selector.",
                ));
            }
            Ok(value)
        })
        .transpose()
}

#[derive(Clone, Copy)]
enum ReadSurface {
    Ticket,
    Incidents,
    Sla,
    Sessions,
    Evidence,
}

async fn read_ticket<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    Query(query): Query<SupportReadQuery>,
    headers: HeaderMap,
) -> Response {
    read_surface(state, slug, query, headers, ReadSurface::Ticket).await
}

async fn read_incidents<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    Query(query): Query<SupportReadQuery>,
    headers: HeaderMap,
) -> Response {
    read_surface(state, slug, query, headers, ReadSurface::Incidents).await
}

async fn read_sla<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    Query(query): Query<SupportReadQuery>,
    headers: HeaderMap,
) -> Response {
    read_surface(state, slug, query, headers, ReadSurface::Sla).await
}

async fn read_sessions<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    Query(query): Query<SupportReadQuery>,
    headers: HeaderMap,
) -> Response {
    read_surface(state, slug, query, headers, ReadSurface::Sessions).await
}

async fn read_evidence<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    Query(query): Query<SupportReadQuery>,
    headers: HeaderMap,
) -> Response {
    read_surface(state, slug, query, headers, ReadSurface::Evidence).await
}

async fn read_surface<C: Clock + Send + Sync + 'static>(
    state: Arc<ReadState<C>>,
    slug: String,
    query: SupportReadQuery,
    headers: HeaderMap,
    surface: ReadSurface,
) -> Response {
    let result: Result<Json<serde_json::Value>, PgTransactionError<BusinessError>> = async {
        let id = selector(query.id)?;
        let order_id = selector(query.order_id)?;
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = selected_company_target(&state.targets, host, &slug, &headers)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|value| value != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let (branch, department) = state.scopes.slugs(&headers)?.clone();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        let workspace = move || captured.resolve_read(company);
        let data = match surface {
            ReadSurface::Ticket => {
                if let Some(id) = id {
                    target
                        .service
                        .detail(host, inbound.into_proof(), workspace, id)
                        .await?
                } else if let Some(order_id) = order_id {
                    json!(
                        target
                            .service
                            .recipients(host, inbound.into_proof(), workspace, order_id)
                            .await?
                    )
                } else {
                    target
                        .service
                        .workspace(host, inbound.into_proof(), workspace)
                        .await?
                }
            }
            ReadSurface::Incidents => {
                if let Some(id) = id {
                    target
                        .service
                        .incident_detail(host, inbound.into_proof(), workspace, id)
                        .await?
                } else {
                    target
                        .service
                        .incident_workspace(host, inbound.into_proof(), workspace)
                        .await?
                }
            }
            ReadSurface::Sla => {
                target
                    .service
                    .sla_workspace(host, inbound.into_proof(), workspace)
                    .await?
            }
            ReadSurface::Sessions => {
                if let Some(id) = id {
                    target
                        .service
                        .session_detail(host, inbound.into_proof(), workspace, id)
                        .await?
                } else {
                    target
                        .service
                        .session_workspace(host, inbound.into_proof(), workspace)
                        .await?
                }
            }
            ReadSurface::Evidence => {
                let Some(id) = id else {
                    return Err(BusinessError::invalid(
                        "SUPPORT_EVIDENCE_SELECTOR_REQUIRED",
                        "Provide the current Support ticket.",
                    )
                    .into());
                };
                target
                    .service
                    .evidence(host, inbound.into_proof(), workspace, id)
                    .await?
            }
        };
        Ok::<_, PgTransactionError<BusinessError>>(Json(json!({ "data": data })))
    }
    .await;
    let mut response = match result {
        Ok(data) => data.into_response(),
        Err(PgTransactionError::Business(error)) => {
            let status = match error.kind {
                FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
                FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
                FailureKind::Forbidden => StatusCode::FORBIDDEN,
                FailureKind::NotFound => StatusCode::NOT_FOUND,
                FailureKind::Conflict => StatusCode::CONFLICT,
                FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            };
            (
                status,
                Json(json!({"error": {"code": error.code, "message": error.message}})),
            )
                .into_response()
        }
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}
