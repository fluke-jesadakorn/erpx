//! Owner-local HTTP ingress for the native Billing workspace and cost-review
//! commands.
//!
//! Transport facts only: registered Host/Holding target, exact origin,
//! Idempotency-Key, bounded exact JSON input and business-error mapping. The
//! normal Account/Platform session, current Holding authority and serializable
//! Control owner unit remain composition responsibilities supplied through the
//! backend trait. This module never mints a session, actor or Holding scope.
use crate::domain::command::{
    BillingCostCommand, CompleteCostReview, DeleteCost, RecordCost, request_invalid, source_invalid,
};
use crate::domain::workspace::BillingWorkspace;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, get, header, post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, HoldingId, RecordId};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 8 * 1024;

pub type BillingHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct BillingWorkspaceHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

pub struct BillingCommandHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub command: BillingCostCommand,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct BillingHttpResponse {
    /// Exact Billing workspace or `{ok:true}` command receipt.
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

impl BillingHttpResponse {
    pub fn value(self) -> Value {
        json!({ "data": self.data, "context": self.context })
    }
}

/// Composition-owned Account/Platform session, authority and Control unit.
pub trait BillingHttpBackend: Send + Sync {
    fn workspace<'a>(
        &'a self,
        request: BillingWorkspaceHttpRequest<'a>,
    ) -> BillingHttpFuture<'a, BillingHttpResponse>;

    fn command<'a>(
        &'a self,
        request: BillingCommandHttpRequest<'a>,
    ) -> BillingHttpFuture<'a, BillingHttpResponse>;
}

pub struct BillingHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn BillingHttpBackend>,
}

pub struct BillingHttpState {
    targets: BTreeMap<(String, String), BillingHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a BillingHttpTarget,
    host: &'a str,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn command_key_required() -> BusinessError {
    BusinessError::invalid(
        "COMMAND_KEY_REQUIRED",
        "Use a valid Idempotency-Key for this action.",
    )
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty()
            && !rest
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
    })
}

fn record_id(value: &str) -> Result<RecordId, BusinessError> {
    RecordId::new(value).map_err(|_| request_invalid())
}

