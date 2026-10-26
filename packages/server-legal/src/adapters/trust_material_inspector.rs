use base64::{Engine, engine::general_purpose::STANDARD};
use der::{DecodePem, Encode};
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use sha2::{Digest, Sha256};
use x509_cert::{Certificate, ext::pkix::BasicConstraints};

use crate::{
    application::trust_management::LegalTrustInspector,
    domain::{pdf_verification::TrustMaterialKind, trust_management::InspectedTrustMaterial},
};

/// Pure Rust certificate metadata parsing; it links no OpenSSL/PDF runtime.
pub struct RustTrustMaterialInspector;
impl LegalTrustInspector for RustTrustMaterialInspector {
    fn inspect(
        &self,
        kind: TrustMaterialKind,
        content: &str,
        now: TimestampMillis,
    ) -> Result<InspectedTrustMaterial, BusinessError> {
        if content.len() > 4_000_000 {
            return Err(invalid("Trust content exceeds its input limit."));
        }
        let bytes = match kind {
            TrustMaterialKind::Root | TrustMaterialKind::Intermediate => {
                let certificate = Certificate::from_pem(content)
                    .map_err(|_| invalid("Use a PEM CA certificate."))?;
                if !certificate
                    .tbs_certificate
                    .get::<BasicConstraints>()
                    .map_err(|_| invalid("Invalid CA certificate."))?
                    .is_some_and(|(_, constraints)| constraints.ca)
                {
                    return Err(invalid("A CA certificate is required."));
                }
                let expires = certificate
                    .tbs_certificate
                    .validity
                    .not_after
                    .to_unix_duration()
                    .as_millis();
                if now.get() < 0 || expires < now.get() as u128 {
                    return Err(invalid("This CA certificate is expired."));
                }
                certificate
                    .to_der()
                    .map_err(|_| invalid("Invalid CA certificate."))?
            }
            TrustMaterialKind::Crl | TrustMaterialKind::Ocsp => {
                if !content
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"+/=\r\n".contains(&byte))
                {
                    return Err(invalid("Upload base64 DER revocation material."));
                }
                let compact: Vec<_> = content
                    .bytes()
                    .filter(|byte| !matches!(byte, b'\r' | b'\n'))
                    .collect();
                STANDARD
                    .decode(compact)
                    .map_err(|_| invalid("Upload base64 DER revocation material."))?
            }
        };
        // Revocation material remains a proposed item for independent review;
        // only the offline verifier can establish usable signature/status proof.
        if bytes.is_empty() || bytes.len() > 2_000_000 {
            return Err(invalid("Trust material exceeds its content limit."));
        }
        Ok(InspectedTrustMaterial {
            checksum_sha256: hex::encode(Sha256::digest(&bytes)),
            content: bytes,
        })
    }
}
fn invalid(message: &'static str) -> BusinessError {
    BusinessError::invalid("LEGAL_INVALID", message)
}
