//! Directory runtime composition contract tests.
use erpx_server_identity_access::public::directory_runtime::{
    DIRECTORY_RUNTIME_CONTRACT, DIRECTORY_RUNTIME_ROUTERS, DirectoryRuntimeComposition,
    directory_runtime_contract, validate_directory_runtime,
};

fn complete() -> DirectoryRuntimeComposition {
    DirectoryRuntimeComposition {
        registered_holdings: 3,
        account_policy: true,
        source_administration: true,
        source_scope_slugs: true,
        account_authentication: true,
        directory_authentication: true,
    }
}

#[test]
fn contract_publishes_the_native_source_and_sign_in_routes() {
    let contract = directory_runtime_contract();
    assert_eq!(contract, &DIRECTORY_RUNTIME_CONTRACT);
    assert!(contract.retained_typescript_transport);
    assert_eq!(contract.source_body_limit, 16 * 1024);
    assert_eq!(contract.sign_in_cookie_transport_owner, "account_http");
    assert_eq!(DIRECTORY_RUNTIME_ROUTERS.len(), 3);
    let preview = &DIRECTORY_RUNTIME_ROUTERS[0];
    assert_eq!(preview.name, "directory_source_preview");
    assert_eq!(preview.method, "POST");
    assert_eq!(
        preview.path,
        "/api/v1/holdings/{holding_slug}/iam/connections/{connection_id}/source/preview"
    );
    assert_eq!(
        preview.required_headers,
        &["host", "origin", "idempotency-key", "content-type"]
    );
    assert!(
        preview
            .required_composition
            .contains(&"DirectorySourceAdministration<C: Clock>")
    );
    let approve = &DIRECTORY_RUNTIME_ROUTERS[1];
    assert_eq!(approve.name, "directory_source_approve");
    assert!(
        approve
            .path
            .ends_with("/source/reviews/{review_id}/approve")
    );
    assert_eq!(approve.router_function, preview.router_function);
    let sign_in = &DIRECTORY_RUNTIME_ROUTERS[2];
    assert_eq!(sign_in.name, "directory_account_sign_in");
    assert_eq!(sign_in.method, "POST");
    assert_eq!(sign_in.path, "/api/auth/directory/sign-in");
    assert_eq!(sign_in.required_headers, &["host", "content-type"]);
    assert!(
        sign_in
            .required_composition
            .contains(&"AccountAuthentication")
    );
    assert!(
        sign_in
            .required_composition
            .contains(&"DirectoryAuthentication")
    );
}

#[test]
fn composition_validation_fails_closed_with_the_exact_missing_binding() {
    assert!(validate_directory_runtime(complete()).is_ok());
    for (name, composition, code) in [
        (
            "no holdings",
            DirectoryRuntimeComposition {
                registered_holdings: 0,
                ..complete()
            },
            "DIRECTORY_RUNTIME_CONFIGURATION_INVALID",
        ),
        (
            "no policy",
            DirectoryRuntimeComposition {
                account_policy: false,
                ..complete()
            },
            "DIRECTORY_RUNTIME_CONFIGURATION_INVALID",
        ),
        (
            "no source admin",
            DirectoryRuntimeComposition {
                source_administration: false,
                ..complete()
            },
            "DIRECTORY_RUNTIME_SOURCE_ADMIN_MISSING",
        ),
        (
            "no scopes",
            DirectoryRuntimeComposition {
                source_scope_slugs: false,
                ..complete()
            },
            "DIRECTORY_RUNTIME_SOURCE_SCOPE_MISSING",
        ),
        (
            "no account auth",
            DirectoryRuntimeComposition {
                account_authentication: false,
                ..complete()
            },
            "DIRECTORY_RUNTIME_SIGN_IN_MISSING",
        ),
        (
            "no directory auth",
            DirectoryRuntimeComposition {
                directory_authentication: false,
                ..complete()
            },
            "DIRECTORY_RUNTIME_SIGN_IN_MISSING",
        ),
    ] {
        assert_eq!(
            validate_directory_runtime(composition).unwrap_err().code,
            code,
            "{name}"
        );
    }
}


