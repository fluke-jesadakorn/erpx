//! Owner-local HTTP ingress for native Projects milestone commands.
//!
//! The router validates exact Host/Holding targets, Origin, Idempotency-Key,
//! media type, query and body bounds, parses camel-case command input, then
//! forwards only transport facts and normalized owner input. Account/session,
//! Company scope, purchase admission and the serializable owner unit remain
//! composition responsibilities supplied through the backend trait.
use crate::domain::project_commands::{
    MilestoneAction, SaveMilestoneCommand, TransitionMilestoneCommand,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::Deserialize;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;
const MAX_TIMESTAMP: i64 = 253_402_300_799_999;

pub type ProjectCommandHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct ProjectMilestoneSaveHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub project_id: &'a str,
    pub milestone_id: Option<&'a str>,
    pub key: &'a str,
    pub headers: &'a HeaderMap,
    pub input: &'a SaveMilestoneCommand,
}

pub struct ProjectMilestoneTransitionHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub project_id: &'a str,
    pub milestone_id: &'a str,
    pub key: &'a str,
    pub headers: &'a HeaderMap,
    pub input: &'a TransitionMilestoneCommand,
}

#[derive(Clone, Debug)]
pub struct ProjectCommandHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl ProjectCommandHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

pub trait ProjectMilestoneCommandHttpBackend: Send + Sync {
    fn save<'a>(
        &'a self,
        request: ProjectMilestoneSaveHttpRequest<'a>,
    ) -> ProjectCommandHttpFuture<'a, ProjectCommandHttpResponse>;
    fn transition<'a>(
        &'a self,
        request: ProjectMilestoneTransitionHttpRequest<'a>,
    ) -> ProjectCommandHttpFuture<'a, ProjectCommandHttpResponse>;
}

pub struct ProjectCommandHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn ProjectMilestoneCommandHttpBackend>,
}

struct ProjectCommandHttpState {
    targets: BTreeMap<(String, String), ProjectCommandHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PROJECTS_REQUEST_INVALID",
        "Use one unambiguous Projects command request.",
    )
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn command_key_required() -> BusinessError {
    BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
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

pub fn project_milestone_commands_router(
    targets: Vec<ProjectCommandHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "PROJECTS_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/projects/{project_id}/milestones",
            post(create_milestone),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/projects/{project_id}/milestones/{milestone_id}",
            post(revise_milestone),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/projects/{project_id}/milestones/{milestone_id}/transition",
            post(transition_milestone),
        )
        .with_state(Arc::new(ProjectCommandHttpState {
            targets: configured,
        })))
}

fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn error_response(error: BusinessError) -> Response {
    (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(value: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
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

fn target<'a>(
    state: &'a ProjectCommandHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a ProjectCommandHttpTarget, BusinessError> {
    let host = unique_header(headers, "host")?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn command_facts<'a>(
    target: &'a ProjectCommandHttpTarget,
    headers: &'a HeaderMap,
    key: Option<&'a str>,
) -> Result<&'a str, BusinessError> {
    let key = key.ok_or_else(command_key_required)?;
    let origin = unique_header(headers, "origin")?.ok_or_else(origin_forbidden)?;
    if origin != target.origin {
        return Err(origin_forbidden());
    }
    let content_type = unique_header(headers, "content-type")?
        .ok_or_else(request_invalid)?
        .split(';')
        .next()
        .unwrap_or_default()
        .trim();
    if content_type != "application/json" {
        return Err(request_invalid());
    }
    Ok(key)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SaveMilestoneBody {
    title: String,
    #[serde(default)]
    description: String,
    due_at: String,
    owner_user_id: String,
    #[serde(default)]
    parent_milestone_id: Option<Option<String>>,
    #[serde(default)]
    ordinal_position: Option<i64>,
    #[serde(default)]
    depends_on_milestone_ids: Option<Vec<String>>,
    #[serde(default)]
    expected_version: Option<i64>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TransitionMilestoneBody {
    action: String,
    expected_version: i64,
    reason: String,
}

impl SaveMilestoneBody {
    fn command(self) -> Result<SaveMilestoneCommand, BusinessError> {
        Ok(SaveMilestoneCommand {
            title: self.title.trim().to_owned(),
            description: self.description.trim().to_owned(),
            due_at: parse_timestamp(&self.due_at).ok_or_else(|| {
                BusinessError::invalid(
                    "PROJECT_MILESTONE_INVALID",
                    "Use an ISO-8601 due time with an explicit offset.",
                )
            })?,
            owner_user_id: self.owner_user_id,
            parent_supplied: self.parent_milestone_id.is_some(),
            parent_milestone_id: self.parent_milestone_id.flatten(),
            ordinal_position: self.ordinal_position,
            depends_on_milestone_ids: self.depends_on_milestone_ids,
            expected_version: self.expected_version.unwrap_or(1),
            reason: self
                .reason
                .unwrap_or_else(|| "Milestone details reviewed by the project owner.".into())
                .trim()
                .to_owned(),
        })
    }
}

impl TransitionMilestoneBody {
    fn command(self) -> Result<TransitionMilestoneCommand, BusinessError> {
        Ok(TransitionMilestoneCommand {
            action: MilestoneAction::parse(&self.action)?,
            expected_version: self.expected_version,
            reason: self.reason.trim().to_owned(),
        })
    }
}

async fn body<T: for<'de> Deserialize<'de>>(request: Request) -> Result<T, BusinessError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| request_invalid())?;
    serde_json::from_slice(&bytes).map_err(|_| request_invalid())
}

async fn create_milestone(
    State(state): State<Arc<ProjectCommandHttpState>>,
    Path((slug, project_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    save(state, slug, project_id, None, request).await
}

async fn revise_milestone(
    State(state): State<Arc<ProjectCommandHttpState>>,
    Path((slug, project_id, milestone_id)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    save(state, slug, project_id, Some(milestone_id), request).await
}

async fn save(
    state: Arc<ProjectCommandHttpState>,
    slug: String,
    project_id: String,
    milestone_id: Option<String>,
    request: Request,
) -> Response {
    let result = async move {
        if request.uri().query().is_some() {
            return Err(request_invalid());
        }
        let target = target(&state, request.headers(), &slug)?;
        let key = unique_header(request.headers(), "idempotency-key")?;
        let key = command_facts(target, request.headers(), key)?.to_owned();
        let headers = request.headers().clone();
        let project_id = RecordId::new(project_id).map_err(|_| request_invalid())?;
        let milestone_id = milestone_id
            .map(RecordId::new)
            .transpose()
            .map_err(|_| request_invalid())?;
        let input: SaveMilestoneBody = body(request).await?;
        let input = input.command()?;
        target
            .backend
            .save(ProjectMilestoneSaveHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                project_id: project_id.as_str(),
                milestone_id: milestone_id.as_ref().map(RecordId::as_str),
                key: &key,
                headers: &headers,
                input: &input,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn transition_milestone(
    State(state): State<Arc<ProjectCommandHttpState>>,
    Path((slug, project_id, milestone_id)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        if request.uri().query().is_some() {
            return Err(request_invalid());
        }
        let target = target(&state, request.headers(), &slug)?;
        let key = unique_header(request.headers(), "idempotency-key")?;
        let key = command_facts(target, request.headers(), key)?.to_owned();
        let headers = request.headers().clone();
        let project_id = RecordId::new(project_id).map_err(|_| request_invalid())?;
        let milestone_id = RecordId::new(milestone_id).map_err(|_| request_invalid())?;
        let input: TransitionMilestoneBody = body(request).await?;
        let input = input.command()?;
        target
            .backend
            .transition(ProjectMilestoneTransitionHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                project_id: project_id.as_str(),
                milestone_id: milestone_id.as_str(),
                key: &key,
                headers: &headers,
                input: &input,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

fn parse_timestamp(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || !matches!(bytes.get(10), Some(&b'T') | Some(&b't'))
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return None;
    }
    let year = digits(bytes, 0, 4)?;
    let month = digits(bytes, 5, 2)?;
    let day = digits(bytes, 8, 2)?;
    let hour = digits(bytes, 11, 2)?;
    let minute = digits(bytes, 14, 2)?;
    let second = digits(bytes, 17, 2)?;
    if !(1..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let mut index = 19;
    let mut millis = 0_i64;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let start = index;
        while bytes.get(index).is_some_and(|byte| byte.is_ascii_digit()) {
            index += 1;
        }
        let count = index - start;
        if count == 0 || count > 9 {
            return None;
        }
        let fraction = digits(bytes, start, count)?;
        millis = fraction * 10_i64.pow((9 - count) as u32) / 1_000_000;
    }
    let offset_minutes = match bytes.get(index) {
        Some(b'Z' | b'z') => {
            index += 1;
            0_i64
        }
        Some(&sign) if sign == b'+' || sign == b'-' => {
            let hours = digits(bytes, index + 1, 2)?;
            if bytes.get(index + 3) != Some(&b':') {
                return None;
            }
            let minutes = digits(bytes, index + 4, 2)?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            index += 6;
            let value = i64::from(hours) * 60 + i64::from(minutes);
            if sign == b'+' { value } else { -value }
        }
        _ => return None,
    };
    if index != bytes.len() {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    let local = days
        .checked_mul(86_400_000)?
        .checked_add(i64::from(hour) * 3_600_000)?
        .checked_add(i64::from(minute) * 60_000)?
        .checked_add(i64::from(second) * 1_000)?
        .checked_add(millis)?;
    let utc = local.checked_sub(offset_minutes.checked_mul(60_000)?)?;
    (0..=MAX_TIMESTAMP).contains(&utc).then_some(utc)
}

fn digits(bytes: &[u8], start: usize, count: usize) -> Option<i64> {
    let end = start.checked_add(count)?;
    let slice = bytes.get(start..end)?;
    let mut value = 0_i64;
    for byte in slice {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(i64::from(*byte - b'0'))?;
    }
    Some(value)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian civil date.
fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = (month + 9) % 12;
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era.checked_mul(146_097)?.checked_add(day_of_era)? - 719_468)
}
