//! Files-owned Learning evidence policy mapping. No database or bytes involved.
use erpx_server_files::public::learning_evidence::{
    LEARNING_EVIDENCE_POLICY_VERSION, learning_business_request,
};
use erpx_server_learning::public::learning::{
    ASSIGNMENT_EVIDENCE_BINDING_TYPE, ASSIGNMENT_EVIDENCE_COMPONENT_KIND,
    ASSIGNMENT_EVIDENCE_PURPOSE, ASSIGNMENT_EVIDENCE_TARGET_TYPE, LearningEvidenceRequest,
    assignment_evidence_request,
};
use erpx_server_shared::public::FailureKind;

fn evidence_request(component_ref: &str) -> LearningEvidenceRequest {
    LearningEvidenceRequest {
        attachment_set_ids: vec!["set-1".into()],
        target_id: "assignment-1".into(),
        target_type: ASSIGNMENT_EVIDENCE_TARGET_TYPE.into(),
        purpose_key: ASSIGNMENT_EVIDENCE_PURPOSE.into(),
        component_kind: ASSIGNMENT_EVIDENCE_COMPONENT_KIND.into(),
        component_ref: Some(component_ref.into()),
        expected_target_version: 3,
        command_id: "learning-command-1".into(),
        binding_type: ASSIGNMENT_EVIDENCE_BINDING_TYPE.into(),
        binding_id: "assignment-1".into(),
    }
}

#[test]
fn completion_and_waiver_map_to_the_exact_business_evidence_context() {
    for component in ["completion", "waiver"] {
        let mapped = learning_business_request(&evidence_request(component)).unwrap();
        assert_eq!(mapped.attachment_set_ids, ["set-1"]);
        assert_eq!(mapped.target_types, [ASSIGNMENT_EVIDENCE_TARGET_TYPE]);
        assert_eq!(mapped.target_id, "assignment-1");
        assert_eq!(mapped.expected_target_version, 3);
        assert_eq!(mapped.command_id, "learning-command-1");
        assert_eq!(mapped.binding_type, ASSIGNMENT_EVIDENCE_BINDING_TYPE);
        assert_eq!(mapped.binding_id, "assignment-1");
        assert_eq!(mapped.contexts.len(), 1);
        assert_eq!(mapped.contexts[0].purpose_key, ASSIGNMENT_EVIDENCE_PURPOSE);
        assert_eq!(
            mapped.contexts[0].component_kind,
            ASSIGNMENT_EVIDENCE_COMPONENT_KIND
        );
        assert_eq!(mapped.contexts[0].component_ref.as_deref(), Some(component));
    }
    assert_eq!(LEARNING_EVIDENCE_POLICY_VERSION, 1);
}

#[test]
fn unsupported_components_and_binding_types_fail_closed() {
    let mut request = evidence_request("start");
    let error = learning_business_request(&request).err().unwrap();
    assert_eq!(error.code, "LEARNING_EVIDENCE_SOURCE_INVALID");
    assert_eq!(error.kind, FailureKind::Unavailable);

    request = evidence_request("completion");
    request.binding_type = "other_binding".into();
    assert_eq!(
        learning_business_request(&request).err().unwrap().code,
        "LEARNING_EVIDENCE_SOURCE_INVALID"
    );

    request = evidence_request("completion");
    request.target_type = "learning_course".into();
    assert_eq!(
        learning_business_request(&request).err().unwrap().code,
        "LEARNING_EVIDENCE_SOURCE_INVALID"
    );
}

#[test]
fn empty_sets_and_missing_binding_identity_use_the_learning_validation_error() {
    let mut request = evidence_request("completion");
    request.attachment_set_ids.clear();
    assert_eq!(
        learning_business_request(&request).err().unwrap().code,
        "LEARNING_EVIDENCE_SOURCE_INVALID"
    );

    let mut request = evidence_request("completion");
    request.binding_id.clear();
    assert_eq!(
        learning_business_request(&request).err().unwrap().code,
        "LEARNING_EVIDENCE_SOURCE_INVALID"
    );
}

#[test]
fn the_learning_domain_builder_produces_the_same_context() {
    let input = erpx_server_learning::public::learning::ValidatedAssignmentChange {
        action: erpx_server_learning::public::learning::AssignmentAction::Complete,
        expected_version: 2,
        reason: "Course completed".into(),
        attendance: None,
        score_bps: Some(9000),
        evidence: Some("Reviewed provider result.".into()),
        certificate_valid_from: None,
        certificate_expires_on: None,
        attachment_set_ids: vec!["set-1".into()],
    };
    let built = assignment_evidence_request("assignment-1", &input, "command-1")
        .unwrap()
        .unwrap();
    let mapped = learning_business_request(&built).unwrap();
    assert_eq!(built.component_ref.as_deref(), Some("completion"));
    assert_eq!(
        mapped.contexts[0].component_ref.as_deref(),
        Some("completion")
    );
}
