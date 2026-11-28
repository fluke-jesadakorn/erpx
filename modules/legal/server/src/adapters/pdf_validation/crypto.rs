use super::trust::OperatorTrustStore;
use crate::domain::pdf_validation::{SignatureValidation, ValidationState};
use cms::{
    cert::CertificateChoices,
    content_info::ContentInfo,
    signed_data::{SignedData, SignerIdentifier, SignerInfo},
};
use der::{Any, Decode, Encode, Sequence, asn1::OctetString};
use openssl::{
    asn1::{Asn1Time, Asn1TimeRef},
    cms::{CMSOptions, CmsContentInfo},
    hash::{MessageDigest, hash},
    ocsp::{OcspCertId, OcspCertStatus, OcspFlag, OcspResponseStatus},
    stack::Stack,
    x509::{
        X509, X509PurposeId, X509Ref, X509StoreContext,
        store::{X509Store, X509StoreBuilder},
        verify::X509VerifyParam,
    },
};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use time::{Duration, OffsetDateTime, PrimitiveDateTime, format_description::well_known::Rfc3339};
use x509_cert::{Certificate, spki::AlgorithmIdentifierOwned};
use x509_tsp::TstInfo;

const SIGNED_DATA: &str = "1.2.840.113549.1.7.2";
const DATA: &str = "1.2.840.113549.1.7.1";
const TIMESTAMP_TOKEN: &str = "1.2.840.113549.1.9.16.2.14";
const TST_INFO: &str = "1.2.840.113549.1.9.16.1.4";

fn parsed_cms(bytes: &[u8]) -> Result<(SignedData, Vec<X509>, X509), &'static str> {
    let content = ContentInfo::from_der(bytes).map_err(|_| "INVALID_SIGNATURE_CMS")?;
    if content.content_type.to_string() != SIGNED_DATA {
        return Err("UNSUPPORTED_CMS_CONTENT_TYPE");
    }
    let signed = SignedData::from_der(
        &content
            .content
            .to_der()
            .map_err(|_| "INVALID_SIGNATURE_CMS")?,
    )
    .map_err(|_| "INVALID_SIGNATURE_CMS")?;
    if signed.signer_infos.0.len() != 1 {
        return Err("UNSUPPORTED_CMS_SIGNER_COUNT");
    }
    let signer = signed.signer_infos.0.get(0).ok_or("MISSING_CMS_SIGNER")?;
    let mut certificates = Vec::new();
    let mut matching = Vec::new();
    for choice in signed
        .certificates
        .as_ref()
        .ok_or("MISSING_SIGNER_CERTIFICATE")?
        .0
        .iter()
    {
        let CertificateChoices::Certificate(certificate) = choice else {
            continue;
        };
        let der = certificate
            .to_der()
            .map_err(|_| "INVALID_SIGNER_CERTIFICATE")?;
        let certificate_x509 = X509::from_der(&der).map_err(|_| "INVALID_SIGNER_CERTIFICATE")?;
        let matches = match &signer.sid {
            SignerIdentifier::IssuerAndSerialNumber(id) => {
                certificate.tbs_certificate.issuer == id.issuer
                    && certificate.tbs_certificate.serial_number == id.serial_number
            }
            SignerIdentifier::SubjectKeyIdentifier(id) => certificate_x509
                .subject_key_id()
                .is_some_and(|key| key.as_slice() == id.0.as_bytes()),
        };
        if matches {
            matching.push(certificate_x509.clone());
        }
        certificates.push(certificate_x509);
    }
    if matching.len() != 1 {
        return Err("AMBIGUOUS_OR_MISSING_SIGNER_CERTIFICATE");
    }
    Ok((signed, certificates, matching.remove(0)))
}

pub(super) fn seed_cms_evidence(bytes: &[u8]) -> Result<(String, bool), &'static str> {
    let (signed, _, _) = parsed_cms(bytes)?;
    let signer = signed.signer_infos.0.get(0).ok_or("MISSING_CMS_SIGNER")?;
    let digest = match signer.digest_alg.oid.to_string().as_str() {
        "2.16.840.1.101.3.4.2.1" => "sha256",
        "2.16.840.1.101.3.4.2.2" => "sha384",
        "2.16.840.1.101.3.4.2.3" => "sha512",
        "2.16.840.1.101.3.4.2.4" => "sha224",
        "1.3.14.3.2.26" => "sha1",
        "1.2.840.113549.2.5" => "md5",
        _ => return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED"),
    };
    let adobe_revocation = signer.signed_attrs.as_ref().is_some_and(|attributes| {
        attributes
            .iter()
            .any(|attribute| attribute.oid.to_string() == "1.2.840.113583.1.1.8")
    });
    Ok((digest.to_owned(), adobe_revocation))
}

