//! The single reviewed Customer Portal invitation email adapter.
//!
//! Live delivery uses Resend with `Idempotency-Key` = the invitation delivery
//! key. Sandbox delivery keeps the retained sensitive-email receipt proof: the
//! neutral control store receives the redacted body plus an AES-256-GCM
//! ciphertext bound to the delivery key, recipient, subject and provider id,
//! and the worker re-reads and decrypts it to prove the exact message it
//! intended. No weaker "accepted" receipt is possible.
use crate::domain::portal_invitation_delivery::{
    EMAIL_DELIVERY_FAILED, EMAIL_IDEMPOTENCY_CONFLICT, EMAIL_NOT_CONFIGURED,
    EMAIL_SANDBOX_FORBIDDEN_IN_PRODUCTION, EMAIL_SANDBOX_NOT_CONFIGURED,
    EMAIL_SANDBOX_RECEIPT_MISSING, EMAIL_SENSITIVE_RECEIPT_INVALID,
};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use ring::{aead, hkdf, rand};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

const ENDPOINT: &str = "https://api.resend.com/emails";
const REDACTED_BODY: &str = "[redacted sensitive email body]";
const SALT: &[u8] = b"erpx:sandbox-email-body:hkdf-salt:v1";
const INFO: &[u8] = b"erpx:manage_erpx:sandbox-email-body:aes-256-gcm:v1";
const AAD_PREFIX: &str = "erpx:sandbox-email-body:v1";

pub type PortalEmailFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// One private enrollment message. `text` is the body the neutral email owner
/// wraps with its subject and anti-phishing footer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortalEmail {
    pub delivery_key: String,
    pub recipient: String,
    pub subject: String,
    pub text: String,
}

pub trait PortalInvitationEmailPort: Send + Sync {
    fn send<'a>(&'a self, email: PortalEmail) -> PortalEmailFuture<'a, ()>;
}

fn delivery_failed() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        EMAIL_DELIVERY_FAILED,
        "The invitation email provider did not accept this message.",
    )
}

fn not_configured() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        EMAIL_NOT_CONFIGURED,
        "Configure the invitation email provider before retrying.",
    )
}

fn sensitive_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        EMAIL_SENSITIVE_RECEIPT_INVALID,
        "The stored sensitive email receipt could not be verified.",
    )
}

fn encryption_key(secret: Option<&str>) -> Result<[u8; 32], BusinessError> {
    let secret = secret.filter(|value| !value.is_empty()).ok_or_else(|| {
        // Deliberately outside the retained safe-code whitelist: the worker
        // records EMAIL_DELIVERY_FAILED, exactly like the retained adapter.
        BusinessError::new(
            FailureKind::Unavailable,
            "EMAIL_SENSITIVE_ENCRYPTION_NOT_CONFIGURED",
            "Sensitive sandbox email encryption is not configured.",
        )
    })?;
    let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, SALT);
    let prk = salt.extract(secret.as_bytes());
    struct Length(usize);
    impl hkdf::KeyType for Length {
        fn len(&self) -> usize {
            self.0
        }
    }
    let okm = prk
        .expand(&[INFO], Length(32))
        .map_err(|_| sensitive_invalid())?;
    let mut key = [0_u8; 32];
    okm.fill(&mut key).map_err(|_| sensitive_invalid())?;
    Ok(key)
}

fn aad(email: &PortalEmail, provider_id: &str) -> Vec<u8> {
    serde_json::json!([
        AAD_PREFIX,
        email.delivery_key,
        email.recipient,
        email.subject,
        provider_id
    ])
    .to_string()
    .into_bytes()
}

