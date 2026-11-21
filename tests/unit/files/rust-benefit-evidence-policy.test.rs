//! Files-owned Benefits dependent/election evidence policy mapping. No
//! database or bytes involved.
use erpx_server_benefits::public::command::{
    BenefitEvidenceRequest, BenefitRecordKind, benefit_evidence_binding_type,
    benefit_evidence_policy,
};
use erpx_server_files::public::benefit_evidence::{
    BENEFITS_EVIDENCE_COMPONENT_KIND, BENEFITS_EVIDENCE_POLICY_VERSION, benefits_business_request,
};
use erpx_server_shared::public::FailureKind;

fn request(kind: BenefitRecordKind, action: &str) -> BenefitEvidenceRequest {
    let policy = benefit_evidence_policy(kind, action).unwrap();
    BenefitEvidenceRequest {
        attachment_set_ids: vec!["set-1".into()],
        target_type: policy.target_type.into(),
        purpose: policy.purpose.into(),
        component_ref: policy.component_ref.into(),
        target_id: "benefit-record-1".into(),
        expected_target_version: 2,
        command_id: "benefits-command-1".into(),
        binding_type: benefit_evidence_binding_type(kind, action),
        now: 1_800_000_000_000,
    }
}

#[test]
fn retained_actions_map_to_the_exact_business_evidence_context() {
    for (kind, action, target_type, purpose, component_ref, binding_type) in [
        (
            BenefitRecordKind::Dependent,
            "verify",
            "benefit_dependent",
            "dependent_proof",
            "verification",
            "benefits_dependent_verify",
        ),
        (
            BenefitRecordKind::Election,
            "submit",
            "benefit_election",
            "election_document",
            "employee-submit",
            "benefits_election_submit",
        ),
        (
            BenefitRecordKind::Election,
            "acknowledge",
            "benefit_election",
            "provider_proof",
            "provider-acknowledgement",
            "benefits_election_acknowledge",
        ),
    ] {
        let mapped = benefits_business_request(&request(kind, action)).unwrap();
        assert_eq!(mapped.attachment_set_ids, ["set-1"]);
        assert_eq!(mapped.target_types, [target_type]);
        assert_eq!(mapped.target_id, "benefit-record-1");
        assert_eq!(mapped.expected_target_version, 2);
        assert_eq!(mapped.command_id, "benefits-command-1");
        assert_eq!(mapped.binding_type, binding_type);
        assert_eq!(mapped.binding_id, "benefit-record-1");
        assert_eq!(mapped.contexts.len(), 1);
        assert_eq!(mapped.contexts[0].purpose_key, purpose);
        assert_eq!(
            mapped.contexts[0].component_kind,
            BENEFITS_EVIDENCE_COMPONENT_KIND
        );
        assert_eq!(
            mapped.contexts[0].component_ref.as_deref(),
            Some(component_ref)
        );
    }
    assert_eq!(BENEFITS_EVIDENCE_COMPONENT_KIND, "action");
    assert_eq!(BENEFITS_EVIDENCE_POLICY_VERSION, 1);
}

#[test]
fn unsupported_target_purpose_component_and_binding_fail_closed() {
    let mut unregistered = request(BenefitRecordKind::Dependent, "verify");
    unregistered.target_type = "benefit_plan".into();
    assert_invalid(&unregistered);

    let mut unregistered = request(BenefitRecordKind::Dependent, "verify");
    unregistered.purpose = "benefit_booklet".into();
    assert_invalid(&unregistered);

    let mut unregistered = request(BenefitRecordKind::Election, "submit");
    unregistered.component_ref = "provider-acknowledgement".into();
    assert_invalid(&unregistered);

    let mut unregistered = request(BenefitRecordKind::Election, "acknowledge");
    unregistered.binding_type = "benefits_election_submit".into();
    assert_invalid(&unregistered);

    let mut unregistered = request(BenefitRecordKind::Dependent, "verify");
    unregistered.binding_type = "record_document".into();
    assert_invalid(&unregistered);
}

#[test]
fn empty_sets_and_missing_identity_fail_closed() {
    let mut empty = request(BenefitRecordKind::Dependent, "verify");
    empty.attachment_set_ids.clear();
    assert_invalid(&empty);

    let mut too_many = request(BenefitRecordKind::Dependent, "verify");
    too_many.attachment_set_ids = (0..21).map(|index| format!("set-{index}")).collect();
    assert_invalid(&too_many);

    let mut no_target = request(BenefitRecordKind::Election, "submit");
    no_target.target_id.clear();
    assert_invalid(&no_target);

    let mut stale = request(BenefitRecordKind::Election, "submit");
    stale.expected_target_version = 0;
    assert_invalid(&stale);

    let mut no_command = request(BenefitRecordKind::Election, "submit");
    no_command.command_id.clear();
    assert_invalid(&no_command);
}

fn assert_invalid(request: &BenefitEvidenceRequest) {
    let error = benefits_business_request(request).err().unwrap();
    assert_eq!(error.code, "BENEFITS_EVIDENCE_INVALID");
    assert_eq!(error.kind, FailureKind::InvalidInput);
}
