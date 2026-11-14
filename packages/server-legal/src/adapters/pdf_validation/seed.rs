//! The supported seed-value subset is checked against the signed revision.
//! Certificate selection, appearances and uncommon policies stay unresolved.
use der::Decode;
use lopdf::{Dictionary, Document, Object};

use super::trust::OperatorTrustStore;
use super::{
    crypto,
    document::{ParsedPdf, SignatureField, byte_range, resolve, signed_content},
};
use crate::domain::pdf_validation::ValidationState;
use time::OffsetDateTime;

pub(super) fn satisfied(
    bytes: &[u8],
    current: &ParsedPdf,
    signature: &SignatureField,
    timestamp: ValidationState,
    trust: &OperatorTrustStore,
    moment: OffsetDateTime,
) -> Result<(), &'static str> {
    let range = byte_range(signature, bytes.len())?;
    let end = range[2] + range[3];
    let previous;
    let original = if end == bytes.len() {
        current
    } else {
        previous = ParsedPdf::parse(&bytes[..end])?;
        &previous
    };
    let mut matches = original
        .signatures
        .iter()
        .filter(|field| field.name == signature.name);
    let signature = matches.next().ok_or("SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
    if matches.next().is_some() {
        return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED");
    }
    let Ok(seed) = signature.field.get(b"SV") else {
        return Ok(());
    };
    let document = &original.document;
    let seed = resolve(document, seed)?
        .as_dict()
        .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
    if seed.has(b"Type")
        && seed.get(b"Type").and_then(Object::as_name).ok() != Some(b"SV".as_slice())
    {
        return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED");
    }
    let flags = match seed.get(b"Ff") {
        Ok(value) => value
            .as_i64()
            .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?,
        Err(_) => 0,
    };
    if flags < 0
        || flags & !511 != 0
        || seed.iter().any(|(key, _)| {
            ![
                b"Type".as_slice(),
                b"Cert",
                b"Ff",
                b"Filter",
                b"SubFilter",
                b"V",
                b"Reasons",
                b"DigestMethod",
                b"TimeStamp",
                b"MDP",
                b"LockDocument",
                b"AddRevInfo",
                b"AppearanceFilter",
                b"LegalAttestation",
            ]
            .contains(&key.as_slice())
        })
    {
        return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED");
    }
    if let Ok(constraints) = seed.get_deref(b"Cert", document) {
        let constraints = constraints
            .as_dict()
            .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
        let content = signed_content(bytes, signature)?;
        certificate_constraints(constraints, document, &content.cms, trust, moment)?;
    }
    if flags & 1 != 0
        && seed.has(b"Filter")
        && seed.get(b"Filter").and_then(Object::as_name).ok() != Some(b"Adobe.PPKLite".as_slice())
    {
        return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED");
    }
    if seed.has(b"V") && !matches!(seed.get(b"V").and_then(Object::as_i64), Ok(1..=3)) {
        return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED");
    }
    if flags & 2 != 0
        && let Ok(required) = seed.get(b"SubFilter")
    {
        let values = resolve(document, required)?
            .as_array()
            .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
        if values.len() > 100 {
            return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED");
        }
        let first = values
            .iter()
            .filter_map(|value| value.as_name().ok())
            .find(|value| {
                [
                    b"adbe.pkcs7.detached".as_slice(),
                    b"ETSI.CAdES.detached",
                    b"ETSI.RFC3161",
                ]
                .contains(value)
            });
        if first.is_none() {
            return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED");
        }
        if signature
            .dictionary
            .get(b"SubFilter")
            .and_then(Object::as_name)
            .ok()
            != first
        {
            return Err("SEED_VALUE_CONSTRAINTS_FAILURE");
        }
    }
    if flags & 8 != 0 {
        let reasons = match seed.get(b"Reasons") {
            Ok(value) => strings(document, value)?,
            Err(_) => Vec::new(),
        };
        let actual = signature
            .dictionary
            .get(b"Reason")
            .ok()
            .map(lopdf::decode_text_string)
            .transpose()
            .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
        let omit = reasons.is_empty() || reasons == ["."];
        if (omit && actual.is_some())
            || (!omit && !actual.is_some_and(|value| reasons.contains(&value)))
        {
            return Err("SEED_VALUE_CONSTRAINTS_FAILURE");
        }
    }
    if flags & (16 | 256) != 0 {
        return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED");
    }
    if flags & (32 | 64) != 0 {
        let content = signed_content(bytes, signature)?;
        let (digest, adobe_revocation) = crypto::seed_cms_evidence(&content.cms)?;
        if flags & 64 != 0
            && let Ok(required) = seed.get(b"DigestMethod")
        {
            let required = strings(document, required)?;
            if !required
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&digest))
            {
                return Err("SEED_VALUE_CONSTRAINTS_FAILURE");
            }
        }
        if flags & 32 != 0
            && let Ok(required) = seed.get(b"AddRevInfo")
        {
            let required = required
                .as_bool()
                .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
            if required != adobe_revocation
                || (required
                    && signature
                        .dictionary
                        .get(b"SubFilter")
                        .and_then(Object::as_name)
                        .ok()
                        != Some(b"adbe.pkcs7.detached".as_slice()))
            {
                return Err("SEED_VALUE_CONSTRAINTS_FAILURE");
            }
        }
    }
    if let Ok(required) = seed.get_deref(b"TimeStamp", document) {
        let required = required
            .as_dict()
            .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
        let flag = required.get(b"Ff").and_then(Object::as_i64).unwrap_or(0);
        if flag != 0 && timestamp != ValidationState::Valid {
            return Err("SEED_VALUE_CONSTRAINTS_FAILURE");
        }
    }
    let permission = certification_permission(document, &signature.dictionary)?;
    if let Ok(required) = seed.get_deref(b"MDP", document) {
        let required = required
            .as_dict()
            .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?
            .get(b"P")
            .and_then(Object::as_i64)
            .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
        if !(0..=3).contains(&required) {
            return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED");
        }
        if (required == 0 && permission.is_some())
            || (required != 0 && permission != Some(required))
        {
            return Err("SEED_VALUE_CONSTRAINTS_FAILURE");
        }
    }
    if flags & 128 != 0
        && let Ok(required) = seed.get(b"LockDocument")
    {
        match required.as_name() {
            Ok(b"true") if permission != Some(1) => return Err("SEED_VALUE_CONSTRAINTS_FAILURE"),
            Ok(b"false") if permission == Some(1) => return Err("SEED_VALUE_CONSTRAINTS_FAILURE"),
            Ok(b"true" | b"false" | b"auto") => {}
            _ => return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED"),
        }
    }
    Ok(())
}

