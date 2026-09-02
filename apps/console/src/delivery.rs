use async_trait::async_trait;
use dioxus::prelude::dioxus_fullstack;
#[cfg(feature = "server")]
use dioxus::prelude::dioxus_server;
use erpx_delivery_contract::{
    AiProviderConfigView, AiProviderPresetView, AiProviderTestRequest, AiProviderTestResponse,
    AiSuggestionRequest, AiSuggestionResponse, AuthenticationRequest, BootstrapRequest,
    CollaborationEvent, CollaborationSubscription, CommandReceipt, CommandRequest, ConsoleGateway,
    DeliveryError, FlowAnalysisRequest, FlowAnalysisResponse, HostedPricingQuote,
    PublicMarketingFormRequest, PublicMarketingPreviewRequest, QueryRequest, RouteData,
    SaveAiProviderConfigRequest, SessionBootstrap,
};
#[cfg(feature = "server")]
use erpx_delivery_contract::{
    AuthorizedNavigation, AuthorizedNavigationGroup, AuthorizedRoute, ClientChannel,
    CommandContext, DELIVERY_CONTRACT_VERSION, DeliveryErrorMeta, QueryContext, ValidatedAuthority,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct ServerFunctionGateway;

#[async_trait(?Send)]
impl ConsoleGateway for ServerFunctionGateway {
    async fn bootstrap(
        &self,
        request: BootstrapRequest,
    ) -> Result<SessionBootstrap, DeliveryError> {
        session_bootstrap(request.locale)
            .await
            .map_err(|_| DeliveryError::unavailable(None))
    }

    async fn query(&self, request: QueryRequest) -> Result<RouteData, DeliveryError> {
        query_route(request)
            .await
            .map_err(|_| DeliveryError::unavailable(None))?
    }

    async fn command(&self, request: CommandRequest) -> Result<CommandReceipt, DeliveryError> {
        command(request)
            .await
            .map_err(|_| DeliveryError::unavailable(None))?
    }

    async fn authenticate(
        &self,
        request: AuthenticationRequest,
    ) -> Result<SessionBootstrap, DeliveryError> {
        authenticate(request)
            .await
            .map_err(|_| DeliveryError::unavailable(None))?
    }

    async fn submit_public_marketing_form(
        &self,
        request: PublicMarketingFormRequest,
    ) -> Result<CommandReceipt, DeliveryError> {
        submit_public_marketing_form(request)
            .await
            .map_err(|_| DeliveryError::unavailable(None))?
    }

    async fn public_marketing_preview(
        &self,
        request: PublicMarketingPreviewRequest,
    ) -> Result<RouteData, DeliveryError> {
        public_marketing_preview(request)
            .await
            .map_err(|_| DeliveryError::unavailable(None))?
    }

    async fn collaboration_stream(
        &self,
        request: CollaborationSubscription,
    ) -> Result<Vec<CollaborationEvent>, DeliveryError> {
        collaboration_events(request)
            .await
            .map_err(|_| DeliveryError::unavailable(None))?
    }

    async fn ai_suggest(
        &self,
        request: AiSuggestionRequest,
    ) -> Result<AiSuggestionResponse, DeliveryError> {
        ai_suggest(request)
            .await
            .map_err(|_| DeliveryError::unavailable(None))?
    }

    async fn ai_flow_analyze(
        &self,
        request: FlowAnalysisRequest,
    ) -> Result<FlowAnalysisResponse, DeliveryError> {
        ai_flow_analyze(request)
            .await
            .map_err(|_| DeliveryError::unavailable(None))?
    }

    async fn ai_provider_presets(&self) -> Result<Vec<AiProviderPresetView>, DeliveryError> {
        ai_provider_presets()
            .await
            .map_err(|_| DeliveryError::unavailable(None))?
    }

    async fn hosted_quote(&self, seats: u32) -> Result<HostedPricingQuote, DeliveryError> {
        hosted_quote(seats)
            .await
            .map_err(|_| DeliveryError::unavailable(None))?
    }

    async fn save_ai_provider_config(
        &self,
        request: SaveAiProviderConfigRequest,
    ) -> Result<AiProviderConfigView, DeliveryError> {
        save_ai_provider_config(request)
            .await
            .map_err(|_| DeliveryError::unavailable(None))?
    }

    async fn load_ai_provider_config(&self) -> Result<Option<AiProviderConfigView>, DeliveryError> {
        load_ai_provider_config()
            .await
            .map_err(|_| DeliveryError::unavailable(None))?
    }

    async fn test_ai_provider_connection(
        &self,
        request: AiProviderTestRequest,
    ) -> Result<AiProviderTestResponse, DeliveryError> {
        test_ai_provider_connection(request)
            .await
            .map_err(|_| DeliveryError::unavailable(None))?
    }
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/bootstrap")]
pub async fn session_bootstrap(
    locale: String,
) -> dioxus::prelude::ServerFnResult<SessionBootstrap> {
    #[cfg(feature = "server")]
    {
        use dioxus::fullstack::FullstackContext;
        use dioxus::server::axum::http::{HeaderValue, header};

        let context = FullstackContext::current()
            .ok_or_else(|| dioxus::prelude::ServerFnError::new("request context unavailable"))?;
        context.add_response_header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
        let locale = {
            let parts = context.parts_mut();
            locale_from_cookie_header(
                parts
                    .headers
                    .get(header::COOKIE)
                    .and_then(|value| value.to_str().ok()),
                &locale,
            )
        };
        let session_context = context
            .extension::<crate::bff::BffSessionContext>()
            .unwrap_or_else(|| crate::bff::BffSessionContext {
                snapshot: erpx_web_session::SafeWebSessionSnapshot::signed_out(),
                credential: None,
                bundle: None,
            });
        let snapshot = session_context.snapshot.clone();
        let active_role = snapshot.active_role.as_ref();
        let platform_origin = context
            .extension::<crate::bff::BffState>()
            .map(|state| state.platform_origin().to_owned())
            .unwrap_or_default();
        let authorized_navigation = if let (Some(state), Some(credential), Some(scope_key)) = (
            context.extension::<crate::bff::BffState>(),
            session_context.credential.as_ref(),
            active_role.and_then(|role| role.scope_key.as_ref()),
        ) {
            let development_context_token = credential
                .development_context_token
                .as_ref()
                .map(|token| token.expose().to_owned());
            let groups = crate::bff_navigation::fetch_navigation_manifest(
                state.grpc_origin(),
                credential.access_token.expose().to_owned(),
                credential.principal_ref.clone(),
                credential.session_id.to_string(),
                scope_key.clone(),
                development_context_token,
                false,
            )
            .await
            .map_err(|_| {
                dioxus::prelude::ServerFnError::new("authorized navigation unavailable")
            })?;
            let authorization_context_version = session_context
                .bundle
                .as_ref()
                .and_then(erpx_web_session::WebSessionBundle::active_account)
                .and_then(|account| account.authorization_context_version);
            Some(AuthorizedNavigation {
                home_route: if scope_key == "platform" {
                    "/platform/holdings".into()
                } else {
                    "/work".into()
                },
                groups: groups
                    .into_iter()
                    .map(|group| AuthorizedNavigationGroup {
                        group_id: group.group_id,
                        label: group.label,
                        label_key: group.label_key,
                        routes: group
                            .items
                            .into_iter()
                            .filter(|item| item.is_authorized())
                            .map(|item| AuthorizedRoute {
                                route_id: item.route_id,
                                label: item.label,
                                label_key: item.label_key,
                                path: item.path,
                                icon: item.icon,
                                action_capabilities: item.action_capabilities,
                            })
                            .collect(),
                    })
                    .collect(),
                authorization_context_version,
            })
        } else {
            None
        };
        let session_id = session_context
            .credential
            .as_ref()
            .map(|c| c.session_id.to_string());
        return Ok(SessionBootstrap {
            authenticated: snapshot.authenticated,
            actor_ref: snapshot.principal_ref,
            data_profile_key: active_role.and_then(|role| role.scope_key.clone()),
            role_ref: active_role.and_then(|role| role.role_key.clone()),
            locale: canonical_locale(&locale),
            channel: ClientChannel::Web,
            catalog_version: DELIVERY_CONTRACT_VERSION,
            platform_origin,
            authorized_navigation,
            session_id,
        });
    }

    #[allow(unreachable_code)]
    Ok(SessionBootstrap {
        authenticated: false,
        actor_ref: None,
        data_profile_key: None,
        role_ref: None,
        locale: canonical_locale(&locale),
        channel: ClientChannel::Web,
        catalog_version: DELIVERY_CONTRACT_VERSION,
        platform_origin: String::new(),
        authorized_navigation: None,
        session_id: None,
    })
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/query")]
pub async fn query_route(
    request: QueryRequest,
) -> dioxus::prelude::ServerFnResult<Result<RouteData, DeliveryError>> {
    #[cfg(feature = "server")]
    {
        use dioxus::fullstack::FullstackContext;
        use dioxus::server::axum::http::{HeaderValue, header};

        let context = FullstackContext::current()
            .ok_or_else(|| dioxus::prelude::ServerFnError::new("request context unavailable"))?;
        context.add_response_header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
        let snapshot = context
            .extension::<crate::bff::BffSessionContext>()
            .map(|session| session.snapshot)
            .unwrap_or_else(erpx_web_session::SafeWebSessionSnapshot::signed_out);
        if !snapshot.authenticated {
            return Ok(Err(DeliveryError::Unauthenticated(error_meta(false, None))));
        }
        if request.limit == 0 || request.limit > erpx_delivery_contract::MAX_PAGE_SIZE_ {
            return Ok(Err(DeliveryError::Validation(error_meta(false, None))));
        }
        let _trusted_context = match trusted_query_context(&request) {
            Ok(context) => context,
            Err(error) => return Ok(Err(error)),
        };
        if matches!(
            &request.route,
            erpx_delivery_contract::RouteQuery::Workspace { .. }
                | erpx_delivery_contract::RouteQuery::Record { .. }
                | erpx_delivery_contract::RouteQuery::Reference { .. }
        ) {
            return Ok(crate::workspace_delivery::query_route(&request).await);
        }
        return Ok(Ok(crate::routes::route_data_for_query(&request)));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/command")]
pub async fn command(
    request: CommandRequest,
) -> dioxus::prelude::ServerFnResult<Result<CommandReceipt, DeliveryError>> {
    #[cfg(feature = "server")]
    {
        use std::collections::BTreeSet;

        validate_mutation_request()?;
        let trusted = match trusted_command_context(&request) {
            Ok(context) => context,
            Err(error) => return Ok(Err(error)),
        };
        let receipt = match &request.operation {
            erpx_delivery_contract::CommandOperation::CreateRecord {
                workspace,
                resource,
                title,
            } => {
                if title.trim().is_empty() || title.len() > 120 {
                    return Ok(Err(DeliveryError::Validation(error_meta(false, None))));
                }
                let _ = (trusted, resource);
                let aggregate_id =
                    match erpx_delivery_contract::RouteUuid::try_new(uuid::Uuid::new_v4()) {
                        Ok(v) => v,
                        Err(_) => {
                            return Ok(Err(DeliveryError::Validation(error_meta(false, None))));
                        }
                    };
                CommandReceipt {
                    command_id: uuid::Uuid::new_v4(),
                    aggregate_id,
                    aggregate_version: 1,
                    projection_cursor: Some(format!(
                        "{}:{}:{}",
                        workspace.slug(),
                        resource.as_str(),
                        title
                    )),
                    invalidation_tags: BTreeSet::from([
                        format!("workspace:{}", workspace.slug()),
                        format!("resource:{}", resource.as_str()),
                    ]),
                    audit_reference: uuid::Uuid::new_v4().to_string(),
                }
            }
            erpx_delivery_contract::CommandOperation::UpdateTitle {
                workspace,
                resource,
                id,
                title,
            } => {
                if title.trim().is_empty() || title.len() > 120 {
                    return Ok(Err(DeliveryError::Validation(error_meta(false, None))));
                }
                let _ = (trusted, workspace, resource);
                CommandReceipt {
                    command_id: uuid::Uuid::new_v4(),
                    aggregate_id: *id,
                    aggregate_version: request.expected_version.unwrap_or(1).saturating_add(1),
                    projection_cursor: Some(format!(
                        "{}:{}:{}:update",
                        workspace.slug(),
                        resource.as_str(),
                        title
                    )),
                    invalidation_tags: BTreeSet::from([
                        format!("workspace:{}", workspace.slug()),
                        format!("record:{}", id),
                    ]),
                    audit_reference: uuid::Uuid::new_v4().to_string(),
                }
            }
            erpx_delivery_contract::CommandOperation::TransitionRecord {
                workspace,
                resource,
                id,
                transition,
            } => {
                let _ = (
                    trusted,
                    workspace,
                    resource,
                    transition,
                    request.expected_version,
                );
                CommandReceipt {
                    command_id: uuid::Uuid::new_v4(),
                    aggregate_id: *id,
                    aggregate_version: request.expected_version.unwrap_or(1).saturating_add(1),
                    projection_cursor: Some(format!(
                        "{}:{}:{:?}:transition",
                        workspace.slug(),
                        resource.as_str(),
                        transition
                    )),
                    invalidation_tags: BTreeSet::from([
                        format!("workspace:{}", workspace.slug()),
                        format!("record:{}", id),
                    ]),
                    audit_reference: uuid::Uuid::new_v4().to_string(),
                }
            }
            erpx_delivery_contract::CommandOperation::ArchiveRecord {
                workspace,
                resource,
                id,
            } => {
                let _ = (trusted, workspace, resource);
                CommandReceipt {
                    command_id: uuid::Uuid::new_v4(),
                    aggregate_id: *id,
                    aggregate_version: request.expected_version.unwrap_or(1).saturating_add(1),
                    projection_cursor: Some(format!(
                        "{}:{}:archive",
                        workspace.slug(),
                        resource.as_str()
                    )),
                    invalidation_tags: BTreeSet::from([
                        format!("workspace:{}", workspace.slug()),
                        format!("record:{}", id),
                    ]),
                    audit_reference: uuid::Uuid::new_v4().to_string(),
                }
            }
        };
        return Ok(Ok(receipt));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/authenticate")]
pub async fn authenticate(
    request: AuthenticationRequest,
) -> dioxus::prelude::ServerFnResult<Result<SessionBootstrap, DeliveryError>> {
    #[cfg(feature = "server")]
    {
        use dioxus::fullstack::FullstackContext;

        validate_mutation_request()?;
        let canonical_return = crate::validated_login_return_path(&request.return_path);
        if request.account.trim().is_empty()
            || request.proof.is_empty()
            || canonical_return.as_deref() != Some(request.return_path.as_str())
        {
            return Ok(Err(DeliveryError::Validation(error_meta(false, None))));
        }
        if let Some(ctx) = FullstackContext::current()
            && let Some(state) = ctx.extension::<crate::bff::BffState>()
        {
            let body = serde_json::json!({
                "principal_ref": request.account,
                "password": request.proof,
                "device_label": "console"
            });
            let url = format!(
                "{}/auth/login",
                state.auth_http_origin().trim_end_matches('/')
            );
            if let Ok(resp) = state.auth_http().post(&url).json(&body).send().await
                && resp.status().is_success()
                && let Ok(val) = resp.json::<serde_json::Value>().await
            {
                let session_id = val
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_owned());
                return Ok(Ok(SessionBootstrap {
                    authenticated: true,
                    actor_ref: Some(request.account),
                    data_profile_key: Some("holding:authenticated".into()),
                    role_ref: Some("role:member".into()),
                    locale: canonical_locale("en"),
                    channel: erpx_delivery_contract::ClientChannel::Web,
                    catalog_version: erpx_delivery_contract::DELIVERY_CONTRACT_VERSION,
                    platform_origin: state.platform_origin().to_owned(),
                    authorized_navigation: None,
                    session_id,
                }));
            }
        }
        return Ok(Err(DeliveryError::Unauthenticated(error_meta(false, None))));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/public-form")]
pub async fn submit_public_marketing_form(
    request: PublicMarketingFormRequest,
) -> dioxus::prelude::ServerFnResult<Result<CommandReceipt, DeliveryError>> {
    #[cfg(feature = "server")]
    {
        use dioxus::fullstack::FullstackContext;

        validate_mutation_request()?;
        let canonical_locale = canonical_locale(&request.locale);
        let valid_email = request.email.len() <= 254
            && request
                .email
                .split_once('@')
                .is_some_and(|(local, domain)| {
                    !local.is_empty()
                        && domain.contains('.')
                        && !domain.starts_with('.')
                        && !domain.ends_with('.')
                });
        if !request.consent
            || request.name.trim().is_empty()
            || request.name.len() > 120
            || !valid_email
            || request.message.trim().is_empty()
            || request.message.len() > 4_000
            || request.idempotency_key.is_empty()
            || request.idempotency_key.len() > 128
            || request.purpose != "marketing_contact"
            || canonical_locale != request.locale
        {
            return Ok(Err(DeliveryError::Validation(error_meta(false, None))));
        }
        let context = FullstackContext::current()
            .ok_or_else(|| dioxus::prelude::ServerFnError::new("request context unavailable"))?;
        let state = context
            .extension::<crate::bff::BffState>()
            .ok_or_else(|| dioxus::prelude::ServerFnError::new("BFF state unavailable"))?;
        let headers = context.parts_mut().headers.clone();
        return Ok(crate::bff::submit_public_marketing_request(&state, &headers, &request).await);
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/public-preview")]
pub async fn public_marketing_preview(
    request: PublicMarketingPreviewRequest,
) -> dioxus::prelude::ServerFnResult<Result<RouteData, DeliveryError>> {
    #[cfg(feature = "server")]
    {
        use dioxus::fullstack::FullstackContext;
        use dioxus::server::axum::http::{HeaderName, HeaderValue, header};

        let context = FullstackContext::current()
            .ok_or_else(|| dioxus::prelude::ServerFnError::new("request context unavailable"))?;
        if request.access_code.is_empty()
            || request.access_code.len() > 512
            || canonical_locale(&request.locale) != request.locale
        {
            return Ok(Err(DeliveryError::NotFound(error_meta(false, None))));
        }
        let state = context
            .extension::<crate::bff::BffState>()
            .ok_or_else(|| dioxus::prelude::ServerFnError::new("BFF state unavailable"))?;
        let headers = context.parts_mut().headers.clone();
        let data = match crate::bff::load_public_marketing_preview(&state, &headers, &request).await
        {
            Ok(data) => data,
            Err(error) => return Ok(Err(error)),
        };
        context.add_response_header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=60, stale-while-revalidate=300"),
        );
        context.add_response_header(
            HeaderName::from_static("vary"),
            HeaderValue::from_static("Accept-Language"),
        );
        if let Some(validator) = &data.public_validator
            && let Ok(value) = HeaderValue::from_str(validator)
        {
            context.add_response_header(header::ETAG, value);
        }
        return Ok(Ok(data));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/collaboration")]
pub async fn collaboration_events(
    request: CollaborationSubscription,
) -> dioxus::prelude::ServerFnResult<Result<Vec<CollaborationEvent>, DeliveryError>> {
    #[cfg(feature = "server")]
    {
        use dioxus::fullstack::FullstackContext;
        use dioxus::server::axum::http::{HeaderValue, header};

        let context = FullstackContext::current()
            .ok_or_else(|| dioxus::prelude::ServerFnError::new("request context unavailable"))?;
        context.add_response_header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
        let authenticated = context
            .extension::<crate::bff::BffSessionContext>()
            .is_some_and(|session| session.snapshot.authenticated);
        if !authenticated {
            return Ok(Err(DeliveryError::Unauthenticated(error_meta(false, None))));
        }
        if let Some(pool) = context
            .extension::<crate::bff::BffState>()
            .and_then(|state| state.control_pool().cloned())
            && let Ok(rows) = sqlx::query_as::<_, (String, Option<String>)>(
                r#"SELECT thread_id::text, anchor->>'locator' FROM erpx_global_collaboration.threads LIMIT 10"#,
            )
            .fetch_all(&pool)
            .await
            {
                let events: Vec<CollaborationEvent> = rows
                    .into_iter()
                    .map(|(cursor, locator)| CollaborationEvent {
                        cursor: cursor.clone(),
                        topic: request
                            .workspace
                            .map(|w| w.slug().to_owned())
                            .unwrap_or_else(|| "global".into()),
                        summary: locator.unwrap_or_else(|| "thread".into()),
                    })
                    .collect();
                if !events.is_empty() {
                    return Ok(Ok(events));
                }
            }
        return Ok(Ok(Vec::new()));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/ai-suggest")]
pub async fn ai_suggest(
    request: AiSuggestionRequest,
) -> dioxus::prelude::ServerFnResult<Result<AiSuggestionResponse, DeliveryError>> {
    #[cfg(feature = "server")]
    {
        use dioxus::fullstack::FullstackContext;
        use dioxus::server::axum::http::{HeaderValue, header};

        let context = FullstackContext::current()
            .ok_or_else(|| dioxus::prelude::ServerFnError::new("request context unavailable"))?;
        context.add_response_header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
        let authenticated = context
            .extension::<crate::bff::BffSessionContext>()
            .is_some_and(|session| session.snapshot.authenticated);
        if !authenticated {
            return Ok(Err(DeliveryError::Unauthenticated(error_meta(false, None))));
        }
        if request.field.trim().is_empty() || request.field.len() > 128 {
            return Ok(Err(DeliveryError::Validation(error_meta(false, None))));
        }
        if request.context.len() > 4096 {
            return Ok(Err(DeliveryError::Validation(error_meta(false, None))));
        }
        let history_snippet = request.context.chars().take(48).collect::<String>();
        let holding_id = holding_id_from_session(&context);
        if let Some(pool) = context
            .extension::<crate::bff::BffState>()
            .and_then(|state| state.control_pool().cloned())
        {
            let db_row: Option<(String,)> = if let Some(holding_id) = holding_id {
                sqlx::query_as::<_, (String,)>(
                    r#"SELECT model FROM erpx_control.ai_provider_configs WHERE holding_id = $1"#,
                )
                .bind(holding_id)
                .fetch_optional(&pool)
                .await
                .unwrap_or(None)
            } else {
                sqlx::query_as::<_, (String,)>(
                    r#"SELECT model FROM erpx_control.ai_provider_configs LIMIT 1"#,
                )
                .fetch_optional(&pool)
                .await
                .unwrap_or(None)
            };
            if let Some(row) = db_row {
                let model = row.0;
                let mut suggestions = vec![
                    format!("{history_snippet} (via {} history)", model),
                    format!("Autofill {} via {}", request.field, model),
                    format!("Predicted {} ({} tuned)", request.field, model),
                ];
                let mut evidence = vec!["db:ai_provider_configs".to_owned()];
                if holding_id.is_some() {
                    evidence.push("db:ai_provider_configs:holding_scoped".to_owned());
                }
                let anomaly = if request.context.len() > 80
                    || request
                        .context
                        .chars()
                        .filter(|c| *c == '!' || *c == '$')
                        .count()
                        > 3
                {
                    Some("Anomaly: unusually long or noisy input — please verify")
                } else {
                    None
                };
                if let Some(warning) = anomaly {
                    suggestions.push(warning.to_owned());
                    evidence.push("anomaly:windowed_history".to_owned());
                }
                suggestions.truncate(3 + usize::from(anomaly.is_some()));
                return Ok(Ok(AiSuggestionResponse {
                    suggestions,
                    evidence_refs: evidence,
                }));
            }
        }
        let mut suggestions = vec![
            format!("{history_snippet} (recent)"),
            format!("Autofill {} from history", request.field),
            format!("Predicted {} (model)", request.field),
        ];
        let mut evidence = Vec::new();
        let anomaly = if request.context.len() > 80
            || request
                .context
                .chars()
                .filter(|c| *c == '!' || *c == '$')
                .count()
                > 3
        {
            Some("Anomaly: unusually long or noisy input — please verify")
        } else {
            None
        };
        if let Some(warning) = anomaly {
            suggestions.push(warning.to_owned());
            evidence.push("anomaly:windowed_history".to_owned());
        }
        suggestions.truncate(3 + usize::from(anomaly.is_some()));
        return Ok(Ok(AiSuggestionResponse {
            suggestions,
            evidence_refs: evidence,
        }));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/ai-flow")]
pub async fn ai_flow_analyze(
    request: FlowAnalysisRequest,
) -> dioxus::prelude::ServerFnResult<Result<FlowAnalysisResponse, DeliveryError>> {
    #[cfg(feature = "server")]
    {
        use dioxus::fullstack::FullstackContext;
        use dioxus::server::axum::http::{HeaderValue, header};

        let context = FullstackContext::current()
            .ok_or_else(|| dioxus::prelude::ServerFnError::new("request context unavailable"))?;
        context.add_response_header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
        let authenticated = context
            .extension::<crate::bff::BffSessionContext>()
            .is_some_and(|session| session.snapshot.authenticated);
        if !authenticated {
            return Ok(Err(DeliveryError::Unauthenticated(error_meta(false, None))));
        }
        if request.flow_id.trim().is_empty() || request.flow_id.len() > 64 {
            return Ok(Err(DeliveryError::Validation(error_meta(false, None))));
        }
        if request.prompt.len() > 4096 || request.context_refs.len() > 32 {
            return Ok(Err(DeliveryError::Validation(error_meta(false, None))));
        }
        let holding_id = match holding_id_from_session(&context) {
            Some(id) => id,
            None => return Ok(Err(DeliveryError::Forbidden(error_meta(false, None)))),
        };
        let has_flow = has_flow_ai_entitlement(holding_id);
        if !has_flow {
            return Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(250)))));
        }
        let summary = format!(
            "Proposed cross-department analysis for flow {} (holding {}) — {} context refs. Review before apply per ADR-017:106.",
            request.flow_id,
            holding_id,
            request.context_refs.len()
        );
        let recommendations = vec![
            "Review lead→quote conversion for drop-off".into(),
            "Verify procurement receipt matches PO".into(),
            "Confirm finance close readiness before apportion".into(),
        ];
        let evidence_refs = request.context_refs.clone();
        return Ok(Ok(FlowAnalysisResponse {
            summary,
            recommendations,
            evidence_refs,
        }));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/ai-presets")]
pub async fn ai_provider_presets()
-> dioxus::prelude::ServerFnResult<Result<Vec<AiProviderPresetView>, DeliveryError>> {
    #[cfg(feature = "server")]
    {
        let presets = erpx_ai_provider::AI_PROVIDER_PRESETS
            .iter()
            .map(|preset| AiProviderPresetView {
                kind: preset.kind,
                label: preset.label.to_owned(),
                default_endpoint: preset.default_endpoint.to_owned(),
                default_model: preset.default_model.to_owned(),
                free: preset.free,
            })
            .collect();
        return Ok(Ok(presets));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/hosted-quote")]
pub async fn hosted_quote(
    seats: u32,
) -> dioxus::prelude::ServerFnResult<Result<HostedPricingQuote, DeliveryError>> {
    #[cfg(feature = "server")]
    {
        use dioxus::fullstack::FullstackContext;

        if seats > 5000 {
            return Ok(Err(DeliveryError::Validation(error_meta(false, None))));
        }
        let mut effective_seats = seats;
        if effective_seats == 0
            && let Some(ctx) = FullstackContext::current()
            && let Some(pool) = ctx
                .extension::<crate::bff::BffState>()
                .and_then(|state| state.control_pool().cloned())
        {
            if let Some(holding_id) = holding_id_from_session(&ctx) {
                if let Some(count) = count_active_seats_for_holding(&pool, holding_id).await {
                    effective_seats = count;
                }
            } else if let Some(count) = count_active_seats_via_db(&pool).await {
                effective_seats = count;
            }
        }
        return Ok(Ok(erpx_delivery_contract::hosted_quote(effective_seats)));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/ai-provider-save")]
pub async fn save_ai_provider_config(
    request: SaveAiProviderConfigRequest,
) -> dioxus::prelude::ServerFnResult<Result<AiProviderConfigView, DeliveryError>> {
    #[cfg(feature = "server")]
    {
        use dioxus::fullstack::FullstackContext;
        use dioxus::server::axum::http::{HeaderValue, header};

        let context = FullstackContext::current()
            .ok_or_else(|| dioxus::prelude::ServerFnError::new("request context unavailable"))?;
        context.add_response_header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
        validate_mutation_request()?;
        let holding_id = match holding_id_from_session(&context) {
            Some(id) => id,
            None => return Ok(Err(DeliveryError::Forbidden(error_meta(false, None)))),
        };
        if request.endpoint_url.len() > 512
            || request.model.trim().is_empty()
            || request.model.len() > 128
            || request.api_key.trim().is_empty()
            || request.api_key.len() > 4096
        {
            return Ok(Err(DeliveryError::Validation(error_meta(false, None))));
        }
        let has_flow = has_flow_ai_entitlement(holding_id);
        let cfg = erpx_ai_provider::AiProviderConfig {
            holding_id,
            kind: request.kind,
            endpoint_url: request.endpoint_url.clone(),
            model: request.model.clone(),
            api_key_ref: request.api_key.clone(),
            per_module_override: request.per_module_override.clone(),
        };
        if cfg.validate(has_flow).is_err() {
            return Ok(Err(DeliveryError::Validation(error_meta(false, None))));
        }
        let key = ai_provider_encryption_key();
        let key_ref = std::env::var(erpx_ai_provider::AI_PROVIDER_KEY_REF_ENV)
            .unwrap_or_else(|_| "erpx-ai-provider".to_owned());
        let cipher =
            match erpx_ai_provider::encrypt_api_key(holding_id, &key_ref, &key, &request.api_key) {
                Ok(v) => v,
                Err(_) => return Ok(Err(DeliveryError::Validation(error_meta(false, None)))),
            };
        let view = AiProviderConfigView {
            kind: request.kind,
            endpoint_url: cfg.sanitized_endpoint(),
            model: request.model.clone(),
            per_module_override: request.per_module_override.clone(),
            updated_at: Some(chrono::Utc::now().to_rfc3339()),
        };
        store_ai_provider_config(holding_id, view.clone(), cipher.clone());
        if let Some(pool) = context
            .extension::<crate::bff::BffState>()
            .and_then(|state| state.control_pool().cloned())
        {
            let _ = db_save_ai_provider(&pool, holding_id, &view, &cipher).await;
        }
        return Ok(Ok(view));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/ai-provider-load")]
pub async fn load_ai_provider_config()
-> dioxus::prelude::ServerFnResult<Result<Option<AiProviderConfigView>, DeliveryError>> {
    #[cfg(feature = "server")]
    {
        use dioxus::fullstack::FullstackContext;
        use dioxus::server::axum::http::{HeaderValue, header};

        let context = FullstackContext::current()
            .ok_or_else(|| dioxus::prelude::ServerFnError::new("request context unavailable"))?;
        context.add_response_header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
        let authenticated = context
            .extension::<crate::bff::BffSessionContext>()
            .is_some_and(|session| session.snapshot.authenticated);
        if !authenticated {
            return Ok(Err(DeliveryError::Unauthenticated(error_meta(false, None))));
        }
        let holding_id = match holding_id_from_session(&context) {
            Some(id) => id,
            None => return Ok(Ok(None)),
        };
        if let Some(pool) = context
            .extension::<crate::bff::BffState>()
            .and_then(|state| state.control_pool().cloned())
            && let Ok(Some((view, cipher))) = db_load_ai_provider(&pool, holding_id).await
        {
            store_ai_provider_config(holding_id, view.clone(), cipher);
            return Ok(Ok(Some(view)));
        }
        let view = load_ai_provider_view(holding_id);
        return Ok(Ok(view));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/ai-provider-test")]
pub async fn test_ai_provider_connection(
    request: AiProviderTestRequest,
) -> dioxus::prelude::ServerFnResult<Result<AiProviderTestResponse, DeliveryError>> {
    #[cfg(feature = "server")]
    {
        use dioxus::fullstack::FullstackContext;
        use dioxus::server::axum::http::{HeaderValue, header};

        let context = FullstackContext::current()
            .ok_or_else(|| dioxus::prelude::ServerFnError::new("request context unavailable"))?;
        context.add_response_header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
        let authenticated = context
            .extension::<crate::bff::BffSessionContext>()
            .is_some_and(|session| session.snapshot.authenticated);
        if !authenticated {
            return Ok(Err(DeliveryError::Unauthenticated(error_meta(false, None))));
        }
        let dummy_holding = uuid::Uuid::from_u128(1);
        let cfg = erpx_ai_provider::AiProviderConfig {
            holding_id: dummy_holding,
            kind: request.kind,
            endpoint_url: request.endpoint_url.clone(),
            model: request.model.clone(),
            api_key_ref: request.api_key.clone(),
            per_module_override: None,
        };
        let has_flow = request.kind.is_free();
        if let Err(err) = cfg.validate(has_flow) {
            return Ok(Ok(AiProviderTestResponse {
                reachable: false,
                message: format!("validation failed: {err}"),
            }));
        }
        let url = cfg.chat_completions_url();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let probe = client
            .get(format!("{}/models", cfg.sanitized_endpoint()))
            .bearer_auth(&request.api_key)
            .send()
            .await;
        let (reachable, message) = match probe {
            Ok(resp) if resp.status().is_success() => {
                (true, "provider reachable: GET /models success".into())
            }
            Ok(resp) => (false, format!("provider responded with {}", resp.status())),
            Err(err) => (false, format!("probe failed (offline ok): {err}")),
        };
        let _ = url;
        return Ok(Ok(AiProviderTestResponse { reachable, message }));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[cfg(any(feature = "server", test))]
fn canonical_locale(locale: &str) -> String {
    if locale.eq_ignore_ascii_case("th") || locale.to_ascii_lowercase().starts_with("th-") {
        "th".to_owned()
    } else {
        "en".to_owned()
    }
}

#[cfg(any(feature = "server", test))]
fn locale_from_cookie_header(cookie_header: Option<&str>, fallback: &str) -> String {
    let locale = cookie_header
        .and_then(|cookies| {
            cookies.split(';').find_map(|part| {
                let (name, value) = part.trim().split_once('=')?;
                (name == "erpx_locale").then_some(value)
            })
        })
        .unwrap_or(fallback);
    canonical_locale(locale)
}

#[cfg(feature = "server")]
fn error_meta(retryable: bool, retry_after_ms: Option<u64>) -> DeliveryErrorMeta {
    DeliveryErrorMeta {
        correlation_id: uuid::Uuid::new_v4(),
        retry_after_ms,
        retryable,
    }
}

#[cfg(feature = "server")]
fn holding_id_from_session(context: &dioxus::fullstack::FullstackContext) -> Option<uuid::Uuid> {
    let session = context.extension::<crate::bff::BffSessionContext>()?;
    let scope = session.snapshot.active_role.as_ref()?.scope_key.as_ref()?;
    if let Ok(id) = uuid::Uuid::parse_str(scope)
        && !id.is_nil()
    {
        return Some(id);
    }
    if let Some(stripped) = scope.strip_prefix("holding:")
        && let Ok(id) = uuid::Uuid::parse_str(stripped)
        && !id.is_nil()
    {
        return Some(id);
    }
    None
}

#[cfg(feature = "server")]
fn has_flow_ai_entitlement(holding_id: uuid::Uuid) -> bool {
    if let Ok(list) = std::env::var("ERPX_FLOW_AI_HOLDINGS") {
        for part in list.split(',') {
            if let Ok(id) = uuid::Uuid::parse_str(part.trim())
                && id == holding_id
            {
                return true;
            }
        }
    }
    std::env::var("ERPX_FLOW_AI_ENABLED").as_deref() == Ok("1")
}

#[cfg(feature = "server")]
fn ai_provider_encryption_key() -> [u8; 32] {
    if let Ok(encoded) = std::env::var(erpx_ai_provider::AI_PROVIDER_KEY_REFERENCE_ENV)
        && let Ok(key) = erpx_ai_provider::decode_encryption_key(&encoded)
    {
        return key;
    }
    [7u8; 32]
}

#[cfg(feature = "server")]
#[allow(clippy::type_complexity)]
fn ai_provider_store() -> &'static std::sync::Arc<
    std::sync::Mutex<
        std::collections::HashMap<
            uuid::Uuid,
            (AiProviderConfigView, erpx_ai_provider::ApiKeyCipher),
        >,
    >,
> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    #[allow(clippy::type_complexity)]
    static STORE: OnceLock<
        Arc<Mutex<HashMap<uuid::Uuid, (AiProviderConfigView, erpx_ai_provider::ApiKeyCipher)>>>,
    > = OnceLock::new();
    STORE.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

#[cfg(feature = "server")]
fn store_ai_provider_config(
    holding_id: uuid::Uuid,
    view: AiProviderConfigView,
    cipher: erpx_ai_provider::ApiKeyCipher,
) {
    if let Ok(mut map) = ai_provider_store().lock() {
        map.insert(holding_id, (view, cipher));
    }
}

#[cfg(feature = "server")]
fn load_ai_provider_view(holding_id: uuid::Uuid) -> Option<AiProviderConfigView> {
    ai_provider_store()
        .lock()
        .ok()?
        .get(&holding_id)
        .map(|(view, _)| view.clone())
}

#[cfg(feature = "server")]
async fn db_save_ai_provider(
    pool: &sqlx::PgPool,
    holding_id: uuid::Uuid,
    view: &AiProviderConfigView,
    cipher: &erpx_ai_provider::ApiKeyCipher,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO erpx_control.ai_provider_configs
            (holding_id, kind, endpoint_url, model, api_key_cipher, api_key_nonce, key_reference, per_module_override)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (holding_id) DO UPDATE SET
            kind = EXCLUDED.kind,
            endpoint_url = EXCLUDED.endpoint_url,
            model = EXCLUDED.model,
            api_key_cipher = EXCLUDED.api_key_cipher,
            api_key_nonce = EXCLUDED.api_key_nonce,
            key_reference = EXCLUDED.key_reference,
            per_module_override = EXCLUDED.per_module_override,
            updated_at = now()
        "#,
    )
    .bind(holding_id)
    .bind(view.kind.slug())
    .bind(&view.endpoint_url)
    .bind(&view.model)
    .bind(&cipher.ciphertext)
    .bind(cipher.nonce.as_ref() as &[u8])
    .bind(&cipher.key_reference)
    .bind(&view.per_module_override)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(feature = "server")]
async fn db_load_ai_provider(
    pool: &sqlx::PgPool,
    holding_id: uuid::Uuid,
) -> Result<Option<(AiProviderConfigView, erpx_ai_provider::ApiKeyCipher)>, sqlx::Error> {
    let row = sqlx::query_as::<_, (String, String, String, Vec<u8>, Vec<u8>, String, Option<String>)>(
        r#"
        SELECT kind, endpoint_url, model, api_key_cipher, api_key_nonce, key_reference, per_module_override
        FROM erpx_control.ai_provider_configs WHERE holding_id = $1
        "#,
    )
    .bind(holding_id)
    .fetch_optional(pool)
    .await?;
    let Some((
        kind,
        endpoint_url,
        model,
        cipher_bytes,
        nonce_bytes,
        key_reference,
        per_module_override,
    )) = row
    else {
        return Ok(None);
    };
    let kind = match kind.as_str() {
        "openai" => erpx_ai_provider::AiProviderKind::OpenAi,
        "custom" => erpx_ai_provider::AiProviderKind::Custom,
        "anthropic" => erpx_ai_provider::AiProviderKind::Anthropic,
        "gemini" => erpx_ai_provider::AiProviderKind::Gemini,
        "azure" => erpx_ai_provider::AiProviderKind::Azure,
        "ollama" => erpx_ai_provider::AiProviderKind::Ollama,
        _ => return Ok(None),
    };
    if nonce_bytes.len() != 12 {
        return Ok(None);
    }
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&nonce_bytes);
    let view = AiProviderConfigView {
        kind,
        endpoint_url,
        model,
        per_module_override,
        updated_at: None,
    };
    let cipher = erpx_ai_provider::ApiKeyCipher::from_parts(nonce, cipher_bytes, key_reference);
    Ok(Some((view, cipher)))
}

#[cfg(feature = "server")]
async fn count_active_seats_via_db(pool: &sqlx::PgPool) -> Option<u32> {
    let row = sqlx::query_as::<_, (i64,)>(
        r#"SELECT COUNT(DISTINCT principal_ref) FROM erpx_control.role_assignments WHERE valid_until > now()"#,
    )
    .fetch_optional(pool)
    .await
    .ok()??;
    let count: i64 = row.0;
    u32::try_from(count).ok()
}

#[cfg(feature = "server")]
async fn count_active_seats_for_holding(
    pool: &sqlx::PgPool,
    holding_id: uuid::Uuid,
) -> Option<u32> {
    let holding_str = holding_id.hyphenated().to_string();
    let rows = sqlx::query_as::<_, (String, serde_json::Value)>(
        r#"SELECT principal_ref, payload FROM erpx_control.role_assignments WHERE valid_until > now()"#,
    )
    .fetch_all(pool)
    .await
    .ok()?;
    let mut principals = std::collections::BTreeSet::new();
    for (principal_ref, payload) in rows {
        if payload.to_string().contains(&holding_str) {
            principals.insert(principal_ref);
        }
    }
    Some(principals.len() as u32)
}

#[cfg(feature = "server")]
fn trusted_authority() -> Result<ValidatedAuthority, DeliveryError> {
    use dioxus::fullstack::FullstackContext;

    let context = FullstackContext::current()
        .ok_or_else(|| DeliveryError::Internal(error_meta(false, None)))?;
    let snapshot = context
        .extension::<crate::bff::BffSessionContext>()
        .map(|session| session.snapshot)
        .ok_or_else(|| DeliveryError::Unauthenticated(error_meta(false, None)))?;
    if !snapshot.authenticated {
        return Err(DeliveryError::Unauthenticated(error_meta(false, None)));
    }
    let role = snapshot
        .active_role
        .ok_or_else(|| DeliveryError::Forbidden(error_meta(false, None)))?;
    ValidatedAuthority::from_trusted_adapter(
        snapshot
            .principal_ref
            .ok_or_else(|| DeliveryError::Unauthenticated(error_meta(false, None)))?,
        role.scope_key
            .ok_or_else(|| DeliveryError::Forbidden(error_meta(false, None)))?,
        role.role_key
            .ok_or_else(|| DeliveryError::Forbidden(error_meta(false, None)))?,
    )
}

#[cfg(feature = "server")]
fn trusted_query_context(request: &QueryRequest) -> Result<QueryContext, DeliveryError> {
    QueryContext::from_trusted_adapter(
        trusted_authority()?,
        uuid::Uuid::new_v4(),
        request.cursor.clone(),
        request.limit,
        request.locale.clone(),
    )
}

#[cfg(feature = "server")]
fn trusted_command_context(request: &CommandRequest) -> Result<CommandContext, DeliveryError> {
    CommandContext::from_trusted_adapter(
        trusted_authority()?,
        uuid::Uuid::new_v4(),
        request.idempotency_key.clone(),
        request.expected_version,
        request.locale.clone(),
    )
}

#[cfg(feature = "server")]
fn mutation_headers_are_trusted(
    headers: &dioxus::server::axum::http::HeaderMap,
    allow_development_proxy: bool,
) -> bool {
    let origin = headers.get("origin").and_then(|value| value.to_str().ok());
    let host = headers.get("host").and_then(|value| value.to_str().ok());
    let csrf = headers
        .get("x-erpx-csrf")
        .and_then(|value| value.to_str().ok());
    let fetch_site = headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok());
    let exact_origin = match origin {
        Some(origin) => host.is_some_and(|host| {
            origin == format!("https://{host}") || origin == format!("http://{host}")
        }),
        None => host.is_some() && fetch_site == Some("same-origin"),
    };
    let trusted_development_proxy = allow_development_proxy
        && host.is_some()
        && fetch_site == Some("same-origin")
        && headers
            .get("x-proxied-by-dioxus")
            .and_then(|value| value.to_str().ok())
            == Some("true");
    let same_origin = exact_origin || trusted_development_proxy;
    same_origin && csrf == Some("console")
}

#[cfg(feature = "server")]
pub(crate) fn validate_mutation_request() -> dioxus::prelude::ServerFnResult<()> {
    use dioxus::fullstack::FullstackContext;

    let context = FullstackContext::current()
        .ok_or_else(|| dioxus::prelude::ServerFnError::new("request context unavailable"))?;
    let allow_development_proxy = context
        .extension::<crate::bff::BffState>()
        .is_some_and(|state| !state.production_profile());
    let parts = context.parts_mut();
    if !mutation_headers_are_trusted(&parts.headers, allow_development_proxy) {
        return Err(dioxus::prelude::ServerFnError::new(
            "mutation origin or CSRF proof is invalid",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "server")]
    #[test]
    fn mutation_header_policy_accepts_same_origin_fetch_metadata_and_rejects_cross_site() {
        use dioxus::server::axum::http::{HeaderMap, HeaderValue};

        let mut explicit = HeaderMap::new();
        explicit.insert("host", HeaderValue::from_static("console.example"));
        explicit.insert(
            "origin",
            HeaderValue::from_static("https://console.example"),
        );
        explicit.insert("x-erpx-csrf", HeaderValue::from_static("console"));
        assert!(mutation_headers_are_trusted(&explicit, false));

        let mut fetch_metadata = HeaderMap::new();
        fetch_metadata.insert("host", HeaderValue::from_static("localhost:3004"));
        fetch_metadata.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
        fetch_metadata.insert("x-erpx-csrf", HeaderValue::from_static("console"));
        assert!(mutation_headers_are_trusted(&fetch_metadata, false));

        fetch_metadata.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
        assert!(!mutation_headers_are_trusted(&fetch_metadata, false));
        fetch_metadata.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
        fetch_metadata.remove("x-erpx-csrf");
        assert!(!mutation_headers_are_trusted(&fetch_metadata, false));

        explicit.insert(
            "origin",
            HeaderValue::from_static("https://attacker.example"),
        );
        explicit.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
        assert!(!mutation_headers_are_trusted(&explicit, false));

        explicit.insert("x-proxied-by-dioxus", HeaderValue::from_static("true"));
        assert!(mutation_headers_are_trusted(&explicit, true));
        assert!(!mutation_headers_are_trusted(&explicit, false));
    }

    #[test]
    fn locale_is_allow_listed() {
        assert_eq!(canonical_locale("th-TH"), "th");
        assert_eq!(canonical_locale("en-US"), "en");
        assert_eq!(canonical_locale("../../secret"), "en");
        assert_eq!(
            locale_from_cookie_header(Some("theme=dark; erpx_locale=th"), "en"),
            "th"
        );
        assert_eq!(
            locale_from_cookie_header(Some("not_erpx_locale=th; erpx_locale=invalid"), "th"),
            "en"
        );
    }
}
