//! Native engineering verification enrollment use case. The caller owns the
//! Control transaction; this layer validates the plan and delegates the single
//! multi-row write to the owner port. All authority checks remain upstream in
//! the retained application before the native command is dispatched.
use crate::domain::engineering_enrollment::{
    EngineeringEnrollmentInput, EnrollmentEntry, PlannedEnrollment,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::json;
use std::future::Future;

pub trait EngineeringEnrollmentRepository: Send {
    type Error: From<BusinessError> + Send;

    fn enroll<'a>(
        &'a mut self,
        input: &'a EngineeringEnrollmentInput,
        entries: &'a [EnrollmentEntry],
        expires_at: i64,
        now: i64,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;
}

pub async fn enroll_engineering_verification<R: EngineeringEnrollmentRepository>(
    repository: &mut R,
    input: &EngineeringEnrollmentInput,
    now: i64,
) -> Result<serde_json::Value, R::Error> {
    let PlannedEnrollment {
        entries,
        expires_at,
    } = input.validate(now).map_err(R::Error::from)?;
    repository.enroll(input, &entries, expires_at, now).await?;
    Ok(json!({
        "scope": "isolated-engineering",
        "run": input.run,
        "sourceFingerprint": input.source_fingerprint,
        "softwareVersion": input.software_version,
        "fileManifestSha256": input.file_manifest_sha256,
        "expiresAt": expires_at,
        "companyIds": input.company_ids,
        "disabledCompanyIds": input.disabled_company_ids,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_plan_is_deterministic() {
        let input = EngineeringEnrollmentInput {
            run: "abcdef123456".into(),
            source_fingerprint: "a".repeat(64),
            software_version: format!("release-{}", "a".repeat(64)),
            file_manifest_sha256: "b".repeat(64),
            company_ids: vec!["c-1".into()],
            disabled_company_ids: vec![],
            expires_at: Some(2_000),
        };
        let plan = input.validate(1_000).unwrap();
        assert_eq!(plan.expires_at, 2_000);
        assert_eq!(plan.entries.len(), 1);
    }
}
