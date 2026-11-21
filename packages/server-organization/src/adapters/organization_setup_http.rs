//! Owner-local HTTP ingress for Organization setup commands.
//!
//! Transport facts only: registered Host/Holding targets, exact origin,
//! Idempotency-Key, bounded JSON input and business-error mapping. The normal
//! Account session, current Holding authority and serializable owner unit are
//! supplied by the composition backend through `OrganizationSetupHttpBackend`.
//! This module never mints a session, actor or Holding scope.
use crate::domain::organization_setup::{ApplyTemplateInput, TemplateInput};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub type OrganizationSetupHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Value, BusinessError>> + Send + 'a>>;

pub struct InstallLevelsHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a str,
}

pub struct SaveTemplateHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a str,
    pub template_id: Option<&'a str>,
    pub input: &'a TemplateInput,
}

pub struct ApplyTemplateHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a str,
    pub template_id: &'a str,
    pub input: &'a ApplyTemplateInput,
}

/// Composition-owned Account session, current IAM authority and Company unit.
pub trait OrganizationSetupHttpBackend: Send + Sync {
    fn install_levels<'a>(
        &'a self,
        request: InstallLevelsHttpRequest<'a>,
    ) -> OrganizationSetupHttpFuture<'a>;

    fn save_template<'a>(
        &'a self,
        request: SaveTemplateHttpRequest<'a>,
    ) -> OrganizationSetupHttpFuture<'a>;

    fn apply_template<'a>(
        &'a self,
        request: ApplyTemplateHttpRequest<'a>,
    ) -> OrganizationSetupHttpFuture<'a>;
}

pub struct OrganizationSetupHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn OrganizationSetupHttpBackend>,
}

pub struct OrganizationSetupHttpState {
    targets: BTreeMap<(String, String), OrganizationSetupHttpTarget>,
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "VALIDATION_FAILED",
        "Provide a complete organization setup request.",
    )
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

fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_SOURCE_INVALID",
        "The organization command reply was invalid.",
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
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len()) && port.bytes().all(|byte| byte.is_ascii_digit())
            })
    })
}

fn identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= 200 && !value.chars().any(char::is_control)
}

fn integer(value: &Value) -> Option<i64> {
    value.as_i64().filter(|value| *value >= 0)
}

fn result_install(value: &Value) -> Result<Value, BusinessError> {
    let object = value.as_object().ok_or_else(source_invalid)?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(source_invalid)?;
    if !identifier(id) {
        return Err(source_invalid());
    }
    Ok(json!({"id": id}))
}

fn result_template(value: &Value) -> Result<Value, BusinessError> {
    let object = value.as_object().ok_or_else(source_invalid)?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(source_invalid)?;
    let version = object
        .get("version")
        .and_then(integer)
        .ok_or_else(source_invalid)?;
    if !identifier(id) {
        return Err(source_invalid());
    }
    Ok(json!({"id": id, "version": version}))
}

fn result_apply(value: &Value) -> Result<Value, BusinessError> {
    let object = value.as_object().ok_or_else(source_invalid)?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(source_invalid)?;
    let ids = object
        .get("positionIds")
        .and_then(Value::as_array)
        .filter(|values| values.len() <= 100)
        .ok_or_else(source_invalid)?;
    if !identifier(id)
        || !ids
            .iter()
            .all(|value| value.as_str().is_some_and(identifier))
    {
        return Err(source_invalid());
    }
    Ok(json!({"id": id, "positionIds": ids}))
}

