use chrono::{Duration, TimeZone, Utc};
use erpx_kernel::TenantScope;
use uuid::Uuid;

use super::*;

fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}

fn scope() -> TenantScope {
    TenantScope::Company {
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
        contract.transition(
            ContractTransition::Approve,
            "creator",
            at() + Duration::minutes(3),
        ),
        Err(LegalDomainError::ApprovalSeparation)
    );
    contract
        .transition(
            ContractTransition::Approve,
            "reviewer",
            at() + Duration::minutes(3),
        )
        .unwrap();
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
            .transition(
                ContractTransition::Approve,
                "creator",
                at() + Duration::minutes(2),
            )
            .is_err()
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
        TenantScope::Holding {
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
