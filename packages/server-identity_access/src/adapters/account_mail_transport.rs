//! Explicit provider adapters. Construction never reads environment variables,
//! opens connections, or sends mail. Test sinks are literal loopback sockets.
use crate::{
    application::account_mail::{AccountMailProvider, AccountMailSendFuture},
    domain::account_mail::*,
};
use erpx_server_shared::public::BusinessError;
use sha2::{Digest, Sha256};
use std::{net::SocketAddr, time::Duration};
fn validate_from(value: &str) -> Result<(), BusinessError> {
    if value.is_empty() || value.len() > 998 || value.chars().any(char::is_control) {
        return Err(account_mail_unavailable());
    }
    let address = if let Some((name, address)) = value.split_once('<') {
        if name.contains('>') || address.contains('<') {
            return Err(account_mail_unavailable());
        }
        address
            .strip_suffix('>')
            .ok_or_else(account_mail_unavailable)?
    } else {
        value
    };
    crate::domain::account_auth::EmailAddress::new(address)
        .map_err(|_| account_mail_unavailable())?;
    Ok(())
}

fn scope(name: &str, values: &[&str]) -> Result<String, BusinessError> {
    if name.is_empty()
        || name.len() > 100
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return Err(account_mail_unavailable());
    }
    let mut hash = Sha256::new();
    for value in std::iter::once(name).chain(values.iter().copied()) {
        hash.update(
            u64::try_from(value.len())
                .map_err(|_| account_mail_unavailable())?
                .to_be_bytes(),
        );
        hash.update(value.as_bytes());
    }
    Ok(hex::encode(hash.finalize()))
}
fn sink(address: SocketAddr) -> Result<(), BusinessError> {
    if !address.ip().is_loopback()
        || address.port() < 1024
        || matches!(
            address.port(),
            3000 | 3001 | 8080 | 3100 | 3101 | 3103 | 5432
        )
    {
        return Err(account_mail_unavailable());
    }
    Ok(())
}

