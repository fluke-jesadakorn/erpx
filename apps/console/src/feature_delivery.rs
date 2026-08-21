use dioxus::prelude::dioxus_fullstack;
#[cfg(feature = "server")]
use dioxus::prelude::dioxus_server;
use erpx_console_feature_identity_access::{
    IdentityAccessDetail, IdentityAccessPage, IdentityPrincipalLifecycleAction, IdentityRecordKind,
};
#[cfg(feature = "server")]
use erpx_console_feature_identity_access::{
    IdentityField, IdentityProjectionEvidence, IdentityRecord,
};
#[cfg(feature = "server")]
use erpx_console_feature_platform_administration::FieldRedaction;
#[cfg(feature = "server")]
use erpx_console_feature_platform_administration::HoldingControlHistoryEntry;
use erpx_console_feature_platform_administration::{
    HoldingControlHistoryPage, HoldingDetail, HoldingPage, HoldingProjectionEvidence,
};
use erpx_delivery_contract::{DeliveryError, DeliveryErrorMeta};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg_attr(not(feature = "server"), allow(dead_code))]
const MAX_FEATURE_PAGE_SIZE: u16 = 100;
#[cfg_attr(not(feature = "server"), allow(dead_code))]
const MAX_HOLDING_HISTORY_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode")]
pub enum FeatureConsistency {
    Eventual,
    ReadYourWrites {
        projection: String,
        partition: String,
        event_id: Uuid,
        global_sequence: u64,
        timeout_millis: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureListRequest {
    pub cursor: Option<String>,
    pub page_size: u16,
    pub locale: String,
    pub consistency: FeatureConsistency,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureDetailRequest {
    pub id: Uuid,
    pub locale: String,
    pub consistency: FeatureConsistency,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldingHistoryRequest {
    pub holding_id: Uuid,
    pub cursor: Option<String>,
    pub page_size: u16,
    pub locale: String,
    pub consistency: FeatureConsistency,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityPrincipalCommandRequest {
    pub principal_id: Uuid,
    pub action: IdentityPrincipalLifecycleAction,
    pub idempotency_key: String,
    pub expected_version: u64,
    pub locale: String,
    pub purpose: String,
    pub mfa_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureCommandReceipt {
    pub command_id: Uuid,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub replayed: bool,
    pub projection: String,
    pub partition: String,
    pub projection_event_id: Uuid,
    pub projection_sequence: u64,
    pub invalidation_tags: Vec<String>,
    pub audit_reference: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityPrincipalCommandOutcome {
    pub receipt: FeatureCommandReceipt,
    /// Present only after the owner projection has reached the receipt cursor
    /// and the authoritative record has been refetched.
    pub detail: Option<IdentityAccessDetail>,
    pub projection_state: String,
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn error_meta(retryable: bool, retry_after_ms: Option<u64>) -> DeliveryErrorMeta {
    DeliveryErrorMeta {
        correlation_id: Uuid::new_v4(),
        retryable,
        retry_after_ms,
    }
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn validate_locale(locale: &str) -> bool {
    matches!(locale, "en" | "th")
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn validate_list_request(request: &FeatureListRequest) -> Result<(), DeliveryError> {
    if request.page_size == 0
        || request.page_size > MAX_FEATURE_PAGE_SIZE
        || !validate_locale(&request.locale)
        || request.cursor.as_ref().is_some_and(|cursor| {
            cursor.is_empty() || cursor.len() > 1_024 || cursor.chars().any(char::is_control)
        })
    {
        return Err(DeliveryError::Validation(error_meta(false, None)));
    }
    validate_consistency(&request.consistency)
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn validate_detail_request(request: &FeatureDetailRequest) -> Result<(), DeliveryError> {
    if request.id.is_nil() || !validate_locale(&request.locale) {
        return Err(DeliveryError::Validation(error_meta(false, None)));
    }
    validate_consistency(&request.consistency)
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn decode_holding_history_cursor(cursor: &str) -> Option<(i64, Uuid)> {
    let (millis, event_id) = cursor.split_once('.')?;
    if millis.is_empty()
        || event_id.is_empty()
        || cursor.len() > 96
        || cursor.chars().any(char::is_control)
    {
        return None;
    }
    let millis = millis.parse::<i64>().ok()?;
    let event_id = Uuid::parse_str(event_id).ok()?;
    (millis > 0 && !event_id.is_nil()).then_some((millis, event_id))
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn validate_holding_history_request(request: &HoldingHistoryRequest) -> Result<(), DeliveryError> {
    if request.holding_id.is_nil()
        || request.page_size == 0
        || request.page_size > MAX_HOLDING_HISTORY_PAGE_SIZE
        || !validate_locale(&request.locale)
        || request
            .cursor
            .as_deref()
            .is_some_and(|cursor| decode_holding_history_cursor(cursor).is_none())
    {
        return Err(DeliveryError::Validation(error_meta(false, None)));
    }
    validate_consistency(&request.consistency)
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn validate_consistency(consistency: &FeatureConsistency) -> Result<(), DeliveryError> {
    if let FeatureConsistency::ReadYourWrites {
        projection,
        partition,
        event_id,
        global_sequence,
        timeout_millis,
    } = consistency
        && (projection.trim().is_empty()
            || projection.len() > 256
            || partition.trim().is_empty()
            || partition.len() > 512
            || event_id.is_nil()
            || *global_sequence == 0
            || *timeout_millis == 0
            || *timeout_millis > 10_000)
    {
        return Err(DeliveryError::Validation(error_meta(false, None)));
    }
    Ok(())
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn validate_principal_command(
    request: &IdentityPrincipalCommandRequest,
) -> Result<(), DeliveryError> {
    if request.principal_id.is_nil()
        || request.expected_version == 0
        || !validate_locale(&request.locale)
        || request.idempotency_key.trim().is_empty()
        || request.idempotency_key.len() > 128
        || request.idempotency_key.chars().any(char::is_control)
        || request.purpose.trim().is_empty()
        || request.purpose.len() > 256
        || request.purpose.chars().any(char::is_control)
        || request.mfa_code.trim().is_empty()
        || request.mfa_code.len() > 512
        || request.mfa_code.chars().any(char::is_control)
    {
        return Err(DeliveryError::Validation(error_meta(false, None)));
    }
    Ok(())
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn identity_command_id(idempotency_key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx:identity-access:command:{idempotency_key}").as_bytes(),
    )
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/platform/holdings/list")]
pub async fn list_platform_holdings(
    request: FeatureListRequest,
) -> dioxus::prelude::ServerFnResult<Result<HoldingPage, DeliveryError>> {
    if let Err(error) = validate_list_request(&request) {
        return Ok(Err(error));
    }
    #[cfg(feature = "server")]
    {
        return Ok(server::list_platform_holdings(request).await);
    }
    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/platform/holdings/get")]
pub async fn get_platform_holding(
    request: FeatureDetailRequest,
) -> dioxus::prelude::ServerFnResult<
    Result<(HoldingDetail, HoldingProjectionEvidence), DeliveryError>,
> {
    if let Err(error) = validate_detail_request(&request) {
        return Ok(Err(error));
    }
    #[cfg(feature = "server")]
    {
        return Ok(server::get_platform_holding(request).await);
    }
    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/platform/holdings/history")]
pub async fn list_platform_holding_history(
    request: HoldingHistoryRequest,
) -> dioxus::prelude::ServerFnResult<Result<HoldingControlHistoryPage, DeliveryError>> {
    if let Err(error) = validate_holding_history_request(&request) {
        return Ok(Err(error));
    }
    #[cfg(feature = "server")]
    {
        return Ok(server::list_platform_holding_history(request).await);
    }
    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/identity/list")]
pub async fn list_identity_records(
    kind: IdentityRecordKind,
    request: FeatureListRequest,
) -> dioxus::prelude::ServerFnResult<Result<IdentityAccessPage, DeliveryError>> {
    if let Err(error) = validate_list_request(&request) {
        return Ok(Err(error));
    }
    #[cfg(feature = "server")]
    {
        return Ok(server::list_identity_records(kind, request).await);
    }
    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/identity/get")]
pub async fn get_identity_record(
    kind: IdentityRecordKind,
    request: FeatureDetailRequest,
) -> dioxus::prelude::ServerFnResult<Result<IdentityAccessDetail, DeliveryError>> {
    if let Err(error) = validate_detail_request(&request) {
        return Ok(Err(error));
    }
    #[cfg(feature = "server")]
    {
        return Ok(server::get_identity_record(kind, request).await);
    }
    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/identity/principal/command")]
pub async fn command_identity_principal(
    request: IdentityPrincipalCommandRequest,
) -> dioxus::prelude::ServerFnResult<Result<IdentityPrincipalCommandOutcome, DeliveryError>> {
    if let Err(error) = validate_principal_command(&request) {
        return Ok(Err(error));
    }
    #[cfg(feature = "server")]
    {
        crate::delivery::validate_mutation_request()?;
        let outcome = server::command_identity_principal(request).await;
        if let Ok(outcome) = &outcome {
            use dioxus::fullstack::FullstackContext;
            use dioxus::server::axum::http::{HeaderName, HeaderValue};

            if let Some(context) = FullstackContext::current() {
                context.add_response_header(
                    HeaderName::from_static("x-erpx-idempotency-replayed"),
                    if outcome.receipt.replayed {
                        HeaderValue::from_static("true")
                    } else {
                        HeaderValue::from_static("false")
                    },
                );
            }
        }
        return Ok(outcome);
    }
    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[cfg(feature = "server")]
pub(crate) mod server {
    use chrono::{DateTime, Utc};
    use dioxus::fullstack::FullstackContext;
    use dioxus::server::axum::http::{HeaderValue, header};
    use erpx_proto::{
        common::{
            ActorAttribution, ActorKind, AuditContext, CommandRequestMetadata, EventualConsistency,
            HoldingRoute, ProjectionFreshnessState, QueryConsistency, QueryContext,
            QueryRequestMetadata, ReadYourWrites, query_consistency,
        },
        erpx::{identity_access as identity, platform_administration as platform},
    };
    use tonic::{Request, metadata::MetadataValue, transport::Channel};

    use super::*;
    use crate::bff::{BffCredential, BffSessionContext, BffState};

    pub(crate) struct FeatureAuthority {
        state: BffState,
        credential: BffCredential,
        scope_key: String,
        scope: erpx_proto::common::AuthorityScope,
        actor_kind: ActorKind,
    }

    pub(crate) fn delivery_status(status: tonic::Status) -> DeliveryError {
        let retryable = matches!(
            status.code(),
            tonic::Code::Unavailable | tonic::Code::DeadlineExceeded | tonic::Code::Cancelled
        );
        let meta = error_meta(retryable, retryable.then_some(500));
        match status.code() {
            tonic::Code::Unauthenticated => DeliveryError::Unauthenticated(meta),
            tonic::Code::PermissionDenied => DeliveryError::Forbidden(meta),
            tonic::Code::NotFound => DeliveryError::NotFound(meta),
            tonic::Code::AlreadyExists | tonic::Code::Aborted => DeliveryError::Conflict(meta),
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

    pub(crate) fn feature_authority() -> Result<FeatureAuthority, DeliveryError> {
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
        let (scope, actor_kind) = crate::bff_navigation::authority_scope(&scope_key)
            .map_err(|_| DeliveryError::Forbidden(error_meta(false, None)))?;
        Ok(FeatureAuthority {
            state,
            credential,
            scope_key,
            scope,
            actor_kind,
        })
    }

    pub(crate) async fn channel(authority: &FeatureAuthority) -> Result<Channel, DeliveryError> {
        tonic::transport::Endpoint::from_shared(authority.state.grpc_origin().to_owned())
            .map_err(|_| DeliveryError::Unavailable(error_meta(true, Some(500))))?
            .connect()
            .await
            .map_err(|_| DeliveryError::Unavailable(error_meta(true, Some(500))))
    }

    pub(crate) fn reference_scope(
        authority: &FeatureAuthority,
    ) -> Result<erpx_proto::erpx::interface::AuthorityScope, DeliveryError> {
        use erpx_proto::common::authority_scope::Scope;
        use erpx_proto::erpx::interface::{AuthorityScope, AuthorityScopeLevel};

        let mut result = AuthorityScope {
            level: AuthorityScopeLevel::Unspecified as i32,
            holding_id: None,
            company_id: None,
            branch_id: None,
            organization_node_id: None,
        };
        match authority.scope.scope.as_ref() {
            Some(Scope::Platform(_)) => result.level = AuthorityScopeLevel::Platform as i32,
            Some(Scope::Holding(value)) => {
                result.level = AuthorityScopeLevel::Holding as i32;
                result.holding_id = Some(value.holding_id.clone());
            }
            Some(Scope::Company(value)) => {
                result.level = AuthorityScopeLevel::Company as i32;
                result.holding_id = Some(value.holding_id.clone());
                result.company_id = Some(value.company_id.clone());
            }
            Some(Scope::Branch(value)) => {
                result.level = AuthorityScopeLevel::Branch as i32;
                result.holding_id = Some(value.holding_id.clone());
                result.company_id = Some(value.company_id.clone());
                result.branch_id = Some(value.branch_id.clone());
            }
            Some(Scope::Subtree(value)) => {
                result.level = AuthorityScopeLevel::Subtree as i32;
                result.holding_id = Some(value.holding_id.clone());
                result.organization_node_id = Some(value.organization_node_id.clone());
            }
            None => return Err(DeliveryError::Forbidden(error_meta(false, None))),
        }
        Ok(result)
    }

    fn actor(authority: &FeatureAuthority) -> ActorAttribution {
        ActorAttribution {
            principal_ref: authority.credential.principal_ref.clone(),
            kind: authority.actor_kind as i32,
            session_id: authority.credential.session_id.to_string(),
            represented_principal_ref: String::new(),
            support_capability_id: String::new(),
        }
    }

    fn consistency(value: &FeatureConsistency) -> QueryConsistency {
        let requirement = match value {
            FeatureConsistency::Eventual => {
                query_consistency::Requirement::Eventual(EventualConsistency {})
            }
            FeatureConsistency::ReadYourWrites {
                projection,
                partition,
                event_id,
                global_sequence,
                timeout_millis,
            } =>
            {
                #[allow(deprecated)]
                query_consistency::Requirement::ReadYourWrites(ReadYourWrites {
                    command_id: String::new(),
                    projection: projection.clone(),
                    partition: partition.clone(),
                    event_id: event_id.to_string(),
                    global_sequence: *global_sequence,
                    timeout_millis: *timeout_millis,
                })
            }
        };
        QueryConsistency {
            requirement: Some(requirement),
        }
    }

    pub(super) fn query_context(
        authority: &FeatureAuthority,
        consistency_mode: &FeatureConsistency,
        module: &str,
        purpose: &str,
        correlation_id: Uuid,
    ) -> QueryContext {
        QueryContext {
            contract_version: 1,
            request_id: Uuid::new_v4().to_string(),
            correlation_id: correlation_id.to_string(),
            actor: Some(actor(authority)),
            active_scope: Some(authority.scope.clone()),
            consistency: Some(consistency(consistency_mode)),
            audit: Some(AuditContext {
                purpose: purpose.into(),
                customer_visible: true,
                module: module.into(),
                correlation_id: correlation_id.to_string(),
            }),
        }
    }

    pub(crate) fn query_metadata(
        authority: &FeatureAuthority,
        consistency_mode: &FeatureConsistency,
        module: &str,
        purpose: &str,
        route: Option<HoldingRoute>,
        correlation_id: Uuid,
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
            consistency: Some(consistency(consistency_mode)),
            holding_route: route,
        }
    }

    fn command_metadata(
        authority: &FeatureAuthority,
        request: &IdentityPrincipalCommandRequest,
        route: HoldingRoute,
        correlation_id: Uuid,
    ) -> CommandRequestMetadata {
        let command_id = identity_command_id(&request.idempotency_key);
        CommandRequestMetadata {
            contract_version: 1,
            command_id: command_id.to_string(),
            idempotency_key: request.idempotency_key.clone(),
            correlation_id: correlation_id.to_string(),
            causation_id: None,
            actor: Some(actor(authority)),
            active_scope: Some(authority.scope.clone()),
            audit: Some(AuditContext {
                purpose: request.purpose.clone(),
                customer_visible: true,
                module: "identity-access".into(),
                correlation_id: correlation_id.to_string(),
            }),
            expected_aggregate_version: Some(request.expected_version),
            subscription_id: None,
            mfa_code: Some(request.mfa_code.clone()),
            holding_route: Some(route),
        }
    }

    pub(crate) fn owner_command_metadata(
        authority: &FeatureAuthority,
        route: HoldingRoute,
        module: &'static str,
        action: &'static str,
        record_id: Uuid,
        expected_version: u64,
        correlation_id: Uuid,
    ) -> CommandRequestMetadata {
        let idempotency_key = format!("{module}:{action}:{record_id}:{expected_version}");
        let command_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("erpx:owner-command:{idempotency_key}").as_bytes(),
        );
        CommandRequestMetadata {
            contract_version: 1,
            command_id: command_id.to_string(),
            idempotency_key,
            correlation_id: correlation_id.to_string(),
            causation_id: None,
            actor: Some(actor(authority)),
            active_scope: Some(authority.scope.clone()),
            audit: Some(AuditContext {
                purpose: format!("{action} from the authorized owner detail"),
                customer_visible: true,
                module: module.into(),
                correlation_id: correlation_id.to_string(),
            }),
            expected_aggregate_version: Some(expected_version),
            subscription_id: None,
            mfa_code: None,
            holding_route: Some(route),
        }
    }

    pub(crate) fn authenticated<T>(
        authority: &FeatureAuthority,
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
            let value = MetadataValue::try_from(value)
                .map_err(|_| DeliveryError::Unauthenticated(error_meta(false, None)))?;
            request.metadata_mut().insert(name, value);
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

    async fn capabilities(authority: &FeatureAuthority, path: &str) -> Vec<String> {
        let token = authority
            .credential
            .development_context_token
            .as_ref()
            .map(|value| value.expose().to_owned());
        crate::bff_navigation::fetch_navigation_manifest(
            authority.state.grpc_origin(),
            authority.credential.access_token.expose().to_owned(),
            authority.credential.principal_ref.clone(),
            authority.credential.session_id.to_string(),
            authority.scope_key.clone(),
            token,
            false,
        )
        .await
        .ok()
        .into_iter()
        .flatten()
        .flat_map(|group| group.items)
        .find(|item| {
            path == item.path
                || path
                    .strip_prefix(item.path.as_str())
                    .is_some_and(|suffix| suffix.starts_with('/'))
        })
        .map(|item| item.action_capabilities)
        .unwrap_or_default()
    }

    fn holding(value: platform::PlatformHoldingSummary, capabilities: &[String]) -> HoldingDetail {
        HoldingDetail {
            id: value.holding_id,
            customer_name: value.customer_name,
            lifecycle: value.state,
            registry_version: value.registry_version,
            routing: value.routing,
            projection: value.projection,
            backup_pitr: value.backup_pitr,
            storage: value.storage,
            configuration: value.configuration_state,
            subscription: value.subscription_state,
            integration_health: value.integration_health,
            health: value.health,
            projected_at: (!value.projected_at.is_empty()).then_some(value.projected_at),
            enterable: value.enterable,
            capabilities: capabilities.to_vec(),
            redactions: vec![
                FieldRedaction {
                    field: "customer_id".into(),
                    reason: "not required by this surface".into(),
                },
                FieldRedaction {
                    field: "canonical_holding_id".into(),
                    reason: "server-owned routing identity".into(),
                },
            ],
        }
    }

    fn holding_freshness(values: &[HoldingDetail]) -> String {
        if values.iter().any(|value| value.projection == "unavailable") {
            "unavailable"
        } else if values.iter().any(|value| value.projection == "rebuilding") {
            "rebuilding"
        } else if values.iter().any(|value| value.projection == "stale") {
            "stale"
        } else {
            "current"
        }
        .into()
    }

    pub(super) async fn list_platform_holdings(
        request: FeatureListRequest,
    ) -> Result<HoldingPage, DeliveryError> {
        let authority = feature_authority()?;
        if authority.scope_key != "platform" {
            return Err(DeliveryError::Forbidden(error_meta(false, None)));
        }
        let capabilities = capabilities(&authority, "/platform/holdings").await;
        let correlation_id = Uuid::new_v4();
        let wire = platform::ListPlatformHoldingsRequest {
            page_size: u32::from(request.page_size),
            after_holding_id: request.cursor.clone().unwrap_or_default(),
            context: Some(query_context(
                &authority,
                &request.consistency,
                "platform-administration",
                "list platform holdings",
                correlation_id,
            )),
            search: String::new(),
            lifecycle: String::new(),
            billing_state: String::new(),
            plan: String::new(),
            module: String::new(),
            health: String::new(),
        };
        let mut client = platform::platform_administration_query_service_client::PlatformAdministrationQueryServiceClient::new(channel(&authority).await?);
        let response = client
            .list_operational_holdings(authenticated(&authority, wire)?)
            .await
            .map_err(delivery_status)?
            .into_inner();
        let values = response
            .holdings
            .into_iter()
            .map(|value| holding(value, &capabilities))
            .collect::<Vec<_>>();
        let measured_at = values
            .iter()
            .filter_map(|value| value.projected_at.as_ref())
            .max()
            .cloned();
        let next_cursor = response
            .has_more
            .then_some(response.next_after_holding_id)
            .filter(|value| !value.is_empty());
        Ok(HoldingPage {
            evidence: HoldingProjectionEvidence {
                availability: if values.is_empty() {
                    "empty"
                } else {
                    "available"
                }
                .into(),
                freshness: holding_freshness(&values),
                measured_at,
                cursor: next_cursor.clone(),
                correlation_id: response.correlation_id,
            },
            values,
            next_cursor,
            page_size: request.page_size,
            capabilities,
            redactions: vec![FieldRedaction {
                field: "customer identifiers".into(),
                reason: "not required by the holdings list".into(),
            }],
        })
    }

    pub(super) async fn get_platform_holding(
        request: FeatureDetailRequest,
    ) -> Result<(HoldingDetail, HoldingProjectionEvidence), DeliveryError> {
        let authority = feature_authority()?;
        if authority.scope_key != "platform" {
            return Err(DeliveryError::Forbidden(error_meta(false, None)));
        }
        let capabilities = capabilities(&authority, "/platform/holdings").await;
        let correlation_id = Uuid::new_v4();
        let wire = platform::GetPlatformHoldingRequest {
            holding_id: request.id.to_string(),
            context: Some(query_context(
                &authority,
                &request.consistency,
                "platform-administration",
                "get platform holding",
                correlation_id,
            )),
        };
        let mut client = platform::platform_administration_query_service_client::PlatformAdministrationQueryServiceClient::new(channel(&authority).await?);
        let response = client
            .get_operational_holding(authenticated(&authority, wire)?)
            .await
            .map_err(delivery_status)?
            .into_inner();
        let value = response
            .holding
            .ok_or_else(|| DeliveryError::NotFound(error_meta(false, None)))?;
        let holding = holding(value, &capabilities);
        let evidence = HoldingProjectionEvidence {
            availability: "available".into(),
            freshness: holding_freshness(std::slice::from_ref(&holding)),
            measured_at: holding.projected_at.clone(),
            cursor: None,
            correlation_id: response.correlation_id,
        };
        Ok((holding, evidence))
    }

    fn holding_history_cursor_to_wire(
        cursor: Option<&str>,
    ) -> Result<(String, String), DeliveryError> {
        let Some(cursor) = cursor else {
            return Ok((String::new(), String::new()));
        };
        let (millis, event_id) = decode_holding_history_cursor(cursor)
            .ok_or_else(|| DeliveryError::Validation(error_meta(false, None)))?;
        let occurred_at = DateTime::<Utc>::from_timestamp_millis(millis)
            .ok_or_else(|| DeliveryError::Validation(error_meta(false, None)))?;
        Ok((occurred_at.to_rfc3339(), event_id.to_string()))
    }

    fn holding_history_cursor_from_wire(
        occurred_at: &str,
        event_id: &str,
    ) -> Result<String, DeliveryError> {
        let occurred_at = DateTime::parse_from_rfc3339(occurred_at)
            .map_err(|_| DeliveryError::Internal(error_meta(false, None)))?;
        let event_id = Uuid::parse_str(event_id)
            .map_err(|_| DeliveryError::Internal(error_meta(false, None)))?;
        if event_id.is_nil() {
            return Err(DeliveryError::Internal(error_meta(false, None)));
        }
        Ok(format!("{}.{}", occurred_at.timestamp_millis(), event_id))
    }

    fn valid_history_text(value: &str, max_len: usize) -> bool {
        !value.trim().is_empty() && value.len() <= max_len && !value.chars().any(char::is_control)
    }

    pub(super) async fn list_platform_holding_history(
        request: HoldingHistoryRequest,
    ) -> Result<HoldingControlHistoryPage, DeliveryError> {
        let authority = feature_authority()?;
        if authority.scope_key != "platform" {
            return Err(DeliveryError::Forbidden(error_meta(false, None)));
        }
        let capabilities = capabilities(&authority, "/platform/holdings").await;
        if !capabilities
            .iter()
            .any(|capability| capability == "ListHoldingControlHistory")
        {
            return Err(DeliveryError::Forbidden(error_meta(false, None)));
        }
        let (before_occurred_at, before_event_id) =
            holding_history_cursor_to_wire(request.cursor.as_deref())?;
        let correlation_id = Uuid::new_v4();
        let wire = platform::ListHoldingControlHistoryRequest {
            holding_id: request.holding_id.to_string(),
            page_size: u32::from(request.page_size),
            before_occurred_at,
            before_event_id,
            context: Some(query_context(
                &authority,
                &request.consistency,
                "platform-administration",
                "list holding control history",
                correlation_id,
            )),
        };
        let mut client = platform::platform_administration_query_service_client::PlatformAdministrationQueryServiceClient::new(channel(&authority).await?);
        let response = client
            .list_holding_control_history(authenticated(&authority, wire)?)
            .await
            .map_err(delivery_status)?
            .into_inner();
        if response.entries.len() > usize::from(request.page_size)
            || Uuid::parse_str(&response.correlation_id).is_err()
        {
            return Err(DeliveryError::Internal(error_meta(false, None)));
        }
        let mut values = Vec::with_capacity(response.entries.len());
        for entry in response.entries {
            if Uuid::parse_str(&entry.event_id).is_err()
                || entry.aggregate_version == 0
                || !valid_history_text(&entry.event_category, 128)
                || !valid_history_text(&entry.safe_action, 256)
                || !valid_history_text(&entry.actor_ref, 512)
                || Uuid::parse_str(&entry.correlation_id).is_err()
                || DateTime::parse_from_rfc3339(&entry.occurred_at).is_err()
            {
                return Err(DeliveryError::Internal(error_meta(false, None)));
            }
            values.push(HoldingControlHistoryEntry {
                event_id: entry.event_id,
                event_category: entry.event_category,
                safe_action: entry.safe_action,
                aggregate_version: entry.aggregate_version,
                actor_ref: entry.actor_ref,
                correlation_id: entry.correlation_id,
                occurred_at: entry.occurred_at,
            });
        }
        let next_cursor = if response.has_more {
            Some(holding_history_cursor_from_wire(
                &response.next_before_occurred_at,
                &response.next_before_event_id,
            )?)
        } else {
            None
        };
        let measured_at = values.first().map(|entry| entry.occurred_at.clone());
        Ok(HoldingControlHistoryPage {
            availability: if values.is_empty() {
                "empty"
            } else {
                "available"
            }
            .into(),
            freshness: "current".into(),
            measured_at,
            values,
            next_cursor,
            page_size: request.page_size,
            correlation_id: response.correlation_id,
            capabilities,
            redactions: vec![
                FieldRedaction {
                    field: "event_payload".into(),
                    reason: "internal command payloads are not exposed by this surface".into(),
                },
                FieldRedaction {
                    field: "persistence_keys".into(),
                    reason: "server-owned storage identifiers are not exposed".into(),
                },
            ],
        })
    }

    pub(crate) async fn resolve_holding_route(
        authority: &FeatureAuthority,
        consistency_mode: &FeatureConsistency,
        module: &'static str,
        purpose: &'static str,
    ) -> Result<HoldingRoute, DeliveryError> {
        let correlation_id = Uuid::new_v4();
        let wire = platform::ResolveActiveHoldingRouteRequest {
            context: Some(query_context(
                authority,
                consistency_mode,
                module,
                purpose,
                correlation_id,
            )),
        };
        let mut client = platform::platform_administration_query_service_client::PlatformAdministrationQueryServiceClient::new(channel(authority).await?);
        let response = client
            .resolve_active_holding_route(authenticated(authority, wire)?)
            .await
            .map_err(delivery_status)?
            .into_inner();
        let route = response
            .route
            .ok_or_else(|| DeliveryError::Unavailable(error_meta(true, Some(500))))?;
        if route.holding_id.is_empty() || route.data_profile_key.is_empty() {
            return Err(DeliveryError::Unavailable(error_meta(true, Some(500))));
        }
        Ok(HoldingRoute {
            holding_id: route.holding_id,
            data_profile_key: route.data_profile_key,
        })
    }

    async fn identity_route(
        authority: &FeatureAuthority,
        consistency_mode: &FeatureConsistency,
    ) -> Result<HoldingRoute, DeliveryError> {
        resolve_holding_route(
            authority,
            consistency_mode,
            "identity-access",
            "resolve active identity holding route",
        )
        .await
    }

    fn projection_evidence(
        metadata: Option<erpx_proto::common::QueryMetadata>,
        empty: bool,
    ) -> Result<IdentityProjectionEvidence, DeliveryError> {
        let metadata =
            metadata.ok_or_else(|| DeliveryError::Unavailable(error_meta(true, Some(500))))?;
        let projection = metadata
            .projection
            .ok_or_else(|| DeliveryError::Unavailable(error_meta(true, Some(500))))?;
        let freshness = match ProjectionFreshnessState::try_from(projection.freshness).ok() {
            Some(ProjectionFreshnessState::Current) => "current",
            Some(ProjectionFreshnessState::Stale) => "stale",
            Some(ProjectionFreshnessState::Rebuilding) => "rebuilding",
            Some(ProjectionFreshnessState::Unavailable) => "unavailable",
            _ => return Err(DeliveryError::Unavailable(error_meta(true, Some(500)))),
        };
        let cursor = projection.checkpoint.as_ref().map(|checkpoint| {
            format!(
                "{}:{}:{}",
                checkpoint.projection, checkpoint.partition, checkpoint.global_sequence
            )
        });
        let measured_at = projection
            .checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.updated_at.clone())
            .filter(|value| !value.is_empty());
        Ok(IdentityProjectionEvidence {
            availability: if freshness == "unavailable" {
                "unavailable"
            } else if empty {
                "empty"
            } else {
                "available"
            }
            .into(),
            freshness: freshness.into(),
            measured_at,
            cursor,
            correlation_id: metadata.correlation_id,
        })
    }

    fn principal_status(value: i32) -> String {
        match identity::IdentityAccessPrincipalStatus::try_from(value).ok() {
            Some(identity::IdentityAccessPrincipalStatus::Invited) => "invited",
            Some(identity::IdentityAccessPrincipalStatus::Active) => "active",
            Some(identity::IdentityAccessPrincipalStatus::Suspended) => "suspended",
            Some(identity::IdentityAccessPrincipalStatus::Disabled) => "disabled",
            _ => "unknown",
        }
        .into()
    }

    fn risk_tier(value: i32) -> String {
        match identity::IdentityAccessRoleRiskTier::try_from(value).ok() {
            Some(identity::IdentityAccessRoleRiskTier::PlatformRoot) => "platform-root",
            Some(identity::IdentityAccessRoleRiskTier::Administrator) => "administrator",
            Some(identity::IdentityAccessRoleRiskTier::High) => "high",
            Some(identity::IdentityAccessRoleRiskTier::Standard) => "standard",
            Some(identity::IdentityAccessRoleRiskTier::Contractor) => "contractor",
            Some(identity::IdentityAccessRoleRiskTier::SupportDiagnostic) => "support-diagnostic",
            Some(identity::IdentityAccessRoleRiskTier::SupportMutation) => "support-mutation",
            Some(identity::IdentityAccessRoleRiskTier::Emergency) => "emergency",
            _ => "unknown",
        }
        .into()
    }

    fn scope_label(value: Option<erpx_proto::common::AuthorityScope>) -> String {
        use erpx_proto::common::authority_scope::Scope;
        match value.and_then(|value| value.scope) {
            Some(Scope::Platform(_)) => "platform".into(),
            Some(Scope::Holding(value)) => format!("holding:{}", value.holding_id),
            Some(Scope::Company(value)) => {
                format!("company:{}:{}", value.holding_id, value.company_id)
            }
            Some(Scope::Branch(value)) => format!(
                "branch:{}:{}:{}",
                value.holding_id, value.company_id, value.branch_id
            ),
            Some(Scope::Subtree(value)) => format!(
                "subtree:{}:{}",
                value.holding_id, value.organization_node_id
            ),
            None => "unknown".into(),
        }
    }

    fn principal_record(
        value: identity::IdentityAccessPrincipalView,
        capabilities: &[String],
    ) -> IdentityRecord {
        let assignment_count = value.assignments.len();
        let active_assignments = value
            .assignments
            .iter()
            .filter(|assignment| assignment.revoked_at.is_none())
            .count();
        IdentityRecord {
            id: value.principal_id,
            title: value.display_name,
            subtitle: format!("{active_assignments}/{assignment_count} active grants"),
            status: principal_status(value.status),
            version: value.source_version,
            measured_at: (!value.projected_at.is_empty()).then_some(value.projected_at),
            fields: vec![
                IdentityField {
                    label: "Scope".into(),
                    value: scope_label(value.scope),
                    redacted: false,
                    redaction_reason: None,
                },
                IdentityField {
                    label: "Assignments".into(),
                    value: assignment_count.to_string(),
                    redacted: false,
                    redaction_reason: None,
                },
                IdentityField {
                    label: "Created".into(),
                    value: value.created_at,
                    redacted: false,
                    redaction_reason: None,
                },
                IdentityField {
                    label: "Updated".into(),
                    value: value.updated_at,
                    redacted: false,
                    redaction_reason: None,
                },
                IdentityField {
                    label: "Directory subject".into(),
                    value: String::new(),
                    redacted: true,
                    redaction_reason: Some(
                        "Immutable directory identity is exposed only on the directory-link surface"
                            .into(),
                    ),
                },
            ],
            capabilities: capabilities.to_vec(),
        }
    }

    fn role_record(
        value: identity::IdentityAccessRoleView,
        capabilities: &[String],
    ) -> IdentityRecord {
        let mut permissions = value.permissions;
        permissions.sort();
        permissions.dedup();
        IdentityRecord {
            id: value.role_id,
            title: value.display_name,
            subtitle: value.code,
            status: risk_tier(value.risk_tier),
            version: value.source_version,
            measured_at: (!value.projected_at.is_empty()).then_some(value.projected_at),
            fields: vec![
                IdentityField {
                    label: "Scope".into(),
                    value: scope_label(value.scope),
                    redacted: false,
                    redaction_reason: None,
                },
                IdentityField {
                    label: "Permissions".into(),
                    value: permissions.join(", "),
                    redacted: false,
                    redaction_reason: None,
                },
                IdentityField {
                    label: "Created".into(),
                    value: value.created_at,
                    redacted: false,
                    redaction_reason: None,
                },
                IdentityField {
                    label: "Updated".into(),
                    value: value.updated_at,
                    redacted: false,
                    redaction_reason: None,
                },
            ],
            capabilities: capabilities.to_vec(),
        }
    }

    fn grant_records(
        principals: Vec<identity::IdentityAccessPrincipalView>,
        capabilities: &[String],
    ) -> Vec<IdentityRecord> {
        principals
            .into_iter()
            .flat_map(|principal| {
                let principal_id = principal.principal_id;
                let display_name = principal.display_name;
                principal.assignments.into_iter().map(move |assignment| {
                    let status = if assignment.revoked_at.is_some() {
                        "revoked"
                    } else {
                        "active"
                    };
                    IdentityRecord {
                        id: assignment.assignment_id,
                        title: display_name.clone(),
                        subtitle: format!("role {}", assignment.role_id),
                        status: status.into(),
                        version: principal.source_version,
                        measured_at: (!principal.projected_at.is_empty())
                            .then_some(principal.projected_at.clone()),
                        fields: vec![
                            IdentityField {
                                label: "Principal".into(),
                                value: principal_id.clone(),
                                redacted: false,
                                redaction_reason: None,
                            },
                            IdentityField {
                                label: "Valid from".into(),
                                value: assignment.valid_from,
                                redacted: false,
                                redaction_reason: None,
                            },
                            IdentityField {
                                label: "Valid until".into(),
                                value: assignment.valid_until,
                                redacted: false,
                                redaction_reason: None,
                            },
                            IdentityField {
                                label: "Grant reason".into(),
                                value: assignment.grant_reason,
                                redacted: false,
                                redaction_reason: None,
                            },
                        ],
                        capabilities: capabilities.to_vec(),
                    }
                })
            })
            .collect()
    }

    pub(super) fn unavailable_page(
        kind: IdentityRecordKind,
        request: &FeatureListRequest,
    ) -> IdentityAccessPage {
        IdentityAccessPage {
            kind,
            values: Vec::new(),
            next_cursor: None,
            page_size: request.page_size,
            evidence: IdentityProjectionEvidence {
                availability: "unavailable".into(),
                freshness: "unavailable".into(),
                measured_at: None,
                cursor: None,
                correlation_id: Uuid::new_v4().to_string(),
            },
            capabilities: Vec::new(),
        }
    }

    pub(super) async fn list_identity_records(
        kind: IdentityRecordKind,
        request: FeatureListRequest,
    ) -> Result<IdentityAccessPage, DeliveryError> {
        if matches!(
            kind,
            IdentityRecordKind::AccessReview | IdentityRecordKind::BreakGlassSession
        ) {
            return Ok(unavailable_page(kind, &request));
        }
        let authority = feature_authority()?;
        if authority.scope_key == "platform" {
            return Err(DeliveryError::Forbidden(error_meta(false, None)));
        }
        let capabilities = capabilities(&authority, "/identity/principal").await;
        let route = identity_route(&authority, &request.consistency).await?;
        let correlation_id = Uuid::new_v4();
        let metadata = query_metadata(
            &authority,
            &request.consistency,
            "identity-access",
            match kind {
                IdentityRecordKind::Role => "list identity roles",
                IdentityRecordKind::Grant => "list identity grants",
                _ => "list identity principals",
            },
            Some(route),
            correlation_id,
        );
        let mut client =
            identity::identity_access_query_service_client::IdentityAccessQueryServiceClient::new(
                channel(&authority).await?,
            );
        match kind {
            IdentityRecordKind::Role => {
                let response = client
                    .list_roles(authenticated(
                        &authority,
                        identity::ListIdentityAccessRolesRequest {
                            metadata: Some(metadata),
                            risk_tier: None,
                            page_size: u32::from(request.page_size),
                            cursor: request.cursor,
                        },
                    )?)
                    .await
                    .map_err(delivery_status)?
                    .into_inner();
                let values = response
                    .roles
                    .into_iter()
                    .map(|value| role_record(value, &capabilities))
                    .collect::<Vec<_>>();
                Ok(IdentityAccessPage {
                    kind,
                    evidence: projection_evidence(response.metadata, values.is_empty())?,
                    values,
                    next_cursor: response.next_cursor,
                    page_size: request.page_size,
                    capabilities,
                })
            }
            IdentityRecordKind::Principal | IdentityRecordKind::Grant => {
                let response = client
                    .list_principals(authenticated(
                        &authority,
                        identity::ListIdentityAccessPrincipalsRequest {
                            metadata: Some(metadata),
                            status: None,
                            page_size: u32::from(request.page_size),
                            cursor: request.cursor,
                        },
                    )?)
                    .await
                    .map_err(delivery_status)?
                    .into_inner();
                let values = if kind == IdentityRecordKind::Grant {
                    grant_records(response.principals, &capabilities)
                } else {
                    response
                        .principals
                        .into_iter()
                        .map(|value| principal_record(value, &capabilities))
                        .collect()
                };
                Ok(IdentityAccessPage {
                    kind,
                    evidence: projection_evidence(response.metadata, values.is_empty())?,
                    values,
                    next_cursor: response.next_cursor,
                    page_size: request.page_size,
                    capabilities,
                })
            }
            IdentityRecordKind::AccessReview | IdentityRecordKind::BreakGlassSession => {
                unreachable!("explicitly handled above")
            }
        }
    }

    pub(super) async fn get_identity_record(
        kind: IdentityRecordKind,
        request: FeatureDetailRequest,
    ) -> Result<IdentityAccessDetail, DeliveryError> {
        if !matches!(
            kind,
            IdentityRecordKind::Principal | IdentityRecordKind::Role
        ) {
            return Err(DeliveryError::Unavailable(error_meta(true, Some(500))));
        }
        let authority = feature_authority()?;
        if authority.scope_key == "platform" {
            return Err(DeliveryError::Forbidden(error_meta(false, None)));
        }
        let capabilities = capabilities(&authority, "/identity/principal").await;
        let route = identity_route(&authority, &request.consistency).await?;
        let correlation_id = Uuid::new_v4();
        let metadata = query_metadata(
            &authority,
            &request.consistency,
            "identity-access",
            if kind == IdentityRecordKind::Role {
                "get identity role"
            } else {
                "get identity principal"
            },
            Some(route),
            correlation_id,
        );
        let mut client =
            identity::identity_access_query_service_client::IdentityAccessQueryServiceClient::new(
                channel(&authority).await?,
            );
        if kind == IdentityRecordKind::Role {
            let response = client
                .get_role(authenticated(
                    &authority,
                    identity::GetIdentityAccessRoleRequest {
                        metadata: Some(metadata),
                        role_id: request.id.to_string(),
                    },
                )?)
                .await
                .map_err(delivery_status)?
                .into_inner();
            let record = response
                .role
                .map(|value| role_record(value, &capabilities))
                .ok_or_else(|| DeliveryError::NotFound(error_meta(false, None)))?;
            Ok(IdentityAccessDetail {
                kind,
                evidence: projection_evidence(response.metadata, false)?,
                record,
            })
        } else {
            let response = client
                .get_principal(authenticated(
                    &authority,
                    identity::GetIdentityAccessPrincipalRequest {
                        metadata: Some(metadata),
                        principal_id: request.id.to_string(),
                    },
                )?)
                .await
                .map_err(delivery_status)?
                .into_inner();
            let record = response
                .principal
                .map(|value| principal_record(value, &capabilities))
                .ok_or_else(|| DeliveryError::NotFound(error_meta(false, None)))?;
            Ok(IdentityAccessDetail {
                kind,
                evidence: projection_evidence(response.metadata, false)?,
                record,
            })
        }
    }

    pub(super) async fn command_identity_principal(
        request: IdentityPrincipalCommandRequest,
    ) -> Result<IdentityPrincipalCommandOutcome, DeliveryError> {
        let authority = feature_authority()?;
        if authority.scope_key == "platform" {
            return Err(DeliveryError::Forbidden(error_meta(false, None)));
        }
        let capabilities = capabilities(&authority, "/identity/principal").await;
        if !capabilities
            .iter()
            .any(|capability| capability == request.action.capability())
        {
            return Err(DeliveryError::Forbidden(error_meta(false, None)));
        }
        let route = identity_route(&authority, &FeatureConsistency::Eventual).await?;
        let correlation_id = Uuid::new_v4();
        let metadata = command_metadata(&authority, &request, route, correlation_id);
        let wire = identity::IdentityAccessPrincipalMutationRequest {
            metadata: Some(metadata),
            principal_id: request.principal_id.to_string(),
        };
        let mut client = identity::identity_access_command_service_client::IdentityAccessCommandServiceClient::new(channel(&authority).await?);
        let response = match request.action {
            IdentityPrincipalLifecycleAction::Activate => {
                client
                    .activate_principal(authenticated(&authority, wire)?)
                    .await
            }
            IdentityPrincipalLifecycleAction::Suspend => {
                client
                    .suspend_principal(authenticated(&authority, wire)?)
                    .await
            }
            IdentityPrincipalLifecycleAction::Reactivate => {
                client
                    .reactivate_principal(authenticated(&authority, wire)?)
                    .await
            }
            IdentityPrincipalLifecycleAction::Disable => {
                client
                    .disable_principal(authenticated(&authority, wire)?)
                    .await
            }
        }
        .map_err(|status| {
            tracing::warn!(
                code = ?status.code(),
                reason = status.message(),
                "Identity principal command owner rejected the request"
            );
            delivery_status(status)
        })?
        .into_inner();
        let principal = response
            .principal
            .ok_or_else(|| DeliveryError::Internal(error_meta(false, None)))?;
        let receipt = response
            .receipt
            .ok_or_else(|| DeliveryError::Internal(error_meta(false, None)))?;
        let aggregate = receipt
            .aggregate
            .ok_or_else(|| DeliveryError::Internal(error_meta(false, None)))?;
        let target = receipt
            .committed_events
            .last()
            .ok_or_else(|| DeliveryError::Internal(error_meta(false, None)))?;
        let command_id = Uuid::parse_str(&receipt.command_id)
            .map_err(|_| DeliveryError::Internal(error_meta(false, None)))?;
        let audit_reference = Uuid::parse_str(&receipt.correlation_id)
            .map_err(|_| DeliveryError::Internal(error_meta(false, None)))?;
        let aggregate_id = Uuid::parse_str(&aggregate.aggregate_id)
            .map_err(|_| DeliveryError::Internal(error_meta(false, None)))?;
        let projection_event_id = Uuid::parse_str(&target.event_id)
            .map_err(|_| DeliveryError::Internal(error_meta(false, None)))?;
        if aggregate_id != request.principal_id
            || principal.principal_id != request.principal_id.to_string()
            || aggregate.version == 0
            || target.global_sequence == 0
        {
            return Err(DeliveryError::Internal(error_meta(false, None)));
        }
        let partition = format!(
            "identity_access.principal:{}",
            request.principal_id.simple()
        );
        let authoritative = get_identity_record(
            IdentityRecordKind::Principal,
            FeatureDetailRequest {
                id: request.principal_id,
                locale: request.locale,
                consistency: FeatureConsistency::ReadYourWrites {
                    projection: "identity_access.principals".into(),
                    partition: partition.clone(),
                    event_id: projection_event_id,
                    global_sequence: target.global_sequence,
                    timeout_millis: 8_000,
                },
            },
        )
        .await;
        let (detail, projection_state) = match authoritative {
            Ok(detail) => (Some(detail), "current"),
            Err(DeliveryError::Unavailable(_)) => (None, "pending"),
            Err(error) => return Err(error),
        };
        Ok(IdentityPrincipalCommandOutcome {
            receipt: FeatureCommandReceipt {
                command_id,
                aggregate_id,
                aggregate_version: aggregate.version,
                replayed: receipt.replayed,
                projection: "identity_access.principals".into(),
                partition,
                projection_event_id,
                projection_sequence: target.global_sequence,
                invalidation_tags: vec![
                    "identity-access".into(),
                    format!("identity-principal:{}", request.principal_id),
                    "authorized-navigation".into(),
                ],
                audit_reference,
            },
            detail,
            projection_state: projection_state.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_contract_bounds_page_cursor_locale_and_ryw_wait() {
        assert!(
            validate_list_request(&FeatureListRequest {
                cursor: None,
                page_size: 100,
                locale: "th".into(),
                consistency: FeatureConsistency::Eventual,
            })
            .is_ok()
        );
        assert!(
            validate_list_request(&FeatureListRequest {
                cursor: None,
                page_size: 101,
                locale: "en".into(),
                consistency: FeatureConsistency::Eventual,
            })
            .is_err()
        );
        assert!(
            validate_consistency(&FeatureConsistency::ReadYourWrites {
                projection: "identity_access_principals".into(),
                partition: "holding".into(),
                event_id: Uuid::new_v4(),
                global_sequence: 1,
                timeout_millis: 10_001,
            })
            .is_err()
        );
    }

    #[test]
    fn unsupported_governance_pages_are_explicitly_unavailable_not_empty_success() {
        #[cfg(feature = "server")]
        {
            let request = FeatureListRequest {
                cursor: None,
                page_size: 25,
                locale: "en".into(),
                consistency: FeatureConsistency::Eventual,
            };
            let page = server::unavailable_page(IdentityRecordKind::AccessReview, &request);
            assert_eq!(page.evidence.availability, "unavailable");
            assert_eq!(page.evidence.freshness, "unavailable");
            assert!(page.values.is_empty());
        }
    }

    #[test]
    fn principal_commands_require_versioned_idempotent_fresh_mfa_input() {
        let request = IdentityPrincipalCommandRequest {
            principal_id: Uuid::new_v4(),
            action: IdentityPrincipalLifecycleAction::Suspend,
            idempotency_key: "iam:suspend:stable-request".into(),
            expected_version: 2,
            locale: "th".into(),
            purpose: "Suspend a compromised account".into(),
            mfa_code: "123456".into(),
        };
        assert!(validate_principal_command(&request).is_ok());
        assert_eq!(
            identity_command_id(&request.idempotency_key),
            identity_command_id(&request.idempotency_key)
        );
        let mut changed = request.clone();
        changed.idempotency_key.push_str(":changed");
        assert_ne!(
            identity_command_id(&request.idempotency_key),
            identity_command_id(&changed.idempotency_key)
        );
        changed.expected_version = 0;
        assert!(validate_principal_command(&changed).is_err());
        changed.expected_version = 2;
        changed.mfa_code.clear();
        assert!(validate_principal_command(&changed).is_err());
    }

    #[test]
    fn holding_history_cursor_is_opaque_bounded_and_stable() {
        let event_id = Uuid::from_u128(7);
        let cursor = format!("1787270400000.{event_id}");
        assert_eq!(
            decode_holding_history_cursor(&cursor),
            Some((1_787_270_400_000, event_id))
        );
        let request = HoldingHistoryRequest {
            holding_id: Uuid::from_u128(8),
            cursor: Some(cursor),
            page_size: 100,
            locale: "en".into(),
            consistency: FeatureConsistency::Eventual,
        };
        assert!(validate_holding_history_request(&request).is_ok());

        let mut invalid = request.clone();
        invalid.cursor = Some("2026-08-21T00:00:00Z|not-a-uuid".into());
        assert!(validate_holding_history_request(&invalid).is_err());
        invalid = request;
        invalid.page_size = 101;
        assert!(validate_holding_history_request(&invalid).is_err());
    }
}
