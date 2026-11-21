#[path = "../../support/legal/trust-facts.rs"]
mod facts;

use erpx_server_legal::public::{
    LegalTrustCommand, LegalTrustInspector, LegalTrustReceipt, PreparedLegalTrustCommand,
    RustTrustMaterialInspector, TrustMaterialKind, TrustMaterialState, prepare_legal_trust_command,
};
use erpx_server_shared::public::{CommandKey, CompanyId, RecordId, TimestampMillis, UserId};
use facts::{FixedClock, SyntheticInspector, TrustFacts, actor, material};

fn key() -> CommandKey {
    CommandKey::new("legal-trust-command-key").unwrap()
}
fn approval() -> LegalTrustCommand {
    LegalTrustCommand::Approve {
        id: material().id,
        version: 1,
    }
}

#[tokio::test]
async fn current_permission_is_required_even_for_an_existing_replay() {
    let mut ports = TrustFacts::authorized_company();
    ports.allow_manage = false;
    ports.replay = Some(LegalTrustReceipt {
        id: actor().company_id,
        version: 1,
    });
    let result = prepare_legal_trust_command(
        &mut ports,
        &SyntheticInspector,
        &FixedClock,
        &actor(),
        &key(),
        &approval(),
    )
    .await;
    assert_eq!(result.err().unwrap().code, "LEGAL_PERMISSION_REQUIRED");
    assert_eq!(ports.calls, ["authorize"]);
    ports.allow_manage = true;
    assert!(matches!(
        prepare_legal_trust_command(
            &mut ports,
            &SyntheticInspector,
            &FixedClock,
            &actor(),
            &key(),
            &approval()
        )
        .await
        .unwrap(),
        PreparedLegalTrustCommand::Replay(_)
    ));
    assert!(!ports.calls.contains(&"lock"));
}

#[tokio::test]
async fn approve_requires_a_different_person_and_the_current_proposed_version() {
    for scenario in [
        "same-person",
        "same-user",
        "approved",
        "stale",
        "foreign-company",
    ] {
        let mut ports = TrustFacts::authorized_company();
        match scenario {
            "same-person" => ports.same_person = true,
            "same-user" => ports.source.as_mut().unwrap().created_by = actor().user_id,
            "approved" => ports.source.as_mut().unwrap().state = TrustMaterialState::Approved,
            "stale" => ports.source.as_mut().unwrap().version = 2,
            _ => {
                ports.source.as_mut().unwrap().company_id = CompanyId::new("other-company").unwrap()
            }
        }
        let result = prepare_legal_trust_command(
            &mut ports,
            &SyntheticInspector,
            &FixedClock,
            &actor(),
            &key(),
            &approval(),
        )
        .await;
        assert!(result.is_err(), "{scenario}");
        assert!(ports.calls.contains(&"lock"));
    }
    let mut ports = TrustFacts::authorized_company();
    let result = prepare_legal_trust_command(
        &mut ports,
        &SyntheticInspector,
        &FixedClock,
        &actor(),
        &key(),
        &approval(),
    )
    .await
    .unwrap();
    let PreparedLegalTrustCommand::Change(plan) = result else {
        panic!("expected prepared change");
    };
    assert_eq!(plan.actor().user_id, UserId::new("reviewer").unwrap());
    assert_eq!(plan.authority_witness(), "current-authority-1");
    assert_eq!(plan.receipt().id, actor().company_id);
    assert_eq!(plan.command_key(), &key());
}

#[tokio::test]
async fn company_scope_limits_and_sources_are_checked_before_preparing_writes() {
    let mut scoped = actor();
    scoped.branch_id = Some(RecordId::new("branch").unwrap());
    let mut ports = TrustFacts::authorized_company();
    assert!(
        prepare_legal_trust_command(
            &mut ports,
            &SyntheticInspector,
            &FixedClock,
            &scoped,
            &key(),
            &facts::addition()
        )
        .await
        .is_err()
    );
    assert!(!ports.calls.contains(&"lock"));
    let mut ports = TrustFacts::authorized_company();
    ports.active_count = 100;
    assert_eq!(
        prepare_legal_trust_command(
            &mut ports,
            &SyntheticInspector,
            &FixedClock,
            &actor(),
            &key(),
            &facts::addition()
        )
        .await
        .err()
        .unwrap()
        .code,
        "LEGAL_TRUST_LIMIT"
    );
    for source in ["http://unreachable.invalid/source", "not a URL"] {
        let mut input = facts::addition();
        if let LegalTrustCommand::Add { source: value, .. } = &mut input {
            *value = source.to_owned();
        }
        assert!(
            prepare_legal_trust_command(
                &mut TrustFacts::authorized_company(),
                &SyntheticInspector,
                &FixedClock,
                &actor(),
                &key(),
                &input
            )
            .await
            .is_err()
        );
    }
}

#[test]
fn actual_rust_inspector_accepts_ca_metadata_and_refuses_expired_or_malformed_material() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/pdf-signatures/conformance.v1.json"
    ))
    .unwrap();
    let case = &corpus["cases"][0];
    let root = case["trust"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["path"] == "roots/root.pem")
        .unwrap();
    let pem = String::from_utf8(hex::decode(root["hex"].as_str().unwrap()).unwrap()).unwrap();
    let inspector = RustTrustMaterialInspector;
    let result = inspector
        .inspect(
            TrustMaterialKind::Root,
            &pem,
            TimestampMillis::new(1_793_080_800_000),
        )
        .unwrap();
    assert!(result.content.starts_with(&[0x30]));
    use sha2::Digest;
    assert_eq!(
        result.checksum_sha256,
        hex::encode(sha2::Sha256::digest(&result.content))
    );
    assert!(
        inspector
            .inspect(
                TrustMaterialKind::Root,
                &pem,
                TimestampMillis::new(2_500_000_000_000)
            )
            .is_err()
    );
    assert!(
        inspector
            .inspect(
                TrustMaterialKind::Root,
                "not a certificate",
                TimestampMillis::new(0)
            )
            .is_err()
    );
    assert!(
        inspector
            .inspect(
                TrustMaterialKind::Crl,
                "base64 contains spaces",
                TimestampMillis::new(0)
            )
            .is_err()
    );
}