#[cfg(feature = "mail-resend")]
pub struct ResendAccountMail {
    client: reqwest::Client,
    endpoint: url::Url,
    authorization: reqwest::header::HeaderValue,
    profile: AccountMailProviderProfile,
}
#[cfg(feature = "mail-resend")]
impl ResendAccountMail {
    pub fn new(profile_name: &str, from: String, api_key: String) -> Result<Self, BusinessError> {
        Self::build(
            profile_name,
            from,
            api_key,
            url::Url::parse("https://api.resend.com/emails")
                .map_err(|_| account_mail_unavailable())?,
            false,
        )
    }
    /// Explicit local fixture transport; never selected by runtime configuration.
    pub fn for_local_test_sink(
        profile_name: &str,
        from: String,
        synthetic_key: String,
        address: SocketAddr,
    ) -> Result<Self, BusinessError> {
        sink(address)?;
        Self::build(
            profile_name,
            from,
            synthetic_key,
            url::Url::parse(&format!("http://{address}/emails"))
                .map_err(|_| account_mail_unavailable())?,
            true,
        )
    }
    fn build(
        name: &str,
        from: String,
        key: String,
        endpoint: url::Url,
        local: bool,
    ) -> Result<Self, BusinessError> {
        validate_from(&from)?;
        if key.len() < 16
            || key.len() > 4096
            || !key.is_ascii()
            || key.chars().any(|c| c.is_whitespace() || c.is_control())
            || from.is_empty()
            || from.len() > 998
            || from.chars().any(|c| c.is_control())
        {
            return Err(account_mail_unavailable());
        }
        let profile = AccountMailProviderProfile {
            kind: AccountMailProviderKind::Resend,
            profile_id: scope(name, &["resend", endpoint.as_str(), &key])?,
            from,
        };
        let roots =
            rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let tls = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|_| account_mail_unavailable())?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let client = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .referer(false)
            .https_only(!local)
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(3))
            .pool_max_idle_per_host(1)
            .build()
            .map_err(|_| account_mail_unavailable())?;
        let mut authorization = reqwest::header::HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| account_mail_unavailable())?;
        authorization.set_sensitive(true);
        Ok(Self {
            client,
            endpoint,
            authorization,
            profile,
        })
    }
}
#[cfg(feature = "mail-resend")]
impl AccountMailProvider for ResendAccountMail {
    fn profile(&self) -> &AccountMailProviderProfile {
        &self.profile
    }
    fn send<'a>(&'a self, message: &'a AccountEmailMessage) -> AccountMailSendFuture<'a> {
        Box::pin(async move {
            let request=self.client.post(self.endpoint.clone()).header(reqwest::header::AUTHORIZATION,self.authorization.clone()).header("Idempotency-Key",message.delivery_key()).json(&serde_json::json!({"from":self.profile.from,"to":[message.recipient()],"subject":message.subject(),"text":message.body()}));
            let mut response = match request.send().await {
                Ok(response) => response,
                Err(error) => {
                    return if error.is_connect() {
                        AccountMailSendOutcome::Retryable(AccountMailFailure::ProviderUnavailable)
                    } else {
                        AccountMailSendOutcome::Uncertain(AccountMailFailure::DeliveryUncertain)
                    };
                }
            };
            let status = response.status();
            if status.as_u16() == 429 {
                return AccountMailSendOutcome::Retryable(AccountMailFailure::ProviderRateLimited);
            }
            if status.is_client_error() && !matches!(status.as_u16(), 408 | 409) {
                return AccountMailSendOutcome::Rejected(AccountMailFailure::ProviderRejected);
            }
            if !status.is_success() {
                return AccountMailSendOutcome::Uncertain(AccountMailFailure::DeliveryUncertain);
            }
            let mut body = Vec::new();
            loop {
                match response.chunk().await {
                    Ok(Some(chunk)) if body.len() + chunk.len() <= 8192 => {
                        body.extend_from_slice(&chunk)
                    }
                    Ok(None) => break,
                    _ => {
                        return AccountMailSendOutcome::Uncertain(
                            AccountMailFailure::DeliveryUncertain,
                        );
                    }
                }
            }
            #[derive(serde::Deserialize)]
            struct Receipt {
                id: String,
            }
            let Ok(receipt) = serde_json::from_slice::<Receipt>(&body) else {
                return AccountMailSendOutcome::Uncertain(AccountMailFailure::DeliveryUncertain);
            };
            let valid = receipt.id.len() == 36
                && receipt.id.bytes().enumerate().all(|(index, byte)| {
                    if matches!(index, 8 | 13 | 18 | 23) {
                        byte == b'-'
                    } else {
                        byte.is_ascii_hexdigit()
                    }
                });
            if !valid {
                return AccountMailSendOutcome::Uncertain(AccountMailFailure::DeliveryUncertain);
            }
            AccountMailSendOutcome::Accepted {
                provider_id: receipt.id,
            }
        })
    }
}

