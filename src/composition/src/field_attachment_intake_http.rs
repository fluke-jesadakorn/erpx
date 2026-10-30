//! Unmounted contract for the private Field visit attachment-set transport.
//!
//! Execution is deliberately deferred: the command currently takes a sampled
//! readiness value before it knows whether the request is a recorded replay.
use erpx_server_files::public::field_visit_intake::{FieldSetListRequest, FieldSetRequest};
use erpx_server_shared::public::{BusinessError, FailureKind};

/// Supplied by trusted composition configuration, never by an HTTP request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldIntakeMode {
    Disabled,
    ReadOnly,
    Enabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldIntakePolicy {
    pub mode: FieldIntakeMode,
    pub uploads_frozen: bool,
}

pub use crate::field_attachment_intake_command::{
    FieldIntakeProviderReadiness, FieldIntakeProviderStatus,
};

impl FieldIntakePolicy {
    pub fn require_read(self) -> Result<bool, BusinessError> {
        match self.mode {
            FieldIntakeMode::Disabled => Err(disabled()),
            FieldIntakeMode::ReadOnly => Ok(true),
            FieldIntakeMode::Enabled => Ok(false),
        }
    }
    pub fn require_create(self) -> Result<(), BusinessError> {
        if self.mode != FieldIntakeMode::Enabled {
            return Err(disabled());
        }
        if self.uploads_frozen {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "ATTACHMENT_UPLOAD_FREEZE",
                "File intake is briefly paused for storage reconciliation.",
            ));
        }
        Ok(())
    }
}
fn disabled() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENTS_DISABLED",
        "The attachment platform is not enabled for this release.",
    )
}
fn invalid() -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", "Invalid file set.")
}
fn target_required() -> BusinessError {
    BusinessError::invalid(
        "ATTACHMENT_TARGET_REQUIRED",
        "Choose a record before listing files.",
    )
}
fn valid_len(value: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&value.encode_utf16().count())
}
fn component(value: &str) -> bool {
    matches!(
        value,
        "record" | "action" | "line" | "event" | "message" | "publication"
    )
}

/// Validate after selected Account ingress and before any Files SQL.
/// Unknown JSON fields are ignored, matching the original Zod object schema.
pub fn parse_create(body: &[u8]) -> Result<FieldSetRequest, BusinessError> {
    let mut value: serde_json::Value = serde_json::from_slice(body).map_err(|_| invalid())?;
    let object = value.as_object_mut().ok_or_else(invalid)?;
    if object.get("componentRef").is_some_and(|v| v.is_null()) {
        return Err(invalid());
    }
    // JSON numbers such as 1.0 are integers to JavaScript/Zod as well.
    if let Some(number) = object.get_mut("expectedTargetVersion")
        && let Some(n) = number.as_f64().filter(|n| {
            n.is_finite() && n.fract() == 0.0 && *n >= 1.0 && *n <= 9_007_199_254_740_991.0
        })
    {
        *number = serde_json::Value::from(n as i64);
    }
    let input: FieldSetRequest = serde_json::from_value(value).map_err(|_| invalid())?;
    if input.target_type != "field_service_visit"
        || !valid_len(&input.target_id, 1, 180)
        || !component(&input.component_kind)
        || input
            .component_ref
            .as_ref()
            .is_some_and(|v| !valid_len(v, 0, 180))
        || !valid_len(&input.purpose_key, 1, 80)
        || !(1..=9_007_199_254_740_991).contains(&input.expected_target_version)
    {
        return Err(invalid());
    }
    Ok(input)
}

