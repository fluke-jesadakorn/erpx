//! Offline CRL evidence. Cryptographic signatures remain OpenSSL's responsibility;
//! RustCrypto supplies the typed RFC5280 structures used for scope and deltas.
use der::Decode;
use openssl::x509::X509Ref;
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};
use x509_cert::{
    Certificate,
    crl::CertificateList,
    ext::pkix::{
        AuthorityKeyIdentifier, BaseCrlNumber, BasicConstraints, CrlNumber, CrlReason, FreshestCrl,
        IssuingDistributionPoint, KeyUsage, name::GeneralName,
    },
};

use super::{crypto::approved_signature_algorithm, trust::OperatorTrustStore};
use crate::domain::pdf_validation::ValidationState;

struct CrlEvidence {
    list: CertificateList,
    number: Option<Vec<u8>>,
    base: Option<Vec<u8>>,
    authority: Option<Vec<u8>>,
    distribution: Option<Vec<u8>>,
    indirect: bool,
    freshest: bool,
}

pub(super) struct CrlEvaluation {
    pub state: ValidationState,
    pub unresolved: bool,
}
impl From<ValidationState> for CrlEvaluation {
    fn from(state: ValidationState) -> Self {
        Self {
            state,
            unresolved: false,
        }
    }
}

pub(super) fn certificate_status(
    certificate: &X509Ref,
    issuer: &X509Ref,
    trust: &OperatorTrustStore,
    moment: OffsetDateTime,
) -> CrlEvaluation {
    let (Ok(certificate_der), Ok(issuer_der), Ok(issuer_key)) =
        (certificate.to_der(), issuer.to_der(), issuer.public_key())
    else {
        return ValidationState::Indeterminate.into();
    };
    let (Ok(certificate), Ok(issuer_certificate)) = (
        Certificate::from_der(&certificate_der),
        Certificate::from_der(&issuer_der),
    ) else {
        return ValidationState::Indeterminate.into();
    };
    match issuer_certificate.tbs_certificate.get::<KeyUsage>() {
        Ok(Some((_, usage))) if !usage.crl_sign() => return ValidationState::Indeterminate.into(),
        Err(_) => return ValidationState::Indeterminate.into(),
        _ => {}
    }
    let is_ca = match certificate.tbs_certificate.get::<BasicConstraints>() {
        Ok(Some((_, constraints))) => constraints.ca,
        Ok(None) => false,
        Err(_) => return ValidationState::Indeterminate.into(),
    };
    let mut candidates = Vec::new();
    let mut seen = std::collections::HashSet::<[u8; 32]>::new();
    let mut unresolved = false;
    let mut unresolved_delta = false;
    for (openssl_crl, der) in &trust.crls {
        if openssl_crl.issuer_name().to_der().ok() != issuer.subject_name().to_der().ok()
            || !openssl_crl.verify(&issuer_key).unwrap_or(false)
        {
            continue;
        }
        let Ok(list) = CertificateList::from_der(der) else {
            continue;
        };
        if !approved_signature_algorithm(&list.signature_algorithm)
            || list.signature_algorithm != list.tbs_cert_list.signature
        {
            continue;
        }
        let this = OffsetDateTime::from_unix_timestamp(
            list.tbs_cert_list.this_update.to_unix_duration().as_secs() as i64,
        );
        let Ok(this) = this else {
            continue;
        };
        let next = list
            .tbs_cert_list
            .next_update
            .and_then(|time| {
                OffsetDateTime::from_unix_timestamp(time.to_unix_duration().as_secs() as i64).ok()
            })
            .unwrap_or(this + Duration::minutes(30));
        if this > moment + Duration::SECOND || next < moment - Duration::SECOND {
            continue;
        }
        if !seen.insert(Sha256::digest(der).into()) {
            continue;
        }
        let is_delta = list
            .tbs_cert_list
            .crl_extensions
            .as_ref()
            .is_some_and(|extensions| {
                extensions
                    .iter()
                    .any(|extension| extension.extn_id.to_string() == "2.5.29.27")
            });
        match evidence(list, is_ca) {
            Ok(info) => candidates.push(info),
            Err(ScopeFailure::Excluded) => {}
            Err(ScopeFailure::Unresolved) if is_delta => unresolved_delta = true,
            Err(ScopeFailure::Unresolved) => unresolved = true,
        }
    }
    unresolved |= unresolved_delta
        && candidates
            .iter()
            .any(|candidate| candidate.base.is_none() && candidate.freshest);
    let mut good = false;
    for base in candidates
        .iter()
        .filter(|candidate| candidate.base.is_none())
    {
        // B0 requires a supplied matching delta only when the complete CRL
        // advertises FreshestCRL. This never triggers a URL or issuer fetch.
        let delta = if base.freshest {
            let matching: Vec<_> = candidates
                .iter()
                .filter(|delta| {
                    delta.base.is_some()
                        && delta.authority == base.authority
                        && delta.distribution == base.distribution
                        && match (&base.number, &delta.number, &delta.base) {
                            (Some(base), Some(number), Some(minimum)) => {
                                integer_cmp(base, minimum).is_ge()
                                    && integer_cmp(base, number).is_lt()
                            }
                            _ => false,
                        }
                })
                .collect();
            if matching.len() > 1 {
                // Multiple current, nonidentical deltas need full reconciliation.
                // Never select a good answer by ordering or CRL number alone.
                unresolved = true;
                let mut all_absent = true;
                for delta in &matching {
                    match entry_status(delta, &certificate, moment) {
                        Ok(EntryStatus::Revoked) => return ValidationState::Invalid.into(),
                        Ok(EntryStatus::NotListed) => {}
                        _ => all_absent = false,
                    }
                }
                if all_absent
                    && matches!(
                        entry_status(base, &certificate, moment),
                        Ok(EntryStatus::Revoked)
                    )
                {
                    return ValidationState::Invalid.into();
                }
                continue;
            }
            let Some(matching) = matching.first() else {
                continue;
            };
            Some(*matching)
        } else {
            None
        };
        let state = match delta
            .map(|delta| entry_status(delta, &certificate, moment))
            .transpose()
        {
            Ok(Some(EntryStatus::Revoked)) => return ValidationState::Invalid.into(),
            Ok(Some(EntryStatus::Removed)) => EntryStatus::NotListed,
            Ok(Some(EntryStatus::NotListed)) | Ok(None) => {
                match entry_status(base, &certificate, moment) {
                    Ok(state) => state,
                    Err(()) => {
                        unresolved = true;
                        continue;
                    }
                }
            }
            Err(()) => {
                unresolved = true;
                continue;
            }
        };
        match state {
            EntryStatus::Revoked => return ValidationState::Invalid.into(),
            EntryStatus::NotListed | EntryStatus::Removed => good = true,
        }
    }
    CrlEvaluation {
        state: if good && !unresolved {
            ValidationState::Valid
        } else {
            ValidationState::Indeterminate
        },
        unresolved,
    }
}