pub(super) fn seed_signer_certificate(bytes: &[u8]) -> Result<Certificate, &'static str> {
    let (_, _, signer) = parsed_cms(bytes)?;
    Certificate::from_der(
        &signer
            .to_der()
            .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?,
    )
    .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")
}

pub(super) fn seed_authorities(
    bytes: &[u8],
    trust: &OperatorTrustStore,
    moment: OffsetDateTime,
) -> Result<Vec<Certificate>, &'static str> {
    let (_, embedded, signer) = parsed_cms(bytes)?;
    chain(&signer, &embedded, trust, moment, false)?
        .iter()
        .skip(1)
        .map(|certificate| {
            Certificate::from_der(
                &certificate
                    .to_der()
                    .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?,
            )
            .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")
        })
        .collect()
}

pub(super) fn seed_subject_attributes(bytes: &[u8]) -> Result<Vec<(String, String)>, &'static str> {
    let (_, _, signer) = parsed_cms(bytes)?;
    let typed = seed_signer_certificate(bytes)?;
    let identifiers: Vec<_> = typed
        .tbs_certificate
        .subject
        .0
        .iter()
        .flat_map(|rdn| rdn.0.iter())
        .map(|attribute| attribute.oid.to_string())
        .collect();
    let values = signer
        .subject_name()
        .entries()
        .map(|entry| {
            entry
                .data()
                .to_string()
                .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if identifiers.len() != values.len() {
        return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED");
    }
    Ok(identifiers.into_iter().zip(values).collect())
}

fn digest(oid: &str) -> Option<MessageDigest> {
    match oid {
        "2.16.840.1.101.3.4.2.1" => Some(MessageDigest::sha256()),
        "2.16.840.1.101.3.4.2.2" => Some(MessageDigest::sha384()),
        "2.16.840.1.101.3.4.2.3" => Some(MessageDigest::sha512()),
        "2.16.840.1.101.3.4.2.4" => Some(MessageDigest::sha224()),
        "1.3.14.3.2.26" => Some(MessageDigest::sha1()),
        "1.2.840.113549.2.5" => Some(MessageDigest::md5()),
        _ => None,
    }
}

fn weak_digest(oid: &str) -> bool {
    ["1.3.14.3.2.26", "1.2.840.113549.2.2", "1.2.840.113549.2.5"].contains(&oid)
}

#[derive(Sequence)]
struct PssParameters {
    #[asn1(context_specific = "0", tag_mode = "EXPLICIT", optional = "true")]
    hash: Option<AlgorithmIdentifierOwned>,
    #[asn1(context_specific = "1", tag_mode = "EXPLICIT", optional = "true")]
    mask: Option<AlgorithmIdentifierOwned>,
    #[asn1(context_specific = "2", tag_mode = "EXPLICIT", optional = "true")]
    salt_length: Option<u64>,
    #[asn1(context_specific = "3", tag_mode = "EXPLICIT", optional = "true")]
    trailer: Option<u64>,
}

pub(super) fn approved_signature_algorithm(algorithm: &AlgorithmIdentifierOwned) -> bool {
    match algorithm.oid.to_string().as_str() {
        "1.2.840.113549.1.1.10" => {
            let Some(parameters) = &algorithm.parameters else {
                return false;
            };
            let Ok(encoded) = parameters.to_der() else {
                return false;
            };
            let Ok(parameters) = PssParameters::from_der(&encoded) else {
                return false;
            };
            parameters.hash.as_ref().is_some_and(|hash| {
                !weak_digest(&hash.oid.to_string()) && digest(&hash.oid.to_string()).is_some()
            })
        }
        "1.2.840.113549.1.1.11"
        | "1.2.840.113549.1.1.12"
        | "1.2.840.113549.1.1.13"
        | "1.2.840.113549.1.1.14"
        | "1.2.840.10045.4.3.1"
        | "1.2.840.10045.4.3.2"
        | "1.2.840.10045.4.3.3"
        | "1.2.840.10045.4.3.4"
        | "2.16.840.1.101.3.4.3.2"
        | "1.3.101.112"
        | "1.3.101.113" => true,
        _ => false,
    }
}

fn cms_integrity(bytes: &[u8], data: Option<&[u8]>) -> Result<(), ValidationState> {
    let mut cms = CmsContentInfo::from_der(bytes).map_err(|_| ValidationState::Indeterminate)?;
    // This proves cryptography only. Trust is separately evaluated against the
    // operator store; NEVER turn this flag into a trusted-signature judgment.
    cms.verify(
        None,
        None,
        data,
        None,
        CMSOptions::BINARY | CMSOptions::NO_SIGNER_CERT_VERIFY,
    )
    .map_err(|errors| {
        if errors.errors().iter().any(|error| {
            error.reason().is_some_and(|reason| {
                reason.contains("unsupported") || reason.contains("unknown digest")
            })
        }) {
            ValidationState::Indeterminate
        } else {
            ValidationState::Invalid
        }
    })
}

fn store(
    trust: &OperatorTrustStore,
    moment: OffsetDateTime,
    purpose: X509PurposeId,
) -> Result<X509Store, &'static str> {
    let mut store = X509StoreBuilder::new().map_err(|_| "CRYPTO_ENGINE_UNAVAILABLE")?;
    for root in &trust.roots {
        store
            .add_cert(root.clone())
            .map_err(|_| "INVALID_TRUST_ROOT")?;
    }
    let mut parameters = X509VerifyParam::new().map_err(|_| "CRYPTO_ENGINE_UNAVAILABLE")?;
    parameters.set_time(moment.unix_timestamp());
    parameters.set_auth_level(2);
    store
        .set_param(&parameters)
        .map_err(|_| "CRYPTO_ENGINE_UNAVAILABLE")?;
    store
        .set_purpose(purpose)
        .map_err(|_| "CRYPTO_ENGINE_UNAVAILABLE")?;
    Ok(store.build())
}

