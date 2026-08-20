//! Typed, same-origin delivery boundary for the authorized process shell.
//!
//! Browser input is limited to presentation paging. The server resolves the
//! active principal, role, scope, access token, holding window, and navigation
//! capabilities from the protected BFF session before calling an owner API.

#[cfg(feature = "server")]
use chrono::DateTime;
use dioxus::prelude::dioxus_fullstack;
#[cfg(feature = "server")]
use dioxus::prelude::dioxus_server;
use erpx_delivery_contract::{DeliveryError, DeliveryErrorMeta};
use erpx_ui_core::{ReadModelAvailability, ReadModelFreshness};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(feature = "server")]
const MAX_PROCESS_ITEMS: usize = 48;
#[cfg_attr(not(feature = "server"), allow(dead_code))]
const MAX_PROCESS_PAGE_SIZE: u32 = 48;
const OWNER_WINDOW_SIZE: u32 = 100;
#[cfg(feature = "server")]
const HOLDING_WINDOW_SIZE: u32 = 24;
#[cfg(feature = "server")]
const MAX_PROCESS_TEXT_BYTES: usize = 512;
#[cfg(feature = "server")]
const MAX_PROCESS_REF_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessConsistency {
    Eventual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizedProcessRequest {
    pub cursor: Option<String>,
    pub page_size: u32,
    pub locale: String,
    pub consistency: ProcessConsistency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessKind {
    HoldingBackup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStage {
    Pending,
    Running,
    RetryScheduled,
    Failed,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessReadModel {
    pub key: String,
    pub kind: ProcessKind,
    pub title: String,
    pub owner: String,
    pub stage: ProcessStage,
    pub state_label: String,
    pub attempts: u32,
    pub max_attempts: u32,
    pub retryable: bool,
    pub failure_category: Option<String>,
    pub next_attempt_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub target_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessSourceContribution {
    pub source: String,
    pub label: String,
    pub availability: ReadModelAvailability,
    pub freshness: ReadModelFreshness,
    pub item_count: u32,
    pub message: String,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizedProcessPage {
    pub scope: String,
    pub measured_at: String,
    pub availability: ReadModelAvailability,
    pub freshness: ReadModelFreshness,
    pub reason_code: Option<String>,
    pub items: Vec<ProcessReadModel>,
    pub contributions: Vec<ProcessSourceContribution>,
    pub page_size: u32,
    pub next_cursor: Option<String>,
    pub redacted_fields: Vec<String>,
    pub authorization_context_version: Option<u64>,
    pub correlation_id: Uuid,
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn error_meta(retryable: bool, retry_after_ms: Option<u64>) -> DeliveryErrorMeta {
    DeliveryErrorMeta {
        correlation_id: Uuid::new_v4(),
        retry_after_ms,
        retryable,
    }
}

pub fn valid_process_cursor(value: &str) -> bool {
    value
        .strip_prefix("offset:")
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|offset| offset < OWNER_WINDOW_SIZE as usize)
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn valid_request(request: &AuthorizedProcessRequest) -> bool {
    matches!(request.locale.as_str(), "en" | "th")
        && request.page_size > 0
        && request.page_size <= MAX_PROCESS_PAGE_SIZE
        && request.cursor.as_deref().is_none_or(valid_process_cursor)
}

#[cfg(feature = "server")]
fn valid_text(value: &str, maximum: usize, empty: bool) -> bool {
    (empty || !value.trim().is_empty())
        && value.len() <= maximum
        && !value.chars().any(char::is_control)
}

#[cfg(feature = "server")]
fn valid_reason(value: &str) -> bool {
    valid_text(value, MAX_PROCESS_REF_BYTES, false)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

#[cfg(feature = "server")]
impl AuthorizedProcessPage {
    fn is_bounded(&self) -> bool {
        self.page_size > 0
            && self.page_size <= MAX_PROCESS_PAGE_SIZE
            && self.items.len() <= self.page_size as usize
            && self.items.len() <= MAX_PROCESS_ITEMS
            && self.contributions.len() <= 4
            && valid_text(&self.scope, MAX_PROCESS_TEXT_BYTES, false)
            && DateTime::parse_from_rfc3339(&self.measured_at).is_ok()
            && self.reason_code.as_deref().is_none_or(valid_reason)
            && self.next_cursor.as_deref().is_none_or(valid_process_cursor)
            && self.items.iter().all(ProcessReadModel::is_bounded)
            && self
                .contributions
                .iter()
                .all(ProcessSourceContribution::is_bounded)
            && self.redacted_fields.iter().all(|value| valid_reason(value))
    }
}

#[cfg(feature = "server")]
impl ProcessReadModel {
    fn is_bounded(&self) -> bool {
        valid_text(&self.key, MAX_PROCESS_REF_BYTES, false)
            && valid_text(&self.title, MAX_PROCESS_TEXT_BYTES, false)
            && valid_text(&self.owner, MAX_PROCESS_REF_BYTES, false)
            && valid_text(&self.state_label, MAX_PROCESS_REF_BYTES, false)
            && self.attempts <= self.max_attempts
            && self.max_attempts > 0
            && self.failure_category.as_deref().is_none_or(valid_reason)
            && self
                .next_attempt_at
                .as_deref()
                .is_none_or(|value| DateTime::parse_from_rfc3339(value).is_ok())
            && DateTime::parse_from_rfc3339(&self.created_at).is_ok()
            && DateTime::parse_from_rfc3339(&self.updated_at).is_ok()
            && self
                .completed_at
                .as_deref()
                .is_none_or(|value| DateTime::parse_from_rfc3339(value).is_ok())
            && self.target_path.starts_with("/platform/holdings/")
            && valid_text(&self.target_path, MAX_PROCESS_TEXT_BYTES, false)
    }
}

#[cfg(feature = "server")]
impl ProcessSourceContribution {
    fn is_bounded(&self) -> bool {
        valid_reason(&self.source)
            && valid_text(&self.label, MAX_PROCESS_REF_BYTES, false)
            && self.item_count <= OWNER_WINDOW_SIZE
            && valid_text(&self.message, MAX_PROCESS_TEXT_BYTES, true)
            && self.reason_code.as_deref().is_none_or(valid_reason)
    }
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/processes/list")]
pub async fn list_authorized_processes(
    request: AuthorizedProcessRequest,
) -> dioxus::prelude::ServerFnResult<Result<AuthorizedProcessPage, DeliveryError>> {
    if !valid_request(&request) {
        return Ok(Err(DeliveryError::Validation(error_meta(false, None))));
    }
    #[cfg(feature = "server")]
    {
        return Ok(server::list(request).await);
    }
    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[cfg(feature = "server")]
mod server {
    use chrono::Utc;
    use dioxus::fullstack::FullstackContext;
    use dioxus::server::axum::http::{HeaderName, HeaderValue, header};
    use erpx_proto::{
        common::{
            ActorAttribution, ActorKind, AuditContext, AuthorityScope, EventualConsistency,
            QueryConsistency, QueryContext, query_consistency,
        },
        erpx::platform_administration as platform,
    };
    use erpx_ui_core::{NavGroup, NavItem};
    use tonic::{Request, metadata::MetadataValue, transport::Channel};

    use super::*;
    use crate::bff::{BffCredential, BffSessionContext, BffState};

    #[derive(Clone)]
    struct ProcessAuthority {
        state: BffState,
        credential: BffCredential,
        scope_key: String,
        scope: AuthorityScope,
        actor_kind: ActorKind,
        authorization_context_version: Option<u64>,
    }

    pub(super) fn owner_reason(status: &tonic::Status) -> &'static str {
        match status.code() {
            tonic::Code::Unauthenticated => "owner_unauthenticated",
            tonic::Code::PermissionDenied => "owner_forbidden",
            tonic::Code::InvalidArgument | tonic::Code::OutOfRange => "owner_request_invalid",
            tonic::Code::Unavailable
            | tonic::Code::DeadlineExceeded
            | tonic::Code::Cancelled
            | tonic::Code::FailedPrecondition => "owner_unavailable",
            _ => "owner_response_invalid",
        }
    }

    fn authority() -> Result<ProcessAuthority, DeliveryError> {
        let context = FullstackContext::current()
            .ok_or_else(|| DeliveryError::Unavailable(error_meta(true, Some(500))))?;
        context.add_response_header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
        let state = context
            .extension::<BffState>()
            .ok_or_else(|| DeliveryError::Unavailable(error_meta(true, Some(500))))?;
        let session = context
            .extension::<BffSessionContext>()
            .ok_or_else(|| DeliveryError::Unauthenticated(error_meta(false, None)))?;
        let credential = session
            .credential
            .ok_or_else(|| DeliveryError::Unauthenticated(error_meta(false, None)))?;
        let scope_key = session
            .snapshot
            .active_role
            .and_then(|role| role.scope_key)
            .ok_or_else(|| DeliveryError::Forbidden(error_meta(false, None)))?;
        let authorization_context_version = session
            .bundle
            .as_ref()
            .and_then(erpx_web_session::WebSessionBundle::active_account)
            .and_then(|account| account.authorization_context_version);
        let (scope, actor_kind) = crate::bff_navigation::authority_scope(&scope_key)
            .map_err(|_| DeliveryError::Forbidden(error_meta(false, None)))?;
        Ok(ProcessAuthority {
            state,
            credential,
            scope_key,
            scope,
            actor_kind,
            authorization_context_version,
        })
    }

    async fn navigation(authority: &ProcessAuthority) -> Result<Vec<NavGroup>, DeliveryError> {
        let development_context_token = authority
            .credential
            .development_context_token
            .as_ref()
            .map(|token| token.expose().to_owned());
        crate::bff_navigation::fetch_navigation_manifest(
            authority.state.grpc_origin(),
            authority.credential.access_token.expose().to_owned(),
            authority.credential.principal_ref.clone(),
            authority.credential.session_id.to_string(),
            authority.scope_key.clone(),
            development_context_token,
            false,
        )
        .await
        .map_err(|_| DeliveryError::Unavailable(error_meta(true, Some(500))))
    }

    fn route_item<'a>(groups: &'a [NavGroup], path: &str) -> Option<&'a NavItem> {
        groups
            .iter()
            .flat_map(|group| group.items.iter())
            .filter(|item| item.is_authorized())
            .find(|item| item.path == path)
    }

    fn actor(authority: &ProcessAuthority) -> ActorAttribution {
        ActorAttribution {
            principal_ref: authority.credential.principal_ref.clone(),
            kind: authority.actor_kind as i32,
            session_id: authority.credential.session_id.to_string(),
            represented_principal_ref: String::new(),
            support_capability_id: String::new(),
        }
    }

    fn query_context(
        authority: &ProcessAuthority,
        purpose: &str,
        correlation_id: Uuid,
    ) -> QueryContext {
        QueryContext {
            contract_version: 1,
            request_id: Uuid::new_v4().to_string(),
            correlation_id: correlation_id.to_string(),
            actor: Some(actor(authority)),
            active_scope: Some(authority.scope.clone()),
            consistency: Some(QueryConsistency {
                requirement: Some(query_consistency::Requirement::Eventual(
                    EventualConsistency {},
                )),
            }),
            audit: Some(AuditContext {
                purpose: purpose.into(),
                customer_visible: true,
                module: "authorized-processes".into(),
                correlation_id: correlation_id.to_string(),
            }),
        }
    }

    fn authenticated<T>(
        authority: &ProcessAuthority,
        message: T,
    ) -> Result<Request<T>, DeliveryError> {
        let mut request = Request::new(message);
        let session_id = authority.credential.session_id.to_string();
        for (name, value) in [
            (
                "x-erpx-access-token",
                authority.credential.access_token.expose(),
            ),
            ("x-erpx-actor", authority.credential.principal_ref.as_str()),
            ("x-erpx-session", session_id.as_str()),
        ] {
            request.metadata_mut().insert(
                name,
                MetadataValue::try_from(value)
                    .map_err(|_| DeliveryError::Unauthenticated(error_meta(false, None)))?,
            );
        }
        if let Some(token) = authority.credential.development_context_token.as_ref() {
            request.metadata_mut().insert(
                "x-erpx-dev-persona-context",
                MetadataValue::try_from(token.expose())
                    .map_err(|_| DeliveryError::Unauthenticated(error_meta(false, None)))?,
            );
        }
        Ok(request)
    }

    async fn channel(authority: &ProcessAuthority) -> Result<Channel, DeliveryError> {
        tonic::transport::Endpoint::from_shared(authority.state.grpc_origin().to_owned())
            .map_err(|_| DeliveryError::Unavailable(error_meta(true, Some(500))))?
            .connect()
            .await
            .map_err(|_| DeliveryError::Unavailable(error_meta(true, Some(500))))
    }

    async fn platform_holding_ids(
        channel: Channel,
        authority: &ProcessAuthority,
    ) -> Result<(Vec<String>, bool), tonic::Status> {
        let correlation_id = Uuid::new_v4();
        let response = platform::platform_administration_query_service_client::PlatformAdministrationQueryServiceClient::new(channel)
            .list_operational_holdings(
                authenticated(
                    authority,
                    platform::ListPlatformHoldingsRequest {
                        page_size: HOLDING_WINDOW_SIZE,
                        after_holding_id: String::new(),
                        context: Some(query_context(
                            authority,
                            "resolve server-owned process holding window",
                            correlation_id,
                        )),
                        search: String::new(),
                        lifecycle: String::new(),
                        billing_state: String::new(),
                        plan: String::new(),
                        module: String::new(),
                        health: String::new(),
                    },
                )
                .map_err(|_| tonic::Status::unauthenticated("request metadata unavailable"))?,
            )
            .await
            ?
            .into_inner();
        if response.correlation_id != correlation_id.to_string() {
            return Err(tonic::Status::internal("owner correlation mismatch"));
        }
        let mut holding_ids = Vec::with_capacity(response.holdings.len());
        for holding in response.holdings {
            if Uuid::parse_str(&holding.holding_id).is_err() {
                return Err(tonic::Status::internal("owner holding reference invalid"));
            }
            holding_ids.push(holding.holding_id);
        }
        Ok((holding_ids, response.has_more))
    }

    pub(super) fn unavailable_contribution(
        reason: &'static str,
        thai: bool,
    ) -> ProcessSourceContribution {
        ProcessSourceContribution {
            source: "holding_backups".into(),
            label: if thai {
                "งานสำรองข้อมูลโฮลดิ้ง".into()
            } else {
                "Holding backup jobs".into()
            },
            availability: ReadModelAvailability::Unavailable,
            freshness: ReadModelFreshness::Unknown,
            item_count: 0,
            message: if thai {
                "ไม่สามารถอ่านสถานะงานจากบริการเจ้าของข้อมูลได้".into()
            } else {
                "The bounded-context owner could not provide process state.".into()
            },
            reason_code: Some(reason.into()),
        }
    }

    fn stage(state: &str) -> Option<(ProcessStage, bool)> {
        match state {
            "pending" => Some((ProcessStage::Pending, false)),
            "claimed" => Some((ProcessStage::Running, false)),
            "retryable" => Some((ProcessStage::RetryScheduled, true)),
            "dead_letter" | "failed" => Some((ProcessStage::Failed, false)),
            "completed" => Some((ProcessStage::Completed, false)),
            _ => None,
        }
    }

    fn state_label(state: &str, thai: bool) -> &'static str {
        match (state, thai) {
            ("pending", false) => "Pending",
            ("pending", true) => "รอดำเนินการ",
            ("claimed", false) => "Running",
            ("claimed", true) => "กำลังดำเนินการ",
            ("retryable", false) => "Retry scheduled",
            ("retryable", true) => "กำหนดลองใหม่แล้ว",
            ("dead_letter" | "failed", false) => "Failed",
            ("dead_letter" | "failed", true) => "ล้มเหลว",
            ("completed", false) => "Completed",
            ("completed", true) => "เสร็จสมบูรณ์",
            _ => "",
        }
    }

    pub(super) async fn list(
        request: AuthorizedProcessRequest,
    ) -> Result<AuthorizedProcessPage, DeliveryError> {
        let authority = authority()?;
        let groups = navigation(&authority).await?;
        let thai = request.locale == "th";
        let measured_at = Utc::now();
        let source_authorized = authority.scope_key == "platform"
            && route_item(&groups, "/platform/holdings").is_some();
        let mut items = Vec::new();
        let mut contributions = Vec::new();
        let reason_code;

        if source_authorized {
            let transport = channel(&authority).await?;
            match platform_holding_ids(transport.clone(), &authority).await {
                Err(status) => {
                    let reason = owner_reason(&status);
                    contributions.push(unavailable_contribution(reason, thai));
                    reason_code = Some(reason.into());
                }
                Ok((holding_ids, holdings_truncated)) => {
                    let mut successful_calls = 0usize;
                    let mut unavailable_calls = 0usize;
                    let mut first_owner_failure = None;
                    let mut owner_window_truncated = holdings_truncated;
                    let mut invalid_reason = None;
                    for holding_id in holding_ids {
                        let remaining = OWNER_WINDOW_SIZE as usize - items.len();
                        if remaining == 0 {
                            owner_window_truncated = true;
                            break;
                        }
                        let correlation_id = Uuid::new_v4();
                        let response = platform::platform_administration_query_service_client::PlatformAdministrationQueryServiceClient::new(transport.clone())
                            .list_holding_backup_jobs(authenticated(
                                &authority,
                                platform::ListHoldingBackupJobsRequest {
                                    holding_id: holding_id.clone(),
                                    page_size: remaining as u32,
                                    context: Some(query_context(
                                        &authority,
                                        "read authorized process lifecycle state",
                                        correlation_id,
                                    )),
                                },
                            )?)
                            .await;
                        let response = match response {
                            Err(status) => {
                                unavailable_calls += 1;
                                if first_owner_failure.is_none() {
                                    first_owner_failure = Some(owner_reason(&status));
                                }
                                continue;
                            }
                            Ok(response) => response.into_inner(),
                        };
                        if response.correlation_id != correlation_id.to_string() {
                            invalid_reason = Some("owner_correlation_mismatch");
                            break;
                        }
                        successful_calls += 1;
                        let response_count = response.jobs.len();
                        if response_count == remaining {
                            owner_window_truncated = true;
                        }
                        for job in response.jobs {
                            if job.holding_id != holding_id || Uuid::parse_str(&job.job_id).is_err()
                            {
                                invalid_reason = Some("owner_reference_invalid");
                                break;
                            }
                            let Some((process_stage, retryable)) = stage(&job.state) else {
                                invalid_reason = Some("owner_state_invalid");
                                break;
                            };
                            if !valid_text(&job.kind, MAX_PROCESS_REF_BYTES, false)
                                || job.max_attempts == 0
                                || job.attempts > job.max_attempts
                                || DateTime::parse_from_rfc3339(&job.created_at).is_err()
                                || DateTime::parse_from_rfc3339(&job.updated_at).is_err()
                                || (retryable
                                    && DateTime::parse_from_rfc3339(&job.available_at).is_err())
                            {
                                invalid_reason = Some("owner_payload_invalid");
                                break;
                            }
                            let completed_at = (process_stage == ProcessStage::Completed)
                                .then(|| job.updated_at.clone());
                            items.push(ProcessReadModel {
                                key: format!("holding-backup:{holding_id}:{}", job.job_id),
                                kind: ProcessKind::HoldingBackup,
                                title: if thai {
                                    format!("สำรองข้อมูลโฮลดิ้ง · {}", job.kind)
                                } else {
                                    format!("Holding backup · {}", job.kind)
                                },
                                owner: "platform_administration".into(),
                                stage: process_stage,
                                state_label: state_label(&job.state, thai).into(),
                                attempts: job.attempts,
                                max_attempts: job.max_attempts,
                                retryable,
                                failure_category: job
                                    .error_present
                                    .then(|| "owner_error_recorded".into()),
                                next_attempt_at: retryable.then(|| job.available_at.clone()),
                                created_at: job.created_at,
                                updated_at: job.updated_at,
                                completed_at,
                                target_path: format!("/platform/holdings/{holding_id}"),
                            });
                        }
                        if invalid_reason.is_some() {
                            break;
                        }
                    }

                    if let Some(reason) = invalid_reason {
                        items.clear();
                        contributions.push(unavailable_contribution(reason, thai));
                        reason_code = Some(reason.into());
                    } else {
                        let owner_count = items.len();
                        let partial = owner_window_truncated || unavailable_calls > 0;
                        let availability = if successful_calls == 0 && unavailable_calls > 0 {
                            ReadModelAvailability::Unavailable
                        } else if partial {
                            ReadModelAvailability::Partial
                        } else if owner_count == 0 {
                            ReadModelAvailability::Empty
                        } else {
                            ReadModelAvailability::Available
                        };
                        let contribution_reason = if successful_calls == 0 && unavailable_calls > 0
                        {
                            first_owner_failure.or(Some("owner_unavailable"))
                        } else if unavailable_calls > 0 {
                            Some("owner_partial_unavailable")
                        } else if owner_window_truncated {
                            Some("owner_window_truncated")
                        } else {
                            None
                        };
                        reason_code = contribution_reason.map(str::to_owned);
                        contributions.push(ProcessSourceContribution {
                            source: "holding_backups".into(),
                            label: if thai {
                                "งานสำรองข้อมูลโฮลดิ้ง".into()
                            } else {
                                "Holding backup jobs".into()
                            },
                            availability,
                            freshness: if successful_calls == 0 && unavailable_calls > 0 {
                                ReadModelFreshness::Unknown
                            } else {
                                ReadModelFreshness::Live
                            },
                            item_count: owner_count as u32,
                            message: match (availability, thai) {
                                (ReadModelAvailability::Unavailable, true) => {
                                    "ไม่สามารถอ่านสถานะงานจากบริการเจ้าของข้อมูลได้".into()
                                }
                                (ReadModelAvailability::Unavailable, false) => {
                                    "The bounded-context owner could not provide process state."
                                        .into()
                                }
                                (ReadModelAvailability::Empty, true) => {
                                    "บริการเจ้าของข้อมูลยืนยันว่าไม่มีงานสำรองข้อมูลในขอบเขตนี้".into()
                                }
                                (ReadModelAvailability::Empty, false) => {
                                    "The owner reports no backup jobs in this bounded scope."
                                        .into()
                                }
                                (ReadModelAvailability::Partial, true) => {
                                    "แสดงเฉพาะ lifecycle ที่ยืนยันได้จากหน้าต่างข้อมูลเจ้าของที่มีขอบเขต".into()
                                }
                                (ReadModelAvailability::Partial, false) => {
                                    "Only lifecycle evidence confirmed within the bounded owner window is shown."
                                        .into()
                                }
                                (_, true) => {
                                    "อ่าน lifecycle จากบริการเจ้าของข้อมูลโดยตรง".into()
                                }
                                (_, false) => {
                                    "Lifecycle state was read directly from the owner service."
                                        .into()
                                }
                            },
                            reason_code: contribution_reason.map(str::to_owned),
                        });
                    }
                }
            }
        } else {
            reason_code = Some("no_authorized_process_contributors".into());
        }

        let offset = request
            .cursor
            .as_deref()
            .and_then(|value| value.strip_prefix("offset:"))
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or_default();
        let has_more = offset.saturating_add(request.page_size as usize) < items.len();
        let items = items
            .into_iter()
            .skip(offset)
            .take(request.page_size as usize)
            .collect::<Vec<_>>();
        let successful = contributions
            .iter()
            .any(|source| source.availability != ReadModelAvailability::Unavailable);
        let availability = if !successful {
            ReadModelAvailability::Unavailable
        } else if contributions
            .iter()
            .any(|source| source.availability == ReadModelAvailability::Partial)
        {
            ReadModelAvailability::Partial
        } else if items.is_empty() {
            ReadModelAvailability::Empty
        } else {
            ReadModelAvailability::Available
        };
        let freshness = if successful {
            ReadModelFreshness::Live
        } else {
            ReadModelFreshness::Unknown
        };
        let correlation_id = Uuid::new_v4();
        if let Some(context) = FullstackContext::current()
            && let Ok(value) = HeaderValue::from_str(&correlation_id.to_string())
        {
            context.add_response_header(HeaderName::from_static("x-erpx-correlation-id"), value);
        }
        let page = AuthorizedProcessPage {
            scope: authority.scope_key,
            measured_at: measured_at.to_rfc3339(),
            availability,
            freshness,
            reason_code,
            items,
            contributions,
            page_size: request.page_size,
            next_cursor: has_more
                .then(|| format!("offset:{}", offset + request.page_size as usize)),
            redacted_fields: vec![
                "access_token".into(),
                "backup_ref".into(),
                "restore_target_ref".into(),
                "database_profile_key".into(),
                "owner_error_detail".into(),
            ],
            authorization_context_version: authority.authorization_context_version,
            correlation_id,
        };
        if page.is_bounded() {
            Ok(page)
        } else {
            Err(DeliveryError::Internal(error_meta(false, None)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_request_and_cursor_are_bounded() {
        let request = AuthorizedProcessRequest {
            cursor: Some("offset:47".into()),
            page_size: 24,
            locale: "th".into(),
            consistency: ProcessConsistency::Eventual,
        };
        assert!(valid_request(&request));
        assert!(!valid_process_cursor("offset:100"));
        assert!(!valid_process_cursor("https://example.test/token"));
        let mut invalid = request;
        invalid.page_size = MAX_PROCESS_PAGE_SIZE + 1;
        assert!(!valid_request(&invalid));
    }

    #[cfg(feature = "server")]
    #[test]
    fn authoritative_unavailable_page_retains_safe_owner_reason() {
        let page = AuthorizedProcessPage {
            scope: "platform".into(),
            measured_at: chrono::Utc::now().to_rfc3339(),
            availability: ReadModelAvailability::Unavailable,
            freshness: ReadModelFreshness::Unknown,
            reason_code: Some("owner_unavailable".into()),
            items: Vec::new(),
            contributions: vec![server::unavailable_contribution("owner_unavailable", true)],
            page_size: 24,
            next_cursor: None,
            redacted_fields: vec!["access_token".into(), "owner_error_detail".into()],
            authorization_context_version: Some(7),
            correlation_id: Uuid::new_v4(),
        };
        assert!(page.is_bounded());
        assert_eq!(
            page.contributions[0].reason_code.as_deref(),
            Some("owner_unavailable")
        );
        assert!(!page.contributions[0].message.contains("secret"));
    }

    #[cfg(feature = "server")]
    #[test]
    fn owner_reasons_never_echo_status_payloads() {
        let status = tonic::Status::internal("backup_ref=secret-value");
        assert_eq!(server::owner_reason(&status), "owner_response_invalid");
        assert!(!server::owner_reason(&status).contains("secret"));
    }
}