fn integer_cmp(left: &[u8], right: &[u8]) -> std::cmp::Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

enum ScopeFailure {
    Excluded,
    Unresolved,
}

fn evidence(list: CertificateList, is_ca: bool) -> Result<CrlEvidence, ScopeFailure> {
    let mut number = None;
    let mut base = None;
    let mut authority = None;
    let mut distribution = None;
    let mut indirect = false;
    let mut freshest = false;
    let mut seen = std::collections::HashSet::new();
    for extension in list
        .tbs_cert_list
        .crl_extensions
        .as_ref()
        .into_iter()
        .flatten()
    {
        let oid = extension.extn_id.to_string();
        if !seen.insert(oid.clone()) {
            return Err(ScopeFailure::Unresolved);
        }
        let bytes = extension.extn_value.as_bytes();
        match oid.as_str() {
            "2.5.29.20" => {
                number = Some(
                    CrlNumber::from_der(bytes)
                        .map_err(|_| ScopeFailure::Unresolved)?
                        .0
                        .as_bytes()
                        .to_vec(),
                )
            }
            "2.5.29.27" => {
                if !extension.critical {
                    return Err(ScopeFailure::Unresolved);
                }
                base = Some(
                    BaseCrlNumber::from_der(bytes)
                        .map_err(|_| ScopeFailure::Unresolved)?
                        .0
                        .as_bytes()
                        .to_vec(),
                );
            }
            "2.5.29.35" => {
                AuthorityKeyIdentifier::from_der(bytes).map_err(|_| ScopeFailure::Unresolved)?;
                authority = Some(bytes.to_vec());
            }
            "2.5.29.46" => {
                freshest = !FreshestCrl::from_der(bytes)
                    .map_err(|_| ScopeFailure::Unresolved)?
                    .0
                    .is_empty()
            }
            "2.5.29.28" => {
                // Match the RFC OID explicitly; do not rely on the external
                // type's AssociatedOid metadata to choose an extension.
                let point = IssuingDistributionPoint::from_der(bytes)
                    .map_err(|_| ScopeFailure::Unresolved)?;
                if point.only_contains_ca_certs && point.only_contains_user_certs {
                    return Err(ScopeFailure::Unresolved);
                }
                if point.only_contains_attribute_certs
                    || (point.only_contains_user_certs && is_ca)
                    || (point.only_contains_ca_certs && !is_ca)
                {
                    return Err(ScopeFailure::Excluded);
                }
                if point.distribution_point.is_some() || point.only_some_reasons.is_some() {
                    return Err(ScopeFailure::Unresolved);
                }
                indirect = point.indirect_crl;
                distribution = Some(bytes.to_vec());
            }
            _ if extension.critical => return Err(ScopeFailure::Unresolved),
            _ => {}
        }
    }
    Ok(CrlEvidence {
        list,
        number,
        base,
        authority,
        distribution,
        indirect,
        freshest,
    })
}

