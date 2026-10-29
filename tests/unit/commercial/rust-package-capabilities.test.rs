use erpx_server_commercial::public::purchases::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct Scenario {
    grants: Vec<CompanyPackageGrant>,
    history: Vec<CompanyPackageHistoryRight>,
}
#[derive(Deserialize)]
struct Decision {
    scenario: usize,
    capability: String,
    purpose: CapabilityPurpose,
    expected: PackageDecision,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Change {
    current: Option<CompanyPackageGrant>,
    next: CompanyPackageGrant,
    expected_version: i64,
    outstanding: i64,
    expected_code: Option<String>,
    requires_review: bool,
}
#[derive(Deserialize)]
struct Corpus {
    node: String,
    packages: Vec<PackageDefinition>,
    scenarios: Vec<Scenario>,
    decisions: Vec<Decision>,
    changes: Vec<Change>,
}
fn corpus() -> Corpus {
    serde_json::from_str(include_str!(
        "../../../fixtures/commercial/rust-packages/conformance.v1.json"
    ))
    .unwrap()
}

#[test]
fn current_catalogue_and_capability_decisions_match_independent_node_baseline() {
    let c = corpus();
    let catalogue = current_package_catalogue().unwrap();
    assert_eq!(c.node, "v26.11.1");
    assert_eq!(catalogue.definitions(), c.packages);
    assert!(c.decisions.len() > 5000);
    for case in c.decisions {
        let facts = &c.scenarios[case.scenario];
        assert_eq!(
            catalogue.purchased_capability(
                &facts.grants,
                "company",
                &case.capability,
                case.purpose,
                5000,
                &facts.history
            ),
            case.expected,
            "scenario {}: {} {:?}",
            case.scenario,
            case.capability,
            case.purpose
        );
    }
}
#[test]
fn grant_changes_preserve_drain_first_version_evidence_and_obligation_rules() {
    let catalogue = current_package_catalogue().unwrap();
    for case in corpus().changes {
        assert_eq!(
            catalogue.requires_settlement_review(case.current.as_ref(), &case.next),
            case.requires_review
        );
        let code = catalogue
            .validate_grant_change(
                case.current.as_ref(),
                &case.next,
                case.expected_version,
                case.outstanding,
            )
            .err()
            .map(|e| e.code.to_owned());
        assert_eq!(code, case.expected_code, "{:?}", case.next);
    }
}
#[test]
fn enforcement_modes_are_explicit_and_never_create_personal_authority() {
    let denied = PackageDecision {
        allowed: false,
        code: PackageDecisionCode::PackageRequired,
    };
    assert!(PackageEnforcementMode::Disabled.permits(denied));
    assert!(PackageEnforcementMode::Shadow.permits(denied));
    assert!(!PackageEnforcementMode::Enforced.permits(denied));
    trait CheckedBoundary<Marker> {
        fn owner_catalogue_only() {}
    }
    impl<T: ?Sized> CheckedBoundary<()> for T {}
    impl<T: serde::de::DeserializeOwned> CheckedBoundary<u8> for T {}
    let _ = <CommercialCatalogue as CheckedBoundary<_>>::owner_catalogue_only;
    let _ = <PurchaseWitness as CheckedBoundary<_>>::owner_catalogue_only;
}

#[test]
fn dependency_policy_and_actual_legacy_application_receipt_hashes_match_node() {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct HashCase {
        input: GrantChange,
        expected_hash: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct DependencyCase {
        scenario: usize,
        grant: CompanyPackageGrant,
        expected_code: Option<String>,
    }
    #[derive(Deserialize)]
    struct CommandCorpus {
        node: String,
        scenarios: Vec<Scenario>,
        hashes: Vec<HashCase>,
        dependencies: Vec<DependencyCase>,
    }
    let c: CommandCorpus = serde_json::from_str(include_str!(
        "../../../fixtures/commercial/rust-packages/grant-commands.v1.json"
    ))
    .unwrap();
    assert_eq!(c.node, "v26.11.1");
    assert_eq!(c.hashes.len(), 21);
    assert_eq!(c.dependencies.len(), 243);
    for case in c.hashes {
        assert_eq!(
            grant_change_digest(&case.input).unwrap(),
            case.expected_hash
        );
    }
    let catalogue = current_package_catalogue().unwrap();
    for case in c.dependencies {
        let actual = catalogue
            .validate_dependencies(&case.grant, &c.scenarios[case.scenario].grants, 5000)
            .err()
            .map(|e| e.code.to_owned());
        assert_eq!(
            actual, case.expected_code,
            "{} {:?}",
            case.scenario, case.grant
        );
    }
}

#[test]
fn batches_match_retained_node_normalization_request_hashes_and_company_revisions() {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct HashCase {
        input: GrantBatch,
        expected_hash: Option<String>,
        normalized: Option<NormalizedGrantBatch>,
        expected_code: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RevisionCase {
        grants: Vec<CompanyPackageGrant>,
        expected_revision: String,
    }
    #[derive(Deserialize)]
    struct BatchCorpus {
        node: String,
        hashes: Vec<HashCase>,
        revisions: Vec<RevisionCase>,
    }
    let corpus: BatchCorpus = serde_json::from_str(include_str!(
        "../../../fixtures/commercial/rust-packages/grant-batches.v1.json"
    ))
    .unwrap();
    assert_eq!(corpus.node, "v26.11.1");
    assert_eq!(corpus.hashes.len(), 43);
    assert_eq!(corpus.revisions.len(), 27);
    let catalogue = current_package_catalogue().unwrap();
    for row in corpus.hashes {
        match normalize_grant_batch(&catalogue, &row.input) {
            Ok(normalized) => {
                assert!(row.expected_code.is_none());
                assert_eq!(Some(&normalized), row.normalized.as_ref());
                assert_eq!(
                    Some(grant_batch_digest(&normalized).unwrap()),
                    row.expected_hash
                );
            }
            Err(error) => assert_eq!(Some(error.code.to_string()), row.expected_code),
        }
    }
    for row in corpus.revisions {
        assert_eq!(
            package_grant_revision("company", &row.grants).unwrap(),
            row.expected_revision
        );
    }
}

#[test]
fn unknown_unicode_package_ids_keep_legacy_collation_hashes_and_stable_map_previews() {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Comparison {
        left: String,
        right: String,
        expected_ordering: i8,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct HashCase {
        input: GrantBatch,
        expected_hash: Option<String>,
        normalized: Option<NormalizedGrantBatch>,
        expected_code: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Revision {
        grants: Vec<CompanyPackageGrant>,
        expected_revision: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Prospective {
        input: GrantBatch,
        before: Vec<CompanyPackageGrant>,
        expected_preview: GrantBatchPreview,
    }
    #[derive(Deserialize)]
    struct UnicodeCorpus {
        node: String,
        runtime: serde_json::Value,
        comparisons: Vec<Comparison>,
        hashes: Vec<HashCase>,
        revisions: Vec<Revision>,
        prospectives: Vec<Prospective>,
    }
    let corpus: UnicodeCorpus = serde_json::from_str(include_str!(
        "../../../fixtures/commercial/rust-packages/grant-batches-unicode.v1.json"
    ))
    .unwrap();
    assert_eq!(corpus.node, "v26.11.1");
    assert_eq!(corpus.runtime["cldr"], "48.0");
    assert_eq!(corpus.comparisons.len(), 841);
    assert_eq!(corpus.hashes.len(), 9);
    assert_eq!(corpus.revisions.len(), 10);
    assert_eq!(corpus.prospectives.len(), 18);
    let catalogue = current_package_catalogue().unwrap();
    let template = corpus.hashes[0].input.changes[0].grant.clone();
    for row in corpus.comparisons {
        let mut left = template.clone();
        left.package_id = row.left.clone();
        let mut right = template.clone();
        right.package_id = row.right.clone();
        let input = GrantBatch {
            company_id: "company".into(),
            expected_revision: "a".repeat(64),
            idempotency_key: "synthetic-comparison-key".into(),
            changes: vec![
                BatchChange {
                    grant: left,
                    expected_version: 0,
                },
                BatchChange {
                    grant: right,
                    expected_version: 0,
                },
            ],
        };
        if row.left == row.right {
            assert_eq!(
                normalize_grant_batch(&catalogue, &input).unwrap_err().code,
                "INVALID_PACKAGE_BATCH"
            );
            continue;
        }
        let left_addon = catalogue
            .definition(&row.left, template.package_version)
            .is_some_and(|p| p.kind == "addon");
        let right_addon = catalogue
            .definition(&row.right, template.package_version)
            .is_some_and(|p| p.kind == "addon");
        let ordering = left_addon
            .cmp(&right_addon)
            .then_with(|| row.expected_ordering.cmp(&0));
        let expected = if ordering.is_gt() {
            vec![row.right, row.left]
        } else {
            vec![row.left, row.right]
        };
        assert_eq!(
            normalize_grant_batch(&catalogue, &input)
                .unwrap()
                .changes
                .into_iter()
                .map(|c| c.grant.package_id)
                .collect::<Vec<_>>(),
            expected
        );
    }
    for row in corpus.hashes {
        match normalize_grant_batch(&catalogue, &row.input) {
            Ok(normalized) => {
                assert!(row.expected_code.is_none());
                assert_eq!(Some(&normalized), row.normalized.as_ref());
                assert_eq!(
                    Some(grant_batch_digest(&normalized).unwrap()),
                    row.expected_hash
                );
            }
            Err(error) => assert_eq!(Some(error.code.to_string()), row.expected_code),
        }
    }
    for row in corpus.revisions {
        assert_eq!(
            package_grant_revision("company", &row.grants).unwrap(),
            row.expected_revision
        );
    }
    for row in corpus.prospectives {
        let normalized = normalize_grant_batch(&catalogue, &row.input).unwrap();
        assert_eq!(normalized.changes, row.expected_preview.changes);
        assert_eq!(
            package_grant_revision("company", &row.before).unwrap(),
            row.input.expected_revision
        );
        assert_eq!(
            prospective_package_grants(&row.before, &normalized).unwrap(),
            row.expected_preview.after
        );
    }
}
