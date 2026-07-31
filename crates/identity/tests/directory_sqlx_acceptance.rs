//! Local, bounded SQLx evidence for the G13/G14 directory slice.
//!
//! This target is deliberately independent of external directory credentials.
//! It exercises the identity-owned PostgreSQL write paths with deterministic
//! pages and changes.  A run with no local database reference is a queued
//! blocker, not a vendor-certification result.

use std::{
    collections::{BTreeSet, VecDeque},
    env,
};

use chrono::{DateTime, Duration, TimeZone, Utc};
use erpx_identity::directory::feeds::{
    ChangeFeedRequest, DirectoryFeed, FeedCursor, FeedError, FeedFuture, FeedKind,
    FullImportPageRequest,
};
use erpx_identity::directory::rbac::{DirectoryRoleIdentity, SqlxDirectoryRbacStore};
use erpx_identity::directory::sync::{
    ChangeKind, ConnectorScope, DirectoryChange, DirectoryChangeBatch,
    DirectoryConnector as SyncConnector, DirectoryPage, DirectorySyncStore,
    DirectoryUserRecord as SyncUserRecord, FeedVendor, FullImportRunner, ImmutableIdentityLink,
    ImportConfig, ImportPlan, SqlxDirectorySyncStore, SyncStoreError, TransportSecurity,
};
use erpx_identity::{
    AccessTokenSigner, DirectoryChange as LegacyDirectoryChange, DirectoryConnector,
    DirectoryError, DirectoryFeedKind, DirectoryProvider, DirectoryRoleAccess,
    DirectoryRoleMapping, DirectoryScope, DirectorySyncCursor, DirectorySyncPage, DirectoryTlsMode,
    DirectoryTlsPolicy, DirectoryUser, PasswordService, SqlxDirectoryStore, SqlxIdentityStore,
    apply_identity_migration, immutable_identity_link,
};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

const LOCAL_DATABASE_ENV: &str = "ERPX_G14_LOCAL_DIRECTORY_DATABASE_URL";
const LOCAL_MAX_USERS: u64 = 4;
const CERTIFICATION_MAX_USERS: u64 = 1_000_000;
const FIXED_AT: (i32, u32, u32, u32, u32, u32) = (2026, 7, 26, 12, 0, 0);

#[derive(Debug, sqlx::FromRow)]
struct NativeRevocationRow {
    external_id: String,
    observed_at: DateTime<Utc>,
    deadline: DateTime<Utc>,
    status: String,
    applied_at: Option<DateTime<Utc>>,
}

fn fixed_at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(
        FIXED_AT.0, FIXED_AT.1, FIXED_AT.2, FIXED_AT.3, FIXED_AT.4, FIXED_AT.5,
    )
    .single()
    .expect("fixed SQLx acceptance timestamp")
}

fn local_database_url() -> Option<String> {
    env::var(LOCAL_DATABASE_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty() && !value.chars().any(char::is_control))
}

fn tls(mode: DirectoryTlsMode, server_name: &str) -> DirectoryTlsPolicy {
    DirectoryTlsPolicy {
        mode,
        ca_certificate_sha256: "a".repeat(64),
        server_name: server_name.into(),
        minimum_tls_version: "1.2".into(),
    }
}

fn root_connector(
    id: Uuid,
    provider: DirectoryProvider,
    feed: DirectoryFeedKind,
    endpoint: &str,
    tls_mode: DirectoryTlsMode,
    server_name: &str,
) -> DirectoryConnector {
    DirectoryConnector {
        id,
        provider,
        scope: DirectoryScope::Holding {
            holding_ref: format!("g14-local-{}", id.simple()),
        },
        endpoint: endpoint.into(),
        base_dn: "dc=example,dc=test".into(),
        tls: tls(tls_mode, server_name),
        feed,
        credential_ref: "test-only:directory-bind-secret".into(),
        enabled: true,
        max_import_users: 1_000_000,
        revision: 1,
    }
}

fn make_sync_connector(
    id: Uuid,
    vendor: FeedVendor,
    feed: FeedKind,
    scope_id: Uuid,
    endpoint: &str,
    security: TransportSecurity,
) -> SyncConnector {
    SyncConnector::new(
        id,
        vendor,
        feed,
        ConnectorScope::Holding(scope_id),
        endpoint,
        security,
        2,
        2,
    )
    .expect("bounded TLS-only sync connector")
}

fn sync_record(
    external_id: &str,
    authority: &str,
    value: &str,
    state: erpx_identity::directory::sync::DirectoryUserState,
) -> SyncUserRecord {
    sync_record_with_groups(external_id, authority, value, state, [])
}

fn sync_record_with_groups(
    external_id: &str,
    authority: &str,
    value: &str,
    state: erpx_identity::directory::sync::DirectoryUserState,
    groups: impl IntoIterator<Item = &'static str>,
) -> SyncUserRecord {
    SyncUserRecord::builder(
        external_id,
        ImmutableIdentityLink::new(authority, value).expect("immutable link"),
    )
    .state(state)
    .groups(groups)
    .login_name(format!("{external_id}@example.test"))
    .email(format!("{external_id}@example.test"))
    .display_name(external_id)
    .build()
    .expect("valid directory record")
}

#[derive(Debug)]
struct LocalImportFeed {
    connector: SyncConnector,
    pages: VecDeque<DirectoryPage>,
}

impl DirectoryFeed for LocalImportFeed {
    fn connector(&self) -> &SyncConnector {
        &self.connector
    }

    fn preflight<'a>(&'a mut self) -> FeedFuture<'a, ()> {
        // This feed is an explicit local SQLx fixture, not an external
        // directory adapter. The production native adapters retain the
        // fail-closed default and must provide external certification.
        Box::pin(async { Ok(()) })
    }

    fn full_import_page<'a>(
        &'a mut self,
        request: FullImportPageRequest,
    ) -> FeedFuture<'a, DirectoryPage> {
        Box::pin(async move {
            if request.connector_id() != self.connector.id() {
                return Err(FeedError::InvalidCursor("connector mismatch".into()));
            }
            let page = self.pages.pop_front().ok_or_else(|| {
                FeedError::ExternalUnavailable("local page source exhausted".into())
            })?;
            request.validate_page_len(page.records().len())?;
            Ok(page)
        })
    }

    fn change_batch<'a>(
        &'a mut self,
        _request: ChangeFeedRequest,
    ) -> FeedFuture<'a, DirectoryChangeBatch> {
        Box::pin(async { Err(FeedError::Unsupported) })
    }
}

