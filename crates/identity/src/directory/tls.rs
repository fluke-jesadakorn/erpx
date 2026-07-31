//! TLS and endpoint policy for directory connectors.
//!
//! This module deliberately does not implement an LDAP socket.  The concrete
//! adapter must use a certificate-validating TLS implementation and return
//! [`TlsHandshakeEvidence`] after the handshake.  The policy is intentionally
//! unable to represent a plaintext or anonymous fallback.

use std::{fmt, time::Duration};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// The maximum time a directory adapter may spend establishing a TCP
/// connection.  The adapter must apply this value before any LDAP bytes are
/// exchanged.
pub const MAX_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// The maximum time a StartTLS negotiation may take.
pub const MAX_STARTTLS_TIMEOUT: Duration = Duration::from_secs(15);

/// The maximum time for a single bind operation, including the protected
/// transport operation as observed by the authentication service.
pub const MAX_BIND_TIMEOUT: Duration = Duration::from_secs(30);

/// The maximum age of TLS handshake evidence at the authentication boundary.
/// A certificate-valid result is not reusable forever after it has been
/// serialized or passed between tasks.
pub const MAX_HANDSHAKE_EVIDENCE_AGE: ChronoDuration = ChronoDuration::minutes(5);

/// Small clock skew is tolerated for adapter evidence, but a materially
/// future-dated record is rejected rather than becoming a way to extend its
/// lifetime.
pub const MAX_HANDSHAKE_CLOCK_SKEW: ChronoDuration = ChronoDuration::seconds(30);

/// A single peer certificate is small in every supported LDAP/AD/UCS TLS
/// deployment.  Bound the adapter input before hashing it so a malformed or
/// compromised adapter cannot turn the authentication boundary into an
/// unbounded certificate-processing allocation.
pub const MAX_PEER_CERTIFICATE_DER_BYTES: usize = 1024 * 1024;

/// Returns the normalized SHA-256 fingerprint of a DER certificate supplied by
/// a TLS adapter.  The adapter must obtain the bytes from the verified peer
/// connection; this helper does not treat operator-supplied text as evidence.
pub fn sha256_certificate_fingerprint(der_certificate: &[u8]) -> String {
    Sha256::digest(der_certificate)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum TlsPolicyError {
    #[error("directory endpoint must use the configured LDAPS or StartTLS scheme")]
    InsecureOrUnexpectedScheme,
    #[error("directory endpoint is missing a valid server name")]
    InvalidEndpoint,
    #[error("TLS certificate validation is mandatory")]
    CertificateValidationRequired,
    #[error("TLS handshake evidence must include a verified peer certificate")]
    PeerCertificateEvidenceRequired,
    #[error("directory certificate validation failed")]
    CertificateValidationFailed,
    #[error("directory TLS server name does not match the validated certificate")]
    ServerNameMismatch,
    #[error("directory TLS protocol version is below the configured minimum")]
    ProtocolTooOld,
    #[error("directory TLS trust anchor does not match the configured anchor")]
    TrustAnchorMismatch,
    #[error("directory TLS handshake evidence is too old")]
    HandshakeEvidenceExpired,
    #[error("directory TLS handshake evidence is from the future")]
    HandshakeEvidenceFromFuture,
    #[error("a rotated trust anchor must have a strictly higher generation")]
    TrustAnchorRotationRejected,
    #[error("directory timeout is zero or exceeds the safe bound")]
    TimeoutOutOfBounds,
    #[error("certificate fingerprint must be 64 hexadecimal characters")]
    InvalidFingerprint,
    #[error("TLS trust-anchor reference is empty or too long")]
    InvalidTrustAnchorReference,
}

/// The only supported directory transport modes.  `StartTls` means the
/// adapter connects to an LDAP endpoint and must upgrade the connection before
/// sending bind credentials; it is not permission to continue in plaintext.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryTransportMode {
    Ldaps,
    StartTls,
    Https,
}

