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
