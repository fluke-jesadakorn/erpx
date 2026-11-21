//! Native PMS dashboard/report read ingress **and** the PostgreSQL owner read
//! adapter. The migration packet allocates one adapter module for this slice,
//! so transport and persistence live together while staying separate: HTTP
//! ingress carries only registered Host/Holding targets, the exact browser
//! origin, bounded query facts and business-error mapping; the repository reads
//! only PMS-owned rows and is read-only, so it needs neither a serializable
//! unit nor a source lock.
//!
//! Account session resolution, the current IAM/assignment authority, the
//! selected Company/branch context and the PMS actor remain composition
//! responsibilities behind [`PmsReportHttpBackend`]. No value below is trusted
//! as authority, and no Finance/Inventory/People read happens here.
use crate::application::report_projection::{
    PmsReportFuture, PmsReportNightRead, PmsReportRepository,
};
use crate::domain::night_audit::{MAX_SHIFTS, MAX_STAYS, PmsNightAuditFacts};
use crate::domain::report_projection::{
    PmsReportBalances, PmsReportDay, PmsReportEntries, PmsReportEntry, PmsReportFlow,
    PmsReportKind, PmsReportRange, invalid as report_invalid, read_invalid as report_read_invalid,
    read_limit as report_read_limit,
};
use crate::domain::room_board::{PmsPropertyRow, cp_len};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{PgConnection, Row, postgres::PgRow};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const QUERY_LIMIT: usize = 4 * 1024;

pub type PmsReportHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct PmsReportRangeRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub selection: PmsReportRange,
}

pub struct PmsReportEntriesRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub selection: PmsReportEntries,
}

#[derive(Clone, Debug)]
pub struct PmsReportHttpResponse {
    /// Owner read data. The backend adds the validated current context.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl PmsReportHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope and owner-unit adapter.
pub trait PmsReportHttpBackend: Send + Sync {
    fn range<'a>(
        &'a self,
        request: PmsReportRangeRequest<'a>,
    ) -> PmsReportHttpFuture<'a, PmsReportHttpResponse>;
    fn entries<'a>(
        &'a self,
        request: PmsReportEntriesRequest<'a>,
    ) -> PmsReportHttpFuture<'a, PmsReportHttpResponse>;
}

pub struct PmsReportHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn PmsReportHttpBackend>,
}

struct PmsReportHttpState {
    targets: BTreeMap<(String, String), PmsReportHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    report_invalid()
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed());
    }
    first
        .map(|value| value.to_str().map_err(|_| malformed()))
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
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(body: serde_json::Value) -> Response {
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
    state: &'a PmsReportHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a PmsReportHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).copied().and_then(hex);
            let low = bytes.get(index + 2).copied().and_then(hex);
            let (Some(high), Some(low)) = (high, low) else {
                return Err(malformed());
            };
            out.push(high * 16 + low);
            index += 3;
        } else if bytes[index] == b'+' {
            out.push(b' ');
            index += 1;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| malformed())
}

/// Bounded read query facts with duplicate-name refusal.
fn query(request: &Request) -> Result<BTreeMap<String, String>, BusinessError> {
    let mut values = BTreeMap::new();
    let Some(raw) = request.uri().query() else {
        return Ok(values);
    };
    if raw.len() > QUERY_LIMIT {
        return Err(malformed());
    }
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = percent_decode(name)?;
        let value = percent_decode(value)?;
        if values.insert(name, value).is_some() {
            return Err(malformed());
        }
    }
    Ok(values)
}

fn bounded_id(value: &str) -> Result<String, BusinessError> {
    if value.is_empty() || cp_len(value) > 200 || value.chars().any(char::is_control) {
        return Err(malformed());
    }
    Ok(value.to_owned())
}

fn required(values: &BTreeMap<String, String>, name: &str) -> Result<String, BusinessError> {
    values.get(name).cloned().ok_or_else(malformed)
}

fn only(values: &BTreeMap<String, String>, allowed: &[&str]) -> Result<(), BusinessError> {
    if values
        .keys()
        .any(|name| !allowed.iter().any(|candidate| candidate == name))
    {
        return Err(malformed());
    }
    Ok(())
}

fn read_origin(target: &PmsReportHttpTarget, headers: &HeaderMap) -> Result<(), BusinessError> {
    // A read may be issued by the page itself without an Origin header; when a
    // browser does send one it must be this registered Holding's exact origin.
    match unique_header(headers, header::ORIGIN)? {
        None => Ok(()),
        Some(origin) if origin == target.origin.as_str() => Ok(()),
        Some(_) => Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        )),
    }
}

