use chrono::{Duration, TimeZone, Utc};
use erpx_domain::{
    access::{RiskTier, RoleAssignment},
    contracts::AuthorityScope,
};
use erpx_identity::{
    AccessTokenLifetime, AccessTokenSigner, ActiveRoleClaim, AssertionRequest,
    AssertionVerification, AuthenticatedAuthority, AuthenticatedPrincipalKind, AuthenticatedRealm,
    ConfiguredWebAuthnProvider, DirectoryConnector, DirectoryFeedKind,
    DirectoryIdentityReplacementDecision, DirectoryIdentityReplacementDecisionAction,
    DirectoryProvider, DirectoryScope, DirectoryTlsMode, DirectoryTlsPolicy,
    IDENTITY_ACTIVE_ROLE_SESSION_CONTEXT_MIGRATION_SQL,
    IDENTITY_AUTHENTICATED_AUTHORITY_MIGRATION_SQL, IDENTITY_DEVELOPMENT_ACCESS_MIGRATION_SQL,
    IDENTITY_DEVELOPMENT_CONTEXT_LIFETIME_MIGRATION_SQL,
    IDENTITY_DIRECTORY_CAPABILITIES_MIGRATION_SQL,
    IDENTITY_DIRECTORY_PROFILE_LEGACY_DEFAULT_MIGRATION_SQL,
    IDENTITY_DIRECTORY_ROLE_TARGET_SCOPE_MIGRATION_SQL, IDENTITY_MIGRATION_SQL,
    IDENTITY_RECONCILIATION_MIGRATION_SQL, IDENTITY_REVOCATION_APPLIED_AT_MIGRATION_SQL,
    IDENTITY_ROLE_PROJECTION_CONSTRAINTS_MIGRATION_SQL,
    IDENTITY_SCOPED_DIRECTORY_ADMINISTRATION_MIGRATION_SQL,
    IDENTITY_SECURITY_COMMAND_AUDIT_MIGRATION_SQL, IDENTITY_SOURCE_PAYLOAD_MIGRATION_SQL,
    IdentityStoreError, PasskeyCeremonyService, PasskeyCredential, RateLimitDecision,
    RateLimitPolicy, RegistrationRequest, SecurityCommandContext, SqlxDirectoryStore,
    SqlxIdentityStore, TotpKeyResolver, VerifiedRegistration, WebAuthnProvider,
    WebAuthnProviderConfig, apply_identity_migration, directory_identity_replacement_decision_ref,
};
use erpx_kernel::{
    ActorAttribution as KernelActorAttribution, ActorKind as KernelActorKind,
    AuthorityScope as KernelAuthorityScope, HumanDecisionProof, SessionId,
};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy)]
struct PersistedPasskeyProvider {
    assertion_count: u32,
}

impl WebAuthnProvider for PersistedPasskeyProvider {
    fn verify_registration(
        &self,
        request: &RegistrationRequest,
    ) -> Result<VerifiedRegistration, erpx_identity::IdentityError> {
        Ok(VerifiedRegistration {
            credential_id: request.credential_id.clone(),
            public_key: request.public_key.clone(),
            sign_count: request.sign_count,
        })
    }

