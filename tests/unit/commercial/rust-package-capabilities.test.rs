use erpx_server_commercial::public::purchases::*;
use serde::Deserialize;

#[test]
fn lifecycle_review_matches_original_node_and_rejects_changed_authority_or_request() {
    use erpx_server_commercial::public::package_review::*;
    let fixture:serde_json::Value=serde_json::from_str(include_str!("../../../fixtures/commercial/rust-packages/lifecycle-review.v1.json")).unwrap();
    assert_eq!(fixture["node"],"v26.11.1");
    let catalogue=current_package_catalogue().unwrap();
    let actor:PackageReviewActor=serde_json::from_value(fixture["actor"].clone()).unwrap();
    let input:GrantBatch=serde_json::from_value(fixture["input"].clone()).unwrap();
    let signer=PackageLifecycleReviewSigner::new(fixture["secret"].as_str().unwrap()).unwrap();
    let authority="b".repeat(64);
    let review=signer.issue(&catalogue,actor.clone(),"h001",&input,&authority,1000).unwrap();
    assert_eq!(review.token,fixture["review"]["token"].as_str().unwrap());
    assert_eq!(review.expires_at,301000);
    let claims=signer.read(&review.token,&catalogue,&actor,"h001",&input,1000).unwrap();
    assert_eq!(serde_json::to_value(&claims).unwrap(),fixture["claims"]);
    for changed in [PackageReviewActor{user_id:"other".into(),..actor.clone()},PackageReviewActor{session_id:"other".into(),..actor.clone()},PackageReviewActor{assignment_id:"other".into(),..actor.clone()},PackageReviewActor{role:"other".into(),..actor.clone()},PackageReviewActor{role_revision:4,..actor.clone()}]{
        assert_eq!(signer.read(&review.token,&catalogue,&changed,"h001",&input,1000).err().unwrap().code,"PACKAGE_REVIEW_INVALID");
    }
    let mut changed=input.clone();changed.idempotency_key="another-reviewed-key".into();
    assert!(signer.read(&review.token,&catalogue,&actor,"h001",&changed,1000).is_err());
    changed=input.clone();changed.changes[0].grant.reason.push('!');
    assert!(signer.read(&review.token,&catalogue,&actor,"h001",&changed,1000).is_err());
    assert!(signer.read(&review.token,&catalogue,&actor,"h002",&input,1000).is_err());
    assert!(signer.read(&review.token,&catalogue,&actor,"h001",&input,999).is_err());
    let mut tampered=review.token.clone().into_bytes();tampered[0]=if tampered[0]==b'e'{b'f'}else{b'e'};
    assert!(signer.read(&String::from_utf8(tampered).unwrap(),&catalogue,&actor,"h001",&input,1000).is_err());
    let other=PackageLifecycleReviewSigner::new("different-fixture-signing-secret-minimum32").unwrap();
    assert!(other.read(&review.token,&catalogue,&actor,"h001",&input,1000).is_err());
    signer.assert_fresh(&claims,300999).unwrap();
    let recovered=signer.read(&review.token,&catalogue,&actor,"h001",&input,301000).unwrap();
    assert_eq!(signer.assert_fresh(&recovered,301000).unwrap_err().code,"PACKAGE_REVIEW_EXPIRED");
    assert!(PackageLifecycleReviewSigner::new("short").is_err());
}

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
#[test]
fn wire_revision_matches_node_jsonb_number_and_locale_corpus() {
    use erpx_server_commercial::public::purchases::{purchase_wire_revision, CompanyPackageGrant, CompanyPackageHistoryRight};
    use erpx_server_shared::public::TimestampMillis;
    let grant = |package_id: &str, version, state: &str, starts_at, ends_at| CompanyPackageGrant {
        company_id: "company".into(), package_id: package_id.into(), package_version: 1,
        state: state.into(), starts_at, ends_at, version, reason: "fixture".into(), evidence: "fixture".into(),
    };
    let grants = vec![grant("z", 2, "active", 9_007_199_254_740_993, Some(9_007_199_254_740_995)), grant("é", 1, "active", 10, None), grant("e\u{301}", 1, "active", 10, None), grant("a\"x", 1, "draining", 0, Some(5))];
    let history = vec![
        CompanyPackageHistoryRight { company_id: "company".into(), package_id: "é".into(), package_version: 1, source_grant_version: 1, source_event_id: "ev😀".into(), retained_at: 3 },
        CompanyPackageHistoryRight { company_id: "company".into(), package_id: "e\u{301}".into(), package_version: 1, source_grant_version: 1, source_event_id: "ev\\n".into(), retained_at: 6 },
    ];
    // Pinned Node 26.11.1 JSON.stringify / localeCompare / SHA-256 oracle.
    assert_eq!(purchase_wire_revision("company", &grants, &history, TimestampMillis::new(5)).unwrap(), "0eb960248dd90395f7ea80e0620123cd387d81f4fdc061197f6819d8e0b7442c");
    assert_eq!(purchase_wire_revision("company", &[], &[], TimestampMillis::new(5)).unwrap(), "39ed07fbbd040d61afcab922e85fc1a9eba49b14b70593a1e4727c96cb63f8e1");
    // Node's stable sort preserves database input order for locale-equal
    // byte-distinct tuples. An ordinal tie-break would change these hashes.
    let composed_first = vec![grant("é", 1, "active", 0, None), grant("e\u{301}", 1, "active", 0, None)];
    let decomposed_first = vec![composed_first[1].clone(), composed_first[0].clone()];
    assert_eq!(purchase_wire_revision("company", &composed_first, &[], TimestampMillis::new(5)).unwrap(), "2aa48f7e3d2d6b8b87c372f47b2ab3b95f5b5a1b7e104dec5cd5550ae13d38ff");
    assert_eq!(purchase_wire_revision("company", &decomposed_first, &[], TimestampMillis::new(5)).unwrap(), "8253db6b44f3f8db1835814d228a8570ec3972a19e3af5376451eac6ced5ca96");
    let mut other = grants.clone();
    other[0].company_id = "foreign".into();
    assert_eq!(purchase_wire_revision("company", &other, &history, TimestampMillis::new(5)).unwrap(), purchase_wire_revision("company", &grants[1..], &history, TimestampMillis::new(5)).unwrap());
}

