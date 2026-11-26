pub mod account_contracts;
pub mod account_mail;
pub mod account_profile_read;
pub mod account_sessions;
pub mod account_sso;
pub mod account_time_preference;
pub mod account_time_preference_http;
pub mod account_invitation_list_read;
pub mod identity_write;
pub mod identity_write_http;
pub mod portal_session;
pub use crate::adapters::account_crypto::{
    Argon2Passwords, SystemAccountClock, SystemAuthenticationSecrets,
};
pub use crate::adapters::account_http::account_auth_router;
#[cfg(feature = "contracts")]
pub use crate::adapters::account_http::account_openapi;
pub use crate::adapters::account_postgres::PostgresAccountRepository;
pub use crate::application::account_auth::{
    AccountAuthentication, InitializedDevice, PresentedSecrets, SelectedAccount, SignedIn,
};
pub use crate::application::account_ports::{
    AccountFuture, AccountRepository, AuthenticationSecrets, CompleteSignIn, CompletedSignIn,
    DeviceProof, DeviceSelection, PasswordHashing, ReserveSignIn, SelectedDevice,
    SignInReservation,
};
pub use crate::domain::account_auth::{
    AccountPolicy, AccountSession, AccountUser, CredentialSnapshot, DeviceAccount, DeviceAccounts,
    EmailAddress, HoldingOrigin, MAXIMUM_SAVED_ACCOUNTS, OpaqueSecret, Password, PasswordHash,
    ProductOrigin, RequestAudience, SESSION_LIFETIME_MS, SecretDigest, SecretPurpose,
    valid_session_window,
};

pub mod work_context;
pub use crate::adapters::account_credentials_http::account_credentials_router;
#[cfg(feature = "contracts")]
pub use crate::adapters::account_credentials_http::credentials_openapi;
pub use crate::adapters::account_credentials_postgres::PostgresAccountCredentials;
pub use crate::adapters::account_invitations_http::account_invitations_router;
#[cfg(feature = "contracts")]
pub use crate::adapters::account_invitations_http::invitation_openapi;
pub use crate::adapters::account_invitations_postgres::PostgresAccountInvitations;
pub use crate::adapters::account_mail_crypto::{AccountMailKeyring, OpenedAccountMail};
pub use crate::adapters::postgres_invitation_authority::PostgresInvitationCredentialAuthority;
pub use crate::application::account_credentials::{
    AccountCredentialRepository, AccountCredentials, AccountMailCryptography, ChangeCredential,
    InvitationCredentialAuthority, InvitationCredentialLease, InvitationCredentialQuery,
    PreparedCredentialMail, ResetCredential, SignUpCredential,
    UnavailableInvitationCredentialAuthority, VerifiedCredential, VerifyCredential,
};
pub use crate::application::account_invitations::{
    AccountInvitationRepository, AccountInvitations, InvitationAccountState,
};
pub use crate::domain::account_credentials::{
    CREDENTIAL_PROOF_LIFETIME_MS, CredentialProofKind, InvitationCredentialFacts,
    account_return_path, validate_invitation_credential,
};
pub use crate::domain::invitation_authority::{
    DirectInvitationGrant, InvitationGrant, StaffInvitationGrant, invitation_scope_covers,
};

pub mod permission_catalog;
pub mod permission_catalog_lock;
pub mod quality_recipients;
pub mod quality_scrap_approvers;
pub mod sales_recipient;
pub mod work_admission;

pub mod effective_authority;
pub mod release_reviewers;

pub mod account_revocation;

pub mod command_metadata;

pub mod account_device_http;
pub mod account_security;
pub mod account_security_http;

pub mod assignment;
pub mod assignment_http;

pub mod account_holding_bindings;
pub mod account_inbound_http;

pub mod directory;
#[cfg(feature = "directory-ldaps")]
pub mod directory_connection;

pub use crate::adapters::account_http::account_session_router;
pub mod operational_worker;

pub mod oidc_url_values;

pub mod field_service_assignment;

pub mod journal_flow_participants;

pub mod field_service_workspace;

pub mod selected_command_authority;
pub mod service_count_flow_participants;
pub mod service_initial_count_authority;

pub mod collaboration_candidates;

