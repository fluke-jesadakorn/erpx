use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::webauthn::{
    BrowserAssertionRequest, BrowserAuthenticationStart, BrowserRegistrationRequest,
    BrowserRegistrationStart,
};
use crate::{
    AssertionRequest, ChallengeCeremony, ChallengeLifetime, ChallengeMaterial, IdentityError,
    IdentityStoreError, PasskeyCredential, PasskeyService, RegistrationRequest, SqlxIdentityStore,
    WebAuthnChallenge, WebAuthnProvider,
};

const PASSKEY_CHALLENGE_LIFETIME: Duration = Duration::minutes(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasskeyCeremonyStart {
    challenge: WebAuthnChallenge,
    material: ChallengeMaterial,
}

impl PasskeyCeremonyStart {
    pub fn challenge(&self) -> &WebAuthnChallenge {
        &self.challenge
    }

    /// The challenge value is public ceremony material, not a session or
    /// credential secret. Callers must return it only in the ceremony DTO.
    pub fn material(&self) -> &ChallengeMaterial {
        &self.material
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct BrowserRegistrationCeremonyStart {
    challenge: PasskeyCeremonyStart,
    browser: BrowserRegistrationStart,
}

impl BrowserRegistrationCeremonyStart {
    pub fn challenge(&self) -> &PasskeyCeremonyStart {
        &self.challenge
    }

    pub fn browser(&self) -> &BrowserRegistrationStart {
        &self.browser
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct BrowserAuthenticationCeremonyStart {
    challenge: PasskeyCeremonyStart,
    browser: BrowserAuthenticationStart,
}

impl BrowserAuthenticationCeremonyStart {
    pub fn challenge(&self) -> &PasskeyCeremonyStart {
        &self.challenge
    }

    pub fn browser(&self) -> &BrowserAuthenticationStart {
        &self.browser
    }
}

#[derive(Debug, Clone)]
pub struct PasskeyCeremonyService<P> {
    provider: P,
    identity: SqlxIdentityStore,
}

impl<P> PasskeyCeremonyService<P>
where
    P: WebAuthnProvider,
{
    pub fn new(provider: P, identity: SqlxIdentityStore) -> Self {
        Self { provider, identity }
    }

    pub async fn begin_registration(
        &self,
        principal_ref: &str,
        at: DateTime<Utc>,
    ) -> Result<PasskeyCeremonyStart, IdentityStoreError> {
        self.begin(principal_ref, ChallengeCeremony::Registration, at)
            .await
    }

    pub async fn begin_authentication(
        &self,
        principal_ref: &str,
        at: DateTime<Utc>,
    ) -> Result<PasskeyCeremonyStart, IdentityStoreError> {
        self.begin(principal_ref, ChallengeCeremony::Assertion, at)
            .await
    }

    pub async fn begin_browser_registration(
        &self,
        principal_ref: &str,
        user_unique_id: Uuid,
        user_name: &str,
        user_display_name: &str,
        exclude_credentials: &[Vec<u8>],
        at: DateTime<Utc>,
    ) -> Result<BrowserRegistrationCeremonyStart, IdentityStoreError> {
        let challenge = self.begin_registration(principal_ref, at).await?;
        let browser = self
            .provider
            .start_browser_registration(
                user_unique_id,
                user_name,
                user_display_name,
                challenge.material().as_bytes(),
                exclude_credentials,
            )
            .map_err(IdentityStoreError::Primitive)?;
        self.identity
            .save_webauthn_provider_state(
                principal_ref,
                challenge.challenge().id,
                ChallengeCeremony::Registration,
                browser.state(),
            )
            .await?;
        Ok(BrowserRegistrationCeremonyStart { challenge, browser })
    }

    pub async fn begin_browser_authentication(
        &self,
        principal_ref: &str,
        credential: &PasskeyCredential,
        at: DateTime<Utc>,
    ) -> Result<BrowserAuthenticationCeremonyStart, IdentityStoreError> {
        if credential.principal_ref() != principal_ref {
            return Err(IdentityStoreError::Primitive(
                IdentityError::InvalidCredential,
            ));
        }
        if credential.revoked_at().is_some() {
            return Err(IdentityStoreError::PasskeyRevoked);
        }
        let challenge = self.begin_authentication(principal_ref, at).await?;
        let browser = self
            .provider
            .start_browser_authentication(credential, challenge.material().as_bytes())
            .map_err(IdentityStoreError::Primitive)?;
        self.identity
            .save_webauthn_provider_state(
                principal_ref,
                challenge.challenge().id,
                ChallengeCeremony::Assertion,
                browser.state(),
            )
            .await?;
        Ok(BrowserAuthenticationCeremonyStart { challenge, browser })
    }

    pub async fn finish_registration(
        &self,
        principal_ref: &str,
        challenge_id: Uuid,
        material: &[u8],
        request: &RegistrationRequest,
        at: DateTime<Utc>,
    ) -> Result<PasskeyCredential, IdentityStoreError> {
        let challenge = self
            .identity
            .load_webauthn_challenge(principal_ref, challenge_id, ChallengeCeremony::Registration)
            .await?;
        let material = ChallengeMaterial::from_slice(material);
        self.identity
            .consume_webauthn_challenge(
                principal_ref,
                challenge_id,
                ChallengeCeremony::Registration,
                material.as_bytes(),
                at,
            )
            .await?;
        let (credential, _) = PasskeyService::new(&self.provider).register(
            principal_ref,
            &challenge,
            &material,
            request,
            at,
        )?;
        self.identity
            .register_passkey(principal_ref, &credential)
            .await
    }

    pub async fn finish_authentication(
        &self,
        principal_ref: &str,
        challenge_id: Uuid,
        material: &[u8],
        request: &AssertionRequest,
        at: DateTime<Utc>,
    ) -> Result<PasskeyCredential, IdentityStoreError> {
        let credential = self
            .identity
            .find_passkey(principal_ref, &request.credential_id)
            .await?
            .ok_or(IdentityStoreError::PasskeyNotFound)?;
        if credential.revoked_at().is_some() {
            return Err(IdentityStoreError::PasskeyRevoked);
        }
        let challenge = self
            .identity
            .load_webauthn_challenge(principal_ref, challenge_id, ChallengeCeremony::Assertion)
            .await?;
        let material = ChallengeMaterial::from_slice(material);
        self.identity
            .consume_webauthn_challenge(
                principal_ref,
                challenge_id,
                ChallengeCeremony::Assertion,
                material.as_bytes(),
                at,
            )
            .await?;
        let (updated, _) = PasskeyService::new(&self.provider).assert(
            principal_ref,
            &credential,
            &challenge,
            &material,
            request,
            at,
        )?;
        let credential_id = credential.credential_id().to_vec();
        self.identity
            .compare_and_set_passkey_counter(
                principal_ref,
                &credential_id,
                credential.sign_count(),
                updated.sign_count(),
                at,
            )
            .await
    }

    pub async fn finish_browser_registration(
        &self,
        principal_ref: &str,
        challenge_id: Uuid,
        material: &[u8],
        request: &BrowserRegistrationRequest,
        at: DateTime<Utc>,
    ) -> Result<PasskeyCredential, IdentityStoreError> {
        let challenge = self
            .identity
            .load_webauthn_challenge(principal_ref, challenge_id, ChallengeCeremony::Registration)
            .await?;
        let material = ChallengeMaterial::from_slice(material);
        let mut request = request.clone();
        request.state = self
            .identity
            .load_webauthn_provider_state(
                principal_ref,
                challenge_id,
                ChallengeCeremony::Registration,
            )
            .await?;
        self.identity
            .consume_webauthn_challenge(
                principal_ref,
                challenge_id,
                ChallengeCeremony::Registration,
                material.as_bytes(),
                at,
            )
            .await?;
        let verified = self
            .provider
            .verify_browser_registration(principal_ref, &challenge, &material, &request, at)
            .map_err(IdentityStoreError::Primitive)?;
        let credential =
            PasskeyCredential::from_verified(principal_ref, &request.rp_id, verified, at)
                .map_err(IdentityStoreError::Primitive)?;
        self.identity
            .register_passkey(principal_ref, &credential)
            .await
    }

    pub async fn finish_browser_authentication(
        &self,
        principal_ref: &str,
        challenge_id: Uuid,
        material: &[u8],
        request: &BrowserAssertionRequest,
        at: DateTime<Utc>,
    ) -> Result<PasskeyCredential, IdentityStoreError> {
        let credential = self
            .identity
            .find_passkey(principal_ref, &request.credential_id)
            .await?
            .ok_or(IdentityStoreError::PasskeyNotFound)?;
        if credential.revoked_at().is_some() {
            return Err(IdentityStoreError::PasskeyRevoked);
        }
        let challenge = self
            .identity
            .load_webauthn_challenge(principal_ref, challenge_id, ChallengeCeremony::Assertion)
            .await?;
        let material = ChallengeMaterial::from_slice(material);
        let mut request = request.clone();
        request.state = self
            .identity
            .load_webauthn_provider_state(principal_ref, challenge_id, ChallengeCeremony::Assertion)
            .await?;
        self.identity
            .consume_webauthn_challenge(
                principal_ref,
                challenge_id,
                ChallengeCeremony::Assertion,
                material.as_bytes(),
                at,
            )
            .await?;
        let verified = self
            .provider
            .verify_browser_assertion(
                principal_ref,
                &credential,
                &challenge,
                &material,
                &request,
                at,
            )
            .map_err(IdentityStoreError::Primitive)?;
        if verified.credential_id != credential.credential_id().to_vec() {
            return Err(IdentityStoreError::Primitive(
                IdentityError::WebAuthnCredentialIdMismatch,
            ));
        }
        if verified.sign_count < credential.sign_count() {
            return Err(IdentityStoreError::Primitive(
                IdentityError::CredentialCounterRollback,
            ));
        }
        self.identity
            .compare_and_set_passkey_counter(
                principal_ref,
                credential.credential_id(),
                credential.sign_count(),
                verified.sign_count,
                at,
            )
            .await
    }

    async fn begin(
        &self,
        principal_ref: &str,
        ceremony: ChallengeCeremony,
        at: DateTime<Utc>,
    ) -> Result<PasskeyCeremonyStart, IdentityStoreError> {
        let lifetime = ChallengeLifetime::try_new(PASSKEY_CHALLENGE_LIFETIME)
            .map_err(IdentityStoreError::Primitive)?;
        let (challenge, material) = self
            .identity
            .issue_webauthn_challenge(principal_ref, ceremony, lifetime, at)
            .await?;
        Ok(PasskeyCeremonyStart {
            challenge,
            material,
        })
    }
}