#[tokio::test]
#[ignore = "requires ERPX_G14_LOCAL_DIRECTORY_DATABASE_URL"]
async fn g14_local_directory_sqlx_acceptance() {
    let Some(database_url) = local_database_url() else {
        panic!(
            "QUEUED BLOCKER (not a pass): local G13/G14 directory SQLx evidence requires {LOCAL_DATABASE_ENV}; external Generic LDAP, AD DS, and UCS certification remains unclaimed"
        );
    };

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect to the explicitly configured local directory evidence database");
    apply_identity_migration(&pool)
        .await
        .expect("apply the identity write-side schema");

    let generic_id = Uuid::new_v4();
    let ad_id = Uuid::new_v4();
    let ucs_id = Uuid::new_v4();
    let legacy_id = Uuid::new_v4();
    let platform_id = Uuid::new_v4();
    let run_suffix = generic_id.simple().to_string();
    let generic_entry_001 = format!("entry-001-{run_suffix}");
    let generic_entry_002 = format!("entry-002-{run_suffix}");
    let generic_entry_003 = format!("entry-003-{run_suffix}");
    let generic_replaced_entry = format!("entry-replaced-{run_suffix}");
    let ad_guid = format!("ad-guid-001-{run_suffix}");
    let ucs_uuid = format!("ucs-uuid-001-{run_suffix}");
    let generic = root_connector(
        generic_id,
        DirectoryProvider::GenericLdap,
        DirectoryFeedKind::Polling,
        "ldaps://ldap.example.test:636",
        DirectoryTlsMode::Ldaps,
        "ldap.example.test",
    );
    let ad = root_connector(
        ad_id,
        DirectoryProvider::ActiveDirectoryDomainServices,
        DirectoryFeedKind::AdDirSync,
        "ldap://ad.example.test:389",
        DirectoryTlsMode::StartTls,
        "ad.example.test",
    );
    let ucs = root_connector(
        ucs_id,
        DirectoryProvider::UniventionCorporateServer,
        DirectoryFeedKind::UcsProvisioningService,
        "https://ucs.example.test:443",
        DirectoryTlsMode::Https,
        "ucs.example.test",
    );
    let legacy = root_connector(
        legacy_id,
        DirectoryProvider::GenericLdap,
        DirectoryFeedKind::Rfc4533,
        "ldaps://rfc4533.example.test:636",
        DirectoryTlsMode::Ldaps,
        "rfc4533.example.test",
    );
    let mut platform = root_connector(
        platform_id,
        DirectoryProvider::GenericLdap,
        DirectoryFeedKind::Polling,
        "ldaps://platform.example.test:636",
        DirectoryTlsMode::Ldaps,
        "platform.example.test",
    );
    platform.scope = DirectoryScope::Platform;
    let directory_store = SqlxDirectoryStore::new(pool.clone());
    for connector in [&generic, &ad, &ucs, &legacy, &platform] {
        directory_store
            .save_connector(connector, fixed_at())
            .await
            .expect("persist local evidence connector");
        assert_eq!(
            directory_store
                .connector(connector.id)
                .await
                .expect("round-trip connector")
                .provider,
            connector.provider
        );
    }
    sqlx::query(
        "UPDATE erpx_identity.directory_connectors
         SET page_size=2, max_in_flight_pages=2
         WHERE id=$1",
    )
    .bind(generic_id)
    .execute(&pool)
    .await
    .expect("persist the native worker tuning used by the local import");
    assert_eq!(
        directory_store
            .connector(platform_id)
            .await
            .expect("round-trip platform connector")
            .scope,
        DirectoryScope::Platform
    );
    let platform_holding_mapping = DirectoryRoleMapping {
        connector_id: platform_id,
        group_ref: "group:platform-directory-users".into(),
        role_ref: "role:directory-reader".into(),
        scope: DirectoryScope::Holding {
            holding_ref: "platform-contained-holding".into(),
        },
        requires_mfa: false,
        active: true,
        revision: 1,
    };
    directory_store
        .save_role_mapping(&platform_holding_mapping, fixed_at())
        .await
        .expect("persist holding role mapping from platform connector");
    let platform_mapping_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_identity.directory_role_mappings
             WHERE connector_id=$1 AND scope_key=$2",
    )
    .bind(platform_id)
    .bind(
        platform_holding_mapping
            .scope
            .authorization_key()
            .expect("normalized platform holding mapping scope"),
    )
    .fetch_one(&pool)
    .await
    .expect("read platform connector mapping scope");
    assert_eq!(platform_mapping_count, 1);
    let role_mapping = DirectoryRoleMapping {
        connector_id: generic_id,
        group_ref: "group:directory-users".into(),
        role_ref: "role:directory-reader".into(),
        scope: generic.scope.clone(),
        requires_mfa: false,
        active: true,
        revision: 1,
    };
    directory_store
        .save_role_mapping(&role_mapping, fixed_at())
        .await
        .expect("persist full-import role mapping");

    let sync_scope_id = match &generic.scope {
        DirectoryScope::Holding { holding_ref } => {
            Uuid::new_v5(&Uuid::NAMESPACE_URL, holding_ref.as_bytes())
        }
        DirectoryScope::Platform => panic!("generic local fixture must be holding-scoped"),
    };
    let sync_connector = make_sync_connector(
        generic_id,
        FeedVendor::GenericLdap,
        FeedKind::GenericLdapPolling,
        sync_scope_id,
        "ldaps://ldap.example.test:636",
        TransportSecurity::Ldaps,
    );
    let import_config = ImportConfig::new(2, 2)
        .expect("bounded import configuration")
        .with_max_users(LOCAL_MAX_USERS)
        .expect("bounded local import ceiling");
    let import_id = Uuid::new_v4();
    let plan = ImportPlan::with_id(import_id, sync_connector.clone(), import_config, fixed_at())
        .expect("local full-import plan");
    let sync_store = SqlxDirectorySyncStore::new(pool.clone());
    let first_checkpoint = sync_store
        .begin_import(&plan)
        .await
        .expect("begin local full import");
    let persisted_config = sync_store
        .active_import_config(generic_id)
        .await
        .expect("read persisted native worker tuning")
        .expect("new import is active");
    assert_eq!(persisted_config.1, 2);
    assert_eq!(persisted_config.2, LOCAL_MAX_USERS);
    assert_eq!(persisted_config.3, 2);
    let first_page = DirectoryPage::new(
        FeedCursor::Start,
        vec![
            sync_record_with_groups(
                "user-001",
                "ldap:entryUUID",
                &generic_entry_001,
                erpx_identity::directory::sync::DirectoryUserState::Active,
                ["group:directory-users"],
            ),
            sync_record(
                "user-002",
                "ldap:entryUUID",
                &generic_entry_002,
                erpx_identity::directory::sync::DirectoryUserState::Active,
            ),
        ],
        Some(FeedCursor::LdapPaged(vec![1])),
        false,
        Some("local-watermark-1".into()),
    )
    .expect("first bounded import page");
    let first_next = first_checkpoint
        .accept_page(&first_page, plan.config(), fixed_at())
        .expect("accept first checkpoint");
    let first_receipt = sync_store
        .stage_page(&plan, &first_checkpoint, &first_page, &first_next)
        .await
        .expect("stage first page atomically");
    assert!(!first_receipt.duplicate_page);
    assert_eq!(first_receipt.records_applied, 2);
    assert_eq!(
        sync_store.active_import_id(generic_id).await.unwrap(),
        Some(import_id)
    );
    let changed_resume_plan = ImportPlan::with_id(
        import_id,
        sync_connector.clone(),
        ImportConfig::new(2, 2)
            .unwrap()
            .with_max_users(1_000_000)
            .expect("bounded changed resume ceiling"),
        fixed_at() + Duration::milliseconds(1),
    )
    .expect("changed resume plan");
    assert!(matches!(
        sync_store.begin_import(&changed_resume_plan).await,
        Err(SyncStoreError::Conflict(message))
            if message.contains("persisted") && message.contains("capacity")
    ));

    let terminal_page = DirectoryPage::new(
        FeedCursor::LdapPaged(vec![1]),
        vec![sync_record_with_groups(
            "user-003",
            "ldap:entryUUID",
            &generic_entry_003,
            erpx_identity::directory::sync::DirectoryUserState::Disabled,
            ["group:directory-users"],
        )],
        None,
        true,
        Some("local-watermark-1".into()),
    )
    .expect("terminal import page");
    let mut resumed_runner = FullImportRunner::new(
        LocalImportFeed {
            connector: sync_connector,
            pages: VecDeque::from([terminal_page]),
        },
        sync_store.clone(),
    );
    let summary = resumed_runner
        .run_with_role_mappings(
            &plan,
            std::slice::from_ref(&role_mapping),
            fixed_at() + Duration::seconds(1),
        )
        .await
        .expect("resume and finalize local full import");
    assert_eq!(summary.users_seen, 3);
    assert_eq!(summary.pages_applied, 2);
    assert_eq!(sync_store.active_import_id(generic_id).await.unwrap(), None);

    let import_state: String =
        sqlx::query_scalar("SELECT state FROM erpx_identity.directory_imports WHERE id=$1")
            .bind(import_id)
            .fetch_one(&pool)
            .await
            .expect("read completed import evidence");
    assert_eq!(import_state, "completed");
    let (staged, canonical, disabled_revocations): (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_identity.directory_import_members WHERE import_id=$1),
            (SELECT count(*) FROM erpx_identity.directory_identities WHERE last_import_id=$1),
            (SELECT count(*) FROM erpx_identity.directory_revocation_sla
             WHERE connector_id=$2 AND external_id='user-003')",
    )
    .bind(import_id)
    .bind(generic_id)
    .fetch_one(&pool)
    .await
    .expect("read full-import row counts");
    assert_eq!((staged, canonical, disabled_revocations), (3, 3, 1));
    let (role_facts, grants, revokes, projected_users): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_identity.directory_role_facts
             WHERE connector_id=$1 AND role_ref='role:directory-reader'),
            (SELECT count(*) FROM erpx_identity.directory_role_facts
             WHERE connector_id=$1 AND role_ref='role:directory-reader' AND action='grant'),
            (SELECT count(*) FROM erpx_identity.directory_role_facts
             WHERE connector_id=$1 AND role_ref='role:directory-reader' AND action='revoke'),
            (SELECT count(*) FROM erpx_identity.directory_identity_projection
             WHERE connector_id=$1)",
    )
    .bind(generic_id)
    .fetch_one(&pool)
    .await
    .expect("read full-import role and projection evidence");
    assert_eq!((role_facts, grants, revokes, projected_users), (3, 1, 2, 3));
    let (role_events, grant_events, revoke_events, orphan_role_facts): (i64, i64, i64, i64) =
        sqlx::query_as(
            "SELECT
                (SELECT count(*) FROM erpx_identity.directory_outbox
                 WHERE connector_id=$1 AND aggregate_type='directory_role'),
                (SELECT count(*) FROM erpx_identity.directory_outbox
                 WHERE connector_id=$1 AND aggregate_type='directory_role'
                   AND event_type='directory.role_granted.v1'),
                (SELECT count(*) FROM erpx_identity.directory_outbox
                 WHERE connector_id=$1 AND aggregate_type='directory_role'
                   AND event_type='directory.role_revoked.v1'),
                (SELECT count(*) FROM erpx_identity.directory_role_facts f
                 WHERE f.connector_id=$1
                   AND NOT EXISTS (
                       SELECT 1 FROM erpx_identity.directory_outbox o
                       WHERE o.event_id=f.id AND o.aggregate_type='directory_role'
                   ))",
        )
        .bind(generic_id)
        .fetch_one(&pool)
        .await
        .expect("read matching full-import role outbox evidence");
    assert_eq!(
        (role_events, grant_events, revoke_events, orphan_role_facts),
        (3, 1, 2, 0)
    );
    let full_import_revocation: (DateTime<Utc>, DateTime<Utc>, String, Option<DateTime<Utc>>) =
        sqlx::query_as(
            "SELECT observed_at, deadline, status, applied_at
         FROM erpx_identity.directory_revocation_sla
         WHERE connector_id=$1 AND external_id='user-003'",
        )
        .bind(generic_id)
        .fetch_one(&pool)
        .await
        .expect("read full-import revocation SLA evidence");
    let full_import_observed_at = fixed_at() + Duration::seconds(1);
    assert_eq!(full_import_revocation.0, full_import_observed_at);
    assert_eq!(
        full_import_revocation.1,
        full_import_observed_at + Duration::minutes(5)
    );
    assert_eq!(
        full_import_revocation.3.is_some(),
        full_import_revocation.2 == "applied",
        "applied revocations must retain evidence and breached rows must not"
    );
    let stored_identity_link: String = sqlx::query_scalar(
        "SELECT identity_link
           FROM erpx_identity.directory_identity_links
          WHERE connector_id=$1 AND external_object_id='user-001'",
    )
    .bind(generic_id)
    .fetch_one(&pool)
    .await
    .expect("read canonical immutable identity link");
    let expected_identity_link = erpx_identity::directory::sync::immutable_identity_database_key(
        &ImmutableIdentityLink::new("ldap:entryUUID", &generic_entry_001)
            .expect("immutable LDAP identity link"),
    );
    assert_eq!(stored_identity_link, expected_identity_link);
    assert_ne!(
        stored_identity_link,
        immutable_identity_link(generic_id, "user-001").unwrap()
    );
    let (identity_link_page, identity_links_have_more) = directory_store
        .list_identity_links(generic_id, 2, None)
        .await
        .expect("read bounded immutable identity-link page");
    assert_eq!(identity_link_page.len(), 2);
    assert!(identity_links_have_more);
    assert!(identity_link_page.iter().all(|link| {
        link.identity_link.is_some()
            && link.immutable_authority.is_some()
            && link.immutable_value.is_some()
    }));
    let next_identity_link = identity_link_page
        .last()
        .and_then(|link| link.identity_link.as_deref())
        .expect("bounded identity-link page has a cursor");
    let (next_identity_link_page, next_identity_links_have_more) = directory_store
        .list_identity_links(generic_id, 2, Some(next_identity_link))
        .await
        .expect("resume bounded immutable identity-link page");
    assert_eq!(next_identity_link_page.len(), 1);
    assert!(!next_identity_links_have_more);
    assert!(next_identity_link_page.iter().all(|link| {
        link.identity_link
            .as_deref()
            .is_some_and(|value| value > next_identity_link)
    }));
    let identity_store = SqlxIdentityStore::new(pool.clone());
    let signer = AccessTokenSigner::new(b"01234567890123456789012345678901")
        .expect("directory login signer");
    let issued_directory_session = identity_store
        .login_with_directory_link(
            &signer,
            generic_id,
            &stored_identity_link,
            "directory-sqlx-acceptance",
            fixed_at() + Duration::seconds(3),
        )
        .await
        .expect("directory login must issue a durable ERPX session");
    let claims = identity_store
        .authorize_access_token(
            &signer,
            issued_directory_session.access_token().token(),
            fixed_at() + Duration::seconds(3),
        )
        .await
        .expect("directory-issued access token must authorize against the write side");
    assert_eq!(claims.principal_ref, "user-001@example.test");
    assert!(
        identity_store
            .principal_is_directory_managed("user-001@example.test")
            .await
            .expect("directory-linked principal must be marked managed")
    );
    let local_digest = PasswordService::default()
        .hash("local-break-glass-password")
        .expect("local password fixture hash");
    sqlx::query(
        "INSERT INTO erpx_identity.password_credentials
            (principal_id, argon2id_digest, changed_at)
         SELECT id, $2, $3
         FROM erpx_identity.principals
         WHERE principal_ref=$1
         ON CONFLICT (principal_id) DO UPDATE
         SET argon2id_digest=EXCLUDED.argon2id_digest, changed_at=EXCLUDED.changed_at",
    )
    .bind("user-001@example.test")
    .bind(local_digest.as_str())
    .bind(fixed_at() + Duration::seconds(4))
    .execute(&pool)
    .await
    .expect("seed only-for-break-glass local credential");
    let ordinary_password = identity_store
        .login(
            &signer,
            "user-001@example.test",
            "local-break-glass-password",
            "ordinary-password-path",
            fixed_at() + Duration::seconds(5),
        )
        .await
        .expect_err("directory-managed principal must not use ordinary password login");
    assert!(matches!(
        ordinary_password,
        erpx_identity::IdentityStoreError::InvalidCredentials
    ));
    assert!(
        !identity_store
            .revoke_session_for_principal(
                "another-principal",
                issued_directory_session.record().id(),
                fixed_at() + Duration::seconds(6),
            )
            .await
            .expect("cross-principal session revocation must be a no-op")
    );
    assert!(
        identity_store
            .revoke_session_for_principal(
                "user-001@example.test",
                issued_directory_session.record().id(),
                fixed_at() + Duration::seconds(7),
            )
            .await
            .expect("own session revocation must succeed")
    );
    let (session_count, auth_event_count): (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_identity.sessions WHERE id=$1),
            (SELECT count(*) FROM erpx_identity.directory_outbox
             WHERE event_type='directory.authentication_succeeded.v1'
               AND idempotency_key=$2)",
    )
    .bind(issued_directory_session.record().id())
    .bind(issued_directory_session.record().id().to_string())
    .fetch_one(&pool)
    .await
    .expect("read directory authentication persistence evidence");
    assert_eq!((session_count, auth_event_count), (1, 1));
    // Exercise the compatibility boundary for a mapping written before
    // opaque holding references were normalized at the authorization edge.
    // The durable role mapping remains append-only history; authorization
    // resolves its raw reference to the same immutable holding identity.
    let platform_holding_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"platform-contained-holding");
    sqlx::query(
        "UPDATE erpx_identity.directory_role_mappings
            SET scope_key='holding:platform-contained-holding'
          WHERE connector_id=$1
            AND group_ref='group:platform-directory-users'
            AND role_ref='role:directory-reader'",
    )
    .bind(platform_id)
    .execute(&pool)
    .await
    .expect("seed legacy opaque role-mapping key");
    sqlx::query(
        "INSERT INTO erpx_identity.directory_identity_projection
            (identity_link, connector_id, principal_ref, active, groups_json,
             source_revision, last_feed_at, revoked_at, updated_at)
         VALUES ($1,$2,$3,true,$4,1,$5,NULL,$5)",
    )
    .bind("platform-user-immutable")
    .bind(platform_id)
    .bind("platform-user@example.test")
    .bind(serde_json::json!(["group:platform-directory-users"]))
    .bind(fixed_at())
    .execute(&pool)
    .await
    .expect("persist platform identity projection for scope authorization");
    let platform_scope_key = format!("holding:{platform_holding_id}");
    assert_eq!(
        identity_store
            .directory_role_access(
                "platform-user@example.test",
                "role:directory-reader",
                &platform_scope_key,
                Some(&platform_scope_key),
                fixed_at() + Duration::seconds(2),
            )
            .await
            .expect("opaque holding role mapping authorization read"),
        DirectoryRoleAccess::Allowed
    );
    let mapping_scope = role_mapping
        .scope
        .canonical_key()
        .expect("canonical role-mapping scope");
    assert_eq!(
        identity_store
            .directory_role_access(
                "user-001@example.test",
                "role:directory-reader",
                &mapping_scope,
                Some(&mapping_scope),
                fixed_at() + Duration::seconds(2),
            )
            .await
            .expect("directory shared-authorization read"),
        DirectoryRoleAccess::Allowed
    );
    assert_eq!(
        identity_store
            .directory_role_access(
                "user-003@example.test",
                "role:directory-reader",
                &mapping_scope,
                Some(&mapping_scope),
                fixed_at() + Duration::seconds(2),
            )
            .await
            .expect("disabled directory identity authorization read"),
        DirectoryRoleAccess::Denied
    );
    assert_eq!(
        identity_store
            .directory_role_access(
                "user-001@example.test",
                "role:directory-reader",
                &mapping_scope,
                Some(&mapping_scope),
                fixed_at() + Duration::minutes(5) + Duration::seconds(1),
            )
            .await
            .expect("stale directory identity authorization read"),
        DirectoryRoleAccess::Denied
    );
    let high_risk_mapping = DirectoryRoleMapping {
        connector_id: generic_id,
        group_ref: "group:directory-users".into(),
        role_ref: "role:directory-high-risk".into(),
        scope: role_mapping.scope.clone(),
        requires_mfa: true,
        active: true,
        revision: 1,
    };
    directory_store
        .save_role_mapping(&high_risk_mapping, fixed_at())
        .await
        .expect("persist high-risk directory mapping");
    assert_eq!(
        identity_store
            .directory_role_access(
                "user-001@example.test",
                "role:directory-high-risk",
                &mapping_scope,
                Some(&mapping_scope),
                fixed_at() + Duration::seconds(2),
            )
            .await
            .expect("high-risk directory authorization without MFA"),
        DirectoryRoleAccess::Denied
    );
    let principal_id: Uuid = sqlx::query_scalar(
        "INSERT INTO erpx_identity.principals
            (id, principal_ref, status, created_at, updated_at)
         VALUES ($1, $2, 'active', $3, $3)
         ON CONFLICT (principal_ref) DO UPDATE
             SET updated_at=EXCLUDED.updated_at
         RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind("user-001@example.test")
    .bind(fixed_at())
    .fetch_one(&pool)
    .await
    .expect("persist high-risk MFA principal");
    sqlx::query(
        "INSERT INTO erpx_identity.totp_secrets
            (principal_id, key_reference, nonce, ciphertext, updated_at,
             last_verified_step)
         VALUES ($1, 'test-only:key', 'nonce', 'ciphertext', $2, 1)",
    )
    .bind(principal_id)
    .bind(fixed_at() + Duration::seconds(1))
    .execute(&pool)
    .await
    .expect("persist recent high-risk MFA evidence");
    assert_eq!(
        identity_store
            .directory_role_access(
                "user-001@example.test",
                "role:directory-high-risk",
                &mapping_scope,
                Some(&mapping_scope),
                fixed_at() + Duration::seconds(2),
            )
            .await
            .expect("high-risk directory authorization with recent MFA"),
        DirectoryRoleAccess::Allowed
    );
    let has_reconciliation_table: bool = sqlx::query_scalar(
        "SELECT to_regclass('erpx_identity.directory_reconciliation_runs') IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("check the reconciliation evidence relation");
    if has_reconciliation_table {
        let report = sync_store
            .reconcile_import(import_id, fixed_at() + Duration::seconds(2))
            .await
            .expect("reconcile completed import");
        assert!(report.is_reconciled());
        let reconciliation_status: String = sqlx::query_scalar(
            "SELECT status FROM erpx_identity.directory_reconciliation_runs
             WHERE import_id=$1 ORDER BY checked_at DESC LIMIT 1",
        )
        .bind(import_id)
        .fetch_one(&pool)
        .await
        .expect("read reconciliation evidence");
        assert_eq!(reconciliation_status, "reconciled");
        let (event_type, event_connector, event_version, idempotency_key, payload): (
            String,
            Uuid,
            i64,
            String,
            serde_json::Value,
        ) = sqlx::query_as(
            "SELECT event_type, connector_id, aggregate_version,
                    idempotency_key, payload_json
             FROM erpx_identity.directory_outbox
             WHERE aggregate_id=$1
               AND event_type='directory.import_reconciliation_reconciled.v1'
             ORDER BY occurred_at DESC
             LIMIT 1",
        )
        .bind(import_id)
        .fetch_one(&pool)
        .await
        .expect("read reconciliation outbox evidence");
        assert_eq!(event_type, "directory.import_reconciliation_reconciled.v1");
        assert_eq!(event_connector, generic_id);
        assert_eq!(event_version, 2);
        assert!(idempotency_key.starts_with("directory-reconciliation/"));
        assert_eq!(payload["status"], "reconciled");
    } else {
        eprintln!(
            "QUEUED BLOCKER (not a pass): SQLx reconciliation evidence requires the migration-owned erpx_identity.directory_reconciliation_runs relation; no reconciliation claim is made"
        );
    }

    let conflicting_plan = ImportPlan::with_id(
        Uuid::new_v4(),
        make_sync_connector(
            generic_id,
            FeedVendor::GenericLdap,
            FeedKind::GenericLdapPolling,
            sync_scope_id,
            "ldaps://ldap.example.test:636",
            TransportSecurity::Ldaps,
        ),
        ImportConfig::new(2, 2)
            .unwrap()
            .with_max_users(LOCAL_MAX_USERS)
            .unwrap(),
        fixed_at() + Duration::seconds(3),
    )
    .expect("conflicting immutable-link import plan");
    let conflicting_checkpoint = sync_store
        .begin_import(&conflicting_plan)
        .await
        .expect("begin conflicting import");
    let conflicting_page = DirectoryPage::new(
        FeedCursor::Start,
        vec![sync_record(
            "user-001",
            "ldap:entryUUID",
            &generic_replaced_entry,
            erpx_identity::directory::sync::DirectoryUserState::Active,
        )],
        None,
        true,
        Some("conflicting-watermark".into()),
    )
    .expect("conflicting import page");
    let conflicting_next = conflicting_checkpoint
        .accept_page(&conflicting_page, conflicting_plan.config(), fixed_at())
        .expect("accept conflicting page checkpoint");
    sync_store
        .stage_page(
            &conflicting_plan,
            &conflicting_checkpoint,
            &conflicting_page,
            &conflicting_next,
        )
        .await
        .expect("stage conflicting page before immutable check");
    let immutable_error = sync_store
        .finalize_import(&conflicting_plan, &conflicting_next, &[], fixed_at())
        .await
        .expect_err("immutable identity replacement must be blocked");
    assert!(matches!(
        immutable_error,
        erpx_identity::directory::sync::SyncStoreError::Conflict(_)
    ));
    sync_store
        .rollback_import(
            conflicting_plan.import_id(),
            "local acceptance immutable-link conflict cleanup",
            fixed_at(),
        )
        .await
        .expect("rollback blocked conflicting import");
    let stored_immutable_value: String = sqlx::query_scalar(
        "SELECT immutable_value FROM erpx_identity.directory_identities
         WHERE connector_id=$1 AND external_id='user-001'",
    )
    .bind(generic_id)
    .fetch_one(&pool)
    .await
    .expect("read immutable identity after blocked replacement");
    assert_eq!(stored_immutable_value, generic_entry_001);

    let ad_sync = make_sync_connector(
        ad_id,
        FeedVendor::ActiveDirectoryDomainServices,
        FeedKind::ActiveDirectoryDirSync,
        Uuid::new_v4(),
        "ldap://ad.example.test:389",
        TransportSecurity::StartTls,
    );
    assert_eq!(ad_sync.feed_kind(), FeedKind::ActiveDirectoryDirSync);
    let ad_link = ImmutableIdentityLink::new("ad:objectGUID", &ad_guid).unwrap();
    let ad_upsert = DirectoryChange::new(
        ad_id,
        "ad-user-001",
        ad_link.clone(),
        ChangeKind::Upsert,
        Some(sync_record(
            "ad-user-001",
            "ad:objectGUID",
            &ad_guid,
            erpx_identity::directory::sync::DirectoryUserState::Active,
        )),
        fixed_at(),
        Some("100".into()),
        FeedCursor::AdDirSync(vec![1]),
    )
    .unwrap();
    assert!(sync_store.apply_change(&ad_upsert).await.unwrap());
    assert!(
        !sync_store
            .apply_change(&ad_upsert)
            .await
            .expect("replay AD DirSync change idempotently")
    );
    let ad_replay_state: (i64, i64) = sqlx::query_as(
        "SELECT aggregate_version,
                (SELECT count(*) FROM erpx_identity.directory_outbox
                 WHERE connector_id=$1 AND idempotency_key=$2)
         FROM erpx_identity.directory_identities
         WHERE connector_id=$1 AND external_id='ad-user-001'",
    )
    .bind(ad_id)
    .bind(
        ad_upsert
            .idempotency_key()
            .expect("AD source idempotency key"),
    )
    .fetch_one(&pool)
    .await
    .expect("read idempotent AD DirSync evidence");
    assert_eq!(ad_replay_state, (1, 1));
    let conflicting_ad_upsert = DirectoryChange::new(
        ad_id,
        "ad-user-001",
        ad_link.clone(),
        ChangeKind::Upsert,
        Some(
            SyncUserRecord::builder("ad-user-001", ad_link.clone())
                .login_name("ad-user-001@example.test")
                .display_name("different-source-content")
                .build()
                .expect("valid conflicting AD source content"),
        ),
        fixed_at() + Duration::minutes(1),
        Some("100".into()),
        FeedCursor::AdDirSync(vec![1]),
    )
    .expect("construct conflicting AD source event");
    assert!(matches!(
        sync_store.apply_change(&conflicting_ad_upsert).await,
        Err(SyncStoreError::Conflict(_))
    ));
    let ad_conflict_state: (i64, i64) = sqlx::query_as(
        "SELECT current.aggregate_version,
                (SELECT count(*) FROM erpx_identity.directory_outbox
                 WHERE connector_id=$1 AND aggregate_id=current.id)
         FROM erpx_identity.directory_identities current
         WHERE connector_id=$1 AND external_id='ad-user-001'",
    )
    .bind(ad_id)
    .fetch_one(&pool)
    .await
    .expect("read rejected AD source conflict evidence");
    assert_eq!(ad_conflict_state, (1, 1));
    let native_revocation_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT CURRENT_TIMESTAMP - INTERVAL '10 seconds'")
            .fetch_one(&pool)
            .await
            .expect("read a current timestamp for native revocation evidence");
    let ad_disable = DirectoryChange::new(
        ad_id,
        "ad-user-001",
        ad_link,
        ChangeKind::Disable,
        None,
        native_revocation_at,
        Some("101".into()),
        FeedCursor::AdDirSync(vec![2]),
    )
    .unwrap();
    assert!(sync_store.apply_change(&ad_disable).await.unwrap());
    let stale_ad_upsert = DirectoryChange::new(
        ad_id,
        "ad-user-001",
        ImmutableIdentityLink::new("ad:objectGUID", &ad_guid).unwrap(),
        ChangeKind::Upsert,
        Some(sync_record(
            "ad-user-001",
            "ad:objectGUID",
            &ad_guid,
            erpx_identity::directory::sync::DirectoryUserState::Active,
        )),
        native_revocation_at + Duration::seconds(1),
        Some("100".into()),
        FeedCursor::AdDirSync(vec![3]),
    )
    .unwrap();
    assert!(
        !sync_store
            .apply_change(&stale_ad_upsert)
            .await
            .expect("ignore an older AD DirSync source revision")
    );
    let ad_order_state: (String, i64, i64, i64) = sqlx::query_as(
        "SELECT state, aggregate_version,
                (SELECT source_revision FROM erpx_identity.directory_identity_links
                 WHERE connector_id=$1 AND external_object_id='ad-user-001'),
                (SELECT count(*) FROM erpx_identity.directory_outbox
                 WHERE connector_id=$1 AND aggregate_id=directory_identities.id)
         FROM erpx_identity.directory_identities
         WHERE connector_id=$1 AND external_id='ad-user-001'",
    )
    .bind(ad_id)
    .fetch_one(&pool)
    .await
    .expect("read AD source ordering evidence");
    assert_eq!(ad_order_state, ("disabled".into(), 2, 101, 2));
    sync_store
        .save_change_checkpoint(
            ad_id,
            FeedKind::ActiveDirectoryDirSync,
            &FeedCursor::Start,
            &FeedCursor::AdDirSync(vec![2]),
            fixed_at(),
        )
        .await
        .expect("persist the current AD DirSync checkpoint");
    assert!(matches!(
        sync_store
            .save_change_checkpoint(
                ad_id,
                FeedKind::ActiveDirectoryDirSync,
                &FeedCursor::AdDirSync(vec![1]),
                &FeedCursor::AdDirSync(vec![3]),
                fixed_at() + Duration::seconds(1),
            )
            .await,
        Err(SyncStoreError::Conflict(message))
            if message.contains("predecessor")
    ));

    let ucs_sync = make_sync_connector(
        ucs_id,
        FeedVendor::UniventionCorporateServer,
        FeedKind::UcsProvisioningService,
        Uuid::new_v4(),
        "https://ucs.example.test:443",
        TransportSecurity::Https,
    );
    assert_eq!(ucs_sync.feed_kind(), FeedKind::UcsProvisioningService);
    let ucs_link = ImmutableIdentityLink::new("ucs:entryUUID", &ucs_uuid).unwrap();
    let ucs_upsert = DirectoryChange::new(
        ucs_id,
        "ucs-user-001",
        ucs_link.clone(),
        ChangeKind::Upsert,
        Some(sync_record(
            "ucs-user-001",
            "ucs:entryUUID",
            &ucs_uuid,
            erpx_identity::directory::sync::DirectoryUserState::Active,
        )),
        native_revocation_at,
        Some("ucs-sequence-1".into()),
        FeedCursor::Ucs {
            token: "ucs-token-1".into(),
            sequence: 1,
        },
    )
    .unwrap();
    assert!(sync_store.apply_change(&ucs_upsert).await.unwrap());
    let ucs_delete = DirectoryChange::new(
        ucs_id,
        "ucs-user-001",
        ucs_link,
        ChangeKind::Delete,
        None,
        native_revocation_at + Duration::seconds(1),
        Some("ucs-sequence-2".into()),
        FeedCursor::Ucs {
            token: "ucs-token-2".into(),
            sequence: 2,
        },
    )
    .unwrap();
    assert!(sync_store.apply_change(&ucs_delete).await.unwrap());

    let (ad_state, ucs_state, ad_sla, ucs_sla): (String, String, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT state FROM erpx_identity.directory_identities
             WHERE connector_id=$1 AND external_id='ad-user-001'),
            (SELECT state FROM erpx_identity.directory_identities
             WHERE connector_id=$2 AND external_id='ucs-user-001'),
            (SELECT count(*) FROM erpx_identity.directory_revocation_sla
             WHERE connector_id=$1 AND external_id='ad-user-001'),
            (SELECT count(*) FROM erpx_identity.directory_revocation_sla
             WHERE connector_id=$2 AND external_id='ucs-user-001')",
    )
    .bind(ad_id)
    .bind(ucs_id)
    .fetch_one(&pool)
    .await
    .expect("read AD and UCS change-feed evidence");
    assert_eq!(
        (ad_state, ucs_state, ad_sla, ucs_sla),
        ("disabled".into(), "deleted".into(), 1, 1)
    );
    let native_revocations: Vec<NativeRevocationRow> = sqlx::query_as(
        "SELECT external_id, observed_at, deadline, status, applied_at
             FROM erpx_identity.directory_revocation_sla
             WHERE connector_id IN ($1, $2)
             ORDER BY connector_id, external_id",
    )
    .bind(ad_id)
    .bind(ucs_id)
    .fetch_all(&pool)
    .await
    .expect("read native-feed revocation SLA evidence");
    assert_eq!(native_revocations.len(), 2);
    for (index, row) in native_revocations.iter().enumerate() {
        assert!(matches!(
            row.external_id.as_str(),
            "ad-user-001" | "ucs-user-001"
        ));
        assert_eq!(
            row.deadline,
            row.observed_at + Duration::minutes(5),
            "native {} revocation must carry the five-minute deadline",
            index
        );
        assert_eq!(row.status, "applied");
        let applied_at = row
            .applied_at
            .expect("applied native revocation must retain evidence");
        assert!(applied_at >= row.observed_at && applied_at <= row.deadline);
    }

    let legacy_scope = legacy.scope.clone();
    let role_mapping = DirectoryRoleMapping {
        connector_id: legacy_id,
        group_ref: "directory-admins".into(),
        role_ref: "role:directory-admin".into(),
        scope: legacy_scope.clone(),
        requires_mfa: true,
        active: true,
        revision: 1,
    };
    directory_store
        .save_role_mapping(&role_mapping, fixed_at())
        .await
        .expect("persist connector-scoped role mapping");
    let mut groups = BTreeSet::new();
    groups.insert("directory-admins".into());
    let legacy_page = DirectorySyncPage {
        connector_id: legacy_id,
        cursor: DirectorySyncCursor {
            connector_id: legacy_id,
            feed: DirectoryFeedKind::Rfc4533,
            sequence: 1,
            opaque_cookie: Some("rfc-cookie-1".into()),
            observed_at: fixed_at(),
        },
        changes: vec![LegacyDirectoryChange::Upsert(DirectoryUser {
            external_object_id: "legacy-user-001".into(),
            principal_ref: "legacy-principal".into(),
            email: Some("legacy@example.test".into()),
            display_name: Some("Legacy User".into()),
            groups,
            active: true,
            source_revision: 1,
        })],
    };
    directory_store
        .apply_sync_page(
            &legacy_page,
            DirectoryProvider::GenericLdap,
            None,
            std::slice::from_ref(&role_mapping),
        )
        .await
        .expect("apply generic LDAP change feed with role mapping");
    let legacy_cursor = directory_store
        .latest_cursor(legacy_id)
        .await
        .expect("read committed generic cursor")
        .expect("generic cursor exists");
    assert_eq!(legacy_cursor.sequence, 1);
    directory_store
        .apply_sync_page(
            &legacy_page,
            DirectoryProvider::GenericLdap,
            None,
            std::slice::from_ref(&role_mapping),
        )
        .await
        .expect("replay generic change feed page idempotently");
    let conflicting_legacy_page = DirectorySyncPage {
        changes: vec![LegacyDirectoryChange::Upsert(DirectoryUser {
            external_object_id: "legacy-user-001".into(),
            principal_ref: "legacy-principal".into(),
            email: Some("changed@example.test".into()),
            display_name: Some("Conflicting same revision".into()),
            groups: ["directory-admins".to_owned()].into_iter().collect(),
            active: true,
            source_revision: 1,
        })],
        ..legacy_page.clone()
    };
    assert!(matches!(
        directory_store
            .apply_sync_page(
                &conflicting_legacy_page,
                DirectoryProvider::GenericLdap,
                None,
                std::slice::from_ref(&role_mapping),
            )
            .await,
        Err(DirectoryError::InvalidBatch(_))
    ));
    let grant_facts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_identity.directory_role_facts
         WHERE connector_id=$1 AND action='grant'",
    )
    .bind(legacy_id)
    .fetch_one(&pool)
    .await
    .expect("read role grant fact evidence");
    assert_eq!(grant_facts, 1);

    let legacy_disable_page = DirectorySyncPage {
        connector_id: legacy_id,
        cursor: DirectorySyncCursor {
            connector_id: legacy_id,
            feed: DirectoryFeedKind::Rfc4533,
            sequence: 9,
            opaque_cookie: Some("rfc-cookie-9".into()),
            observed_at: fixed_at() + Duration::seconds(1),
        },
        changes: vec![LegacyDirectoryChange::Disable {
            external_object_id: "legacy-user-001".into(),
            source_revision: 9,
        }],
    };
    directory_store
        .apply_sync_page(
            &legacy_disable_page,
            DirectoryProvider::GenericLdap,
            Some(&legacy_cursor),
            std::slice::from_ref(&role_mapping),
        )
        .await
        .expect("apply generic LDAP disable and revoke role");
    let legacy_link = erpx_identity::directory::sync::immutable_identity_database_key(
        &ImmutableIdentityLink::new("generic_ldap", "legacy-user-001")
            .expect("canonical native-feed immutable link"),
    );
    let revoked = directory_store
        .read_revocation_state(&legacy_link, fixed_at() + Duration::seconds(2))
        .await
        .expect("read immediate generic revocation state");
    assert!(!revoked.authorization_allowed(fixed_at() + Duration::seconds(2)));
    let (role_facts, role_mappings, outbox_events): (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_identity.directory_role_facts WHERE connector_id=$1),
            (SELECT count(*) FROM erpx_identity.directory_role_mappings WHERE connector_id=$1),
            (SELECT count(*) FROM erpx_identity.directory_outbox WHERE aggregate_id=$1)",
    )
    .bind(legacy_id)
    .fetch_one(&pool)
    .await
    .expect("read role and outbox evidence");
    assert_eq!((role_facts, role_mappings), (2, 1));
    assert!(outbox_events >= 3);

    let role_chain: Vec<(String, i64, Option<String>, String)> = sqlx::query_as(
        "SELECT action, sequence, previous_hash, fact_hash
         FROM erpx_identity.directory_role_facts
         WHERE connector_id=$1 AND identity_link=$2
           AND role_ref='role:directory-admin'
         ORDER BY sequence, occurred_at",
    )
    .bind(legacy_id)
    .bind(&legacy_link)
    .fetch_all(&pool)
    .await
    .expect("read append-only role revocation chain");
    assert_eq!(role_chain.len(), 2);
    assert_eq!(role_chain[0].0, "grant");
    assert_eq!(role_chain[0].1, 1);
    assert!(role_chain[0].2.is_none());
    assert_eq!(role_chain[1].0, "revoke");
    assert_eq!(role_chain[1].1, 2);
    assert_eq!(role_chain[1].2.as_deref(), Some(role_chain[0].3.as_str()));
    assert_ne!(role_chain[1].3, role_chain[0].3);

    let final_projection_state: (bool, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT active, revoked_at FROM erpx_identity.directory_identity_projection
         WHERE identity_link=$1",
    )
    .bind(&legacy_link)
    .fetch_one(&pool)
    .await
    .expect("read persisted role/revocation projection");
    assert!(!final_projection_state.0);
    assert!(final_projection_state.1.is_some());

    let rbac_store = SqlxDirectoryRbacStore::new(pool.clone());
    let current_role_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_identity.directory_outbox
         WHERE connector_id IN ($1, $2) AND aggregate_type='directory_role'",
    )
    .bind(generic_id)
    .bind(legacy_id)
    .fetch_one(&pool)
    .await
    .expect("count durable role events before projection");
    let projection_report = rbac_store
        .project_role_outbox(100, fixed_at() + Duration::seconds(2))
        .await
        .expect("project durable role outbox into the SQLx read model");
    assert!(
        projection_report.projected >= u64::try_from(current_role_events).unwrap(),
        "every current role event must be eligible for durable projection"
    );
    let projected_current_role_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_identity.directory_outbox
         WHERE connector_id IN ($1, $2)
           AND aggregate_type='directory_role' AND projected_at IS NOT NULL",
    )
    .bind(generic_id)
    .bind(legacy_id)
    .fetch_one(&pool)
    .await
    .expect("read projected role event evidence");
    assert_eq!(projected_current_role_events, current_role_events);
    let generic_role_identity = DirectoryRoleIdentity::from_immutable_link(
        ImmutableIdentityLink::new("ldap:entryUUID", &generic_entry_001)
            .expect("generic immutable role identity"),
    );
    let projected_generic_roles = rbac_store
        .projected_roles_for(&generic_role_identity, generic_id)
        .await
        .expect("read active Generic LDAP role projection");
    assert_eq!(projected_generic_roles.len(), 1);
    assert_eq!(
        projected_generic_roles
            .iter()
            .next()
            .map(|(role, _)| role.as_str()),
        Some("role:directory-reader")
    );
    let legacy_role_identity = DirectoryRoleIdentity::from_immutable_link(
        ImmutableIdentityLink::new("generic_ldap", "legacy-user-001")
            .expect("legacy immutable role identity"),
    );
    assert!(
        rbac_store
            .projected_roles_for(&legacy_role_identity, legacy_id)
            .await
            .expect("read revoked RFC4533 role projection")
            .is_empty()
    );

    eprintln!(
        "LOCAL G13/G14 DIRECTORY SQLx EVIDENCE (not vendor certification): platform and holding connectors; generic full import/resume/immutability; AD DirSync disable SLA; UCS Provisioning delete SLA; Generic LDAP RFC4533 role grant/revoke, cursor replay, durable role projector/read model, matching role outbox, applied revocation evidence, MFA, and five-minute SLA"
    );
    pool.close().await;
}

