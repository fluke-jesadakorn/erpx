//! Typed, same-origin delivery boundary for the authorized work shell.
//!
//! The browser supplies filters only. Principal, active authority scope,
//! credentials, navigation capabilities, and authorization-context version are
//! resolved from the BFF session on the server.

use dioxus::prelude::dioxus_fullstack;
#[cfg(feature = "server")]
use dioxus::prelude::dioxus_server;
use erpx_delivery_contract::{DeliveryError, DeliveryErrorMeta};
use erpx_ui_core::{
    MAX_WORK_PAGE_SIZE, ReadModelFreshness, WorkDomain, WorkItemSeverity, WorkQueueReadModel,
    WorkQueueView, valid_work_cursor,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkConsistency {
    Eventual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizedWorkRequest {
    pub view: WorkQueueView,
    pub domain: Option<WorkDomain>,
    pub severity: Option<WorkItemSeverity>,
    pub cursor: Option<String>,
    pub page_size: u32,
    pub locale: String,
    pub consistency: WorkConsistency,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkProjectionCursor {
    pub source: String,
    pub projection: String,
    pub partition: String,
    pub event_id: String,
    pub global_sequence: u64,
    pub measured_at: String,
    pub freshness: ReadModelFreshness,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizedWorkPage {
    pub queue: WorkQueueReadModel,
    pub projection_cursors: Vec<WorkProjectionCursor>,
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

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn valid_request(request: &AuthorizedWorkRequest) -> bool {
    matches!(request.locale.as_str(), "en" | "th")
        && request.page_size > 0
        && request.page_size <= MAX_WORK_PAGE_SIZE
        && request.cursor.as_deref().is_none_or(valid_work_cursor)
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/work/list")]
pub async fn list_authorized_work(
    request: AuthorizedWorkRequest,
) -> dioxus::prelude::ServerFnResult<Result<AuthorizedWorkPage, DeliveryError>> {
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
pub(crate) mod server {
    use chrono::{DateTime, Utc};
    use dioxus::fullstack::FullstackContext;
    use dioxus::server::axum::http::{HeaderName, HeaderValue, header};
    use erpx_delivery_contract::{QueryRequest, RouteQuery, Workspace};
    use erpx_proto::{
        common::{
            ActorAttribution, ActorKind, AuditContext, AuthorityScope, EventualConsistency,
            HoldingRoute, ProjectionFreshnessState, QueryConsistency, QueryContext, QueryMetadata,
            QueryRequestMetadata, query_consistency,
        },
        erpx::{operations, platform_administration as platform, support},
    };
    use erpx_ui_core::{
        NavGroup, NavItem, ReadModelAvailability, WorkItemKind, WorkItemReadModel, WorkItemStage,
        WorkItemTarget, WorkQueueMeta, WorkRoute, WorkSlaState, WorkSourceContribution, sla_state,
    };
    use tonic::{Request, metadata::MetadataValue, transport::Channel};

    use super::*;
    use crate::bff::{BffCredential, BffSessionContext, BffState};

    const OWNER_PAGE_SIZE: u32 = MAX_WORK_PAGE_SIZE;

    #[derive(Clone)]
    struct WorkAuthority {
        state: BffState,
        credential: BffCredential,
        scope_key: String,
        scope: AuthorityScope,
        actor_kind: ActorKind,
        authorization_context_version: Option<u64>,
    }

    struct SourceResult {
        items: Vec<WorkItemReadModel>,
        contribution: WorkSourceContribution,
        projection: Option<WorkProjectionCursor>,
    }

    fn delivery_status(status: tonic::Status) -> DeliveryError {
        let retryable = matches!(
            status.code(),
            tonic::Code::Unavailable | tonic::Code::DeadlineExceeded | tonic::Code::Cancelled
        );
        let meta = error_meta(retryable, retryable.then_some(500));
        match status.code() {
            tonic::Code::Unauthenticated => DeliveryError::Unauthenticated(meta),
            tonic::Code::PermissionDenied => DeliveryError::Forbidden(meta),
            tonic::Code::InvalidArgument | tonic::Code::OutOfRange => {
                DeliveryError::Validation(meta)
            }
            tonic::Code::ResourceExhausted => DeliveryError::RateLimited(meta),
            tonic::Code::Unavailable
            | tonic::Code::DeadlineExceeded
            | tonic::Code::Cancelled
            | tonic::Code::FailedPrecondition => DeliveryError::Unavailable(meta),
            _ => DeliveryError::Internal(meta),
        }
    }

    pub(super) fn owner_status(status: tonic::Status) -> (DeliveryError, &'static str) {
        let reason = match status.message() {
            "holding route does not match holding scope" => "holding_route_scope_mismatch",
            "data_profile_key" | "holding route is required" | "holding route is invalid" => {
                "holding_route_invalid"
            }
            "query metadata is required" | "query metadata is invalid" => {
                "owner_query_metadata_invalid"
            }
            "actor is required"
            | "actor kind is invalid"
            | "session_id is invalid"
            | "active_scope is required"
            | "holding scope is empty" => "owner_authority_invalid",
            "audit context is required" | "audit correlation does not match request" => {
                "owner_audit_invalid"
            }
            "Operations actor does not match the session" => "owner_actor_mismatch",
            "Operations permission is denied" => "owner_permission_denied",
            "Operations holding route is denied" => "holding_route_denied",
            "Operations holding scope is denied" => "cross_holding_denied",
            "exception status is invalid"
            | "source context is invalid"
            | "page_size is invalid"
            | "assignee_id"
            | "due_before" => "owner_filter_invalid",
            _ => match status.code() {
                tonic::Code::Unauthenticated => "owner_unauthenticated",
                tonic::Code::PermissionDenied => "owner_forbidden",
                tonic::Code::InvalidArgument | tonic::Code::OutOfRange => "owner_request_invalid",
                tonic::Code::Unavailable
                | tonic::Code::DeadlineExceeded
                | tonic::Code::Cancelled
                | tonic::Code::FailedPrecondition => "owner_unavailable",
                _ => "owner_response_invalid",
            },
        };
        (delivery_status(status), reason)
    }

    fn authority() -> Result<WorkAuthority, DeliveryError> {
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
        Ok(WorkAuthority {
            state,
            credential,
            scope_key,
            scope,
            actor_kind,
            authorization_context_version,
        })
    }

    async fn navigation(authority: &WorkAuthority) -> Result<Vec<NavGroup>, DeliveryError> {
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
            .find(|item| {
                path == item.path
                    || path
                        .strip_prefix(item.path.as_str())
                        .is_some_and(|suffix| suffix.starts_with('/'))
            })
    }

    async fn channel(authority: &WorkAuthority) -> Result<Channel, DeliveryError> {
        tonic::transport::Endpoint::from_shared(authority.state.grpc_origin().to_owned())
            .map_err(|_| DeliveryError::Unavailable(error_meta(true, Some(500))))?
            .connect()
            .await
            .map_err(|_| DeliveryError::Unavailable(error_meta(true, Some(500))))
    }

    fn actor(authority: &WorkAuthority) -> ActorAttribution {
        ActorAttribution {
            principal_ref: authority.credential.principal_ref.clone(),
            kind: authority.actor_kind as i32,
            session_id: authority.credential.session_id.to_string(),
            represented_principal_ref: String::new(),
            support_capability_id: String::new(),
        }
    }

    fn query_metadata(
        authority: &WorkAuthority,
        module: &str,
        purpose: &str,
        correlation_id: Uuid,
        route: &HoldingRoute,
    ) -> QueryRequestMetadata {
        QueryRequestMetadata {
            contract_version: 1,
            query_id: Uuid::new_v4().to_string(),
            correlation_id: correlation_id.to_string(),
            actor: Some(actor(authority)),
            active_scope: Some(authority.scope.clone()),
            audit: Some(AuditContext {
                purpose: purpose.into(),
                customer_visible: true,
                module: module.into(),
                correlation_id: correlation_id.to_string(),
            }),
            consistency: Some(QueryConsistency {
                requirement: Some(query_consistency::Requirement::Eventual(
                    EventualConsistency {},
                )),
            }),
            holding_route: Some(route.clone()),
        }
    }

    fn query_context(
        authority: &WorkAuthority,
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
                module: "authorized-work".into(),
                correlation_id: correlation_id.to_string(),
            }),
        }
    }

    async fn active_holding_route(
        channel: Channel,
        authority: &WorkAuthority,
    ) -> Result<HoldingRoute, DeliveryError> {
        let correlation_id = Uuid::new_v4();
        let request = platform::ResolveActiveHoldingRouteRequest {
            context: Some(query_context(
                authority,
                "resolve server-owned work contributor route",
                correlation_id,
            )),
        };
        let response = platform::platform_administration_query_service_client::PlatformAdministrationQueryServiceClient::new(channel)
            .resolve_active_holding_route(authenticated(authority, request)?)
            .await
            .map_err(delivery_status)?
            .into_inner();
        let route = response
            .route
            .ok_or_else(|| DeliveryError::Unavailable(error_meta(true, Some(500))))?;
        if route.holding_id.trim().is_empty() || route.data_profile_key.trim().is_empty() {
            return Err(DeliveryError::Unavailable(error_meta(true, Some(500))));
        }
        Ok(HoldingRoute {
            holding_id: route.holding_id,
            data_profile_key: route.data_profile_key,
        })
    }

    fn authenticated<T>(
        authority: &WorkAuthority,
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

    fn projection(
        source: &str,
        metadata: Option<QueryMetadata>,
        expected_correlation: Uuid,
    ) -> Result<(ReadModelFreshness, Option<WorkProjectionCursor>), (DeliveryError, &'static str)>
    {
        let metadata = metadata.ok_or_else(|| {
            (
                DeliveryError::Internal(error_meta(false, None)),
                "projection_metadata_missing",
            )
        })?;
        if metadata.correlation_id != expected_correlation.to_string() {
            return Err((
                DeliveryError::Internal(error_meta(false, None)),
                "projection_correlation_mismatch",
            ));
        }
        let projection = metadata.projection.ok_or_else(|| {
            (
                DeliveryError::Unavailable(error_meta(true, Some(500))),
                "projection_metadata_missing",
            )
        })?;
        let freshness = match ProjectionFreshnessState::try_from(projection.freshness).ok() {
            Some(ProjectionFreshnessState::Current) => ReadModelFreshness::Current,
            Some(ProjectionFreshnessState::Stale | ProjectionFreshnessState::Rebuilding) => {
                ReadModelFreshness::Stale
            }
            _ => {
                return Err((
                    DeliveryError::Unavailable(error_meta(true, Some(500))),
                    "projection_unavailable",
                ));
            }
        };
        let Some(checkpoint) = projection.checkpoint else {
            return Ok((freshness, None));
        };
        if checkpoint.projection.trim().is_empty()
            || checkpoint.partition.trim().is_empty()
            || checkpoint.global_sequence == 0
            || Uuid::parse_str(&checkpoint.event_id).is_err()
            || DateTime::parse_from_rfc3339(&checkpoint.updated_at).is_err()
        {
            return Err((
                DeliveryError::Internal(error_meta(false, None)),
                "projection_checkpoint_invalid",
            ));
        }
        Ok((
            freshness,
            Some(WorkProjectionCursor {
                source: source.into(),
                projection: checkpoint.projection,
                partition: checkpoint.partition,
                event_id: checkpoint.event_id,
                global_sequence: checkpoint.global_sequence,
                measured_at: checkpoint.updated_at,
                freshness,
            }),
        ))
    }

    fn safe_text(value: &str, maximum_bytes: usize, fallback: &str) -> String {
        let mut result = String::new();
        for character in value.trim().chars().filter(|value| !value.is_control()) {
            if result.len() + character.len_utf8() > maximum_bytes {
                break;
            }
            result.push(character);
        }
        if result.is_empty() {
            fallback.into()
        } else {
            result
        }
    }

    fn severity(value: &str) -> WorkItemSeverity {
        match value.trim().to_ascii_lowercase().as_str() {
            "critical" | "sev0" | "sev1" => WorkItemSeverity::Critical,
            "high" | "sev2" => WorkItemSeverity::High,
            "medium" | "normal" | "sev3" => WorkItemSeverity::Medium,
            "low" | "sev4" => WorkItemSeverity::Low,
            _ => WorkItemSeverity::Informational,
        }
    }

    fn capabilities(item: Option<&NavItem>) -> Vec<String> {
        item.map(|item| item.action_capabilities.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|value| {
                !value.to_ascii_lowercase().contains("token")
                    && !value.to_ascii_lowercase().contains("secret")
            })
            .take(erpx_ui_core::MAX_WORK_ACTION_CAPABILITIES)
            .collect()
    }

    fn contribution(
        domain: WorkDomain,
        label: &str,
        item_count: usize,
        freshness: ReadModelFreshness,
        upstream_has_more: bool,
        thai: bool,
    ) -> WorkSourceContribution {
        let availability = if upstream_has_more {
            ReadModelAvailability::Partial
        } else if item_count == 0 {
            ReadModelAvailability::Empty
        } else {
            ReadModelAvailability::Available
        };
        let message = match (thai, upstream_has_more, item_count == 0) {
            (true, true, _) => {
                "แสดงเฉพาะหน้าข้อมูลที่เชื่อถือได้ซึ่งมีขอบเขตจำกัด เปิดหน้าของเจ้าของข้อมูลเพื่อดูรายการถัดไป"
            }
            (false, true, _) => {
                "Only the bounded authoritative window is shown; open the owner surface for later records."
            }
            (true, false, true) => "บริการเจ้าของข้อมูลยืนยันว่าไม่มีงานที่กำลังดำเนินการในหน้าข้อมูลนี้",
            (false, false, true) => {
                "The bounded-context owner returned an authoritative empty page."
            }
            (true, false, false) => "รวมรายการงานที่ได้รับอนุญาตจากบริการเจ้าของข้อมูลแล้ว",
            (false, false, false) => "Authorized work from the bounded-context owner is included.",
        };
        WorkSourceContribution {
            domain,
            label: label.into(),
            availability,
            freshness,
            item_count: item_count as u32,
            message: message.into(),
            reason_code: None,
        }
    }

    fn unavailable_contribution(
        domain: WorkDomain,
        label: &str,
        reason_code: &str,
        thai: bool,
    ) -> WorkSourceContribution {
        let message = match (thai, reason_code) {
            (true, "holding_route_unavailable") => {
                "ไม่สามารถผูกขอบเขตปัจจุบันกับ holding route ที่เซิร์ฟเวอร์รับรองได้"
            }
            (false, "holding_route_unavailable") => {
                "The active scope could not be bound to a server-authorized holding route."
            }
            (true, "projection_evidence_unavailable") => {
                "บริการเจ้าของข้อมูลไม่ส่งหลักฐาน projection ที่ตรวจสอบได้"
            }
            (false, "projection_evidence_unavailable") => {
                "The bounded-context owner did not return verifiable projection evidence."
            }
            (true, "not_connected") => "Contributor นี้ยังไม่ได้เชื่อมต่อกับ Authorized Work Shell",
            (false, "not_connected") => {
                "This contributor is not connected to the Authorized Work Shell yet."
            }
            (true, _) => "ไม่สามารถอ่าน projection ของบริการเจ้าของข้อมูลได้ในขณะนี้",
            (false, _) => "The bounded-context owner projection could not be reached.",
        };
        WorkSourceContribution {
            domain,
            label: label.into(),
            availability: ReadModelAvailability::Unavailable,
            freshness: ReadModelFreshness::Unknown,
            item_count: 0,
            message: message.into(),
            reason_code: Some(reason_code.into()),
        }
    }

    async fn workspace_contribution(
        workspace: Workspace,
        domain: WorkDomain,
        label: &str,
        thai: bool,
    ) -> SourceResult {
        let request = QueryRequest {
            route: RouteQuery::Workspace {
                workspace,
                resource: None,
            },
            cursor: None,
            limit: 1,
            locale: if thai { "th" } else { "en" }.into(),
        };
        match crate::workspace_delivery::query_route(&request).await {
            Ok(data) if matches!(data.freshness_tag.as_str(), "fresh" | "stale") => {
                let freshness = if data.freshness_tag == "stale" {
                    ReadModelFreshness::Stale
                } else {
                    ReadModelFreshness::Current
                };
                SourceResult {
                    items: Vec::new(),
                    contribution: contribution(
                        domain,
                        label,
                        0,
                        freshness,
                        !data.page.items.is_empty() || data.page.next_cursor.is_some(),
                        thai,
                    ),
                    projection: None,
                }
            }
            _ => SourceResult {
                items: Vec::new(),
                contribution: unavailable_contribution(
                    domain,
                    label,
                    "workspace_projection_unavailable",
                    thai,
                ),
                projection: None,
            },
        }
    }

    pub(super) fn queue_health(
        contributions: &[WorkSourceContribution],
        total: u32,
    ) -> (ReadModelAvailability, ReadModelFreshness) {
        if contributions.is_empty() {
            return (ReadModelAvailability::Empty, ReadModelFreshness::Current);
        }
        let successful_sources = contributions
            .iter()
            .filter(|source| source.availability != ReadModelAvailability::Unavailable)
            .count();
        let availability = if successful_sources == 0 {
            ReadModelAvailability::Unavailable
        } else if contributions.iter().any(|source| {
            matches!(
                source.availability,
                ReadModelAvailability::Partial | ReadModelAvailability::Unavailable
            )
        }) {
            ReadModelAvailability::Partial
        } else if total == 0 {
            ReadModelAvailability::Empty
        } else {
            ReadModelAvailability::Available
        };
        let freshness = if successful_sources == 0 {
            ReadModelFreshness::Unknown
        } else if contributions
            .iter()
            .any(|source| source.freshness == ReadModelFreshness::Stale)
        {
            ReadModelFreshness::Stale
        } else {
            ReadModelFreshness::Current
        };
        (availability, freshness)
    }

    async fn support_incidents(
        channel: Channel,
        authority: &WorkAuthority,
        route: Option<&NavItem>,
        holding_route: Option<&HoldingRoute>,
        thai: bool,
    ) -> SourceResult {
        let label = if thai {
            "เหตุขัดข้องของแพลตฟอร์ม"
        } else {
            "Platform incidents"
        };
        let Some(route) = route else {
            return SourceResult {
                items: Vec::new(),
                contribution: unavailable_contribution(
                    WorkDomain::Support,
                    label,
                    "owner_unavailable",
                    thai,
                ),
                projection: None,
            };
        };
        let Some(holding_route) = holding_route else {
            return SourceResult {
                items: Vec::new(),
                contribution: unavailable_contribution(
                    WorkDomain::Support,
                    label,
                    "holding_route_unavailable",
                    thai,
                ),
                projection: None,
            };
        };
        let correlation = Uuid::new_v4();
        let request = support::ListSupportIncidentRecordsRequest {
            metadata: Some(query_metadata(
                authority,
                "support",
                "list authorized platform incident work",
                correlation,
                holding_route,
            )),
            status: None,
            page_size: OWNER_PAGE_SIZE,
            cursor: None,
        };
        let response = async {
            support::support_query_service_client::SupportQueryServiceClient::new(channel)
                .list_incidents(
                    authenticated(authority, request)
                        .map_err(|error| (error, "owner_unauthenticated"))?,
                )
                .await
                .map_err(owner_status)
                .map(tonic::Response::into_inner)
        }
        .await;
        let response = match response {
            Ok(response) => response,
            Err((_error, reason)) => {
                return SourceResult {
                    items: Vec::new(),
                    contribution: unavailable_contribution(
                        WorkDomain::Support,
                        label,
                        reason,
                        thai,
                    ),
                    projection: None,
                };
            }
        };
        let (freshness, projection) =
            match projection("support.incidents", response.metadata, correlation) {
                Ok(value) => value,
                Err((_error, reason)) => {
                    return SourceResult {
                        items: Vec::new(),
                        contribution: unavailable_contribution(
                            WorkDomain::Support,
                            label,
                            reason,
                            thai,
                        ),
                        projection: None,
                    };
                }
            };
        let route_capabilities = capabilities(Some(route));
        let items = response
            .incidents
            .into_iter()
            .filter_map(|value| {
                let status = support::SupportIncidentStatus::try_from(value.status).ok()?;
                let (stage, status_label) = match status {
                    support::SupportIncidentStatus::Investigating => (
                        WorkItemStage::InProgress,
                        if thai {
                            "กำลังตรวจสอบ"
                        } else {
                            "Investigating"
                        },
                    ),
                    support::SupportIncidentStatus::Identified => (
                        WorkItemStage::InProgress,
                        if thai {
                            "ระบุสาเหตุแล้ว"
                        } else {
                            "Identified"
                        },
                    ),
                    support::SupportIncidentStatus::Monitoring => (
                        WorkItemStage::Waiting,
                        if thai {
                            "กำลังเฝ้าระวัง"
                        } else {
                            "Monitoring"
                        },
                    ),
                    _ => return None,
                };
                let id = Uuid::parse_str(&value.incident_id).ok()?.to_string();
                if value.source_version == 0
                    || DateTime::parse_from_rfc3339(&value.updated_at).is_err()
                {
                    return None;
                }
                let summary = if value.affected_services.is_empty() {
                    if thai {
                        "เหตุขัดข้องที่กำลังดำเนินการ"
                    } else {
                        "Active service incident"
                    }
                    .into()
                } else {
                    safe_text(
                        &value.affected_services.join(", "),
                        512,
                        "Active service incident",
                    )
                };
                Some(WorkItemReadModel {
                    key: format!("support-incident:{id}"),
                    record_version: value.source_version,
                    domain: WorkDomain::Support,
                    kind: WorkItemKind::Incident,
                    title: safe_text(&value.incident_code, 512, "Platform incident"),
                    summary,
                    stage,
                    status_label: status_label.into(),
                    severity: severity(&value.severity),
                    assignee: (!value.commander_ref.trim().is_empty())
                        .then(|| safe_text(&value.commander_ref, 128, "assigned")),
                    due_at: None,
                    updated_at: Some(value.updated_at),
                    sla: WorkSlaState::NoDueDate,
                    freshness,
                    target: WorkItemTarget {
                        route: WorkRoute::PlatformIncidents,
                        record_ref: Some(id),
                    },
                    action_capabilities: if freshness == ReadModelFreshness::Current {
                        route_capabilities.clone()
                    } else {
                        Vec::new()
                    },
                })
            })
            .collect::<Vec<_>>();
        SourceResult {
            contribution: contribution(
                WorkDomain::Support,
                label,
                items.len(),
                freshness,
                response.next_cursor.is_some(),
                thai,
            ),
            items,
            projection,
        }
    }

    async fn support_tickets(
        channel: Channel,
        authority: &WorkAuthority,
        route: Option<&NavItem>,
        holding_route: Option<&HoldingRoute>,
        thai: bool,
    ) -> SourceResult {
        let label = if thai {
            "คำขอฝ่ายสนับสนุน"
        } else {
            "Support tickets"
        };
        let Some(route) = route else {
            return SourceResult {
                items: Vec::new(),
                contribution: unavailable_contribution(
                    WorkDomain::Support,
                    label,
                    "owner_unavailable",
                    thai,
                ),
                projection: None,
            };
        };
        let Some(holding_route) = holding_route else {
            return SourceResult {
                items: Vec::new(),
                contribution: unavailable_contribution(
                    WorkDomain::Support,
                    label,
                    "holding_route_unavailable",
                    thai,
                ),
                projection: None,
            };
        };
        let correlation = Uuid::new_v4();
        let request = support::ListSupportInternalTicketRecordsRequest {
            metadata: Some(query_metadata(
                authority,
                "support",
                "list authorized support ticket work",
                correlation,
                holding_route,
            )),
            status: None,
            customer_id: None,
            page_size: OWNER_PAGE_SIZE,
            cursor: None,
        };
        let response = async {
            support::support_query_service_client::SupportQueryServiceClient::new(channel)
                .list_internal_tickets(
                    authenticated(authority, request)
                        .map_err(|error| (error, "owner_unauthenticated"))?,
                )
                .await
                .map_err(owner_status)
                .map(tonic::Response::into_inner)
        }
        .await;
        let response = match response {
            Ok(response) => response,
            Err((_error, reason)) => {
                return SourceResult {
                    items: Vec::new(),
                    contribution: unavailable_contribution(
                        WorkDomain::Support,
                        label,
                        reason,
                        thai,
                    ),
                    projection: None,
                };
            }
        };
        let (freshness, projection) =
            match projection("support.tickets", response.metadata, correlation) {
                Ok(value) => value,
                Err((_error, reason)) => {
                    return SourceResult {
                        items: Vec::new(),
                        contribution: unavailable_contribution(
                            WorkDomain::Support,
                            label,
                            reason,
                            thai,
                        ),
                        projection: None,
                    };
                }
            };
        let route_capabilities = capabilities(Some(route));
        let items = response
            .tickets
            .into_iter()
            .filter_map(|value| {
                let status = support::SupportTicketStatus::try_from(value.status).ok()?;
                let (stage, status_label, kind) = match status {
                    support::SupportTicketStatus::New => (
                        WorkItemStage::Open,
                        if thai { "ใหม่" } else { "New" },
                        WorkItemKind::Task,
                    ),
                    support::SupportTicketStatus::Triaged => (
                        WorkItemStage::Open,
                        if thai {
                            "คัดกรองแล้ว"
                        } else {
                            "Triaged"
                        },
                        WorkItemKind::Task,
                    ),
                    support::SupportTicketStatus::WaitingForCustomer => (
                        WorkItemStage::Waiting,
                        if thai {
                            "รอลูกค้า"
                        } else {
                            "Waiting for customer"
                        },
                        WorkItemKind::Task,
                    ),
                    support::SupportTicketStatus::AccessRequested => (
                        WorkItemStage::Waiting,
                        if thai {
                            "ขอสิทธิ์เข้าถึง"
                        } else {
                            "Access requested"
                        },
                        WorkItemKind::Approval,
                    ),
                    support::SupportTicketStatus::Approved => (
                        WorkItemStage::Open,
                        if thai {
                            "อนุมัติแล้ว"
                        } else {
                            "Approved"
                        },
                        WorkItemKind::Task,
                    ),
                    support::SupportTicketStatus::InProgress => (
                        WorkItemStage::InProgress,
                        if thai {
                            "กำลังดำเนินการ"
                        } else {
                            "In progress"
                        },
                        WorkItemKind::Task,
                    ),
                    _ => return None,
                };
                let id = Uuid::parse_str(&value.ticket_id).ok()?.to_string();
                if value.source_version == 0
                    || DateTime::parse_from_rfc3339(&value.updated_at).is_err()
                {
                    return None;
                }
                Some(WorkItemReadModel {
                    key: format!("support-ticket:{id}"),
                    record_version: value.source_version,
                    domain: WorkDomain::Support,
                    kind,
                    title: safe_text(&value.case_code, 512, "Support ticket"),
                    summary: safe_text(&value.category, 512, "Support request"),
                    stage,
                    status_label: status_label.into(),
                    severity: severity(&value.severity),
                    assignee: None,
                    due_at: None,
                    updated_at: Some(value.updated_at),
                    sla: WorkSlaState::NoDueDate,
                    freshness,
                    target: WorkItemTarget {
                        route: WorkRoute::PlatformSupport,
                        record_ref: Some(id),
                    },
                    action_capabilities: if freshness == ReadModelFreshness::Current {
                        route_capabilities.clone()
                    } else {
                        Vec::new()
                    },
                })
            })
            .collect::<Vec<_>>();
        SourceResult {
            contribution: contribution(
                WorkDomain::Support,
                label,
                items.len(),
                freshness,
                response.next_cursor.is_some(),
                thai,
            ),
            items,
            projection,
        }
    }

    async fn operations_exceptions(
        channel: Channel,
        authority: &WorkAuthority,
        route: Option<&NavItem>,
        holding_route: Option<&HoldingRoute>,
        thai: bool,
        now: DateTime<Utc>,
    ) -> SourceResult {
        let label = if thai {
            "ข้อยกเว้นฝ่ายปฏิบัติการ"
        } else {
            "Operations exceptions"
        };
        let Some(route) = route else {
            return SourceResult {
                items: Vec::new(),
                contribution: unavailable_contribution(
                    WorkDomain::Operations,
                    label,
                    "owner_unavailable",
                    thai,
                ),
                projection: None,
            };
        };
        let Some(holding_route) = holding_route else {
            return SourceResult {
                items: Vec::new(),
                contribution: unavailable_contribution(
                    WorkDomain::Operations,
                    label,
                    "holding_route_unavailable",
                    thai,
                ),
                projection: None,
            };
        };
        let correlation = Uuid::new_v4();
        let request = operations::ListOperationsExceptionQueueRequest {
            metadata: Some(query_metadata(
                authority,
                "operations",
                "list authorized operations exception work",
                correlation,
                holding_route,
            )),
            statuses: vec![
                operations::OperationsExceptionStatus::Assigned as i32,
                operations::OperationsExceptionStatus::Acknowledged as i32,
                operations::OperationsExceptionStatus::Escalated as i32,
            ],
            source_context: None,
            assignee_id: None,
            due_before: None,
            page_size: OWNER_PAGE_SIZE,
            cursor: None,
        };
        let response = async {
            operations::operations_query_service_client::OperationsQueryServiceClient::new(channel)
                .list_exception_queue(
                    authenticated(authority, request)
                        .map_err(|error| (error, "owner_unauthenticated"))?,
                )
                .await
                .map_err(owner_status)
                .map(tonic::Response::into_inner)
        }
        .await;
        let response = match response {
            Ok(response) => response,
            Err((_error, reason)) => {
                return SourceResult {
                    items: Vec::new(),
                    contribution: unavailable_contribution(
                        WorkDomain::Operations,
                        label,
                        reason,
                        thai,
                    ),
                    projection: None,
                };
            }
        };
        let (freshness, projection) =
            match projection("operations.exceptions", response.metadata, correlation) {
                Ok(value) => value,
                Err((_error, reason)) => {
                    return SourceResult {
                        items: Vec::new(),
                        contribution: unavailable_contribution(
                            WorkDomain::Operations,
                            label,
                            reason,
                            thai,
                        ),
                        projection: None,
                    };
                }
            };
        let route_capabilities = capabilities(Some(route));
        let items = response
            .exceptions
            .into_iter()
            .filter_map(|value| {
                let status = operations::OperationsExceptionStatus::try_from(value.status).ok()?;
                let (stage, status_label) = match status {
                    operations::OperationsExceptionStatus::Assigned => (
                        WorkItemStage::Open,
                        if thai {
                            "มอบหมายแล้ว"
                        } else {
                            "Assigned"
                        },
                    ),
                    operations::OperationsExceptionStatus::Acknowledged => (
                        WorkItemStage::InProgress,
                        if thai {
                            "รับทราบแล้ว"
                        } else {
                            "Acknowledged"
                        },
                    ),
                    operations::OperationsExceptionStatus::Escalated => (
                        WorkItemStage::InProgress,
                        if thai {
                            "ยกระดับแล้ว"
                        } else {
                            "Escalated"
                        },
                    ),
                    _ => return None,
                };
                let id = Uuid::parse_str(&value.exception_id).ok()?.to_string();
                if value.aggregate_version == 0
                    || DateTime::parse_from_rfc3339(&value.updated_at).is_err()
                {
                    return None;
                }
                let due_at = (!value.due_at.trim().is_empty())
                    .then_some(value.due_at)
                    .filter(|value| DateTime::parse_from_rfc3339(value).is_ok());
                let kind = if value.category.to_ascii_lowercase().contains("approval") {
                    WorkItemKind::Approval
                } else {
                    WorkItemKind::Exception
                };
                Some(WorkItemReadModel {
                    key: format!("operations-exception:{id}"),
                    record_version: value.aggregate_version,
                    domain: WorkDomain::Operations,
                    kind,
                    title: safe_text(&value.reference, 512, "Operations exception"),
                    summary: safe_text(&value.category, 512, "Operations exception"),
                    stage,
                    status_label: status_label.into(),
                    severity: severity(&value.severity),
                    assignee: (!value.assignee_id.trim().is_empty())
                        .then(|| safe_text(&value.assignee_id, 128, "assigned")),
                    sla: sla_state(due_at.as_deref(), now),
                    due_at,
                    updated_at: Some(value.updated_at),
                    freshness,
                    target: WorkItemTarget {
                        route: WorkRoute::OperationsExceptions,
                        record_ref: Some(id),
                    },
                    action_capabilities: if freshness == ReadModelFreshness::Current {
                        route_capabilities.clone()
                    } else {
                        Vec::new()
                    },
                })
            })
            .collect::<Vec<_>>();
        SourceResult {
            contribution: contribution(
                WorkDomain::Operations,
                label,
                items.len(),
                freshness,
                response.next_cursor.is_some(),
                thai,
            ),
            items,
            projection,
        }
    }

    fn work_rank(item: &WorkItemReadModel) -> (u8, u8, std::cmp::Reverse<Option<String>>, String) {
        let sla = match item.sla {
            WorkSlaState::Overdue => 0,
            WorkSlaState::DueToday => 1,
            WorkSlaState::Healthy => 2,
            WorkSlaState::NoDueDate => 3,
        };
        (
            sla,
            item.severity.rank(),
            std::cmp::Reverse(item.updated_at.clone()),
            item.key.clone(),
        )
    }

    pub(crate) async fn list(
        request: AuthorizedWorkRequest,
    ) -> Result<AuthorizedWorkPage, DeliveryError> {
        let authority = authority()?;
        let groups = navigation(&authority).await?;
        let thai = request.locale == "th";
        let now = Utc::now();
        let incident_route = route_item(&groups, WorkRoute::PlatformIncidents.path());
        let ticket_route = route_item(&groups, WorkRoute::PlatformSupport.path());
        let operations_route = route_item(&groups, WorkRoute::OperationsExceptions.path());
        let supported_source_authorized =
            incident_route.is_some() || ticket_route.is_some() || operations_route.is_some();

        let mut results = Vec::new();
        if supported_source_authorized {
            let transport = channel(&authority).await?;
            let holding_route = active_holding_route(transport.clone(), &authority)
                .await
                .ok();
            let (incidents, tickets, exceptions) = tokio::join!(
                support_incidents(
                    transport.clone(),
                    &authority,
                    incident_route,
                    holding_route.as_ref(),
                    thai
                ),
                support_tickets(
                    transport.clone(),
                    &authority,
                    ticket_route,
                    holding_route.as_ref(),
                    thai
                ),
                operations_exceptions(
                    transport,
                    &authority,
                    operations_route,
                    holding_route.as_ref(),
                    thai,
                    now
                ),
            );
            if incident_route.is_some() {
                results.push(incidents);
            }
            if ticket_route.is_some() {
                results.push(tickets);
            }
            if operations_route.is_some() {
                results.push(exceptions);
            }
        }

        for (path, workspace, domain, english, thai_label) in [
            (
                "/finance",
                Workspace::Finance,
                WorkDomain::Finance,
                "Finance close reviews",
                "การตรวจสอบปิดบัญชี",
            ),
            (
                "/people",
                Workspace::People,
                WorkDomain::People,
                "People approvals",
                "การอนุมัติด้านบุคลากร",
            ),
            (
                "/maintenance",
                Workspace::Maintenance,
                WorkDomain::Maintenance,
                "Maintenance work orders",
                "ใบงานบำรุงรักษา",
            ),
            (
                "/procurement",
                Workspace::Procurement,
                WorkDomain::Procurement,
                "Procurement approvals",
                "การอนุมัติจัดซื้อ",
            ),
        ] {
            if route_item(&groups, path).is_some() {
                results.push(
                    workspace_contribution(
                        workspace,
                        domain,
                        if thai { thai_label } else { english },
                        thai,
                    )
                    .await,
                );
            }
        }

        let mut all_items = results
            .iter_mut()
            .flat_map(|source| std::mem::take(&mut source.items))
            .collect::<Vec<_>>();
        all_items.sort_by_key(work_rank);
        let counters = WorkQueueReadModel::counters_for(&all_items, now);
        let filtered = all_items
            .into_iter()
            .filter(|item| request.domain.is_none_or(|domain| item.domain == domain))
            .filter(|item| {
                request
                    .severity
                    .is_none_or(|severity| item.severity == severity)
            })
            .filter(|item| match request.view {
                WorkQueueView::All => true,
                WorkQueueView::Mine => {
                    item.assignee.as_deref() == Some(&authority.credential.principal_ref)
                }
                WorkQueueView::Team => {
                    item.assignee.as_deref() != Some(&authority.credential.principal_ref)
                }
                WorkQueueView::Overdue => item.sla == WorkSlaState::Overdue,
                WorkQueueView::Approvals => item.kind == WorkItemKind::Approval,
            })
            .collect::<Vec<_>>();
        let offset = request
            .cursor
            .as_deref()
            .and_then(|cursor| cursor.strip_prefix("offset:"))
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or_default();
        let has_more = offset.saturating_add(request.page_size as usize) < filtered.len();
        let items = filtered
            .into_iter()
            .skip(offset)
            .take(request.page_size as usize)
            .collect::<Vec<_>>();
        let contributions = results
            .iter()
            .map(|source| source.contribution.clone())
            .collect::<Vec<_>>();
        let (availability, freshness) = queue_health(&contributions, counters.total);
        let queue = WorkQueueReadModel {
            view: request.view,
            meta: WorkQueueMeta {
                scope: authority.scope_key.clone(),
                as_of: now.to_rfc3339(),
                freshness,
                availability,
            },
            counters,
            returned_count: items.len() as u32,
            items,
            contributions,
            page_size: request.page_size,
            next_cursor: has_more
                .then(|| format!("offset:{}", offset + request.page_size as usize)),
        };
        if !queue.is_bounded() {
            return Err(DeliveryError::Internal(error_meta(false, None)));
        }
        let projection_cursors = results
            .into_iter()
            .filter_map(|source| source.projection)
            .collect::<Vec<_>>();
        let correlation_id = Uuid::new_v4();
        if let Some(context) = FullstackContext::current()
            && let Ok(value) = HeaderValue::from_str(&correlation_id.to_string())
        {
            context.add_response_header(HeaderName::from_static("x-erpx-correlation-id"), value);
        }
        Ok(AuthorizedWorkPage {
            queue,
            projection_cursors,
            redacted_fields: vec![
                "access_token".into(),
                "customer_identity".into(),
                "ticket_description".into(),
                "incident_customer_accounts".into(),
                "owner_internal_payload".into(),
            ],
            authorization_context_version: authority.authorization_context_version,
            correlation_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> AuthorizedWorkRequest {
        AuthorizedWorkRequest {
            view: WorkQueueView::All,
            domain: None,
            severity: None,
            cursor: None,
            page_size: 24,
            locale: "en".into(),
            consistency: WorkConsistency::Eventual,
        }
    }

    #[test]
    fn work_request_accepts_only_bounded_typed_filters() {
        assert!(valid_request(&request()));
        let mut invalid = request();
        invalid.page_size = MAX_WORK_PAGE_SIZE + 1;
        assert!(!valid_request(&invalid));
        invalid = request();
        invalid.cursor = Some("https://example.test/leak".into());
        assert!(!valid_request(&invalid));
        invalid = request();
        invalid.locale = "th-TH".into();
        assert!(!valid_request(&invalid));
    }

    #[cfg(feature = "server")]
    #[test]
    fn owner_failures_expose_only_allowlisted_reason_codes() {
        let (_, route) = server::owner_status(tonic::Status::invalid_argument("data_profile_key"));
        assert_eq!(route, "holding_route_invalid");
        let (_, denied) = server::owner_status(tonic::Status::permission_denied(
            "Operations permission is denied",
        ));
        assert_eq!(denied, "owner_permission_denied");
        let (_, unknown) = server::owner_status(tonic::Status::invalid_argument(
            "untrusted owner payload: customer@example.test",
        ));
        assert_eq!(unknown, "owner_request_invalid");
        assert!(!unknown.contains('@'));
    }

    #[cfg(feature = "server")]
    #[test]
    fn roles_without_supported_work_sources_render_an_authoritative_empty_shell() {
        assert_eq!(
            server::queue_health(&[], 0),
            (
                erpx_ui_core::ReadModelAvailability::Empty,
                ReadModelFreshness::Current
            )
        );
    }
}
