//! Private Field file ingress. Filesystem work occurs between short, independently
//! authorized SERIALIZABLE units; HTTP input never supplies storage authority.
use crate::{
    field_attachment_intake::FieldAttachmentIntakeAuthority,
    field_attachment_intake_http::{FieldIntakeTarget, HttpState, ingress, response},
    http_scope::BusinessScopeSlugs,
};
use axum::{
    Router,
    extract::{Path, RawQuery, Request, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use erpx_server_files::public::{attachment_scan::LocalSanitizationStore, field_visit_upload::*};
use erpx_server_identity_access::public::{
    AccountPolicy,
    account_holding_bindings::ConfiguredHoldingRegistry,
    effective_authority::{
        HoldingWorkSourcePreparation, SelectedIngressFuture, SelectedTransportCallbacks,
        SelectedWorkIngress, WorkAuthorizationRequest,
    },
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind};
use sqlx::{PgConnection, PgPool};
use std::{collections::BTreeMap, sync::Arc};
type Error = PgTransactionError<BusinessError>;
pub struct FieldUploadTarget<C> {
    pub intake: Arc<FieldIntakeTarget<C>>,
    pub pool: PgPool,
    pub writer: LocalFieldQuarantineWriter,
    pub store: LocalSanitizationStore,
}
struct UploadState<C> {
    ingress: HttpState<C>,
    targets: BTreeMap<(String, String), Arc<FieldUploadTarget<C>>>,
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_UPLOAD_RUNTIME_UNAVAILABLE",
        "The Field file runtime is unavailable.",
    )
}
fn invalid() -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", "Invalid file request.")
}
pub fn field_upload_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<FieldUploadTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut map = BTreeMap::new();
    let mut intake = BTreeMap::new();
    for target in targets {
        let t = &target.intake;
        if !policy
            .audience(&t.host)
            .and_then(|a| registry.expected_for_audience(&a).ok())
            .is_some_and(|h| h.slug == t.holding_slug && h.holding_id == t.holding)
        {
            return Err(unavailable());
        }
        target.writer.require_available()?;
        target.store.require_available()?;
        let key = (t.host.clone(), t.holding_slug.clone());
        intake.insert(key.clone(), t.clone());
        if map.insert(key, Arc::new(target)).is_some() {
            return Err(unavailable());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/field-service/attachment-sets/{id}/files",
            post(reserve::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/field-service/uploads/{id}/content",
            put(upload::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/field-service/attachment-links/{id}/status",
            get(status::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/field-service/attachment-links/{id}/content",
            get(content::<C>),
        )
        .with_state(Arc::new(UploadState {
            ingress: HttpState {
                policy,
                scopes,
                targets: intake,
            },
            targets: map,
        })))
}
#[derive(Clone)]
struct Sources;
impl HoldingWorkSourcePreparation for Sources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            crate::field_attachment::lock_attachment_sources(c, false).await?;
            erpx_server_manage_erpx::public::lock_beta_policy_sources(c).await?;
            Ok(())
        })
    }
}
struct CurrentActor;
impl FieldDenialActorCurrent for CurrentActor {
    fn require_current<'a>(
        &'a self,
        c: &'a mut PgConnection,
        actor: &'a erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext,
        now: i64,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            erpx_server_identity_access::public::selected_command_authority::require_selected_actor_current(c,actor,now).await
        })
    }
}
struct Quota;
impl FieldStorageQuota for Quota {
    fn storage_limit<'a>(
        &'a self,
        c: &'a mut PgConnection,
        company: &'a str,
    ) -> PgWorkFuture<'a, Option<i64>, BusinessError> {
        Box::pin(async move {
            erpx_server_manage_erpx::public::company_storage_limit(c, &CompanyId::new(company)?)
                .await
        })
    }
}
#[derive(Clone)]
struct DeferredBody {
    body: Arc<tokio::sync::Mutex<Option<axum::body::Body>>>,
    bytes: Arc<tokio::sync::OnceCell<Option<Vec<u8>>>>,
}
impl DeferredBody {
    fn new(body: axum::body::Body) -> Self {
        Self {
            body: Arc::new(tokio::sync::Mutex::new(Some(body))),
            bytes: Arc::new(tokio::sync::OnceCell::new()),
        }
    }
    async fn read(&self, limit: usize) -> Result<Arc<[u8]>, BusinessError> {
        self.bytes
            .get_or_init(|| async {
                let body = self.body.lock().await.take()?;
                axum::body::to_bytes(body, limit)
                    .await
                    .ok()
                    .map(|v| v.to_vec())
            })
            .await
            .as_ref()
            .map(|v| v.clone().into())
            .ok_or_else(invalid)
    }
}
#[derive(Clone)]
enum Action {
    Denial {
        id: String,
        variant: FieldContentVariant,
        reason: String,
    },
    ReserveBody {
        id: String,
        body: DeferredBody,
        key: Result<String, BusinessError>,
    },
    PrepareBody {
        id: String,
        body: DeferredBody,
        media: Result<String, BusinessError>,
    },
    Reserve {
        id: String,
        input: Arc<[u8]>,
        key: Result<String, BusinessError>,
    },
    Prepare {
        id: String,
        bytes: Arc<[u8]>,
        media: String,
    },
    Complete {
        id: String,
        pin: FieldPublicationPin,
        receipt: Arc<FieldQuarantineReceipt>,
    },
    Status {
        id: String,
    },
    Content {
        id: String,
        variant: FieldContentVariant,
    },
    ContentQuery {
        id: String,
        query: Option<String>,
    },
    Finish {
        bytes: Arc<FieldContentBytes>,
    },
}
impl Action {
    fn write(&self) -> bool {
        matches!(
            self,
            Self::ReserveBody { .. }
                | Self::PrepareBody { .. }
                | Self::Reserve { .. }
                | Self::Prepare { .. }
                | Self::Complete { .. }
        )
    }
}
enum Output {
    Json(serde_json::Value),
    Prepared(FieldUploadPreparation),
    Witness(FieldContentWitness),
    Content(FieldContentResponse),
}
fn checked(
    value: serde_json::Value,
    check: fn(
        serde_json::Value,
    ) -> Result<
        serde_json::Value,
        crate::field_attachment_intake_contracts::FieldIntakeWireError,
    >,
) -> Result<serde_json::Value, Error> {
    check(value).map_err(|_| {
        erpx_server_runtime::public::postgres::PersistenceError::new("FIELD_UPLOAD_WIRE_INVALID")
            .into()
    })
}
fn completion(value: FieldUploadCompletion) -> serde_json::Value {
    serde_json::json!({"uploadIntentId":value.upload_intent_id,"attachmentLinkId":value.attachment_link_id,"sourceId":value.source_id,"state":value.source_state,"status":"scan_pending"})
}
fn target<C>(
    s: &UploadState<C>,
    headers: &HeaderMap,
    slug: &str,
) -> Result<Arc<FieldUploadTarget<C>>, BusinessError> {
    let host = erpx_server_identity_access::public::account_inbound_http::unique_request_header(
        headers, "host",
    )?
    .ok_or_else(unavailable)?;
    s.targets
        .get(&(host.into(), slug.into()))
        .cloned()
        .ok_or_else(unavailable)
}
async fn execute<C: Clock + Send + Sync + 'static>(
    s: &UploadState<C>,
    headers: &HeaderMap,
    slug: &str,
    action: Action,
) -> Result<Output, Error> {
    let write = action.write();
    let (intake, host, proof, selection) = ingress(&s.ingress, headers, slug.into(), write)?;
    let ingress_target = intake.clone();
    let action_target = intake.clone();
    let callbacks = SelectedTransportCallbacks::<_, Action, BusinessError, _>::new::<Output>(
        move |selected: &SelectedWorkIngress<'_>| -> SelectedIngressFuture<Action, BusinessError> {
            let action = action.clone();
            let target = ingress_target.clone();
            let holding = selected.holding_id().clone();
            let company = selected.company_id().clone();
            Box::pin(async move {
                if holding != target.holding
                    || matches!(&target.company,crate::service_company_ownership::ServiceCompanyOwnership::Fixed(expected) if expected!=&company)
                {
                    return Err(invalid());
                }
                if target
                    .companies
                    .as_ref()
                    .is_some_and(|companies| !companies.contains(&company))
                {
                    return Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "FIELD_FILES_COMPANY_UNAVAILABLE",
                        "Field files are unavailable for the selected Company.",
                    ));
                }
                if action.write() {
                    target.policy.require_create()?;
                } else {
                    target.policy.require_read()?;
                }
                match action {
                    Action::ContentQuery { id, query } => {
                        let variant = match query.as_deref() {
                            None | Some("") | Some("variant=original") => {
                                FieldContentVariant::Original
                            }
                            Some("variant=preview") => FieldContentVariant::Preview,
                            _ => return Err(invalid()),
                        };
                        Ok(Action::Content { id, variant })
                    }
                    Action::ReserveBody { id, body, key } => Ok(Action::Reserve {
                        id,
                        input: body.read(64 * 1024).await?,
                        key,
                    }),
                    Action::PrepareBody { id, body, media } => Ok(Action::Prepare {
                        id,
                        bytes: body.read(FIELD_UPLOAD_MAX_BYTES as usize).await?,
                        media: media?,
                    }),
                    action => Ok(action),
                }
            })
        },
        |_, e| Err(e),
        move |c, actor, action: Action| {
            let t = action_target.clone();
            Box::pin(async move {
                t.require_company(actor)?;
                let company = actor.selection().workspace().company_id.clone();
                let now = t.clock.now().get();
                let authority = FieldAttachmentIntakeAuthority::new(
                    actor,
                    &t.holding,
                    &company,
                    &t.catalogue,
                    t.clock.as_ref(),
                );
                let read_only = t.policy.require_read()?;
                let result = match action {
                    Action::Denial {
                        id,
                        variant,
                        reason,
                    } => {
                        record_field_content_denial(
                            c,
                            &CurrentActor,
                            actor,
                            &id,
                            if matches!(variant, FieldContentVariant::Original) {
                                "original"
                            } else {
                                "preview"
                            },
                            &reason,
                            t.clock.as_ref(),
                        )
                        .await?;
                        Output::Json(serde_json::Value::Null)
                    }
                    Action::ReserveBody { .. }
                    | Action::PrepareBody { .. }
                    | Action::ContentQuery { .. } => {
                        return Err(unavailable().into());
                    }
                    Action::Reserve { id, input, key } => {
                        let key = key?;
                        let input: FieldFileReservation =
                            serde_json::from_slice(&input).map_err(|_| invalid())?;
                        // A fresh reservation requires current providers, not a stale startup flag.
                        // Replays are still resolved inside the owner; a failed readiness cannot write.
                        let result = reserve_field_file(
                            c, &authority, &Quota, actor, &t.holding, &id, &input, &key, now,
                        )
                        .await?;
                        if !result.replay {
                            let p = t.readiness.probe().await?;
                            if !p.scanner_enabled || !p.scanner_ready || !p.preview_ready {
                                return Err(unavailable().into());
                            }
                        }
                        let value = serde_json::json!({"uploadIntentId":result.upload_intent_id,"attachmentLinkId":result.attachment_link_id,"expiresAt":result.expires_at,"uploadUrl":format!("/api/v1/holdings/{}/field-service/uploads/{}/content",t.holding_slug,result.upload_intent_id)});
                        Output::Json(checked(value,crate::field_attachment_intake_contracts::checked_field_file_reservation)?)
                    }
                    Action::Prepare { id, bytes, media } => Output::Prepared(
                        prepare_field_upload(
                            c,
                            &authority,
                            actor,
                            &id,
                            &media,
                            bytes.to_vec(),
                            now,
                        )
                        .await?,
                    ),
                    Action::Complete { id, pin, receipt } => {
                        let result = complete_field_upload(
                            c,
                            &authority,
                            actor,
                            &id,
                            &receipt,
                            &pin,
                            t.clock.as_ref(),
                        )
                        .await?;
                        verify_field_completion_pin(c, &pin, t.clock.as_ref()).await?;
                        Output::Json(checked(
                            completion(result),
                            crate::field_attachment_intake_contracts::checked_field_file_upload,
                        )?)
                    }
                    Action::Status { id } => Output::Json(checked(
                        read_field_link_status(c, &authority, actor, &id, read_only, now).await?,
                        crate::field_attachment_intake_contracts::checked_field_file_status,
                    )?),
                    Action::Content { id, variant } => Output::Witness(
                        prepare_field_content(c, &authority, actor, &id, variant, read_only, now)
                            .await?,
                    ),
                    Action::Finish { bytes } => Output::Content(
                        finish_field_content(
                            c,
                            &authority,
                            actor,
                            bytes.as_ref().clone(),
                            read_only,
                            now,
                        )
                        .await?,
                    ),
                };
                Ok(Ok(result))
            })
        },
    );
    intake
        .authorizer
        .execute_with_selected_transport_ingress_deferred(
            &host,
            proof,
            selection,
            (
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("field_service".into()),
                    destination: "service/orders".into(),
                    api: true,
                },
                Sources,
            ),
            callbacks,
        )
        .await?
        .map_err(Into::into)
}
fn json_output(value: Result<Output, Error>, created: bool) -> Response {
    response(
        value.and_then(|v| match v {
            Output::Json(v) => Ok(v),
            _ => Err(unavailable().into()),
        }),
        created,
    )
}
async fn reserve<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<UploadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let result = async {
        // The bounded body is read by selected ingress only after current authority.
        let key = erpx_server_identity_access::public::account_inbound_http::unique_request_header(
            &parts.headers,
            "idempotency-key",
        )?
        .map(str::to_owned)
        .ok_or_else(|| {
            BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "A valid Idempotency-Key is required.",
            )
        });
        execute(
            &s,
            &parts.headers,
            &slug,
            Action::ReserveBody {
                id,
                body: DeferredBody::new(body),
                key,
            },
        )
        .await
    }
    .await;
    json_output(result, true)
}
async fn status<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<UploadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    json_output(
        execute(&s, &headers, &slug, Action::Status { id }).await,
        false,
    )
}
async fn upload<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<UploadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let result = async {
        let target = target(&s, &parts.headers, &slug)?;
        let media =
            erpx_server_identity_access::public::account_inbound_http::unique_request_header(
                &parts.headers,
                "content-type",
            )?
            .map(str::to_owned)
            .ok_or_else(invalid);
        match execute(
            &s,
            &parts.headers,
            &slug,
            Action::PrepareBody {
                id: id.clone(),
                body: DeferredBody::new(body),
                media,
            },
        )
        .await?
        {
            Output::Prepared(FieldUploadPreparation::Replay(value)) => Ok(Output::Json(checked(
                completion(value),
                crate::field_attachment_intake_contracts::checked_field_file_upload,
            )?)),
            Output::Prepared(FieldUploadPreparation::Pending { publication, pin }) => {
                let receipt = publish_pinned_field_upload(
                    &target.writer,
                    &target.pool,
                    &pin,
                    publication,
                    target.intake.clock.as_ref(),
                )
                .await?;
                execute(
                    &s,
                    &parts.headers,
                    &slug,
                    Action::Complete {
                        id,
                        pin,
                        receipt: Arc::new(receipt),
                    },
                )
                .await
            }
            _ => Err(unavailable().into()),
        }
    }
    .await;
    let mut response = json_output(result, false);
    if response.status() == axum::http::StatusCode::OK {
        *response.status_mut() = axum::http::StatusCode::ACCEPTED;
    }
    response
}
async fn content<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<UploadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let target = target(&s, &headers, &slug)?;
        let witness = match execute(
            &s,
            &headers,
            &slug,
            Action::ContentQuery {
                id: id.clone(),
                query: query.clone(),
            },
        )
        .await?
        {
            Output::Witness(v) => v,
            _ => return Err(unavailable().into()),
        };
        let bytes = read_field_content_bytes(&target.store, witness).await?;
        execute(
            &s,
            &headers,
            &slug,
            Action::Finish {
                bytes: Arc::new(bytes),
            },
        )
        .await
    }
    .await;
    match result {
        Ok(Output::Content(value)) => {
            use sha2::{Digest, Sha256};
            let checksum = hex::encode(Sha256::digest(&value.bytes));
            let mut response = value.bytes.into_response();
            let media = HeaderValue::from_str(&value.media_type);
            let filename = if value.inline {
                format!("preview-{}", value.filename)
            } else {
                value.filename
            };
            let encoded = filename
                .as_bytes()
                .iter()
                .map(|b| {
                    if b.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(b) {
                        (*b as char).to_string()
                    } else {
                        format!("%{b:02X}")
                    }
                })
                .collect::<String>();
            let disposition = HeaderValue::from_str(&format!(
                "{}; filename*=UTF-8''{}",
                if value.inline { "inline" } else { "attachment" },
                encoded
            ));
            let (Ok(media), Ok(disposition)) = (media, disposition) else {
                return json_output(Err(unavailable().into()), false);
            };
            response.headers_mut().insert(
                "x-erpx-checksum-sha256",
                HeaderValue::from_str(&checksum)
                    .map_err(|_| unavailable())
                    .expect("hex digest is an HTTP header"),
            );
            response.headers_mut().insert(header::CONTENT_TYPE, media);
            response
                .headers_mut()
                .insert(header::CONTENT_DISPOSITION, disposition);
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response.headers_mut().insert(
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            );
            response
        }
        Err(e) => {
            if let PgTransactionError::Business(ref failure) = e {
                let variant = if query.as_deref() == Some("variant=preview") {
                    FieldContentVariant::Preview
                } else {
                    FieldContentVariant::Original
                };
                let _ = execute(
                    &s,
                    &headers,
                    &slug,
                    Action::Denial {
                        id,
                        variant,
                        reason: failure.code.to_string(),
                    },
                )
                .await;
            }
            json_output(Err(e), false)
        }
        _ => json_output(Err(unavailable().into()), false),
    }
}