fn range_selection(values: &BTreeMap<String, String>) -> Result<PmsReportRange, BusinessError> {
    only(values, &["property", "from", "to"])?;
    let selection = PmsReportRange {
        property_id: bounded_id(&required(values, "property")?)?,
        from: required(values, "from")?,
        to: required(values, "to")?,
    };
    selection.validate()?;
    Ok(selection)
}

fn entries_selection(values: &BTreeMap<String, String>) -> Result<PmsReportEntries, BusinessError> {
    only(values, &["property", "date", "page"])?;
    let page = match values.get("page") {
        Some(value) => value.parse::<u32>().map_err(|_| malformed())?,
        None => 1,
    };
    let selection = PmsReportEntries {
        property_id: bounded_id(&required(values, "property")?)?,
        date: required(values, "date")?,
        page,
    };
    selection.validate()?;
    Ok(selection)
}

pub fn pms_report_router(targets: Vec<PmsReportHttpTarget>) -> Result<Router, BusinessError> {
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
                "PMS_REPORT_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pms/reports/{report}",
            get(read_report),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/pms/reports",
            get(read_retained_report),
        )
        .with_state(Arc::new(PmsReportHttpState {
            targets: configured,
        })))
}

/// Loads the bounded read query facts.
fn read_query(request: &Request) -> Result<BTreeMap<String, String>, BusinessError> {
    query(request)
}

async fn read_report(
    State(state): State<Arc<PmsReportHttpState>>,
    Path((slug, report)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        read_origin(target, request.headers())?;
        let kind = PmsReportKind::parse(&report).ok_or_else(malformed)?;
        let values = read_query(&request)?;
        let headers = request.headers().clone();
        let _ = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| malformed())?;
        dispatch(target, &headers, kind, &values).await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

/// Retained route shape: `GET .../pms/reports?view=range|entries`.
async fn read_retained_report(
    State(state): State<Arc<PmsReportHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        read_origin(target, request.headers())?;
        let values = read_query(&request)?;
        let kind = match values.get("view") {
            Some(view) => PmsReportKind::parse(view).ok_or_else(malformed)?,
            None => PmsReportKind::Range,
        };
        let mut facts = values.clone();
        facts.remove("view");
        let headers = request.headers().clone();
        let _ = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| malformed())?;
        dispatch(target, &headers, kind, &facts).await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn dispatch(
    target: &PmsReportHttpTarget,
    headers: &HeaderMap,
    kind: PmsReportKind,
    values: &BTreeMap<String, String>,
) -> Result<PmsReportHttpResponse, BusinessError> {
    match kind {
        PmsReportKind::Range => {
            let selection = range_selection(values)?;
            target
                .backend
                .range(PmsReportRangeRequest {
                    host: &target.host,
                    holding_slug: &target.holding_slug,
                    headers,
                    selection,
                })
                .await
        }
        PmsReportKind::Entries => {
            let selection = entries_selection(values)?;
            target
                .backend
                .entries(PmsReportEntriesRequest {
                    host: &target.host,
                    holding_slug: &target.holding_slug,
                    headers,
                    selection,
                })
                .await
        }
    }
}

// ---------------------------------------------------------------------------
// PostgreSQL owner read adapter
// ---------------------------------------------------------------------------

type Error = PgTransactionError<BusinessError>;

fn record_row(row: &PgRow) -> Result<crate::domain::room_board::PmsRecord, Error> {
    crate::adapters::room_board::record_row(row)
}

fn property_row(row: &PgRow) -> Result<PmsPropertyRow, Error> {
    Ok(PmsPropertyRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        name: row.try_get("name")?,
        business_date: row.try_get("business_date")?,
        state: row.try_get("state")?,
        config_json: row.try_get("config_json")?,
        version: row.try_get("version")?,
    })
}

async fn read_property(
    c: &mut PgConnection,
    company_id: &str,
    property_id: &str,
) -> Result<Option<PmsPropertyRow>, Error> {
    let row = sqlx::query(
        "SELECT id,company_id,branch_id,name,business_date,state,config_json,version \
         FROM pms_properties WHERE id=$1 AND company_id=$2",
    )
    .bind(property_id)
    .bind(company_id)
    .fetch_optional(&mut *c)
    .await?;
    row.as_ref().map(property_row).transpose()
}

