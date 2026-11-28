//! Native owner read for the retained Benefits executive evidence surface.
//!
//! The owner reads only its own plan/election facts. Current Account/IAM
//! admission, Company selection, package history and source revalidation stay
//! in composition; the Runtime owns the access audit on the same transaction.
//! No TypeScript adapter, SQL fallback or private row projection participates.
use crate::domain::workspace::BenefitsActor;
use base64::Engine;
use erpx_platform_runtime_server::public::executive_access::EvidenceAccessRow;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};

const PROVIDER: &str = "benefits-current-records";
const METRICS: [&str; 2] = ["active-benefit-elections", "published-benefit-plans"];
const FIELDS: [&str; 6] = [
    "id",
    "status",
    "version",
    "updated_at",
    "effective_from",
    "effective_to",
];

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "EVIDENCE_INPUT_INVALID",
        "Use one bounded Benefits executive evidence request.",
    )
}

fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "EVIDENCE_FORBIDDEN",
        "Original Benefits history and current source authority are required.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "EVIDENCE_NOT_FOUND",
        "The original Benefits evidence is unavailable in this role and source.",
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BenefitEvidenceAction {
    Search,
    Open,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BenefitEvidenceFilters {
    #[serde(default = "default_from")]
    pub from: String,
    #[serde(default = "default_to")]
    pub to: String,
    #[serde(default = "default_currency")]
    pub currency: String,
}

fn default_from() -> String {
    "2026-01-01".into()
}
fn default_to() -> String {
    "2026-12-31".into()
}
fn default_currency() -> String {
    "THB".into()
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BenefitEvidenceRequest {
    pub metric: String,
    pub filters: BenefitEvidenceFilters,
    pub query_hash: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub record_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenefitEvidenceRow {
    pub id: String,
    pub label: String,
    pub summary: String,
    pub status: String,
    pub version: i64,
    pub href: String,
}

#[derive(Debug, Serialize)]
pub struct BenefitEvidenceRecord {
    pub id: String,
    pub status: String,
    pub version: i64,
    pub updated_at: i64,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub href: String,
}

pub struct BenefitEvidenceResult {
    pub data: Value,
    pub rows: Vec<EvidenceAccessRow>,
}

fn valid_date(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn cursor_id(value: Option<&str>) -> Result<Option<String>, BusinessError> {
    let Some(value) = value else { return Ok(None) };
    if value.is_empty() || value.len() > 2000 {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "EVIDENCE_CURSOR_INVALID",
            "The Benefits evidence cursor is invalid. Refresh this list.",
        ));
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Cursor {
        id: String,
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| {
            BusinessError::new(
                FailureKind::InvalidInput,
                "EVIDENCE_CURSOR_INVALID",
                "The Benefits evidence cursor is invalid. Refresh this list.",
            )
        })?;
    let cursor: Cursor = serde_json::from_slice(&decoded).map_err(|_| {
        BusinessError::new(
            FailureKind::InvalidInput,
            "EVIDENCE_CURSOR_INVALID",
            "The Benefits evidence cursor is invalid. Refresh this list.",
        )
    })?;
    if cursor.id.is_empty() || cursor.id.len() > 200 {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "EVIDENCE_CURSOR_INVALID",
            "The Benefits evidence cursor is invalid. Refresh this list.",
        ));
    }
    Ok(Some(cursor.id))
}

fn href(metric: &str, id: &str) -> String {
    if metric == "active-benefit-elections" {
        format!("/benefits/{}", urlencode(id))
    } else {
        format!("/benefits/records/benefit_plan/{}", urlencode(id))
    }
}

fn urlencode(value: &str) -> String {
    let mut result = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            result.push(byte as char);
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}

/// Owner read and projection. `assignment_id` is rechecked by the owner against
/// the selected Company; `person_id` is the current worker for self visibility
/// and `reviewer` grants the retained reviewer scope.
pub async fn read_benefit_evidence(
    connection: &mut PgConnection,
    actor: &BenefitsActor,
    assignment_id: &str,
    person_id: Option<&str>,
    reviewer: bool,
    history_allowed: bool,
    action: BenefitEvidenceAction,
    input: BenefitEvidenceRequest,
) -> Result<BenefitEvidenceResult, BusinessError> {
    if !METRICS.contains(&input.metric.as_str())
        || !valid_date(&input.filters.from)
        || !valid_date(&input.filters.to)
        || input.filters.from > input.filters.to
        || input.filters.currency.len() != 3
        || !input
            .filters
            .currency
            .bytes()
            .all(|byte| byte.is_ascii_uppercase())
        || !valid_hash(&input.query_hash)
        || input.query.encode_utf16().count() > 100
    {
        return Err(invalid());
    }
    if !actor.modules.iter().any(|module| module == "benefits")
        || !actor.permissions.iter().any(|permission| permission == "records.read")
    {
        return Err(forbidden());
    }
    if !history_allowed {
        return Err(forbidden());
    }
    let record_id = match action {
        BenefitEvidenceAction::Search if input.record_id.is_some() => return Err(invalid()),
        BenefitEvidenceAction::Search => None,
        BenefitEvidenceAction::Open => match input.record_id.as_deref() {
            Some(id) if !id.is_empty() => Some(id.to_owned()),
            _ => return Err(invalid()),
        },
    };
    let cursor = cursor_id(input.cursor.as_deref())?;
    let page_limit = input.limit.unwrap_or(25);
    if page_limit < 1 || page_limit > 50 {
        return Err(invalid());
    }
    let take = if record_id.is_some() {
        1
    } else {
        page_limit as i64 + 1
    };
    let election = input.metric == "active-benefit-elections";
    let mut sql = if election {
        String::from(
            "SELECT e.id,e.status,e.version,e.updated_at,e.effective_from,e.effective_to \
             FROM benefit_elections e \
             JOIN benefit_plans p ON p.id=e.plan_id AND p.company_id=e.company_id \
             WHERE e.company_id=$1 AND e.status<>'cancelled' \
             AND e.effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
             AND (e.effective_to IS NULL OR e.effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) \
             AND ($2::bool OR e.person_id=$3) \
             AND EXISTS(SELECT 1 FROM assignments a WHERE a.id=$4 AND a.company_id=e.company_id)",
        )
    } else {
        String::from(
            "SELECT p.id,p.status,p.version,p.updated_at,p.effective_from,p.effective_to \
             FROM benefit_plans p \
             WHERE p.company_id=$1 AND p.status='published' \
             AND p.effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
             AND (p.effective_to IS NULL OR p.effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) \
             AND EXISTS(SELECT 1 FROM assignments a WHERE a.id=$4 AND a.company_id=p.company_id)",
        )
    };
    let alias = if election { 'e' } else { 'p' };
    if action == BenefitEvidenceAction::Search && !input.query.is_empty() {
        sql.push_str(&format!(
            " AND strpos(lower({alias}.status||' '||{alias}.id),lower($5))>0"
        ));
    }
    if cursor.is_some() {
        sql.push_str(&format!(" AND {alias}.id>$6"));
    }
    if record_id.is_some() {
        sql.push_str(&format!(" AND {alias}.id=$7"));
    }
    sql.push_str(&format!(" ORDER BY {alias}.id COLLATE \"C\" LIMIT $8"));
    if !reviewer && person_id.is_none() && election {
        return Ok(BenefitEvidenceResult {
            data: search_empty(&input.metric, &input.query_hash),
            rows: Vec::new(),
        });
    }
    let statement = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(actor.company_id.as_str())
        .bind(reviewer)
        .bind(person_id)
        .bind(assignment_id)
        .bind(input.query.as_str())
        .bind(cursor.as_deref())
        .bind(record_id.as_deref())
        .bind(take);
    let fetched = statement
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| invalid())?;
    let mut decoded = Vec::new();
    for row in fetched {
        let id: String = row.try_get(0).map_err(|_| invalid())?;
        let status: String = row.try_get(1).map_err(|_| invalid())?;
        let version: i64 = row.try_get(2).map_err(|_| invalid())?;
        let updated_at: i64 = row.try_get(3).map_err(|_| invalid())?;
        let effective_from: String = row.try_get(4).map_err(|_| invalid())?;
        let effective_to: Option<String> = row.try_get(5).map_err(|_| invalid())?;
        if version < 1 {
            return Err(invalid());
        }
        decoded.push((id, status, version, updated_at, effective_from, effective_to));
    }
    if record_id.is_some() && decoded.is_empty() {
        return Err(missing());
    }
    let rows = decoded
        .iter()
        .take(if record_id.is_some() { 1 } else { page_limit as usize })
        .map(|(id, _, version, ..)| EvidenceAccessRow {
            id: id.clone(),
            version: *version,
        })
        .collect::<Vec<_>>();
    let data = match action {
        BenefitEvidenceAction::Search => {
            let has_more = decoded.len() > page_limit as usize;
            let items = decoded
                .iter()
                .take(page_limit as usize)
                .map(|(id, status, version, _, _, _)| BenefitEvidenceRow {
                    label: if election {
                        "Benefit election".into()
                    } else {
                        String::new()
                    },
                    summary: status.clone(),
                    status: status.clone(),
                    version: *version,
                    href: href(&input.metric, id),
                    id: id.clone(),
                })
                .collect::<Vec<_>>();
            // Plan labels require the owner name; fill them in one bounded read.
            let items = if election {
                items
            } else {
                fill_plan_labels(connection, actor.company_id.as_str(), items).await?
            };
            let next_cursor = if has_more {
                items.last().map(|row| {
                    base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .encode(json!({"id": row.id}).to_string())
                })
            } else {
                None
            };
            json!({
                "provider": PROVIDER,
                "metric": input.metric,
                "fields": FIELDS,
                "items": items,
                "hasMore": has_more,
                "nextCursor": next_cursor,
                "queryHash": input.query_hash,
            })
        }
        BenefitEvidenceAction::Open => {
            let (id, status, version, updated_at, effective_from, effective_to) = &decoded[0];
            json!({
                "provider": PROVIDER,
                "metric": input.metric,
                "fields": FIELDS,
                "record": BenefitEvidenceRecord {
                    id: id.clone(), status: status.clone(), version: *version,
                    updated_at: *updated_at, effective_from: effective_from.clone(),
                    effective_to: effective_to.clone(), href: href(&input.metric, id),
                },
                "queryHash": input.query_hash,
            })
        }
    };
    Ok(BenefitEvidenceResult { data, rows })
}