fn secured(mut response: Response) -> Response {
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

impl OrganizationSetupHttpState {
    pub fn new(
        targets: Vec<OrganizationSetupHttpTarget>,
    ) -> Result<OrganizationSetupHttpState, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || target.host.len() > 253
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
                    "ORGANIZATION_HTTP_CONFIGURATION_INVALID",
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
    ) -> Result<(&'a OrganizationSetupHttpTarget, &'a str), BusinessError> {
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
        Ok((target, host))
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

    fn body(value: &[u8]) -> Result<Value, BusinessError> {
        if value.len() > BODY_LIMIT {
            return Err(request_invalid());
        }
        serde_json::from_slice(value).map_err(|_| request_invalid())
    }

    pub async fn install_levels(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .install_levels_inner(headers, holding_slug, query, body)
            .await
        {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn install_levels_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Response, BusinessError> {
        if query.is_some() {
            return Err(request_invalid());
        }
        let (target, host) = self.target(headers, holding_slug)?;
        Self::content_type(headers)?;
        let key = Self::command_key(headers)?;
        let value = Self::body(body)?;
        let object = value.as_object().ok_or_else(request_invalid)?;
        if !object.is_empty() {
            return Err(request_invalid());
        }
        let result = target
            .backend
            .install_levels(InstallLevelsHttpRequest {
                host,
                holding_slug: &target.holding_slug,
                headers,
                key: key.as_str(),
            })
            .await?;
        Ok(json_response(
            StatusCode::OK,
            json!({"data": result_install(&result)?}),
        ))
    }

    pub async fn save_template(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        template_id: Option<&str>,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .save_template_inner(headers, holding_slug, template_id, query, body)
            .await
        {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn save_template_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        template_id: Option<&str>,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Response, BusinessError> {
        if query.is_some() {
            return Err(request_invalid());
        }
        let (target, host) = self.target(headers, holding_slug)?;
        Self::content_type(headers)?;
        let key = Self::command_key(headers)?;
        let value = Self::body(body)?;
        let input = TemplateInput::parse(&value)?;
        if template_id.is_some_and(|id| !identifier(id)) {
            return Err(request_invalid());
        }
        let result = target
            .backend
            .save_template(SaveTemplateHttpRequest {
                host,
                holding_slug: &target.holding_slug,
                headers,
                key: key.as_str(),
                template_id,
                input: &input,
            })
            .await?;
        Ok(json_response(
            StatusCode::OK,
            json!({"data": result_template(&result)?}),
        ))
    }

    pub async fn apply_template(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        template_id: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self
            .apply_template_inner(headers, holding_slug, template_id, query, body)
            .await
        {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn apply_template_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        template_id: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Response, BusinessError> {
        if query.is_some() {
            return Err(request_invalid());
        }
        let (target, host) = self.target(headers, holding_slug)?;
        Self::content_type(headers)?;
        let key = Self::command_key(headers)?;
        if !identifier(template_id) {
            return Err(request_invalid());
        }
        let value = Self::body(body)?;
        let input = ApplyTemplateInput::parse(&value)?;
        let result = target
            .backend
            .apply_template(ApplyTemplateHttpRequest {
                host,
                holding_slug: &target.holding_slug,
                headers,
                key: key.as_str(),
                template_id,
                input: &input,
            })
            .await?;
        Ok(json_response(
            StatusCode::OK,
            json!({"data": result_apply(&result)?}),
        ))
    }
}

/// Router is mounted by the composition root after it implements the backend
/// with the normal Account session, current IAM authority and owner unit.
pub fn organization_setup_router(
    targets: Vec<OrganizationSetupHttpTarget>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/people/organization/levels",
            post(install_levels_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/people/organization/templates",
            post(save_template_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/people/organization/templates/{template_id}",
            post(update_template_handler),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/people/organization/templates/{template_id}/apply",
            post(apply_template_handler),
        )
        .with_state(Arc::new(OrganizationSetupHttpState::new(targets)?)))
}

async fn install_levels_handler(
    State(state): State<Arc<OrganizationSetupHttpState>>,
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
        .install_levels(&headers, &holding_slug, query.as_deref(), &body)
        .await
}

async fn save_template_handler(
    State(state): State<Arc<OrganizationSetupHttpState>>,
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
        .save_template(&headers, &holding_slug, None, query.as_deref(), &body)
        .await
}

async fn update_template_handler(
    State(state): State<Arc<OrganizationSetupHttpState>>,
    Path((holding_slug, template_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .save_template(
            &headers,
            &holding_slug,
            Some(&template_id),
            query.as_deref(),
            &body,
        )
        .await
}

async fn apply_template_handler(
    State(state): State<Arc<OrganizationSetupHttpState>>,
    Path((holding_slug, template_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .apply_template(
            &headers,
            &holding_slug,
            &template_id,
            query.as_deref(),
            &body,
        )
        .await
}
