//! Pure types and validation for the native Files work-evidence read.
//!
//! This is the domain authority for the two Files-owned SELECTs retired from
//! `adapters/work-read-facts.ts`. Row field names are deliberately snake_case
//! so the native payload matches the retired TypeScript `SqlResult` contract
//! byte for byte; no presentation-layer remapping is permitted.

use serde::{Deserialize, Serialize};

/// Bounded discriminator for the two Files-owned evidence reads. The wire
/// accepts no other value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkEvidenceKind {
    Sets,
    Legacy,
}

impl WorkEvidenceKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "sets" => Some(Self::Sets),
            "legacy" => Some(Self::Legacy),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sets => "sets",
            Self::Legacy => "legacy",
        }
    }
}

/// One complete evidence row. `bytes`, `media_type`, `source_state` and
/// `set_id` are nullable because both retired statements projected optional
/// joins; the native route keeps those nulls instead of coercing them.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkEvidenceRow {
    pub id: String,
    pub set_id: Option<String>,
    pub purpose_key: String,
    pub original_name: String,
    pub bytes: Option<i64>,
    pub media_type: Option<String>,
    pub source_state: Option<String>,
    pub link_state: String,
    pub preview_available: bool,
    pub created_at: i64,
}

/// Envelope returned by the native route. `rows` is the complete authority;
/// the TypeScript transport only validates and forwards these fields.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkEvidenceResponse {
    pub kind: String,
    pub rows: Vec<WorkEvidenceRow>,
}

/// Company/record identifiers follow the same bounded character set used by
/// the other native owner read routers. Holding isolation is enforced by the
/// target table in `http.rs`, never by trusting a caller-supplied company.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// Parse the rollback feature flag carried on the wire. Missing means the
/// normal (draft+bound) read; any other token than `true`/`false` is invalid.
pub fn parse_read_only(value: Option<&str>) -> Result<bool, ()> {
    match value {
        None => Ok(false),
        Some("false") => Ok(false),
        Some("true") => Ok(true),
        Some(_) => Err(()),
    }
}

/// Bounded discriminator for the Files-owned personnel-evidence reads retired
/// from `people-evaluation-facts.ts` and `records-employee-requests-owned-facts.ts`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersonnelEvidenceKind {
    EvaluationFiles,
    EvaluationLink,
    EmployeeRequestAttachments,
}

impl PersonnelEvidenceKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "evaluation-files" => Some(Self::EvaluationFiles),
            "evaluation-link" => Some(Self::EvaluationLink),
            "employee-request-attachments" => Some(Self::EmployeeRequestAttachments),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::EvaluationFiles => "evaluation-files",
            Self::EvaluationLink => "evaluation-link",
            Self::EmployeeRequestAttachments => "employee-request-attachments",
        }
    }
}

/// Envelope for the personnel-evidence reads. `rows` is the complete authority;
/// the TypeScript transport only validates and forwards these fields.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PersonnelEvidenceResponse {
    pub kind: String,
    pub rows: Vec<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    #[test]
    fn kind_accepts_only_registered_discriminators() {
        assert_eq!(WorkEvidenceKind::parse("sets"), Some(WorkEvidenceKind::Sets));
        assert_eq!(WorkEvidenceKind::parse("legacy"), Some(WorkEvidenceKind::Legacy));
        assert_eq!(WorkEvidenceKind::parse("Sets"), None);
        assert_eq!(WorkEvidenceKind::parse(""), None);
        assert_eq!(WorkEvidenceKind::Sets.as_str(), "sets");
        assert_eq!(WorkEvidenceKind::Legacy.as_str(), "legacy");
    }

    #[test]
    fn identifiers_are_bounded() {
        assert!(valid_id("company-1"));
        assert!(valid_id("leave.request:001"));
        assert!(!valid_id(""));
        assert!(!valid_id("bad company"));
        assert!(!valid_id(&"x".repeat(201)));
        assert!(!valid_id("bad/id"));
    }

    #[test]
    fn read_only_flag_fails_closed_on_unknown_token() {
        assert_eq!(parse_read_only(None), Ok(false));
        assert_eq!(parse_read_only(Some("false")), Ok(false));
        assert_eq!(parse_read_only(Some("true")), Ok(true));
        assert_eq!(parse_read_only(Some("TRUE")), Err(()));
        assert_eq!(parse_read_only(Some("1")), Err(()));
    }

    #[test]
    fn personnel_kind_accepts_only_registered_discriminators() {
        assert_eq!(
            PersonnelEvidenceKind::parse("evaluation-files"),
            Some(PersonnelEvidenceKind::EvaluationFiles)
        );
        assert_eq!(
            PersonnelEvidenceKind::parse("evaluation-link"),
            Some(PersonnelEvidenceKind::EvaluationLink)
        );
        assert_eq!(
            PersonnelEvidenceKind::parse("employee-request-attachments"),
            Some(PersonnelEvidenceKind::EmployeeRequestAttachments)
        );
        assert_eq!(PersonnelEvidenceKind::parse("EvaluationFiles"), None);
        assert_eq!(PersonnelEvidenceKind::parse(""), None);
        assert_eq!(PersonnelEvidenceKind::EvaluationLink.as_str(), "evaluation-link");
    }

    /// Result oracle: a fixed projected row is serialized to the exact JSON
    /// shape the TypeScript transport validates. The TS contract test pins the
    /// same bytes; drift in either direction fails one of the two hashes.
    #[test]
    fn result_oracle_sha256() {
        let rows = vec![
            WorkEvidenceRow {
                id: "link-1".into(),
                set_id: Some("set-1".into()),
                purpose_key: "medical_certificate".into(),
                original_name: "certificate.pdf".into(),
                bytes: Some(4096),
                media_type: Some("application/pdf".into()),
                source_state: Some("available".into()),
                link_state: "bound".into(),
                preview_available: true,
                created_at: 1_760_000_000_000,
            },
            WorkEvidenceRow {
                id: "link-2".into(),
                set_id: Some("set-2".into()),
                purpose_key: "medical_certificate".into(),
                original_name: "second.pdf".into(),
                bytes: None,
                media_type: None,
                source_state: None,
                link_state: "draft".into(),
                preview_available: false,
                created_at: 1_760_000_000_001,
            },
        ];
        let body = WorkEvidenceResponse {
            kind: "sets".into(),
            rows,
        };
        let json = serde_json::to_string(&body).expect("response serializes");
        let digest = sha2::Sha256::digest(json.as_bytes());
        let hash: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(
            hash,
            "b10b0ac1f263f64aa48d591ae87edd8f1bc668ab1634dabb7699a1085ea29fce"
        );
    }
}
