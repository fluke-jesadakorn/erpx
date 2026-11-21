//! RFC5280 6.3.3(b), (d): distribution-point applicability and reason coverage.
//! No name resolution, issuer discovery or network requests are performed here.
use der::Encode;
use openssl::x509::X509Name;
use x509_cert::{
    Certificate,
    ext::pkix::{
        CrlDistributionPoints, IssuingDistributionPoint,
        name::{DistributionPointName, GeneralName},
    },
    name::Name,
};

pub(super) const ALL_REASONS: u16 = 0x1fe;

/// A failed name/extension decode is unresolved, rather than a proof of exclusion.
pub(super) fn applicable_reasons(
    certificate: &Certificate,
    crl_issuer: &Name,
    point: Option<&IssuingDistributionPoint>,
) -> Result<u16, ()> {
    let idp_names = point
        .and_then(|point| point.distribution_point.as_ref())
        .map(|name| expand(name, crl_issuer))
        .transpose()?;
    let idp_reasons = point
        .and_then(|point| point.only_some_reasons)
        .map(|reasons| reasons.bits())
        .unwrap_or(ALL_REASONS)
        & ALL_REASONS;
    if idp_names.is_none() && same_directory_name(&certificate.tbs_certificate.issuer, crl_issuer)?
    {
        // 6.3.3's additional issuer-DP fallback also applies when the cert's
        // advertised partitions alone do not establish complete coverage.
        return Ok(idp_reasons);
    }
    let points = certificate
        .tbs_certificate
        .get::<CrlDistributionPoints>()
        .map_err(|_| ())?;
    let Some((_, points)) = points else {
        // RFC5280 6.3.3: the fallback distribution point is the certificate
        // issuer. A named URI partition cannot thereby cover an unnamed cert.
        let fallback = [GeneralName::DirectoryName(
            certificate.tbs_certificate.issuer.clone(),
        )];
        return if let Some(names) = idp_names {
            Ok(if overlaps(&fallback, &names)? {
                idp_reasons
            } else {
                0
            })
        } else {
            Ok(idp_reasons)
        };
    };
    if points.0.is_empty() {
        return Err(());
    }
    let mut coverage = 0;
    let mut unresolved = false;
    for distribution in points.0 {
        let authority_matches = if let Some(issuers) = &distribution.crl_issuer {
            point.is_some_and(|point| point.indirect_crl)
                && overlaps(issuers, &[GeneralName::DirectoryName(crl_issuer.clone())])?
        } else {
            same_directory_name(&certificate.tbs_certificate.issuer, crl_issuer)?
        };
        if !authority_matches {
            continue;
        }
        let names = if let Some(name) = &distribution.distribution_point {
            let issuer = match &distribution.crl_issuer {
                Some(issuers) => match issuers.as_slice() {
                    [GeneralName::DirectoryName(name)] => name,
                    _ if matches!(name, DistributionPointName::NameRelativeToCRLIssuer(_)) => {
                        unresolved = true;
                        continue;
                    }
                    _ => &certificate.tbs_certificate.issuer,
                },
                None => &certificate.tbs_certificate.issuer,
            };
            expand(name, issuer)?
        } else if let Some(issuers) = &distribution.crl_issuer {
            issuers.clone()
        } else {
            // A DistributionPoint must contain a name or cRLIssuer.
            unresolved = true;
            continue;
        };
        if let Some(idp) = &idp_names {
            match overlaps(&names, idp) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(()) => {
                    unresolved = true;
                    continue;
                }
            }
        }
        coverage |= idp_reasons
            & distribution
                .reasons
                .map(|reasons| reasons.bits())
                .unwrap_or(ALL_REASONS);
    }
    if unresolved && coverage != ALL_REASONS {
        Err(())
    } else {
        Ok(coverage)
    }
}

fn expand(point: &DistributionPointName, issuer: &Name) -> Result<Vec<GeneralName>, ()> {
    match point {
        DistributionPointName::FullName(names) if names.is_empty() => Err(()),
        DistributionPointName::FullName(names) => Ok(names.clone()),
        DistributionPointName::NameRelativeToCRLIssuer(relative) => {
            let mut name = issuer.clone();
            name.0.push(relative.clone());
            Ok(vec![GeneralName::DirectoryName(name)])
        }
    }
}

fn overlaps(left: &[GeneralName], right: &[GeneralName]) -> Result<bool, ()> {
    let mut unresolved = false;
    for left in left {
        for right in right {
            match same_general_name(left, right) {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(()) => unresolved = true,
            }
        }
    }
    if unresolved { Err(()) } else { Ok(false) }
}

fn same_general_name(left: &GeneralName, right: &GeneralName) -> Result<bool, ()> {
    match (left, right) {
        (GeneralName::DirectoryName(left), GeneralName::DirectoryName(right)) => {
            same_directory_name(left, right)
        }
        (GeneralName::DnsName(left), GeneralName::DnsName(right)) => {
            Ok(left.as_str().eq_ignore_ascii_case(right.as_str()))
        }
        (
            GeneralName::UniformResourceIdentifier(left),
            GeneralName::UniformResourceIdentifier(right),
        ) => {
            // The vetted URL parser normalizes scheme and host without I/O.
            let left = url::Url::parse(left.as_str()).map_err(|_| ())?;
            let right = url::Url::parse(right.as_str()).map_err(|_| ())?;
            Ok(left == right)
        }
        (GeneralName::Rfc822Name(left), GeneralName::Rfc822Name(right)) => {
            let (Some((left_local, left_domain)), Some((right_local, right_domain))) = (
                left.as_str().rsplit_once('@'),
                right.as_str().rsplit_once('@'),
            ) else {
                return Err(());
            };
            Ok(left_local == right_local && left_domain.eq_ignore_ascii_case(right_domain))
        }
        (GeneralName::OtherName(_), GeneralName::OtherName(_))
        | (GeneralName::EdiPartyName(_), GeneralName::EdiPartyName(_))
            if left != right =>
        {
            Err(())
        }
        _ => Ok(left == right),
    }
}

pub(super) fn same_directory_name(left: &Name, right: &Name) -> Result<bool, ()> {
    let left = X509Name::from_der(&left.to_der().map_err(|_| ())?).map_err(|_| ())?;
    let right = X509Name::from_der(&right.to_der().map_err(|_| ())?).map_err(|_| ())?;
    Ok(left.try_cmp(&right).map_err(|_| ())?.is_eq())
}
