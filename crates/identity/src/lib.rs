#![doc = "Bounded ERPX identity, session, recovery, TOTP, and passkey primitives."]
#![forbid(unsafe_code)]

// The standalone directory synchronization acceptance target includes the
// source module directly. Keep a stable self-alias so its test-only imports
// can use the same crate-qualified type names as the library build.
extern crate self as erpx_identity;

mod access_token;
mod ceremony;
mod challenge;
pub mod development_access;
mod device;
pub mod directory;
mod error;
mod password;
mod recovery;
mod secret;
mod session;
mod signup;
mod store;
mod totp;
mod webauthn;

pub use access_token::{
    AccessToken, AccessTokenClaims, AccessTokenIssueWindow, AccessTokenLifetime, AccessTokenSigner,
    ActiveRoleClaim, AuthenticatedAuthority, AuthenticatedPrincipalKind, AuthenticatedRealm,
    MAX_ACCESS_TOKEN_LIFETIME_SECONDS,
};
pub use ceremony::{
    BrowserAuthenticationCeremonyStart, BrowserRegistrationCeremonyStart, PasskeyCeremonyService,
    PasskeyCeremonyStart,
};
pub use challenge::{
    ChallengeCeremony, ChallengeLifetime, ChallengeMaterial, MAX_CHALLENGE_LIFETIME_SECONDS,
    WebAuthnChallenge,
};
pub use development_access::{
    BUSINESS_UNITS, BusinessUnitRoleLevel, DEVELOPMENT_CLUSTER_MARKER, DEVELOPMENT_CONTEXT_HEADER,
    DEVELOPMENT_CONTEXT_LIFETIME, DEVELOPMENT_CONTROLLER_MUTATION_PERMISSIONS,
    DEVELOPMENT_CONTROLLER_PRINCIPAL, DEVELOPMENT_CONTROLLER_READ_PERMISSIONS,
    DEVELOPMENT_CONTROLLER_ROLE, DEVELOPMENT_CONTROLLER_ROLE_PROFILE,
    DEVELOPMENT_CONTROLLER_TARGET_HOLDING, DEVELOPMENT_FRESH_PROFILE,
    DEVELOPMENT_MFA_BYPASS_PERMISSION, DEVELOPMENT_PERSONA_MODE, DEVELOPMENT_PROFILE,
    DEVELOPMENT_SOD_BYPASS_PERMISSION, DevelopmentAccessError, DevelopmentAccessGates,
    DevelopmentControllerRoleConfig, DevelopmentPersona, DevelopmentPersonaContext,
    DirectoryRoleTargetScope, PermissionCatalogEntry, context_expiry, context_token_hash,
    demo_personas, development_controller_permissions, development_controller_role_config_from_env,
    development_gates_from_env, issue_context_token, permission_catalog, platform_personas,
    require_development_gates,
};
pub use device::DeviceRecord;
pub use directory::live::{
    DirectoryPreflightEvidence, LiveDirectoryError, authenticate_directory_user,
    authenticate_directory_user_with_resolver, authenticate_directory_user_with_runtime,
    preflight_directory_connector,
};
pub use directory::secrets::{
    DirectorySecretBundle, DirectorySecretError, DirectorySecretResolver,
    HttpsDirectorySecretResolver, LocalDirectorySecretResolver,
    UnconfiguredDirectorySecretResolver,
};
pub use directory::{
    DIRECTORY_PREFLIGHT_MAX_AGE, DIRECTORY_REVOCATION_SLA, DirectoryAuthDecision,
    DirectoryAuthPolicy, DirectoryAuthRequest, DirectoryChange, DirectoryConnector,
    DirectoryConnectorLifecycle, DirectoryConnectorReadinessSnapshot,
    DirectoryConnectorRuntimeSettings, DirectoryConnectorSnapshot, DirectoryError,
    DirectoryFeedKind, DirectoryIdentityConflictSnapshot, DirectoryImportPlan,
    DirectoryImportStatus, DirectoryPrincipalBindingSnapshot, DirectoryProvider,
    DirectoryRevocationState, DirectoryRisk, DirectoryRoleAction, DirectoryRoleFact,
    DirectoryRoleMapping, DirectoryScope, DirectorySupervisorClaim, DirectorySyncCursor,
    DirectorySyncPage, DirectoryTlsMode, DirectoryTlsPolicy, DirectoryUser,
    MAX_DIRECTORY_BATCH_SIZE, MAX_DIRECTORY_IMPORT_USERS, MAX_DIRECTORY_PAGE_SIZE,
    SqlxDirectoryStore, directory_authority_key, immutable_identity_link,
};
pub use error::IdentityError;
pub use password::{PasswordDigest, PasswordHashingConfig, PasswordService};
pub use recovery::{RecoveryCodeBatch, RecoveryCodeRecord, RecoveryCodeSet, RecoverySession};
pub use secret::{SecretBytes, SecretValue, TokenHash};
pub use session::{
    IssuedSession, RefreshRotationOutcome, RotatedSession, SessionRecord, SessionService,
};
pub use signup::{
    InitialOwnerAssignment, SIGNUP_CHALLENGE_LIFETIME, SIGNUP_RATE_LIMIT_ATTEMPTS,
    SIGNUP_RATE_LIMIT_WINDOW, SignupChallengeSecret, SignupEmailChallenge, SignupEmailDelivery,
    SignupError, SignupMailAdapter, SignupMailError, SignupRateWindow,
};
pub use store::{
    AccountSessionSummary, DeviceSnapshot, DirectoryRoleAccess, IdentityStoreError,
    RateLimitDecision, RateLimitPolicy, RoleOption, RoleSelectionSource, SqlxIdentityStore,
};
pub use totp::{
    KeyResolver, TOTP_ALGORITHM, TOTP_CLOCK_SKEW_STEPS, TOTP_DIGITS, TOTP_PERIOD_SECONDS,
    TotpEnrollmentStart, TotpKeyResolver, TotpSecret, TotpSecretEnvelope, decrypt_totp_secret,
    encrypt_totp_secret, verify_totp_code,
};
pub use webauthn::{
    AssertionRequest, AssertionVerification, BrowserAssertionRequest, BrowserAuthenticationStart,
    BrowserRegistrationRequest, BrowserRegistrationStart, ConfiguredWebAuthnProvider,
    PasskeyCredential, PasskeyService, RegistrationRequest, UnsupportedWebAuthnProvider,
    VerifiedRegistration, WebAuthnProvider, WebAuthnProviderConfig, WebAuthnRsProvider,
};

