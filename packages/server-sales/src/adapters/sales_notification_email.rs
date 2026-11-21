//! The single reviewed customer-email provider adapter.
//!
//! Only the Sales delivery worker constructs this port. It owns no database
//! row, no message type and no retry: an ambiguous provider response is
//! returned as `Uncertain` so the durable attempt stays reconciliation work.
use crate::application::sales_notification_execution::{
    DispatchOutcome, DispatchPlan, EmailDispatchPort, ExecutionFuture, OutboundEmail,
};
use crate::domain::sales_notification_execution::not_configured;
use erpx_server_shared::public::BusinessError;
use std::time::Duration;

const ENDPOINT: &str = "https://api.resend.com/emails";
const MAX_RESPONSE_BYTES: usize = 8_192;

#[derive(serde::Deserialize)]
struct Receipt {
    id: String,
}

pub struct ResendEmailDispatch {
    client: reqwest::Client,
    from: String,
    authorization: reqwest::header::HeaderValue,
}

impl ResendEmailDispatch {
    fn build(from: String, key: String) -> Result<Self, BusinessError> {
        if from.trim().is_empty() || from.len() > 998 || from.chars().any(char::is_control) {
            return Err(not_configured());
        }
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
            from,
            authorization,
        })
    }

    /// The reviewed runtime configuration. A missing or invalid credential is
    /// an explicit unconfigured port, never a silent TypeScript fallback.
    pub fn from_environment() -> Option<Self> {
        let from = std::env::var("RESEND_FROM").ok()?;
        let key = std::env::var("RESEND_API_KEY").ok()?;
        Self::build(from, key).ok()
    }
}

impl EmailDispatchPort for ResendEmailDispatch {
    fn prepare(&self, plan: DispatchPlan) -> Result<(), BusinessError> {
        match plan {
            DispatchPlan::Sandbox => Ok(()),
            DispatchPlan::Resend => Ok(()),
        }
    }

    fn dispatch<'a>(
        &'a self,
        email: OutboundEmail<'a>,
    ) -> ExecutionFuture<'a, DispatchOutcome, BusinessError> {
        Box::pin(async move {
            let request = self
                .client
                .post(ENDPOINT)
                .header(reqwest::header::AUTHORIZATION, self.authorization.clone())
                .header("Idempotency-Key", email.notification_id)
                .json(&serde_json::json!({
                    "from": self.from,
                    "to": [email.recipient_email],
                    "subject": email.subject,
                    "text": email.body_text,
                }));
            let Ok(response) = request.send().await else {
                return Ok(DispatchOutcome::Uncertain);
            };
            if !response.status().is_success() {
                return Ok(DispatchOutcome::Uncertain);
            }
            let Ok(body) = response.text().await else {
                return Ok(DispatchOutcome::Uncertain);
            };
            if body.len() > MAX_RESPONSE_BYTES {
                return Ok(DispatchOutcome::Uncertain);
            }
            let Ok(receipt) = serde_json::from_str::<Receipt>(&body) else {
                return Ok(DispatchOutcome::Uncertain);
            };
            if receipt.id.trim().is_empty() {
                return Ok(DispatchOutcome::Uncertain);
            }
            Ok(DispatchOutcome::Accepted {
                provider_id: receipt.id,
            })
        })
    }
}

/// Fail-closed port: no live provider is configured, so no notice is sent and
/// the durable attempt records `CUSTOMER_NOTIFICATION_AUTHORIZATION_REQUIRED`
/// exactly like the retained worker's pre-dispatch failure.
pub struct UnconfiguredEmailDispatch;

impl EmailDispatchPort for UnconfiguredEmailDispatch {
    fn prepare(&self, plan: DispatchPlan) -> Result<(), BusinessError> {
        match plan {
            DispatchPlan::Sandbox => Ok(()),
            DispatchPlan::Resend => Err(not_configured()),
        }
    }

    fn dispatch<'a>(
        &'a self,
        _email: OutboundEmail<'a>,
    ) -> ExecutionFuture<'a, DispatchOutcome, BusinessError> {
        Box::pin(async move { Err(not_configured()) })
    }
}