impl DirectoryTransportMode {
    pub const fn endpoint_scheme(self) -> &'static str {
        match self {
            Self::Ldaps => "ldaps://",
            Self::StartTls => "ldap://",
            Self::Https => "https://",
        }
    }

    pub const fn requires_upgrade(self) -> bool {
        matches!(self, Self::StartTls)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlsProtocolVersion {
    Tls1_2,
    Tls1_3,
}

/// A normalized SHA-256 fingerprint.  A fingerprint is a trust-anchor
/// reference, not a certificate supplied by the user at authentication time.
#[derive(Clone, PartialEq, Eq, Hash, Serialize)]
pub struct CertificateFingerprint(String);

impl CertificateFingerprint {
    pub fn try_new(value: impl Into<String>) -> Result<Self, TlsPolicyError> {
        let value = value.into();
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(TlsPolicyError::InvalidFingerprint);
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for CertificateFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("CertificateFingerprint")
            .field(&self.0)
            .finish()
    }
}

/// Versioned trust material.  Rotation is monotonic so a stale operator
/// update cannot silently restore a retired certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustAnchor {
    reference: String,
    fingerprint: CertificateFingerprint,
    generation: u64,
}

impl TrustAnchor {
    pub fn try_new(
        reference: impl Into<String>,
        fingerprint: CertificateFingerprint,
        generation: u64,
    ) -> Result<Self, TlsPolicyError> {
        let reference = reference.into();
        if reference.trim().is_empty()
            || reference.len() > 128
            || reference.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(TlsPolicyError::InvalidTrustAnchorReference);
        }
        if generation == 0 {
            return Err(TlsPolicyError::TrustAnchorRotationRejected);
        }
        Ok(Self {
            reference,
            fingerprint,
            generation,
        })
    }

    pub fn reference(&self) -> &str {
        &self.reference
    }

    pub fn fingerprint(&self) -> &CertificateFingerprint {
        &self.fingerprint
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }
}

/// Certificate-validating policy for one directory endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TlsPolicy {
    mode: DirectoryTransportMode,
    server_name: String,
    trust_anchor: TrustAnchor,
    minimum_protocol: TlsProtocolVersion,
}

impl TlsPolicy {
    pub fn ldaps(
        server_name: impl Into<String>,
        trust_anchor: TrustAnchor,
    ) -> Result<Self, TlsPolicyError> {
        Self::new(DirectoryTransportMode::Ldaps, server_name, trust_anchor)
    }

    pub fn starttls(
        server_name: impl Into<String>,
        trust_anchor: TrustAnchor,
    ) -> Result<Self, TlsPolicyError> {
        Self::new(DirectoryTransportMode::StartTls, server_name, trust_anchor)
    }

    pub fn https(
        server_name: impl Into<String>,
        trust_anchor: TrustAnchor,
    ) -> Result<Self, TlsPolicyError> {
        Self::new(DirectoryTransportMode::Https, server_name, trust_anchor)
    }

    fn new(
        mode: DirectoryTransportMode,
        server_name: impl Into<String>,
        trust_anchor: TrustAnchor,
    ) -> Result<Self, TlsPolicyError> {
        let server_name = server_name.into();
        if !is_valid_server_name(&server_name) {
            return Err(TlsPolicyError::InvalidEndpoint);
        }
        Ok(Self {
            mode,
            server_name,
            trust_anchor,
            minimum_protocol: TlsProtocolVersion::Tls1_2,
        })
    }

