use erpx_server_identity_access::public::permission_catalog::*;
use std::collections::BTreeSet;
#[test]
fn current_permission_catalog_matches_legacy_explicit_iam_scope_and_identity_decisions() {
    let catalog = permission_catalog().unwrap();
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/identity_access/permissions/legacy-explicit-permissions.v1.json"
    ))
    .unwrap();
    assert_eq!(catalog.definitions().len(), 200);
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 3216);
    for case in corpus["cases"].as_array().unwrap() {
        let permission = case["permission"].as_str().unwrap();
        let permissions = if case["granted"] == true {
            BTreeSet::from([permission.to_owned()])
        } else {
            BTreeSet::new()
        };
        assert_eq!(
            catalog.allows_current_permissions(
                permission,
                case["scope"].as_str().unwrap(),
                case["identity"].as_bool().unwrap(),
                &permissions
            ),
            case["expected"].as_bool().unwrap(),
            "{case}"
        );
    }
}
#[test]
fn unknown_preview_and_scope_restricted_permissions_never_use_role_name_fallback() {
    let catalog = permission_catalog().unwrap();
    let empty = BTreeSet::new();
    assert!(!catalog.allows_current_permissions("finance.read", "holding", true, &empty));
    assert!(!catalog.allows_current_permissions(
        "unknown",
        "holding",
        true,
        &BTreeSet::from(["unknown".into()])
    ));
    for p in catalog
        .definitions()
        .iter()
        .filter(|p| p.availability == PermissionAvailability::Preview)
    {
        assert!(!catalog.allows_current_permissions(
            &p.id,
            "holding",
            true,
            &BTreeSet::from([p.id.clone()])
        ));
    }
    assert!(!catalog.allows_current_permissions(
        "iam.roles.manage",
        "department",
        true,
        &BTreeSet::from(["iam.roles.manage".into()])
    ));
    assert!(catalog.allows_current_permissions("iam.self", "department", true, &empty));
    assert!(!catalog.allows_current_permissions("iam.self", "department", false, &empty));
}
