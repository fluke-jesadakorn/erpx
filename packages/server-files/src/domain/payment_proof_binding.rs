//! Pure Files policy for Finance payment confirmation. No actor or target authority is implied.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentProofBindingFeatures {
    pub enabled: bool,
    pub read_only: bool,
    pub uploads_frozen: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentProofManifestItem {
    pub link_id: String,
    pub source_id: String,
    pub purpose_key: String,
    pub checksum_sha256: String,
    pub media_type: String,
    pub bytes: i64,
}
#[derive(Clone, Debug)]
pub struct PaymentProofBindingSet {
    pub id: String,
    pub target_type: String,
    pub target_id: String,
    pub purpose: String,
    pub component: String,
    pub component_ref: Option<String>,
    pub state: String,
    pub expected_version: i64,
    pub policy_version: i64,
    pub required_count: i64,
    pub compatibility_created: bool,
    pub links: Vec<PaymentProofBindingLink>,
}
#[derive(Clone, Debug)]
pub struct PaymentProofBindingLink {
    pub id: String,
    pub state: String,
    pub source_id: Option<String>,
    pub source_state: Option<String>,
    pub checksum: Option<String>,
    pub media_type: Option<String>,
    pub bytes: Option<i64>,
}
pub(crate) fn trim_js(value: &str) -> &str {
    value.trim_matches(|c: char| matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'))
}
fn fail(kind: FailureKind, code: &'static str, msg: &'static str) -> BusinessError {
    BusinessError::new(kind, code, msg)
}
/// Preserve the first occurrence of each set supplied to the Finance command.
pub fn unique_set_ids(ids: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    ids.iter()
        .filter(|id| seen.insert((*id).clone()))
        .cloned()
        .collect()
}
pub fn prepare_payment_proof_binding(
    sets: &[PaymentProofBindingSet],
    target_type: &str,
    target_id: &str,
    version: i64,
    features: &PaymentProofBindingFeatures,
) -> Result<Vec<Vec<PaymentProofManifestItem>>, BusinessError> {
    if sets.is_empty() || sets.len() > 20 {
        return Err(fail(
            FailureKind::InvalidInput,
            "ATTACHMENT_SETS_REQUIRED",
            "Choose one to twenty file sets.",
        ));
    }
    let compatibility = !features.enabled && sets.iter().all(|s| s.compatibility_created);
    if !features.enabled && !compatibility {
        return Err(fail(
            FailureKind::NotFound,
            "ATTACHMENTS_DISABLED",
            "The attachment platform is not enabled for this release.",
        ));
    }
    if compatibility && features.read_only {
        return Err(fail(
            FailureKind::Unavailable,
            "ATTACHMENTS_READ_ONLY",
            "Attachment intake is disabled during rollback.",
        ));
    }
    if features.uploads_frozen {
        return Err(fail(
            FailureKind::Unavailable,
            "ATTACHMENT_UPLOAD_FREEZE",
            "File binding is briefly paused for storage reconciliation.",
        ));
    }
    if target_type != "finance_payment" {
        return Err(fail(
            FailureKind::InvalidInput,
            "ATTACHMENT_BINDING_CONTEXT",
            "Choose files for this exact owner and action.",
        ));
    }
    let mut manifest = Vec::new();
    let mut prefixes = Vec::new();
    let mut prefix_bytes = 0usize;
    for set in sets {
        if set.target_type != target_type || set.target_id != target_id {
            return Err(fail(
                FailureKind::InvalidInput,
                "ATTACHMENT_SET_TARGET",
                "A file set belongs to another record.",
            ));
        }
        if set.component != "action"
            || set.component_ref.as_deref().map(trim_js) != Some("bank-confirmation")
            || set.purpose != "payment_proof"
        {
            return Err(fail(
                FailureKind::InvalidInput,
                "ATTACHMENT_BINDING_CONTEXT",
                "Choose only files prepared for this exact domain action and component.",
            ));
        }
        if set.state != "draft" {
            return Err(fail(
                FailureKind::Conflict,
                "ATTACHMENT_SET_LOCKED",
                "A file set was already bound or closed.",
            ));
        }
        if set.expected_version != version {
            return Err(fail(
                FailureKind::Conflict,
                "ATTACHMENT_SET_STALE",
                "Create a file set for the current record version.",
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
        if (links.len() as i64) < set.required_count {
            return Err(fail(
                FailureKind::InvalidInput,
                "ATTACHMENT_REQUIRED",
                "Required evidence is missing.",
            ));
        }
        for l in links {
            manifest.push(PaymentProofManifestItem {
                link_id: l.id.clone(),
                source_id: l.source_id.clone().ok_or_else(|| {
                    fail(
                        FailureKind::Conflict,
                        "ATTACHMENT_SCAN_PENDING",
                        "The file source changed.",
                    )
                })?,
                purpose_key: set.purpose.clone(),
                checksum_sha256: l.checksum.clone().ok_or_else(|| {
                    fail(
                        FailureKind::Conflict,
                        "ATTACHMENT_SCAN_PENDING",
                        "The file source changed.",
                    )
                })?,
                media_type: l.media_type.clone().ok_or_else(|| {
                    fail(
                        FailureKind::Conflict,
                        "ATTACHMENT_SCAN_PENDING",
                        "The file source changed.",
                    )
                })?,
                bytes: l.bytes.ok_or_else(|| {
                    fail(
                        FailureKind::Conflict,
                        "ATTACHMENT_SCAN_PENDING",
                        "The file source changed.",
                    )
                })?,
            });
        }
        let bytes = serde_json::to_vec(&manifest)
            .map_err(|_| {
                fail(
                    FailureKind::Unavailable,
                    "ATTACHMENT_SOURCE_LIMIT",
                    "The Payment evidence exceeds its bounded source budget.",
                )
            })?
            .len();
        prefix_bytes = prefix_bytes.checked_add(bytes).ok_or_else(|| {
            fail(
                FailureKind::Unavailable,
                "ATTACHMENT_SOURCE_LIMIT",
                "The Payment evidence exceeds its bounded source budget.",
            )
        })?;
        if prefix_bytes > 4 * 1024 * 1024 {
            return Err(fail(
                FailureKind::Unavailable,
                "ATTACHMENT_SOURCE_LIMIT",
                "The Payment evidence exceeds its bounded source budget.",
            ));
        }
        prefixes.push(manifest.clone());
    }
    Ok(prefixes)
}
