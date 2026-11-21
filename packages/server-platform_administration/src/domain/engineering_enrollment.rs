//! Pure validation and transition plan for the native engineering verification
//! enrollment write. No IO, no SQL and no authority checks happen here; those
//! stay in the caller-owned application and composition.
use erpx_server_shared::public::BusinessError;

pub const ENROLLMENT_REASON: &str =
    "Synthetic engineering verification only; all Company/provider certification remains pending";
pub const MAX_COMPANIES: usize = 100;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineeringEnrollmentInput {
    pub run: String,
    pub source_fingerprint: String,
    pub software_version: String,
    pub file_manifest_sha256: String,
    pub company_ids: Vec<String>,
    pub disabled_company_ids: Vec<String>,
    pub expires_at: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnrollmentEntry {
    pub company_id: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedEnrollment {
    pub entries: Vec<EnrollmentEntry>,
    pub expires_at: i64,
}

fn hex(value: &str, len: usize) -> bool {
    value.len() == len && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

impl EngineeringEnrollmentInput {
    pub fn validate(&self, now: i64) -> Result<PlannedEnrollment, BusinessError> {
        let invalid = || {
            BusinessError::invalid(
                "ENGINEERING_ENROLLMENT_INVALID",
                "Provide a bounded engineering enrollment.",
            )
        };
        if !hex(&self.run, 12)
            || !hex(&self.source_fingerprint, 64)
            || !hex(&self.file_manifest_sha256, 64)
            || self.software_version != format!("release-{}", self.source_fingerprint)
        {
            return Err(invalid());
        }
        if self.company_ids.len() > MAX_COMPANIES
            || self.disabled_company_ids.len() > MAX_COMPANIES
            || self
                .company_ids
                .iter()
                .chain(&self.disabled_company_ids)
                .any(|id| id.is_empty() || id.len() > 200)
        {
            return Err(invalid());
        }
        let expires_at = self.expires_at.unwrap_or(now + 12 * 60 * 60_000);
        if expires_at <= now || expires_at > now + 24 * 60 * 60_000 {
            return Err(invalid());
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut entries =
            Vec::with_capacity(self.company_ids.len() + self.disabled_company_ids.len());
        for (ids, enabled) in [
            (&self.company_ids, true),
            (&self.disabled_company_ids, false),
        ] {
            for company_id in ids {
                if !seen.insert(company_id.clone()) {
                    return Err(invalid());
                }
                entries.push(EnrollmentEntry {
                    company_id: company_id.clone(),
                    enabled,
                });
            }
        }
        if entries.is_empty() {
            return Err(invalid());
        }
        Ok(PlannedEnrollment {
            entries,
            expires_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> EngineeringEnrollmentInput {
        EngineeringEnrollmentInput {
            run: "abcdef123456".into(),
            source_fingerprint: "a".repeat(64),
            software_version: format!("release-{}", "a".repeat(64)),
            file_manifest_sha256: "b".repeat(64),
            company_ids: vec!["company-1".into()],
            disabled_company_ids: vec![],
            expires_at: Some(2_000),
        }
    }

    #[test]
    fn plans_enabled_and_disabled_entries() {
        let mut value = input();
        value.disabled_company_ids.push("company-2".into());
        let plan = value.validate(1_000).unwrap();
        assert_eq!(plan.expires_at, 2_000);
        assert_eq!(plan.entries.len(), 2);
        assert!(plan.entries[0].enabled);
        assert!(!plan.entries[1].enabled);
    }

    #[test]
    fn rejects_duplicates_bad_build_and_bad_expiry() {
        let mut value = input();
        value.disabled_company_ids.push("company-1".into());
        assert!(value.validate(1_000).is_err());
        let mut value = input();
        value.software_version = "release-wrong".into();
        assert!(value.validate(1_000).is_err());
        let mut value = input();
        value.expires_at = Some(100);
        assert!(value.validate(1_000).is_err());
    }
}
