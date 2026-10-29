//! Pure policy for binding a current Field visit record-document set.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FieldBindManifestItem {
    pub link_id: String,
    pub source_id: String,
    pub purpose_key: String,
    pub checksum_sha256: String,
    pub media_type: String,
    pub bytes: i64,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct FieldBindLink {
    pub id: String,
    pub state: String,
    pub source_id: Option<String>,
    pub source_state: Option<String>,
    pub checksum: Option<String>,
    pub media_type: Option<String>,
    pub bytes: Option<i64>,
    pub binding_type: Option<String>,
    pub binding_id: Option<String>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct FieldBindSet {
    pub id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub target_id: String,
    pub target_type: String,
    pub component_kind: String,
    pub component_ref: Option<String>,
    pub purpose_key: String,
    pub expected_version: i64,
    pub policy_version: i64,
    pub required_count: i64,
    pub state: String,
    pub bound_command_id: Option<String>,
    pub links: Vec<FieldBindLink>,
}
fn fail(kind: FailureKind, code: &'static str, msg: &'static str) -> BusinessError {
    BusinessError::new(kind, code, msg)
}
pub fn prepare_field_bind(
    set: &FieldBindSet,
    expected: i64,
) -> Result<Vec<FieldBindManifestItem>, BusinessError> {
    if !super::field_visit_link::valid_field_purpose(
        &set.target_type,
        &set.component_kind,
        &set.purpose_key,
    ) {
        return Err(fail(
            FailureKind::NotFound,
            "ATTACHMENT_SET_NOT_FOUND",
            "File set not found.",
        ));
    }
    if set.expected_version != expected {
        return Err(fail(
            FailureKind::Conflict,
            "ATTACHMENT_SET_STALE",
            "Create a record-document set for the current record version.",
        ));
    }
    if set.state != "draft" {
        return Err(fail(
            FailureKind::Conflict,
            "ATTACHMENT_SET_LOCKED",
            "A file set was already bound or closed.",
        ));
    }
    if set.policy_version != 1 {
        return Err(fail(
            FailureKind::Conflict,
            "ATTACHMENT_BINDING_CONTEXT",
            "The file policy changed.",
        ));
    }
    let links: Vec<_> = set.links.iter().filter(|l| l.state == "draft").collect();
    if links.is_empty() {
        return Err(fail(
            FailureKind::InvalidInput,
            "ATTACHMENT_SET_EMPTY",
            "Bind at least one available file.",
        ));
    }
    if links
        .iter()
        .any(|l| l.source_state.as_deref() != Some("available"))
    {
        return Err(fail(
            FailureKind::Conflict,
            "ATTACHMENT_SCAN_PENDING",
            "Every active file must pass malware scanning before this action.",
        ));
    }
    if i64::try_from(links.len()).unwrap_or(i64::MAX) < set.required_count {
        return Err(fail(
            FailureKind::InvalidInput,
            "ATTACHMENT_REQUIRED",
            "Required evidence is missing.",
        ));
    }
    links
        .into_iter()
        .map(|l| {
            Ok(FieldBindManifestItem {
                link_id: l.id.clone(),
                source_id: l.source_id.clone().ok_or_else(|| {
                    fail(
                        FailureKind::Conflict,
                        "ATTACHMENT_SCAN_PENDING",
                        "Every active file must pass malware scanning before this action.",
                    )
                })?,
                purpose_key: set.purpose_key.clone(),
                checksum_sha256: l.checksum.clone().ok_or_else(|| {
                    fail(
                        FailureKind::Conflict,
                        "ATTACHMENT_SCAN_PENDING",
                        "Every active file must pass malware scanning before this action.",
                    )
                })?,
                media_type: l.media_type.clone().ok_or_else(|| {
                    fail(
                        FailureKind::Conflict,
                        "ATTACHMENT_SCAN_PENDING",
                        "Every active file must pass malware scanning before this action.",
                    )
                })?,
                bytes: l.bytes.ok_or_else(|| {
                    fail(
                        FailureKind::Conflict,
                        "ATTACHMENT_SCAN_PENDING",
                        "Every active file must pass malware scanning before this action.",
                    )
                })?,
            })
        })
        .collect()
}
