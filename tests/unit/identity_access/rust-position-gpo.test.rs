//! Portable Position GPO policy: input bounds, reviewed preview tokens, delivery
//! identity and Directory evidence. These are not HTTP or provider journeys.
use erpx_server_identity_access::public::position_gpo::{
    inventory_filters, inventory_links, inventory_object, membership_key, rule_source_id,
    sha256_hex, waiting_key, ConfigRetryInput, FreshMfa, GpoAction, Inventory, InventoryObject,
    JobRetryInput, JobRow, PositionGpoInput, PublishInput, UnmapInput, UnmapPublishInput,
};
use serde_json::json;

fn input() -> serde_json::Value {
    json!({
        "positionId": "position-1",
        "expectedVersion": 0,
        "connectionId": "connection-1",
        "gpoId": "gpo-1",
        "userFilterGroupId": "group-1",
        "computerTargetOuId": "ou-1",
        "reason": "  Publish the reviewed mapping  ",
    })
}

fn preview_token() -> String {
    "a".repeat(64)
}

#[test]
fn mapping_input_normalizes_the_reason_and_rejects_bounds() {
    let parsed = PositionGpoInput::parse(&input()).unwrap();
    assert_eq!(parsed.reason, "Publish the reviewed mapping");
    assert_eq!(parsed.expected_version, 0);
    assert_eq!(parsed.to_json()["positionId"], "position-1");

    let mut short = input();
    short["reason"] = json!("  x ");
    assert_eq!(
        PositionGpoInput::parse(&short).unwrap_err().code,
        "GPO_REQUEST_INVALID"
    );
    let mut long_id = input();
    long_id["positionId"] = json!("p".repeat(181));
    assert_eq!(
        PositionGpoInput::parse(&long_id).unwrap_err().code,
        "GPO_REQUEST_INVALID"
    );
    let mut negative = input();
    negative["expectedVersion"] = json!(-1);
    assert_eq!(
        PositionGpoInput::parse(&negative).unwrap_err().code,
        "GPO_REQUEST_INVALID"
    );
    let mut missing = input();
    missing.as_object_mut().unwrap().remove("gpoId");
    assert_eq!(
        PositionGpoInput::parse(&missing).unwrap_err().code,
        "GPO_REQUEST_INVALID"
    );
}

#[test]
fn publish_requires_a_reviewed_64_hex_token() {
    let mut raw = input();
    raw["previewToken"] = json!(preview_token());
    let parsed = PublishInput::parse(&raw).unwrap();
    assert_eq!(parsed.preview_token, preview_token());
    assert_eq!(
        parsed.request_json()["previewToken"],
        json!(preview_token())
    );

    for token in [
        "A".repeat(64),
        "a".repeat(63),
        "g".repeat(64),
        String::new(),
    ] {
        let mut raw = input();
        raw["previewToken"] = json!(token);
        assert_eq!(
            PublishInput::parse(&raw).unwrap_err().code,
            "GPO_REQUEST_INVALID"
        );
    }
}

#[test]
fn unmap_retry_and_config_retry_require_positive_versions() {
    let unmap = UnmapInput::parse(&json!({
        "positionId": "position-1",
        "expectedVersion": 1,
        "reason": "Remove the mapping",
    }))
    .unwrap();
    assert_eq!(unmap.expected_version, 1);
    let mut zero = json!({
        "positionId": "position-1",
        "expectedVersion": 0,
        "reason": "Remove the mapping",
    });
    assert_eq!(
        UnmapInput::parse(&zero).unwrap_err().code,
        "GPO_REQUEST_INVALID"
    );
    zero["expectedVersion"] = json!(1);
    zero["previewToken"] = json!(preview_token());
    assert!(UnmapPublishInput::parse(&zero).is_ok());

    assert_eq!(
        JobRetryInput::parse(&json!({
            "id": "job-1",
            "expectedVersion": 0,
            "reason": "Retry delivery",
        }))
        .unwrap_err()
        .code,
        "GPO_REQUEST_INVALID"
    );
    assert_eq!(
        ConfigRetryInput::parse(&json!({
            "positionId": "position-1",
            "expectedVersion": -2,
            "reason": "Retry config",
        }))
        .unwrap_err()
        .code,
        "GPO_REQUEST_INVALID"
    );
}

