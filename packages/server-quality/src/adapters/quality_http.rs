//! Native Quality HTTP ingress.
//!
//! Transport facts only: exact registered Host/Holding targets, origin,
//! idempotency, bounded JSON and error mapping. Normal Account session
//! resolution, current Company/branch/department scope, Commercial purchase
//! admission and the serializable owner unit belong to the composition
//! backend, which must implement [`QualityHttpBackend`].
use crate::application::quality::{
    QualityAlert, QualityCancellation, QualityCancellationCommand, QualityCancellationReceipt,
    QualityDetail, QualityDetailCore, QualityDisposition, QualityHold, QualityInspection,
    QualityMeasurement, QualityWorkspace,
};
use crate::domain::quality::{
    CancellationAction, MAX_SAFE_INTEGER, QualityDecision, QualityMetric, validate_reason,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, Value,
    body::to_bytes,
    header, json,
    routing::{get, post},
    serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub type QualityHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct QualityHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub id: Option<&'a str>,
}

pub struct QualityHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub id: &'a str,
    pub body: &'a Value,
    pub key: &'a CommandKey,
}

/// The query is intentionally left unparsed until after selected Account and
/// Company authorization in the composition backend.
pub struct QualityHttpHistoryRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub id: &'a str,
    pub raw_query: Option<&'a str>,
}

pub struct QualityHistoryCursor {
    pub before: Option<i64>,
    pub head: Option<i64>,
    pub revision: Option<String>,
}

pub struct QualityHttpHistoryReply {
    pub items: Vec<QualityCancellation>,
    pub next_before: Option<i64>,
    pub head_version: i64,
    pub history_revision: String,
}

pub enum QualityHttpReadReply {
    Workspace(Box<QualityWorkspace>),
    Detail(Box<QualityDetail>),
}

/// Composition-owned session, scope and owner-unit adapter. The router never
/// mints an actor, a Company or Quality permission.
pub trait QualityHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: QualityHttpReadRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityHttpReadReply>;
    fn cancel<'a>(
        &'a self,
        request: QualityHttpCommandRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityCancellationReceipt>;
    fn history<'a>(
        &'a self,
        _request: QualityHttpHistoryRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityHttpHistoryReply> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "QUALITY_HISTORY_UNAVAILABLE",
                "Quality cancellation history is unavailable.",
            ))
        })
    }
    fn detail_core<'a>(
        &'a self,
        _request: QualityHttpReadRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityDetailCore> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "QUALITY_DETAIL_UNAVAILABLE",
                "Quality detail is unavailable.",
            ))
        })
    }
}