    fn verify_assertion(
        &self,
        _credential: &PasskeyCredential,
        request: &AssertionRequest,
    ) -> Result<AssertionVerification, erpx_identity::IdentityError> {
        Ok(AssertionVerification {
            credential_id: request.credential_id.clone(),
            sign_count: self.assertion_count,
        })
    }
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL databases"]
async fn identity_schema_is_write_side_only() {
    let write_url = std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required");
    assert_ne!(write_url, read_url);
    let write = sqlx::PgPool::connect(&write_url).await.expect("write pool");
    let read = sqlx::PgPool::connect(&read_url).await.expect("read pool");
    apply_identity_migration(&write)
        .await
        .expect("identity migration");
    let write_schema: Option<String> = sqlx::query_scalar(
        "SELECT schema_name FROM information_schema.schemata WHERE schema_name = 'erpx_identity'",
    )
    .fetch_one(&write)
    .await
    .expect("write schema query");
    let read_schema: Option<String> = sqlx::query_scalar(
        "SELECT schema_name FROM information_schema.schemata WHERE schema_name = 'erpx_identity'",
    )
    .fetch_optional(&read)
    .await
    .expect("read schema query");
    assert_eq!(write_schema.as_deref(), Some("erpx_identity"));
    assert_eq!(read_schema, None);
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database"]
async fn identity_store_persists_and_revokes_sessions() {
    let write_url = std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required");
    let write = sqlx::PgPool::connect(&write_url).await.expect("write pool");
    apply_identity_migration(&write)
        .await
        .expect("identity migration");
    let store = SqlxIdentityStore::new(write.clone());
    let at = Utc
        .with_ymd_and_hms(2026, 7, 24, 12, 0, 0)
        .single()
        .expect("fixed time");
    store
        .create_principal_with_password("principal-1", "correct horse", at)
        .await
        .expect("principal");
    let signer = AccessTokenSigner::new(b"01234567890123456789012345678901").expect("signer");
    let issued = store
        .login(&signer, "principal-1", "correct horse", "test-device", at)
        .await
        .expect("login");
    assert_eq!(
        issued.access_token().claims().authority(),
        AuthenticatedAuthority::legacy_human()
    );
    store
        .authorize_access_token(
            &signer,
            issued.access_token().token(),
            at + Duration::seconds(1),
        )
        .await
        .expect("active access token");

    let old_refresh = issued.refresh_token().as_str().to_owned();
    let rotated = store
        .refresh(
            &signer,
            issued.record().id(),
            &old_refresh,
            at + Duration::seconds(2),
        )
        .await
        .expect("refresh rotation");
    assert_ne!(rotated.refresh_token().as_str(), old_refresh);
    let reuse = store
        .refresh(
            &signer,
            issued.record().id(),
            &old_refresh,
            at + Duration::seconds(3),
        )
        .await
        .expect_err("reuse must revoke the session");
    assert!(matches!(
        reuse,
        IdentityStoreError::Primitive(erpx_identity::IdentityError::RefreshTokenReuseDetected)
    ));
    let revoked = store
        .authorize_access_token(
            &signer,
            rotated.access_token().token(),
            at + Duration::seconds(4),
        )
        .await
        .expect_err("reuse revokes the entire session family");
    assert!(matches!(revoked, IdentityStoreError::SessionInactive));
}

#[tokio::test]
#[ignore = "requires ERPX_TEST_IDENTITY_UPGRADE_DATABASE_URL pointing to a dedicated disposable PostgreSQL database; drops erpx_identity"]
async fn authenticated_authority_upgrade_revokes_legacy_sessions_that_change_authority() {
    let write_url = std::env::var("ERPX_TEST_IDENTITY_UPGRADE_DATABASE_URL")
        .expect("ERPX_TEST_IDENTITY_UPGRADE_DATABASE_URL is required");
    let write = sqlx::PgPool::connect(&write_url).await.expect("write pool");
    let database_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&write)
        .await
        .expect("upgrade test database name");
    assert!(
        database_name.ends_with("_identity_upgrade_test"),
        "identity upgrade acceptance refuses to drop a database schema unless the database name ends with _identity_upgrade_test"
    );
    sqlx::raw_sql("DROP SCHEMA IF EXISTS erpx_identity CASCADE")
        .execute(&write)
        .await
        .expect("reset dedicated identity upgrade schema");

    let mut migration = write.begin().await.expect("migration transaction");
    for sql in [
        IDENTITY_MIGRATION_SQL,
        IDENTITY_RECONCILIATION_MIGRATION_SQL,
        IDENTITY_SOURCE_PAYLOAD_MIGRATION_SQL,
        IDENTITY_REVOCATION_APPLIED_AT_MIGRATION_SQL,
        IDENTITY_ROLE_PROJECTION_CONSTRAINTS_MIGRATION_SQL,
        IDENTITY_DIRECTORY_CAPABILITIES_MIGRATION_SQL,
        IDENTITY_SCOPED_DIRECTORY_ADMINISTRATION_MIGRATION_SQL,
        IDENTITY_DIRECTORY_PROFILE_LEGACY_DEFAULT_MIGRATION_SQL,
        IDENTITY_DEVELOPMENT_ACCESS_MIGRATION_SQL,
        IDENTITY_DIRECTORY_ROLE_TARGET_SCOPE_MIGRATION_SQL,
        IDENTITY_ACTIVE_ROLE_SESSION_CONTEXT_MIGRATION_SQL,
        IDENTITY_DEVELOPMENT_CONTEXT_LIFETIME_MIGRATION_SQL,
        IDENTITY_SECURITY_COMMAND_AUDIT_MIGRATION_SQL,
    ] {
        sqlx::raw_sql(sql)
            .execute(&mut *migration)
            .await
            .expect("pre-authority identity migration");
    }
    migration.commit().await.expect("pre-authority schema");

    let issued_at = Utc::now() - Duration::minutes(10);
    let platform_root_id = uuid::Uuid::new_v4();
    let controller_id = uuid::Uuid::new_v4();
    let workforce_id = uuid::Uuid::new_v4();
    for (principal_id, principal_ref) in [
        (platform_root_id, "local:platform-root"),
        (controller_id, "local:dev-controller"),
        (workforce_id, "holding:legacy-workforce"),
    ] {
        sqlx::query(
            "INSERT INTO erpx_identity.principals
                (id, principal_ref, status, created_at, updated_at)
             VALUES ($1,$2,'active',$3,$3)",
        )
        .bind(principal_id)
        .bind(principal_ref)
        .bind(issued_at)
        .execute(&write)
        .await
        .expect("legacy principal");
    }
    sqlx::query(
        "INSERT INTO erpx_identity.protected_local_principals
            (principal_id, protection_kind, designated_by, designated_at)
         VALUES ($1,'platform_root','test:legacy-upgrade',$2)",
    )
    .bind(platform_root_id)
    .bind(issued_at)
    .execute(&write)
    .await
    .expect("legacy protected Platform Root");

    let platform_root_session_id = uuid::Uuid::new_v4();
    let controller_session_id = uuid::Uuid::new_v4();
    let workforce_session_id = uuid::Uuid::new_v4();
    for (principal_id, session_id, refresh_token) in [
        (
            platform_root_id,
            platform_root_session_id,
            "legacy-platform-root-refresh",
        ),
        (
            controller_id,
            controller_session_id,
            "legacy-controller-refresh",
        ),
        (
            workforce_id,
            workforce_session_id,
            "legacy-workforce-refresh",
        ),
    ] {
        let device_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO erpx_identity.devices
                (id, principal_id, label, created_at, last_seen_at)
             VALUES ($1,$2,'legacy-upgrade-device',$3,$3)",
        )
        .bind(device_id)
        .bind(principal_id)
        .bind(issued_at)
        .execute(&write)
        .await
        .expect("legacy device");
        sqlx::query(
            "INSERT INTO erpx_identity.sessions
                (id, principal_id, device_id, refresh_token_hash, issued_at,
                 refresh_expires_at, revoked_at, last_used_at)
             VALUES ($1,$2,$3,$4,$5,$6,NULL,$5)",
        )
        .bind(session_id)
        .bind(principal_id)
        .bind(device_id)
        .bind(Sha256::digest(refresh_token.as_bytes()).to_vec())
        .bind(issued_at)
        .bind(issued_at + Duration::days(30))
        .execute(&write)
        .await
        .expect("legacy session");
    }

    sqlx::raw_sql(IDENTITY_AUTHENTICATED_AUTHORITY_MIGRATION_SQL)
        .execute(&write)
        .await
        .expect("authenticated-authority upgrade");

