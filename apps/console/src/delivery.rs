use async_trait::async_trait;
use dioxus::prelude::dioxus_fullstack;
#[cfg(feature = "server")]
use dioxus::prelude::dioxus_server;
use erpx_delivery_contract::{
    AuthenticationRequest, BootstrapRequest, CollaborationEvent, CollaborationSubscription,
    CommandReceipt, CommandRequest, ConsoleGateway, DeliveryError, PublicMarketingFormRequest,
    PublicMarketingPreviewRequest, QueryRequest, RouteData, SessionBootstrap,
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
        validate_mutation_request()?;
        let _trusted_context = match trusted_command_context(&request) {
            Ok(context) => context,
            Err(error) => return Ok(Err(error)),
        };
        // Domain-specific adapters are selected by the typed operation. Until
        // an operation is registered, fail closed instead of accepting an
        // arbitrary protobuf or JSON envelope.
        return Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(250)))));
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
        validate_mutation_request()?;
        let canonical_return = crate::validated_login_return_path(&request.return_path);
        if request.account.trim().is_empty()
            || request.proof.is_empty()
            || canonical_return.as_deref() != Some(request.return_path.as_str())
        {
            return Ok(Err(DeliveryError::Validation(error_meta(false, None))));
        }
        // Credential verification remains owned by the existing identity
        // application service. The current façade fails closed until that typed
        // adapter has installed an authenticated session in this request.
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
    _request: CollaborationSubscription,
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
        return Ok(Ok(Vec::new()));
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
fn validate_mutation_request() -> dioxus::prelude::ServerFnResult<()> {
    use dioxus::fullstack::FullstackContext;

    let context = FullstackContext::current()
        .ok_or_else(|| dioxus::prelude::ServerFnError::new("request context unavailable"))?;
    let parts = context.parts_mut();
    let origin = parts
        .headers
        .get("origin")
        .and_then(|value| value.to_str().ok());
    let host = parts
        .headers
        .get("host")
        .and_then(|value| value.to_str().ok());
    let csrf = parts
        .headers
        .get("x-erpx-csrf")
        .and_then(|value| value.to_str().ok());
    let origin_matches = origin.zip(host).is_some_and(|(origin, host)| {
        origin == format!("https://{host}") || origin == format!("http://{host}")
    });
    if !origin_matches || csrf != Some("console") {
        return Err(dioxus::prelude::ServerFnError::new(
            "mutation origin or CSRF proof is invalid",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