#[test]
fn reviewed_preview_token_is_stable_and_binds_directory_evidence() {
    let input = input();
    let result = json!({"version": 1, "revision": 1});
    let evidence = json!({"gpo": {"id": "gpo-1", "version": "7"}, "observedAt": 500});
    let first = erpx_server_identity_access::public::position_gpo::preview_token(
        &input, &result, &evidence,
    );
    let mut rerun = erpx_server_identity_access::public::position_gpo::preview_token(
        &input, &result, &evidence,
    );
    assert_eq!(first, rerun);
    assert_eq!(first.len(), 64);
    assert!(first
        .bytes()
        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
    rerun = erpx_server_identity_access::public::position_gpo::preview_token(
        &input,
        &result,
        &json!({"gpo": {"id": "gpo-1", "version": "8"}, "observedAt": 500}),
    );
    assert_ne!(first, rerun);
}

#[test]
fn authority_requires_it_holding_scope_and_fresh_mfa() {
    use erpx_server_identity_access::public::position_gpo::GpoAuthority;
    let it = GpoAuthority { it_holding: true };
    it.require_write(FreshMfa(true)).unwrap();
    assert_eq!(
        it.require_write(FreshMfa(false)).unwrap_err().code,
        "FRESH_MFA_REQUIRED"
    );
    assert_eq!(
        GpoAuthority { it_holding: false }
            .require_write(FreshMfa(true))
            .unwrap_err()
            .code,
        "IT_AUTHORITY_REQUIRED"
    );
}

#[test]
fn delivery_source_ids_distinguish_the_legacy_and_rule_paths() {
    assert_eq!(
        rule_source_id("legacy-position:position-1", "engagement-1", true),
        "position-gpo:engagement-1"
    );
    assert_eq!(
        rule_source_id("rule-1", "engagement-1", false),
        "gpo-rule:[\"rule-1\",\"engagement-1\"]"
    );
    assert_eq!(
        membership_key("connection-1", "group-1", "account-1", "position-gpo:e"),
        "[\"connection-1\",\"group-1\",\"account-1\",\"position-gpo:e\"]"
    );
    assert_eq!(
        waiting_key("company", "engagement-1", "position-1", "rule-1", 2),
        "[\"company\",\"engagement-1\",\"position-1\",\"rule-1\",2]"
    );
}

#[test]
fn delivery_id_is_stable_for_the_same_effect_and_differs_by_revision() {
    use erpx_server_identity_access::public::position_gpo::delivery_id;
    let parts = vec![
        "add".to_owned(),
        "connection-1".to_owned(),
        "group-1".to_owned(),
        "account-1".to_owned(),
        "position-gpo:e1".to_owned(),
        "rule-1".to_owned(),
        "3".to_owned(),
        "2".to_owned(),
        "uuid-1".to_owned(),
    ];
    let first = delivery_id(&parts);
    assert_eq!(first, delivery_id(&parts));
    assert!(first.starts_with("gpo:v3:"));
    let mut changed = parts.clone();
    changed[7] = "3".into();
    assert_ne!(first, delivery_id(&changed));
    assert_eq!(sha256_hex(&json!({"a": 1})).len(), 64);
}

#[test]
fn directory_evidence_reads_filters_links_and_members() {
    let inventory = Inventory {
        observed_at: 900,
        objects: vec![
            InventoryObject {
                id: "gpo-1".into(),
                kind: "gpo".into(),
                version: "7".into(),
                properties: json!({"securityFilterGroupIds": ["group-1", "group-2"]}),
                member_ids: vec![],
            },
            InventoryObject {
                id: "ou-1".into(),
                kind: "ou".into(),
                version: "3".into(),
                properties: json!({"gpoLinks": [{"gpoId": "gpo-1", "enabled": true}]}),
                member_ids: vec![],
            },
            InventoryObject {
                id: "group-1".into(),
                kind: "group".into(),
                version: "1".into(),
                properties: json!({}),
                member_ids: vec!["account-1".into()],
            },
        ],
    };
    assert_eq!(
        inventory_filters(&inventory, "gpo-1"),
        vec!["group-1".to_owned(), "group-2".to_owned()]
    );
    assert_eq!(
        inventory_links(&inventory, "ou-1"),
        vec![("gpo-1".to_owned(), true)]
    );
    assert_eq!(
        inventory_object(&inventory, "group-1", "group")
            .unwrap()
            .member_ids,
        vec!["account-1".to_owned()]
    );
    assert!(inventory_object(&inventory, "group-1", "gpo").is_none());
    assert!(inventory_filters(&inventory, "absent").is_empty());

    let _ = GpoAction::Members {
        object_id: "group-1".into(),
        member_ids: vec!["account-1".into()],
        add: true,
    };
}

#[test]
fn a_job_is_deliverable_only_with_account_engagement_rule_and_source() {
    let mut job = JobRow {
        id: "job-1".into(),
        company_id: "company".into(),
        position_id: "position-1".into(),
        person_id: "person-1".into(),
        engagement_id: Some("engagement-1".into()),
        account_id: Some("account-1".into()),
        revision: 2,
        engagement_version: 3,
        connection_id: "connection-1".into(),
        gpo_id: "gpo-1".into(),
        filter_group_id: "group-1".into(),
        computer_target_ou_id: "ou-1".into(),
        desired_action: "add".into(),
        status: "queued".into(),
        version: 1,
        rule_id: Some("rule-1".into()),
        source_id: Some("gpo-rule:[\"rule-1\",\"engagement-1\"]".into()),
    };
    assert!(job.deliverable());
    job.account_id = None;
    assert!(!job.deliverable());
    job.account_id = Some("account-1".into());
    job.rule_id = Some(String::new());
    assert!(!job.deliverable());
    job.rule_id = Some("rule-1".into());
    job.engagement_id = None;
    assert!(!job.deliverable());
}