fn chain(
    leaf: &X509Ref,
    embedded: &[X509],
    trust: &OperatorTrustStore,
    moment: OffsetDateTime,
    timestamp: bool,
) -> Result<Vec<X509>, &'static str> {
    if trust.roots.is_empty() {
        return Err("NO_OPERATOR_TRUST_ROOTS");
    }
    let store = store(
        trust,
        moment,
        if timestamp {
            X509PurposeId::TIMESTAMP_SIGN
        } else {
            X509PurposeId::ANY
        },
    )?;
    let mut untrusted = Stack::new().map_err(|_| "CRYPTO_ENGINE_UNAVAILABLE")?;
    for certificate in embedded.iter().chain(&trust.intermediates) {
        untrusted
            .push(certificate.clone())
            .map_err(|_| "CRYPTO_ENGINE_UNAVAILABLE")?;
    }
    let mut context = X509StoreContext::new().map_err(|_| "CRYPTO_ENGINE_UNAVAILABLE")?;
    context
        .init(&store, leaf, &untrusted, |context| {
            if !context.verify_cert()? {
                return Ok(None);
            }
            Ok(context.chain().map(|certificates| {
                certificates
                    .iter()
                    .map(|certificate| certificate.to_owned())
                    .collect()
            }))
        })
        .map_err(|_| "CERTIFICATE_CHAIN_INDETERMINATE")?
        .ok_or("CERTIFICATE_CHAIN_INDETERMINATE")
}

fn certificate_time_valid(certificate: &X509Ref, moment: OffsetDateTime) -> bool {
    let Ok(now) = Asn1Time::from_unix(moment.unix_timestamp()) else {
        return false;
    };
    certificate
        .not_before()
        .compare(&now)
        .is_ok_and(|order| order != Ordering::Greater)
        && certificate
            .not_after()
            .compare(&now)
            .is_ok_and(|order| order != Ordering::Less)
}

fn signing_key_usage(certificate: &X509Ref) -> bool {
    let Ok(der) = certificate.to_der() else {
        return false;
    };
    let Ok(certificate) = Certificate::from_der(&der) else {
        return false;
    };
    match certificate
        .tbs_certificate
        .get::<x509_cert::ext::pkix::KeyUsage>()
    {
        Ok(Some((_, usage))) => usage.digital_signature() || usage.non_repudiation(),
        Ok(None) => true,
        Err(_) => false,
    }
}

fn openssl_time(value: &impl ToString) -> Option<OffsetDateTime> {
    let format = time::format_description::parse_borrowed::<2>(
        "[month repr:short] [day padding:space] [hour]:[minute]:[second] [year] GMT",
    )
    .ok()?;
    PrimitiveDateTime::parse(&value.to_string(), &format)
        .ok()
        .map(|value| value.assume_utc())
}

fn iso_time(value: &Asn1TimeRef) -> String {
    openssl_time(&value)
        .and_then(|value| value.format(&Rfc3339).ok())
        .unwrap_or_default()
}

fn revocation(
    certificates: &[X509],
    trust: &OperatorTrustStore,
    moment: OffsetDateTime,
) -> ValidationState {
    if certificates.is_empty() {
        return ValidationState::Indeterminate;
    }
    let mut all_good = true;
    for pair in certificates.windows(2) {
        let state = certificate_revocation(&pair[0], &pair[1], certificates, trust, moment, 0);
        if state == ValidationState::Invalid {
            return state;
        }
        all_good &= state == ValidationState::Valid;
    }
    if all_good {
        ValidationState::Valid
    } else {
        ValidationState::Indeterminate
    }
}

fn certificate_revocation(
    certificate: &X509Ref,
    issuer: &X509Ref,
    chain: &[X509],
    trust: &OperatorTrustStore,
    moment: OffsetDateTime,
    depth: usize,
) -> ValidationState {
    certificate_revocation_with_policy(certificate, issuer, chain, trust, moment, depth, true)
}