/// Retained `days` query verbatim (lineage roots, room entries, completed night
/// metrics with the room/stay fallbacks, entry sums, cancellations and
/// no-shows), bounded by the caller-validated date range.
async fn read_days(
    c: &mut PgConnection,
    property: &PmsPropertyRow,
    from: &str,
    to: &str,
) -> Result<Vec<PmsReportDay>, Error> {
    let timezone = serde_json::from_str::<serde_json::Value>(&property.config_json)
        .ok()
        .and_then(|config| {
            config
                .get("timezone")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .ok_or_else(|| Error::from(report_read_invalid()))?;
    let rows = sqlx::query(
        "WITH RECURSIVE days AS (SELECT generate_series($1::date,$2::date,interval '1 day')::date d),
 lineage AS (
  SELECT id root,id,data_json::jsonb data,ARRAY[id] seen FROM pms_records WHERE property_id=$3 AND kind='entry'
   AND data_json::jsonb->>'date'>=$4 AND data_json::jsonb->>'date'<=$5
  UNION ALL SELECT l.root,e.id,e.data_json::jsonb,l.seen||e.id FROM lineage l JOIN pms_records e
   ON e.id=l.data->>'originalId' AND e.property_id=$6 AND e.kind='entry' WHERE NOT e.id=ANY(l.seen)
 ), room_entries AS (SELECT DISTINCT root FROM lineage WHERE data->>'sourceKey' LIKE 'night:%')
 SELECT d::text date,
 COALESCE((SELECT (data_json::jsonb->'roomMetrics'->>'capacity')::bigint FROM pms_records WHERE property_id=$7 AND kind='night' AND data_json::jsonb->>'date'=d::text AND data_json::jsonb->>'state'='completed' LIMIT 1),
  (SELECT COUNT(*) FROM pms_records r WHERE r.property_id=$8 AND r.kind='room' AND r.data_json::jsonb->>'active'='true' AND NOT EXISTS(SELECT 1 FROM pms_records b WHERE b.property_id=r.property_id AND b.kind='block' AND b.data_json::jsonb->>'kind'='maintenance' AND b.data_json::jsonb->>'state'='active' AND b.data_json::jsonb->>'roomId'=r.id AND b.data_json::jsonb->>'from'<=d::text)))::bigint capacity,
 COALESCE((SELECT (data_json::jsonb->'roomMetrics'->>'sold')::bigint FROM pms_records WHERE property_id=$9 AND kind='night' AND data_json::jsonb->>'date'=d::text AND data_json::jsonb->>'state'='completed' LIMIT 1),
  (SELECT COUNT(*) FROM pms_records WHERE property_id=$10 AND kind='stay' AND data_json::jsonb->>'state' IN ('in_house','checked_out') AND data_json::jsonb->>'arrival'<=d::text AND data_json::jsonb->>'departure'>d::text))::bigint sold,
 COALESCE(SUM((e.data_json::jsonb->>'net')::bigint) FILTER(WHERE e.id IN (SELECT root FROM room_entries)),0)::bigint room_revenue,
 COALESCE(SUM((e.data_json::jsonb->>'net')::bigint+(e.data_json::jsonb->>'service')::bigint) FILTER(WHERE e.data_json::jsonb->>'kind' IN ('charge','adjustment')),0)::bigint net_revenue,
 COALESCE(SUM((e.data_json::jsonb->>'tax')::bigint) FILTER(WHERE e.data_json::jsonb->>'kind' IN ('charge','adjustment')),0)::bigint tax,
 COALESCE(-SUM((e.data_json::jsonb->>'total')::bigint) FILTER(WHERE e.data_json::jsonb->>'kind'='refund'),0)::bigint refunds,
 (SELECT COUNT(*) FROM pms_records WHERE property_id=$11 AND kind='stay' AND data_json::jsonb->>'state'='cancelled' AND to_char(to_timestamp(updated_at/1000.0) AT TIME ZONE $12::text,'YYYY-MM-DD')=d::text)::bigint cancellations,
 (SELECT COUNT(*) FROM pms_records WHERE property_id=$13 AND kind='stay' AND data_json::jsonb->>'state'='no_show' AND data_json::jsonb->>'arrival'=d::text)::bigint no_shows
 FROM days LEFT JOIN pms_records e ON e.property_id=$14 AND e.kind='entry' AND e.data_json::jsonb->>'date'=d::text GROUP BY d ORDER BY d",
    )
    .bind(from)
    .bind(to)
    .bind(&property.id)
    .bind(from)
    .bind(to)
    .bind(&property.id)
    .bind(&property.id)
    .bind(&property.id)
    .bind(&property.id)
    .bind(&property.id)
    .bind(&property.id)
    .bind(&timezone)
    .bind(&property.id)
    .bind(&property.id)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > crate::domain::report_projection::MAX_DAYS {
        return Err(Error::from(report_read_limit(
            "The hotel report exceeds its bounded range.",
        )));
    }
    rows.iter()
        .map(|row| {
            Ok(PmsReportDay {
                date: row.try_get("date")?,
                capacity: row.try_get("capacity")?,
                sold: row.try_get("sold")?,
                room_revenue: row.try_get("room_revenue")?,
                net_revenue: row.try_get("net_revenue")?,
                tax: row.try_get("tax")?,
                refunds: row.try_get("refunds")?,
                cancellations: row.try_get("cancellations")?,
                no_shows: row.try_get("no_shows")?,
            })
        })
        .collect()
}

async fn read_flow(
    c: &mut PgConnection,
    property_id: &str,
    business_date: &str,
) -> Result<PmsReportFlow, Error> {
    let row = sqlx::query(
        "SELECT count(*) FILTER(WHERE data_json::jsonb->>'state'='reserved' AND data_json::jsonb->>'arrival'=$2::text)::bigint arrivals, \
         count(*) FILTER(WHERE data_json::jsonb->>'state'='in_house' AND data_json::jsonb->>'departure'<=$2::text)::bigint departures, \
         count(*) FILTER(WHERE data_json::jsonb->>'state'='in_house')::bigint in_house \
         FROM pms_records WHERE property_id=$1 AND kind='stay'",
    )
    .bind(property_id)
    .bind(business_date)
    .fetch_one(&mut *c)
    .await?;
    Ok(PmsReportFlow {
        arrivals: row.try_get("arrivals")?,
        departures: row.try_get("departures")?,
        in_house: row.try_get("in_house")?,
    })
}

async fn read_balances(
    c: &mut PgConnection,
    property_id: &str,
) -> Result<PmsReportBalances, Error> {
    let row = sqlx::query(
        "SELECT COALESCE(SUM((data_json::jsonb->>'due')::bigint),0)::bigint due, \
         COALESCE(SUM((data_json::jsonb->>'advance')::bigint),0)::bigint advance, \
         COALESCE(SUM((data_json::jsonb->>'deposit')::bigint),0)::bigint deposit, \
         count(*)::bigint total_accounts, \
         count(*) FILTER(WHERE data_json::jsonb->>'state'<>'closed')::bigint open_accounts, \
         count(*) FILTER(WHERE data_json::jsonb->>'state'='credit')::bigint credit_accounts, \
         count(*) FILTER(WHERE data_json::jsonb->>'state'<>'closed' AND COALESCE((data_json::jsonb->>'due')::bigint,0)>0)::bigint accounts_with_due \
         FROM pms_records WHERE property_id=$1 AND kind='guest_account'",
    )
    .bind(property_id)
    .fetch_one(&mut *c)
    .await?;
    Ok(PmsReportBalances {
        source: "pms_guest_accounts".to_owned(),
        due: row.try_get("due")?,
        advance: row.try_get("advance")?,
        deposit: row.try_get("deposit")?,
        total_accounts: row.try_get("total_accounts")?,
        open_accounts: row.try_get("open_accounts")?,
        credit_accounts: row.try_get("credit_accounts")?,
        accounts_with_due: row.try_get("accounts_with_due")?,
    })
}

async fn kind_records(
    c: &mut PgConnection,
    property_id: &str,
    kind: &str,
    limit: i64,
    active: bool,
) -> Result<Vec<crate::domain::room_board::PmsRecord>, Error> {
    let rows = sqlx::query(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind=$2 AND ($4::boolean IS NOT TRUE OR data_json::jsonb->>'active'='true') \
         ORDER BY id LIMIT $3",
    )
    .bind(property_id)
    .bind(kind)
    .bind(limit)
    .bind(active)
    .fetch_all(&mut *c)
    .await?;
    rows.iter().map(record_row).collect()
}

async fn read_night_read(
    c: &mut PgConnection,
    property: &PmsPropertyRow,
) -> Result<PmsReportNightRead, Error> {
    let night_state: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT data_json::jsonb->>'state' FROM pms_records WHERE property_id=$1 AND kind='night' \
         AND data_json::jsonb->>'date'=$2 ORDER BY updated_at DESC,id LIMIT 1",
    )
    .bind(&property.id)
    .bind(&property.business_date)
    .fetch_optional(&mut *c)
    .await?
    .flatten();
    let job_state: Option<String> = sqlx::query_scalar::<_, String>(
        "SELECT state FROM pms_jobs WHERE property_id=$1 AND business_date=$2",
    )
    .bind(&property.id)
    .bind(&property.business_date)
    .fetch_optional(&mut *c)
    .await?;
    let stays = sqlx::query(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records WHERE property_id=$1 \
         AND kind='stay' AND data_json::jsonb->>'state' IN ('reserved','in_house') ORDER BY id LIMIT $2",
    )
    .bind(&property.id)
    .bind(MAX_STAYS as i64 + 1)
    .fetch_all(&mut *c)
    .await?;
    if stays.len() > MAX_STAYS {
        return Err(Error::from(report_read_limit(
            "The hotel has more open stays than this bounded report can show.",
        )));
    }
    let shifts = sqlx::query(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records WHERE property_id=$1 \
         AND kind='shift' AND (data_json::jsonb->>'state'='open' OR (data_json::jsonb->>'state'='closed' \
         AND COALESCE((data_json::jsonb->>'variance')::bigint,0)<>0)) ORDER BY id LIMIT $2",
    )
    .bind(&property.id)
    .bind(MAX_SHIFTS as i64 + 1)
    .fetch_all(&mut *c)
    .await?;
    if shifts.len() > MAX_SHIFTS {
        return Err(Error::from(report_read_limit(
            "The hotel has more shifts than this bounded report can show.",
        )));
    }
    // `readiness` only asks whether the hotel has any room type, any active
    // room and any active rate, so one bounded row per question is enough.
    let room_types = kind_records(c, &property.id, "room_type", 1, false).await?;
    let rooms = kind_records(c, &property.id, "room", 1, true).await?;
    let rates = kind_records(c, &property.id, "rate", 1, true).await?;
    let mut facts = PmsNightAuditFacts::empty(property.clone());
    facts.stays = stays
        .iter()
        .map(record_row)
        .collect::<Result<Vec<_>, _>>()?;
    facts.shifts = shifts
        .iter()
        .map(record_row)
        .collect::<Result<Vec<_>, _>>()?;
    facts.room_types = room_types;
    facts.rooms = rooms;
    facts.rates = rates;
    Ok(PmsReportNightRead {
        night_state,
        job_state,
        facts,
    })
}

async fn read_entries(
    c: &mut PgConnection,
    property_id: &str,
    date: &str,
    offset: i64,
) -> Result<Vec<PmsReportEntry>, Error> {
    let rows = sqlx::query(
        "SELECT id,data_json FROM pms_records WHERE property_id=$1 AND kind='entry' \
         AND data_json::jsonb->>'date'=$2 ORDER BY updated_at,id LIMIT $3 OFFSET $4",
    )
    .bind(property_id)
    .bind(date)
    .bind(crate::domain::report_projection::PAGE_SIZE as i64 + 1)
    .bind(offset)
    .fetch_all(&mut *c)
    .await?;
    rows.iter()
        .map(|row| {
            let raw: String = row.try_get("data_json")?;
            let data =
                serde_json::from_str(&raw).map_err(|_| Error::from(report_read_invalid()))?;
            Ok(PmsReportEntry {
                id: row.try_get("id")?,
                data,
            })
        })
        .collect()
}

/// Read-only PMS report repository bound to the caller's connection.
pub struct PgPmsReportRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgPmsReportRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl PmsReportRepository for PgPmsReportRead<'_> {
    type Error = Error;

    fn property<'a>(
        &'a mut self,
        company_id: &'a str,
        property_id: &'a str,
    ) -> PmsReportFuture<'a, Option<PmsPropertyRow>, Self::Error> {
        Box::pin(async move { read_property(self.connection, company_id, property_id).await })
    }

    fn days<'a>(
        &'a mut self,
        property: &'a PmsPropertyRow,
        from: &'a str,
        to: &'a str,
    ) -> PmsReportFuture<'a, Vec<PmsReportDay>, Self::Error> {
        Box::pin(async move { read_days(self.connection, property, from, to).await })
    }

    fn flow<'a>(
        &'a mut self,
        property_id: &'a str,
        business_date: &'a str,
    ) -> PmsReportFuture<'a, PmsReportFlow, Self::Error> {
        Box::pin(async move { read_flow(self.connection, property_id, business_date).await })
    }

    fn balances<'a>(
        &'a mut self,
        property_id: &'a str,
    ) -> PmsReportFuture<'a, PmsReportBalances, Self::Error> {
        Box::pin(async move { read_balances(self.connection, property_id).await })
    }

    fn night_read<'a>(
        &'a mut self,
        property: &'a PmsPropertyRow,
    ) -> PmsReportFuture<'a, PmsReportNightRead, Self::Error> {
        Box::pin(async move { read_night_read(self.connection, property).await })
    }

    fn entries<'a>(
        &'a mut self,
        property_id: &'a str,
        date: &'a str,
        offset: i64,
    ) -> PmsReportFuture<'a, Vec<PmsReportEntry>, Self::Error> {
        Box::pin(async move { read_entries(self.connection, property_id, date, offset).await })
    }
}