#[test]
fn local_sqlx_acceptance_reports_the_exact_database_blocker() {
    if local_database_url().is_none() {
        eprintln!(
            "QUEUED BLOCKER (not a pass): local SQLx directory evidence is waiting for {LOCAL_DATABASE_ENV}"
        );
    }
}

#[tokio::test]
#[ignore = "requires ERPX_G14_LOCAL_DIRECTORY_DATABASE_URL"]
async fn api_started_import_is_claimable_by_the_native_worker_contract() {
    let Some(database_url) = local_database_url() else {
        panic!(
            "QUEUED BLOCKER (not a pass): API/native-worker import handoff evidence requires {LOCAL_DATABASE_ENV}"
        );
    };

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect to the explicitly configured local directory evidence database");
    apply_identity_migration(&pool)
        .await
        .expect("apply the identity write-side schema");

    let connector = root_connector(
        Uuid::new_v4(),
        DirectoryProvider::GenericLdap,
        DirectoryFeedKind::Polling,
        "ldaps://api-worker-handoff.example.test:636",
        DirectoryTlsMode::Ldaps,
        "api-worker-handoff.example.test",
    );
    let store = SqlxDirectoryStore::new(pool.clone());
    store
        .save_connector(&connector, fixed_at())
        .await
        .expect("persist API/native-worker handoff connector");

    let plan = store
        .start_import(&connector, 1_000_000, 5_000, fixed_at())
        .await
        .expect("persist API-started full import");
    assert_eq!(plan.total_users, CERTIFICATION_MAX_USERS);
    assert_eq!(plan.page_size, 5_000);
    assert_eq!(plan.page_count(), 200);
    assert_eq!(plan.max_in_flight_pages, 4);

    let sync_store = SqlxDirectorySyncStore::new(pool.clone());
    let persisted = sync_store
        .active_import_config(connector.id)
        .await
        .expect("read the import contract claimed by the native worker")
        .expect("API-started import must be visible as active work");
    assert_eq!(persisted.1, 5_000);
    assert_eq!(persisted.2, 1_000_000);
    assert_eq!(persisted.3, 4);
    assert_eq!(
        sync_store.active_import_id(connector.id).await.unwrap(),
        Some(persisted.0)
    );

    let status = store
        .import_status(connector.id, Some(persisted.0))
        .await
        .expect("read the authoritative import checkpoint")
        .expect("started import must have a persisted checkpoint");
    assert_eq!(status.import_id, persisted.0);
    assert_eq!(status.connector_id, connector.id);
    assert_eq!(status.state, "running");
    assert_eq!(status.max_users, CERTIFICATION_MAX_USERS);
    assert_eq!(status.users_seen, 0);
    assert_eq!(status.pages_applied, 0);
    assert_eq!(status.page_count, 200);
    assert_eq!(
        status.cursor_kind, "start",
        "a newly queued import exposes the initial cursor kind until its first native page"
    );
    assert!(status.finalized_at.is_none());
    assert!(status.failure_category.is_none());

    let payload: serde_json::Value = sqlx::query_scalar(
        "SELECT payload_json
         FROM erpx_identity.directory_outbox
         WHERE aggregate_id=$1 AND aggregate_type='directory_import'
         ORDER BY occurred_at DESC
         LIMIT 1",
    )
    .bind(connector.id)
    .fetch_one(&pool)
    .await
    .expect("read API import outbox evidence");
    assert_eq!(payload["event"], "directory.import_started");
    assert_eq!(payload["import_id"], persisted.0.to_string());
    assert_eq!(payload["page_size"], 5_000);
    assert_eq!(payload["max_in_flight_pages"], 4);
    assert_eq!(payload["state_table"], "directory_imports");

    let legacy_jobs: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM erpx_identity.directory_import_jobs
         WHERE connector_id=$1",
    )
    .bind(connector.id)
    .fetch_one(&pool)
    .await
    .expect("read legacy import queue evidence");
    assert_eq!(
        legacy_jobs, 0,
        "API imports must not be stranded in the legacy queue"
    );
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires ERPX_G14_LOCAL_DIRECTORY_DATABASE_URL"]
async fn role_mapping_save_rejects_scope_escape_without_side_effects() {
    let Some(database_url) = local_database_url() else {
        panic!(
            "QUEUED BLOCKER (not a pass): role-mapping scope-containment SQLx evidence requires {LOCAL_DATABASE_ENV}"
        );
    };

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect to the explicitly configured local directory evidence database");
    apply_identity_migration(&pool)
        .await
        .expect("apply the identity write-side schema");

    let connector = root_connector(
        Uuid::new_v4(),
        DirectoryProvider::GenericLdap,
        DirectoryFeedKind::Rfc4533,
        "ldaps://scope-containment.example.test:636",
        DirectoryTlsMode::Ldaps,
        "scope-containment.example.test",
    );
    let store = SqlxDirectoryStore::new(pool.clone());
    store
        .save_connector(&connector, fixed_at())
        .await
        .expect("persist holding-scoped connector");

    let escaped_mapping = DirectoryRoleMapping {
        connector_id: connector.id,
        group_ref: "directory-admins".into(),
        role_ref: "role:directory-admin".into(),
        scope: DirectoryScope::Platform,
        requires_mfa: true,
        active: true,
        revision: 1,
    };
    let error = store
        .save_role_mapping(&escaped_mapping, fixed_at())
        .await
        .expect_err("holding connector must not persist a platform mapping");
    assert!(matches!(
        error,
        erpx_identity::DirectoryError::InvalidReference(message)
            if message == "role mapping scope is outside the connector scope"
    ));

    let counts: (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_identity.directory_role_mappings
             WHERE connector_id=$1),
            (SELECT count(*) FROM erpx_identity.directory_outbox
             WHERE aggregate_id=$1 AND aggregate_type='directory_role_mapping')",
    )
    .bind(connector.id)
    .fetch_one(&pool)
    .await
    .expect("read rejected role-mapping side-effect evidence");
    assert_eq!(counts, (0, 0));

    let valid_mapping = DirectoryRoleMapping {
        scope: connector.scope.clone(),
        revision: 1,
        ..escaped_mapping
    };
    store
        .save_role_mapping(&valid_mapping, fixed_at())
        .await
        .expect("persist connector-contained mapping");
    let stored: (i64, i64, bool) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_identity.directory_role_mappings
             WHERE connector_id=$1),
            (SELECT count(*) FROM erpx_identity.directory_outbox
             WHERE aggregate_id=$1 AND aggregate_type='directory_role_mapping'),
            (SELECT requires_mfa FROM erpx_identity.directory_role_mappings
             WHERE connector_id=$1)",
    )
    .bind(connector.id)
    .fetch_one(&pool)
    .await
    .expect("read committed role-mapping and outbox evidence");
    assert_eq!(stored, (1, 1, true));
    pool.close().await;
}
