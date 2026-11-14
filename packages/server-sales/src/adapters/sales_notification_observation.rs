//! Bounded read-only Resend email observation for reconciliation.
//!
//! One direct HTTPS GET per command: no proxy, no redirect, no retry and one
//! response body bound. Any missing or ambiguous provider identity fails
//! closed; this adapter never sends, resends or rewrites a notice.
use crate::application::sales_notification_reconciliation::{
    ProviderObservationPort, ReconcileFuture,
};
use crate::domain::sales_notification_reconciliation::{
    ProviderEvent, ProviderObservation, provider_unconfirmed,
};
use crate::domain::sales_notification_execution::not_configured;
use erpx_server_shared::public::BusinessError;
use std::time::Duration;

const ENDPOINT: &str = "https://api.resend.com/emails";
const MAX_RESPONSE_BYTES: usize = 8_192;

#[derive(serde::Deserialize)]
struct WireObservation {
    object: String,
    id: String,
    to: Vec<String>,
    subject: String,
    text: String,
    created_at: String,
    last_event: String,
}

pub struct ResendEmailObservation {
    client: reqwest::Client,
    authorization: reqwest::header::HeaderValue,
}

impl ResendEmailObservation {
    fn build(key: String) -> Result<Self, BusinessError> {
        if key.len() < 16
            || key.len() > 4096
            || !key.is_ascii()
            || key
                .chars()
                .any(|value| value.is_whitespace() || value.is_control())
        {
            return Err(not_configured());
        }
        let roots =
            rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let tls = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|_| not_configured())?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let client = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .referer(false)
            .https_only(true)
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(3))
            .pool_max_idle_per_host(1)
            .build()
            .map_err(|_| not_configured())?;
        let mut authorization = reqwest::header::HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| not_configured())?;
        authorization.set_sensitive(true);
        Ok(Self {
            client,
            authorization,
        })
    }

    /// The reviewed runtime credential. A missing or invalid key is an explicit
    /// unconfigured port, never a silent TypeScript fallback.
    pub fn from_environment() -> Option<Self> {
        let key = std::env::var("RESEND_API_KEY").ok()?;
        Self::build(key).ok()
    }
}

impl ProviderObservationPort for ResendEmailObservation {
    fn observe<'a>(
        &'a self,
        provider_message_id: &'a str,
    ) -> ReconcileFuture<'a, ProviderObservation, BusinessError> {
        Box::pin(async move {
            if provider_message_id.is_empty()
                || provider_message_id.len() > 256
                || !provider_message_id
                    .chars()
                    .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
            {
                return Err(provider_unconfirmed());
            }
            let url = format!("{ENDPOINT}/{}", provider_message_id);
            let response = self
                .client
                .get(url)
                .header(reqwest::header::AUTHORIZATION, self.authorization.clone())
                .send()
                .await
                .map_err(|_| provider_unconfirmed())?;
            if !response.status().is_success() {
                return Err(provider_unconfirmed());
            }
            let body = response
                .text()
                .await
                .map_err(|_| provider_unconfirmed())?;
            if body.len() > MAX_RESPONSE_BYTES {
                return Err(provider_unconfirmed());
            }
            let wire: WireObservation =
                serde_json::from_str(&body).map_err(|_| provider_unconfirmed())?;
            if wire.object != "email" {
                return Err(provider_unconfirmed());
            }
            let event = ProviderEvent::parse(&wire.last_event).ok_or_else(provider_unconfirmed)?;
            let created = time::OffsetDateTime::parse(
                &wire.created_at,
                &time::format_description::well_known::Rfc3339,
            )
            .map_err(|_| provider_unconfirmed())?;
            let created_at = i64::try_from(created.unix_timestamp_nanos() / 1_000_000)
                .map_err(|_| provider_unconfirmed())?;
            Ok(ProviderObservation {
                id: wire.id,
                to: wire.to,
                subject: wire.subject,
                text: wire.text,
                created_at,
                last_event: event,
            })
        })
    }
}

/// Fail-closed observation port: no provider credential means reconciliation
/// cannot be confirmed and the durable attempt stays uncertain.
pub struct UnconfiguredObservation;

impl ProviderObservationPort for UnconfiguredObservation {
    fn observe<'a>(
        &'a self,
        _: &'a str,
    ) -> ReconcileFuture<'a, ProviderObservation, BusinessError> {
        Box::pin(async { Err(not_configured()) })
    }
}

/// Composition-time selection, mirroring the delivery worker.
pub fn configured_observation() -> Box<dyn ProviderObservationPort> {
    match ResendEmailObservation::from_environment() {
        Some(port) => Box::new(port),
        None => Box::new(UnconfiguredObservation),
    }
}