    pub const fn mode(&self) -> DirectoryTransportMode {
        self.mode
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    pub fn trust_anchor(&self) -> &TrustAnchor {
        &self.trust_anchor
    }

    pub const fn minimum_protocol(&self) -> TlsProtocolVersion {
        self.minimum_protocol
    }

    /// Rejects all schemes except the exact scheme associated with this
    /// policy.  An adapter using StartTLS must still call
    /// [`TlsPolicy::validate_handshake`] before sending credentials.
    pub fn validate_endpoint(&self, endpoint: &str) -> Result<(), TlsPolicyError> {
        let scheme = self.mode.endpoint_scheme();
        let Some(after_scheme) = endpoint.strip_prefix(scheme) else {
            return Err(TlsPolicyError::InsecureOrUnexpectedScheme);
        };
        let (authority, path) = if self.mode == DirectoryTransportMode::Https {
            after_scheme
                .split_once('/')
                .map_or((after_scheme, ""), |(authority, path)| (authority, path))
        } else {
            (after_scheme, "")
        };
        if authority.is_empty()
            || authority.contains(['?', '#', '@'])
            || authority.matches(':').count() > 1
            || (self.mode == DirectoryTransportMode::Https
                && (path.len() > 512 || path.contains(['?', '#', '@', '\\'])))
        {
            return Err(TlsPolicyError::InvalidEndpoint);
        }
        let (host, port) = authority
            .rsplit_once(':')
            .map_or((authority, None), |(host, port)| (host, Some(port)));
        if let Some(port) = port
            && port.parse::<u16>().ok().filter(|port| *port != 0).is_none()
        {
            return Err(TlsPolicyError::InvalidEndpoint);
        }
        if host.is_empty() || !host.eq_ignore_ascii_case(&self.server_name) {
            return Err(TlsPolicyError::InvalidEndpoint);
        }
        Ok(())
    }

    /// Validates evidence produced by the concrete rustls/native TLS adapter.
    /// A boolean certificate-valid flag is never sufficient by itself; the
    /// peer-certificate digest and adapter provenance are mandatory. False,
    /// mismatched names, old protocols, or stale anchors all fail closed.
    pub fn validate_handshake(
        &self,
        evidence: &TlsHandshakeEvidence,
    ) -> Result<(), TlsPolicyError> {
        self.validate_handshake_at(evidence, Utc::now())
    }

    /// Validates handshake evidence against an explicit boundary time.  The
    /// explicit form keeps callers and tests from trusting an adapter- or
    /// caller-supplied clock while retaining a convenient wall-clock API.
    pub fn validate_handshake_at(
        &self,
        evidence: &TlsHandshakeEvidence,
        now: DateTime<Utc>,
    ) -> Result<(), TlsPolicyError> {
        if evidence.observed_at < now - MAX_HANDSHAKE_EVIDENCE_AGE {
            return Err(TlsPolicyError::HandshakeEvidenceExpired);
        }
        if evidence.observed_at > now + MAX_HANDSHAKE_CLOCK_SKEW {
            return Err(TlsPolicyError::HandshakeEvidenceFromFuture);
        }
        if evidence.mode != self.mode {
            return Err(TlsPolicyError::InsecureOrUnexpectedScheme);
        }
        if !evidence.certificate_validated {
            return Err(TlsPolicyError::CertificateValidationRequired);
        }
        if evidence.source != HandshakeEvidenceSource::AdapterVerified
            || evidence.peer_certificate_fingerprint.is_none()
        {
            return Err(TlsPolicyError::PeerCertificateEvidenceRequired);
        }
        // The adapter must prove the concrete peer certificate that was
        // presented by the verified connection is the certificate pinned by
        // this connector.  Merely copying the configured trust-anchor fields
        // into evidence is insufficient: otherwise a validly-shaped
        // handshake from a different certificate could cross this boundary.
        if evidence.peer_certificate_fingerprint.as_ref() != Some(&self.trust_anchor.fingerprint) {
            return Err(TlsPolicyError::TrustAnchorMismatch);
        }
        if !evidence.server_name.eq_ignore_ascii_case(&self.server_name) {
            return Err(TlsPolicyError::ServerNameMismatch);
        }
        if evidence.negotiated_protocol < self.minimum_protocol {
            return Err(TlsPolicyError::ProtocolTooOld);
        }
        if evidence.trust_anchor_reference != self.trust_anchor.reference
            || evidence.trust_anchor_fingerprint != self.trust_anchor.fingerprint
            || evidence.trust_anchor_generation != self.trust_anchor.generation
        {
            return Err(TlsPolicyError::TrustAnchorMismatch);
        }
        Ok(())
    }

    /// Returns a new policy only for a monotonic trust-anchor rotation.  The
    /// old policy remains valid for already-established sessions, while new
    /// binds must use the returned policy.
    pub fn rotate_trust_anchor(&self, next: TrustAnchor) -> Result<Self, TlsPolicyError> {
        if next.generation <= self.trust_anchor.generation {
            return Err(TlsPolicyError::TrustAnchorRotationRejected);
        }
        Ok(Self {
            mode: self.mode,
            server_name: self.server_name.clone(),
            trust_anchor: next,
            minimum_protocol: self.minimum_protocol,
        })
    }
}

/// Evidence supplied by the network adapter after completing LDAPS or
/// StartTLS.  The adapter must not return a successful bind without this
/// evidence.  A bare certificate-valid boolean is deliberately insufficient:
/// successful evidence must carry a digest of the peer certificate obtained
/// from the verified TLS connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TlsHandshakeEvidence {
    mode: DirectoryTransportMode,
    server_name: String,
    negotiated_protocol: TlsProtocolVersion,
    observed_at: DateTime<Utc>,
    certificate_validated: bool,
    peer_certificate_fingerprint: Option<CertificateFingerprint>,
    trust_anchor_reference: String,
    trust_anchor_fingerprint: CertificateFingerprint,
    trust_anchor_generation: u64,
    #[serde(skip)]
    source: HandshakeEvidenceSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HandshakeEvidenceSource {
    AdapterVerified,
    #[cfg(test)]
    Synthetic,
}

impl TlsHandshakeEvidence {
    /// Constructs evidence from the concrete adapter's completed,
    /// certificate-validating TLS connection.  `peer_certificate_der` must be
    /// the DER bytes of the certificate presented by that connection; the
    /// identity boundary stores only its digest.  The adapter remains
    /// responsible for validating the chain, hostname, and configured trust
    /// anchor before calling this constructor.
    pub fn from_verified_peer(
        mode: DirectoryTransportMode,
        server_name: impl Into<String>,
        negotiated_protocol: TlsProtocolVersion,
        peer_certificate_der: &[u8],
        trust_anchor: &TrustAnchor,
    ) -> Result<Self, TlsPolicyError> {
        Self::from_verified_peer_at(
            mode,
            server_name,
            negotiated_protocol,
            peer_certificate_der,
            trust_anchor,
            Utc::now(),
        )
    }