pub struct QualityHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin, for example `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn QualityHttpBackend>,
}

struct QualityHttpState {
    targets: BTreeMap<(String, String), QualityHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "QUALITY_COMMAND_REQUIRED",
        "Choose a current Quality hold and cancellation action.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(BusinessError::invalid(
            "QUALITY_REQUEST_INVALID",
            "Use one unambiguous request header value.",
        ));
    }
    first
        .map(|value| {
            value.to_str().map_err(|_| {
                BusinessError::invalid("QUALITY_REQUEST_INVALID", "Use a valid request header.")
            })
        })
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

pub fn quality_router(targets: Vec<QualityHttpTarget>) -> Result<Router, BusinessError> {
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
                "QUALITY_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/quality",
            get(read_workspace),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/quality/{id}",
            get(read_detail),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/quality/{id}/cancel",
            post(cancel),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/quality/{id}/cancellations",
            get(read_history),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/quality/{id}/detail-core",
            get(read_detail_core),
        )
        .with_state(Arc::new(QualityHttpState {
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
        Json(json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(body: Value) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
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
    state: &'a QualityHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a QualityHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn decision_json(value: QualityDecision) -> Value {
    json!({
        "canInspect": value.can_inspect,
        "canRelease": value.can_release,
        "canCancel": value.can_cancel,
        "canReviewCancellation": value.can_review_cancellation,
        "canDisposition": value.can_disposition,
    })
}

fn hold_json(hold: &QualityHold, decision: QualityDecision, next_action: &str) -> Value {
    json!({
        "id": hold.id,
        "companyId": hold.company_id,
        "branchId": hold.branch_id,
        "warehouseId": hold.warehouse_id,
        "productId": hold.product_id,
        "lotId": hold.lot_id,
        "reason": hold.reason,
        "dueDate": hold.due_date,
        "status": hold.status.as_str(),
        "createdBy": hold.created_by,
        "lastInspector": hold.last_inspector,
        "lastResult": hold.last_result.map(|result| result.as_str()),
        "releasedBy": hold.released_by,
        "releaseReason": hold.release_reason,
        "releasedAt": hold.released_at,
        "version": hold.version,
        "createdAt": hold.created_at,
        "updatedAt": hold.updated_at,
        "specId": hold.spec_id,
        "specVersion": hold.spec_version,
        "quantityScope": hold.quantity_scope,
        "heldQuantityMilli": hold.held_quantity_milli,
        "sourceType": hold.source_type,
        "sourceId": hold.source_id,
        "cancelRequestedBy": hold.cancel_requested_by,
        "cancelReason": hold.cancel_reason,
        "cancelledBy": hold.cancelled_by,
        "cancelledAt": hold.cancelled_at,
        "dispositionKind": hold.disposition_kind,
        "dispositionReason": hold.disposition_reason,
        "dispositionRequestedBy": hold.disposition_requested_by,
        "dispositionCountId": hold.disposition_count_id,
        "dispositionQuantityMilli": hold.disposition_quantity_milli,
        "disposedBy": hold.disposed_by,
        "disposedAt": hold.disposed_at,
        "inspectionCount": hold.inspection_count,
        "decision": decision_json(decision),
        "nextAction": next_action,
    })
}

fn metric(metric: &QualityMetric) -> Value {
    json!({
        "id": metric.id,
        "code": metric.code,
        "label": metric.label,
        "unit": metric.unit,
        "minimum": metric.minimum,
        "maximum": metric.maximum,
        "ordinal": metric.ordinal,
    })
}

fn inspection(row: &QualityInspection) -> Value {
    json!({
        "id": row.id,
        "holdId": row.hold_id,
        "reviewedVersion": row.reviewed_version,
        "result": row.result.as_str(),
        "evidence": row.evidence,
        "inspectedBy": row.inspected_by,
        "createdAt": row.created_at,
        "specId": row.spec_id,
        "specVersion": row.spec_version,
    })
}

fn measurement(row: &QualityMeasurement) -> Value {
    json!({
        "id": row.id,
        "inspectionId": row.inspection_id,
        "metricId": row.metric_id,
        "code": row.code,
        "label": row.label,
        "unit": row.unit,
        "minimum": row.minimum,
        "maximum": row.maximum,
        "measuredValue": row.measured_value,
        "passed": row.passed,
        "ordinal": row.ordinal,
    })
}

fn cancellation(row: &QualityCancellation) -> Value {
    json!({
        "id": row.id,
        "holdId": row.hold_id,
        "version": row.version,
        "action": row.action.as_str(),
        "reason": row.reason,
        "actorId": row.actor_id,
        "createdAt": row.created_at,
    })
}

fn disposition(row: &QualityDisposition) -> Value {
    json!({
        "id": row.id,
        "holdId": row.hold_id,
        "countId": row.count_id,
        "action": row.action,
        "actorId": row.actor_id,
        "reviewedVersion": row.reviewed_version,
        "reason": row.reason,
        "createdAt": row.created_at,
    })
}

fn alert(row: &QualityAlert) -> Value {
    json!({
        "id": row.id,
        "holdId": row.hold_id,
        "phase": row.phase,
        "sourceVersion": row.source_version,
        "dueDate": row.due_date,
        "detectedAt": row.detected_at,
        "version": row.version,
        "acknowledgedBy": row.acknowledged_by,
        "acknowledgedAt": row.acknowledged_at,
        "reason": row.reason,
    })
}

fn workspace(value: &QualityWorkspace) -> Value {
    json!({
        "data": {
            "holds": value.holds.iter().map(|row| hold_json(&row.hold, row.decision, row.next_action)).collect::<Vec<_>>(),
            "canHold": value.can_hold,
            "canManageSpecs": value.can_manage_specs,
        }
    })
}

fn detail(value: &QualityDetail) -> Value {
    json!({
        "data": {
            "hold": hold_json(&value.hold.hold, value.hold.decision, value.hold.next_action),
            "inspections": value.inspections.iter().map(inspection).collect::<Vec<_>>(),
            "measurements": value.measurements.iter().map(measurement).collect::<Vec<_>>(),
            "cancellations": value.cancellations.iter().map(cancellation).collect::<Vec<_>>(),
            "dispositions": value.dispositions.iter().map(disposition).collect::<Vec<_>>(),
            "specMetrics": value.spec_metrics.iter().map(metric).collect::<Vec<_>>(),
            "alerts": value.alerts.iter().map(alert).collect::<Vec<_>>(),
        }
    })
}

fn detail_core(value: &QualityDetailCore) -> Value {
    json!({"data": {
        "hold": hold_json(&value.hold.hold, value.hold.decision, value.hold.next_action),
        "inspections": value.inspections.iter().map(inspection).collect::<Vec<_>>(),
        "measurements": value.measurements.iter().map(measurement).collect::<Vec<_>>(),
        "dispositions": value.dispositions.iter().map(disposition).collect::<Vec<_>>(),
        "specMetrics": value.spec_metrics.iter().map(metric).collect::<Vec<_>>(),
        "alerts": value.alerts.iter().map(alert).collect::<Vec<_>>(),
        "cancellationHistory": "paged",
    }})
}

fn receipt_json(value: &QualityCancellationReceipt) -> Value {
    json!({
        "data": {
            "id": value.id,
            "holdId": value.hold_id,
            "cancellationId": value.cancellation_id,
            "status": value.status.as_str(),
            "version": value.version,
        }
    })
}

async fn read_workspace(
    State(state): State<Arc<QualityHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        target
            .backend
            .read(QualityHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: request.headers(),
                id: None,
            })
            .await
    }
    .await;
    match result {
        Ok(QualityHttpReadReply::Workspace(data)) => bounded(workspace(&data)),
        Ok(QualityHttpReadReply::Detail(_)) => error_response(missing()),
        Err(error) => error_response(error),
    }
}

async fn read_detail(
    State(state): State<Arc<QualityHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let id = RecordId::new(id)?;
        target
            .backend
            .read(QualityHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: request.headers(),
                id: Some(id.as_str()),
            })
            .await
    }
    .await;
    match result {
        Ok(QualityHttpReadReply::Detail(data)) => bounded(detail(&data)),
        Ok(QualityHttpReadReply::Workspace(_)) => error_response(missing()),
        Err(error) => error_response(error),
    }
}

/// Parse only after selected Account/Company authorization. A cursor is a
/// comparison input, never authority; duplicate or unknown keys are rejected.
pub fn parse_quality_history_query(
    raw: Option<&str>,
) -> Result<QualityHistoryCursor, BusinessError> {
    let invalid = || {
        BusinessError::invalid(
            "QUALITY_HISTORY_CURSOR_INVALID",
            "Reload Quality cancellation history.",
        )
    };
    let Some(raw) = raw.filter(|raw| !raw.is_empty()) else {
        return Ok(QualityHistoryCursor {
            before: None,
            head: None,
            revision: None,
        });
    };
    if raw.len() > 256 {
        return Err(invalid());
    }
    let mut values = BTreeMap::new();
    for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
        if !matches!(key.as_ref(), "before" | "head" | "revision")
            || values
                .insert(key.into_owned(), value.into_owned())
                .is_some()
        {
            return Err(invalid());
        }
    }
    if values.len() != 3 {
        return Err(invalid());
    }
    let number = |name: &str| -> Result<i64, BusinessError> {
        let value = values.get(name).ok_or_else(invalid)?;
        let number = value.parse::<i64>().map_err(|_| invalid())?;
        if number.to_string() != *value
            || !(-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&number)
            || (name == "head" && number < 1)
        {
            return Err(invalid());
        }
        Ok(number)
    };
    let revision = values.get("revision").ok_or_else(invalid)?;
    if revision.len() != 69
        || !revision.starts_with("qch1:")
        || !revision[5..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    Ok(QualityHistoryCursor {
        before: Some(number("before")?),
        head: Some(number("head")?),
        revision: Some(revision.clone()),
    })
}

async fn read_history(
    State(state): State<Arc<QualityHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let id = RecordId::new(id)?;
        target
            .backend
            .history(QualityHttpHistoryRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: request.headers(),
                id: id.as_str(),
                raw_query: request.uri().query(),
            })
            .await
    }
    .await;
    match result {
        Ok(page) => bounded(json!({"data": {
            "items": page.items.iter().map(cancellation).collect::<Vec<_>>(),
            "nextBefore": page.next_before,
            "headVersion": page.head_version,
            "historyRevision": page.history_revision,
        }})),
        Err(error) => error_response(error),
    }
}

async fn read_detail_core(
    State(state): State<Arc<QualityHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let id = RecordId::new(id)?;
        target
            .backend
            .detail_core(QualityHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: request.headers(),
                id: Some(id.as_str()),
            })
            .await
    }
    .await;
    match result {
        Ok(core) => bounded(detail_core(&core)),
        Err(error) => error_response(error),
    }
}

pub fn json_integer(value: &Value, minimum: i64) -> Option<i64> {
    if let Some(integer) = value.as_i64() {
        return (integer >= minimum && integer <= MAX_SAFE_INTEGER).then_some(integer);
    }
    let float = value.as_f64()?;
    (float.is_finite()
        && float.fract() == 0.0
        && float >= minimum as f64
        && float <= MAX_SAFE_INTEGER as f64)
        .then_some(float as i64)
}

pub fn parse_cancellation(
    value: &Value,
    hold_id: &str,
    command_key: CommandKey,
) -> Result<QualityCancellationCommand, BusinessError> {
    let invalid = || {
        BusinessError::invalid(
            "QUALITY_CANCELLATION_REQUIRED",
            "Review the current Quality hold, action, version and reason.",
        )
    };
    let object = value.as_object().ok_or_else(invalid)?;
    let expected_version =
        json_integer(object.get("expectedVersion").ok_or_else(invalid)?, 1).ok_or_else(invalid)?;
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .and_then(CancellationAction::parse)
        .ok_or_else(invalid)?;
    let reason = object
        .get("reason")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    let reason = validate_reason(
        reason,
        "QUALITY_CANCELLATION_REASON",
        "Enter a cancellation reason between 3 and 2000 characters.",
    )?;
    Ok(QualityCancellationCommand {
        command_key,
        hold_id: RecordId::new(hold_id)?,
        expected_version,
        action,
        reason,
    })
}

async fn cancel(
    State(state): State<Arc<QualityHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        if unique_header(request.headers(), header::ORIGIN)?
            .is_some_and(|value| value != target.origin)
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        let key = unique_header(
            request.headers(),
            HeaderName::from_static("idempotency-key"),
        )?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
        })
        .and_then(CommandKey::new)?;
        if !unique_header(request.headers(), header::CONTENT_TYPE)?.is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
        }) {
            return Err(malformed());
        }
        let hold_id = RecordId::new(id)?;
        let headers = request.headers().clone();
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| {
                BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller Quality command.")
            })?;
        let raw = serde_json::from_slice::<Value>(&body).map_err(|_| malformed())?;
        let command = parse_cancellation(&raw, hold_id.as_str(), key)?;
        target
            .backend
            .cancel(QualityHttpCommandRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                id: hold_id.as_str(),
                body: &raw,
                key: &command.command_key,
            })
            .await
    }
    .await;
    match result {
        Ok(receipt) => bounded(receipt_json(&receipt)),
        Err(error) => error_response(error),
    }
}
