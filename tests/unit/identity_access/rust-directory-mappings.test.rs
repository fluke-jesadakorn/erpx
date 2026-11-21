use erpx_server_identity_access::public::directory::grants::*;
use serde_json::Value;
fn fixture() -> Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/identity_access/directory/mappings.v1.json"
    )))
    .unwrap()
}
fn evaluate(case: &Value, now: i64) -> Result<(), erpx_server_shared::public::BusinessError> {
    let f = &case["facts"];
    let parent = &f["parent"];
    let role = &f["role"];
    let member = &f["member"];
    let input = &f["input"];
    let node = &f["node"];
    let ceiling: Option<Vec<String>> = if f["parentRole"].is_null() {
        None
    } else {
        Some(serde_json::from_value(f["parentRole"]["grantable_json"].clone()).unwrap())
    };
    let permissions: Option<Vec<String>> = if role.is_null() {
        None
    } else {
        Some(serde_json::from_value(role["permissions_json"].clone()).unwrap())
    };
    let grantable: Option<Vec<String>> = if role.is_null() {
        None
    } else {
        Some(serde_json::from_value(role["grantable_json"].clone()).unwrap())
    };
    let scopes: Option<Vec<String>> = if role.is_null() {
        None
    } else {
        Some(serde_json::from_value(role["scopes_json"].clone()).unwrap())
    };
    let facts = DirectoryGrantFacts {
        same_person: f["same"].as_bool().unwrap() || input["userId"] == f["actor"]["userId"],
        parent: if parent.is_null() {
            None
        } else {
            Some(DirectoryParentFact {
                current: f["parentCurrent"].as_bool().unwrap(),
                company_matches: parent["company_id"] == f["actor"]["companyId"],
                scope: parent["scope_kind"].as_str().unwrap(),
                node: parent["scope_node_id"].as_str(),
                valid_until: parent["valid_until"].as_i64().unwrap(),
                grantable: ceiling.as_deref(),
            })
        },
        member: if member.is_null() {
            None
        } else {
            Some(DirectoryMemberFact {
                active: member["status"] == "active",
                directory: member["login_policy"] == "directory",
            })
        },
        role: if role.is_null() {
            None
        } else {
            Some(DirectoryRoleFact {
                revision: role["published_revision"].as_i64().unwrap(),
                permissions: permissions.as_ref().unwrap(),
                grantable: grantable.as_ref().unwrap(),
                scopes: scopes.as_ref().unwrap(),
            })
        },
        requested_scope: input["scopeKind"].as_str().unwrap(),
        requested_node: input["scopeNodeId"].as_str(),
        requested_until: input["validUntil"].as_i64().unwrap(),
        scope: if node.is_null() {
            None
        } else {
            Some(DirectoryScopeFact {
                branch: if node["kind"] == "branch" {
                    node["id"].as_str()
                } else {
                    node["branch_id"].as_str()
                },
                department: if node["kind"] == "department" {
                    node["id"].as_str()
                } else {
                    None
                },
            })
        },
        platform: f["actor"]["platform"].as_bool().unwrap(),
    };
    validate_mapping_grant(&facts, now)
}
#[test]
fn ordinary_directory_assembled_b0_mapping_cases_keep_the_same_decisions_and_error_kinds() {
    let fixture = fixture();
    let now = fixture["now"].as_i64().unwrap();
    let mut count = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        if !case["directoryAssembled"].as_bool().unwrap()
            || id == "baseline-holding-extra-node-is-ignored"
        {
            continue;
        }
        count += 1;
        let result = evaluate(case, now);
        assert_eq!(result.is_ok(), case["ok"].as_bool().unwrap(), "{id}");
        if let Err(error) = result {
            assert_eq!(error.code, case["error"]["code"].as_str().unwrap(), "{id}");
            let expected = match case["error"]["status"].as_i64().unwrap() {
                403 => erpx_server_shared::public::FailureKind::Forbidden,
                409 => erpx_server_shared::public::FailureKind::Conflict,
                422 => erpx_server_shared::public::FailureKind::InvalidInput,
                _ => panic!("unexpected source error class"),
            };
            assert_eq!(error.kind, expected, "{id}");
        }
    }
    assert_eq!(count, 24);
}
#[test]
fn the_two_preserved_b0_scope_quirks_have_explicit_native_rejections_without_normalization() {
    let fixture = fixture();
    let now = fixture["now"].as_i64().unwrap();
    for id in [
        "baseline-holding-extra-node-is-ignored",
        "baseline-platform-branch-request-is-ignored-by-insert",
    ] {
        let case = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["id"] == id)
            .unwrap();
        assert!(
            case["ok"].as_bool().unwrap(),
            "unchanged B0 oracle must retain its observed acceptance"
        );
        let before = case["facts"]["input"].clone();
        let error = evaluate(case, now).unwrap_err();
        assert_eq!(error.code, "SCOPE_FORBIDDEN");
        assert_eq!(
            case["facts"]["input"], before,
            "native rejection must not rewrite source scope fields"
        );
    }
}