enum EntryStatus {
    NotListed,
    Revoked,
    Removed,
}

fn entry_status(
    evidence: &CrlEvidence,
    certificate: &Certificate,
    moment: OffsetDateTime,
) -> Result<EntryStatus, ()> {
    let mut issuers = vec![evidence.list.tbs_cert_list.issuer.clone()];
    for entry in evidence
        .list
        .tbs_cert_list
        .revoked_certificates
        .as_ref()
        .into_iter()
        .flatten()
    {
        let mut reason = CrlReason::Unspecified;
        let mut seen = std::collections::HashSet::new();
        for extension in entry.crl_entry_extensions.as_ref().into_iter().flatten() {
            let oid = extension.extn_id.to_string();
            if !seen.insert(oid.clone()) {
                return Err(());
            }
            let bytes = extension.extn_value.as_bytes();
            match oid.as_str() {
                "2.5.29.21" => reason = CrlReason::from_der(bytes).map_err(|_| ())?,
                "2.5.29.29" => {
                    if !evidence.indirect {
                        return Err(());
                    }
                    let names = Vec::<GeneralName>::from_der(bytes).map_err(|_| ())?;
                    issuers = names
                        .into_iter()
                        .filter_map(|name| match name {
                            GeneralName::DirectoryName(name) => Some(name),
                            _ => None,
                        })
                        .collect();
                    if issuers.is_empty() {
                        return Err(());
                    }
                }
                "2.5.29.24" => {
                    der::asn1::GeneralizedTime::from_der(bytes).map_err(|_| ())?;
                }
                _ if extension.critical => return Err(()),
                _ => {}
            }
        }
        if entry.serial_number == certificate.tbs_certificate.serial_number
            && issuers.contains(&certificate.tbs_certificate.issuer)
            && entry.revocation_date.to_unix_duration().as_secs() as i128
                <= i128::from(moment.unix_timestamp())
        {
            return Ok(if reason == CrlReason::RemoveFromCRL {
                EntryStatus::Removed
            } else {
                EntryStatus::Revoked
            });
        }
    }
    Ok(EntryStatus::NotListed)
}