    for session_id in [platform_root_session_id, controller_session_id] {
        let (realm, kind, revoked_at, last_used_at): (
            String,
            String,
            Option<chrono::DateTime<Utc>>,
            chrono::DateTime<Utc>,
        ) = sqlx::query_as(
            "SELECT authenticated_realm, principal_kind, revoked_at, last_used_at
             FROM erpx_identity.sessions WHERE id=$1",
        )
        .bind(session_id)
        .fetch_one(&write)
        .await
        .expect("elevated legacy session");
        assert_eq!(realm, "platform_administration");
        assert_eq!(kind, "human");
        let revoked_at = revoked_at.expect("changed authority session is fenced");
        assert!(last_used_at >= revoked_at);
    }

    let (realm, kind, revoked_at): (String, String, Option<chrono::DateTime<Utc>>) =
        sqlx::query_as(
            "SELECT authenticated_realm, principal_kind, revoked_at
             FROM erpx_identity.sessions WHERE id=$1",
        )
        .bind(workforce_session_id)
        .fetch_one(&write)
        .await
        .expect("unchanged legacy session");
    assert_eq!(realm, "internal_workforce");
    assert_eq!(kind, "human");
    assert!(revoked_at.is_none());

    let signer = AccessTokenSigner::new(b"01234567890123456789012345678901").expect("signer");
    let store = SqlxIdentityStore::new(write.clone());
    let root_refresh = store
        .refresh(
            &signer,
            platform_root_session_id,
            "legacy-platform-root-refresh",
            Utc::now(),
        )
        .await
        .expect_err("legacy Platform Root refresh must require reauthentication");
    assert!(matches!(
        root_refresh,
        IdentityStoreError::Primitive(erpx_identity::IdentityError::SessionInactive)
    ));
    let workforce_refresh = store
        .refresh(
            &signer,
            workforce_session_id,
            "legacy-workforce-refresh",
            Utc::now(),
        )
        .await
        .expect("unchanged workforce refresh remains valid");
    assert_eq!(
        workforce_refresh.access_token().claims().authority(),
        AuthenticatedAuthority::legacy_human()
    );

