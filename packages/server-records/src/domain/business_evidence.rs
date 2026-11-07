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
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceSelection {
    pub set_ids: Vec<String>,
    pub target_type: String,
    pub contexts: Vec<EvidenceContext>,
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
        || target.is_none_or(|t| !allowed_targets.iter().any(|allowed| allowed == t))
        || facts.iter().any(|f| Some(f.target_type.as_str()) != target)
    {
        return Err(invalid("Choose files for this exact owner and action."));
    }
    let target_type = target.expect("nonempty matching scoped facts").to_owned();
    let contexts: Vec<_> = requested
        .iter()
        .filter(|context| {
            policies.iter().any(|p| {
                p.target_type == target_type
                    && p.purpose_key == context.purpose_key
                    && p.component_kinds.contains(&context.component_kind)
            })
        })
        .cloned()
        .collect();
    if contexts.is_empty() {
        return Err(invalid(
            "This owner has no registered file context for this action.",
        ));
    }
    Ok(Some(EvidenceSelection {
        set_ids: ids,
        target_type,
        contexts,
    }))
}
fn invalid(message: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "ATTACHMENT_BINDING_CONTEXT",
        message,
    )
}
