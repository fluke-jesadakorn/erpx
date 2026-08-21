use chrono::{Duration, TimeZone, Utc};
use erpx_integration_contracts::{BoundedContext, RecordRef};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuthorityScope, CommandId, HumanDecisionProof, SessionId,
};
use uuid::Uuid;

use super::*;

fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}

fn scope() -> AuthorityScope {
    AuthorityScope::Company {
        holding_id: Uuid::from_u128(1),
        company_id: Uuid::from_u128(2),
    }
}

fn contract() -> LegalContract {
    LegalContract::create(
        LegalContractId::try_from_uuid(Uuid::from_u128(10)).unwrap(),
        scope(),
        "CTR-010".into(),
        "Supplier agreement".into(),
        "legal/contracts/ctr-010/v1".into(),
        format!("sha256:{}", "a".repeat(64)),
        at() + Duration::days(30),
        "creator".into(),
        at(),
    )
    .unwrap()
}

fn command_id() -> CommandId {
    CommandId::try_from_uuid(Uuid::from_u128(90)).unwrap()
}

fn human_decision(
    principal_ref: &str,
    authority_scope: AuthorityScope,
    command_id: CommandId,
    decided_at: DateTime<Utc>,
) -> HumanDecisionProof {
    HumanDecisionProof::try_new(
        ActorAttribution {
            principal_ref: principal_ref.into(),
            kind: ActorKind::HoldingUser,
            session_id: Some(SessionId::try_from_uuid(Uuid::from_u128(91)).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        authority_scope,
        command_id.to_string(),
        decided_at,
    )
    .unwrap()
}

fn versioned_document() -> LegalDocument {
    let mut document = LegalDocument::create(
        LegalDocumentId::try_from_uuid(Uuid::from_u128(20)).unwrap(),
        scope(),
        "Legal opinion".into(),
        at(),
    )
    .unwrap();
    document
        .add_version(
            "legal/documents/opinion/v1".into(),
            format!("sha256:{}", "b".repeat(64)),
            "author".into(),
            at() + Duration::minutes(1),
        )
        .unwrap();
    document
}

#[test]
fn contract_requires_separated_approval_and_retention_before_terminal_archive() {
    let mut contract = contract();
    contract
        .add_party(
            ContractParty::try_new(Uuid::from_u128(11), "supplier".into()).unwrap(),
            at() + Duration::minutes(1),
        )
        .unwrap();
    contract
        .transition(
            ContractTransition::SubmitForReview,
            "creator",
            at() + Duration::minutes(2),
        )
        .unwrap();
    assert_eq!(
        contract.approve(
            human_decision(
                "creator",
                scope(),
                command_id(),
                at() + Duration::minutes(3),
            ),
            command_id(),
            at() + Duration::minutes(3),
        ),
        Err(LegalDomainError::ApprovalSeparation)
    );
    contract
        .approve(
            human_decision(
                "reviewer",
                scope(),
                command_id(),
                at() + Duration::minutes(3),
            ),
            command_id(),
            at() + Duration::minutes(3),
        )
        .unwrap();
    assert_eq!(contract.approved_by(), Some("reviewer"));
    contract
        .transition(
            ContractTransition::Activate,
            "reviewer",
            at() + Duration::minutes(4),
        )
        .unwrap();
    contract
        .transition(
            ContractTransition::Terminate,
            "legal-admin",
            at() + Duration::minutes(5),
        )
        .unwrap();
    assert_eq!(
        contract.transition(
            ContractTransition::Archive,
            "legal-admin",
            at() + Duration::days(29),
        ),
        Err(LegalDomainError::RetentionActive)
    );
    contract
        .transition(
            ContractTransition::Archive,
            "legal-admin",
            at() + Duration::days(31),
        )
        .unwrap();
    assert_eq!(contract.status(), ContractStatus::Archived);
    assert_eq!(
        contract.transition(
            ContractTransition::Terminate,
            "legal-admin",
            at() + Duration::days(32),
        ),
        Err(LegalDomainError::InvalidTransition)
    );
}

#[test]
fn failed_contract_transition_does_not_partially_mutate_state() {
    let mut contract = contract();
    contract
        .transition(
            ContractTransition::SubmitForReview,
            "creator",
            at() + Duration::minutes(1),
        )
        .unwrap();
    let before = contract.clone();
    assert!(
        contract
            .approve(
                human_decision(
                    "creator",
                    scope(),
                    command_id(),
                    at() + Duration::minutes(2),
                ),
                command_id(),
                at() + Duration::minutes(2),
            )
            .is_err()
    );
    assert_eq!(contract, before);
}

#[test]
fn contract_approval_proof_rejects_non_humans_and_mismatched_ref_time_or_scope() {
    let mut contract = contract();
    contract
        .transition(
            ContractTransition::SubmitForReview,
            "creator",
            at() + Duration::minutes(1),
        )
        .unwrap();
    let before = contract.clone();
    let decided_at = at() + Duration::minutes(2);

    for kind in [
        ActorKind::Agent,
        ActorKind::Service,
        ActorKind::SystemWorker,
    ] {
        assert!(
            HumanDecisionProof::try_new(
                ActorAttribution {
                    principal_ref: "reviewer".into(),
                    kind,
                    session_id: Some(SessionId::try_from_uuid(Uuid::from_u128(91)).unwrap(),),
                    represented_principal_ref: None,
                    support_capability_id: None,
                },
                scope(),
                command_id().to_string(),
                decided_at,
            )
            .is_err()
        );
        assert_eq!(contract, before);
    }

    assert_eq!(
        contract.transition(ContractTransition::Approve, "reviewer", decided_at),
        Err(LegalDomainError::InvalidApprovalDecision)
    );
    assert_eq!(
        contract.approve(
            human_decision(
                "reviewer",
                scope(),
                CommandId::try_from_uuid(Uuid::from_u128(92)).unwrap(),
                decided_at,
            ),
            command_id(),
            decided_at,
        ),
        Err(LegalDomainError::InvalidApprovalDecision)
    );
    assert_eq!(contract, before);
    assert_eq!(
        contract.approve(
            human_decision("reviewer", scope(), command_id(), decided_at),
            command_id(),
            decided_at + Duration::seconds(1),
        ),
        Err(LegalDomainError::InvalidApprovalDecision)
    );
    assert_eq!(contract, before);
    assert_eq!(
        contract.approve(
            human_decision(
                "reviewer",
                AuthorityScope::Company {
                    holding_id: Uuid::from_u128(1),
                    company_id: Uuid::from_u128(3),
                },
                command_id(),
                decided_at,
            ),
            command_id(),
            decided_at,
        ),
        Err(LegalDomainError::InvalidApprovalDecision)
    );
    assert_eq!(contract, before);
}

#[test]
fn retention_horizon_is_bounded_from_contract_creation_time() {
    let result = LegalContract::create(
        LegalContractId::try_from_uuid(Uuid::from_u128(10)).unwrap(),
        scope(),
        "CTR-010".into(),
        "Supplier agreement".into(),
        "legal/contracts/ctr-010/v1".into(),
        format!("sha256:{}", "a".repeat(64)),
        at() + Duration::days(MAX_RETENTION_DAYS + 1),
        "creator".into(),
        at(),
    );
    assert_eq!(result, Err(LegalDomainError::InvalidRetention));
}

#[test]
fn archived_document_is_terminal_and_versions_remain_ordered_metadata() {
    let mut document = versioned_document();
    document.archive(at() + Duration::minutes(2)).unwrap();
    let before = document.clone();
    assert_eq!(
        document.add_version(
            "legal/documents/opinion/v2".into(),
            format!("sha256:{}", "c".repeat(64)),
            "author".into(),
            at() + Duration::minutes(3),
        ),
        Err(LegalDomainError::Terminal)
    );
    assert_eq!(document, before);
    assert_eq!(document.current_version(), 1);
    assert_eq!(document.versions()[0].version, 1);
}

#[test]
fn access_history_entry_is_append_only_actor_scope_and_version_evidence() {
    let entry = AccessEntry::record(
        AccessEntryId::try_from_uuid(Uuid::from_u128(30)).unwrap(),
        scope(),
        LegalDocumentId::try_from_uuid(Uuid::from_u128(20)).unwrap(),
        1,
        "reader".into(),
        "download".into(),
        "litigation review".into(),
        at(),
        at(),
    )
    .unwrap();
    assert_eq!(entry.version(), 1);
    assert_eq!(entry.actor_ref(), "reader");
    assert_eq!(entry.document_version(), 1);
    assert_eq!(entry.scope(), &scope());
    // AccessEntry deliberately exposes no mutation method: every observation is
    // a distinct aggregate and therefore an append-only audit record.
    assert_eq!(entry.action(), "download");
    assert_eq!(entry.purpose(), "litigation review");
}

#[test]
fn access_history_rejects_nonexistent_version_shape_and_future_clock_skew() {
    let document_id = LegalDocumentId::try_from_uuid(Uuid::from_u128(20)).unwrap();
    assert_eq!(
        AccessEntry::record(
            AccessEntryId::try_from_uuid(Uuid::from_u128(30)).unwrap(),
            scope(),
            document_id,
            0,
            "reader".into(),
            "download".into(),
            "review".into(),
            at(),
            at(),
        ),
        Err(LegalDomainError::InvalidDocumentVersion)
    );
    assert_eq!(
        AccessEntry::record(
            AccessEntryId::try_from_uuid(Uuid::from_u128(31)).unwrap(),
            scope(),
            document_id,
            1,
            "reader".into(),
            "download".into(),
            "review".into(),
            at() + Duration::minutes(MAX_ACCESS_CLOCK_SKEW_MINUTES + 1),
            at(),
        ),
        Err(LegalDomainError::AccessClockSkew)
    );
}

#[test]
fn legal_core_rejects_broad_scope_and_secret_bearing_metadata() {
    let broad = LegalDocument::create(
        LegalDocumentId::try_from_uuid(Uuid::from_u128(20)).unwrap(),
        AuthorityScope::Holding {
            holding_id: Uuid::from_u128(1),
        },
        "Opinion".into(),
        at(),
    );
    assert_eq!(broad, Err(LegalDomainError::InvalidScope));

    let mut document = LegalDocument::create(
        LegalDocumentId::try_from_uuid(Uuid::from_u128(20)).unwrap(),
        scope(),
        "Opinion".into(),
        at(),
    )
    .unwrap();
    assert!(matches!(
        document.add_version(
            "legal/token/credential".into(),
            format!("sha256:{}", "d".repeat(64)),
            "author".into(),
            at(),
        ),
        Err(LegalDomainError::SensitiveMaterial { .. })
    ));
}

#[test]
fn matter_graph_pins_version_digest_scope_and_rejects_cross_holding_edges() {
    let mut matter = LegalMatter::create(
        LegalMatterId::try_from_uuid(Uuid::from_u128(40)).unwrap(),
        scope(),
        "MAT-040".into(),
        "Customer dispute".into(),
        "legal-owner".into(),
        at(),
    )
    .unwrap();
    let target =
        RecordRef::try_new(BoundedContext::Sales, "sales_order", Uuid::from_u128(41), 7).unwrap();
    matter
        .link_record(
            RecordRelation::Disputes,
            target.clone(),
            scope(),
            format!("sha256:{}", "e".repeat(64)),
            "legal-owner".into(),
            at() + Duration::minutes(1),
        )
        .unwrap();
    assert_eq!(matter.links()[0].target.aggregate_version, 7);
    assert_eq!(matter.links()[0].target, target);
    assert_eq!(
        matter.link_record(
            RecordRelation::Disputes,
            RecordRef::try_new(BoundedContext::Sales, "sales_order", Uuid::from_u128(42), 1,)
                .unwrap(),
            AuthorityScope::Company {
                holding_id: Uuid::from_u128(99),
                company_id: Uuid::from_u128(2),
            },
            format!("sha256:{}", "f".repeat(64)),
            "legal-owner".into(),
            at() + Duration::minutes(2),
        ),
        Err(LegalDomainError::CrossHoldingLink)
    );
}

#[test]
fn legal_hold_requires_separate_release_and_blocks_matter_close() {
    let mut matter = LegalMatter::create(
        LegalMatterId::try_from_uuid(Uuid::from_u128(50)).unwrap(),
        scope(),
        "MAT-050".into(),
        "Regulatory review".into(),
        "legal-owner".into(),
        at(),
    )
    .unwrap();
    matter
        .place_hold(
            "Preserve all linked evidence".into(),
            "legal-owner".into(),
            at() + Duration::minutes(1),
        )
        .unwrap();
    let hold_ref = matter.legal_hold_record_ref().unwrap().unwrap();
    assert_eq!(hold_ref.record_type().as_str(), "legal.hold");
    assert_ne!(hold_ref.id().as_uuid(), matter.id().as_uuid());
    assert_eq!(hold_ref, legal_hold_record_ref_for(matter.id()).unwrap());
    assert_eq!(
        matter.close(at() + Duration::minutes(2)),
        Err(LegalDomainError::InvalidTransition)
    );
    assert_eq!(
        matter.release_hold("legal-owner".into(), at() + Duration::minutes(2)),
        Err(LegalDomainError::InvalidLegalHold)
    );
    matter
        .release_hold("legal-reviewer".into(), at() + Duration::minutes(2))
        .unwrap();
    matter.close(at() + Duration::minutes(3)).unwrap();
    assert_eq!(matter.status(), MatterStatus::Closed);
}

fn signature_recipient(value: u128, order: u16) -> SignatureRecipient {
    SignatureRecipient::try_new(
        SignatureRecipientId::try_from_uuid(Uuid::from_u128(value)).unwrap(),
        SignatureSubject::Internal {
            principal_ref: format!("signer-{value}"),
        },
        order,
    )
    .unwrap()
}

fn callback(event: &str, sequence: u64, kind: SignatureCallbackKind) -> VerifiedSignatureCallback {
    VerifiedSignatureCallback {
        provider_event_id: event.into(),
        provider_envelope_ref: "provider-envelope-60".into(),
        sequence,
        verified: true,
        occurred_at: at() + Duration::minutes(i64::try_from(sequence).unwrap() + 1),
        kind,
    }
}

#[test]
fn signature_envelope_pins_document_orders_signers_and_deduplicates_callbacks() {
    let first = signature_recipient(61, 1);
    let second = signature_recipient(62, 2);
    let mut envelope = SignatureEnvelope::create(
        SignatureEnvelopeId::try_from_uuid(Uuid::from_u128(60)).unwrap(),
        scope(),
        LegalDocumentId::try_from_uuid(Uuid::from_u128(20)).unwrap(),
        7,
        format!("sha256:{}", "a".repeat(64)),
        "docusign".into(),
        "provider-policy-2026-08".into(),
        SignatureOrder::Serial,
        vec![first.clone(), second.clone()],
        at() + Duration::days(365),
        "legal-owner".into(),
        at(),
    )
    .unwrap();
    envelope
        .submit("provider-envelope-60".into(), at() + Duration::minutes(1))
        .unwrap();

    let before = envelope.clone();
    let mut unverified = callback("event-unverified", 1, SignatureCallbackKind::Delivered);
    unverified.verified = false;
    assert_eq!(
        envelope.apply_callback(unverified, at() + Duration::minutes(2)),
        Err(LegalDomainError::UnverifiedSignatureCallback)
    );
    assert_eq!(envelope, before);

    assert_eq!(
        envelope.apply_callback(
            callback(
                "event-out-of-order-signer",
                1,
                SignatureCallbackKind::RecipientSigned {
                    recipient_id: second.id,
                },
            ),
            at() + Duration::minutes(2),
        ),
        Err(LegalDomainError::InvalidSignatureOrder)
    );

    let first_signed = callback(
        "event-first-signed",
        1,
        SignatureCallbackKind::RecipientSigned {
            recipient_id: first.id,
        },
    );
    assert_eq!(
        envelope
            .apply_callback(first_signed.clone(), at() + Duration::minutes(2))
            .unwrap(),
        CallbackOutcome::Applied
    );
    let version = envelope.version();
    assert_eq!(
        envelope
            .apply_callback(first_signed, at() + Duration::minutes(3))
            .unwrap(),
        CallbackOutcome::Duplicate
    );
    assert_eq!(envelope.version(), version);

    envelope
        .apply_callback(
            callback(
                "event-second-signed",
                2,
                SignatureCallbackKind::RecipientSigned {
                    recipient_id: second.id,
                },
            ),
            at() + Duration::minutes(3),
        )
        .unwrap();
    envelope
        .apply_callback(
            callback(
                "event-completed",
                3,
                SignatureCallbackKind::Completed {
                    evidence_sha256: format!("sha256:{}", "b".repeat(64)),
                },
            ),
            at() + Duration::minutes(4),
        )
        .unwrap();
    assert_eq!(envelope.status(), SignatureStatus::Completed);
    assert_eq!(envelope.document_version(), 7);
    assert!(envelope.evidence_sha256().is_some());
    assert_eq!(
        envelope.cancel("legal-owner".into(), at() + Duration::minutes(5)),
        Err(LegalDomainError::InvalidSignatureTransition)
    );
}

#[test]
fn external_signatory_is_one_time_realm_metadata_without_raw_secret() {
    let recipient = SignatureRecipient::try_new(
        SignatureRecipientId::try_from_uuid(Uuid::from_u128(70)).unwrap(),
        SignatureSubject::ExternalOneTime {
            subject_ref: "external-party-70".into(),
            access_grant_sha256: format!("sha256:{}", "c".repeat(64)),
            expires_at: at() + Duration::hours(1),
        },
        1,
    )
    .unwrap();
    assert!(matches!(
        recipient.subject,
        SignatureSubject::ExternalOneTime { .. }
    ));

    assert!(matches!(
        SignatureRecipient::try_new(
            SignatureRecipientId::try_from_uuid(Uuid::from_u128(71)).unwrap(),
            SignatureSubject::ExternalOneTime {
                subject_ref: "external-party-71".into(),
                access_grant_sha256: "raw-secret".into(),
                expires_at: at() + Duration::hours(1),
            },
            1,
        ),
        Err(LegalDomainError::SensitiveMaterial {
            field: "content_sha256"
        })
    ));
}

#[test]
fn retention_is_fail_closed_when_hold_evidence_is_unavailable() {
    let policy = RetentionPolicySnapshot {
        policy_version: "legal-retention-2026-08".into(),
        jurisdiction_version: "th-core-2026-08".into(),
        retention_until: at() + Duration::days(30),
    };
    assert_eq!(
        RetentionDecision::evaluate(&policy, &HoldEvidence::Unavailable, at()),
        Err(LegalDomainError::LegalHoldEvidenceUnavailable)
    );
    assert_eq!(
        RetentionDecision::evaluate(
            &policy,
            &HoldEvidence::Clear {
                checked_at: at(),
                evidence_sha256: format!("sha256:{}", "d".repeat(64)),
            },
            at() + Duration::days(31),
        )
        .unwrap(),
        RetentionDecision::EligibleForDisposal
    );
    assert_eq!(
        RetentionDecision::evaluate(
            &policy,
            &HoldEvidence::Active {
                matter_id: LegalMatterId::try_from_uuid(Uuid::from_u128(72)).unwrap(),
                matter_version: 4,
                evidence_sha256: format!("sha256:{}", "e".repeat(64)),
            },
            at() + Duration::days(31),
        )
        .unwrap(),
        RetentionDecision::RetainForLegalHold
    );
}