fn certificate_revocation_with_policy(
    certificate: &X509Ref,
    issuer: &X509Ref,
    chain: &[X509],
    trust: &OperatorTrustStore,
    moment: OffsetDateTime,
    depth: usize,
    use_crls: bool,
) -> ValidationState {
    if depth >= 8 {
        return ValidationState::Indeterminate;
    }
    let crl = if use_crls {
        super::crl::certificate_status(certificate, issuer, trust, moment)
    } else {
        ValidationState::Indeterminate.into()
    };
    if crl.state == ValidationState::Invalid {
        return crl.state;
    }
    let mut good = crl.state == ValidationState::Valid;
    let Ok(response_store) = store(trust, moment, X509PurposeId::ANY) else {
        return ValidationState::Indeterminate;
    };
    // B0's policy considers OCSP only when the certificate declares an OCSP
    // access method. The URL selects evidence relevance, never a fetch target.
    if !certificate
        .ocsp_responders()
        .is_ok_and(|responders| !responders.is_empty())
    {
        return if good && !crl.unresolved {
            ValidationState::Valid
        } else {
            ValidationState::Indeterminate
        };
    }
    let Ok(mut candidates) = Stack::new() else {
        return ValidationState::Indeterminate;
    };
    for cert in chain.iter().chain(&trust.intermediates) {
        if candidates.push(cert.clone()).is_err() {
            return ValidationState::Indeterminate;
        }
    }
    for response in &trust.ocsps {
        if response.status() != OcspResponseStatus::SUCCESSFUL {
            continue;
        }
        let Ok(basic) = response.basic() else {
            continue;
        };
        // Determine relevance before recursively checking a delegated responder.
        // Unrelated responses must not create recursion or consume crypto work.
        let matching: Vec<_> = [
            MessageDigest::sha1(),
            MessageDigest::sha256(),
            MessageDigest::sha384(),
            MessageDigest::sha512(),
        ]
        .into_iter()
        .filter_map(|algorithm| {
            OcspCertId::from_cert(algorithm, certificate, issuer)
                .ok()
                .and_then(|id| basic.find_status(&id))
        })
        .collect();
        if matching.is_empty() {
            continue;
        }
        if basic
            .verify(&candidates, &response_store, OcspFlag::empty())
            .is_err()
        {
            continue;
        }
        let Ok(encoded) = response.to_der() else {
            continue;
        };
        let Ok(parsed) = x509_ocsp::OcspResponse::from_der(&encoded) else {
            continue;
        };
        let Some(bytes) = parsed.response_bytes else {
            continue;
        };
        let Ok(parsed) = x509_ocsp::BasicOcspResponse::from_der(bytes.response.as_bytes()) else {
            continue;
        };
        let mut ids = std::collections::HashSet::new();
        if parsed.tbs_response_data.responses.iter().any(|response| {
            response
                .cert_id
                .to_der()
                .map(|encoded| !ids.insert(encoded))
                .unwrap_or(true)
        }) {
            continue;
        }
        if !approved_signature_algorithm(&parsed.signature_algorithm)
            || !ocsp_responder_authorized(&parsed, issuer, chain, trust, moment, depth)
        {
            continue;
        }
        for status in matching {
            let Some(this_update) = openssl_time(&status.this_update) else {
                continue;
            };
            let next_update = status
                .next_update()
                .and_then(|value| openssl_time(&value))
                .unwrap_or(this_update + Duration::minutes(30));
            if this_update > moment + Duration::SECOND || next_update < moment - Duration::SECOND {
                continue;
            }
            if status.status == OcspCertStatus::REVOKED
                && status
                    .revocation_time
                    .and_then(|value| openssl_time(&value))
                    .is_some_and(|time| time <= moment)
            {
                return ValidationState::Invalid;
            }
            if status.status == OcspCertStatus::GOOD {
                good = true;
            }
        }
    }
    if good && !crl.unresolved {
        ValidationState::Valid
    } else {
        ValidationState::Indeterminate
    }
}

