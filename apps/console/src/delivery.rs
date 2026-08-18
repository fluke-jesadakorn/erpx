use async_trait::async_trait;
use dioxus::prelude::dioxus_fullstack;
#[cfg(feature = "server")]
use dioxus::prelude::dioxus_server;
use erpx_delivery_contract::{
    AuthenticationRequestV2, BootstrapRequestV2, CollaborationEventV2, CollaborationSubscriptionV2,
    CommandReceiptV2, CommandRequestV2, ConsoleGateway, DeliveryErrorV2,
    PublicMarketingFormRequestV2, PublicMarketingPreviewRequestV2, QueryRequestV2, RouteDataV2,
    SessionBootstrapV2,
};
#[cfg(feature = "server")]
use erpx_delivery_contract::{
    ClientChannelV2, CommandContextV2, DELIVERY_CONTRACT_VERSION, DeliveryErrorMetaV2,
    QueryContextV2, ValidatedAuthorityV2,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct ServerFunctionGatewayV2;

#[async_trait(?Send)]
impl ConsoleGateway for ServerFunctionGatewayV2 {
    async fn bootstrap(
        &self,
        request: BootstrapRequestV2,
    ) -> Result<SessionBootstrapV2, DeliveryErrorV2> {
        session_bootstrap_v2(request.locale)
            .await
            .map_err(|_| DeliveryErrorV2::unavailable(None))
    }

    async fn query(&self, request: QueryRequestV2) -> Result<RouteDataV2, DeliveryErrorV2> {
        query_route_v2(request)
            .await
            .map_err(|_| DeliveryErrorV2::unavailable(None))?
    }

    async fn command(
        &self,
        request: CommandRequestV2,
    ) -> Result<CommandReceiptV2, DeliveryErrorV2> {
        command_v2(request)
            .await
            .map_err(|_| DeliveryErrorV2::unavailable(None))?
    }

    async fn authenticate(
        &self,
        request: AuthenticationRequestV2,
    ) -> Result<SessionBootstrapV2, DeliveryErrorV2> {
        authenticate_v2(request)
            .await
            .map_err(|_| DeliveryErrorV2::unavailable(None))?
    }

    async fn submit_public_marketing_form(
        &self,
        request: PublicMarketingFormRequestV2,
    ) -> Result<CommandReceiptV2, DeliveryErrorV2> {
        submit_public_marketing_form_v2(request)
            .await
            .map_err(|_| DeliveryErrorV2::unavailable(None))?
    }

    async fn public_marketing_preview(
        &self,
        request: PublicMarketingPreviewRequestV2,
    ) -> Result<RouteDataV2, DeliveryErrorV2> {
        public_marketing_preview_v2(request)
            .await
            .map_err(|_| DeliveryErrorV2::unavailable(None))?
    }

    async fn collaboration_stream(
        &self,
        request: CollaborationSubscriptionV2,
    ) -> Result<Vec<CollaborationEventV2>, DeliveryErrorV2> {
        collaboration_events_v2(request)
            .await
            .map_err(|_| DeliveryErrorV2::unavailable(None))?
    }
}

#[dioxus::prelude::server(endpoint = "/api/v2/server-fn/bootstrap")]
pub async fn session_bootstrap_v2(
    locale: String,
) -> dioxus::prelude::ServerFnResult<SessionBootstrapV2> {
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
        let active_role = snapshot.active_role.as_ref();
        return Ok(SessionBootstrapV2 {
            authenticated: snapshot.authenticated,
            actor_ref: snapshot.principal_ref,
            tenant_ref: active_role.and_then(|role| role.scope_key.clone()),
            role_ref: active_role.and_then(|role| role.role_key.clone()),
            locale: canonical_locale(&locale),
            channel: ClientChannelV2::Web,
            catalog_version: DELIVERY_CONTRACT_VERSION,
        });
    }

    #[allow(unreachable_code)]
    Ok(SessionBootstrapV2 {
        authenticated: false,
        actor_ref: None,
        tenant_ref: None,
        role_ref: None,
        locale: canonical_locale(&locale),
        channel: ClientChannelV2::Web,
        catalog_version: DELIVERY_CONTRACT_VERSION,
    })
}

