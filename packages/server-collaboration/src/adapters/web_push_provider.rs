//! RFC 8291 aes128gcm, RFC 8292 VAPID and the existing Node AES-256-GCM
//! subscription envelope. Reuses Ring already needed by native Account/TLS.
//! Secrets, endpoints and prepared authorization deliberately have no Debug.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use erpx_server_shared::public::{BusinessError, FailureKind};
use ring::{
    aead, agreement, hmac,
    rand::{SecureRandom, SystemRandom},
    signature::{self, KeyPair},
};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
type Error = BusinessError;
fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "PUSH_PROVIDER_UNAVAILABLE",
        "Push delivery is unavailable.",
    )
}
fn invalid() -> Error {
    BusinessError::invalid(
        "INVALID_PUSH_SUBSCRIPTION",
        "This browser's push service is not supported.",
    )
}
fn decode(value: &str, max: usize) -> Result<Vec<u8>, Error> {
    if value.len() > max {
        return Err(invalid());
    }
    B64.decode(value).map_err(|_| invalid())
}
fn agree(key: agreement::EphemeralPrivateKey, peer: &[u8]) -> Result<Vec<u8>, Error> {
    agreement::agree_ephemeral(
        key,
        &agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, peer),
        |shared| shared.to_vec(),
    )
    .map_err(|_| invalid())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PushSubscriptionKeys {
    pub p256dh: String,
    pub auth: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PushSubscription {
    pub endpoint: String,
    pub keys: PushSubscriptionKeys,
}
impl PushSubscription {
    pub fn validate(&self) -> Result<url::Url, Error> {
        let u = url::Url::parse(&self.endpoint).map_err(|_| invalid())?;
        let host = u.host_str().ok_or_else(invalid)?;
        let known = matches!(
            host,
            "fcm.googleapis.com" | "updates.push.services.mozilla.com" | "web.push.apple.com"
        ) || host.ends_with(".notify.windows.com");
        let authority = self.endpoint.split('/').nth(2).unwrap_or("");
        if self.endpoint.len() > 2048
            || u.scheme() != "https"
            || !known
            || !u.username().is_empty()
            || u.password().is_some()
            || u.port().is_some()
            || authority.contains(':')
            || u.fragment().is_some()
            || decode(&self.keys.auth, 80)?.len() != 16
        {
            return Err(invalid());
        }
        let receiver = decode(&self.keys.p256dh, 160)?;
        if receiver.len() != 65 || receiver[0] != 4 {
            return Err(invalid());
        }
        // Ring validates the complete P-256 point during agreement, including the
        // point-at-infinity and off-curve cases, before accepting stored metadata.
        agree(
            agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &SystemRandom::new())
                .map_err(|_| unavailable())?,
            &receiver,
        )?;
        Ok(u)
    }
}
fn hmac(key: &[u8], value: &[u8]) -> Vec<u8> {
    hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), value)
        .as_ref()
        .to_vec()
}
fn expand(prk: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    let mut value = info.to_vec();
    value.push(1);
    let mut out = hmac(prk, &value);
    out.truncate(len);
    out
}
/// Content-encryption boundary after validated P-256 agreement. Explicit inputs
/// support RFC vectors; normal preparation always generates fresh key and salt.
pub fn encrypt_message(
    payload: &[u8],
    receiver: &[u8],
    auth: &[u8],
    sender_public: &[u8],
    shared: &[u8],
    salt: &[u8],
) -> Result<Vec<u8>, Error> {
    if payload.len() > 3993
        || receiver.len() != 65
        || receiver[0] != 4
        || sender_public.len() != 65
        || sender_public[0] != 4
        || shared.len() != 32
        || auth.len() != 16
        || salt.len() != 16
    {
        return Err(invalid());
    }
    let mut info = b"WebPush: info\0".to_vec();
    info.extend_from_slice(receiver);
    info.extend_from_slice(sender_public);
    let ikm = expand(&hmac(auth, shared), &info, 32);
    let prk = hmac(salt, &ikm);
    let cek = expand(&prk, b"Content-Encoding: aes128gcm\0", 16);
    let nonce: [u8; 12] = expand(&prk, b"Content-Encoding: nonce\0", 12)
        .try_into()
        .map_err(|_| unavailable())?;
    let key = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_128_GCM, &cek).map_err(|_| unavailable())?,
    );
    let mut encrypted = payload.to_vec();
    encrypted.push(2);
    key.seal_in_place_append_tag(
        aead::Nonce::assume_unique_for_key(nonce),
        aead::Aad::empty(),
        &mut encrypted,
    )
    .map_err(|_| unavailable())?;
    let mut body = salt.to_vec();
    body.extend_from_slice(&4096u32.to_be_bytes());
    body.push(65);
    body.extend_from_slice(sender_public);
    body.extend(encrypted);
    Ok(body)
}
pub struct PreparedPush {
    endpoint: url::Url,
    authorization: String,
    body: Vec<u8>,
}
impl PreparedPush {
    /// Trusted boundary; never log endpoint or authorization.
    pub fn parts(&self) -> (&url::Url, &str, &[u8]) {
        (&self.endpoint, &self.authorization, &self.body)
    }
}
pub struct NativePushProvider {
    public: String,
    key: signature::EcdsaKeyPair,
    subject: String,
    data_key: [u8; 32],
    client: reqwest::Client,
}
impl NativePushProvider {
    pub fn new(public: &str, private: &str, subject: &str, data_key: &str) -> Result<Self, Error> {
        if subject.len() > 2048
            || subject.chars().any(char::is_control)
            || !(subject.starts_with("mailto:") || subject.starts_with("https://"))
        {
            return Err(unavailable());
        }
        let key = signature::EcdsaKeyPair::from_private_key_and_public_key(
            &signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            &decode(private, 80)?,
            &decode(public, 160)?,
            &SystemRandom::new(),
        )
        .map_err(|_| unavailable())?;
        let data_key: [u8; 32] = decode(data_key, 80)?
            .try_into()
            .map_err(|_| unavailable())?;
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|_| unavailable())?
        .with_root_certificates(rustls::RootCertStore::from_iter(
            webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
        ))
        .with_no_client_auth();
        let client = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .referer(false)
            .https_only(true)
            .timeout(Duration::from_secs(8))
            .connect_timeout(Duration::from_secs(3))
            .pool_max_idle_per_host(1)
            .build()
            .map_err(|_| unavailable())?;
        Ok(Self {
            public: public.into(),
            key,
            subject: subject.into(),
            data_key,
            client,
        })
    }
    pub fn prepare(
        &self,
        sealed: &str,
        payload: &[u8],
        now_ms: i64,
    ) -> Result<PreparedPush, Error> {
        if !(0..=9_007_199_254_740_991).contains(&now_ms) {
            return Err(unavailable());
        }
        let bytes = decode(sealed, 16384)?;
        if bytes.len() < 28 {
            return Err(invalid());
        }
        let key = aead::LessSafeKey::new(
            aead::UnboundKey::new(&aead::AES_256_GCM, &self.data_key).map_err(|_| unavailable())?,
        );
        let mut cipher = bytes[28..].to_vec();
        cipher.extend_from_slice(&bytes[12..28]);
        let nonce: [u8; 12] = bytes[..12].try_into().map_err(|_| invalid())?;
        let clear = key
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::empty(),
                &mut cipher,
            )
            .map_err(|_| invalid())?;
        let subscription: PushSubscription =
            serde_json::from_slice(clear).map_err(|_| invalid())?;
        let endpoint = subscription.validate()?;
        let ephemeral =
            agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &SystemRandom::new())
                .map_err(|_| unavailable())?;
        let public = ephemeral.compute_public_key().map_err(|_| unavailable())?;
        let shared = agree(ephemeral, &decode(&subscription.keys.p256dh, 160)?)?;
        let mut salt = [0u8; 16];
        SystemRandom::new()
            .fill(&mut salt)
            .map_err(|_| unavailable())?;
        let body = encrypt_message(
            payload,
            &decode(&subscription.keys.p256dh, 160)?,
            &decode(&subscription.keys.auth, 80)?,
            public.as_ref(),
            &shared,
            &salt,
        )?;
        let header = B64.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims=B64.encode(serde_json::to_vec(&serde_json::json!({"aud":endpoint.origin().ascii_serialization(),"exp":now_ms/1000+12*60*60,"sub":self.subject})).map_err(|_|unavailable())?);
        let token = format!("{header}.{claims}");
        let signature = self
            .key
            .sign(&SystemRandom::new(), token.as_bytes())
            .map_err(|_| unavailable())?;
        debug_assert_eq!(self.key.public_key().as_ref().len(), 65);
        Ok(PreparedPush {
            endpoint,
            authorization: format!(
                "vapid t={token}.{}, k={}",
                B64.encode(signature.as_ref()),
                self.public
            ),
            body,
        })
    }
    pub async fn send(&self, request: PreparedPush) -> Result<u16, Error> {
        let mut authorization = reqwest::header::HeaderValue::from_str(&request.authorization)
            .map_err(|_| unavailable())?;
        authorization.set_sensitive(true);
        let response = self
            .client
            .post(request.endpoint)
            .header("Authorization", authorization)
            .header("TTL", "300")
            .header("Urgency", "normal")
            .header("Content-Encoding", "aes128gcm")
            .header("Content-Type", "application/octet-stream")
            .body(request.body)
            .send()
            .await
            .map_err(|_| unavailable())?;
        Ok(response.status().as_u16())
    }
}
