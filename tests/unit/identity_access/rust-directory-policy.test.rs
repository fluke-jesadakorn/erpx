use erpx_server_identity_access::public::directory::*;
use serde::Deserialize;
use std::collections::BTreeMap;

fn oracle() -> serde_json::Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/identity_access/directory/conformance.v1.json"
    )))
    .unwrap()
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Container {
    external_id: String,
    parent_id: Option<String>,
    name: String,
    kind: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Placement {
    kind: String,
    external_id: String,
    container_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Edge {
    parent_id: String,
    child_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Hierarchy {
    containers: Vec<Container>,
    placements: Vec<Placement>,
    group_edges: Vec<Edge>,
}
#[derive(Deserialize)]
struct HierarchyCase {
    name: String,
    users: Vec<String>,
    groups: Vec<String>,
    hierarchy: Hierarchy,
    ok: bool,
}
fn hierarchy(value: Hierarchy) -> DirectoryHierarchy {
    DirectoryHierarchy {
        containers: value
            .containers
            .into_iter()
            .map(|v| ObservedContainer {
                external_id: v.external_id,
                parent_id: v.parent_id,
                name: v.name,
                kind: v.kind,
            })
            .collect(),
        placements: value
            .placements
            .into_iter()
            .map(|v| DirectoryPlacement {
                kind: v.kind,
                external_id: v.external_id,
                container_id: v.container_id,
            })
            .collect(),
        group_edges: value
            .group_edges
            .into_iter()
            .map(|v| DirectoryGroupEdge {
                parent_id: v.parent_id,
                child_id: v.child_id,
            })
            .collect(),
    }
}
#[test]
fn actual_b0_hierarchy_oracle_preserves_original_root_order_depth_and_complete_topology() {
    let cases: Vec<HierarchyCase> = serde_json::from_value(oracle()["hierarchy"].clone()).unwrap();
    assert_eq!(cases.len(), 32);
    for c in cases {
        let users = c
            .users
            .into_iter()
            .map(|id| ObservedDirectoryUser {
                external_id: id.clone(),
                username: id.clone(),
                name: id,
                email: None,
                enabled: true,
                group_ids: vec![],
            })
            .collect();
        let groups = c
            .groups
            .into_iter()
            .map(|id| ObservedDirectoryGroup {
                external_id: id.clone(),
                name: id,
            })
            .collect();
        assert_eq!(
            CompleteDirectorySnapshot::checked(
                users,
                groups,
                hierarchy(c.hierarchy),
                1_800_000_000_000
            )
            .is_ok(),
            c.ok,
            "{}",
            c.name
        );
    }
}
#[test]
fn actual_b0_control_coercion_and_binary_immutable_id_oracles_are_preserved() {
    let fixture = oracle();
    let now = fixture["now"].as_i64().unwrap();
    let cases = fixture["enabled"].as_array().unwrap();
    assert_eq!(cases.len(), 44);
    for c in cases {
        let v = &c["values"];
        let field = |name| v[name].as_str().unwrap_or("");
        let result = LdapAccountControls {
            user_account_control: field("userAccountControl"),
            account_expires: field("accountExpires"),
            shadow_expire: field("shadowExpire"),
            password_locked_at: field("pwdAccountLockedTime"),
            samba_account_flags: field("sambaAcctFlags"),
        }
        .enabled(c["ad"].as_bool().unwrap(), now);
        assert_eq!(result.is_ok(), c["ok"].as_bool().unwrap(), "{}", c["name"]);
        if let Ok(enabled) = result {
            assert_eq!(enabled, c["enabled"].as_bool().unwrap(), "{}", c["name"]);
        }
    }
    for c in fixture["guids"].as_array().unwrap() {
        let bytes: Vec<u8> = serde_json::from_value(c["bytes"].clone()).unwrap();
        assert_eq!(ad_guid(&bytes).unwrap(), c["result"].as_str().unwrap());
    }
    assert!(ad_guid(&[0; 15]).is_err());
    assert!(ad_guid(&[0; 17]).is_err());
}
#[test]
fn every_unicode_scalar_lowercase_mapping_and_contextual_alias_case_matches_actual_node() {
    let fixture = oracle();
    assert_eq!(fixture["node"], "v26.11.1");
    assert_eq!(char::UNICODE_VERSION, (17, 0, 0));
    let expected = fixture["lower"].as_array().unwrap();
    let mut index = 0;
    for value in 0..=0x10ffffu32 {
        if let Some(ch) = char::from_u32(value) {
            let input = ch.to_string();
            let lower = input.to_lowercase();
            if input != lower {
                assert_eq!(expected[index]["input"], input);
                assert_eq!(expected[index]["expected"], lower);
                index += 1;
            }
        }
    }
    assert_eq!(index, 1488);
    assert_eq!(index, expected.len());
    for case in fixture["contexts"].as_array().unwrap() {
        assert_eq!(
            case["input"].as_str().unwrap().to_lowercase(),
            case["expected"]
        );
    }
    let aliases: BTreeMap<String, String> =
        serde_json::from_value(fixture["aliasMap"].clone()).unwrap();
    for case in fixture["aliases"].as_array().unwrap() {
        assert_eq!(
            canonical_external_id(&aliases, case["input"].as_str().unwrap()),
            case["result"]
        );
    }
}
#[test]
fn a_snapshot_is_never_a_grant_and_rejects_unknown_membership_or_stale_observation() {
    let empty = || DirectoryHierarchy {
        containers: vec![],
        placements: vec![],
        group_edges: vec![],
    };
    let source =
        CompleteDirectorySnapshot::checked(vec![], vec![], empty(), 1_800_000_000_000).unwrap();
    assert!(source.require_fresh(1_800_000_299_999).is_ok());
    assert!(source.require_fresh(1_800_000_300_000).is_err());
    assert!(source.require_fresh(1_799_999_999_999).is_err());
    let missing = ObservedDirectoryUser {
        external_id: "actual-user".into(),
        username: "a".into(),
        name: "A".into(),
        email: None,
        enabled: true,
        group_ids: vec!["not-observed".into()],
    };
    assert!(
        CompleteDirectorySnapshot::checked(vec![missing], vec![], empty(), 1_800_000_000_000)
            .is_err()
    );
    let huge = ObservedDirectoryGroup {
        external_id: "g".into(),
        name: "a".repeat(16_385),
    };
    assert!(
        CompleteDirectorySnapshot::checked(vec![], vec![huge], empty(), 1_800_000_000_000).is_err()
    );
}