fn ocsp_responder_authorized(
    response: &x509_ocsp::BasicOcspResponse,
    issuer: &X509Ref,
    chain: &[X509],
    trust: &OperatorTrustStore,
    moment: OffsetDateTime,
    depth: usize,
) -> bool {
    // OpenSSL has already verified the response signature and responder chain.
    // RFC6960 delegated responders additionally need their own revocation proof
    // unless the issuing CA explicitly issued the OCSP no-check extension.
    let mut candidates: Vec<X509> = chain.iter().chain(&trust.intermediates).cloned().collect();
    for certificate in response.certs.as_ref().into_iter().flatten() {
        let Ok(encoded) = certificate.to_der() else {
            return false;
        };
        let Ok(certificate) = X509::from_der(&encoded) else {
            return false;
        };
        candidates.push(certificate);
    }
    let mut matched = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for certificate in candidates {
        let Ok(encoded) = certificate.to_der() else {
            continue;
        };
        if !seen.insert(encoded.clone()) {
            continue;
        }
        let Ok(parsed) = Certificate::from_der(&encoded) else {
            continue;
        };
        let matches = match &response.tbs_response_data.responder_id {
            x509_ocsp::ResponderId::ByName(name) => parsed.tbs_certificate.subject == *name,
            x509_ocsp::ResponderId::ByKey(key) => parsed
                .tbs_certificate
                .subject_public_key_info
                .subject_public_key
                .as_bytes()
                .is_some_and(|public_key| {
                    hash(MessageDigest::sha1(), public_key)
                        .is_ok_and(|hash| hash.as_ref() == key.as_bytes())
                }),
        };
        if matches {
            matched.push((certificate, parsed));
        }
    }
    if matched.len() != 1 {
        return false;
    }
    let (responder, parsed) = matched.remove(0);
    if responder.to_der().ok() == issuer.to_der().ok() {
        return true;
    }
    if !certificate_time_valid(&responder, moment) || !signing_key_usage(&responder) {
        return false;
    }
    if issuer.issued(&responder) != openssl::x509::X509VerifyResult::OK {
        return false;
    }
    let Ok(issuer_key) = issuer.public_key() else {
        return false;
    };
    if !responder.verify(&issuer_key).unwrap_or(false) {
        return false;
    }
    if !parsed
        .tbs_certificate
        .get::<x509_cert::ext::pkix::ExtendedKeyUsage>()
        .ok()
        .flatten()
        .is_some_and(|(_, usage)| {
            usage
                .0
                .iter()
                .any(|oid| oid.to_string() == "1.3.6.1.5.5.7.3.9")
        })
    {
        return false;
    }
    if parsed
        .tbs_certificate
        .extensions
        .as_ref()
        .is_some_and(|extensions| {
            extensions.iter().any(|extension| {
                extension.extn_id.to_string() == "1.3.6.1.5.5.7.48.1.5"
                    && extension.extn_value.as_bytes() == [5, 0]
            })
        })
    {
        return true;
    }
    certificate_revocation(&responder, issuer, chain, trust, moment, depth + 1)
        == ValidationState::Valid
}

fn has_relevant_ocsp(chain: &[X509], trust: &OperatorTrustStore) -> bool {
    chain.windows(2).any(|pair| {
        pair[0]
            .ocsp_responders()
            .is_ok_and(|responders| !responders.is_empty())
            && trust.ocsps.iter().any(|response| {
                let exact = response.basic().is_ok_and(|basic| {
                    [
                        MessageDigest::sha1(),
                        MessageDigest::sha256(),
                        MessageDigest::sha384(),
                        MessageDigest::sha512(),
                    ]
                    .into_iter()
                    .any(|algorithm| {
                        OcspCertId::from_cert(algorithm, &pair[0], &pair[1])
                            .is_ok_and(|id| basic.find_status(&id).is_some())
                    })
                });
                if exact {
                    return true;
                }
                // A supplied assertion for this serial with a mismatched issuer is
                // a failed jurisdiction check, not an absent-evidence soft failure.
                let parsed = || {
                    let certificate = Certificate::from_der(&pair[0].to_der().ok()?).ok()?;
                    let issuer = Certificate::from_der(&pair[1].to_der().ok()?).ok()?;
                    let response =
                        x509_ocsp::OcspResponse::from_der(&response.to_der().ok()?).ok()?;
                    let bytes = response.response_bytes?;
                    let response =
                        x509_ocsp::BasicOcspResponse::from_der(bytes.response.as_bytes()).ok()?;
                    Some(response.tbs_response_data.responses.iter().any(|response| {
                        let id = &response.cert_id;
                        let Some(algorithm) = digest(&id.hash_algorithm.oid.to_string()) else {
                            return true;
                        };
                        // B0 treats a response for another serial under the
                        // same issuer key as a failed jurisdiction assertion.
                        let name_matches =
                            issuer.tbs_certificate.subject.to_der().is_ok_and(|name| {
                                hash(algorithm, &name).is_ok_and(|value| {
                                    value.as_ref() == id.issuer_name_hash.as_bytes()
                                })
                            });
                        let key_matches = hash(
                            algorithm,
                            issuer
                                .tbs_certificate
                                .subject_public_key_info
                                .subject_public_key
                                .raw_bytes(),
                        )
                        .is_ok_and(|value| value.as_ref() == id.issuer_key_hash.as_bytes());
                        key_matches
                            || (name_matches
                                && id.serial_number == certificate.tbs_certificate.serial_number)
                    }))
                };
                parsed().unwrap_or(false)
            })
    })
}