pub mod push_session;

pub mod account_work_roles;

pub mod account_companies;

pub mod account_control_scope;
pub mod platform_commercial_authority;

pub mod account_work_presentation;
pub mod operations_recipients;
pub mod quality_same_person;
pub mod staff_notifications;

pub mod directory_runtime;
pub mod iam_roles;
pub mod iam_roles_http;

pub mod staff_facts;

pub mod pms_social_event_recipients;

pub mod routine_owner;

pub mod position_gpo;

/// Native Position GPO HTTP ingress, re-exported from the owner adapter.
pub mod position_gpo_http {
    pub use crate::adapters::position_gpo_http::*;
}

pub mod private_file_denial;
pub mod private_file_denial_http;

/// Byte-pinned owner plans for TypeScript read fragments retired in this owner.
pub mod native_read_plan {
    pub use crate::adapters::native_read_plan::{plan_sha256, FINANCE_REFUND_NOTICE_ASSIGNMENT_SQL};
}

/// Native Control read-plan port: a closed kind resolves to a byte-pinned
/// statement the caller executes on its own Control unit.
pub mod control_read_plan {
    pub use crate::adapters::control_read_plan::{
        ControlReadPlan, ControlReadTarget, PC_CONTEXT_RECEIPT, PC_CONNECTION_ROUTE,
        PC_DELIVERABLE_INVITATION, PC_FRESH_MFA, PC_INVITATION_WAITING, PC_MFA_ENABLED,
        PC_PLATFORM_ASSIGNMENTS, PC_PREF_BINDING_PROBE, PC_PREF_BINDING_SHARE,
        PC_PREF_PARENT_BINDING_SHARE, PC_PREF_SESSION_IMAGE, PC_PREF_SESSION_SHARE,
        PC_QUEUED_ASSIGNMENT_VERSION, PC_WORKFLOW_ACTIVE_CONTEXT,
        PC_DIAGNOSIS_ASSIGNMENTS, PC_DIAGNOSIS_MEMBER_PROBE, PC_DIAGNOSIS_PLATFORM_ASSIGNMENTS,
        PC_PROVIDER_CONTEXT, PC_REVOKED_SESSION, PC_WORKSPACE_PREFERENCE, plan,
        router_for_holdings, SQL_CONTEXT_RECEIPT, SQL_CONNECTION_ROUTE, SQL_DELIVERABLE_INVITATION,
        SQL_DIAGNOSIS_ASSIGNMENTS, SQL_DIAGNOSIS_MEMBER_PROBE, SQL_DIAGNOSIS_PLATFORM_ASSIGNMENTS,
        SQL_FRESH_MFA, SQL_INVITATION_WAITING, SQL_MFA_ENABLED, SQL_PLATFORM_ASSIGNMENTS,
        SQL_PREF_BINDING_PROBE, SQL_PREF_BINDING_SHARE, SQL_PREF_PARENT_BINDING_SHARE,
        SQL_PREF_SESSION_IMAGE, SQL_PREF_SESSION_SHARE, SQL_PROVIDER_CONTEXT,
        SQL_QUEUED_ASSIGNMENT_VERSION, SQL_WORKFLOW_ACTIVE_CONTEXT,
        SQL_REVOKED_SESSION, SQL_WORKSPACE_PREFERENCE,
    };
}

pub mod people_access;

/// Native Identity port for People account linking (Control DB `user` plus
/// Holding assignments), composed into the People workforce command.
pub mod people_account_link {
    pub use crate::adapters::people_account_link::{
        into_business, revoke_company_access, verify_person_account_link,
    };
}
pub mod people_access_http;
pub mod people_grants;
pub mod people_grants_http;
pub mod recovery;
pub mod recovery_http;

pub mod user_management;

pub mod directory_operation;
pub mod messaging_people;
pub mod records_principal_search {
    pub use crate::adapters::records_principal_search::{active_assignment, search_principals};
}
pub mod portal_private_session_lease;
pub mod rfq_request_membership;
pub mod rfq_target_authority;
pub mod native_lane_10;
pub mod native_lane_11;
pub mod native_lane_12;
pub mod native_lane_44;
pub mod native_lane_45;
pub mod native_lane_46;
