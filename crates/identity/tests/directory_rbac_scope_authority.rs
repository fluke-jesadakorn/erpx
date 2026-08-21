//! Live SQLx evidence that directory RBAC uses the persisted connector scope.
//!
//! This test is intentionally ignored unless a disposable PostgreSQL database
//! is explicitly supplied. It exercises the real identity schema and the
//! directory RBAC write transaction; it is not vendor-directory certification.

use std::env;

use chrono::{DateTime, TimeZone, Utc};
use erpx_identity::apply_identity_migration;
use erpx_identity::directory::rbac::{
    DirectoryGroupRef, DirectoryManagedRoleMapping, DirectoryMappingWriteOutcome,
    DirectoryRbacAuthorizationContext, DirectoryRbacError, DirectoryRoleFactAction,
    DirectoryRoleFactInput, DirectoryRoleFactLedger, DirectoryRoleFactReason,
    DirectoryRoleIdentity, DirectoryRoleRef, SqlxDirectoryRbacStore,
};
use erpx_identity::directory::sync::{ConnectorScope, ImmutableIdentityLink};
use erpx_identity::{DirectoryRoleAccess, SqlxIdentityStore};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

const DATABASE_ENV: &str = "ERPX_TEST_DIRECTORY_RBAC_DATABASE_URL";

fn database_url() -> Option<String> {
    env::var(DATABASE_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty() && !value.chars().any(char::is_control))
}

fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 7, 27, 12, 0, 0)
        .single()
        .expect("fixed RBAC SQLx timestamp")
}

async fn reset_disposable_rbac_fixture(pool: &PgPool) {
    sqlx::query(
        "TRUNCATE TABLE erpx_identity.directory_outbox,
            erpx_identity.directory_connectors CASCADE",
    )
    .execute(pool)
    .await
    .expect("clear disposable directory RBAC fixtures");
}

fn mapping(
    connector_id: Uuid,
    scope: ConnectorScope,
    revision: u64,
) -> DirectoryManagedRoleMapping {
    DirectoryManagedRoleMapping::try_new(
        connector_id,
        DirectoryGroupRef::try_new("group:directory-admins").expect("group ref"),
        DirectoryRoleRef::try_new("role:directory-admin").expect("role ref"),
        scope,
        true,
        true,
        revision,
        at(),
    )
    .expect("valid managed mapping")
}

