//! Owner-local axum router for the native beta feedback read.
//!
//! The Host + Holding slug pair selects one exact registered target and that
//! target's own reader/pool; the Company header supplies the only SQL bind.
//! Missing or duplicate headers and unknown targets fail closed with the same
//! status mapping as the other owner routers. No TypeScript business fallback
//! exists behind this route.

use crate::adapters::native_lane_34::application::{
    BetaFeedbackQuery, BetaInvitationQuery, EmailReceiptQuery, load, load_email_receipt,
    load_invitations,
};
use crate::adapters::native_lane_34::domain::{
    EmailReceiptResponse, FeedbackResponse, InvitationResponse, configuration_invalid,
    invalid_request, missing, valid_company_id, valid_delivery_key, valid_slug,
};
use crate::adapters::native_lane_34::postgres::{
    PgBetaFeedbackQuery, PgBetaInvitationQuery, PgEmailReceiptQuery,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, get, header, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const DELIVERY_KEY: HeaderName = HeaderName::from_static("x-erpx-delivery-key");

/// One exact registered Holding target. Duplicate or malformed pairs are
/// rejected at router construction; no prefix or wildcard resolution occurs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

/// Reader-per-target map. The key is exactly `(host, holding_slug)` so one
/// Holding can never borrow another Holding's database pool.
type Targets = BTreeMap<(String, String), Arc<dyn BetaFeedbackQuery>>;
type InvitationTargets = BTreeMap<(String, String), Arc<dyn BetaInvitationQuery>>;
type ReceiptTargets = BTreeMap<(String, String), Arc<dyn EmailReceiptQuery>>;

struct LaneState {
    targets: Targets,
    invitation_targets: InvitationTargets,
    receipt_targets: ReceiptTargets,
}

/// Validate the exact distinct target set and insert each reader under its
/// own `(host, holding_slug)` key. Duplicate keys fail closed.
fn configure_targets<R>(
    entries: Vec<(LaneTarget, R)>,
) -> Result<BTreeMap<(String, String), R>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in entries {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(configured)
}

fn from_targets(
    targets: Targets,
    invitation_targets: InvitationTargets,
    receipt_targets: ReceiptTargets,
) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/manage-erpx/beta-feedback",
            get(feedback),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/manage-erpx/beta-invitations",
            get(invitations),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/manage-erpx/sandbox-email/receipt",
            get(email_receipt),
        )
        .with_state(Arc::new(LaneState {
            targets,
            invitation_targets,
            receipt_targets,
        }))
}

/// Reader-injection constructor retained for tests and for composition code
/// that already owns reader instances. One reader is shared across targets.
pub fn router_with_reader(
    reader: Arc<dyn BetaFeedbackQuery>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let feedback = configure_targets(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )?;
    Ok(from_targets(feedback, BTreeMap::new(), BTreeMap::new()))
}

/// Per-Holding constructor: every target carries its own database pool, so
/// composition binds one `PgBetaFeedbackQuery` and one `PgBetaInvitationQuery`
/// per Holding from that same pool. Cross-Holding reads are impossible because
/// the resolved reader comes from the map entry that matched Host and slug.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut feedback_entries = Vec::new();
    let mut invitation_entries = Vec::new();
    let mut receipt_entries = Vec::new();
    for (target, pool) in targets {
        let feedback: Arc<dyn BetaFeedbackQuery> = Arc::new(PgBetaFeedbackQuery::new(pool.clone()));
        let invitation: Arc<dyn BetaInvitationQuery> = Arc::new(PgBetaInvitationQuery::new(pool.clone()));
        let receipt: Arc<dyn EmailReceiptQuery> = Arc::new(PgEmailReceiptQuery::new(pool));
        feedback_entries.push((target.clone(), feedback));
        invitation_entries.push((target.clone(), invitation));
        receipt_entries.push((target, receipt));
    }
    Ok(from_targets(
        configure_targets(feedback_entries)?,
        configure_targets(invitation_entries)?,
        configure_targets(receipt_entries)?,
    ))
}

/// Single-pool compatibility constructor. The caller supplies the pool; the
/// same pool is cloned for each registered target, preserving prior behavior.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid_request());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid_request()))
        .transpose()
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