fn holding(value: &str) -> Result<HoldingId, BusinessError> {
    HoldingId::new(value).map_err(|_| request_invalid())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecordCostBody {
    version: i64,
    reason: String,
    period_id: String,
    cost_id: Option<String>,
    category: String,
    amount_satang: i64,
    service_start: i64,
    service_end: i64,
    note: String,
    #[serde(default)]
    simulated: bool,
    evidence_id: Option<String>,
    original_invoice_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeleteCostBody {
    version: i64,
    reason: String,
    cost_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompleteCostReviewBody {
    version: i64,
    reason: String,
    period_id: String,
    complete: bool,
}

fn record_cost(key: &str, body: RecordCostBody) -> Result<BillingCostCommand, BusinessError> {
    Ok(BillingCostCommand::Record(RecordCost {
        key: CommandKey::new(key)?,
        expected_version: body.version,
        reason: body.reason,
        period_id: record_id(&body.period_id)?,
        cost_id: body.cost_id.as_deref().map(record_id).transpose()?,
        category: body.category,
        amount_satang: body.amount_satang,
        service_start: body.service_start,
        service_end: body.service_end,
        note: body.note,
        simulated: body.simulated,
        evidence_id: body.evidence_id.as_deref().map(record_id).transpose()?,
        original_invoice_id: body
            .original_invoice_id
            .as_deref()
            .map(record_id)
            .transpose()?,
    }))
}

fn delete_cost(key: &str, body: DeleteCostBody) -> Result<BillingCostCommand, BusinessError> {
    Ok(BillingCostCommand::Delete(DeleteCost {
        key: CommandKey::new(key)?,
        expected_version: body.version,
        reason: body.reason,
        cost_id: record_id(&body.cost_id)?,
    }))
}

fn complete_cost_review(
    key: &str,
    body: CompleteCostReviewBody,
) -> Result<BillingCostCommand, BusinessError> {
    Ok(BillingCostCommand::Complete(CompleteCostReview {
        key: CommandKey::new(key)?,
        expected_version: body.version,
        reason: body.reason,
        period_id: record_id(&body.period_id)?,
        complete: body.complete,
    }))
}

fn secured(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn json_response(status: StatusCode, value: Value) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    secured(response)
}

fn error_response(error: BusinessError) -> Response {
    let status = match error.kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    json_response(
        status,
        json!({"error": {"code": error.code, "message": error.message}}),
    )
}

impl BillingHttpState {
    pub fn new(targets: Vec<BillingHttpTarget>) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || target.holding_slug.is_empty()
                || target.holding_slug.len() > 128
                || !target.holding_slug.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
                || !valid_origin(&target.origin)
                || configured
                    .insert((target.host.clone(), target.holding_slug.clone()), target)
                    .is_some()
            {
                return Err(BusinessError::invalid(
                    "BILLING_HTTP_CONFIGURATION_INVALID",
                    "Use exact distinct registered Holding targets.",
                ));
            }
        }
        Ok(Self {
            targets: configured,
        })
    }

    fn target<'a>(
        &'a self,
        headers: &'a HeaderMap,
        holding_slug: &str,
    ) -> Result<ParsedTarget<'a>, BusinessError> {
        let host = unique_header(headers, "host")?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let target = self
            .targets
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .ok_or_else(missing)?;
        let origin = unique_header(headers, "origin")?;
        if origin.is_some_and(|value| value != target.origin) {
            return Err(origin_forbidden());
        }
        Ok(ParsedTarget { target, host })
    }

    fn content_type(headers: &HeaderMap) -> Result<(), BusinessError> {
        let content_type = unique_header(headers, "content-type")?
            .ok_or_else(request_invalid)?
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if content_type != "application/json" {
            return Err(request_invalid());
        }
        Ok(())
    }

    fn command_key(headers: &HeaderMap) -> Result<CommandKey, BusinessError> {
        let key = unique_header(headers, "idempotency-key")?
            .filter(|value| !value.is_empty())
            .ok_or_else(command_key_required)?;
        CommandKey::new(key)
    }

    /// Exact GET handler used by both the Router and owner tests.
    pub async fn workspace(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
    ) -> Response {
        match self.workspace_inner(headers, holding_slug, query).await {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn workspace_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
    ) -> Result<Response, BusinessError> {
        if query.is_some() {
            return Err(request_invalid());
        }
        let target = self.target(headers, holding_slug)?;
        let response = target
            .target
            .backend
            .workspace(BillingWorkspaceHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
            })
            .await?;
        let data: BillingWorkspace =
            serde_json::from_value(response.data).map_err(|_| source_invalid())?;
        if !data.sandbox || data.now < 0 {
            return Err(source_invalid());
        }
        Ok(json_response(
            StatusCode::OK,
            json!({"data": data, "context": response.context}),
        ))
    }

    pub async fn record_cost(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .command(headers, holding_slug, query, body, CommandBody::Record)
            .await
        {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    pub async fn delete_cost(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .command(headers, holding_slug, query, body, CommandBody::Delete)
            .await
        {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    pub async fn complete_costs(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .command(headers, holding_slug, query, body, CommandBody::Complete)
            .await
        {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn command(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
        kind: CommandBody,
    ) -> Result<Response, BusinessError> {
        if query.is_some() {
            return Err(request_invalid());
        }
        let target = self.target(headers, holding_slug)?;
        Self::content_type(headers)?;
        let key = Self::command_key(headers)?;
        if body.len() > BODY_LIMIT {
            return Err(request_invalid());
        }
        let command = match kind {
            CommandBody::Record => record_cost(
                key.as_str(),
                serde_json::from_slice::<RecordCostBody>(body).map_err(|_| request_invalid())?,
            )?,
            CommandBody::Delete => delete_cost(
                key.as_str(),
                serde_json::from_slice::<DeleteCostBody>(body).map_err(|_| request_invalid())?,
            )?,
            CommandBody::Complete => complete_cost_review(
                key.as_str(),
                serde_json::from_slice::<CompleteCostReviewBody>(body)
                    .map_err(|_| request_invalid())?,
            )?,
        };
        // Validate the exact request and current Holding identifier before the
        // backend sees it. The application repeats the policy in its own unit.
        command.validate()?;
        let _ = holding(target.target.holding_slug.as_str())?;
        let response = target
            .target
            .backend
            .command(BillingCommandHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                command,
                headers,
            })
            .await?;
        if response.data != json!({"ok": true}) {
            return Err(source_invalid());
        }
        Ok(json_response(
            StatusCode::OK,
            json!({"data": {"ok": true}, "context": response.context}),
        ))
    }
}

#[derive(Clone, Copy)]
enum CommandBody {
    Record,
    Delete,
    Complete,
}

async fn workspace_handler(
    State(state): State<Arc<BillingHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    state
        .workspace(request.headers(), &holding_slug, query.as_deref())
        .await
}

async fn command_handler(
    state: Arc<BillingHttpState>,
    holding_slug: String,
    request: Request,
    kind: CommandBody,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    match kind {
        CommandBody::Record => {
            state
                .record_cost(&headers, &holding_slug, query.as_deref(), &body)
                .await
        }
        CommandBody::Delete => {
            state
                .delete_cost(&headers, &holding_slug, query.as_deref(), &body)
                .await
        }
        CommandBody::Complete => {
            state
                .complete_costs(&headers, &holding_slug, query.as_deref(), &body)
                .await
        }
    }
}

async fn record_handler(
    State(state): State<Arc<BillingHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    command_handler(state, holding_slug, request, CommandBody::Record).await
}

async fn delete_handler(
    State(state): State<Arc<BillingHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    command_handler(state, holding_slug, request, CommandBody::Delete).await
}

async fn complete_handler(
    State(state): State<Arc<BillingHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    command_handler(state, holding_slug, request, CommandBody::Complete).await
}

pub fn billing_router(targets: Vec<BillingHttpTarget>) -> Result<Router, BusinessError> {
    let state = Arc::new(BillingHttpState::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/billing",
            get(workspace_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/billing/costs/record",
            post(record_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/billing/costs/delete",
            post(delete_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/billing/costs/complete",
            post(complete_handler),
        )
        .with_state(state))
}