#[dioxus::prelude::server(endpoint = "/api/v2/server-fn/query")]
pub async fn query_route_v2(
    request: QueryRequestV2,
) -> dioxus::prelude::ServerFnResult<Result<RouteDataV2, DeliveryErrorV2>> {
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
            return Ok(Err(DeliveryErrorV2::Unauthenticated(error_meta(
                false, None,
            ))));
        }
        if request.limit == 0 || request.limit > erpx_delivery_contract::MAX_PAGE_SIZE_V2 {
            return Ok(Err(DeliveryErrorV2::Validation(error_meta(false, None))));
        }
        let _trusted_context = match trusted_query_context(&request) {
            Ok(context) => context,
            Err(error) => return Ok(Err(error)),
        };
        return Ok(Ok(crate::routes::route_data_for_query(&request)));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryErrorV2::Unavailable(error_meta(
        true,
        Some(500),
    ))))
}

#[dioxus::prelude::server(endpoint = "/api/v2/server-fn/command")]
pub async fn command_v2(
    request: CommandRequestV2,
) -> dioxus::prelude::ServerFnResult<Result<CommandReceiptV2, DeliveryErrorV2>> {
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
        return Ok(Err(DeliveryErrorV2::Unavailable(error_meta(
            true,
            Some(250),
        ))));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryErrorV2::Unavailable(error_meta(
        true,
        Some(500),
    ))))
}

#[dioxus::prelude::server(endpoint = "/api/v2/server-fn/authenticate")]
pub async fn authenticate_v2(
    request: AuthenticationRequestV2,
) -> dioxus::prelude::ServerFnResult<Result<SessionBootstrapV2, DeliveryErrorV2>> {
    #[cfg(feature = "server")]
    {
        validate_mutation_request()?;
        let canonical_return = crate::validated_login_return_path(&request.return_path);
        if request.account.trim().is_empty()
            || request.proof.is_empty()
            || canonical_return.as_deref() != Some(request.return_path.as_str())
        {
            return Ok(Err(DeliveryErrorV2::Validation(error_meta(false, None))));
        }
        // Credential verification remains owned by the existing identity
        // application service. The v2 façade fails closed until that typed
        // adapter has installed an authenticated session in this request.
        return Ok(Err(DeliveryErrorV2::Unauthenticated(error_meta(
            false, None,
        ))));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryErrorV2::Unavailable(error_meta(
        true,
        Some(500),
    ))))
}

#[dioxus::prelude::server(endpoint = "/api/v2/server-fn/public-form")]
pub async fn submit_public_marketing_form_v2(
    request: PublicMarketingFormRequestV2,
) -> dioxus::prelude::ServerFnResult<Result<CommandReceiptV2, DeliveryErrorV2>> {
    #[cfg(feature = "server")]
    {
        validate_mutation_request()?;
        if !request.consent || request.idempotency_key.is_empty() {
            return Ok(Err(DeliveryErrorV2::Validation(error_meta(false, None))));
        }
        return Ok(Err(DeliveryErrorV2::Unavailable(error_meta(
            true,
            Some(250),
        ))));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryErrorV2::Unavailable(error_meta(
        true,
        Some(500),
    ))))
}