#[cfg(feature = "mail-smtp")]
#[derive(Clone, Copy)]
pub enum AccountSmtpTls {
    Implicit,
    RequiredStartTls,
}
#[cfg(feature = "mail-smtp")]
pub struct SmtpAccountMail {
    transport: lettre::AsyncSmtpTransport<lettre::Tokio1Executor>,
    from: lettre::message::Mailbox,
    profile: AccountMailProviderProfile,
}
#[cfg(feature = "mail-smtp")]
impl SmtpAccountMail {
    pub fn new(
        profile_name: &str,
        from: String,
        server: String,
        port: u16,
        mode: AccountSmtpTls,
        username: String,
        password: String,
    ) -> Result<Self, BusinessError> {
        validate_from(&from)?;
        if server.is_empty()
            || server.len() > 253
            || server.chars().any(|c| c.is_whitespace() || c.is_control())
            || url::Host::parse(&server).is_err()
            || port == 0
            || username.is_empty()
            || username.len() > 4096
            || password.is_empty()
            || password.len() > 4096
            || username.chars().any(char::is_control)
            || password.chars().any(char::is_control)
        {
            return Err(account_mail_unavailable());
        }
        let tls = match mode {
            AccountSmtpTls::Implicit => "implicit",
            AccountSmtpTls::RequiredStartTls => "required-starttls",
        };
        let profile = AccountMailProviderProfile {
            kind: AccountMailProviderKind::Smtp,
            profile_id: scope(
                profile_name,
                &[
                    "smtp",
                    &server,
                    &port.to_string(),
                    tls,
                    &username,
                    &password,
                ],
            )?,
            from: from.clone(),
        };
        let builder = match mode {
            AccountSmtpTls::Implicit => {
                lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::relay(&server)
            }
            AccountSmtpTls::RequiredStartTls => {
                lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::starttls_relay(&server)
            }
        }
        .map_err(|_| account_mail_unavailable())?;
        let transport = builder
            .port(port)
            .timeout(Some(Duration::from_secs(10)))
            .credentials(lettre::transport::smtp::authentication::Credentials::new(
                username, password,
            ))
            .build();
        Ok(Self {
            transport,
            from: from.parse().map_err(|_| account_mail_unavailable())?,
            profile,
        })
    }
    /// No plaintext network fallback exists in the production SMTP constructor.
    pub fn for_local_test_sink(
        profile_name: &str,
        from: String,
        address: SocketAddr,
    ) -> Result<Self, BusinessError> {
        validate_from(&from)?;
        sink(address)?;
        let profile = AccountMailProviderProfile {
            kind: AccountMailProviderKind::Smtp,
            profile_id: scope(
                profile_name,
                &["smtp-local-synthetic", &address.to_string()],
            )?,
            from: from.clone(),
        };
        let transport = lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::builder_dangerous(
            address.ip().to_string(),
        )
        .port(address.port())
        .timeout(Some(Duration::from_secs(3)))
        .build();
        Ok(Self {
            transport,
            from: from.parse().map_err(|_| account_mail_unavailable())?,
            profile,
        })
    }
}
#[cfg(feature = "mail-smtp")]
impl AccountMailProvider for SmtpAccountMail {
    fn profile(&self) -> &AccountMailProviderProfile {
        &self.profile
    }
    fn send<'a>(&'a self, message: &'a AccountEmailMessage) -> AccountMailSendFuture<'a> {
        use lettre::AsyncTransport;
        Box::pin(async move {
            let id = format!(
                "<{}@{}>",
                hex::encode(Sha256::digest(message.delivery_key().as_bytes())),
                self.from.email.domain()
            );
            let recipient = match message.recipient().parse::<lettre::message::Mailbox>() {
                Ok(value) => value,
                Err(_) => {
                    return AccountMailSendOutcome::Rejected(AccountMailFailure::InvalidPayload);
                }
            };
            let email = match lettre::Message::builder()
                .from(self.from.clone())
                .to(recipient)
                .subject(message.subject())
                .message_id(Some(id.clone()))
                .header(lettre::message::header::ContentType::TEXT_PLAIN)
                .body(message.body().to_owned())
            {
                Ok(email) => email,
                Err(_) => {
                    return AccountMailSendOutcome::Rejected(AccountMailFailure::InvalidPayload);
                }
            };
            match self.transport.send(email).await {
                Ok(response) if response.is_positive() => AccountMailSendOutcome::Accepted {
                    provider_id: format!("smtp:{id}"),
                },
                Ok(_) => AccountMailSendOutcome::Uncertain(AccountMailFailure::DeliveryUncertain),
                Err(error) if error.is_transient() => {
                    AccountMailSendOutcome::Retryable(AccountMailFailure::ProviderUnavailable)
                }
                Err(error) if error.is_permanent() => {
                    AccountMailSendOutcome::Rejected(AccountMailFailure::ProviderRejected)
                }
                Err(_) => AccountMailSendOutcome::Uncertain(AccountMailFailure::DeliveryUncertain),
            }
        })
    }
}
