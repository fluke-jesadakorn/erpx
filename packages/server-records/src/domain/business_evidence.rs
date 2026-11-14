//! Portable retained selection policy. Files supplies current scoped facts and
//! its registered policy; these DTOs confer no read or binding authority.
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceSetFact {
    pub id: String,
    pub target_type: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceContext {
    pub purpose_key: String,
    pub component_kind: String,
    pub component_ref: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidencePolicyFact {
    pub target_type: String,
    pub purpose_key: String,
    pub component_kinds: Vec<String>,
    pub policy_version: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceSelection {
    pub set_ids: Vec<String>,
    pub target_type: String,
    pub contexts: Vec<EvidenceContext>,
    pub policy_version: i64,
}
pub fn unique_evidence_ids(ids: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.iter()
        .filter(|id| seen.insert(id.as_str()))
        .cloned()
        .collect()
}
pub fn select_evidence(
    ids: &[String],
    allowed_targets: &[String],
    requested: &[EvidenceContext],
    facts: &[EvidenceSetFact],
    policies: &[EvidencePolicyFact],
) -> Result<Option<EvidenceSelection>, BusinessError> {
    if ids.is_empty() {
        return Ok(None);
    }
    let ids = unique_evidence_ids(ids);
    let target = facts.first().map(|f| f.target_type.as_str());
    let observed: HashSet<_> = facts.iter().map(|f| f.id.as_str()).collect();
    if facts.len() != ids.len()
        || observed.len() != ids.len()
        || ids.iter().any(|id| !observed.contains(id.as_str()))
    {
        return Err(failure(
            FailureKind::NotFound,
            "ATTACHMENT_SET_NOT_FOUND",
            "Attachment set not found.",
        ));
    }
    if target.is_none_or(|t| !allowed_targets.iter().any(|allowed| allowed == t))
        || facts.iter().any(|f| Some(f.target_type.as_str()) != target)
    {
        return Err(failure(
            FailureKind::InvalidInput,
            "ATTACHMENT_SET_TARGET",
            "A file set belongs to another record.",
        ));
    }
    let target_type = target.expect("nonempty matching scoped facts").to_owned();
    let mut contexts = Vec::new();
    let mut policy_version = None;
    for context in requested {
        let matched: Vec<_> = policies
            .iter()
            .filter(|p| {
                p.target_type == target_type
                    && p.purpose_key == context.purpose_key
                    && p.component_kinds.contains(&context.component_kind)
            })
            .collect();
        let Some(policy) = matched.first() else {
            continue;
        };
        if matched
            .iter()
            .any(|candidate| candidate.policy_version != policy.policy_version)
            || policy_version.is_some_and(|version| version != policy.policy_version)
        {
            return Err(failure(
                FailureKind::InvalidInput,
                "ATTACHMENT_POLICY_UNKNOWN",
                "This file purpose is not registered for the selected record and context.",
            ));
        }
        policy_version = Some(policy.policy_version);
        contexts.push(context.clone());
    }
    if contexts.is_empty() {
        return Err(failure(
            FailureKind::InvalidInput,
            "ATTACHMENT_POLICY_UNKNOWN",
            "This file purpose is not registered for the selected record and context.",
        ));
    }
    Ok(Some(EvidenceSelection {
        set_ids: ids,
        target_type,
        contexts,
        policy_version: policy_version.expect("nonempty matching policy"),
    }))
}
fn failure(kind: FailureKind, code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(kind, code, message)
}