fn encrypt_body(
    body: &str,
    secret: Option<&str>,
    email: &PortalEmail,
    provider_id: &str,
) -> Result<String, BusinessError> {
    let key = encryption_key(secret)?;
    let unbound =
        aead::UnboundKey::new(&aead::AES_256_GCM, &key).map_err(|_| sensitive_invalid())?;
    let key = aead::LessSafeKey::new(unbound);
    let mut nonce_bytes = [0_u8; 12];
    rand::SecureRandom::fill(&rand::SystemRandom::new(), &mut nonce_bytes)
        .map_err(|_| sensitive_invalid())?;
    let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);
    let mut sealed = body.as_bytes().to_vec();
    key.seal_in_place_append_tag(nonce, aead::Aad::from(aad(email, provider_id)), &mut sealed)
        .map_err(|_| sensitive_invalid())?;
    let tag = sealed.split_off(sealed.len() - 16);
    Ok(format!(
        "v1.{}.{}.{}",
        URL_SAFE_NO_PAD.encode(nonce_bytes),
        URL_SAFE_NO_PAD.encode(&tag),
        URL_SAFE_NO_PAD.encode(&sealed)
    ))
}

fn decode_base64url(value: &str) -> Result<Vec<u8>, BusinessError> {
    let decoded = URL_SAFE_NO_PAD
        .decode(value.as_bytes())
        .map_err(|_| sensitive_invalid())?;
    if decoded.is_empty() && !value.is_empty() {
        return Err(sensitive_invalid());
    }
    if URL_SAFE_NO_PAD.encode(&decoded) != value {
        return Err(sensitive_invalid());
    }
    Ok(decoded)
}

fn decrypt_body(
    envelope: &str,
    secret: Option<&str>,
    delivery_key: &str,
    recipient: &str,
    subject: &str,
    provider_id: &str,
) -> Result<String, BusinessError> {
    let key = encryption_key(secret)?;
    let parts: Vec<&str> = envelope.split('.').collect();
    let (Some("v1"), 4) = (parts.first().copied(), parts.len()) else {
        return Err(sensitive_invalid());
    };
    let nonce_bytes = decode_base64url(parts[1])?;
    let tag = decode_base64url(parts[2])?;
    let ciphertext = decode_base64url(parts[3])?;
    if nonce_bytes.len() != 12 || tag.len() != 16 {
        return Err(sensitive_invalid());
    }
    let unbound =
        aead::UnboundKey::new(&aead::AES_256_GCM, &key).map_err(|_| sensitive_invalid())?;
    let key = aead::LessSafeKey::new(unbound);
    let nonce = <[u8; 12]>::try_from(nonce_bytes.as_slice())
        .map(aead::Nonce::assume_unique_for_key)
        .map_err(|_| sensitive_invalid())?;
    let mut sealed = ciphertext;
    sealed.extend_from_slice(&tag);
    let aad = serde_json::json!([AAD_PREFIX, delivery_key, recipient, subject, provider_id])
        .to_string()
        .into_bytes();
    let plain = key
        .open_in_place(nonce, aead::Aad::from(aad), &mut sealed)
        .map_err(|_| sensitive_invalid())?;
    String::from_utf8(plain.to_vec()).map_err(|_| sensitive_invalid())
}

/// Retained sandbox adapter: redacted neutral store plus encrypted body proof.
pub struct SandboxPortalInvitationEmail {
    control: Option<DatabasePool>,
    secret: Option<String>,
    production: bool,
}

impl SandboxPortalInvitationEmail {
    pub fn new(control: Option<DatabasePool>, secret: Option<String>, production: bool) -> Self {
        Self {
            control,
            secret,
            production,
        }
    }
}