fn bounded<T: serde::Serialize>(body: T) -> Response {
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

async fn feedback(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_company_id(value))
            .ok_or_else(invalid_request)?;
        let page = load(reader.as_ref(), company).await?;
        Ok::<_, BusinessError>(FeedbackResponse::from(page))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn invitations(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .invitation_targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_company_id(value))
            .ok_or_else(invalid_request)?;
        let page = load_invitations(reader.as_ref(), company).await?;
        Ok::<_, BusinessError>(InvitationResponse::from(page))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn email_receipt(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .receipt_targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let key = unique_header(request.headers(), DELIVERY_KEY)?
            .filter(|value| valid_delivery_key(value))
            .ok_or_else(invalid_request)?;
        let receipt = load_email_receipt(reader.as_ref(), key).await?;
        Ok::<_, BusinessError>(EmailReceiptResponse::from(receipt))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_34::application::{
        EmailReceiptFuture, FeedbackFuture, InvitationFuture,
    };
    use crate::adapters::native_lane_34::domain::{
        FeedbackItem, FeedbackPage, InvitationItem, InvitationPage,
    };
    use erpx_server_runtime::public::http_support::{Body, to_bytes};

    struct StaticReader {
        id: &'static str,
    }

    impl BetaFeedbackQuery for StaticReader {
        fn list<'a>(&'a self, _company: &'a str) -> FeedbackFuture<'a> {
            let id = self.id.to_owned();
            Box::pin(async move {
                Ok(FeedbackPage {
                    items: vec![FeedbackItem {
                        id: format!("{id}-row"),
                        body: "b".to_owned(),
                        created_at: 1,
                    }],
                })
            })
        }
    }

    struct InvitationStubReader {
        id: &'static str,
    }

    impl BetaInvitationQuery for InvitationStubReader {
        fn list<'a>(&'a self, _company: &'a str) -> InvitationFuture<'a> {
            let id = self.id.to_owned();
            Box::pin(async move {
                Ok(InvitationPage {
                    items: vec![InvitationItem {
                        id: format!("inv-{id}"),
                        email: "owner@example.test".to_owned(),
                        branch_id: "branch-1".to_owned(),
                        expires_at: 1_700_000_000_000,
                        accepted_at: None,
                        revoked_at: None,
                        invitation_state: "Invited".to_owned(),
                    }],
                })
            })
        }
    }

    fn invitation_reader(id: &'static str) -> Arc<dyn BetaInvitationQuery> {
        Arc::new(InvitationStubReader { id })
    }

    fn acme() -> LaneTarget {
        LaneTarget {
            host: "acme.example.test".to_owned(),
            holding_slug: "acme".to_owned(),
        }
    }

    fn northstar() -> LaneTarget {
        LaneTarget {
            host: "northstar.example.test".to_owned(),
            holding_slug: "northstar".to_owned(),
        }
    }

    fn reader(id: &'static str) -> Arc<dyn BetaFeedbackQuery> {
        Arc::new(StaticReader { id })
    }

    fn err<T>(result: Result<T, BusinessError>) -> BusinessError {
        result.err().expect("expected configuration error")
    }

    async fn call(state: Arc<LaneState>, host: &str, slug: &str) -> String {
        let request = Request::builder()
            .header(header::HOST, host)
            .header(CONTEXT_COMPANY, "acme:co")
            .body(Body::empty())
            .expect("request");
        let response = feedback(State(state), Path(slug.to_owned()), request).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1 << 20).await.expect("body");
        String::from_utf8(bytes.to_vec()).expect("utf8")
    }

    #[test]
    fn rejects_unconfigured_and_duplicate_targets() {
        assert_eq!(
            err(configure_targets(Vec::<(
                LaneTarget,
                Arc<dyn BetaFeedbackQuery>
            )>::new())),
            configuration_invalid()
        );
        let mut bad_host = acme();
        bad_host.host = String::new();
        assert_eq!(
            err(configure_targets(vec![(bad_host, reader("acme"))])),
            configuration_invalid()
        );
        let mut bad_slug = acme();
        bad_slug.holding_slug = "Acme".to_owned();
        assert_eq!(
            err(configure_targets(vec![(bad_slug, reader("acme"))])),
            configuration_invalid()
        );
        let duplicate = acme();
        assert_eq!(
            err(configure_targets(vec![
                (duplicate.clone(), reader("one")),
                (duplicate, reader("two")),
            ])),
            configuration_invalid()
        );
    }

    #[test]
    fn accepts_an_exact_distinct_target_set() {
        let configured = configure_targets(vec![
            (acme(), reader("acme")),
            (northstar(), reader("northstar")),
        ])
        .expect("targets");
        assert_eq!(configured.len(), 2);
        assert!(configured.contains_key(&("acme.example.test".to_owned(), "acme".to_owned())));
    }

    #[tokio::test]
    async fn two_targets_route_to_two_distinct_readers() {
        let acme_reader = reader("acme");
        let northstar_reader = reader("northstar");
        let targets = configure_targets(vec![
            (acme(), acme_reader.clone()),
            (northstar(), northstar_reader.clone()),
        ])
        .expect("targets");
        let first = targets
            .get(&("acme.example.test".to_owned(), "acme".to_owned()))
            .expect("acme reader");
        let second = targets
            .get(&("northstar.example.test".to_owned(), "northstar".to_owned()))
            .expect("northstar reader");
        assert!(!Arc::ptr_eq(first, second));
        assert_eq!(
            first.list("acme:co").await.expect("acme page").items[0].id,
            "acme-row"
        );
        assert_eq!(
            second.list("acme:co").await.expect("northstar page").items[0].id,
            "northstar-row"
        );

        let state = Arc::new(LaneState {
            targets,
            invitation_targets: BTreeMap::new(),
            receipt_targets: BTreeMap::new(),
        });
        assert!(
            call(state.clone(), "acme.example.test", "acme")
                .await
                .contains("acme-row")
        );
        assert!(
            call(state, "northstar.example.test", "northstar")
                .await
                .contains("northstar-row")
        );
    }

    #[tokio::test]
    async fn invitations_route_uses_each_holdings_own_reader() {
        let first = invitation_reader("acme");
        let second = invitation_reader("northstar");
        let targets =
            configure_targets(vec![(acme(), first.clone()), (northstar(), second.clone())])
                .expect("targets");
        assert!(!Arc::ptr_eq(
            targets
                .get(&("acme.example.test".to_owned(), "acme".to_owned()))
                .expect("acme"),
            targets
                .get(&("northstar.example.test".to_owned(), "northstar".to_owned()))
                .expect("northstar")
        ));
        let state = Arc::new(LaneState {
            targets: BTreeMap::new(),
            invitation_targets: targets,
            receipt_targets: BTreeMap::new(),
        });
        for (host, slug, expected) in [
            ("acme.example.test", "acme", "inv-acme"),
            ("northstar.example.test", "northstar", "inv-northstar"),
        ] {
            let request = Request::builder()
                .header(header::HOST, host)
                .header(CONTEXT_COMPANY, "acme:co")
                .body(Body::empty())
                .expect("request");
            let response = invitations(State(state.clone()), Path(slug.to_owned()), request).await;
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = to_bytes(response.into_body(), 1 << 20).await.expect("body");
            assert!(
                String::from_utf8(bytes.to_vec())
                    .expect("utf8")
                    .contains(expected)
            );
        }
    }

    #[test]
    fn router_with_reader_keeps_the_single_reader_convenience() {
        assert!(router_with_reader(reader("acme"), vec![acme()]).is_ok());
    }

    struct ReceiptReader;

    impl EmailReceiptQuery for ReceiptReader {
        fn receipt<'a>(&'a self, key: &'a str) -> EmailReceiptFuture<'a> {
            let delivery_key = key.to_owned();
            Box::pin(async move {
                Ok(Some(crate::adapters::native_lane_34::domain::EmailReceipt {
                    delivery_key,
                    recipient: "to@example.test".to_owned(),
                    subject: "S".to_owned(),
                    body_text: "body".to_owned(),
                    body_ciphertext: None,
                    provider_id: "sandbox-key-1".to_owned(),
                }))
            })
        }
    }

    #[tokio::test]
    async fn email_receipt_route_binds_only_the_delivery_key() {
        let receipt: Arc<dyn EmailReceiptQuery> = Arc::new(ReceiptReader);
        let receipt_targets = configure_targets(vec![(acme(), receipt)]).expect("targets");
        let state = Arc::new(LaneState {
            targets: BTreeMap::new(),
            invitation_targets: BTreeMap::new(),
            receipt_targets,
        });
        let request = Request::builder()
            .header(header::HOST, "acme.example.test")
            .header(DELIVERY_KEY, "portal:key")
            .body(Body::empty())
            .expect("request");
        let response = email_receipt(State(state), Path("acme".to_owned()), request).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1 << 20).await.expect("body");
        let body = String::from_utf8(bytes.to_vec()).expect("utf8");
        assert!(body.contains("portal:key"));
        assert!(body.contains("sandbox-key-1"));

        let bad = Request::builder()
            .header(header::HOST, "acme.example.test")
            .header(DELIVERY_KEY, "bad/key")
            .body(Body::empty())
            .expect("request");
        let state = Arc::new(LaneState {
            targets: BTreeMap::new(),
            invitation_targets: BTreeMap::new(),
            receipt_targets: configure_targets(vec![(acme(), Arc::new(ReceiptReader) as Arc<dyn EmailReceiptQuery>)])
                .expect("targets"),
        });
        let response = email_receipt(State(state), Path("acme".to_owned()), bad).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn maps_failure_kinds_to_the_owner_status_contract() {
        assert_eq!(
            status(FailureKind::InvalidInput),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(
            status(FailureKind::Unavailable),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}