    sqlx::raw_sql(IDENTITY_AUTHENTICATED_AUTHORITY_MIGRATION_SQL)
        .execute(&write)
        .await
        .expect("authenticated-authority migration is rerunnable");
    let workforce_revoked: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT revoked_at FROM erpx_identity.sessions WHERE id=$1")
            .bind(workforce_session_id)
            .fetch_one(&write)
            .await
            .expect("rerun workforce session");
    assert!(workforce_revoked.is_none());
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database"]
async fn non_human_authority_survives_login_role_selection_and_refresh() {
    let write_url = std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required");
    let write = sqlx::PgPool::connect(&write_url).await.expect("write pool");
    apply_identity_migration(&write)
        .await
        .expect("identity migration");
    let store = SqlxIdentityStore::new(write.clone());
    let at = Utc
        .with_ymd_and_hms(2026, 8, 4, 13, 0, 0)
        .single()
        .expect("fixed time");
    let signer = AccessTokenSigner::new(b"01234567890123456789012345678901").expect("signer");

    for principal_kind in [
        AuthenticatedPrincipalKind::Agent,
        AuthenticatedPrincipalKind::Service,
    ] {
        let authority = AuthenticatedAuthority {
            realm: AuthenticatedRealm::InternalWorkforce,
            principal_kind,
        };
        let principal = format!(
            "authority-{}-{}",
            match principal_kind {
                AuthenticatedPrincipalKind::Agent => "agent",
                AuthenticatedPrincipalKind::Service => "service",
                AuthenticatedPrincipalKind::Human => unreachable!(),
            },
            uuid::Uuid::new_v4()
        );
        store
            .create_principal_with_password_and_authority(
                &principal,
                "correct horse",
                authority,
                at,
            )
            .await
            .expect("non-human principal");
        let issued = store
            .login(&signer, &principal, "correct horse", "authority-device", at)
            .await
            .expect("non-human login");
        assert_eq!(issued.access_token().claims().authority(), authority);

        let assignment_id = uuid::Uuid::new_v4();
        let assignment = RoleAssignment {
            id: assignment_id,
            principal_ref: principal.clone(),
            role_key: "authority-test".into(),
            permissions: vec!["operations.read".into()],
            functional_domain: "operations".into(),
            authority_level: 1,
            approval_limit_minor: 0,
            scope: AuthorityScope::Platform,
            valid_from: at - Duration::minutes(1),
            valid_until: at + Duration::days(1),
            timezone: "Asia/Bangkok".into(),
            risk_tier: RiskTier::Standard,
            grant_reason: "authenticated authority propagation test".into(),
            ticket_reference: Some("test:authority-propagation".into()),
            grantor: "test:bootstrap".into(),
            approver: Some("test:approver".into()),
            renewal_reviewer: None,
            parent_assignment_id: None,
            assignment_version: 1,
        };
        sqlx::query(
            "INSERT INTO erpx_control.role_assignments
                (id, principal_ref, role_key, parent_assignment_id, valid_from,
                 valid_until, assignment_version, payload)
             VALUES ($1,$2,$3,NULL,$4,$5,1,$6)",
        )
        .bind(assignment_id)
        .bind(&principal)
        .bind(&assignment.role_key)
        .bind(assignment.valid_from)
        .bind(assignment.valid_until)
        .bind(serde_json::to_value(&assignment).expect("role payload"))
        .execute(&write)
        .await
        .expect("role assignment");
        sqlx::query(
            "INSERT INTO erpx_control.role_assignment_target_policies
                (assignment_id, policy_kind, environment_gate, configured_by, configured_at)
             VALUES ($1,'unrestricted',NULL,'test:bootstrap',$2)",
        )
        .bind(assignment_id)
        .bind(at)
        .execute(&write)
        .await
        .expect("role target policy");

        let selection_id = assignment_id.to_string();
        let (_, selected_token) = store
            .select_active_role(
                &signer,
                &principal,
                issued.record().id(),
                &selection_id,
                at + Duration::seconds(1),
            )
            .await
            .expect("role selection");
        assert_eq!(selected_token.claims().authority(), authority);
        store
            .authorize_access_token(&signer, selected_token.token(), at + Duration::seconds(2))
            .await
            .expect("selected token authority");

        let refreshed = store
            .refresh(
                &signer,
                issued.record().id(),
                issued.refresh_token().as_str(),
                at + Duration::seconds(2),
            )
            .await
            .expect("selected-role refresh");
        assert_eq!(refreshed.access_token().claims().authority(), authority);
        assert_eq!(
            refreshed
                .access_token()
                .claims()
                .active_role
                .as_ref()
                .map(|role| role.selection_id.as_str()),
            Some(selection_id.as_str())
        );
    }
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database"]
async fn directory_replacement_decision_requires_distinct_authenticated_sessions_before_mutation() {
    let write_url = std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required");
    let write = sqlx::PgPool::connect(&write_url).await.expect("write pool");
    apply_identity_migration(&write)
        .await
        .expect("identity migration");
    let identity = SqlxIdentityStore::new(write.clone());
    let directory = SqlxDirectoryStore::new(write.clone());
    let at = Utc
        .with_ymd_and_hms(2026, 8, 4, 14, 0, 0)
        .single()
        .expect("fixed time");
    let connector_id = uuid::Uuid::new_v4();
    let holding_id = uuid::Uuid::new_v4();
    directory
        .save_connector(
            &DirectoryConnector {
                id: connector_id,
                provider: DirectoryProvider::GenericLdap,
                scope: DirectoryScope::Holding {
                    data_profile_key: holding_id.to_string(),
                },
                endpoint: "ldaps://replacement.example.test:636".into(),
                base_dn: "dc=replacement,dc=example,dc=test".into(),
                tls: DirectoryTlsPolicy {
                    mode: DirectoryTlsMode::Ldaps,
                    ca_certificate_sha256: "a".repeat(64),
                    server_name: "replacement.example.test".into(),
                    minimum_tls_version: "1.3".into(),
                },
                feed: DirectoryFeedKind::Polling,
                credential_ref: "secret://directory/replacement".into(),
                credential_profile_key: "replacement-test".into(),
                parent_connector_id: None,
                directory_authority_fingerprint: None,
                fallback_to_holding: false,
                enabled: true,
                max_import_users: 10,
                revision: 1,
            },
            at,
        )
        .await
        .expect("connector");
    let local_ref = format!("replacement-local-{}", uuid::Uuid::new_v4());
    let principal_id = identity
        .create_principal_with_password(&local_ref, "correct horse", at)
        .await
        .expect("local principal");
    let conflict_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO erpx_identity.directory_identity_conflicts
            (id,connector_id,external_object_id,principal_id,conflict_kind,state,
             detected_at,match_reasons,conflict_version)
         VALUES ($1,$2,'replacement-object',$3,'local_identity_collision',
                 'unresolved',$4,ARRAY['canonical_principal'],1)",
    )
    .bind(conflict_id)
    .bind(connector_id)
    .bind(principal_id)
    .bind(at)
    .execute(&write)
    .await
    .expect("identity conflict");

    let proposer_session = SessionId::try_from_uuid(uuid::Uuid::new_v4()).expect("session");
    let proposer = KernelActorAttribution {
        principal_ref: "holding:replacement-proposer".into(),
        kind: KernelActorKind::HoldingUser,
        session_id: Some(proposer_session),
        represented_principal_ref: None,
        support_capability_id: None,
    };
    let proposed = directory
        .propose_identity_replacement(
            conflict_id,
            1,
            &proposer,
            uuid::Uuid::new_v4(),
            at + Duration::seconds(1),
        )
        .await
        .expect("proposal");
    assert_eq!(proposed.conflict_version, 2);

    let decision = |principal_ref: &str, session: SessionId, correlation_id: uuid::Uuid| {
        let scope = KernelAuthorityScope::Holding { holding_id };
        let proof = HumanDecisionProof::try_new(
            KernelActorAttribution {
                principal_ref: principal_ref.into(),
                kind: KernelActorKind::HoldingUser,
                session_id: Some(session),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope.clone(),
            directory_identity_replacement_decision_ref(
                DirectoryIdentityReplacementDecisionAction::Reject,
                conflict_id,
                2,
                correlation_id,
            ),
            at + Duration::seconds(2),
        )
        .expect("human proof");
        DirectoryIdentityReplacementDecision::try_new(
            DirectoryIdentityReplacementDecisionAction::Reject,
            conflict_id,
            2,
            correlation_id,
            scope,
            proof,
        )
        .expect("replacement decision")
    };

    let same_principal = decision(
        &proposer.principal_ref,
        SessionId::try_from_uuid(uuid::Uuid::new_v4()).expect("session"),
        uuid::Uuid::new_v4(),
    );
    assert!(
        directory
            .reject_identity_replacement(&same_principal)
            .await
            .is_err()
    );
    let same_session = decision(
        "platform:replacement-approver",
        proposer_session,
        uuid::Uuid::new_v4(),
    );
    assert!(
        directory
            .reject_identity_replacement(&same_session)
            .await
            .is_err()
    );
    let state: String = sqlx::query_scalar(
        "SELECT state FROM erpx_identity.directory_identity_replacement_approvals
         WHERE conflict_id=$1",
    )
    .bind(conflict_id)
    .fetch_one(&write)
    .await
    .expect("proposal state");
    assert_eq!(state, "awaiting_approval");
    let evidence_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM erpx_identity.directory_identity_replacement_decision_evidence
         WHERE conflict_id=$1",
    )
    .bind(conflict_id)
    .fetch_one(&write)
    .await
    .expect("decision evidence count");
    assert_eq!(evidence_before, 0);

    let approver_session = SessionId::try_from_uuid(uuid::Uuid::new_v4()).expect("session");
    let approved_correlation = uuid::Uuid::new_v4();
    let valid = decision(
        "platform:replacement-approver",
        approver_session,
        approved_correlation,
    );
    directory
        .reject_identity_replacement(&valid)
        .await
        .expect("independent human rejection");
    let evidence: (
        String,
        uuid::Uuid,
        String,
        String,
        uuid::Uuid,
        String,
        String,
    ) = sqlx::query_as(
        "SELECT initiator_ref,initiator_session_id,decision_actor_ref,
                decision_actor_kind,decision_session_id,authority_scope_key,decision_ref
         FROM erpx_identity.directory_identity_replacement_decision_evidence
         WHERE conflict_id=$1 AND expected_conflict_version=2",
    )
    .bind(conflict_id)
    .fetch_one(&write)
    .await
    .expect("bound decision evidence");
    assert_eq!(evidence.0, proposer.principal_ref);
    assert_eq!(evidence.1, proposer_session.as_uuid());
    assert_eq!(evidence.2, valid.principal_ref());
    assert_eq!(evidence.3, "human");
    assert_eq!(evidence.4, approver_session.as_uuid());
    assert_eq!(evidence.5, format!("holding:{holding_id}"));
    assert_eq!(evidence.6, valid.proof().decision_ref());
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database"]
async fn authorization_context_fence_invalidates_old_tokens_and_refresh_preserves_selection() {
    let write_url = std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required");
    let write = sqlx::PgPool::connect(&write_url).await.expect("write pool");
    apply_identity_migration(&write)
        .await
        .expect("identity migration");
    let store = SqlxIdentityStore::new(write.clone());
    let at = Utc
        .with_ymd_and_hms(2026, 8, 4, 12, 0, 0)
        .single()
        .expect("fixed time");
    let principal = format!("context-principal-{}", uuid::Uuid::new_v4());
    store
        .create_principal_with_password(&principal, "correct horse", at)
        .await
        .expect("principal");
    let signer = AccessTokenSigner::new(b"01234567890123456789012345678901").expect("signer");
    let issued = store
        .login(&signer, &principal, "correct horse", "context-device", at)
        .await
        .expect("login");
    assert_eq!(
        issued.access_token().claims().authorization_context_version,
        Some(1)
    );
    assert!(issued.access_token().claims().active_role.is_none());

    let selection_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "UPDATE erpx_identity.sessions
         SET active_role_source='local', active_role_selection_id=$2,
             authorization_context_version=2
         WHERE id=$1",
    )
    .bind(issued.record().id())
    .bind(&selection_id)
    .execute(&write)
    .await
    .expect("select role context");

    assert!(matches!(
        store
            .authorize_access_token(
                &signer,
                issued.access_token().token(),
                at + Duration::seconds(1),
            )
            .await,
        Err(IdentityStoreError::SessionInactive)
    ));
    let matching = signer
        .issue_with_context(
            &principal,
            issued.record().id(),
            Some(ActiveRoleClaim {
                source: "local".into(),
                selection_id: selection_id.clone(),
            }),
            Some(2),
            at + Duration::seconds(1),
            AccessTokenLifetime::five_minutes(),
        )
        .expect("matching selected-role token");
    store
        .authorize_access_token(&signer, matching.token(), at + Duration::seconds(2))
        .await
        .expect("matching authorization context");

    let rotated = store
        .refresh(
            &signer,
            issued.record().id(),
            issued.refresh_token().as_str(),
            at + Duration::seconds(2),
        )
        .await
        .expect("refresh selected session");
    assert_eq!(
        rotated.access_token().claims().active_role,
        Some(ActiveRoleClaim {
            source: "local".into(),
            selection_id,
        })
    );
    assert_eq!(
        rotated
            .access_token()
            .claims()
            .authorization_context_version,
        Some(2)
    );
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database"]
async fn identity_store_persists_one_time_recovery_codes_and_device_revocation() {
    let write_url = std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required");
    let write = sqlx::PgPool::connect(&write_url).await.expect("write pool");
    apply_identity_migration(&write)
        .await
        .expect("identity migration");
    let store = SqlxIdentityStore::new(write.clone());
    let at = Utc
        .with_ymd_and_hms(2026, 7, 24, 12, 0, 0)
        .single()
        .expect("fixed time");
    store
        .create_principal_with_password("recovery-principal", "correct horse", at)
        .await
        .expect("principal");
    let signer = AccessTokenSigner::new(b"01234567890123456789012345678901").expect("signer");
    let issued = store
        .login(
            &signer,
            "recovery-principal",
            "correct horse",
            "trusted-device",
            at,
        )
        .await
        .expect("login");
    let device_id = issued.record().device_id();
    let batch = store
        .issue_recovery_codes("recovery-principal", 2, at)
        .await
        .expect("recovery codes");
    let first_code = batch.code(0).expect("first recovery code").to_owned();
    let stored_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_identity.recovery_codes rc
         JOIN erpx_identity.principals p ON p.id = rc.principal_id
         WHERE p.principal_ref = 'recovery-principal' AND rc.consumed_at IS NULL",
    )
    .fetch_one(&write)
    .await
    .expect("stored recovery code count");
    assert_eq!(stored_count, 2);

    let (first, second) = tokio::join!(
        store.consume_recovery_code("recovery-principal", &first_code, at + Duration::seconds(1),),
        store.consume_recovery_code("recovery-principal", &first_code, at + Duration::seconds(2),),
    );
    let outcomes = [first, second];
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| {
                matches!(
                    outcome,
                    Err(IdentityStoreError::Primitive(
                        erpx_identity::IdentityError::RecoveryCodeAlreadyConsumed
                    ))
                )
            })
            .count(),
        1
    );

    let devices = store
        .list_devices("recovery-principal")
        .await
        .expect("device list");
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id(), device_id);
    assert_eq!(devices[0].label(), "trusted-device");

    assert!(
        store
            .revoke_device("recovery-principal", device_id, at + Duration::seconds(3),)
            .await
            .expect("device revocation")
    );
    assert!(matches!(
        store
            .authorize_access_token(
                &signer,
                issued.access_token().token(),
                at + Duration::seconds(4),
            )
            .await,
        Err(IdentityStoreError::SessionInactive)
    ));
    let revoked_device = store
        .list_devices("recovery-principal")
        .await
        .expect("revoked device list");
    assert!(revoked_device[0].revoked_at().is_some());
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database"]
async fn identity_rate_limit_is_durable_atomic_and_clears_without_storing_identifiers() {
    let write_url = std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required");
    let write = sqlx::PgPool::connect(&write_url).await.expect("write pool");
    apply_identity_migration(&write)
        .await
        .expect("identity migration");
    let store = SqlxIdentityStore::new(write.clone());
    let policy = RateLimitPolicy::new(2, 60, 120);
    let at = Utc
        .with_ymd_and_hms(2026, 7, 24, 12, 0, 0)
        .single()
        .expect("fixed time");

    assert_eq!(
        store
            .admit_rate_limit("login", "durable-principal", policy, at)
            .await
            .expect("first admission"),
        RateLimitDecision::Allowed
    );
    assert_eq!(
        store
            .admit_rate_limit(
                "login",
                "durable-principal",
                policy,
                at + Duration::seconds(1),
            )
            .await
            .expect("second admission"),
        RateLimitDecision::Allowed
    );
    assert_eq!(
        store
            .admit_rate_limit(
                "login",
                "durable-principal",
                policy,
                at + Duration::seconds(2),
            )
            .await
            .expect("limited admission"),
        RateLimitDecision::Limited
    );

    let (first, second) = tokio::join!(
        store.admit_rate_limit("recovery", "concurrent-principal", policy, at),
        store.admit_rate_limit(
            "recovery",
            "concurrent-principal",
            policy,
            at + Duration::milliseconds(1),
        ),
    );
    let concurrent = [first, second];
    assert_eq!(
        concurrent
            .iter()
            .filter(|result| matches!(result, Ok(RateLimitDecision::Allowed)))
            .count(),
        2
    );
    assert!(
        store
            .clear_rate_limit("login", "durable-principal")
            .await
            .is_ok()
    );
    assert_eq!(
        store
            .admit_rate_limit("login", "durable-principal", policy, at)
            .await
            .expect("admission after clear"),
        RateLimitDecision::Allowed
    );

    let stored_identifier: Option<String> = sqlx::query_scalar(
        "SELECT encode(bucket_digest, 'escape')
         FROM erpx_identity.auth_rate_limits
         LIMIT 1",
    )
    .fetch_optional(&write)
    .await
    .expect("rate-limit bucket query");
    assert!(
        !stored_identifier
            .as_deref()
            .unwrap_or_default()
            .contains("durable-principal")
    );
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database"]
async fn identity_passkeys_persist_bind_and_update_counters_atomically() {
    let write_url = std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required");
    let write = sqlx::PgPool::connect(&write_url).await.expect("write pool");
    apply_identity_migration(&write)
        .await
        .expect("identity migration");
    let store = SqlxIdentityStore::new(write.clone());
    let at = Utc
        .with_ymd_and_hms(2026, 7, 24, 12, 0, 0)
        .single()
        .expect("fixed time");
    store
        .create_principal_with_password("passkey-principal", "correct horse", at)
        .await
        .expect("principal");

    let provider = ConfiguredWebAuthnProvider::try_new(
        PersistedPasskeyProvider { assertion_count: 1 },
        WebAuthnProviderConfig::try_new("example.com", ["https://app.example.com"])
            .expect("provider config"),
    )
    .expect("configured provider");
    let service = PasskeyCeremonyService::new(provider, store.clone());
    let registration = service
        .begin_registration("passkey-principal", at)
        .await
        .expect("registration challenge");
    let request = RegistrationRequest {
        rp_id: "example.com".into(),
        origin: "https://app.example.com".into(),
        credential_id: vec![11, 12, 13],
        public_key: vec![21, 22, 23],
        sign_count: 1,
    };
    let credential = service
        .finish_registration(
            "passkey-principal",
            registration.challenge().id,
            registration.material().as_bytes(),
            &request,
            at + Duration::seconds(1),
        )
        .await
        .expect("provider-verified credential");
    assert!(matches!(
        service
            .finish_registration(
                "passkey-principal",
                registration.challenge().id,
                registration.material().as_bytes(),
                &request,
                at + Duration::seconds(2),
            )
            .await,
        Err(IdentityStoreError::Primitive(
            erpx_identity::IdentityError::ChallengeReplay
        ))
    ));
    let stored: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_identity.passkeys
         WHERE credential_id = $1 AND credential_id_hash IS NOT NULL
           AND octet_length(credential_id_hash) = 32 AND rp_id = $2",
    )
    .bind(credential.credential_id())
    .bind(credential.rp_id())
    .fetch_one(&write)
    .await
    .expect("stored passkey binding");
    assert_eq!(stored, 1);
    assert!(
        store
            .find_passkey("other-principal", credential.credential_id())
            .await
            .expect("cross-principal lookup")
            .is_none()
    );
    let found = store
        .find_passkey("passkey-principal", credential.credential_id())
        .await
        .expect("lookup")
        .expect("stored credential");
    assert_eq!(found.rp_id(), "example.com");
    assert_eq!(found.sign_count(), 1);

    let authentication = service
        .begin_authentication("passkey-principal", at + Duration::seconds(2))
        .await
        .expect("authentication challenge");
    let assertion = AssertionRequest {
        rp_id: "example.com".into(),
        origin: "https://app.example.com".into(),
        credential_id: credential.credential_id().to_vec(),
        sign_count: 1,
    };
    let authenticated = service
        .finish_authentication(
            "passkey-principal",
            authentication.challenge().id,
            authentication.material().as_bytes(),
            &assertion,
            at + Duration::seconds(3),
        )
        .await
        .expect("durable authentication");
    assert_eq!(authenticated.sign_count(), 1);

    let equal = store
        .compare_and_set_passkey_counter(
            "passkey-principal",
            credential.credential_id(),
            1,
            1,
            at + Duration::seconds(4),
        )
        .await
        .expect("equal counter");
    assert_eq!(equal.sign_count(), 1);
    let advanced = store
        .compare_and_set_passkey_counter(
            "passkey-principal",
            credential.credential_id(),
            1,
            2,
            at + Duration::seconds(5),
        )
        .await
        .expect("advanced counter");
    assert_eq!(advanced.sign_count(), 2);
    assert!(matches!(
        store
            .compare_and_set_passkey_counter(
                "passkey-principal",
                credential.credential_id(),
                2,
                1,
                at + Duration::seconds(6),
            )
            .await,
        Err(IdentityStoreError::Primitive(
            erpx_identity::IdentityError::CredentialCounterRollback
        ))
    ));

    let (first, second) = tokio::join!(
        store.compare_and_set_passkey_counter(
            "passkey-principal",
            credential.credential_id(),
            2,
            3,
            at + Duration::seconds(7),
        ),
        store.compare_and_set_passkey_counter(
            "passkey-principal",
            credential.credential_id(),
            2,
            4,
            at + Duration::seconds(8),
        ),
    );
    let concurrent = [first, second];
    assert_eq!(concurrent.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        concurrent
            .iter()
            .filter(|result| {
                matches!(
                    result,
                    Err(IdentityStoreError::Primitive(
                        erpx_identity::IdentityError::CredentialCounterRollback
                    ))
                )
            })
            .count(),
        1
    );

    assert!(
        store
            .revoke_passkey("passkey-principal", credential.credential_id(), at)
            .await
            .expect("revoke passkey")
    );
    assert!(
        store
            .find_passkey("passkey-principal", credential.credential_id())
            .await
            .expect("lookup revoked passkey")
            .expect("revoked credential")
            .revoked_at()
            .is_some()
    );
    assert!(matches!(
        store
            .compare_and_set_passkey_counter(
                "passkey-principal",
                credential.credential_id(),
                3,
                4,
                at + Duration::seconds(9),
            )
            .await,
        Err(IdentityStoreError::PasskeyRevoked)
            | Err(IdentityStoreError::Primitive(
                erpx_identity::IdentityError::CredentialCounterRollback
            ))
    ));
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database"]
async fn identity_totp_and_recovery_handoffs_are_encrypted_bounded_and_one_time() {
    let write_url = std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required");
    let write = sqlx::PgPool::connect(&write_url).await.expect("write pool");
    apply_identity_migration(&write)
        .await
        .expect("identity migration");
    let store = SqlxIdentityStore::new(write.clone());
    let at = Utc
        .with_ymd_and_hms(2026, 7, 24, 12, 0, 0)
        .single()
        .expect("fixed time");
    store
        .create_principal_with_password("totp-principal", "correct horse", at)
        .await
        .expect("principal");
    let keys = TotpKeyResolver::from_bytes("test-totp-key", [7; 32]).expect("key resolver");
    let enrollment = store
        .begin_totp_enrollment(
            "totp-principal",
            &keys,
            "test-totp-key",
            "ERPX",
            "totp-principal",
            at,
            Duration::minutes(10),
        )
        .await
        .expect("totp enrollment");
    let secret = decode_base32(enrollment.secret().as_str());
    let code = totp_code(&secret, at);
    store
        .verify_totp_enrollment(
            "totp-principal",
            enrollment.id(),
            &code,
            &keys,
            at + Duration::seconds(1),
        )
        .await
        .expect("verify enrollment");
    let ciphertext: Vec<u8> = sqlx::query_scalar(
        "SELECT ciphertext FROM erpx_identity.totp_secrets t
         JOIN erpx_identity.principals p ON p.id = t.principal_id
         WHERE p.principal_ref = 'totp-principal'",
    )
    .fetch_one(&write)
    .await
    .expect("totp ciphertext");
    assert!(!ciphertext.is_empty());
    assert!(
        !ciphertext
            .windows(secret.len())
            .any(|window| window == secret)
    );

    let next_at = at + Duration::seconds(30);
    store
        .verify_totp(
            "totp-principal",
            &totp_code(&secret, next_at),
            &keys,
            next_at,
        )
        .await
        .expect("verify next totp step");
    assert!(matches!(
        store
            .verify_totp(
                "totp-principal",
                &totp_code(&secret, next_at),
                &keys,
                next_at
            )
            .await,
        Err(IdentityStoreError::Primitive(
            erpx_identity::IdentityError::TotpReplay
        ))
    ));

    let recovery_codes = store
        .issue_recovery_codes("totp-principal", 1, at)
        .await
        .expect("recovery codes");
    let recovery_code = recovery_codes.code(0).expect("recovery code").to_owned();
    let recovery = store
        .initiate_recovery_session(
            "totp-principal",
            &recovery_code,
            at + Duration::seconds(2),
            Duration::minutes(10),
        )
        .await
        .expect("recovery handoff");
    let recovery_token = recovery.token().as_str().to_owned();
    let signer = AccessTokenSigner::new(b"01234567890123456789012345678901").expect("signer");
    store
        .reset_password_with_recovery_session(
            &signer,
            &recovery_token,
            "new recovery password",
            "recovery-device",
            at + Duration::seconds(3),
        )
        .await
        .expect("reset password through recovery handoff");
    assert!(
        store
            .login(
                &signer,
                "totp-principal",
                "correct horse",
                "old-password-device",
                at + Duration::seconds(4),
            )
            .await
            .is_err()
    );
    store
        .login(
            &signer,
            "totp-principal",
            "new recovery password",
            "new-password-device",
            at + Duration::seconds(4),
        )
        .await
        .expect("new password verifies");
    assert!(matches!(
        store
            .reset_password_with_recovery_session(
                &signer,
                &recovery_token,
                "another password",
                "recovery-device-2",
                at + Duration::seconds(5),
            )
            .await,
        Err(IdentityStoreError::Primitive(
            erpx_identity::IdentityError::RecoverySessionInvalid
        ))
    ));
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database"]
async fn identity_security_commands_are_atomic_audited_and_replay_safe() {
    let write_url = std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required");
    let write = sqlx::PgPool::connect(&write_url).await.expect("write pool");
    apply_identity_migration(&write)
        .await
        .expect("identity migration");
    let store = SqlxIdentityStore::new(write.clone());
    let at = Utc
        .with_ymd_and_hms(2026, 8, 21, 9, 0, 0)
        .single()
        .expect("fixed time");
    store
        .create_principal_with_password("security-principal", "correct horse", at)
        .await
        .expect("principal");
    let keys =
        TotpKeyResolver::from_bytes("security-test-key", [11; 32]).expect("TOTP key resolver");
    let enrollment = store
        .begin_totp_enrollment(
            "security-principal",
            &keys,
            "security-test-key",
            "ERPX",
            "security-principal",
            at,
            Duration::minutes(10),
        )
        .await
        .expect("TOTP enrollment");
    let secret = decode_base32(enrollment.secret().as_str());
    store
        .verify_totp_enrollment(
            "security-principal",
            enrollment.id(),
            &totp_code(&secret, at),
            &keys,
            at + Duration::seconds(1),
        )
        .await
        .expect("verify enrollment");
    let before = store
        .security_factor_state("security-principal", at + Duration::seconds(2))
        .await
        .expect("factor state");
    assert!(before.totp_enabled());
    assert_eq!(before.recovery_codes_total(), 0);

    let rotate_command_id = uuid::Uuid::new_v4();
    let rotate_audit_reference = uuid::Uuid::new_v4();
    let rotate = SecurityCommandContext::new(
        rotate_command_id,
        rotate_audit_reference,
        "security-rotate-001".into(),
        [1; 32],
        "rotate recovery codes".into(),
        "en".into(),
    )
    .expect("rotate context");
    let rotate_at = at + Duration::seconds(30);
    let first = store
        .rotate_recovery_codes_with_totp(
            "security-principal",
            8,
            &totp_code(&secret, rotate_at),
            &keys,
            &rotate,
            rotate_at,
        )
        .await
        .expect("rotate recovery codes");
    assert!(!first.replayed());
    assert_eq!(
        first.batch().expect("one-time batch").set().records().len(),
        8
    );
    let replay = store
        .rotate_recovery_codes_with_totp(
            "security-principal",
            8,
            &totp_code(&secret, rotate_at),
            &keys,
            &rotate,
            rotate_at + Duration::seconds(1),
        )
        .await
        .expect("exact replay");
    assert!(replay.replayed());
    assert!(replay.batch().is_none());
    assert_eq!(replay.occurred_at(), rotate_at);

    let conflicting_rotate = SecurityCommandContext::new(
        rotate_command_id,
        rotate_audit_reference,
        "security-rotate-001".into(),
        [2; 32],
        "rotate recovery codes".into(),
        "en".into(),
    )
    .expect("conflicting context");
    assert!(matches!(
        store
            .rotate_recovery_codes_with_totp(
                "security-principal",
                8,
                &totp_code(&secret, rotate_at),
                &keys,
                &conflicting_rotate,
                rotate_at + Duration::seconds(2),
            )
            .await,
        Err(IdentityStoreError::SecurityCommandConflict)
    ));

    let remove = SecurityCommandContext::new(
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
        "security-remove-001".into(),
        [3; 32],
        "remove TOTP".into(),
        "th".into(),
    )
    .expect("remove context");
    let remove_at = at + Duration::seconds(60);
    let removed = store
        .remove_totp_with_proof(
            "security-principal",
            &totp_code(&secret, remove_at),
            &keys,
            &remove,
            remove_at,
        )
        .await
        .expect("remove TOTP");
    assert!(!removed.replayed());
    let remove_replay = store
        .remove_totp_with_proof(
            "security-principal",
            &totp_code(&secret, remove_at),
            &keys,
            &remove,
            remove_at + Duration::seconds(1),
        )
        .await
        .expect("replay removal after factor deletion");
    assert!(remove_replay.replayed());
    let after = store
        .security_factor_state("security-principal", remove_at + Duration::seconds(2))
        .await
        .expect("refetched factor state");
    assert!(!after.totp_enabled());
    assert_eq!(after.recovery_codes_total(), 8);
    assert_eq!(after.recovery_codes_remaining(), 8);

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_identity.security_command_audit audit
         JOIN erpx_identity.principals principal ON principal.id=audit.principal_id
         WHERE principal.principal_ref='security-principal'",
    )
    .fetch_one(&write)
    .await
    .expect("audit count");
    assert_eq!(audit_count, 2);
    assert!(
        sqlx::query(
            "UPDATE erpx_identity.security_command_audit
             SET purpose='mutated' WHERE audit_reference=$1",
        )
        .bind(rotate_audit_reference)
        .execute(&write)
        .await
        .is_err(),
        "security audit must remain append-only"
    );
}

fn decode_base32(encoded: &str) -> Vec<u8> {
    let mut buffer = 0_u32;
    let mut bits = 0_u8;
    let mut output = Vec::new();
    for byte in encoded.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'2'..=b'7' => byte - b'2' + 26,
            _ => panic!("invalid base32"),
        };
        buffer = (buffer << 5) | u32::from(value);
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    output
}

fn totp_code(secret: &[u8], at: chrono::DateTime<Utc>) -> String {
    let step = at.timestamp().div_euclid(30);
    let mut counter = [0_u8; 8];
    counter.copy_from_slice(&step.to_be_bytes());
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret).expect("hmac key");
    mac.update(&counter);
    let digest = mac.finalize().into_bytes();
    let offset = usize::from(digest[31] & 0x0f);
    let binary = (u32::from(digest[offset]) & 0x7f) << 24
        | u32::from(digest[offset + 1]) << 16
        | u32::from(digest[offset + 2]) << 8
        | u32::from(digest[offset + 3]);
    format!("{:06}", binary % 1_000_000)
}
