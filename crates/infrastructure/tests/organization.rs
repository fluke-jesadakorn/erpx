use chrono::Utc;
use erpx_domain::{
    contracts::{ActorAttribution, ActorKind},
    operations::OrganizationStatus,
};
use erpx_infrastructure::{
    OrganizationNodeCommand, OrganizationNodeKind, SqlxOrganizationStore, tenant_holding_scope_id,
};
use erpx_migration::{migrate_tenant_read, migrate_tenant_write};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

fn actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "organization-test-owner".into(),
        kind: ActorKind::TenantUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn command(
    kind: OrganizationNodeKind,
    node_id: Uuid,
    holding_id: Uuid,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    department_id: Option<Uuid>,
    code: &str,
    name: &str,
    expected_version: Option<u64>,
    key: &str,
) -> OrganizationNodeCommand {
    OrganizationNodeCommand {
        kind,
        node_id,
        holding_ref: "holding-organization-test".into(),
        holding_id,
        company_id,
        branch_id,
        department_id,
        code: code.into(),
        name: name.into(),
        registration_number: format!("REG-{code}"),
        status: OrganizationStatus::Active,
        expected_version,
        idempotency_key: key.into(),
        actor: actor(),
        correlation_id: Uuid::new_v4(),
        at: Utc::now(),
    }
}

#[test]
fn organization_node_kind_is_strict_and_transport_neutral() {
    assert_eq!(
        OrganizationNodeKind::parse("company").unwrap().as_str(),
        "company"
    );
    assert!(OrganizationNodeKind::parse("Company").is_err());
    assert_eq!(
        OrganizationNodeKind::Branch.aggregate_type(),
        "organization.branch"
    );
}

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases"]
async fn organization_write_read_projection_is_tenant_bound_idempotent_and_versioned() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required for this ignored test");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required for this ignored test");
    let write_pool = PgPoolOptions::new()
        .max_connections(3)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    let read_pool = PgPoolOptions::new()
        .max_connections(3)
        .connect(&read_url)
        .await
        .expect("tenant read pool");
    migrate_tenant_write(&write_pool)
        .await
        .expect("tenant write migrations");
    migrate_tenant_read(&read_pool)
        .await
        .expect("tenant read migrations");

    let store =
        SqlxOrganizationStore::from_pools(&write_pool, &read_pool, "holding-organization-test")
            .expect("organization store");
    let holding_id = tenant_holding_scope_id("holding-organization-test");
    let company_id = Uuid::from_u128(0x8101);
    let headquarters_id = Uuid::from_u128(0x8102);
    let branch_id = Uuid::from_u128(0x8103);
    let department_id = Uuid::from_u128(0x8104);
    let team_id = Uuid::from_u128(0x8105);

    let holding = store
        .save(&command(
            OrganizationNodeKind::Holding,
            holding_id,
            holding_id,
            None,
            None,
            None,
            "ORG",
            "Organization Test Holding",
            None,
            "organization-holding-v1",
        ))
        .await
        .expect("holding write");
    assert!(holding.applied);
    assert_eq!(holding.aggregate_version, 1);

    for (kind, node_id, parent, code, name, key) in [
        (
            OrganizationNodeKind::Company,
            company_id,
            None,
            "CO",
            "Organization Test Company",
            "organization-company-v1",
        ),
        (
            OrganizationNodeKind::Headquarters,
            headquarters_id,
            Some(company_id),
            "HQ",
            "Organization Test Headquarters",
            "organization-hq-v1",
        ),
        (
            OrganizationNodeKind::Branch,
            branch_id,
            Some(company_id),
            "BR",
            "Organization Test Branch",
            "organization-branch-v1",
        ),
    ] {
        let value = store
            .save(&command(
                kind, node_id, holding_id, parent, None, None, code, name, None, key,
            ))
            .await
            .expect("organization node write");
        assert!(value.applied);
        assert_eq!(value.aggregate_version, 1);
    }
    for (kind, node_id, branch, department, name, key) in [
        (
            OrganizationNodeKind::Department,
            department_id,
            Some(branch_id),
            None,
            "Organization Test Department",
            "organization-department-v1",
        ),
        (
            OrganizationNodeKind::Team,
            team_id,
            Some(branch_id),
            Some(department_id),
            "Organization Test Team",
            "organization-team-v1",
        ),
    ] {
        let value = store
            .save(&command(
                kind,
                node_id,
                holding_id,
                Some(company_id),
                branch,
                department,
                "NODE",
                name,
                None,
                key,
            ))
            .await
            .expect("nested organization node write");
        assert!(value.applied);
        assert_eq!(value.aggregate_version, 1);
    }

    let missing_company_parent = store
        .save(&command(
            OrganizationNodeKind::Headquarters,
            Uuid::from_u128(0x8110),
            holding_id,
            Some(Uuid::from_u128(0x81ff)),
            None,
            None,
            "HQ-MISSING",
            "Missing Parent Headquarters",
            None,
            "organization-missing-company-parent",
        ))
        .await
        .expect_err("organization children require an authoritative company parent");
    assert!(matches!(
        missing_company_parent,
        erpx_infrastructure::PersistenceError::InvalidData(message)
            if message.contains("company parent does not exist")
    ));

    let replay = store
        .save(&command(
            OrganizationNodeKind::Holding,
            holding_id,
            holding_id,
            None,
            None,
            None,
            "ORG",
            "Organization Test Holding",
            None,
            "organization-holding-v1",
        ))
        .await
        .expect("idempotent holding replay");
    assert!(!replay.applied);
    assert_eq!(replay.aggregate_version, 1);

    let page = store
        .list(None, None, 100)
        .await
        .expect("organization read page");
    assert!(!page.has_more);
    assert_eq!(page.nodes.len(), 6);
    assert!(page.nodes.iter().all(|node| node.holding_id == holding_id));
    assert_eq!(
        store
            .list(Some(OrganizationNodeKind::Branch), None, 100)
            .await
            .expect("branch read page")
            .nodes
            .len(),
        1
    );

    let cross_scope = SqlxOrganizationStore::from_pools(
        &write_pool,
        &read_pool,
        "holding-other-organization-test",
    )
    .expect("other holding store");
    let foreign_page = cross_scope
        .list(None, None, 100)
        .await
        .expect("foreign holding route is fail-closed");
    assert!(foreign_page.nodes.is_empty());
    assert!(!foreign_page.has_more);
    assert!(foreign_page.next_cursor.is_none());
}