    /// Adapter entry point for a handshake whose observation time is known.
    /// Production adapters should pass the time captured immediately after
    /// their certificate-validating handshake; the policy then bounds its age
    /// at the authentication decision.
    pub fn from_verified_peer_at(
        mode: DirectoryTransportMode,
        server_name: impl Into<String>,
        negotiated_protocol: TlsProtocolVersion,
        peer_certificate_der: &[u8],
        trust_anchor: &TrustAnchor,
        observed_at: DateTime<Utc>,
    ) -> Result<Self, TlsPolicyError> {
        let server_name = server_name.into();
        if !is_valid_server_name(&server_name) {
            return Err(TlsPolicyError::InvalidEndpoint);
        }
        if peer_certificate_der.is_empty()
            || peer_certificate_der.len() > MAX_PEER_CERTIFICATE_DER_BYTES
        {
            return Err(TlsPolicyError::PeerCertificateEvidenceRequired);
        }
        Ok(Self {
            mode,
            server_name,
            negotiated_protocol,
            observed_at,
            certificate_validated: true,
            peer_certificate_fingerprint: Some(CertificateFingerprint::try_new(
                sha256_certificate_fingerprint(peer_certificate_der),
            )?),
            trust_anchor_reference: trust_anchor.reference.clone(),
            trust_anchor_fingerprint: trust_anchor.fingerprint.clone(),
            trust_anchor_generation: trust_anchor.generation,
            source: HandshakeEvidenceSource::AdapterVerified,
        })
    }