mod directory_operation_vocabulary {
    use erpx_server_identity_access::public::directory_operation::{
        DirectoryAction, DirectoryChangeInput, DirectoryInventory, PreviewEvidence, affected_objects,
        canonical, compile_preferences, compile_registry_policies, preview_token,
        validate_object_target,
    };
    use serde_json::json;

fn inventory() -> DirectoryInventory {
    DirectoryInventory::checked(json!({
        "providerVersion": "v1",
        "observedAt": 1,
        "complete": true,
        "fields": [
            {"name": "phone", "type": "text", "read": true, "write": true}
        ],
        "objects": [
            {"id": "ou-root", "dn": "dc=acme", "name": "Acme", "kind": "ou", "parentId": null,
             "version": "1", "module": "container/ou", "properties": {}, "memberIds": []},
            {"id": "ou-child", "dn": "ou=eng,dc=acme", "name": "Eng", "kind": "ou", "parentId": "ou-root",
             "version": "1", "module": "container/ou", "properties": {}, "memberIds": []},
            {"id": "acct-1", "dn": "uid=ada,ou=eng,dc=acme", "name": "Ada", "kind": "account",
             "parentId": "ou-child", "version": "1", "module": "users/user", "properties": {}, "memberIds": []},
            {"id": "grp-1", "dn": "cn=staff,ou=eng,dc=acme", "name": "Staff", "kind": "group",
             "parentId": "ou-child", "version": "1", "module": "groups/group", "properties": {},
             "memberIds": ["acct-1"]}
        ]
    }))
    .expect("inventory")
}

fn change(action: serde_json::Value) -> DirectoryChangeInput {
    serde_json::from_value(json!({
        "connectionId": "conn",
        "expectedVersion": 2,
        "action": action,
        "reason": "reviewed change",
    }))
    .expect("change")
}

#[test]
fn rejects_invalid_names_and_reasons() {
    assert!(change(json!({"kind": "group.create", "name": "x", "parentId": "ou-root"}))
        .validate()
        .is_err());
    assert!(change(json!({"kind": "group.create", "name": "Legal Team", "parentId": "ou-root"}))
        .validate()
        .is_ok());
    let mut input = change(json!({"kind": "ou.create", "name": "Legal", "parentId": "ou-root"}));
    input.reason = "  x ".into();
    assert!(input.validate().is_err());
}

#[test]
fn validates_account_and_group_targets() {
    let inventory = inventory();
    let update = change(json!({
        "kind": "account.update",
        "objectId": "acct-1",
        "fields": {"phone": "123"}
    }));
    assert!(validate_object_target(&update.action, &inventory).is_ok());
    let unsupported = change(json!({
        "kind": "account.update",
        "objectId": "acct-1",
        "fields": {"email": "ada@acme.test"}
    }));
    assert!(validate_object_target(&unsupported.action, &inventory).is_err());

    let cycle = change(json!({
        "kind": "object.move",
        "objectId": "ou-root",
        "parentId": "ou-child"
    }));
    assert!(validate_object_target(&cycle.action, &inventory).is_err());

    let group_cycle = change(json!({
        "kind": "group.members",
        "objectId": "grp-1",
        "memberIds": ["grp-1"],
        "action": "add"
    }));
    assert!(validate_object_target(&group_cycle.action, &inventory).is_err());
}

#[test]
fn impact_is_transitive_for_parents_and_members() {
    let inventory = inventory();
    let action = change(json!({
        "kind": "account.update",
        "objectId": "acct-1",
        "fields": {"phone": "123"}
    }))
    .action;
    let ids = affected_objects(&action, &inventory)
        .into_iter()
        .map(|object| object.id)
        .collect::<Vec<_>>();
    assert!(ids.contains(&"acct-1".into()));
    assert!(ids.contains(&"grp-1".into()));
    assert!(!ids.contains(&"ou-child".into()));

    let group = change(json!({
        "kind": "group.members",
        "objectId": "grp-1",
        "memberIds": ["acct-1"],
        "action": "remove"
    }))
    .action;
    let ids = affected_objects(&group, &inventory)
        .into_iter()
        .map(|object| object.id)
        .collect::<Vec<_>>();
    assert!(ids.contains(&"acct-1".into()));
    assert!(ids.contains(&"grp-1".into()));
}

#[test]
fn preview_token_binds_input_and_evidence() {
    let input = change(json!({
        "kind": "ou.update",
        "objectId": "ou-child",
        "description": "Engineering"
    }));
    let observed = inventory();
    let evidence = DirectoryInventory::checked(
        serde_json::to_value(&observed).unwrap(),
    )
    .unwrap();
    let first = preview_token(&input, &PreviewEvidence {
        connection_version: 2,
        state_version: 1,
        inventory: evidence.clone(),
        objects: vec!["ou-child".into()],
        companies: vec!["acme".into()],
        identity_links: vec![],
    });
    let second = preview_token(&input, &PreviewEvidence {
        connection_version: 2,
        state_version: 1,
        inventory: evidence,
        objects: vec!["ou-child".into()],
        companies: vec!["acme".into()],
        identity_links: vec![],
    });
    assert_eq!(first, second);
    let mut changed = input.clone();
    changed.reason = "different review".into();
    assert_ne!(first, preview_token(&changed, &PreviewEvidence {
        connection_version: 2,
        state_version: 1,
        inventory: DirectoryInventory::checked(
            serde_json::to_value(&observed).unwrap(),
        )
        .unwrap(),
        objects: vec!["ou-child".into()],
        companies: vec!["acme".into()],
        identity_links: vec![],
    }));
}

#[test]
fn canonical_json_sorts_keys_and_rejects_duplicate_identity() {
    assert_eq!(
        canonical(&json!({"b": 1, "a": [{"d": 2, "c": 3}]})),
        json!({"a": [{"c": 3, "d": 2}], "b": 1})
    );
    let duplicate = DirectoryInventory::checked(json!({
        "providerVersion": "v1",
        "observedAt": 1,
        "complete": true,
        "fields": [],
        "objects": [
            {"id": "a", "dn": "a", "name": "a", "kind": "ou", "parentId": null,
             "version": "1", "module": "container/ou", "properties": {}, "memberIds": []},
            {"id": "a", "dn": "b", "name": "b", "kind": "ou", "parentId": null,
             "version": "1", "module": "container/ou", "properties": {}, "memberIds": []}
        ]
    }));
    assert!(duplicate.is_err());
}

#[test]
fn action_kind_and_windows_policy_validation() {
    let action: DirectoryAction = serde_json::from_value(json!({
        "kind": "gpo.publish",
        "definition": {
            "name": "Baseline",
            "scope": "user",
            "targetOuId": "ou-root",
            "filterGroupId": "grp-1"
        }
    }))
    .unwrap();
    assert_eq!(action.kind(), "gpo.publish");
    let bad: Result<DirectoryAction, _> = serde_json::from_value(json!({
        "kind": "gpo.publish",
        "definition": {
            "name": "Baseline",
            "scope": "computer",
            "targetOuId": "ou-root",
            "filterGroupId": "grp-1"
        }
    }));
    let bad = bad.unwrap();
    assert!(change(serde_json::to_value(bad).unwrap()).validate().is_err());
}
    #[test]
    fn compiles_registry_and_preference_policy_artifacts() {
        let policy: erpx_server_identity_access::public::directory_operation::WindowsPolicyDefinition =
            serde_json::from_value(json!({
                "name": "Baseline",
                "scope": "user",
                "targetOuId": "ou-root",
                "filterGroupId": "grp-1",
                "usb": "deny",
                "lockSeconds": 300,
                "wallpaper": "\\\\files\\wall.bmp",
                "wallpaperStyle": "10",
                "drives": [{"id": "11111111-1111-4111-8111-111111111111", "letter": "H",
                            "path": "\\\\files\\home", "label": "Home"}],
                "printers": [{"id": "22222222-2222-4222-8222-222222222222",
                              "path": "\\\\files\\printer", "isDefault": true}]
            }))
            .unwrap();
        let registry = compile_registry_policies(&policy);
        assert!(registry.iter().any(|entry| entry["name"] == "Deny_All"));
        assert!(registry.iter().any(|entry| entry["name"] == "ScreenSaveTimeOut"));
        assert!(registry.iter().any(|entry| entry["name"] == "Wallpaper"));
        let preferences = compile_preferences(&policy, "2026-01-01 00:00:00");
        assert_eq!(preferences.len(), 2);
        assert!(preferences[0]["xml"]
            .as_str()
            .unwrap()
            .contains("letter=\"H\""));
        assert!(preferences[1]["xml"]
            .as_str()
            .unwrap()
            .contains("default=\"1\""));
    }

}
