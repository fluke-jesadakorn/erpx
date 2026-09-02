use dioxus::prelude::dioxus_fullstack;
#[cfg(feature = "server")]
use dioxus::prelude::dioxus_server;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SupportListStatus {
    pub connected: bool,
    pub reason: String,
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/support-list-status")]
pub async fn support_list_status(
    locale: String,
) -> dioxus::prelude::ServerFnResult<SupportListStatus> {
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
            .is_some_and(|s| s.snapshot.authenticated);
        if !authenticated {
            return Ok(SupportListStatus {
                connected: false,
                reason: "unauthenticated".into(),
            });
        }
        let _ = locale;
        if let Some(pool) = context
            .extension::<crate::bff::BffState>()
            .and_then(|state| state.control_pool().cloned())
        {
            let ok = sqlx::query_scalar::<_, i64>(
                r#"SELECT 1 FROM erpx_control.support_tickets LIMIT 1"#,
            )
            .fetch_optional(&pool)
            .await
            .is_ok();
            if ok {
                return Ok(SupportListStatus {
                    connected: true,
                    reason: "connected via control: support_tickets".into(),
                });
            }
        }
        return Ok(SupportListStatus {
            connected: false,
            reason: "support projection unavailable".into(),
        });
    }
    #[allow(unreachable_code)]
    Ok(SupportListStatus {
        connected: false,
        reason: "unavailable".into(),
    })
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/incident-list-status")]
pub async fn incident_list_status(
    locale: String,
) -> dioxus::prelude::ServerFnResult<SupportListStatus> {
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
            .is_some_and(|s| s.snapshot.authenticated);
        if !authenticated {
            return Ok(SupportListStatus {
                connected: false,
                reason: "unauthenticated".into(),
            });
        }
        let _ = locale;
        if let Some(pool) = context
            .extension::<crate::bff::BffState>()
            .and_then(|state| state.control_pool().cloned())
        {
            let ok =
                sqlx::query_scalar::<_, i64>(r#"SELECT 1 FROM erpx_control.incidents LIMIT 1"#)
                    .fetch_optional(&pool)
                    .await
                    .is_ok();
            if ok {
                return Ok(SupportListStatus {
                    connected: true,
                    reason: "connected via control: incidents".into(),
                });
            }
        }
        return Ok(SupportListStatus {
            connected: false,
            reason: "incident projection unavailable".into(),
        });
    }
    #[allow(unreachable_code)]
    Ok(SupportListStatus {
        connected: false,
        reason: "unavailable".into(),
    })
}