impl PortalInvitationEmailPort for SandboxPortalInvitationEmail {
    fn send<'a>(&'a self, email: PortalEmail) -> PortalEmailFuture<'a, ()> {
        Box::pin(async move {
            if self.production {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    EMAIL_SANDBOX_FORBIDDEN_IN_PRODUCTION,
                    "The sandbox email adapter is forbidden in production.",
                ));
            }
            let Some(control) = self.control.as_ref() else {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    EMAIL_SANDBOX_NOT_CONFIGURED,
                    "The sandbox email store is not configured.",
                ));
            };
            let provider_id = format!("sandbox-{}", email.delivery_key);
            let ciphertext =
                encrypt_body(&email.text, self.secret.as_deref(), &email, &provider_id)?;
            let id = format!("sandbox-email-{}", uuid::Uuid::new_v4());
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
                .unwrap_or(0);
            sqlx::query(
                "INSERT INTO sandbox_email_deliveries\
                 (id,delivery_key,recipient,subject,body_text,body_ciphertext,provider_id,created_at,metadata_json) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(delivery_key) DO NOTHING",
            )
            .bind(&id)
            .bind(&email.delivery_key)
            .bind(&email.recipient)
            .bind(&email.subject)
            .bind(REDACTED_BODY)
            .bind(&ciphertext)
            .bind(&provider_id)
            .bind(now)
            .bind(serde_json::json!({"adapter":"sandbox"}).to_string())
            .execute(control.raw())
            .await
            .map_err(|_| delivery_failed())?;
            let stored = sqlx::query(
                "SELECT delivery_key,recipient,subject,body_text,body_ciphertext,provider_id \
                 FROM sandbox_email_deliveries WHERE delivery_key=$1",
            )
            .bind(&email.delivery_key)
            .fetch_optional(control.raw())
            .await
            .map_err(|_| delivery_failed())?;
            let Some(stored) = stored else {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    EMAIL_SANDBOX_RECEIPT_MISSING,
                    "The sandbox email receipt is missing.",
                ));
            };
            use sqlx::Row;
            let recipient: String = stored.try_get("recipient").map_err(|_| delivery_failed())?;
            let subject: String = stored.try_get("subject").map_err(|_| delivery_failed())?;
            let body_text: String = stored.try_get("body_text").map_err(|_| delivery_failed())?;
            let envelope: Option<String> = stored
                .try_get("body_ciphertext")
                .map_err(|_| delivery_failed())?;
            let stored_provider: String = stored
                .try_get("provider_id")
                .map_err(|_| delivery_failed())?;
            let delivery_key: String = stored
                .try_get("delivery_key")
                .map_err(|_| delivery_failed())?;
            let redacted = body_text == REDACTED_BODY;
            let plain = match envelope {
                None => body_text,
                Some(envelope) => decrypt_body(
                    &envelope,
                    self.secret.as_deref(),
                    &delivery_key,
                    &recipient,
                    &subject,
                    &stored_provider,
                )?,
            };
            if recipient != email.recipient
                || subject != email.subject
                || plain != email.text
                || !redacted
            {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    EMAIL_IDEMPOTENCY_CONFLICT,
                    "The stored sandbox email receipt does not match this message.",
                ));
            }
            Ok(())
        })
    }
}

/// Live Resend adapter with the retained idempotency key.
pub struct ResendPortalInvitationEmail {
    client: reqwest::Client,
    from: String,
    authorization: reqwest::header::HeaderValue,
}

impl ResendPortalInvitationEmail {
    pub fn new(from: String, key: String) -> Result<Self, BusinessError> {
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
}

impl PortalInvitationEmailPort for ResendPortalInvitationEmail {
    fn send<'a>(&'a self, email: PortalEmail) -> PortalEmailFuture<'a, ()> {
        Box::pin(async move {
            let request = self
                .client
                .post(ENDPOINT)
                .header(reqwest::header::AUTHORIZATION, self.authorization.clone())
                .header("Idempotency-Key", email.delivery_key.as_str())
                .json(&serde_json::json!({
                    "from": self.from,
                    "to": [email.recipient],
                    "subject": email.subject,
                    "text": email.text,
                }));
            let response = request.send().await.map_err(|_| delivery_failed())?;
            if !response.status().is_success() {
                return Err(delivery_failed());
            }
            Ok(())
        })
    }
}

/// Fail-closed port: no live provider is configured.
pub struct UnconfiguredPortalInvitationEmail;

impl PortalInvitationEmailPort for UnconfiguredPortalInvitationEmail {
    fn send<'a>(&'a self, _email: PortalEmail) -> PortalEmailFuture<'a, ()> {
        Box::pin(async move { Err(not_configured()) })
    }
}
