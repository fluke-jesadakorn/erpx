use std::{fmt, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use webauthn_rs::prelude::{
    CreationChallengeResponse, Passkey, PasskeyAuthentication, PasskeyRegistration,
    PublicKeyCredential, RegisterPublicKeyCredential, RequestChallengeResponse, Url, Webauthn,
    WebauthnBuilder,
};

use crate::{ChallengeCeremony, ChallengeMaterial, IdentityError, WebAuthnChallenge};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationRequest {
    pub rp_id: String,
    pub origin: String,
    pub credential_id: Vec<u8>,
    pub public_key: Vec<u8>,
    pub sign_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionRequest {
    pub rp_id: String,
    pub origin: String,
    pub credential_id: Vec<u8>,
    pub sign_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedRegistration {
    pub credential_id: Vec<u8>,
    pub public_key: Vec<u8>,
    pub sign_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionVerification {
    pub credential_id: Vec<u8>,
    pub sign_count: u32,
}

/// Browser registration data that must be supplied to the real provider.
/// `state` is opaque server-side provider state and must not be accepted from
/// an untrusted client without a server-side lookup.
#[derive(Clone, PartialEq, Eq)]
pub struct BrowserRegistrationRequest {
    pub rp_id: String,
    pub origin: String,
    pub credential_id: Vec<u8>,
    pub response: Vec<u8>,
    pub state: Vec<u8>,
}

/// Browser assertion data that must be supplied to the real provider.
/// `state` is opaque server-side provider state and must not be accepted from
/// an untrusted client without a server-side lookup.
#[derive(Clone, PartialEq, Eq)]
pub struct BrowserAssertionRequest {
    pub rp_id: String,
    pub origin: String,
    pub credential_id: Vec<u8>,
    pub response: Vec<u8>,
    pub state: Vec<u8>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserRegistrationStart {
    options: Vec<u8>,
    state: Vec<u8>,
}

impl BrowserRegistrationStart {
    pub fn options(&self) -> &[u8] {
        &self.options
    }

    pub fn state(&self) -> &[u8] {
        &self.state
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserAuthenticationStart {
    options: Vec<u8>,
    state: Vec<u8>,
}

impl BrowserAuthenticationStart {
    pub fn options(&self) -> &[u8] {
        &self.options
    }

    pub fn state(&self) -> &[u8] {
        &self.state
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum BrowserCeremonyKind {
    Registration,
    Authentication,
}

#[allow(dead_code)]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BrowserProviderState {
    kind: BrowserCeremonyKind,
    provider_state: Vec<u8>,
    erpx_challenge_digest: [u8; 32],
}

#[allow(dead_code)]
const MAX_BROWSER_RESPONSE_BYTES: usize = 512 * 1024;
#[allow(dead_code)]
const MAX_PROVIDER_STATE_BYTES: usize = 128 * 1024;

pub trait WebAuthnProvider: Send + Sync {
    fn verify_registration(
        &self,
        request: &RegistrationRequest,
    ) -> Result<VerifiedRegistration, IdentityError>;
    fn verify_assertion(
        &self,
        credential: &PasskeyCredential,
        request: &AssertionRequest,
    ) -> Result<AssertionVerification, IdentityError>;

    /// Legacy request shape intentionally has no browser ceremony payload.
    /// Real providers must override the browser-specific methods below.
    fn verify_browser_registration(
        &self,
        _principal_ref: &str,
        _challenge: &WebAuthnChallenge,
        _material: &ChallengeMaterial,
        _request: &BrowserRegistrationRequest,
        _at: DateTime<Utc>,
    ) -> Result<VerifiedRegistration, IdentityError> {
        Err(IdentityError::WebAuthnProviderUnavailable)
    }

    fn verify_browser_assertion(
        &self,
        _principal_ref: &str,
        _credential: &PasskeyCredential,
        _challenge: &WebAuthnChallenge,
        _material: &ChallengeMaterial,
        _request: &BrowserAssertionRequest,
        _at: DateTime<Utc>,
    ) -> Result<AssertionVerification, IdentityError> {
        Err(IdentityError::WebAuthnProviderUnavailable)
    }

    fn start_browser_registration(
        &self,
        _user_unique_id: Uuid,
        _user_name: &str,
        _user_display_name: &str,
        _erpx_material: &[u8],
        _exclude_credentials: &[Vec<u8>],
    ) -> Result<BrowserRegistrationStart, IdentityError> {
        Err(IdentityError::WebAuthnProviderUnavailable)
    }

    fn start_browser_authentication(
        &self,
        _credential: &PasskeyCredential,
        _erpx_material: &[u8],
    ) -> Result<BrowserAuthenticationStart, IdentityError> {
        Err(IdentityError::WebAuthnProviderUnavailable)
    }
}

impl<P> WebAuthnProvider for &P
where
    P: WebAuthnProvider + ?Sized,
{
    fn verify_registration(
        &self,
        request: &RegistrationRequest,
    ) -> Result<VerifiedRegistration, IdentityError> {
        (*self).verify_registration(request)
    }

    fn verify_assertion(
        &self,
        credential: &PasskeyCredential,
        request: &AssertionRequest,
    ) -> Result<AssertionVerification, IdentityError> {
        (*self).verify_assertion(credential, request)
    }

    fn verify_browser_registration(
        &self,
        principal_ref: &str,
        challenge: &WebAuthnChallenge,
        material: &ChallengeMaterial,
        request: &BrowserRegistrationRequest,
        at: DateTime<Utc>,
    ) -> Result<VerifiedRegistration, IdentityError> {
        (*self).verify_browser_registration(principal_ref, challenge, material, request, at)
    }

    fn verify_browser_assertion(
        &self,
        principal_ref: &str,
        credential: &PasskeyCredential,
        challenge: &WebAuthnChallenge,
        material: &ChallengeMaterial,
        request: &BrowserAssertionRequest,
        at: DateTime<Utc>,
    ) -> Result<AssertionVerification, IdentityError> {
        (*self).verify_browser_assertion(
            principal_ref,
            credential,
            challenge,
            material,
            request,
            at,
        )
    }

    fn start_browser_registration(
        &self,
        user_unique_id: Uuid,
        user_name: &str,
        user_display_name: &str,
        erpx_material: &[u8],
        exclude_credentials: &[Vec<u8>],
    ) -> Result<BrowserRegistrationStart, IdentityError> {
        (*self).start_browser_registration(
            user_unique_id,
            user_name,
            user_display_name,
            erpx_material,
            exclude_credentials,
        )
    }

    fn start_browser_authentication(
        &self,
        credential: &PasskeyCredential,
        erpx_material: &[u8],
    ) -> Result<BrowserAuthenticationStart, IdentityError> {
        (*self).start_browser_authentication(credential, erpx_material)
    }
}

impl<P> WebAuthnProvider for Arc<P>
where
    P: WebAuthnProvider + ?Sized,
{
    fn verify_registration(
        &self,
        request: &RegistrationRequest,
    ) -> Result<VerifiedRegistration, IdentityError> {
        (**self).verify_registration(request)
    }

    fn verify_assertion(
        &self,
        credential: &PasskeyCredential,
        request: &AssertionRequest,
    ) -> Result<AssertionVerification, IdentityError> {
        (**self).verify_assertion(credential, request)
    }

    fn verify_browser_registration(
        &self,
        principal_ref: &str,
        challenge: &WebAuthnChallenge,
        material: &ChallengeMaterial,
        request: &BrowserRegistrationRequest,
        at: DateTime<Utc>,
    ) -> Result<VerifiedRegistration, IdentityError> {
        (**self).verify_browser_registration(principal_ref, challenge, material, request, at)
    }

    fn verify_browser_assertion(
        &self,
        principal_ref: &str,
        credential: &PasskeyCredential,
        challenge: &WebAuthnChallenge,
        material: &ChallengeMaterial,
        request: &BrowserAssertionRequest,
        at: DateTime<Utc>,
    ) -> Result<AssertionVerification, IdentityError> {
        (**self).verify_browser_assertion(
            principal_ref,
            credential,
            challenge,
            material,
            request,
            at,
        )
    }

    fn start_browser_registration(
        &self,
        user_unique_id: Uuid,
        user_name: &str,
        user_display_name: &str,
        erpx_material: &[u8],
        exclude_credentials: &[Vec<u8>],
    ) -> Result<BrowserRegistrationStart, IdentityError> {
        (**self).start_browser_registration(
            user_unique_id,
            user_name,
            user_display_name,
            erpx_material,
            exclude_credentials,
        )
    }

    fn start_browser_authentication(
        &self,
        credential: &PasskeyCredential,
        erpx_material: &[u8],
    ) -> Result<BrowserAuthenticationStart, IdentityError> {
        (**self).start_browser_authentication(credential, erpx_material)
    }
}

/// RP and browser-origin policy for a provider implementation.
///
/// This validates the request context before delegating to a real WebAuthn
/// library. It does not perform browser ceremony parsing or cryptographic
/// verification itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebAuthnProviderConfig {
    rp_id: String,
    allowed_origins: Vec<String>,
}

impl WebAuthnProviderConfig {
    pub fn try_new<I, S>(
        rp_id: impl Into<String>,
        allowed_origins: I,
    ) -> Result<Self, IdentityError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let rp_id = canonical_rp_id(&rp_id.into())?;
        let mut origins = Vec::new();

        for origin in allowed_origins {
            let origin = origin.into();
            let (canonical_origin, host) = canonical_origin(&origin)?;
            if !origin_host_matches_rp(&host, &rp_id) {
                return Err(IdentityError::WebAuthnOriginInvalid);
            }
            if !origins.contains(&canonical_origin) {
                origins.push(canonical_origin);
            }
        }

        if origins.is_empty() {
            return Err(IdentityError::WebAuthnConfigurationInvalid);
        }

        Ok(Self {
            rp_id,
            allowed_origins: origins,
        })
    }

    pub fn rp_id(&self) -> &str {
        &self.rp_id
    }

    pub fn allowed_origins(&self) -> &[String] {
        &self.allowed_origins
    }

    fn validate(&self) -> Result<(), IdentityError> {
        let configured_rp_id = canonical_rp_id(&self.rp_id)?;
        if configured_rp_id != self.rp_id || self.allowed_origins.is_empty() {
            return Err(IdentityError::WebAuthnConfigurationInvalid);
        }
        for origin in &self.allowed_origins {
            let (canonical, host) = canonical_origin(origin)?;
            if canonical != *origin || !origin_host_matches_rp(&host, &self.rp_id) {
                return Err(IdentityError::WebAuthnConfigurationInvalid);
            }
        }
        Ok(())
    }

    pub fn validate_request_context(&self, rp_id: &str, origin: &str) -> Result<(), IdentityError> {
        self.validate()?;
        let request_rp_id = canonical_rp_id(rp_id)?;
        if request_rp_id != self.rp_id {
            return Err(IdentityError::WebAuthnRpIdMismatch);
        }
        let (request_origin, _) = canonical_origin(origin)?;
        if !self.allowed_origins.contains(&request_origin) {
            return Err(IdentityError::WebAuthnOriginNotAllowed);
        }
        Ok(())
    }
}

/// Validates deployment context and then delegates ceremony verification to
/// the configured provider. A live provider must still parse ceremony data
/// and perform WebAuthn cryptographic verification behind this seam.
#[derive(Debug, Clone)]
pub struct ConfiguredWebAuthnProvider<P> {
    provider: P,
    config: WebAuthnProviderConfig,
}

impl<P> ConfiguredWebAuthnProvider<P> {
    pub fn try_new(provider: P, config: WebAuthnProviderConfig) -> Result<Self, IdentityError> {
        config.validate()?;
        Ok(Self { provider, config })
    }

    pub fn config(&self) -> &WebAuthnProviderConfig {
        &self.config
    }

    pub fn provider(&self) -> &P {
        &self.provider
    }
}

impl<P> WebAuthnProvider for ConfiguredWebAuthnProvider<P>
where
    P: WebAuthnProvider,
{
    fn verify_registration(
        &self,
        request: &RegistrationRequest,
    ) -> Result<VerifiedRegistration, IdentityError> {
        self.config
            .validate_request_context(&request.rp_id, &request.origin)?;
        self.provider.verify_registration(request)
    }

    fn verify_assertion(
        &self,
        credential: &PasskeyCredential,
        request: &AssertionRequest,
    ) -> Result<AssertionVerification, IdentityError> {
        self.config
            .validate_request_context(&request.rp_id, &request.origin)?;
        self.provider.verify_assertion(credential, request)
    }

    fn verify_browser_registration(
        &self,
        principal_ref: &str,
        challenge: &WebAuthnChallenge,
        material: &ChallengeMaterial,
        request: &BrowserRegistrationRequest,
        at: DateTime<Utc>,
    ) -> Result<VerifiedRegistration, IdentityError> {
        self.config
            .validate_request_context(&request.rp_id, &request.origin)?;
        self.provider
            .verify_browser_registration(principal_ref, challenge, material, request, at)
    }

    fn verify_browser_assertion(
        &self,
        principal_ref: &str,
        credential: &PasskeyCredential,
        challenge: &WebAuthnChallenge,
        material: &ChallengeMaterial,
        request: &BrowserAssertionRequest,
        at: DateTime<Utc>,
    ) -> Result<AssertionVerification, IdentityError> {
        self.config
            .validate_request_context(&request.rp_id, &request.origin)?;
        self.provider.verify_browser_assertion(
            principal_ref,
            credential,
            challenge,
            material,
            request,
            at,
        )
    }

    fn start_browser_registration(
        &self,
        user_unique_id: Uuid,
        user_name: &str,
        user_display_name: &str,
        erpx_material: &[u8],
        exclude_credentials: &[Vec<u8>],
    ) -> Result<BrowserRegistrationStart, IdentityError> {
        self.provider.start_browser_registration(
            user_unique_id,
            user_name,
            user_display_name,
            erpx_material,
            exclude_credentials,
        )
    }

    fn start_browser_authentication(
        &self,
        credential: &PasskeyCredential,
        erpx_material: &[u8],
    ) -> Result<BrowserAuthenticationStart, IdentityError> {
        self.provider
            .start_browser_authentication(credential, erpx_material)
    }
}

/// Real browser-ceremony provider backed by `webauthn-rs`.
///
/// The provider state returned by the start methods is opaque server-side
/// state. Callers must persist it by ceremony ID and must never accept it
/// directly from a browser. The ERPX challenge digest is embedded in that
/// state so both challenge systems are required at finish time.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct WebAuthnRsProvider {
    config: WebAuthnProviderConfig,
    webauthn: Webauthn,
}

#[allow(dead_code)]
impl WebAuthnRsProvider {
    pub fn try_new(config: WebAuthnProviderConfig) -> Result<Self, IdentityError> {
        config.validate()?;
        let origins = config
            .allowed_origins()
            .iter()
            .map(|origin| Url::parse(origin).map_err(|_| IdentityError::WebAuthnOriginInvalid))
            .collect::<Result<Vec<_>, _>>()?;
        let first_origin = origins
            .first()
            .ok_or(IdentityError::WebAuthnConfigurationInvalid)?;
        let allow_subdomains = origins
            .iter()
            .any(|origin| origin.domain().is_some_and(|host| host != config.rp_id()));
        let mut builder = WebauthnBuilder::new(config.rp_id(), first_origin)
            .map_err(|_| IdentityError::WebAuthnConfigurationInvalid)?
            .allow_subdomains(allow_subdomains);
        for origin in origins.iter().skip(1) {
            builder = builder.append_allowed_origin(origin);
        }
        let webauthn = builder
            .build()
            .map_err(|_| IdentityError::WebAuthnConfigurationInvalid)?;
        Ok(Self { config, webauthn })
    }

    pub fn config(&self) -> &WebAuthnProviderConfig {
        &self.config
    }

    fn start_state<T: Serialize>(
        kind: BrowserCeremonyKind,
        state: &T,
        erpx_material: &[u8],
    ) -> Result<Vec<u8>, IdentityError> {
        let provider_state =
            serde_json::to_vec(state).map_err(|_| IdentityError::InvalidCredential)?;
        if provider_state.len() > MAX_PROVIDER_STATE_BYTES {
            return Err(IdentityError::InvalidCredential);
        }
        let envelope = BrowserProviderState {
            kind,
            provider_state,
            erpx_challenge_digest: challenge_digest(erpx_material),
        };
        let encoded =
            serde_json::to_vec(&envelope).map_err(|_| IdentityError::InvalidCredential)?;
        if encoded.len() > MAX_PROVIDER_STATE_BYTES {
            return Err(IdentityError::InvalidCredential);
        }
        Ok(encoded)
    }

    fn provider_state<T: serde::de::DeserializeOwned>(
        kind: BrowserCeremonyKind,
        material: &[u8],
        state: &[u8],
    ) -> Result<T, IdentityError> {
        if state.len() > MAX_PROVIDER_STATE_BYTES {
            return Err(IdentityError::InvalidCredential);
        }
        let envelope: BrowserProviderState =
            serde_json::from_slice(state).map_err(|_| IdentityError::InvalidCredential)?;
        if envelope.kind != kind
            || envelope.erpx_challenge_digest != challenge_digest(material)
            || envelope.provider_state.is_empty()
        {
            return Err(IdentityError::ChallengeMismatch);
        }
        serde_json::from_slice(&envelope.provider_state)
            .map_err(|_| IdentityError::InvalidCredential)
    }

    fn browser_response<T: serde::de::DeserializeOwned>(
        response: &[u8],
    ) -> Result<T, IdentityError> {
        if response.is_empty() || response.len() > MAX_BROWSER_RESPONSE_BYTES {
            return Err(IdentityError::InvalidCredential);
        }
        serde_json::from_slice(response).map_err(|_| IdentityError::InvalidCredential)
    }

    fn validate_challenge(
        &self,
        principal_ref: &str,
        challenge: &WebAuthnChallenge,
        ceremony: ChallengeCeremony,
        material: &ChallengeMaterial,
        at: DateTime<Utc>,
    ) -> Result<(), IdentityError> {
        challenge
            .consume_for(principal_ref, ceremony, material.as_bytes(), at)
            .map(|_| ())
    }

    fn serialized_passkey(passkey: &Passkey) -> Result<Vec<u8>, IdentityError> {
        let encoded = serde_json::to_vec(passkey).map_err(|_| IdentityError::InvalidCredential)?;
        if encoded.len() > MAX_PROVIDER_STATE_BYTES {
            return Err(IdentityError::InvalidCredential);
        }
        Ok(encoded)
    }

    fn persisted_passkey(bytes: &[u8]) -> Result<Passkey, IdentityError> {
        if bytes.is_empty() || bytes.len() > MAX_PROVIDER_STATE_BYTES {
            return Err(IdentityError::InvalidCredential);
        }
        serde_json::from_slice(bytes).map_err(|_| IdentityError::InvalidCredential)
    }
}

impl WebAuthnProvider for WebAuthnRsProvider {
    fn verify_registration(
        &self,
        _request: &RegistrationRequest,
    ) -> Result<VerifiedRegistration, IdentityError> {
        // The legacy request has no browser response or provider state. Never
        // reinterpret its public_key field as ceremony data.
        Err(IdentityError::WebAuthnProviderUnavailable)
    }

    fn verify_assertion(
        &self,
        _credential: &PasskeyCredential,
        _request: &AssertionRequest,
    ) -> Result<AssertionVerification, IdentityError> {
        Err(IdentityError::WebAuthnProviderUnavailable)
    }

    fn start_browser_registration(
        &self,
        user_unique_id: Uuid,
        user_name: &str,
        user_display_name: &str,
        erpx_material: &[u8],
        exclude_credentials: &[Vec<u8>],
    ) -> Result<BrowserRegistrationStart, IdentityError> {
        let exclude = (!exclude_credentials.is_empty()).then(|| {
            exclude_credentials
                .iter()
                .cloned()
                .map(Into::into)
                .collect()
        });
        let (options, state): (CreationChallengeResponse, PasskeyRegistration) = self
            .webauthn
            .start_passkey_registration(user_unique_id, user_name, user_display_name, exclude)
            .map_err(|_| IdentityError::InvalidCredential)?;
        Ok(BrowserRegistrationStart {
            options: serde_json::to_vec(&options).map_err(|_| IdentityError::InvalidCredential)?,
            state: Self::start_state(BrowserCeremonyKind::Registration, &state, erpx_material)?,
        })
    }

    fn start_browser_authentication(
        &self,
        credential: &PasskeyCredential,
        erpx_material: &[u8],
    ) -> Result<BrowserAuthenticationStart, IdentityError> {
        if credential.rp_id() != self.config.rp_id() {
            return Err(IdentityError::WebAuthnRpIdMismatch);
        }
        let passkey = Self::persisted_passkey(credential.public_key())?;
        if passkey.cred_id().as_ref() != credential.credential_id() {
            return Err(IdentityError::WebAuthnCredentialIdMismatch);
        }
        let (options, state): (RequestChallengeResponse, PasskeyAuthentication) = self
            .webauthn
            .start_passkey_authentication(std::slice::from_ref(&passkey))
            .map_err(|_| IdentityError::InvalidCredential)?;
        Ok(BrowserAuthenticationStart {
            options: serde_json::to_vec(&options).map_err(|_| IdentityError::InvalidCredential)?,
            state: Self::start_state(BrowserCeremonyKind::Authentication, &state, erpx_material)?,
        })
    }

    fn verify_browser_registration(
        &self,
        principal_ref: &str,
        challenge: &WebAuthnChallenge,
        material: &ChallengeMaterial,
        request: &BrowserRegistrationRequest,
        at: DateTime<Utc>,
    ) -> Result<VerifiedRegistration, IdentityError> {
        self.config
            .validate_request_context(&request.rp_id, &request.origin)?;
        self.validate_challenge(
            principal_ref,
            challenge,
            ChallengeCeremony::Registration,
            material,
            at,
        )?;
        let state: PasskeyRegistration = Self::provider_state(
            BrowserCeremonyKind::Registration,
            material.as_bytes(),
            &request.state,
        )?;
        let response: RegisterPublicKeyCredential = Self::browser_response(&request.response)?;
        let passkey = self
            .webauthn
            .finish_passkey_registration(&response, &state)
            .map_err(|_| IdentityError::InvalidCredential)?;
        let credential_id = passkey.cred_id().as_ref().to_vec();
        if credential_id != request.credential_id {
            return Err(IdentityError::WebAuthnCredentialIdMismatch);
        }
        Ok(VerifiedRegistration {
            credential_id,
            public_key: Self::serialized_passkey(&passkey)?,
            sign_count: 0,
        })
    }

    fn verify_browser_assertion(
        &self,
        principal_ref: &str,
        credential: &PasskeyCredential,
        challenge: &WebAuthnChallenge,
        material: &ChallengeMaterial,
        request: &BrowserAssertionRequest,
        at: DateTime<Utc>,
    ) -> Result<AssertionVerification, IdentityError> {
        self.config
            .validate_request_context(&request.rp_id, &request.origin)?;
        if credential.principal_ref() != principal_ref
            || credential.credential_id() != request.credential_id
        {
            return Err(IdentityError::WebAuthnCredentialIdMismatch);
        }
        if credential.rp_id() != self.config.rp_id() {
            return Err(IdentityError::WebAuthnRpIdMismatch);
        }
        self.validate_challenge(
            principal_ref,
            challenge,
            ChallengeCeremony::Assertion,
            material,
            at,
        )?;
        let state: PasskeyAuthentication = Self::provider_state(
            BrowserCeremonyKind::Authentication,
            material.as_bytes(),
            &request.state,
        )?;
        let response: PublicKeyCredential = Self::browser_response(&request.response)?;
        let result = self
            .webauthn
            .finish_passkey_authentication(&response, &state)
            .map_err(|_| IdentityError::InvalidCredential)?;
        if result.cred_id().as_ref() != request.credential_id {
            return Err(IdentityError::WebAuthnCredentialIdMismatch);
        }
        Ok(AssertionVerification {
            credential_id: result.cred_id().as_ref().to_vec(),
            sign_count: result.counter(),
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UnsupportedWebAuthnProvider;

impl WebAuthnProvider for UnsupportedWebAuthnProvider {
    fn verify_registration(
        &self,
        _request: &RegistrationRequest,
    ) -> Result<VerifiedRegistration, IdentityError> {
        Err(IdentityError::WebAuthnProviderUnavailable)
    }

    fn verify_assertion(
        &self,
        _credential: &PasskeyCredential,
        _request: &AssertionRequest,
    ) -> Result<AssertionVerification, IdentityError> {
        Err(IdentityError::WebAuthnProviderUnavailable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasskeyCredential {
    id: Uuid,
    principal_ref: String,
    rp_id: String,
    credential_id: Vec<u8>,
    public_key: Vec<u8>,
    sign_count: u32,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
}

impl PasskeyCredential {
    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn principal_ref(&self) -> &str {
        &self.principal_ref
    }
    pub fn rp_id(&self) -> &str {
        &self.rp_id
    }
    pub fn credential_id(&self) -> &[u8] {
        &self.credential_id
    }
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }
    pub fn sign_count(&self) -> u32 {
        self.sign_count
    }
    pub fn revoked_at(&self) -> Option<DateTime<Utc>> {
        self.revoked_at
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn last_used_at(&self) -> Option<DateTime<Utc>> {
        self.last_used_at
    }

    pub(crate) fn from_verified(
        principal_ref: &str,
        rp_id: &str,
        verified: VerifiedRegistration,
        at: DateTime<Utc>,
    ) -> Result<Self, IdentityError> {
        if principal_ref.trim().is_empty()
            || verified.credential_id.is_empty()
            || verified.public_key.is_empty()
        {
            return Err(IdentityError::InvalidCredential);
        }
        Ok(Self {
            id: Uuid::new_v4(),
            principal_ref: principal_ref.into(),
            rp_id: canonical_rp_id(rp_id)?,
            credential_id: verified.credential_id,
            public_key: verified.public_key,
            sign_count: verified.sign_count,
            created_at: at,
            last_used_at: None,
            revoked_at: None,
        })
    }

    pub(crate) fn from_persisted(record: PersistedPasskey) -> Result<Self, IdentityError> {
        let rp_id = canonical_rp_id(&record.rp_id)?;
        let credential = Self {
            id: record.id,
            principal_ref: record.principal_ref,
            rp_id,
            credential_id: record.credential_id,
            public_key: record.public_key,
            sign_count: record.sign_count,
            created_at: record.created_at,
            last_used_at: record.last_used_at,
            revoked_at: record.revoked_at,
        };
        credential.validate_for_persistence()?;
        Ok(credential)
    }

    pub(crate) fn validate_for_persistence(&self) -> Result<(), IdentityError> {
        if self.principal_ref.trim().is_empty()
            || self.credential_id.is_empty()
            || self.public_key.is_empty()
        {
            return Err(IdentityError::InvalidCredential);
        }
        if canonical_rp_id(&self.rp_id)? != self.rp_id {
            return Err(IdentityError::WebAuthnRpIdInvalid);
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct PersistedPasskey {
    pub(crate) id: Uuid,
    pub(crate) principal_ref: String,
    pub(crate) rp_id: String,
    pub(crate) credential_id: Vec<u8>,
    pub(crate) public_key: Vec<u8>,
    pub(crate) sign_count: u32,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) last_used_at: Option<DateTime<Utc>>,
    pub(crate) revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct PasskeyService<P> {
    provider: P,
}

impl<P> PasskeyService<P>
where
    P: WebAuthnProvider,
{
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    pub fn register(
        &self,
        principal_ref: &str,
        challenge: &WebAuthnChallenge,
        material: &ChallengeMaterial,
        request: &RegistrationRequest,
        at: DateTime<Utc>,
    ) -> Result<(PasskeyCredential, WebAuthnChallenge), IdentityError> {
        let consumed = challenge.consume_for(
            principal_ref,
            ChallengeCeremony::Registration,
            material.as_bytes(),
            at,
        )?;
        if request.credential_id.is_empty() || request.public_key.is_empty() {
            return Err(IdentityError::InvalidCredential);
        }
        let rp_id = canonical_rp_id(&request.rp_id)?;
        let verified = self.provider.verify_registration(request)?;
        if verified.credential_id.is_empty() || verified.public_key.is_empty() {
            return Err(IdentityError::InvalidCredential);
        }
        if verified.credential_id != request.credential_id {
            return Err(IdentityError::WebAuthnCredentialIdMismatch);
        }
        Ok((
            PasskeyCredential::from_verified(principal_ref, &rp_id, verified, at)?,
            consumed,
        ))
    }

    pub fn assert(
        &self,
        principal_ref: &str,
        credential: &PasskeyCredential,
        challenge: &WebAuthnChallenge,
        material: &ChallengeMaterial,
        request: &AssertionRequest,
        at: DateTime<Utc>,
    ) -> Result<(PasskeyCredential, WebAuthnChallenge), IdentityError> {
        if credential.principal_ref != principal_ref || credential.revoked_at.is_some() {
            return Err(IdentityError::InvalidCredential);
        }
        if request.credential_id != credential.credential_id {
            return Err(IdentityError::WebAuthnCredentialIdMismatch);
        }
        if canonical_rp_id(&request.rp_id)? != credential.rp_id {
            return Err(IdentityError::WebAuthnRpIdMismatch);
        }
        let consumed = challenge.consume_for(
            principal_ref,
            ChallengeCeremony::Assertion,
            material.as_bytes(),
            at,
        )?;
        let verified = self.provider.verify_assertion(credential, request)?;
        if verified.credential_id != credential.credential_id {
            return Err(IdentityError::WebAuthnCredentialIdMismatch);
        }
        if verified.sign_count < credential.sign_count {
            return Err(IdentityError::CredentialCounterRollback);
        }
        let mut next = credential.clone();
        next.sign_count = verified.sign_count;
        next.last_used_at = Some(at);
        Ok((next, consumed))
    }
}

fn canonical_rp_id(value: &str) -> Result<String, IdentityError> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 253
        || value.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(IdentityError::WebAuthnRpIdInvalid);
    }

    let labels = value.split('.');
    if labels.clone().any(|label| label.is_empty()) {
        return Err(IdentityError::WebAuthnRpIdInvalid);
    }

    for label in labels {
        if label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || label
                .bytes()
                .any(|byte| !(byte.is_ascii_alphanumeric() || byte == b'-'))
        {
            return Err(IdentityError::WebAuthnRpIdInvalid);
        }
    }

    Ok(value.to_ascii_lowercase())
}

#[allow(dead_code)]
fn challenge_digest(material: &[u8]) -> [u8; 32] {
    Sha256::digest(material).into()
}

fn canonical_origin(value: &str) -> Result<(String, String), IdentityError> {
    if value.is_empty()
        || value.trim() != value
        || value.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(IdentityError::WebAuthnOriginInvalid);
    }
    let (scheme, authority) = value
        .split_once("://")
        .ok_or(IdentityError::WebAuthnOriginInvalid)?;
    if scheme != "https" && scheme != "http" {
        return Err(IdentityError::WebAuthnOriginInvalid);
    }
    if authority.is_empty()
        || authority.contains(['/', '?', '#', '@'])
        || authority.starts_with('[')
    {
        return Err(IdentityError::WebAuthnOriginInvalid);
    }

    let (host, port) = match authority.split_once(':') {
        Some((host, port)) if !port.is_empty() && !port.contains(':') => {
            let port = port
                .parse::<u16>()
                .map_err(|_| IdentityError::WebAuthnOriginInvalid)?;
            if port == 0 {
                return Err(IdentityError::WebAuthnOriginInvalid);
            }
            (host, Some(port))
        }
        Some(_) => return Err(IdentityError::WebAuthnOriginInvalid),
        None => (authority, None),
    };
    let host = canonical_rp_id(host).map_err(|_| IdentityError::WebAuthnOriginInvalid)?;
    if scheme == "http" && !is_local_development_host(&host) {
        return Err(IdentityError::WebAuthnOriginInvalid);
    }

    let canonical = match port {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    };
    if canonical != value {
        return Err(IdentityError::WebAuthnOriginInvalid);
    }
    Ok((canonical, host))
}

fn origin_host_matches_rp(host: &str, rp_id: &str) -> bool {
    host == rp_id || host.ends_with(&format!(".{rp_id}"))
}

fn is_local_development_host(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1"
}

impl fmt::Display for PasskeyCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "passkey:{}", self.id)
    }
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use chrono::{Duration, TimeZone, Utc};
    use serde_json::Value;

    use crate::ChallengeLifetime;

    use super::*;

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 24, 12, 0, 0)
            .single()
            .expect("fixed test time")
    }

    fn config() -> WebAuthnProviderConfig {
        WebAuthnProviderConfig::try_new("example.com", ["https://app.example.com"])
            .expect("valid config")
    }

    fn registration_request() -> RegistrationRequest {
        RegistrationRequest {
            rp_id: "example.com".to_owned(),
            origin: "https://app.example.com".to_owned(),
            credential_id: vec![1, 2, 3],
            public_key: vec![4, 5, 6],
            sign_count: 1,
        }
    }

    fn assertion_request(credential_id: Vec<u8>) -> AssertionRequest {
        AssertionRequest {
            rp_id: "example.com".to_owned(),
            origin: "https://app.example.com".to_owned(),
            credential_id,
            sign_count: 0,
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct TestProvider {
        assertion_count: u32,
    }

    impl WebAuthnProvider for TestProvider {
        fn verify_registration(
            &self,
            request: &RegistrationRequest,
        ) -> Result<VerifiedRegistration, IdentityError> {
            Ok(VerifiedRegistration {
                credential_id: request.credential_id.clone(),
                public_key: request.public_key.clone(),
                sign_count: request.sign_count,
            })
        }

        fn verify_assertion(
            &self,
            _credential: &PasskeyCredential,
            request: &AssertionRequest,
        ) -> Result<AssertionVerification, IdentityError> {
            Ok(AssertionVerification {
                credential_id: request.credential_id.clone(),
                sign_count: self.assertion_count,
            })
        }
    }

    fn configured_provider(assertion_count: u32) -> ConfiguredWebAuthnProvider<TestProvider> {
        ConfiguredWebAuthnProvider::try_new(TestProvider { assertion_count }, config())
            .expect("provider config")
    }

    fn challenge(ceremony: ChallengeCeremony) -> (WebAuthnChallenge, ChallengeMaterial) {
        WebAuthnChallenge::issue(
            "principal-1",
            ceremony,
            now(),
            ChallengeLifetime::try_new(Duration::minutes(5)).expect("lifetime"),
        )
        .expect("challenge")
    }

    #[test]
    fn config_validates_rp_origin_binding_and_allowlist() {
        assert!(matches!(
            WebAuthnProviderConfig::try_new("https://example.com", ["https://app.example.com"]),
            Err(IdentityError::WebAuthnRpIdInvalid)
        ));
        assert!(matches!(
            WebAuthnProviderConfig::try_new("example.com", Vec::<String>::new()),
            Err(IdentityError::WebAuthnConfigurationInvalid)
        ));
        assert!(matches!(
            WebAuthnProviderConfig::try_new("example.com", ["http://example.com"]),
            Err(IdentityError::WebAuthnOriginInvalid)
        ));
        assert!(matches!(
            WebAuthnProviderConfig::try_new("example.com", ["https://evil.example.net"]),
            Err(IdentityError::WebAuthnOriginInvalid)
        ));

        let provider = configured_provider(1);
        let mut registration = registration_request();
        registration.rp_id = "other.example.com".to_owned();
        assert!(matches!(
            provider.verify_registration(&registration),
            Err(IdentityError::WebAuthnRpIdMismatch)
        ));
        registration.rp_id = "example.com".to_owned();
        registration.origin = "https://evil.example.com".to_owned();
        assert!(matches!(
            provider.verify_registration(&registration),
            Err(IdentityError::WebAuthnOriginNotAllowed)
        ));
    }

    #[test]
    fn challenge_context_checks_ceremony_validity_window_and_replay() {
        let (registration, material) = challenge(ChallengeCeremony::Registration);
        assert!(matches!(
            registration.consume_for(
                "principal-1",
                ChallengeCeremony::Assertion,
                material.as_bytes(),
                now() + Duration::seconds(1)
            ),
            Err(IdentityError::CeremonyMismatch)
        ));
        assert!(matches!(
            registration.consume_for(
                "principal-1",
                ChallengeCeremony::Registration,
                material.as_bytes(),
                now() - Duration::seconds(1)
            ),
            Err(IdentityError::ChallengeNotYetValid)
        ));
        let consumed = registration
            .consume_for(
                "principal-1",
                ChallengeCeremony::Registration,
                material.as_bytes(),
                now() + Duration::seconds(1),
            )
            .expect("valid challenge");
        assert!(matches!(
            consumed.consume(material.as_bytes(), now() + Duration::seconds(2)),
            Err(IdentityError::ChallengeReplay)
        ));

        let (expired, expired_material) = challenge(ChallengeCeremony::Assertion);
        assert!(matches!(
            expired.consume_for(
                "principal-1",
                ChallengeCeremony::Assertion,
                expired_material.as_bytes(),
                now() + Duration::minutes(5)
            ),
            Err(IdentityError::ChallengeExpired)
        ));
    }

    #[test]
    fn configured_provider_seam_delegates_without_claiming_browser_verification() {
        let service = PasskeyService::new(configured_provider(2));
        let (registration, material) = challenge(ChallengeCeremony::Registration);
        let (credential, consumed) = service
            .register(
                "principal-1",
                &registration,
                &material,
                &registration_request(),
                now() + Duration::seconds(1),
            )
            .expect("registration");
        assert!(consumed.is_consumed());
        assert_eq!(credential.credential_id(), &[1, 2, 3]);
        assert_eq!(credential.sign_count(), 1);

        let (assertion, assertion_material) = challenge(ChallengeCeremony::Assertion);
        let request = assertion_request(credential.credential_id().to_vec());
        let (updated, consumed) = service
            .assert(
                "principal-1",
                &credential,
                &assertion,
                &assertion_material,
                &request,
                now() + Duration::seconds(2),
            )
            .expect("assertion");
        assert!(consumed.is_consumed());
        assert_eq!(updated.sign_count(), 2);
        assert_eq!(updated.last_used_at, Some(now() + Duration::seconds(2)));
    }

    #[test]
    fn assertion_counter_is_non_decreasing_and_credential_id_is_bound() {
        let registration_service = PasskeyService::new(configured_provider(2));
        let (registration, material) = challenge(ChallengeCeremony::Registration);
        let (credential, _) = registration_service
            .register(
                "principal-1",
                &registration,
                &material,
                &registration_request(),
                now() + Duration::seconds(1),
            )
            .expect("registration");

        let equal_service = PasskeyService::new(configured_provider(1));
        let (equal_challenge, equal_material) = challenge(ChallengeCeremony::Assertion);
        let equal_request = assertion_request(credential.credential_id().to_vec());
        let (equal, _) = equal_service
            .assert(
                "principal-1",
                &credential,
                &equal_challenge,
                &equal_material,
                &equal_request,
                now() + Duration::seconds(2),
            )
            .expect("equal counter is permitted");
        assert_eq!(equal.sign_count(), credential.sign_count());

        let rollback_service = PasskeyService::new(configured_provider(0));
        let (rollback_challenge, rollback_material) = challenge(ChallengeCeremony::Assertion);
        let rollback_request = assertion_request(credential.credential_id().to_vec());
        assert!(matches!(
            rollback_service.assert(
                "principal-1",
                &credential,
                &rollback_challenge,
                &rollback_material,
                &rollback_request,
                now() + Duration::seconds(3),
            ),
            Err(IdentityError::CredentialCounterRollback)
        ));

        let (mismatch_challenge, mismatch_material) = challenge(ChallengeCeremony::Assertion);
        let mismatch_request = assertion_request(vec![9, 9, 9]);
        assert!(matches!(
            equal_service.assert(
                "principal-1",
                &credential,
                &mismatch_challenge,
                &mismatch_material,
                &mismatch_request,
                now() + Duration::seconds(4),
            ),
            Err(IdentityError::WebAuthnCredentialIdMismatch)
        ));
    }

    #[test]
    fn revoked_persisted_credential_is_rejected_before_provider_assertion() {
        let revoked_at = now() + Duration::seconds(1);
        let credential = PasskeyCredential::from_persisted(PersistedPasskey {
            id: Uuid::new_v4(),
            principal_ref: "principal-1".to_owned(),
            rp_id: "example.com".to_owned(),
            credential_id: vec![1, 2, 3],
            public_key: vec![4, 5, 6],
            sign_count: 2,
            created_at: now(),
            last_used_at: None,
            revoked_at: Some(revoked_at),
        })
        .expect("persisted credential");
        let (assertion, material) = challenge(ChallengeCeremony::Assertion);
        let service = PasskeyService::new(configured_provider(3));
        assert!(matches!(
            service.assert(
                "principal-1",
                &credential,
                &assertion,
                &material,
                &assertion_request(credential.credential_id().to_vec()),
                now() + Duration::seconds(2),
            ),
            Err(IdentityError::InvalidCredential)
        ));
    }

    #[test]
    fn unsupported_provider_and_wrong_ceremony_fail_closed() {
        let unsupported = PasskeyService::new(UnsupportedWebAuthnProvider);
        let (registration, material) = challenge(ChallengeCeremony::Registration);
        assert!(matches!(
            unsupported.register(
                "principal-1",
                &registration,
                &material,
                &registration_request(),
                now() + Duration::seconds(1),
            ),
            Err(IdentityError::WebAuthnProviderUnavailable)
        ));

        let service = PasskeyService::new(configured_provider(1));
        let (assertion, assertion_material) = challenge(ChallengeCeremony::Assertion);
        assert!(matches!(
            service.register(
                "principal-1",
                &assertion,
                &assertion_material,
                &registration_request(),
                now() + Duration::seconds(1),
            ),
            Err(IdentityError::CeremonyMismatch)
        ));
    }

    #[test]
    fn real_provider_parses_browser_bound_state_and_rejects_malformed_response() {
        let provider = WebAuthnRsProvider::try_new(config()).expect("provider config");
        let (challenge, material) = challenge(ChallengeCeremony::Registration);
        let start = provider
            .start_browser_registration(
                Uuid::from_u128(1),
                "principal-1",
                "Principal One",
                material.as_bytes(),
                &[],
            )
            .expect("browser registration start");
        assert!(
            start
                .options()
                .windows(9)
                .any(|window| window == b"challenge")
        );
        assert!(!start.state().is_empty());

        let malformed = BrowserRegistrationRequest {
            rp_id: "example.com".into(),
            origin: "https://app.example.com".into(),
            credential_id: vec![1, 2, 3],
            response: br#"{}"#.to_vec(),
            state: start.state().to_vec(),
        };
        assert!(matches!(
            provider.verify_browser_registration(
                "principal-1",
                &challenge,
                &material,
                &malformed,
                now() + Duration::seconds(1),
            ),
            Err(IdentityError::InvalidCredential)
        ));

        let wrong_material = ChallengeMaterial::from_slice(b"wrong-material");
        assert!(matches!(
            provider.verify_browser_registration(
                "principal-1",
                &challenge,
                &wrong_material,
                &malformed,
                now() + Duration::seconds(1),
            ),
            Err(IdentityError::ChallengeMismatch)
        ));
    }

    #[test]
    fn real_provider_rejects_tampered_state_origin_and_legacy_payloads() {
        let provider = WebAuthnRsProvider::try_new(config()).expect("provider config");
        let (challenge, material) = challenge(ChallengeCeremony::Registration);
        let start = provider
            .start_browser_registration(
                Uuid::from_u128(2),
                "principal-1",
                "Principal One",
                material.as_bytes(),
                &[],
            )
            .expect("browser registration start");
        let mut tampered_state = start.state().to_vec();
        tampered_state[0] = b'X';
        let mut request = BrowserRegistrationRequest {
            rp_id: "example.com".into(),
            origin: "https://app.example.com".into(),
            credential_id: vec![1, 2, 3],
            response: br#"{}"#.to_vec(),
            state: tampered_state,
        };
        assert!(matches!(
            provider.verify_browser_registration(
                "principal-1",
                &challenge,
                &material,
                &request,
                now() + Duration::seconds(1),
            ),
            Err(IdentityError::InvalidCredential)
        ));
        request.origin = "https://evil.example.net".into();
        assert!(matches!(
            provider.verify_browser_registration(
                "principal-1",
                &challenge,
                &material,
                &request,
                now() + Duration::seconds(1),
            ),
            Err(IdentityError::WebAuthnOriginNotAllowed)
        ));

        assert!(matches!(
            provider.verify_registration(&registration_request()),
            Err(IdentityError::WebAuthnProviderUnavailable)
        ));
        assert!(matches!(
            provider.start_browser_authentication(
                &PasskeyCredential::from_persisted(PersistedPasskey {
                    id: Uuid::from_u128(3),
                    principal_ref: "principal-1".into(),
                    rp_id: "example.com".into(),
                    credential_id: vec![1, 2, 3],
                    public_key: vec![0],
                    sign_count: 0,
                    created_at: now(),
                    last_used_at: None,
                    revoked_at: None,
                })
                .expect("persisted credential"),
                material.as_bytes(),
            ),
            Err(IdentityError::InvalidCredential)
        ));

        let mismatched_rp = PasskeyCredential::from_persisted(PersistedPasskey {
            id: Uuid::from_u128(5),
            principal_ref: "principal-1".into(),
            rp_id: "other.example.com".into(),
            credential_id: vec![1, 2, 3],
            public_key: vec![0],
            sign_count: 0,
            created_at: now(),
            last_used_at: None,
            revoked_at: None,
        })
        .expect("persisted credential");
        assert!(matches!(
            provider.start_browser_authentication(&mismatched_rp, material.as_bytes()),
            Err(IdentityError::WebAuthnRpIdMismatch)
        ));
    }

    #[test]
    fn real_provider_accepts_deterministic_attestation_vector() {
        let config = WebAuthnProviderConfig::try_new("localhost", ["https://localhost:8443"])
            .expect("local vector config");
        let provider = WebAuthnRsProvider::try_new(config).expect("provider config");
        let (registration_challenge, material) = challenge(ChallengeCeremony::Registration);
        let start = provider
            .start_browser_registration(
                Uuid::from_u128(4),
                "principal-1",
                "Principal One",
                material.as_bytes(),
                &[],
            )
            .expect("browser registration start");

        let client_challenge = URL_SAFE_NO_PAD
            .decode("lP6mWNAtG-_Vv15iM7lb_XRkdWMvVQ-lTyKwZuOg1Vo")
            .expect("vector challenge");
        let mut envelope: BrowserProviderState =
            serde_json::from_slice(start.state()).expect("provider state");
        let mut provider_state: Value =
            serde_json::from_slice(&envelope.provider_state).expect("provider state json");
        provider_state["rs"]["challenge"] = Value::String(URL_SAFE_NO_PAD.encode(client_challenge));
        envelope.provider_state =
            serde_json::to_vec(&provider_state).expect("provider state encoding");
        let state = serde_json::to_vec(&envelope).expect("state encoding");
        let credential_id = URL_SAFE_NO_PAD
            .decode("ATk_7QKbi_ntSdp16LXeU6RDf9YnRLIDTCqEjJFzc6rKBhbqoSYccxNa")
            .expect("vector credential id");
        let response = br#"{
            "id":"ATk_7QKbi_ntSdp16LXeU6RDf9YnRLIDTCqEjJFzc6rKBhbqoSYccxNa",
            "rawId":"ATk_7QKbi_ntSdp16LXeU6RDf9YnRLIDTCqEjJFzc6rKBhbqoSYccxNa",
            "response":{
                "attestationObject":"o2NmbXRmcGFja2VkZ2F0dFN0bXSiY2FsZyZjc2lnWEcwRQIgLXPjBtVEhBH3KdUDFFk3LAd9EtHogllIf48vjX4wgfECIQCXOymmfg12FPMXEdwpSjjtmrvki4K8y0uYxqWN5Bw6DGhhdXRoRGF0YViuSZYN5YgOjGh0NBcPZHZgW4_krrmihjLHmVzzuoMdl2NFXaqejq3OAAI1vMYKZIsLJfHwVQMAKgE5P-0Cm4v57Unadei13lOkQ3_WJ0SyA0wqhIyRc3OqygYW6qEmHHMTWqUBAgMmIAEhWCDNRS_Gw52ow5PNrC9OdFTFNudDmZO6Y3wmM9N8e0tJICJYIC09iIH5_RrT5tbS0PIw3srdAxYDMGao7yWgu0JFIEzT",
                "clientDataJSON":"eyJjaGFsbGVuZ2UiOiJsUDZtV05BdEctX1Z2MTVpTTdsYl9YUmtkV012VlEtbFR5S3dadU9nMVZvIiwiZXh0cmFfa2V5c19tYXlfYmVfYWRkZWRfaGVyZSI6ImRvIG5vdCBjb21wYXJlIGNsaWVudERhdGFKU09OIGFnYWluc3QgYSB0ZW1wbGF0ZS4gU2VlIGh0dHBzOi8vZ29vLmdsL3lhYlBleCIsIm9yaWdpbiI6Imh0dHBzOi8vbG9jYWxob3N0Ojg0NDMiLCJ0eXBlIjoid2ViYXV0aG4uY3JlYXRlIn0"
            },
            "type":"public-key"
        }"#;
        let request = BrowserRegistrationRequest {
            rp_id: "localhost".into(),
            origin: "https://localhost:8443".into(),
            credential_id,
            response: response.to_vec(),
            state,
        };
        let verified = provider
            .verify_browser_registration(
                "principal-1",
                &registration_challenge,
                &material,
                &request,
                now() + Duration::seconds(1),
            )
            .expect("real registration vector verifies");
        assert_eq!(verified.credential_id, request.credential_id);
        assert!(!verified.public_key.is_empty());
        assert_eq!(verified.sign_count, 0);

        let credential = PasskeyCredential::from_persisted(PersistedPasskey {
            id: Uuid::from_u128(6),
            principal_ref: "principal-1".into(),
            rp_id: "localhost".into(),
            credential_id: request.credential_id.clone(),
            public_key: verified.public_key,
            sign_count: 0,
            created_at: now(),
            last_used_at: None,
            revoked_at: None,
        })
        .expect("verified credential persistence shape");
        let (assertion, assertion_material) = challenge(ChallengeCeremony::Assertion);
        let authentication = provider
            .start_browser_authentication(&credential, assertion_material.as_bytes())
            .expect("browser authentication start");
        let malformed_assertion = BrowserAssertionRequest {
            rp_id: "localhost".into(),
            origin: "https://localhost:8443".into(),
            credential_id: credential.credential_id().to_vec(),
            response: br#"{}"#.to_vec(),
            state: authentication.state().to_vec(),
        };
        assert!(matches!(
            provider.verify_browser_assertion(
                "principal-1",
                &credential,
                &assertion,
                &assertion_material,
                &malformed_assertion,
                now() + Duration::seconds(1),
            ),
            Err(IdentityError::InvalidCredential)
        ));
    }
}
