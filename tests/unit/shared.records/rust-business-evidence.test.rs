use erpx_server_records::public::business_evidence::*;
use erpx_server_shared::public::BusinessError;

fn context(purpose: &str, kind: &str) -> EvidenceContext {
    EvidenceContext {
        purpose_key: purpose.into(),
        component_kind: kind.into(),
        component_ref: Some("submit".into()),
    }
}
fn fact(id: &str, target: &str) -> EvidenceSetFact {
    EvidenceSetFact {
        id: id.into(),
        target_type: target.into(),
    }
}
fn policy(target: &str, purpose: &str) -> EvidencePolicyFact {
    EvidencePolicyFact {
        target_type: target.into(),
        purpose_key: purpose.into(),
        component_kinds: vec!["action".into()],
    }
}
#[test]
fn first_occurrence_order_and_actual_alias_policy_are_preserved() {
    let selected = select_evidence(
        &["b".into(), "a".into(), "b".into()],
        &["stock_count".into(), "inventory_stock_count".into()],
        &[
            context("count_sheet", "action"),
            context("count_photo", "action"),
            context("count_evidence", "action"),
        ],
        &[
            fact("a", "inventory_stock_count"),
            fact("b", "inventory_stock_count"),
        ],
        &[
            policy("inventory_stock_count", "count_evidence"),
            policy("stock_count", "count_sheet"),
        ],
    )
    .unwrap()
    .unwrap();
    assert_eq!(selected.set_ids, ["b", "a"]);
    assert_eq!(selected.target_type, "inventory_stock_count");
    assert_eq!(selected.contexts, [context("count_evidence", "action")]);
}
#[test]
fn missing_foreign_duplicate_and_mixed_target_facts_cannot_authorize_a_binding() {
    let ids = ["a".into(), "b".into()];
    for facts in [
        vec![fact("a", "stock_count")],
        vec![fact("a", "stock_count"), fact("foreign", "stock_count")],
        vec![fact("a", "stock_count"), fact("a", "stock_count")],
        vec![fact("a", "stock_count"), fact("b", "inventory_stock_count")],
    ] {
        let error = select_evidence(
            &ids,
            &["stock_count".into(), "inventory_stock_count".into()],
            &[context("count_sheet", "action")],
            &facts,
            &[policy("stock_count", "count_sheet")],
        )
        .unwrap_err();
        assert_eq!(error.code, "ATTACHMENT_BINDING_CONTEXT");
    }
}
#[test]
fn context_requires_the_matching_target_purpose_and_component_kind() {
    for policies in [
        vec![policy("other", "count_sheet")],
        vec![policy("stock_count", "other")],
        vec![policy("stock_count", "count_sheet")],
    ] {
        let error = select_evidence(
            &["a".into()],
            &["stock_count".into()],
            &[context("count_sheet", "record")],
            &[fact("a", "stock_count")],
            &policies,
        )
        .unwrap_err();
        assert_eq!(error.code, "ATTACHMENT_BINDING_CONTEXT");
    }
}

struct FilesPort {
    reads: Vec<Vec<String>>,
    bindings: Vec<BusinessEvidenceBinding>,
    facts: Vec<EvidenceSetFact>,
    policies: Vec<EvidencePolicyFact>,
}
impl BusinessEvidenceFilesPort for FilesPort {
    type Error = BusinessError;
    type Prepared = usize;
    async fn read_set_targets(
        &mut self,
        ids: &[String],
    ) -> Result<Vec<EvidenceSetFact>, BusinessError> {
        self.reads.push(ids.to_vec());
        Ok(self.facts.clone())
    }
    fn policy_facts(&self) -> &[EvidencePolicyFact] {
        &self.policies
    }
    async fn prepare_binding(
        &mut self,
        input: BusinessEvidenceBinding,
    ) -> Result<usize, BusinessError> {
        self.bindings.push(input);
        Ok(self.bindings.len())
    }
}
fn port() -> FilesPort {
    FilesPort {
        reads: vec![],
        bindings: vec![],
        facts: vec![fact("a", "stock_count"), fact("b", "stock_count")],
        policies: vec![policy("stock_count", "count_sheet")],
    }
}
fn request(ids: Vec<String>) -> BusinessEvidenceRequest {
    BusinessEvidenceRequest {
        attachment_set_ids: ids,
        target_types: vec!["stock_count".into(), "inventory_stock_count".into()],
        contexts: vec![context("count_sheet", "action")],
        target_id: "count-original".into(),
        expected_target_version: 3,
        command_id: "original-command".into(),
        binding_type: "stock_count_submission".into(),
        binding_id: "count-submit-original-command".into(),
    }
}
#[tokio::test]
async fn empty_evidence_bypasses_files_and_nonempty_binding_keeps_the_original_command_contract() {
    let mut files = port();
    assert_eq!(
        prepare_business_evidence(&mut files, request(vec![]))
            .await
            .unwrap(),
        None
    );
    assert!(files.reads.is_empty() && files.bindings.is_empty());
    assert_eq!(
        prepare_business_evidence(
            &mut files,
            request(vec!["b".into(), "a".into(), "b".into()])
        )
        .await
        .unwrap(),
        Some(1)
    );
    assert_eq!(files.reads, vec![vec!["b", "a"]]);
    let plan = &files.bindings[0];
    assert_eq!(plan.selection.set_ids, ["b", "a"]);
    assert_eq!(plan.target_id, "count-original");
    assert_eq!(plan.expected_target_version, 3);
    assert_eq!(plan.command_id, "original-command");
    assert_eq!(plan.binding_type, "stock_count_submission");
    assert_eq!(plan.binding_id, "count-submit-original-command");
}
#[tokio::test]
async fn mismatched_scoped_facts_stop_before_files_binding() {
    let mut files = port();
    files.facts.pop();
    assert_eq!(
        prepare_business_evidence(&mut files, request(vec!["a".into(), "b".into()]))
            .await
            .unwrap_err()
            .code,
        "ATTACHMENT_BINDING_CONTEXT"
    );
    assert!(files.bindings.is_empty());
}
