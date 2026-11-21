//! Owner-local HTTP ingress for the native Billing invoice lifecycle.
//!
//! Paths stay inside the existing Management Billing API space
//! (`/api/v1/holdings/{holdingSlug}/billing/...`) and reuse the retained
//! response envelope `{data, context}` and error envelope
//! `{error:{code,message}}`. Transport facts only: registered Host/Holding
//! target, exact origin, Idempotency-Key, bounded exact JSON input and
//! business-error mapping. Account/Platform session, current Holding authority
//! and the serializable Control unit remain composition responsibilities
//! supplied through the backend trait.
use crate::domain::invoice::{
    BillingInvoiceCommand, CreateInvoice, FinalizeInvoice, UpdateInvoice, VoidInvoice,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 8 * 1024;

pub type BillingInvoiceHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct BillingInvoiceHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub command: BillingInvoiceCommand,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct BillingInvoiceHttpResponse {
    /// Exact `{ok:true}` command receipt.
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

/// Composition owns the session, authority and serializable Control unit.
pub trait BillingInvoiceHttpBackend: Send + Sync {
    fn command<'a>(
        &'a self,
        request: BillingInvoiceHttpRequest<'a>,
    ) -> BillingInvoiceHttpFuture<'a, BillingInvoiceHttpResponse>;
}

pub struct BillingInvoiceHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn BillingInvoiceHttpBackend>,
}

pub struct BillingInvoiceHttpState {
    targets: BTreeMap<(String, String), BillingInvoiceHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a BillingInvoiceHttpTarget,
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

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "BILLING_INVOICE_REQUEST_INVALID",
        "Use one bounded Billing invoice request.",
    )
}

fn command_key_required() -> BusinessError {
    BusinessError::invalid(
        "COMMAND_KEY_REQUIRED",
        "Use a valid Idempotency-Key for this action.",
    )
}

fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BILLING_INVOICE_SOURCE_INVALID",
        "The native Billing invoice response is unavailable.",
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
    }) || origin
        .strip_prefix("http://127.0.0.1:")
        .is_some_and(|port| {
            !port.is_empty() && port.len() <= 5 && port.bytes().all(|byte| byte.is_ascii_digit())
        })
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

#[derive(Clone, Copy)]
enum Kind {
    Create,
    Update,
    Finalize,
    Void,
}

