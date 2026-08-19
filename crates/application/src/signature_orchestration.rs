use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_kernel::AuthorityScope;
use erpx_legal::{
    CallbackOutcome, ESignatureProviderPort, SignatureCallbackVerifierPort, SignatureEnvelope,
    SignatureEnvelopeId, SignatureEvidencePackage, SignatureProviderEnvelope,
    SignatureProviderSubmission,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SignatureOrchestrationError {
    #[error("signature envelope was not found")]
    NotFound,
    #[error("signature authority or provider policy is denied")]
    Denied,
    #[error("signature envelope version conflict")]
    Conflict,
    #[error("signature provider or evidence is unavailable")]
    Unavailable,
    #[error("signature lifecycle rejected the operation")]
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureSubmissionAuthority {
    pub principal_ref: String,
    pub scope: AuthorityScope,
    pub assignment_version: u64,
    pub valid_until: DateTime<Utc>,
    pub provider_allowed: bool,
    pub provider_policy_sha256: String,
    pub authority_sha256: String,
}

#[async_trait]
pub trait SignatureEnvelopeRepositoryPort: Send + Sync {
    async fn load(
        &self,
        envelope_id: SignatureEnvelopeId,
    ) -> Result<Option<SignatureEnvelope>, SignatureOrchestrationError>;

    async fn load_by_provider_ref(
        &self,
        provider_envelope_ref: &str,
    ) -> Result<Option<SignatureEnvelope>, SignatureOrchestrationError>;

    async fn save(
        &self,
        envelope: &SignatureEnvelope,
        expected_version: u64,
        evidence_sha256: &str,
    ) -> Result<(), SignatureOrchestrationError>;
}

pub struct SignatureOrchestrationService<R, P, V> {
    repository: R,
    provider: P,
    verifier: V,
}

impl<R, P, V> SignatureOrchestrationService<R, P, V> {
    pub const fn new(repository: R, provider: P, verifier: V) -> Self {
        Self {
            repository,
            provider,
            verifier,
        }
    }
}

impl<R, P, V> SignatureOrchestrationService<R, P, V>
where
    R: SignatureEnvelopeRepositoryPort,
    P: ESignatureProviderPort,
    V: SignatureCallbackVerifierPort,
{
    pub async fn submit(
        &self,
        envelope_id: SignatureEnvelopeId,
        expected_version: u64,
        request: SignatureProviderSubmission,
        authority: SignatureSubmissionAuthority,
        at: DateTime<Utc>,
    ) -> Result<SignatureProviderEnvelope, SignatureOrchestrationError> {
        let mut envelope = self
            .repository
            .load(envelope_id)
            .await?
            .ok_or(SignatureOrchestrationError::NotFound)?;
        if envelope.version() != expected_version {
            return Err(SignatureOrchestrationError::Conflict);
        }
        if !authority.provider_allowed
            || authority.scope != *envelope.scope()
            || authority.scope != request.scope
            || authority.assignment_version == 0
            || authority.valid_until <= at
            || !valid_digest(&authority.provider_policy_sha256)
            || !valid_digest(&authority.authority_sha256)
            || request.envelope_id != envelope.id()
            || request.document_id != envelope.document_id()
            || request.document_version != envelope.document_version()
            || request.document_sha256 != envelope.document_sha256()
            || request.recipients != envelope.recipients()
        {
            return Err(SignatureOrchestrationError::Denied);
        }

        // Providers must bind their idempotency key to `envelope_id`. A retry
        // after an uncertain save therefore reconciles the same provider
        // envelope and never creates a second signing campaign.
        let provider = self
            .provider
            .create_envelope(request)
            .await
            .map_err(|_| SignatureOrchestrationError::Unavailable)?;
        envelope
            .submit(provider.provider_envelope_ref.clone(), at)
            .map_err(|_| SignatureOrchestrationError::Invalid)?;
        let evidence = signature_submission_digest(
            envelope.id(),
            expected_version,
            &provider.provider_envelope_ref,
            &authority,
        );
        self.repository
            .save(&envelope, expected_version, &evidence)
            .await?;
        Ok(provider)
    }

    pub async fn reconcile_callback(
        &self,
        provider_key: &str,
        headers: &[(String, String)],
        body: &[u8],
        received_at: DateTime<Utc>,
    ) -> Result<CallbackOutcome, SignatureOrchestrationError> {
        let callback = self
            .verifier
            .verify_callback(provider_key, headers, body, received_at)
            .await
            .map_err(|_| SignatureOrchestrationError::Denied)?;
        let mut envelope = self
            .repository
            .load_by_provider_ref(&callback.provider_envelope_ref)
            .await?
            .ok_or(SignatureOrchestrationError::NotFound)?;
        let expected = envelope.version();
        let outcome = envelope
            .apply_callback(callback, received_at)
            .map_err(|_| SignatureOrchestrationError::Invalid)?;
        if outcome == CallbackOutcome::Applied {
            use sha2::{Digest, Sha256};
            let evidence = format!("sha256:{}", hex::encode(Sha256::digest(body)));
            self.repository.save(&envelope, expected, &evidence).await?;
        }
        Ok(outcome)
    }

    pub async fn export_evidence(
        &self,
        envelope_id: SignatureEnvelopeId,
        authority: &SignatureSubmissionAuthority,
        at: DateTime<Utc>,
    ) -> Result<SignatureEvidencePackage, SignatureOrchestrationError> {
        let envelope = self
            .repository
            .load(envelope_id)
            .await?
            .ok_or(SignatureOrchestrationError::NotFound)?;
        if authority.scope != *envelope.scope()
            || authority.valid_until <= at
            || authority.assignment_version == 0
        {
            return Err(SignatureOrchestrationError::Denied);
        }
        let provider_ref = envelope
            .provider_envelope_ref()
            .ok_or(SignatureOrchestrationError::Conflict)?;
        let package = self
            .provider
            .fetch_evidence(provider_ref)
            .await
            .map_err(|_| SignatureOrchestrationError::Unavailable)?;
        if package.provider_envelope_ref != provider_ref
            || !valid_digest(&package.evidence_sha256)
            || envelope.evidence_sha256() != Some(package.evidence_sha256.as_str())
        {
            return Err(SignatureOrchestrationError::Invalid);
        }
        Ok(package)
    }
}

fn signature_submission_digest(
    envelope_id: SignatureEnvelopeId,
    expected_version: u64,
    provider_ref: &str,
    authority: &SignatureSubmissionAuthority,
) -> String {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(&(
        envelope_id,
        expected_version,
        provider_ref,
        &authority.provider_policy_sha256,
        &authority.authority_sha256,
    ))
    .expect("serializable signature submission evidence");
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn valid_digest(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use chrono::Duration;
    use erpx_legal::{
        LegalDocumentId, PortError, SignatureOrder, SignatureRecipient, SignatureRecipientId,
        SignatureSubject, VerifiedSignatureCallback,
    };
    use uuid::Uuid;

    use super::*;

    #[derive(Clone)]
    struct Repository(Arc<Mutex<SignatureEnvelope>>);
    #[async_trait]
    impl SignatureEnvelopeRepositoryPort for Repository {
        async fn load(
            &self,
            _id: SignatureEnvelopeId,
        ) -> Result<Option<SignatureEnvelope>, SignatureOrchestrationError> {
            Ok(Some(self.0.lock().unwrap().clone()))
        }
        async fn load_by_provider_ref(
            &self,
            _provider_ref: &str,
        ) -> Result<Option<SignatureEnvelope>, SignatureOrchestrationError> {
            Ok(Some(self.0.lock().unwrap().clone()))
        }
        async fn save(
            &self,
            envelope: &SignatureEnvelope,
            expected_version: u64,
            evidence_sha256: &str,
        ) -> Result<(), SignatureOrchestrationError> {
            if self.0.lock().unwrap().version() != expected_version
                || !valid_digest(evidence_sha256)
            {
                return Err(SignatureOrchestrationError::Conflict);
            }
            *self.0.lock().unwrap() = envelope.clone();
            Ok(())
        }
    }

    #[derive(Clone)]
    struct Provider(Arc<AtomicUsize>);
    #[async_trait]
    impl ESignatureProviderPort for Provider {
        async fn create_envelope(
            &self,
            _request: SignatureProviderSubmission,
        ) -> Result<SignatureProviderEnvelope, PortError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(SignatureProviderEnvelope {
                provider_envelope_ref: "provider-envelope-1".into(),
            })
        }
        async fn cancel_envelope(&self, _provider_envelope_ref: &str) -> Result<(), PortError> {
            Ok(())
        }
        async fn fetch_evidence(
            &self,
            provider_envelope_ref: &str,
        ) -> Result<SignatureEvidencePackage, PortError> {
            Ok(SignatureEvidencePackage {
                provider_envelope_ref: provider_envelope_ref.into(),
                evidence_object_ref: "legal/evidence/1".into(),
                evidence_sha256: format!("sha256:{}", "e".repeat(64)),
            })
        }
    }
    struct Verifier;
    #[async_trait]
    impl SignatureCallbackVerifierPort for Verifier {
        async fn verify_callback(
            &self,
            _provider_key: &str,
            _headers: &[(String, String)],
            _body: &[u8],
            _received_at: DateTime<Utc>,
        ) -> Result<VerifiedSignatureCallback, PortError> {
            Err(PortError::Denied)
        }
    }

    fn at() -> DateTime<Utc> {
        DateTime::from_timestamp(1_786_000_000, 0).unwrap()
    }
    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }
    fn fixture() -> (SignatureEnvelope, SignatureProviderSubmission) {
        let recipient = SignatureRecipient::try_new(
            SignatureRecipientId::try_from_uuid(Uuid::from_u128(3)).unwrap(),
            SignatureSubject::Internal {
                principal_ref: "signer".into(),
            },
            1,
        )
        .unwrap();
        let id = SignatureEnvelopeId::try_from_uuid(Uuid::from_u128(4)).unwrap();
        let document_id = LegalDocumentId::try_from_uuid(Uuid::from_u128(5)).unwrap();
        let digest = format!("sha256:{}", "d".repeat(64));
        let envelope = SignatureEnvelope::create(
            id,
            scope(),
            document_id,
            7,
            digest.clone(),
            "docusign".into(),
            "provider-policy-v2".into(),
            SignatureOrder::Serial,
            vec![recipient.clone()],
            at() + Duration::days(365),
            "legal-owner".into(),
            at(),
        )
        .unwrap();
        let request = SignatureProviderSubmission {
            envelope_id: id,
            scope: scope(),
            document_id,
            document_version: 7,
            document_sha256: digest,
            provider_policy_version: "provider-policy-v2".into(),
            recipients: vec![recipient],
        };
        (envelope, request)
    }

    #[tokio::test]
    async fn authority_is_checked_before_provider_side_effect() {
        let (envelope, request) = fixture();
        let calls = Arc::new(AtomicUsize::new(0));
        let service = SignatureOrchestrationService::new(
            Repository(Arc::new(Mutex::new(envelope))),
            Provider(calls.clone()),
            Verifier,
        );
        let result = service
            .submit(
                request.envelope_id,
                1,
                request,
                SignatureSubmissionAuthority {
                    principal_ref: "legal-owner".into(),
                    scope: scope(),
                    assignment_version: 3,
                    valid_until: at() + Duration::hours(1),
                    provider_allowed: false,
                    provider_policy_sha256: format!("sha256:{}", "a".repeat(64)),
                    authority_sha256: format!("sha256:{}", "b".repeat(64)),
                },
                at() + Duration::minutes(1),
            )
            .await;
        assert_eq!(result, Err(SignatureOrchestrationError::Denied));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