    /// Constructs deliberately synthetic evidence for negative-path tests.
    /// This constructor is unavailable in production builds, and policy
    /// validation rejects its result even when the test supplies
    /// `certificate_validated = true`.
    #[cfg(test)]
    pub(crate) fn synthetic(
        mode: DirectoryTransportMode,
        server_name: impl Into<String>,
        negotiated_protocol: TlsProtocolVersion,
        certificate_validated: bool,
        trust_anchor: &TrustAnchor,
    ) -> Self {
        Self {
            mode,
            server_name: server_name.into(),
            negotiated_protocol,
            observed_at: Utc::now(),
            certificate_validated,
            peer_certificate_fingerprint: None,
            trust_anchor_reference: trust_anchor.reference.clone(),
            trust_anchor_fingerprint: trust_anchor.fingerprint.clone(),
            trust_anchor_generation: trust_anchor.generation,
            source: HandshakeEvidenceSource::Synthetic,
        }
    }

    #[cfg(test)]
    pub(crate) fn unvalidated(
        mode: DirectoryTransportMode,
        server_name: impl Into<String>,
        trust_anchor: &TrustAnchor,
    ) -> Self {
        Self::synthetic(
            mode,
            server_name,
            TlsProtocolVersion::Tls1_2,
            false,
            trust_anchor,
        )
    }