/// The identity schema is deliberately not part of the shared control/read
/// migrator. It must be applied to a control write database only. These
/// migration fragments live outside the control-plane migration directory so
/// they are never accidentally applied to a control read database.
pub const IDENTITY_MIGRATION_FILENAME: &str = "0001_identity.sql";
pub const IDENTITY_MIGRATION_SQL: &str = include_str!("../migrations/0001_identity.sql");
pub const IDENTITY_RECONCILIATION_MIGRATION_SQL: &str =
    include_str!("../migrations/0002_directory_reconciliation_runs.sql");
pub const IDENTITY_SOURCE_PAYLOAD_MIGRATION_SQL: &str =
    include_str!("../migrations/0003_directory_source_payload_hash.sql");
pub const IDENTITY_REVOCATION_APPLIED_AT_MIGRATION_SQL: &str =
    include_str!("../migrations/0004_directory_revocation_applied_at.sql");
pub const IDENTITY_ROLE_PROJECTION_CONSTRAINTS_MIGRATION_SQL: &str =
    include_str!("../migrations/0005_directory_role_projection_constraints.sql");
pub const IDENTITY_DIRECTORY_CAPABILITIES_MIGRATION_SQL: &str =
    include_str!("../migrations/0006_directory_capabilities.sql");
pub const IDENTITY_SCOPED_DIRECTORY_ADMINISTRATION_MIGRATION_SQL: &str =
    include_str!("../migrations/0007_scoped_directory_administration.sql");
pub const IDENTITY_DIRECTORY_PROFILE_LEGACY_DEFAULT_MIGRATION_SQL: &str =
    include_str!("../migrations/0008_directory_profile_legacy_default.sql");
pub const IDENTITY_DEVELOPMENT_ACCESS_MIGRATION_SQL: &str =
    include_str!("../migrations/0009_development_access.sql");
pub const IDENTITY_DIRECTORY_ROLE_TARGET_SCOPE_MIGRATION_SQL: &str =
    include_str!("../migrations/0010_directory_role_target_scope.sql");