fn declares_crl(certificate: &X509Ref) -> bool {
    let declared = || {
        let certificate = Certificate::from_der(&certificate.to_der().ok()?).ok()?;
        Some(
            certificate
                .tbs_certificate
                .get::<x509_cert::ext::pkix::CrlDistributionPoints>()
                .ok()??
                .1
                .0
                .iter()
                .any(|point| point.distribution_point.is_some() || point.crl_issuer.is_some()),
        )
    };
    declared() == Some(true)
}

fn soft_fail_chain_revocation(
    chain: &[X509],
    trust: &OperatorTrustStore,
    moment: OffsetDateTime,
) -> ValidationState {
    for pair in chain.windows(2) {
        let declared_crl = declares_crl(&pair[0]);
        let relevant_crl = declared_crl
            && trust.crls.iter().any(|(crl, _)| {
                crl.issuer_name()
                    .try_cmp(pair[1].subject_name())
                    .is_ok_and(|ordering| ordering.is_eq())
            });
        if (relevant_crl || has_relevant_ocsp(pair, trust))
            && certificate_revocation_with_policy(
                &pair[0],
                &pair[1],
                chain,
                trust,
                moment,
                0,
                declared_crl,
            ) != ValidationState::Valid
        {
            return ValidationState::Indeterminate;
        }
    }
    ValidationState::Valid
}

#[derive(Sequence)]
struct EssIssuerSerial {
    issuer: x509_cert::ext::pkix::name::GeneralNames,
    serial_number: x509_cert::serial_number::SerialNumber,
}
#[derive(Sequence)]
struct EssCertIdV2 {
    #[asn1(optional = "true")]
    algorithm: Option<AlgorithmIdentifierOwned>,
    hash: OctetString,
    #[asn1(optional = "true")]
    issuer_serial: Option<EssIssuerSerial>,
}
#[derive(Sequence)]
struct SigningCertificateV2 {
    certificates: Vec<EssCertIdV2>,
    #[asn1(optional = "true")]
    policies: Option<Vec<Any>>,
}
#[derive(Sequence)]
struct EssCertId {
    hash: OctetString,
    #[asn1(optional = "true")]
    issuer_serial: Option<EssIssuerSerial>,
}
#[derive(Sequence)]
struct SigningCertificate {
    certificates: Vec<EssCertId>,
    #[asn1(optional = "true")]
    policies: Option<Vec<Any>>,
}

fn issuer_serial_matches(expected: &Option<EssIssuerSerial>, certificate: &[u8]) -> bool {
    let Some(expected) = expected else {
        return true;
    };
    let Ok(certificate) = Certificate::from_der(certificate) else {
        return false;
    };
    expected.serial_number == certificate.tbs_certificate.serial_number
        && expected.issuer.len() == 1
        && matches!(&expected.issuer[0], x509_cert::ext::pkix::name::GeneralName::DirectoryName(name) if *name == certificate.tbs_certificate.issuer)
}

#[derive(Sequence)]
struct AlgorithmProtection {
    digest: AlgorithmIdentifierOwned,
    #[asn1(
        context_specific = "1",
        tag_mode = "IMPLICIT",
        constructed = "true",
        optional = "true"
    )]
    signature: Option<AlgorithmIdentifierOwned>,
    #[asn1(
        context_specific = "2",
        tag_mode = "IMPLICIT",
        constructed = "true",
        optional = "true"
    )]
    mac: Option<AlgorithmIdentifierOwned>,
}

fn algorithm_protection(signer: &SignerInfo) -> bool {
    let Some(attributes) = &signer.signed_attrs else {
        return true;
    };
    let protection: Vec<_> = attributes
        .iter()
        .filter(|attribute| attribute.oid.to_string() == "1.2.840.113549.1.9.52")
        .collect();
    if protection.is_empty() {
        return true;
    }
    if protection.len() != 1 || protection[0].values.len() != 1 {
        return false;
    }
    let Some(value) = protection[0].values.get(0) else {
        return false;
    };
    let Ok(encoded) = value.to_der() else {
        return false;
    };
    let Ok(protection) = AlgorithmProtection::from_der(&encoded) else {
        return false;
    };
    protection.digest == signer.digest_alg
        && protection.signature.as_ref() == Some(&signer.signature_algorithm)
        && protection.mac.is_none()
}

