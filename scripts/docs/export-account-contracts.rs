use erpx_server_identity_access::public::account_contracts::*;
use ts_rs::{Config, TS};

fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments == ["--openapi"] {
        let mut document = erpx_server_identity_access::public::account_openapi();
        document.merge(erpx_server_identity_access::public::credentials_openapi());
        document.info.title = "ERPX native Account authentication".into();
        document.info.description=Some("Native Account authentication and credential lifecycle. Unmounted during migration; provider delivery, current invitation owner authority assembly, SSO and complete owner authorization remain pending. This document is not runtime or release acceptance.".into());
        if let Some(alias) = document.paths.paths.get("/api/auth/sign-in/email").cloned() {
            document
                .paths
                .paths
                .insert("/api/v1/account/device-accounts/sign-in".into(), alias);
        }
        println!(
            "{}",
            document
                .to_pretty_json()
                .expect("Account OpenAPI document serializes")
        );
        return;
    }
    assert!(
        arguments.is_empty(),
        "Unsupported Account contract exporter arguments"
    );
    let configuration = Config::new();
    println!("// Generated from the Rust Account HTTP contracts. Do not edit by hand.");
    println!("// Regenerate: bun scripts/docs/generate-account-contracts.ts");
    for declaration in [
        AccountRevision::decl(&configuration),
        EmailSignInInput::decl(&configuration),
        EmailSignUpInput::decl(&configuration),
        EmailVerificationInput::decl(&configuration),
        PasswordChangeInput::decl(&configuration),
        PasswordResetRequestInput::decl(&configuration),
        PasswordResetInput::decl(&configuration),
        EmailVerificationRequestInput::decl(&configuration),
        DeviceSelectionInput::decl(&configuration),
        AccountUserDto::decl(&configuration),
        AccountSessionDto::decl(&configuration),
        AccountSessionResponse::decl(&configuration),
        DeviceAccountDto::decl(&configuration),
        DeviceAccountsDto::decl(&configuration),
        AccountAuthError::decl(&configuration),
        AccountAuthErrorResponse::decl(&configuration),
        EmailSignInResponse::decl(&configuration),
        AccountOperationResponse::decl(&configuration),
    ] {
        println!("export {declaration}");
    }
}
