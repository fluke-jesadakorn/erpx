//! Bounded native observation of engineering verification context rows.
//! No SQL, database IO, attestation, identity or authority decision lives here.
use erpx_platform_shared_server::public::BusinessError;

/// The three build facts the retained TS `prior` statement reads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineeringVerificationPrior {
    pub source_fingerprint: String,
    pub software_version: String,
    pub release_manifest_sha256: String,
}

/// Inputs of the retained TS verification predicate's context sub-read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveVerificationQuery<'a> {
    pub company_id: &'a str,
    pub run: &'a str,
    pub source_fingerprint: &'a str,
    pub software_version: &'a str,
    pub file_manifest_sha256: &'a str,
}

fn hexadecimal(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl ActiveVerificationQuery<'_> {
    /// Bound the read to the same run/build shape the retained TS authority
    /// accepts. This is input validation only, not an attestation or a grant.
    pub fn validate(&self) -> Result<(), BusinessError> {
        let invalid = || {
            BusinessError::invalid(
                "ENGINEERING_VERIFICATION_READ_INVALID",
                "Provide a bounded engineering verification read.",
            )
        };
        if !hexadecimal(self.run, 12)
            || !hexadecimal(self.source_fingerprint, 64)
            || !hexadecimal(self.file_manifest_sha256, 64)
            || self.software_version != format!("release-{}", self.source_fingerprint)
            || self.company_id.is_empty()
            || self.company_id.len() > 200
        {
            return Err(invalid());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_query_shape_is_enforced() {
        let source = "a".repeat(64);
        let version = format!("release-{source}");
        let manifest = "b".repeat(64);
        let valid = ActiveVerificationQuery {
            company_id: "company-1",
            run: "abcdef123456",
            source_fingerprint: &source,
            software_version: &version,
            file_manifest_sha256: &manifest,
        };
        assert!(valid.validate().is_ok());
        assert!(ActiveVerificationQuery {
            run: "xyz",
            ..valid
        }
        .validate()
        .is_err());
        assert!(ActiveVerificationQuery {
            company_id: "",
            ..valid
        }
        .validate()
        .is_err());
        assert!(ActiveVerificationQuery {
            software_version: "other",
            ..valid
        }
        .validate()
        .is_err());
    }
}