    pub const fn mode(&self) -> DirectoryTransportMode {
        self.mode
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    pub const fn negotiated_protocol(&self) -> TlsProtocolVersion {
        self.negotiated_protocol
    }

    pub const fn observed_at(&self) -> DateTime<Utc> {
        self.observed_at
    }

    pub const fn certificate_validated(&self) -> bool {
        self.certificate_validated
    }

    pub fn peer_certificate_fingerprint(&self) -> Option<&CertificateFingerprint> {
        self.peer_certificate_fingerprint.as_ref()
    }

    pub fn trust_anchor_reference(&self) -> &str {
        &self.trust_anchor_reference
    }

    pub fn trust_anchor_fingerprint(&self) -> &CertificateFingerprint {
        &self.trust_anchor_fingerprint
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeoutPolicy {
    connect: Duration,
    starttls: Duration,
    bind: Duration,
}

impl TimeoutPolicy {
    pub fn try_new(
        connect: Duration,
        starttls: Duration,
        bind: Duration,
    ) -> Result<Self, TlsPolicyError> {
        if connect.is_zero()
            || connect > MAX_CONNECT_TIMEOUT
            || starttls.is_zero()
            || starttls > MAX_STARTTLS_TIMEOUT
            || bind.is_zero()
            || bind > MAX_BIND_TIMEOUT
        {
            return Err(TlsPolicyError::TimeoutOutOfBounds);
        }
        Ok(Self {
            connect,
            starttls,
            bind,
        })
    }

    pub const fn conservative() -> Self {
        Self {
            connect: Duration::from_secs(10),
            starttls: Duration::from_secs(10),
            bind: Duration::from_secs(15),
        }
    }

    pub const fn connect(self) -> Duration {
        self.connect
    }

    pub const fn starttls(self) -> Duration {
        self.starttls
    }

    pub const fn bind(self) -> Duration {
        self.bind
    }
}

fn is_valid_server_name(value: &str) -> bool {
    if value.is_empty() || value.len() > 253 || value.contains(['/', ':', '@', ' ']) {
        return false;
    }
    value.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(reference: &str, generation: u64, digit: char) -> TrustAnchor {
        TrustAnchor::try_new(
            reference,
            CertificateFingerprint::try_new(digit.to_string().repeat(64)).expect("fingerprint"),
            generation,
        )
        .expect("anchor")
    }

    #[test]
    fn endpoint_policy_rejects_plaintext_and_wrong_server() {
        let anchor = anchor("directory-ca-v1", 1, 'a');
        let policy = TlsPolicy::ldaps("ad.example.test", anchor).expect("policy");

        assert_eq!(
            policy.validate_endpoint("ldap://ad.example.test:389"),
            Err(TlsPolicyError::InsecureOrUnexpectedScheme)
        );
        assert_eq!(
            policy.validate_endpoint("ldaps://other.example.test:636"),
            Err(TlsPolicyError::InvalidEndpoint)
        );
        assert!(policy.validate_endpoint("ldaps://ad.example.test").is_ok());
        assert!(
            policy
                .validate_endpoint("ldaps://ad.example.test:1636")
                .is_ok()
        );
        for malformed in [
            "ldaps://ad.example.test:636/path",
            "ldaps://ad.example.test:636@other.example.test",
            "ldaps://ad.example.test:",
            "ldaps://ad.example.test:0",
            "ldaps://ad.example.test:ldap",
        ] {
            assert_eq!(
                policy.validate_endpoint(malformed),
                Err(TlsPolicyError::InvalidEndpoint),
                "malformed endpoint must be rejected: {malformed}"
            );
        }
    }

    #[test]
    fn unvalidated_or_stale_handshakes_fail_closed() {
        let old = anchor("directory-ca-v1", 1, 'a');
        let policy = TlsPolicy::ldaps("ldap.example.test", old.clone()).expect("policy");
        assert_eq!(
            policy.validate_handshake(&TlsHandshakeEvidence::unvalidated(
                DirectoryTransportMode::Ldaps,
                "ldap.example.test",
                &old,
            )),
            Err(TlsPolicyError::CertificateValidationRequired)
        );

        let rotated = TrustAnchor::try_new(
            "directory-ca-v2",
            CertificateFingerprint::try_new(sha256_certificate_fingerprint(
                b"current-verified-peer-certificate",
            ))
            .expect("fingerprint"),
            2,
        )
        .expect("rotated anchor");
        let next = policy
            .rotate_trust_anchor(rotated.clone())
            .expect("monotonic rotation");
        assert_eq!(
            next.validate_handshake(
                &TlsHandshakeEvidence::from_verified_peer(
                    DirectoryTransportMode::Ldaps,
                    "ldap.example.test",
                    TlsProtocolVersion::Tls1_3,
                    b"stale-verified-peer-certificate",
                    &old,
                )
                .expect("verified evidence"),
            ),
            Err(TlsPolicyError::TrustAnchorMismatch)
        );
        assert!(
            next.validate_handshake(
                &TlsHandshakeEvidence::from_verified_peer(
                    DirectoryTransportMode::Ldaps,
                    "ldap.example.test",
                    TlsProtocolVersion::Tls1_3,
                    b"current-verified-peer-certificate",
                    &rotated,
                )
                .expect("verified evidence"),
            )
            .is_ok()
        );
        assert_eq!(
            next.rotate_trust_anchor(old),
            Err(TlsPolicyError::TrustAnchorRotationRejected)
        );
    }

    #[test]
    fn a_validity_boolean_without_peer_certificate_evidence_fails_closed() {
        let anchor = anchor("directory-ca-v1", 1, 'd');
        let policy = TlsPolicy::ldaps("ldap.example.test", anchor.clone()).expect("policy");
        let synthetic = TlsHandshakeEvidence::synthetic(
            DirectoryTransportMode::Ldaps,
            "ldap.example.test",
            TlsProtocolVersion::Tls1_3,
            true,
            &anchor,
        );

        assert_eq!(
            policy.validate_handshake(&synthetic),
            Err(TlsPolicyError::PeerCertificateEvidenceRequired)
        );
        assert_eq!(
            TlsHandshakeEvidence::from_verified_peer(
                DirectoryTransportMode::Ldaps,
                "ldap.example.test",
                TlsProtocolVersion::Tls1_3,
                &[],
                &anchor,
            ),
            Err(TlsPolicyError::PeerCertificateEvidenceRequired)
        );
    }

    #[test]
    fn verified_peer_evidence_retains_only_a_certificate_digest() {
        let anchor = anchor("directory-ca-v1", 1, 'e');
        let certificate = b"verified-peer-certificate";
        let evidence = TlsHandshakeEvidence::from_verified_peer(
            DirectoryTransportMode::Ldaps,
            "ldap.example.test",
            TlsProtocolVersion::Tls1_3,
            certificate,
            &anchor,
        )
        .expect("evidence");

        assert!(evidence.certificate_validated());
        assert_eq!(
            evidence
                .peer_certificate_fingerprint()
                .expect("peer fingerprint")
                .as_str(),
            sha256_certificate_fingerprint(certificate)
        );
        assert!(
            !serde_json::to_string(&evidence)
                .expect("evidence JSON")
                .contains("verified-peer-certificate")
        );
    }

    #[test]
    fn verified_peer_evidence_is_bounded_and_server_name_validated() {
        let anchor = anchor("directory-ca-v1", 1, 'b');
        let oversized = vec![0_u8; MAX_PEER_CERTIFICATE_DER_BYTES + 1];

        assert_eq!(
            TlsHandshakeEvidence::from_verified_peer(
                DirectoryTransportMode::Ldaps,
                "ldap.example.test",
                TlsProtocolVersion::Tls1_3,
                &oversized,
                &anchor,
            ),
            Err(TlsPolicyError::PeerCertificateEvidenceRequired)
        );
        assert_eq!(
            TlsHandshakeEvidence::from_verified_peer(
                DirectoryTransportMode::Ldaps,
                "ldap.example.test/attacker",
                TlsProtocolVersion::Tls1_3,
                b"peer-certificate",
                &anchor,
            ),
            Err(TlsPolicyError::InvalidEndpoint)
        );
    }

    #[test]
    fn a_verified_peer_certificate_from_another_anchor_fails_closed() {
        let anchor = anchor("directory-ca-v1", 1, 'f');
        let policy = TlsPolicy::ldaps("ldap.example.test", anchor.clone()).expect("policy");
        let evidence = TlsHandshakeEvidence::from_verified_peer(
            DirectoryTransportMode::Ldaps,
            "ldap.example.test",
            TlsProtocolVersion::Tls1_3,
            b"different-pinned-peer-certificate",
            &anchor,
        )
        .expect("well-formed adapter evidence");

        assert_eq!(
            policy.validate_handshake(&evidence),
            Err(TlsPolicyError::TrustAnchorMismatch)
        );
    }

    #[test]
    fn handshake_evidence_is_bounded_by_observation_time() {
        let anchor = anchor("directory-ca-v1", 1, 'f');
        let policy = TlsPolicy::ldaps("ldap.example.test", anchor.clone()).expect("policy");
        let now = Utc::now();

        let stale = TlsHandshakeEvidence::from_verified_peer_at(
            DirectoryTransportMode::Ldaps,
            "ldap.example.test",
            TlsProtocolVersion::Tls1_3,
            b"stale-peer-certificate",
            &anchor,
            now - MAX_HANDSHAKE_EVIDENCE_AGE - ChronoDuration::seconds(1),
        )
        .expect("evidence");
        assert_eq!(
            policy.validate_handshake_at(&stale, now),
            Err(TlsPolicyError::HandshakeEvidenceExpired)
        );

        let future = TlsHandshakeEvidence::from_verified_peer_at(
            DirectoryTransportMode::Ldaps,
            "ldap.example.test",
            TlsProtocolVersion::Tls1_3,
            b"future-peer-certificate",
            &anchor,
            now + MAX_HANDSHAKE_CLOCK_SKEW + ChronoDuration::seconds(1),
        )
        .expect("evidence");
        assert_eq!(
            policy.validate_handshake_at(&future, now),
            Err(TlsPolicyError::HandshakeEvidenceFromFuture)
        );
    }

    #[test]
    fn starttls_requires_the_upgrade_mode_and_bounded_timeouts() {
        let anchor = anchor("directory-ca-v1", 1, 'c');
        let policy = TlsPolicy::starttls("ucs.example.test", anchor).expect("policy");
        assert!(
            policy
                .validate_endpoint("ldap://ucs.example.test:389")
                .is_ok()
        );
        assert_eq!(
            policy.validate_endpoint("ldaps://ucs.example.test:636"),
            Err(TlsPolicyError::InsecureOrUnexpectedScheme)
        );
        assert_eq!(
            TimeoutPolicy::try_new(
                Duration::ZERO,
                Duration::from_secs(1),
                Duration::from_secs(1)
            ),
            Err(TlsPolicyError::TimeoutOutOfBounds)
        );
        assert_eq!(
            TimeoutPolicy::try_new(
                Duration::from_secs(1),
                Duration::from_secs(1),
                MAX_BIND_TIMEOUT + Duration::from_secs(1)
            ),
            Err(TlsPolicyError::TimeoutOutOfBounds)
        );
    }
}