fn certificate_binding(signer: &SignerInfo, certificate: &X509Ref, required: bool) -> bool {
    let Some(attributes) = &signer.signed_attrs else {
        return !required;
    };
    let Ok(certificate) = certificate.to_der() else {
        return false;
    };
    let mut found = false;
    for attribute in attributes.iter() {
        let oid = attribute.oid.to_string();
        if !["1.2.840.113549.1.9.16.2.12", "1.2.840.113549.1.9.16.2.47"].contains(&oid.as_str()) {
            continue;
        }
        if found || attribute.values.len() != 1 {
            return false;
        }
        found = true;
        let Some(value) = attribute.values.get(0) else {
            return false;
        };
        let Ok(der) = value.to_der() else {
            return false;
        };
        if oid.ends_with(".47") {
            let Ok(binding) = SigningCertificateV2::from_der(&der) else {
                return false;
            };
            let Some(binding) = binding.certificates.first() else {
                return false;
            };
            if !issuer_serial_matches(&binding.issuer_serial, &certificate) {
                return false;
            }
            let algorithm = binding
                .algorithm
                .as_ref()
                .map(|algorithm| algorithm.oid.to_string())
                .unwrap_or_else(|| "2.16.840.1.101.3.4.2.1".to_owned());
            let Some(algorithm) = digest(&algorithm) else {
                return false;
            };
            if !hash(algorithm, &certificate)
                .is_ok_and(|hash| hash.as_ref() == binding.hash.as_bytes())
            {
                return false;
            }
        } else {
            let Ok(binding) = SigningCertificate::from_der(&der) else {
                return false;
            };
            let Some(binding) = binding.certificates.first() else {
                return false;
            };
            if !issuer_serial_matches(&binding.issuer_serial, &certificate) {
                return false;
            }
            if !hash(MessageDigest::sha1(), &certificate)
                .is_ok_and(|hash| hash.as_ref() == binding.hash.as_bytes())
            {
                return false;
            }
        }
    }
    found || !required
}

fn timestamp(
    signer: &SignerInfo,
    trust: &OperatorTrustStore,
    moment: OffsetDateTime,
) -> (ValidationState, Option<String>, &'static str) {
    let Some(attributes) = &signer.unsigned_attrs else {
        return (ValidationState::Indeterminate, None, "NO_TRUSTED_TIMESTAMP");
    };
    let tokens: Vec<_> = attributes
        .iter()
        .filter(|attribute| attribute.oid.to_string() == TIMESTAMP_TOKEN)
        .collect();
    if tokens.is_empty() {
        return (ValidationState::Indeterminate, None, "NO_TRUSTED_TIMESTAMP");
    }
    if tokens.len() != 1 || tokens[0].values.len() != 1 {
        return (ValidationState::Invalid, None, "INVALID_TIMESTAMP_TOKEN");
    }
    let parsed = || -> Result<(ValidationState, String), &'static str> {
        let token = tokens[0]
            .values
            .get(0)
            .ok_or("INVALID_TIMESTAMP_TOKEN")?
            .to_der()
            .map_err(|_| "INVALID_TIMESTAMP_TOKEN")?;
        let (signed, certificates, certificate) = parsed_cms(&token)?;
        if signed.encap_content_info.econtent_type.to_string() != TST_INFO {
            return Err("INVALID_TIMESTAMP_CONTENT_TYPE");
        }
        let signer_info = signed
            .signer_infos
            .0
            .get(0)
            .ok_or("INVALID_TIMESTAMP_TOKEN")?;
        if !algorithm_protection(signer_info) {
            return Err("TIMESTAMP_ALGORITHM_PROTECTION_MISMATCH");
        }
        let digest_oid = signer_info.digest_alg.oid.to_string();
        if digest(&digest_oid).is_none() {
            return Ok((ValidationState::Indeterminate, String::new()));
        }
        cms_integrity(&token, None).map_err(|_| "INVALID_TIMESTAMP_SIGNATURE")?;
        if !certificate_binding(signer_info, &certificate, true) {
            return Err("INVALID_TIMESTAMP_CERTIFICATE_BINDING");
        }
        let content = signed
            .encap_content_info
            .econtent
            .as_ref()
            .ok_or("INVALID_TIMESTAMP_CONTENT")?;
        let content = content
            .decode_as::<OctetString>()
            .map_err(|_| "INVALID_TIMESTAMP_CONTENT")?;
        let info =
            TstInfo::from_der(content.as_bytes()).map_err(|_| "INVALID_TIMESTAMP_CONTENT")?;
        let Some(algorithm) = digest(&info.message_imprint.hash_algorithm.oid.to_string()) else {
            return Ok((ValidationState::Indeterminate, String::new()));
        };
        if !hash(algorithm, signer.signature.as_bytes())
            .is_ok_and(|hash| hash.as_ref() == info.message_imprint.hashed_message.as_bytes())
        {
            return Err("TIMESTAMP_MESSAGE_IMPRINT_MISMATCH");
        }
        let time =
            OffsetDateTime::from_unix_timestamp(info.gen_time.to_unix_duration().as_secs() as i64)
                .map_err(|_| "INVALID_TIMESTAMP_TIME")?;
        if time > moment {
            return Err("TIMESTAMP_IN_FUTURE");
        }
        let iso = time
            .format(&Rfc3339)
            .map_err(|_| "INVALID_TIMESTAMP_TIME")?;
        if weak_digest(&digest_oid)
            || weak_digest(&info.message_imprint.hash_algorithm.oid.to_string())
        {
            return Ok((ValidationState::Indeterminate, iso));
        }
        if !certificate_time_valid(&certificate, moment) {
            return Ok((ValidationState::Indeterminate, iso));
        }
        let Ok(chain) = chain(&certificate, &certificates, trust, moment, true) else {
            return Ok((ValidationState::Indeterminate, iso));
        };
        Ok((revocation(&chain, trust, moment), iso))
    };
    match parsed() {
        Ok((state, time)) => (
            state,
            if time.is_empty() { None } else { Some(time) },
            if state == ValidationState::Valid {
                ""
            } else {
                "TIMESTAMP_TRUST_INDETERMINATE"
            },
        ),
        Err(reason) => (ValidationState::Invalid, None, reason),
    }
}

