use erpx_server_identity_access::public::account_contracts::*;
use erpx_server_identity_access::public::account_device_http::DeviceAccountsResponse;
use ts_rs::{Config, TS};

fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments == ["--openapi"] {
        let mut document = erpx_server_identity_access::public::account_openapi();
        document.merge(erpx_server_identity_access::public::credentials_openapi());
        document.merge(erpx_server_identity_access::public::account_sso::sso_openapi());
        document.merge(erpx_server_identity_access::public::invitation_openapi());
        document.info.title = "ERPX native Account authentication".into();
        document.info.description=Some("Native Account authentication and credential lifecycle. Unmounted during migration; worker/provider runtime assembly and live delivery acceptance, invitation administration/existing-account linking, directory/MFA and complete owner authorization remain pending. This document is not runtime or release acceptance.".into());
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
        AccountSsoStartInput::decl(&configuration),
        AccountSsoAuthorizeInput::decl(&configuration),
        AccountSsoCallbackInput::decl(&configuration),
        AccountInvitationInspectInput::decl(&configuration),
        AccountInvitationAcceptInput::decl(&configuration),
        AccountInvitationInfo::decl(&configuration),
        AccountInvitationInspectResponse::decl(&configuration),
        AccountInvitationAccepted::decl(&configuration),
        AccountInvitationAcceptResponse::decl(&configuration),
        PasswordResetRequestInput::decl(&configuration),
        PasswordResetInput::decl(&configuration),
        AccountPasswordResetLinkQuery::decl(&configuration),
        EmailVerificationRequestInput::decl(&configuration),
        DeviceSelectionInput::decl(&configuration),
        DeviceInitializeInput::decl(&configuration),
        AccountUserDto::decl(&configuration),
        AccountSessionDto::decl(&configuration),
        AccountSessionResponse::decl(&configuration),
        DeviceAccountDto::decl(&configuration),
        DeviceAccountsDto::decl(&configuration),
        DeviceAccountsResponse::decl(&configuration),
        AccountAuthError::decl(&configuration),
        AccountAuthErrorResponse::decl(&configuration),
        EmailSignInResponse::decl(&configuration),
        AccountOperationResponse::decl(&configuration),
    ] {
        println!("export {declaration}");
    }
}