#[test]
fn assets_package_mode_matches_actual_typescript_switch() {
    use erpx_server_commercial::public::asset_package_mode::{ResolvedPackageEnvironment,resolve_assets_package_mode};
    use erpx_server_shared::public::{BusinessError,FailureKind};
    use std::cell::Cell;
    // Mock only the resolved runtime environment; execute the actual TS
    // switch. The runtime's own environment derivation needs separate tests.
    let script=r#"import {mock} from 'bun:test';let environment='local',calls=0;mock.module(process.argv[1],()=>({runtimeEnv:async()=>{calls++;return {ERPX_ENVIRONMENT:environment}}}));const m=await import(process.argv[2]);const cases=[[null,'production'],['disabled','production'],['shadow','local'],['enforced','local'],['shadow','production'],['enforced','staging'],['','local'],['SHADOW','local'],[' enforced','local']];const out=[];for(const [raw,env] of cases){environment=env;calls=0;if(raw===null)delete process.env.ERPX_ASSETS_PACKAGE_ENFORCEMENT;else process.env.ERPX_ASSETS_PACKAGE_ENFORCEMENT=raw;try{out.push({mode:await m.assetsPackageEnforcementMode(),calls})}catch(e){out.push({status:e.status,code:e.code,message:e.message,calls})}}process.stdout.write(JSON.stringify(out));"#;
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let runtime=root.join("packages/server-runtime/src/public/runtime.ts");
    let switch=root.join("packages/server-commercial/src/adapters/development-package-enforcement.ts");
    let output=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(root).args(["--conditions=react-server","-e",script]).arg(runtime).arg(switch).output().unwrap();assert!(output.status.success(),"actual TypeScript mode oracle unavailable");
    let expected:Vec<serde_json::Value>=serde_json::from_slice(&output.stdout).unwrap();
    let cases=[(None,"production"),(Some("disabled"),"production"),(Some("shadow"),"local"),(Some("enforced"),"local"),(Some("shadow"),"production"),(Some("enforced"),"staging"),(Some(""),"local"),(Some("SHADOW"),"local"),(Some(" enforced"),"local")];
    for (index,(raw,environment)) in cases.into_iter().enumerate(){
        let calls=Cell::new(0);let result=resolve_assets_package_mode(raw,||{calls.set(calls.get()+1);Ok(ResolvedPackageEnvironment::from_trusted_runtime(environment))});
        let actual=match result {Ok(mode)=>serde_json::json!({"mode":match mode{PackageEnforcementMode::Disabled=>"disabled",PackageEnforcementMode::Shadow=>"shadow",PackageEnforcementMode::Enforced=>"enforced"},"calls":calls.get()}),Err(e)=>serde_json::json!({"status":503,"code":e.code,"message":e.message,"calls":calls.get()})};
        assert_eq!(actual,expected[index]);
    }
    let result=resolve_assets_package_mode(Some("shadow"),||Err(BusinessError::new(FailureKind::Unavailable,"TRUSTED_RUNTIME_UNAVAILABLE","Unavailable")));
    assert_eq!(result.unwrap_err().code,"TRUSTED_RUNTIME_UNAVAILABLE");
}