fn name(certificate: &openssl::x509::X509NameRef) -> String {
    certificate
        .entries()
        .map(|entry| {
            let label = match entry.object().nid() {
                openssl::nid::Nid::COMMONNAME => "Common Name",
                openssl::nid::Nid::ORGANIZATIONNAME => "Organization",
                openssl::nid::Nid::COUNTRYNAME => "Country",
                _ => entry.object().nid().long_name().unwrap_or("Attribute"),
            };
            format!("{label}: {}", entry.data().to_string().unwrap_or_default())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn verify_signature(
    field: &str,
    cms: &[u8],
    data: &[u8],
    trust: &OperatorTrustStore,
    moment: OffsetDateTime,
) -> SignatureValidation {
    let (signed, certificates, certificate) = match parsed_cms(cms) {
        Ok(value) => value,
        Err(reason) => return SignatureValidation::unresolved(field.to_owned(), reason),
    };
    let mut result = SignatureValidation::unresolved(field.to_owned(), "");
    result.reasons.clear();
    result.subject = name(certificate.subject_name());
    result.issuer = name(certificate.issuer_name());
    result.serial = certificate
        .serial_number()
        .to_bn()
        .and_then(|value| value.to_dec_str())
        .map(|value| value.to_string())
        .unwrap_or_default();
    result.certificate_sha256 = certificate
        .to_der()
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .unwrap_or_default();
    result.not_before = iso_time(certificate.not_before());
    result.not_after = iso_time(certificate.not_after());
    result.certificate_validity = if certificate_time_valid(&certificate, moment) {
        ValidationState::Valid
    } else {
        ValidationState::Invalid
    };
    let Some(signer) = signed.signer_infos.0.get(0) else {
        return result;
    };
    let digest_oid = signer.digest_alg.oid.to_string();
    if digest(&digest_oid).is_none() {
        result.reasons.push("UNSUPPORTED_DIGEST".to_owned());
        return result;
    }
    if signed.encap_content_info.econtent_type.to_string() != DATA
        || signed.encap_content_info.econtent.is_some()
    {
        result
            .reasons
            .push("UNSUPPORTED_DETACHED_CMS_CONTENT".to_owned());
        return result;
    }
    result.integrity = match cms_integrity(cms, Some(data)) {
        Ok(()) => ValidationState::Valid,
        Err(state) => state,
    };
    if !algorithm_protection(signer) {
        result.integrity = ValidationState::Indeterminate;
        result
            .reasons
            .push("CMS_ALGORITHM_PROTECTION_MISMATCH".to_owned());
    }
    if !certificate_binding(signer, &certificate, false) {
        result.integrity = ValidationState::Invalid;
        result
            .reasons
            .push("SIGNER_CERTIFICATE_BINDING_MISMATCH".to_owned());
    }
    if result.integrity != ValidationState::Valid {
        result
            .reasons
            .push("SIGNATURE_INTEGRITY_NOT_ESTABLISHED".to_owned());
    } else if weak_digest(&digest_oid) {
        result
            .reasons
            .push("DISALLOWED_SIGNATURE_DIGEST".to_owned());
    } else if !signing_key_usage(&certificate) {
        result
            .reasons
            .push("SIGNER_KEY_USAGE_UNSUITABLE".to_owned());
    } else {
        match chain(&certificate, &certificates, trust, moment, false) {
            Ok(chain) => {
                result.certificate_chain = soft_fail_chain_revocation(&chain, trust, moment);
                result.revocation = revocation(&chain, trust, moment);
                if result.revocation == ValidationState::Valid {
                    result.required_certificate_chain = ValidationState::Valid;
                }
            }
            Err(reason) => result.reasons.push(reason.to_owned()),
        }
    }
    let (state, time, reason) = timestamp(signer, trust, moment);
    result.timestamp = state;
    result.timestamp_time = time;
    if !reason.is_empty() {
        result.reasons.push(reason.to_owned());
    }
    if result.revocation == ValidationState::Indeterminate {
        result
            .reasons
            .push("REVOCATION_EVIDENCE_INSUFFICIENT".to_owned());
    }
    result
}