#[tokio::test]
#[ignore = "requires ERPX_TEST_DIRECTORY_RBAC_DATABASE_URL"]
async fn mapping_write_rejects_forged_connector_scope_without_side_effects() {
    let Some(database_url) = database_url() else {
        panic!(
            "QUEUED BLOCKER (not a pass): authoritative RBAC scope evidence requires {DATABASE_ENV}"
        );
    };
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect to the explicitly supplied disposable RBAC database");
    apply_identity_migration(&pool)
        .await
        .expect("apply the identity write-side schema");
    reset_disposable_rbac_fixture(&pool).await;

    let connector_id = Uuid::new_v4();
    let holding_id = Uuid::new_v4();
    let now = at();
    sqlx::query(
        "INSERT INTO erpx_identity.directory_connectors
            (id, provider, vendor, scope_key, scope_kind, scope_id, data_profile_key,
             endpoint, base_dn, tls_mode, security, page_size, max_in_flight_pages,
             ca_certificate_sha256, server_name, minimum_tls_version, feed_kind,
             credential_ref, enabled, max_import_users, revision, created_at, updated_at)
         VALUES ($1,'generic_ldap','generic_ldap',$2,'holding',$3,$4,
                 'ldaps://rbac.example.test:636','dc=example,dc=test','ldaps','ldaps',
                 500,4,$5,'rbac.example.test','1.2','polling',
                 'test-only:rbac-secret',true,1000000,1,$6,$6)",
    )
    .bind(connector_id)
    .bind(format!("holding:{holding_id}"))
    .bind(holding_id)
    .bind(holding_id.to_string())
    .bind("a".repeat(64))
    .bind(now)
    .execute(&pool)
    .await
    .expect("persist authoritative holding connector");

    let store = SqlxDirectoryRbacStore::new(pool.clone());
    let authorization = DirectoryRbacAuthorizationContext::try_new(
        "platform-directory-admin",
        ConnectorScope::Platform,
        Some(now),
        false,
        false,
    )
    .expect("platform authorization context");
    let forged = mapping(connector_id, ConnectorScope::Platform, 1);
    assert_eq!(
        store
            .save_mapping(&forged, &ConnectorScope::Platform, &authorization, now,)
            .await,
        Err(DirectoryRbacError::ConnectorScopeMismatch)
    );
    let side_effects: (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_identity.directory_role_mappings
             WHERE connector_id=$1),
            (SELECT count(*) FROM erpx_identity.directory_outbox
             WHERE aggregate_id=$1 AND aggregate_type='directory_role_mapping')",
    )
    .bind(connector_id)
    .fetch_one(&pool)
    .await
    .expect("read rejected-write side effects");
    assert_eq!(side_effects, (0, 0));

    let valid = mapping(connector_id, ConnectorScope::Holding(holding_id), 1);
    let outcome = store
        .save_mapping(
            &valid,
            &ConnectorScope::Holding(holding_id),
            &authorization,
            now,
        )
        .await
        .expect("persist connector-contained mapping");
    assert!(matches!(
        outcome,
        DirectoryMappingWriteOutcome::Applied { .. }
    ));
    let stored_scope: String = sqlx::query_scalar(
        "SELECT scope_key FROM erpx_identity.directory_role_mappings
         WHERE connector_id=$1",
    )
    .bind(connector_id)
    .fetch_one(&pool)
    .await
    .expect("read committed mapping scope");
    assert_eq!(stored_scope, format!("holding:{holding_id}"));
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_identity.directory_outbox
         WHERE aggregate_id=$1 AND aggregate_type='directory_role_mapping'",
    )
    .bind(connector_id)
    .fetch_one(&pool)
    .await
    .expect("read committed mapping event");
    assert_eq!(outbox_count, 1);
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires ERPX_TEST_DIRECTORY_RBAC_DATABASE_URL"]
async fn role_facts_reconstruct_effective_roles_from_durable_outbox_and_facts() {
    let Some(database_url) = database_url() else {
        panic!(
            "QUEUED BLOCKER (not a pass): authoritative RBAC reconstruction evidence requires {DATABASE_ENV}"
        );
    };
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect to the explicitly supplied disposable RBAC database");
    apply_identity_migration(&pool)
        .await
        .expect("apply the identity write-side schema");
    reset_disposable_rbac_fixture(&pool).await;

    let connector_id = Uuid::new_v4();
    let holding_id = Uuid::new_v4();
    let now = at();
    sqlx::query(
        "INSERT INTO erpx_identity.directory_connectors
            (id, provider, vendor, scope_key, scope_kind, scope_id, data_profile_key,
             endpoint, base_dn, tls_mode, security, page_size, max_in_flight_pages,
             ca_certificate_sha256, server_name, minimum_tls_version, feed_kind,
             credential_ref, enabled, max_import_users, revision, created_at, updated_at)
         VALUES ($1,'generic_ldap','generic_ldap',$2,'holding',$3,$4,
                 'ldaps://rbac-reconstruct.example.test:636','dc=example,dc=test','ldaps','ldaps',
                 500,4,$5,'rbac-reconstruct.example.test','1.2','polling',
                 'test-only:rbac-secret',true,1000000,1,$6,$6)",
    )
    .bind(connector_id)
    .bind(format!("holding:{holding_id}"))
    .bind(holding_id)
    .bind(holding_id.to_string())
    .bind("b".repeat(64))
    .bind(now)
    .execute(&pool)
    .await
    .expect("persist reconstruction connector");

    let store = SqlxDirectoryRbacStore::new(pool.clone());
    let authorization = DirectoryRbacAuthorizationContext::try_new(
        "platform-directory-admin",
        ConnectorScope::Platform,
        Some(now),
        false,
        false,
    )
    .expect("platform authorization context");
    let role_mapping = mapping(connector_id, ConnectorScope::Holding(holding_id), 1);
    store
        .save_mapping(
            &role_mapping,
            &ConnectorScope::Holding(holding_id),
            &authorization,
            now,
        )
        .await
        .expect("persist reconstruction mapping");

    let immutable_link = ImmutableIdentityLink::new("ad:object_guid", "rbac-reconstruction-user")
        .expect("immutable identity link");
    let identity = DirectoryRoleIdentity::from_immutable_link(immutable_link.clone());
    sqlx::query(
        "INSERT INTO erpx_identity.directory_identity_links
            (connector_id, external_object_id, identity_link, principal_ref,
             active, source_revision, observed_at, revoked_at,
             immutable_authority, immutable_value)
         VALUES ($1,$2,$3,$4,true,1,$5,NULL,$6,$7)",
    )
    .bind(connector_id)
    .bind("rbac-reconstruction-user")
    .bind(identity.database_key())
    .bind("reconstruction-user")
    .bind(now)
    .bind(immutable_link.authority())
    .bind(immutable_link.value())
    .execute(&pool)
    .await
    .expect("persist immutable reconstruction link");

    let mut ledger = DirectoryRoleFactLedger::default();
    let input = |action: DirectoryRoleFactAction,
                 reason: DirectoryRoleFactReason,
                 idempotency_key: &str| DirectoryRoleFactInput {
        identity: identity.clone(),
        connector_id,
        group_ref: DirectoryGroupRef::try_new("group:directory-admins").expect("group ref"),
        role_ref: DirectoryRoleRef::try_new("role:directory-admin").expect("role ref"),
        scope: ConnectorScope::Holding(holding_id),
        action,
        reason,
        idempotency_key: idempotency_key.to_owned(),
        occurred_at: now,
    };
    let (_, grant) = ledger
        .append(input(
            DirectoryRoleFactAction::Grant,
            DirectoryRoleFactReason::GroupMatched,
            "reconstruct-grant",
        ))
        .expect("build grant fact");
    let (_, revoke) = ledger
        .append(input(
            DirectoryRoleFactAction::Revoke,
            DirectoryRoleFactReason::GroupRemoved,
            "reconstruct-revoke",
        ))
        .expect("build revoke fact");
    store.append_fact(&grant).await.expect("persist grant fact");
    store
        .append_fact(&revoke)
        .await
        .expect("persist revoke fact");

    let read_model = store
        .reconstruct_read_model()
        .await
        .expect("reconstruct role read model");
    assert_eq!(read_model.applied_event_count(), 2);
    assert!(read_model.roles_for(&identity, connector_id).is_empty());
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires ERPX_TEST_DIRECTORY_RBAC_DATABASE_URL"]
async fn durable_projection_keeps_a_role_when_one_of_two_group_grants_is_revoked() {
    let Some(database_url) = database_url() else {
        panic!(
            "QUEUED BLOCKER (not a pass): multi-group role projection evidence requires {DATABASE_ENV}"
        );
    };
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect to the explicitly supplied disposable RBAC database");
    apply_identity_migration(&pool)
        .await
        .expect("apply the identity write-side schema");
    reset_disposable_rbac_fixture(&pool).await;

    let connector_id = Uuid::new_v4();
    let holding_id = Uuid::new_v4();
    let identity_link = ImmutableIdentityLink::new("ldap:entryUUID", "two-group-user")
        .expect("immutable identity link");
    let identity = DirectoryRoleIdentity::from_immutable_link(identity_link.clone());
    let now = at();
    sqlx::query(
        "INSERT INTO erpx_identity.directory_connectors
            (id, provider, vendor, scope_key, scope_kind, scope_id, data_profile_key,
             endpoint, base_dn, tls_mode, security, page_size, max_in_flight_pages,
             ca_certificate_sha256, server_name, minimum_tls_version, feed_kind,
             credential_ref, enabled, max_import_users, revision, created_at, updated_at)
         VALUES ($1,'generic_ldap','generic_ldap',$2,'holding',$3,$4,
                 'ldaps://two-group.example.test:636','dc=example,dc=test','ldaps','ldaps',
                 500,4,$5,'two-group.example.test','1.2','polling',
                 'test-only:rbac-secret',true,1000000,1,$6,$6)",
    )
    .bind(connector_id)
    .bind(format!("holding:{holding_id}"))
    .bind(holding_id)
    .bind(holding_id.to_string())
    .bind("c".repeat(64))
    .bind(now)
    .execute(&pool)
    .await
    .expect("persist platform connector");
    sqlx::query(
        "INSERT INTO erpx_identity.directory_identity_links
            (connector_id, external_object_id, identity_link, principal_ref,
             active, source_revision, observed_at, revoked_at,
             immutable_authority, immutable_value)
         VALUES ($1,$2,$3,$4,true,1,$5,NULL,$6,$7)",
    )
    .bind(connector_id)
    .bind("two-group-user")
    .bind(identity.database_key())
    .bind("two-group-principal")
    .bind(now)
    .bind(identity_link.authority())
    .bind(identity_link.value())
    .execute(&pool)
    .await
    .expect("persist active immutable identity link");

    let authorization = DirectoryRbacAuthorizationContext::try_new(
        "platform-directory-admin",
        ConnectorScope::Holding(holding_id),
        Some(now),
        false,
        false,
    )
    .expect("platform authorization context");
    for (group, revision) in [("group:approvers", 1), ("group:auditors", 1)] {
        let role_mapping = DirectoryManagedRoleMapping::try_new(
            connector_id,
            DirectoryGroupRef::try_new(group).expect("group ref"),
            DirectoryRoleRef::try_new("role:finance-approve").expect("role ref"),
            ConnectorScope::Holding(holding_id),
            false,
            true,
            revision,
            now,
        )
        .expect("valid group-to-role mapping");
        SqlxDirectoryRbacStore::new(pool.clone())
            .save_mapping(
                &role_mapping,
                &ConnectorScope::Holding(holding_id),
                &authorization,
                now,
            )
            .await
            .expect("persist group-to-role mapping");
    }

    let mut ledger = DirectoryRoleFactLedger::default();
    let role_ref = DirectoryRoleRef::try_new("role:finance-approve").expect("role ref");
    let role_fact = |group: &str,
                     action: DirectoryRoleFactAction,
                     reason: DirectoryRoleFactReason,
                     idempotency_key: &str| DirectoryRoleFactInput {
        identity: identity.clone(),
        connector_id,
        group_ref: DirectoryGroupRef::try_new(group).expect("group ref"),
        role_ref: role_ref.clone(),
        scope: ConnectorScope::Holding(holding_id),
        action,
        reason,
        idempotency_key: idempotency_key.to_owned(),
        occurred_at: now,
    };
    let (_, grant_approvers) = ledger
        .append(role_fact(
            "group:approvers",
            DirectoryRoleFactAction::Grant,
            DirectoryRoleFactReason::GroupMatched,
            "two-group-grant-approvers",
        ))
        .expect("build approvers grant");
    let (_, grant_auditors) = ledger
        .append(role_fact(
            "group:auditors",
            DirectoryRoleFactAction::Grant,
            DirectoryRoleFactReason::GroupMatched,
            "two-group-grant-auditors",
        ))
        .expect("build auditors grant");
    let (_, revoke_approvers) = ledger
        .append(role_fact(
            "group:approvers",
            DirectoryRoleFactAction::Revoke,
            DirectoryRoleFactReason::GroupRemoved,
            "two-group-revoke-approvers",
        ))
        .expect("build approvers revoke");
    let store = SqlxDirectoryRbacStore::new(pool.clone());
    for fact in [&grant_approvers, &grant_auditors, &revoke_approvers] {
        store.append_fact(fact).await.expect("persist role fact");
    }
    store
        .project_role_outbox(100, now)
        .await
        .expect("project durable role facts");
    assert_eq!(
        store
            .projected_roles_for(&identity, connector_id)
            .await
            .expect("read projected role")
            .len(),
        1
    );
    assert_eq!(
        store
            .reconstruct_read_model()
            .await
            .expect("reconstruct role read model")
            .roles_for(&identity, connector_id)
            .len(),
        1
    );
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires ERPX_TEST_DIRECTORY_RBAC_DATABASE_URL"]
async fn authorization_rejects_a_requested_holding_different_from_the_authorized_holding() {
    let Some(database_url) = database_url() else {
        panic!(
            "QUEUED BLOCKER (not a pass): requested/authorized holding scope evidence requires {DATABASE_ENV}"
        );
    };
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect to the explicitly supplied disposable RBAC database");
    apply_identity_migration(&pool)
        .await
        .expect("apply the identity write-side schema");
    reset_disposable_rbac_fixture(&pool).await;

    let connector_id = Uuid::new_v4();
    let authorized_holding = Uuid::new_v4();
    let requested_holding = Uuid::new_v4();
    let now = at();
    let authorized_scope = format!("holding:{authorized_holding}");
    sqlx::query(
        "INSERT INTO erpx_identity.directory_connectors
            (id, provider, vendor, scope_key, scope_kind, scope_id, data_profile_key,
             endpoint, base_dn, tls_mode, security, page_size, max_in_flight_pages,
             ca_certificate_sha256, server_name, minimum_tls_version, feed_kind,
             credential_ref, enabled, max_import_users, revision, lifecycle,
             created_at, updated_at)
         VALUES ($1,'generic_ldap','generic_ldap','platform','platform',NULL,NULL,
                 'ldaps://scope-access.example.test:636','dc=example,dc=test','ldaps','ldaps',
                 500,4,$2,'scope-access.example.test','1.2','polling',
                 'test-only:rbac-secret',true,1000000,1,'active',$3,$3)",
    )
    .bind(connector_id)
    .bind("d".repeat(64))
    .bind(now)
    .execute(&pool)
    .await
    .expect("persist platform connector");
    sqlx::query(
        "INSERT INTO erpx_identity.directory_identity_projection
            (identity_link, connector_id, principal_ref, active, groups_json,
             source_revision, last_feed_at, revoked_at, updated_at)
         VALUES ($1,$2,$3,true,$4,1,$5,NULL,$5)",
    )
    .bind("scope-access-immutable")
    .bind(connector_id)
    .bind("scope-access-principal")
    .bind(serde_json::json!(["group:scope-admin"]))
    .bind(now)
    .execute(&pool)
    .await
    .expect("persist active directory projection");
    let principal_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO erpx_identity.principals
            (id, principal_ref, status, authenticated_realm, principal_kind,
             created_at, updated_at)
         VALUES ($1,'scope-access-principal','active','internal_workforce','human',$2,$2)",
    )
    .bind(principal_id)
    .bind(now)
    .execute(&pool)
    .await
    .expect("persist canonical scope-access principal");
    sqlx::query(
        "INSERT INTO erpx_identity.directory_identity_links
            (connector_id, external_object_id, identity_link, principal_ref,
             active, source_revision, observed_at, revoked_at,
             immutable_authority, immutable_value, linked_at)
         VALUES ($1,'scope-access-object','scope-access-immutable',
                 'scope-access-principal',true,1,$2,NULL,
                 'ldap:entryUUID','scope-access-object',$2)",
    )
    .bind(connector_id)
    .bind(now)
    .execute(&pool)
    .await
    .expect("persist immutable scope-access identity link");
    sqlx::query(
        "INSERT INTO erpx_identity.directory_principal_bindings
            (id, connector_id, external_object_id, principal_id, active,
             revision, linked_by, linked_at, correlation_id)
         VALUES ($1,$2,'scope-access-object',$3,true,1,
                 'scope-access-admin',$4,$5)",
    )
    .bind(Uuid::new_v4())
    .bind(connector_id)
    .bind(principal_id)
    .bind(now)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .expect("persist explicit scope-access principal binding");
    sqlx::query(
        "INSERT INTO erpx_identity.directory_role_mappings
            (connector_id, group_ref, role_ref, scope_key, requires_mfa, active, revision, updated_at)
         VALUES ($1,'group:scope-admin','role:scope-admin',$2,false,true,1,$3)",
    )
    .bind(connector_id)
    .bind(&authorized_scope)
    .bind(now)
    .execute(&pool)
    .await
    .expect("persist holding-scoped role mapping");

    let store = SqlxIdentityStore::new(pool.clone());
    assert_eq!(
        store
            .directory_role_access(
                "scope-access-principal",
                "role:scope-admin",
                &format!("holding:{requested_holding}"),
                Some(&authorized_scope),
                now,
            )
            .await
            .expect("evaluate mismatched holding request"),
        DirectoryRoleAccess::Denied
    );
    assert_eq!(
        store
            .directory_role_access(
                "scope-access-principal",
                "role:scope-admin",
                &authorized_scope,
                Some(&authorized_scope),
                now,
            )
            .await
            .expect("evaluate matching holding request"),
        DirectoryRoleAccess::Allowed
    );
    pool.close().await;
}