fn search_empty(metric: &str, query_hash: &str) -> Value {
    json!({
        "provider": PROVIDER, "metric": metric, "fields": FIELDS,
        "items": [], "hasMore": false,
        "nextCursor": null, "queryHash": query_hash,
    })
}

async fn fill_plan_labels(
    connection: &mut PgConnection,
    company: &str,
    mut items: Vec<BenefitEvidenceRow>,
) -> Result<Vec<BenefitEvidenceRow>, BusinessError> {
    if items.is_empty() {
        return Ok(items);
    }
    let ids = items.iter().map(|row| row.id.as_str()).collect::<Vec<_>>();
    let placeholders = (1..=ids.len())
        .map(|index| format!("${index}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT id,name FROM benefit_plans WHERE company_id=${} AND id IN ({})",
        ids.len() + 1,
        placeholders
    );
    let mut statement = sqlx::query(sqlx::AssertSqlSafe(sql));
    for id in ids {
        statement = statement.bind(id);
    }
    let statement = statement.bind(company);
    let names = statement
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| invalid())?;
    let mut labels = std::collections::BTreeMap::new();
    for row in names {
        labels.insert(
            row.try_get::<String, _>(0).map_err(|_| invalid())?,
            row.try_get::<String, _>(1).map_err(|_| invalid())?,
        );
    }
    for item in &mut items {
        item.label = labels
            .get(&item.id)
            .cloned()
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "EVIDENCE_NOT_FOUND", "The original Benefits evidence is unavailable in this role and source."))?;
    }
    Ok(items)
}

