//! The caller supplies a Files port bound to the same retained owner unit.
//! No default implementation, storage access, authorization or commit is here.
use crate::domain::business_evidence::{
    EvidenceContext, EvidencePolicyFact, EvidenceSelection, EvidenceSetFact, select_evidence,
    unique_evidence_ids,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;

pub struct BusinessEvidenceRequest {
    pub attachment_set_ids: Vec<String>,
    pub target_types: Vec<String>,
    pub contexts: Vec<EvidenceContext>,
    pub target_id: String,
    pub expected_target_version: i64,
    pub command_id: String,
    pub binding_type: String,
    pub binding_id: String,
}
pub struct BusinessEvidenceBinding {
    pub selection: EvidenceSelection,
    pub target_id: String,
    pub expected_target_version: i64,
    pub command_id: String,
    pub binding_type: String,
    pub binding_id: String,
}
pub trait BusinessEvidenceFilesPort: Send {
    type Error: From<BusinessError> + Send;
    type Prepared: Send;
    /// Exact Company-scoped Files facts, whose witness must survive commit.
    fn read_set_targets(
        &mut self,
        ids: &[String],
    ) -> impl Future<Output = Result<Vec<EvidenceSetFact>, Self::Error>> + Send;
    fn policy_facts(&self) -> &[EvidencePolicyFact];
    /// Files enforces current target authority, policy, source and binding rules.
    fn prepare_binding(
        &mut self,
        input: BusinessEvidenceBinding,
    ) -> impl Future<Output = Result<Self::Prepared, Self::Error>> + Send;
}
pub async fn prepare_business_evidence<P: BusinessEvidenceFilesPort>(
    port: &mut P,
    input: BusinessEvidenceRequest,
) -> Result<Option<P::Prepared>, P::Error> {
    // Retained empty-input behavior bypasses Files, including feature checks.
    if input.attachment_set_ids.is_empty() {
        return Ok(None);
    }
    let ids = unique_evidence_ids(&input.attachment_set_ids);
    let facts = port.read_set_targets(&ids).await?;
    let selection = select_evidence(
        &ids,
        &input.target_types,
        &input.contexts,
        &facts,
        port.policy_facts(),
    )?
    .expect("nonempty selected evidence");
    port.prepare_binding(BusinessEvidenceBinding {
        selection,
        target_id: input.target_id,
        expected_target_version: input.expected_target_version,
        command_id: input.command_id,
        binding_type: input.binding_type,
        binding_id: input.binding_id,
    })
    .await
    .map(Some)
}