pub const IDENTITY_ACTIVE_ROLE_SESSION_CONTEXT_MIGRATION_SQL: &str =
    include_str!("../migrations/0011_active_role_session_context.sql");
pub const IDENTITY_DEVELOPMENT_CONTEXT_LIFETIME_MIGRATION_SQL: &str =
    include_str!("../migrations/0012_development_persona_context_lifetime.sql");

/// Apply the identity schema and its forward reconciliation/outbox migration
/// to a write-side pool. Callers must not run this function against a read
/// database or expose the resulting tables to a projection rebuild.
pub async fn apply_identity_migration(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    // The bootstrap migration acquires a transaction-scoped advisory lock.
    // Keep every forward migration on the same transaction so concurrent
    // server/worker startup cannot release that lock between migration files.
    let mut transaction = pool.begin().await?;
    sqlx::raw_sql(IDENTITY_MIGRATION_SQL)
        .execute(&mut *transaction)
        .await?;
    sqlx::raw_sql(IDENTITY_RECONCILIATION_MIGRATION_SQL)
        .execute(&mut *transaction)
        .await?;
    sqlx::raw_sql(IDENTITY_SOURCE_PAYLOAD_MIGRATION_SQL)
        .execute(&mut *transaction)
        .await?;
    sqlx::raw_sql(IDENTITY_REVOCATION_APPLIED_AT_MIGRATION_SQL)
        .execute(&mut *transaction)
        .await?;
    sqlx::raw_sql(IDENTITY_ROLE_PROJECTION_CONSTRAINTS_MIGRATION_SQL)
        .execute(&mut *transaction)
        .await?;
    sqlx::raw_sql(IDENTITY_DIRECTORY_CAPABILITIES_MIGRATION_SQL)
        .execute(&mut *transaction)
        .await?;
    sqlx::raw_sql(IDENTITY_SCOPED_DIRECTORY_ADMINISTRATION_MIGRATION_SQL)
        .execute(&mut *transaction)
        .await?;
    sqlx::raw_sql(IDENTITY_DIRECTORY_PROFILE_LEGACY_DEFAULT_MIGRATION_SQL)
        .execute(&mut *transaction)
        .await?;
    sqlx::raw_sql(IDENTITY_DEVELOPMENT_ACCESS_MIGRATION_SQL)
        .execute(&mut *transaction)
        .await?;
    sqlx::raw_sql(IDENTITY_DIRECTORY_ROLE_TARGET_SCOPE_MIGRATION_SQL)
        .execute(&mut *transaction)
        .await?;
    sqlx::raw_sql(IDENTITY_ACTIVE_ROLE_SESSION_CONTEXT_MIGRATION_SQL)
        .execute(&mut *transaction)
        .await?;
    sqlx::raw_sql(IDENTITY_DEVELOPMENT_CONTEXT_LIFETIME_MIGRATION_SQL)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::{Duration, TimeZone, Utc};

    use super::*;

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 24, 12, 0, 0)
            .single()
            .expect("fixed test time")
    }

    #[test]
    fn password_hash_verifies_only_the_original_password() {
        let service = PasswordService::default();
        let digest = service.hash("correct horse battery staple").expect("hash");

        service
            .verify("correct horse battery staple", &digest)
            .expect("password verifies");
        assert!(matches!(
            service.verify("wrong", &digest),
            Err(IdentityError::PasswordVerificationFailed)
        ));
        assert!(digest.as_str().starts_with("$argon2id$"));
    }

    #[test]
    fn access_token_expiry_is_half_open_and_maximum_is_five_minutes() {
        let signer = AccessTokenSigner::new(b"01234567890123456789012345678901").expect("key");
        let lifetime = AccessTokenLifetime::try_new(Duration::minutes(5)).expect("five minutes");
        assert!(
            AccessTokenLifetime::try_new(Duration::minutes(5) + Duration::nanoseconds(1)).is_err()
        );
        let issued = signer
            .issue("principal-1", uuid::Uuid::new_v4(), now(), lifetime)
            .expect("token");

        signer
            .verify(
                issued.token(),
                now() + Duration::minutes(5) - Duration::nanoseconds(1),
            )
            .expect("token is valid before its boundary");
        assert!(matches!(
            signer.verify(issued.token(), now() + Duration::minutes(5)),
            Err(IdentityError::AccessTokenExpired)
        ));
    }

    #[test]
    fn refresh_rotation_detects_reuse_and_returns_a_revoked_record() {
        let service = SessionService::new(
            AccessTokenSigner::new(b"01234567890123456789012345678901").expect("key"),
            Duration::days(30),
        )
        .expect("service");
        let issued = service
            .issue(
                "principal-1",
                uuid::Uuid::new_v4(),
                now(),
                AccessTokenLifetime::five_minutes(),
            )
            .expect("session");
        let old_refresh = issued.refresh_token().as_str().to_owned();
        let rotated = service
            .rotate_refresh(issued.record(), &old_refresh, now() + Duration::seconds(1))
            .expect("rotation result");
        let rotated = match rotated {
            RefreshRotationOutcome::Rotated(value) => value,
            RefreshRotationOutcome::ReuseDetected { .. } => panic!("first use was reuse"),
        };

        let reused = service
            .rotate_refresh(rotated.record(), &old_refresh, now() + Duration::seconds(2))
            .expect("reuse is a typed outcome");
        match reused {
            RefreshRotationOutcome::ReuseDetected { revoked_session } => {
                assert!(revoked_session.is_revoked_at(now() + Duration::seconds(2)));
            }
            RefreshRotationOutcome::Rotated(_) => panic!("old token rotated twice"),
        }
    }

    #[test]
    fn recovery_codes_are_hashed_and_one_time() {
        let service = PasswordService::default();
        let batch = RecoveryCodeSet::issue("principal-1", 3, &service, now()).expect("codes");
        let first = batch.code(0).expect("first code").to_owned();
        let (consumed, _) = batch
            .set()
            .consume(&first, now(), &service)
            .expect("first consumption");
        assert!(matches!(
            consumed.consume(&first, now(), &service),
            Err(IdentityError::RecoveryCodeAlreadyConsumed)
        ));
        assert!(
            consumed
                .records()
                .iter()
                .any(|record| record.consumed_at().is_some())
        );
        assert!(!consumed.records()[0].hash().as_str().contains(&first));
    }

    #[test]
    fn totp_ciphertext_roundtrips_and_key_separation_fails() {
        struct Keys(HashMap<String, [u8; 32]>);
        impl KeyResolver for Keys {
            fn resolve(&self, reference: &str) -> Result<[u8; 32], IdentityError> {
                self.0
                    .get(reference)
                    .copied()
                    .ok_or_else(|| IdentityError::KeyReferenceNotFound(reference.to_owned()))
            }
        }

        let resolver = Keys(HashMap::from([
            ("key-a".to_owned(), [7; 32]),
            ("key-b".to_owned(), [8; 32]),
        ]));
        let envelope = encrypt_totp_secret(&resolver, "key-a", "principal-1", b"BASE32SECRET")
            .expect("encrypt");
        assert!(!envelope.ciphertext().is_empty());
        assert_eq!(
            decrypt_totp_secret(&resolver, "principal-1", &envelope).expect("decrypt"),
            SecretBytes::from_slice(b"BASE32SECRET")
        );
        assert!(matches!(
            decrypt_totp_secret(&resolver, "principal-2", &envelope),
            Err(IdentityError::TotpDecryptionFailed)
        ));
    }

    #[test]
    fn webauthn_challenges_expire_and_cannot_be_replayed() {
        let (challenge, material) = WebAuthnChallenge::issue(
            "principal-1",
            ChallengeCeremony::Assertion,
            now(),
            ChallengeLifetime::try_new(Duration::minutes(5)).expect("lifetime"),
        )
        .expect("challenge");
        let consumed = challenge
            .consume(material.as_bytes(), now() + Duration::seconds(1))
            .expect("first use");
        assert!(matches!(
            consumed.consume(material.as_bytes(), now() + Duration::seconds(2)),
            Err(IdentityError::ChallengeReplay)
        ));

        let (expired, expired_material) = WebAuthnChallenge::issue(
            "principal-1",
            ChallengeCeremony::Registration,
            now(),
            ChallengeLifetime::try_new(Duration::minutes(5)).expect("lifetime"),
        )
        .expect("challenge");
        assert!(matches!(
            expired.consume(expired_material.as_bytes(), now() + Duration::minutes(5)),
            Err(IdentityError::ChallengeExpired)
        ));
    }
}