#[dioxus::prelude::server(endpoint = "/api/v2/server-fn/public-preview")]
pub async fn public_marketing_preview_v2(
    request: PublicMarketingPreviewRequestV2,
) -> dioxus::prelude::ServerFnResult<Result<RouteDataV2, DeliveryErrorV2>> {
    #[cfg(feature = "server")]
    {
        use dioxus::fullstack::FullstackContext;
        use dioxus::server::axum::http::{HeaderName, HeaderValue, header};

        let context = FullstackContext::current()
            .ok_or_else(|| dioxus::prelude::ServerFnError::new("request context unavailable"))?;
        context.add_response_header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=60, stale-while-revalidate=300"),
        );
        context.add_response_header(
            HeaderName::from_static("vary"),
            HeaderValue::from_static("Accept-Language"),
        );
        let data = crate::routes::public_preview_data(&request);
        if let Some(validator) = &data.public_validator
            && let Ok(value) = HeaderValue::from_str(validator)
        {
            context.add_response_header(header::ETAG, value);
        }
        return Ok(Ok(data));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryErrorV2::Unavailable(error_meta(
        true,
        Some(500),
    ))))
}

#[dioxus::prelude::server(endpoint = "/api/v2/server-fn/collaboration")]
pub async fn collaboration_events_v2(
    _request: CollaborationSubscriptionV2,
) -> dioxus::prelude::ServerFnResult<Result<Vec<CollaborationEventV2>, DeliveryErrorV2>> {
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
            return Ok(Err(DeliveryErrorV2::Unauthenticated(error_meta(
                false, None,
            ))));
        }
        return Ok(Ok(Vec::new()));
    }

    #[allow(unreachable_code)]
    Ok(Err(DeliveryErrorV2::Unavailable(error_meta(
        true,
        Some(500),
    ))))
}

#[cfg(any(feature = "server", test))]
fn canonical_locale(locale: &str) -> String {
    if locale.eq_ignore_ascii_case("th") || locale.to_ascii_lowercase().starts_with("th-") {
        "th".to_owned()
    } else {
        "en".to_owned()
    }
}

#[cfg(feature = "server")]
fn error_meta(retryable: bool, retry_after_ms: Option<u64>) -> DeliveryErrorMetaV2 {
    DeliveryErrorMetaV2 {
        correlation_id: uuid::Uuid::new_v4(),
        retry_after_ms,
        retryable,
    }
}

#[cfg(feature = "server")]
fn trusted_authority() -> Result<ValidatedAuthorityV2, DeliveryErrorV2> {
    use dioxus::fullstack::FullstackContext;

    let context = FullstackContext::current()
        .ok_or_else(|| DeliveryErrorV2::Internal(error_meta(false, None)))?;
    let snapshot = context
        .extension::<crate::bff::BffSessionContext>()
        .map(|session| session.snapshot)
        .ok_or_else(|| DeliveryErrorV2::Unauthenticated(error_meta(false, None)))?;
    if !snapshot.authenticated {
        return Err(DeliveryErrorV2::Unauthenticated(error_meta(false, None)));
    }
    let role = snapshot
        .active_role
        .ok_or_else(|| DeliveryErrorV2::Forbidden(error_meta(false, None)))?;
    ValidatedAuthorityV2::from_trusted_adapter(
        snapshot
            .principal_ref
            .ok_or_else(|| DeliveryErrorV2::Unauthenticated(error_meta(false, None)))?,
        role.scope_key
            .ok_or_else(|| DeliveryErrorV2::Forbidden(error_meta(false, None)))?,
        role.role_key
            .ok_or_else(|| DeliveryErrorV2::Forbidden(error_meta(false, None)))?,
    )
}

#[cfg(feature = "server")]
fn trusted_query_context(request: &QueryRequestV2) -> Result<QueryContextV2, DeliveryErrorV2> {
    QueryContextV2::from_trusted_adapter(
        trusted_authority()?,
        uuid::Uuid::new_v4(),
        request.cursor.clone(),
        request.limit,
        request.locale.clone(),
    )
}

#[cfg(feature = "server")]
fn trusted_command_context(
    request: &CommandRequestV2,
) -> Result<CommandContextV2, DeliveryErrorV2> {
    CommandContextV2::from_trusted_adapter(
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
    if !origin_matches || csrf != Some("v2") {
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
    }
}