impl Kind {
    fn action(self) -> &'static str {
        match self {
            Self::Create => "create-invoice",
            Self::Update => "update-invoice",
            Self::Finalize => "finalize-invoice",
            Self::Void => "void-invoice",
        }
    }

    fn accepts(self, action: &str) -> bool {
        match self {
            Self::Create => matches!(action, "create-invoice" | "approve"),
            Self::Void => matches!(action, "void-invoice" | "void"),
            _ => action == self.action(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateBody {
    version: i64,
    reason: String,
    period_id: String,
    #[serde(default)]
    action: Option<String>,
    #[serde(default = "default_finalize")]
    finalize: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InvoiceBody {
    version: i64,
    reason: String,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    invoice_id: Option<String>,
}

fn default_finalize() -> bool {
    true
}

fn action_ok(body: Option<&str>, kind: Kind) -> Result<(), BusinessError> {
    match body {
        Some(value) if !kind.accepts(value) => Err(request_invalid()),
        _ => Ok(()),
    }
}

fn invoice_id(value: &str) -> Result<RecordId, BusinessError> {
    RecordId::new(value).map_err(|_| request_invalid())
}

impl BillingInvoiceHttpState {
    pub fn new(targets: Vec<BillingInvoiceHttpTarget>) -> Result<Self, BusinessError> {
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
                    "BILLING_INVOICE_HTTP_CONFIGURATION_INVALID",
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
        let host = unique_header(headers, "host")?.ok_or_else(missing)?;
        let target = self
            .targets
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .ok_or_else(missing)?;
        if unique_header(headers, "origin")?.is_some_and(|value| value != target.origin) {
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

    pub async fn create(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .dispatch(headers, holding_slug, query, body, Kind::Create, None)
            .await
        {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    pub async fn update(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        invoice: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .dispatch(
                headers,
                holding_slug,
                query,
                body,
                Kind::Update,
                Some(invoice),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    pub async fn finalize(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        invoice: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .dispatch(
                headers,
                holding_slug,
                query,
                body,
                Kind::Finalize,
                Some(invoice),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    pub async fn void(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        invoice: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .dispatch(
                headers,
                holding_slug,
                query,
                body,
                Kind::Void,
                Some(invoice),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn dispatch(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
        kind: Kind,
        invoice: Option<&str>,
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
        let command = if matches!(kind, Kind::Create) {
            let parsed =
                serde_json::from_slice::<CreateBody>(body).map_err(|_| request_invalid())?;
            action_ok(parsed.action.as_deref(), kind)?;
            BillingInvoiceCommand::Create(CreateInvoice {
                key,
                expected_version: parsed.version,
                reason: parsed.reason,
                period_id: invoice_id(&parsed.period_id)?,
                finalize: parsed.finalize,
            })
        } else {
            let parsed =
                serde_json::from_slice::<InvoiceBody>(body).map_err(|_| request_invalid())?;
            action_ok(parsed.action.as_deref(), kind)?;
            let path_id = invoice_id(invoice.ok_or_else(request_invalid)?)?;
            if parsed
                .invoice_id
                .as_deref()
                .is_some_and(|value| value != path_id.as_str())
            {
                return Err(request_invalid());
            }
            match kind {
                Kind::Update => BillingInvoiceCommand::Update(UpdateInvoice {
                    key,
                    expected_version: parsed.version,
                    reason: parsed.reason,
                    invoice_id: path_id,
                }),
                Kind::Finalize => BillingInvoiceCommand::Finalize(FinalizeInvoice {
                    key,
                    expected_version: parsed.version,
                    reason: parsed.reason,
                    invoice_id: path_id,
                }),
                Kind::Void => BillingInvoiceCommand::Void(VoidInvoice {
                    key,
                    expected_version: parsed.version,
                    reason: parsed.reason,
                    invoice_id: path_id,
                }),
                Kind::Create => unreachable!(),
            }
        };
        // Validate the exact bounded request before the backend sees it. The
        // application repeats policy and owner-source checks in its own unit.
        command.validate()?;
        let response = target
            .target
            .backend
            .command(BillingInvoiceHttpRequest {
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

async fn create_handler(
    State(state): State<Arc<BillingInvoiceHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .create(&headers, &holding_slug, query.as_deref(), &body)
        .await
}

async fn update_handler(
    State(state): State<Arc<BillingInvoiceHttpState>>,
    Path((holding_slug, invoice)): Path<(String, String)>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .update(&headers, &holding_slug, &invoice, query.as_deref(), &body)
        .await
}

async fn finalize_handler(
    State(state): State<Arc<BillingInvoiceHttpState>>,
    Path((holding_slug, invoice)): Path<(String, String)>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .finalize(&headers, &holding_slug, &invoice, query.as_deref(), &body)
        .await
}

async fn void_handler(
    State(state): State<Arc<BillingInvoiceHttpState>>,
    Path((holding_slug, invoice)): Path<(String, String)>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .void(&headers, &holding_slug, &invoice, query.as_deref(), &body)
        .await
}

pub fn billing_invoice_router(
    targets: Vec<BillingInvoiceHttpTarget>,
) -> Result<Router, BusinessError> {
    let state = Arc::new(BillingInvoiceHttpState::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/billing/invoices",
            post(create_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/billing/invoices/{invoice_id}/update",
            post(update_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/billing/invoices/{invoice_id}/finalize",
            post(finalize_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/billing/invoices/{invoice_id}/void",
            post(void_handler),
        )
        .with_state(state))
}