/// List fields are extracted using the original URLSearchParams first-value
/// semantics by the mounting HTTP layer; empty optional filters mean absent.
#[derive(Clone, Debug, Default)]
pub struct FieldIntakeListQuery {
    pub target_type: String,
    pub target_id: String,
    pub purpose_key: Option<String>,
    pub component_kind: Option<String>,
    pub expected_target_version: Option<i64>,
}
pub fn validate_list(query: FieldIntakeListQuery) -> Result<FieldSetListRequest, BusinessError> {
    if query.target_type.is_empty() || query.target_id.is_empty() {
        return Err(target_required());
    }
    if query.target_type != "field_service_visit" {
        return Err(invalid());
    }
    Ok(FieldSetListRequest {
        target_type: query.target_type,
        target_id: query.target_id,
        purpose_key: query.purpose_key.filter(|v| !v.is_empty()),
        component_kind: query.component_kind.filter(|v| !v.is_empty()),
        expected_target_version: query.expected_target_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn create_fences_target_and_wire_shape() {
        let valid = br#"{"targetType":"field_service_visit","targetId":"visit","componentKind":"record","purposeKey":"site_photo","expectedTargetVersion":1,"ignored":true}"#;
        assert!(parse_create(valid).is_ok());
        for body in [br#"{"targetType":"other","targetId":"visit","componentKind":"record","purposeKey":"site_photo","expectedTargetVersion":1}"#.as_slice(), br#"{"targetType":"field_service_visit","targetId":"visit","componentKind":"bogus","purposeKey":"site_photo","expectedTargetVersion":1}"#, br#"{"targetType":"field_service_visit","targetId":"visit","componentKind":"record","purposeKey":"site_photo","expectedTargetVersion":0}"#] {
            assert_eq!(parse_create(body).unwrap_err().code, "VALIDATION_FAILED");
        }
    }
    #[test]
    fn number_query_matches_javascript_truthiness_and_integer_filters() {
        for (value, expected, never) in [
            ("0x3", Some(3), false),
            ("%EF%BB%BF3", Some(3), false),
            ("%C2%853", None, false),
            ("0b11", Some(3), false),
            ("0o3", Some(3), false),
            ("junk", None, false),
            ("NaN", None, false),
            ("0", None, false),
            ("-0", None, false),
            ("%20", None, false),
            ("1.5", None, true),
            ("Infinity", None, true),
            ("-Infinity", None, true),
            ("3e0", Some(3), false),
        ] {
            let raw = format!(
                "targetType=field_service_visit&targetId=visit&expectedTargetVersion={value}"
            );
            let parsed = query(Some(&raw)).unwrap();
            assert_eq!(
                (parsed.request.expected_target_version, parsed.never_matches),
                (expected, never),
                "{value}"
            );
        }
        let first=query(Some("targetType=field_service_visit&targetId=visit&expectedTargetVersion=3&expectedTargetVersion=4")).unwrap();
        assert_eq!(first.request.expected_target_version, Some(3));
    }
    #[test]
    fn mode_and_target_errors_are_typed() {
        let policy = FieldIntakePolicy {
            mode: FieldIntakeMode::ReadOnly,
            uploads_frozen: false,
        };
        assert_eq!(policy.require_read(), Ok(true));
        assert_eq!(
            policy.require_create().unwrap_err().kind,
            FailureKind::NotFound
        );
        let frozen = FieldIntakePolicy {
            mode: FieldIntakeMode::Enabled,
            uploads_frozen: true,
        };
        assert_eq!(
            frozen.require_create().unwrap_err().code,
            "ATTACHMENT_UPLOAD_FREEZE"
        );
        let wrong = FieldIntakeListQuery {
            target_type: "other".into(),
            target_id: "visit".into(),
            ..Default::default()
        };
        assert_eq!(validate_list(wrong).unwrap_err().code, "VALIDATION_FAILED");
        assert_eq!(
            validate_list(FieldIntakeListQuery::default())
                .unwrap_err()
                .code,
            "ATTACHMENT_TARGET_REQUIRED"
        );
    }
}

use crate::field_attachment_intake::{
    FieldAttachmentIntakeAuthority, FieldAttachmentIntakeSources,
};
use crate::field_attachment_intake_command::{
    FieldAttachmentIntakeCommand, FieldAttachmentIntakeCommandSources,
    execute_field_attachment_intake_with_live_readiness,
};
use crate::http_scope::BusinessScopeSlugs;
use axum::{
    Json, Router,
    extract::{Path, RawQuery, Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use erpx_server_commercial::public::purchases::CommercialCatalogue;
use erpx_server_files::public::field_visit_intake::{
    FieldIntakeAvailability, list_field_sets, prepare_field_intake,
};
use erpx_server_identity_access::public::{
    AccountPolicy, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
    effective_authority::{
        HoldingWorkAuthorizer, SelectedIngressFuture, SelectedTransportCallbacks,
        SelectedWorkIngress, WorkAuthorizationRequest,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{Clock, CompanyId, HoldingId};
use std::{collections::BTreeMap, sync::Arc};

pub struct FieldIntakeTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub holding: HoldingId,
    pub company: CompanyId,
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub catalogue: Arc<CommercialCatalogue>,
    pub clock: Arc<C>,
    pub policy: FieldIntakePolicy,
    pub readiness: Arc<dyn FieldIntakeProviderReadiness>,
}
struct HttpState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String), Arc<FieldIntakeTarget<C>>>,
}
pub fn field_intake_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<FieldIntakeTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut map = BTreeMap::new();
    for target in targets {
        let valid = policy
            .audience(&target.host)
            .and_then(|a| registry.expected_for_audience(&a).ok())
            .is_some_and(|h| h.slug == target.holding_slug && h.holding_id == target.holding);
        if !valid
            || map
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    Arc::new(target),
                )
                .is_some()
        {
            return Err(BusinessError::invalid(
                "FIELD_INTAKE_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/field-service/attachment-sets",
            get(http_list::<C>).post(http_create::<C>),
        )
        .with_state(Arc::new(HttpState {
            policy,
            scopes,
            targets: map,
        })))
}
fn wire_error() -> PgTransactionError<BusinessError> {
    erpx_server_runtime::public::postgres::PersistenceError::new("FIELD_INTAKE_WIRE_INVALID").into()
}
fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn response(
    result: Result<serde_json::Value, PgTransactionError<BusinessError>>,
    created: bool,
) -> Response {
    let (status, value) = match result {
        Ok(value) => (
            if created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            },
            serde_json::json!({"data":value}),
        ),
        Err(PgTransactionError::Business(e)) => (
            match e.kind {
                FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
                FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
                FailureKind::Forbidden => StatusCode::FORBIDDEN,
                FailureKind::NotFound => StatusCode::NOT_FOUND,
                FailureKind::Conflict => StatusCode::CONFLICT,
                FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            },
            serde_json::json!({"error":{"code":e.code,"message":e.message}}),
        ),
        Err(PgTransactionError::Persistence(_)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({"error":{"code":"INTERNAL_ERROR","message":"The request could not be completed."}}),
        ),
    };
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}
fn decoded(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(a), Some(z)) = (
                (b[i + 1] as char).to_digit(16),
                (b[i + 2] as char).to_digit(16),
            ) {
                out.push((a * 16 + z) as u8);
                i += 3;
            } else {
                out.push(b[i]);
                i += 1;
            }
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
#[derive(Clone)]
struct ListIngress {
    request: FieldSetListRequest,
    never_matches: bool,
}
fn js_number(raw: &str) -> f64 {
    let s = erpx_server_files::public::field_visit_intake::component_ref(Some(raw)).unwrap_or("");
    if s.is_empty() {
        return 0.0;
    }
    if matches!(s, "Infinity" | "+Infinity") {
        return f64::INFINITY;
    }
    if s == "-Infinity" {
        return f64::NEG_INFINITY;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0b", 2),
        ("0B", 2),
        ("0o", 8),
        ("0O", 8),
    ] {
        if let Some(digits) = s.strip_prefix(prefix) {
            if digits.is_empty() {
                return f64::NAN;
            }
            let mut result = 0.0;
            for c in digits.chars() {
                let Some(digit) = c.to_digit(radix) else {
                    return f64::NAN;
                };
                result = result * radix as f64 + digit as f64;
            }
            return result;
        }
    }
    if s.bytes()
        .any(|b| b.is_ascii_alphabetic() && !matches!(b, b'e' | b'E'))
    {
        return f64::NAN;
    }
    s.parse::<f64>().unwrap_or(f64::NAN)
}
fn query(raw: Option<&str>) -> Result<ListIngress, BusinessError> {
    if raw.is_some_and(|s| s.len() > 2 * 1024 * 1024) {
        return Err(invalid());
    }
    let mut map = BTreeMap::new();
    for pair in raw.unwrap_or("").split('&').filter(|v| !v.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        map.entry(decoded(k)).or_insert_with(|| decoded(v));
    }
    let take = |k: &str| map.get(k).cloned();
    let number = take("expectedTargetVersion")
        .filter(|v| !v.is_empty())
        .map(|v| js_number(&v));
    // The legacy repository applies the SQL predicate only for truthy numbers.
    // Non-integral, infinite, or out-of-bigint parameters fail the legacy bigint bind.
    let never_matches = number.is_some_and(|n| {
        !n.is_nan()
            && n != 0.0
            && (!n.is_finite()
                || n.fract() != 0.0
                || n < i64::MIN as f64
                || n >= 9_223_372_036_854_775_808.0)
    });
    let version = number
        .filter(|n| !n.is_nan() && *n != 0.0 && !never_matches)
        .map(|n| n as i64);
    let request = validate_list(FieldIntakeListQuery {
        target_type: take("targetType").unwrap_or_default(),
        target_id: take("targetId").unwrap_or_default(),
        purpose_key: take("purposeKey"),
        component_kind: take("componentKind"),
        expected_target_version: version,
    })?;
    Ok(ListIngress {
        request,
        never_matches,
    })
}

fn access() -> WorkAuthorizationRequest {
    WorkAuthorizationRequest {
        permission: "records.read".into(),
        module: Some("field_service".into()),
        destination: "service/orders".into(),
        api: true,
    }
}
type SelectionResolver = Box<
    dyn FnOnce() -> Result<
            erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest,
            BusinessError,
        > + Send,
>;
type PreparedIntake<C> = (
    Arc<FieldIntakeTarget<C>>,
    String,
    erpx_server_identity_access::public::DeviceProof,
    SelectionResolver,
);
fn ingress<C>(
    state: &HttpState<C>,
    headers: &HeaderMap,
    slug: String,
    write: bool,
) -> Result<PreparedIntake<C>, BusinessError> {
    let host = unique_request_header(headers, "host")?
        .ok_or_else(missing)?
        .to_owned();
    let target = state
        .targets
        .get(&(host.clone(), slug))
        .ok_or_else(missing)?
        .clone();
    let inbound = parse_inbound_account_request(&state.policy, headers)?;
    if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
        return Err(missing());
    }
    if write
        && unique_request_header(headers, "origin")?.is_some_and(|v| v != inbound.expected_origin())
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        ));
    }
    let scope = state.scopes.slugs(headers);
    let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
    let captured = CapturedHoldingMutation::capture(headers, branch, department);
    let company = target.company.clone();
    let selection = Box::new(move || {
        scope?;
        if write {
            captured.resolve()
        } else {
            captured.resolve_read(company)
        }
    });
    Ok((target, host, inbound.into_proof(), selection))
}
async fn http_list<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<HttpState<C>>>,
    Path(slug): Path<String>,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Response {
    let result=async {
        let (target,host,proof,selection)=ingress(&state,&headers,slug,false)?;
        let action_target=target.clone();
        let callbacks=SelectedTransportCallbacks::<_,ListIngress,BusinessError,_>::new::<serde_json::Value>(move|_:&SelectedWorkIngress<'_>|->SelectedIngressFuture<ListIngress,BusinessError>{let raw=raw.clone();Box::pin(async move{query(raw.as_deref())})},|_,e|Err(e),move|c,actor,input:ListIngress|{let target=action_target.clone();Box::pin(async move{
            if actor.holding_id()!=&target.holding || actor.selection().workspace().company_id!=target.company {return Err(missing().into());}
            let read_only=target.policy.require_read()?;
            let authority=FieldAttachmentIntakeAuthority::new(actor,&target.holding,&target.company,&target.catalogue,target.clock.as_ref());
            let guard=prepare_field_intake(c,&authority,actor,&input.request.target_id,false,target.clock.now().get()).await?;
            if input.never_matches {guard.require_current(c,target.clock.now().get()).await?;return Err(erpx_server_runtime::public::postgres::PersistenceError::new("FILES_VERSION_BIND_INVALID").into());}
            let data=list_field_sets(c,&guard,&input.request,read_only,target.clock.now().get()).await?;
            let data = data.into_iter().map(|v| crate::field_attachment_intake_contracts::checked_field_intake_set(v).map_err(|_|wire_error())).collect::<Result<Vec<_>,_>>()?;
            Ok(Ok(serde_json::Value::Array(data)))
        })});
        target.authorizer.execute_with_selected_transport_ingress_deferred(&host,proof,selection,(access(),FieldAttachmentIntakeSources),callbacks).await?.map_err(Into::into)
    }.await;
    response(result, false)
}
async fn http_create<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<HttpState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let result=async {
        let (target,host,proof,selection)=ingress(&state,&parts.headers,slug,true)?;
        let key=unique_request_header(&parts.headers,"idempotency-key").map_err(|_|BusinessError::invalid("COMMAND_KEY_REQUIRED","A valid Idempotency-Key is required.")).and_then(|v|v.map(str::to_owned).ok_or_else(||BusinessError::invalid("COMMAND_KEY_REQUIRED","A valid Idempotency-Key is required.")));
        let body=Arc::new(tokio::sync::Mutex::new(Some(body)));let cache=Arc::new(tokio::sync::OnceCell::<Option<Vec<u8>>>::new());
        let action_target=target.clone();let ingress_target=target.clone();
        let callbacks=SelectedTransportCallbacks::<_,(FieldSetRequest,String),BusinessError,_>::new::<serde_json::Value>(move|selected:&SelectedWorkIngress<'_>|->SelectedIngressFuture<(FieldSetRequest,String),BusinessError>{let valid=selected.holding_id()==&ingress_target.holding&&selected.company_id()==&ingress_target.company;let body=body.clone();let cache=cache.clone();let key=key.clone();let policy=ingress_target.policy;Box::pin(async move{
            if !valid{return Err(missing());}
            let bytes=cache.get_or_init(||async{let body=body.lock().await.take()?;axum::body::to_bytes(body,2*1024*1024).await.ok().map(|v|v.to_vec())}).await.as_ref().ok_or_else(invalid)?;
            let input=parse_create(bytes)?;let key=key?;erpx_server_workflow::public::commands::validate_key(&key)?;policy.require_create()?;Ok((input,key))
        })},|_,e|Err(e),move|c,actor,(input,key):(FieldSetRequest,String)|{let target=action_target.clone();Box::pin(async move{
            if actor.holding_id()!=&target.holding||actor.selection().workspace().company_id!=target.company{return Err(missing().into());}
            let availability=FieldIntakeAvailability {uploads_frozen:target.policy.uploads_frozen,scanner_ready:false,preview_ready:false};
            let value=execute_field_attachment_intake_with_live_readiness(c,FieldAttachmentIntakeCommand {actor,holding:&target.holding,company:&target.company,catalogue:&target.catalogue,clock:target.clock.as_ref(),input:&input,key:&key,availability:&availability},target.readiness.as_ref()).await?;
            let data = crate::field_attachment_intake_contracts::checked_field_intake_set(value.data).map_err(|_|wire_error())?;
            Ok(Ok(data))
        })});
        target.authorizer.execute_with_selected_transport_ingress_deferred(&host,proof,selection,(access(),FieldAttachmentIntakeCommandSources),callbacks).await?.map_err(Into::into)
    }.await;
    response(result, true)
}