// Native private HTTP ingress. Transport facts only; the composition-owned
// backend resolves the Account session, current IAM, package history, source
// visibility and the single serializable owner unit.
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post,
};
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::Arc,
};

const BODY_LIMIT: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BenefitsExecutiveEvidenceAction {
    Search,
    Open,
}

impl BenefitsExecutiveEvidenceAction {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "search" => Ok(Self::Search),
            "open" => Ok(Self::Open),
            _ => Err(invalid()),
        }
    }
}

pub type BenefitsExecutiveEvidenceHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct BenefitsExecutiveEvidenceHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub action: BenefitsExecutiveEvidenceAction,
    pub body: &'a Value,
}

#[derive(Clone, Debug)]
pub struct BenefitsExecutiveEvidenceHttpResponse {
    pub data: Value,
    pub context: Value,
}

impl BenefitsExecutiveEvidenceHttpResponse {
    pub fn value(self) -> Value {
        json!({"data": self.data, "context": self.context})
    }
}

pub trait BenefitsExecutiveEvidenceHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: BenefitsExecutiveEvidenceHttpRequest<'a>,
    ) -> BenefitsExecutiveEvidenceHttpFuture<'a, BenefitsExecutiveEvidenceHttpResponse>;
}

pub struct BenefitsExecutiveEvidenceHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn BenefitsExecutiveEvidenceHttpBackend>,
}

struct BenefitsExecutiveEvidenceHttpState {
    targets: BTreeMap<(String, String), BenefitsExecutiveEvidenceHttpTarget>,
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
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
                (1..=5).contains(&port.len())
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|value| value > 0)
            })
    })
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
        Json(json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn target<'a>(
    state: &'a BenefitsExecutiveEvidenceHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a BenefitsExecutiveEvidenceHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))
}

pub fn benefits_executive_evidence_http_router(
    targets: Vec<BenefitsExecutiveEvidenceHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || target.holding_slug.len() > 128
            || !target
                .holding_slug
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(invalid());
        }
    }
    if configured.is_empty() {
        return Err(invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/benefits/executive-evidence/{action}",
            post(run),
        )
        .with_state(Arc::new(BenefitsExecutiveEvidenceHttpState {
            targets: configured,
        })))
}

async fn run(
    State(state): State<Arc<BenefitsExecutiveEvidenceHttpState>>,
    Path((slug, action)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let action = BenefitsExecutiveEvidenceAction::parse(&action)?;
        let target = target(&state, request.headers(), &slug)?;
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        if let Some(origin) = unique_header(request.headers(), HeaderName::from_static("origin"))? {
            if origin != target.origin {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "ORIGIN_FORBIDDEN",
                    "Use the registered application origin.",
                ));
            }
        }
        let headers = request.headers().clone();
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let body: Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if !body.is_object() {
            return Err(invalid());
        }
        target
            .backend
            .execute(BenefitsExecutiveEvidenceHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                action,
                body: &body,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => {
            let mut response = (StatusCode::OK, Json(reply.value())).into_response();
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
        Err(error) => error_response(error),
    }
}