fn strings(document: &Document, value: &Object) -> Result<Vec<String>, &'static str> {
    let values = resolve(document, value)?
        .as_array()
        .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
    if values.len() > 100 {
        return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED");
    }
    values
        .iter()
        .map(|value| {
            lopdf::decode_text_string(value).map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")
        })
        .collect()
}

fn certificate_constraints(
    constraints: &Dictionary,
    document: &Document,
    cms: &[u8],
    trust: &OperatorTrustStore,
    moment: OffsetDateTime,
) -> Result<(), &'static str> {
    if constraints.has(b"Type")
        && constraints.get(b"Type").and_then(Object::as_name).ok() != Some(b"SVCert".as_slice())
    {
        return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED");
    }
    let flags = constraints
        .get(b"Ff")
        .map_or(Ok(0), Object::as_i64)
        .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
    if flags < 0 || flags & !107 != 0 {
        return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED");
    }
    let signer = crypto::seed_signer_certificate(cms)?;
    for (key, flag) in [(b"Subject".as_slice(), 1), (b"Issuer".as_slice(), 2)] {
        if let Ok(values) = constraints.get_deref(key, document) {
            let values = values
                .as_array()
                .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
            if values.len() > 100 {
                return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED");
            }
            let certificates = values
                .iter()
                .map(|value| {
                    x509_cert::Certificate::from_der(
                        resolve(document, value)?
                            .as_str()
                            .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?,
                    )
                    .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")
                })
                .collect::<Result<Vec<_>, _>>()?;
            if flags & flag != 0 && !certificates.is_empty() {
                let authorities = if flag == 2 {
                    crypto::seed_authorities(cms, trust, moment)?
                } else {
                    vec![signer.clone()]
                };
                if !certificates.iter().any(|allowed| {
                    authorities.iter().any(|actual| {
                        actual.tbs_certificate.issuer == allowed.tbs_certificate.issuer
                            && actual.tbs_certificate.serial_number
                                == allowed.tbs_certificate.serial_number
                    })
                }) {
                    return Err("SEED_VALUE_CONSTRAINTS_FAILURE");
                }
            }
        }
    }
    if let Ok(values) = constraints.get_deref(b"SubjectDN", document) {
        let values = values
            .as_array()
            .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
        if values.len() > 100 {
            return Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED");
        }
        let mut required = std::collections::BTreeMap::new();
        for value in values {
            let value = resolve(document, value)?
                .as_dict()
                .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
            for (key, value) in value {
                let key =
                    std::str::from_utf8(key).map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
                let oid = match key.to_ascii_uppercase().as_str() {
                    "CN" => "2.5.4.3",
                    "SERIALNUMBER" => "2.5.4.5",
                    "C" => "2.5.4.6",
                    "L" => "2.5.4.7",
                    "ST" => "2.5.4.8",
                    "O" => "2.5.4.10",
                    "OU" => "2.5.4.11",
                    _ => key,
                };
                const_oid::ObjectIdentifier::new(oid)
                    .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
                required.insert(
                    oid.to_owned(),
                    lopdf::decode_text_string(value)
                        .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?,
                );
            }
        }
        if flags & 8 != 0 {
            let actual = crypto::seed_subject_attributes(cms)?;
            if !required.into_iter().all(|pair| actual.contains(&pair)) {
                return Err("SEED_VALUE_CONSTRAINTS_FAILURE");
            }
        }
    }
    if let Ok(values) = constraints.get(b"KeyUsage") {
        let profiles = strings(document, values)?;
        if flags & 32 != 0 {
            let usage = signer
                .tbs_certificate
                .get::<x509_cert::ext::pkix::KeyUsage>()
                .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?
                .map(|(_, usage)| usage);
            let bits = usage
                .map(|usage| {
                    [
                        usage.digital_signature(),
                        usage.non_repudiation(),
                        usage.key_encipherment(),
                        usage.data_encipherment(),
                        usage.key_agreement(),
                        usage.key_cert_sign(),
                        usage.crl_sign(),
                        usage.encipher_only(),
                        usage.decipher_only(),
                    ]
                })
                .unwrap_or([false; 9]);
            if !profiles.iter().any(|profile| {
                profile
                    .chars()
                    .take(9)
                    .enumerate()
                    .all(|(bit, required)| match required {
                        '1' => bits[bit],
                        '0' => !bits[bit],
                        _ => true,
                    })
            }) {
                return Err("SEED_VALUE_CONSTRAINTS_FAILURE");
            }
        }
    }
    Ok(())
}

fn certification_permission(
    document: &Document,
    signature: &Dictionary,
) -> Result<Option<i64>, &'static str> {
    let catalog = document
        .catalog()
        .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
    let certification = catalog
        .get_deref(b"Perms", document)
        .ok()
        .and_then(|value| value.as_dict().ok())
        .and_then(|value| value.get_deref(b"DocMDP", document).ok())
        .and_then(|value| value.as_dict().ok());
    if certification != Some(signature) {
        return Ok(None);
    }
    let references = signature
        .get(b"Reference")
        .and_then(Object::as_array)
        .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
    for reference in references {
        let reference = resolve(document, reference)?
            .as_dict()
            .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
        if reference
            .get(b"TransformMethod")
            .and_then(Object::as_name)
            .ok()
            == Some(b"DocMDP".as_slice())
        {
            let params = reference
                .get_deref(b"TransformParams", document)
                .and_then(Object::as_dict)
                .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED")?;
            return params
                .get(b"P")
                .and_then(Object::as_i64)
                .map(Some)
                .map_err(|_| "SEED_VALUE_CONSTRAINTS_UNRESOLVED");
        }
    }
    Err("SEED_VALUE_CONSTRAINTS_UNRESOLVED")
}
