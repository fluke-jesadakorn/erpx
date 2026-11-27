//! Private structural lookup for the Holding page gateway. Composition supplies
//! the exact registered hosts with their leased Holding database pools. The
//! result supplies no account, role, or record authorization.
use crate::adapters::postgres_hierarchy::PgOrganizationReader;
use crate::application::hierarchy::resolve_holding_address;
use crate::domain::hierarchy::HoldingAddressQuery;
use erpx_server_runtime::public::http_support::{
    HeaderValue, IntoResponse, Json, Path, Query, Request, Response, Router, State, StatusCode,
    header, json, routing::get,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind, HoldingId};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

pub struct HoldingAddressTarget {
    pub host: String,
    pub holding_slug: String,
    pub holding_id: HoldingId,
    pub pool: DatabasePool,
}

type Targets = Arc<BTreeMap<String, HoldingAddressTarget>>;

pub fn holding_address_router(targets: Vec<HoldingAddressTarget>) -> Result<Router, BusinessError> {
    let mut registrations = BTreeMap::new();
    for target in targets {
        let host = target.host.clone();
        if host.is_empty()
            || host.parse::<HeaderValue>().is_err()
            || target.holding_slug.is_empty()
            || registrations.insert(host, target).is_some()
        {
            return Err(BusinessError::invalid(
                "HOLDING_ADDRESS_CONFIGURATION_INVALID",
                "Use distinct registered Holding hosts.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/internal/organization/holdings/{holding_slug}/workspace-address",
            get(read),
        )
        .with_state(Arc::new(registrations)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selectors {
    company: Option<String>,
    workspace: Option<String>,
    branch: Option<String>,
    department: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Address {
    company_id: String,
    company_slug: String,
    branch_slug: Option<String>,
    department_slug: Option<String>,
}

async fn read(
    State(registrations): State<Targets>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    let mut hosts = request.headers().get_all(header::HOST).iter();
    let target = hosts
        .next()
        .and_then(|host| host.to_str().ok())
        .and_then(|host| registrations.get(host))
        .filter(|target| target.holding_slug == holding_slug && hosts.next().is_none());
    let Some(target) = target else {
        return missing();
    };
    if request
        .uri()
        .query()
        .is_some_and(|query| query.len() > 1024)
    {
        return missing();
    }
    let Ok(Query(selectors)) = Query::<Selectors>::try_from_uri(request.uri()) else {
        return missing();
    };
    let Ok(query) = HoldingAddressQuery::new(
        selectors.company,
        selectors.workspace,
        selectors.branch,
        selectors.department,
    ) else {
        return missing();
    };

    let holding_id = target.holding_id.clone();
    let result = PgUnitOfWork::new(target.pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            let query = query.clone();
            let holding_id = holding_id.clone();
            Box::pin(async move {
                let mut reader = PgOrganizationReader::new(holding_id, connection);
                let workspace = resolve_holding_address(&mut reader, &query).await?;
                Ok(Address {
                    company_id: workspace.company().id.as_str().to_owned(),
                    company_slug: workspace.company().slug.clone(),
                    branch_slug: workspace.branch().map(|node| node.slug.clone()),
                    department_slug: workspace.department().map(|node| node.slug.clone()),
                })
            })
        })
        .await;

    match result {
        Ok(address) => reply(StatusCode::OK, json!({ "data": address })),
        Err(PgTransactionError::Business(error))
            if matches!(
                error.kind,
                FailureKind::NotFound | FailureKind::InvalidInput
            ) =>
        {
            missing()
        }
        Err(_) => reply(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({ "error": { "code": "HOLDING_ADDRESS_UNAVAILABLE" } }),
        ),
    }
}

fn reply(status: StatusCode, body: serde_json::Value) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(body),
    )
        .into_response()
}

fn missing() -> Response {
    reply(
        StatusCode::NOT_FOUND,
        json!({ "error": { "code": "WORKSPACE_NOT_FOUND" } }),
    )
}
