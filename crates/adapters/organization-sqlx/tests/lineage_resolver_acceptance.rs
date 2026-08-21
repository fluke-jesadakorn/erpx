use std::{env, error::Error, str::FromStr};

use chrono::Utc;
use erpx_cqrs_sqlx::WritePool;
use erpx_integration_contracts::organization::{
    OrganizationLineageKey, OrganizationNodeKind, OrganizationNodeStatus,
};
use erpx_kernel::AuthorityScope;
use erpx_organization::{OrganizationNodeId, OrganizationPlacement, ScopeContainmentPort};
use erpx_organization_sqlx::{OrganizationLineageResolver, OrganizationLineageResolverError};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use uuid::Uuid;

const WRITE_DATABASE_URL: &str = "ERPX_TEST_HOLDING_WRITE_DATABASE_URL";

#[test]
#[ignore = "requires a disposable PostgreSQL database prepared by cargo xtask verify-sqlx"]
fn organization_lineage_resolver_sqlx_acceptance() {
    let Some(write_url) = configured_url(WRITE_DATABASE_URL) else {
        eprintln!(
            "Organization lineage resolver SQLx acceptance skipped: {WRITE_DATABASE_URL} is not set"
        );
        return;
    };
    require_verify_database(&write_url, "erpx_verify_tw_")
        .expect("the write URL must target the verify-sqlx disposable database");

    sqlx::__rt::test_block_on(async move {
        run_acceptance(&write_url)
            .await
            .expect("Organization lineage resolver SQLx acceptance must pass");
    });
}

fn configured_url(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn require_verify_database(url: &str, expected_prefix: &str) -> Result<(), Box<dyn Error>> {
    let options = PgConnectOptions::from_str(url)?;
    let database = options
        .get_database()
        .ok_or("the PostgreSQL URL does not name a database")?;
    if !database.starts_with(expected_prefix)
        || !database
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(
            format!("refusing non-disposable Organization acceptance database {database}").into(),
        );
    }
    Ok(())
}

async fn run_acceptance(write_url: &str) -> Result<(), Box<dyn Error>> {
    let database = PgPoolOptions::new()
        .max_connections(4)
        .connect(write_url)
        .await?;
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let branch_id = Uuid::new_v4();
    let department_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let write_pool = WritePool::for_holding(database.clone(), holding_id)?;

    insert_authority(
        &write_pool,
        AuthorityFixture {
            node_id: holding_id,
            kind: "holding",
            parent_node_id: None,
            lineage_node_ids: vec![],
            company_id: None,
            branch_id: None,
            department_id: None,
            code: "HOLDING",
        },
    )
    .await?;
    insert_authority(
        &write_pool,
        AuthorityFixture {
            node_id: company_id,
            kind: "company",
            parent_node_id: Some(holding_id),
            lineage_node_ids: vec![holding_id],
            company_id: Some(company_id),
            branch_id: None,
            department_id: None,
            code: "COMPANY",
        },
    )
    .await?;
    insert_authority(
        &write_pool,
        AuthorityFixture {
            node_id: branch_id,
            kind: "branch",
            parent_node_id: Some(company_id),
            lineage_node_ids: vec![holding_id, company_id],
            company_id: Some(company_id),
            branch_id: Some(branch_id),
            department_id: None,
            code: "BRANCH",
        },
    )
    .await?;
    insert_authority(
        &write_pool,
        AuthorityFixture {
            node_id: department_id,
            kind: "department",
            parent_node_id: Some(branch_id),
            lineage_node_ids: vec![holding_id, company_id, branch_id],
            company_id: Some(company_id),
            branch_id: Some(branch_id),
            department_id: Some(department_id),
            code: "DEPARTMENT",
        },
    )
    .await?;
    insert_authority(
        &write_pool,
        AuthorityFixture {
            node_id: team_id,
            kind: "team",
            parent_node_id: Some(department_id),
            lineage_node_ids: vec![holding_id, company_id, branch_id, department_id],
            company_id: Some(company_id),
            branch_id: Some(branch_id),
            department_id: Some(department_id),
            code: "TEAM",
        },
    )
    .await?;

    let holding_key = key(holding_id, holding_id)?;
    let company_key = key(holding_id, company_id)?;
    let branch_key = key(holding_id, branch_id)?;
    let department_key = key(holding_id, department_id)?;
    let team_key = key(holding_id, team_id)?;
    let resolver = OrganizationLineageResolver::new(write_pool.clone());
    let requested = [team_key, department_key, team_key, branch_key];
    let resolved = resolver.resolve_batch(&requested).await?;
    assert_eq!(
        resolved.iter().map(|value| value.key()).collect::<Vec<_>>(),
        requested,
        "output order and duplicate positions must match the request"
    );
    let team = &resolved[0];
    assert_eq!(team.kind(), OrganizationNodeKind::Team);
    assert_eq!(team.status(), OrganizationNodeStatus::Active);
    assert_eq!(team.aggregate_version(), 1);
    assert_eq!(
        team.lineage_node_ids()
            .iter()
            .map(|id| id.as_uuid())
            .collect::<Vec<_>>(),
        vec![holding_id, company_id, branch_id, department_id]
    );
    assert_eq!(team.company_id().map(|id| id.as_uuid()), Some(company_id));
    assert_eq!(team.branch_id().map(|id| id.as_uuid()), Some(branch_id));
    assert_eq!(
        team.department_id().map(|id| id.as_uuid()),
        Some(department_id)
    );
    assert!(team.is_within(&AuthorityScope::Company {
        holding_id,
        company_id,
    }));
    assert!(team.is_within(&AuthorityScope::Subtree {
        holding_id,
        organization_node_id: department_id,
    }));
    assert!(!team.is_within(&AuthorityScope::Company {
        holding_id,
        company_id: department_id,
    }));

    let team_node = OrganizationNodeId::try_from_uuid(team_id)?;
    let team_placement = OrganizationPlacement::Team {
        holding_id,
        company_id,
        branch_id: Some(branch_id),
        department_id,
    };
    let branch_scope = AuthorityScope::Branch {
        holding_id,
        company_id,
        branch_id,
    };
    let department_scope = AuthorityScope::Subtree {
        holding_id,
        organization_node_id: department_id,
    };
    assert!(
        resolver
            .contains_node(&branch_scope, team_node, &team_placement)
            .await?
    );
    assert!(
        resolver
            .contains_parent(&department_scope, team_node, &team_placement)
            .await?
    );
    assert_eq!(
        resolver
            .contains_nodes(
                &branch_scope,
                &[
                    (team_node, team_placement.clone()),
                    (team_node, team_placement.clone()),
                ],
            )
            .await?,
        vec![true, true],
        "a list page is resolved once while preserving duplicate positions"
    );

    for inactive_ancestor in [branch_id, department_id] {
        set_authority_status(&write_pool, inactive_ancestor, "suspended").await?;
        assert!(
            !resolver
                .contains_node(&branch_scope, team_node, &team_placement)
                .await?,
            "a Team command must fail closed below a suspended Branch or Department"
        );
        set_authority_status(&write_pool, inactive_ancestor, "active").await?;
    }

    let mut transaction = write_pool.begin().await?;
    let locked = resolver
        .resolve_batch_in_transaction(&mut transaction, &[team_key])
        .await?;
    assert_eq!(locked.len(), 1);
    let mut competing = write_pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout='250ms'")
        .execute(&mut *competing)
        .await?;
    let lock_error = sqlx::query(
        "UPDATE erpx_organization_write.organization_node_authority
         SET updated_at=updated_at
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND node_id=$1",
    )
    .bind(branch_id)
    .execute(&mut *competing)
    .await
    .expect_err("a competing ancestor update must wait for the resolver's share lock");
    assert_eq!(
        lock_error
            .as_database_error()
            .and_then(|error| error.code().map(|code| code.into_owned()))
            .as_deref(),
        Some("55P03"),
        "PostgreSQL must report lock_not_available after the bounded timeout"
    );
    competing.rollback().await?;
    transaction.commit().await?;

    let direct_ancestors = resolver
        .resolve_batch(&[holding_key, company_key, branch_key])
        .await?;
    assert_eq!(direct_ancestors.len(), 3);

    let missing_key = key(holding_id, Uuid::new_v4())?;
    assert_eq!(
        resolver.resolve_batch(&[team_key, missing_key]).await,
        Err(OrganizationLineageResolverError::Missing(missing_key)),
        "one missing row rejects the complete live-authorization batch"
    );
    let wrong_holding_key = key(Uuid::new_v4(), team_id)?;
    assert_eq!(
        resolver.resolve_batch(&[wrong_holding_key]).await,
        Err(OrganizationLineageResolverError::Missing(wrong_holding_key)),
        "the holding component is verified rather than treated as routing metadata"
    );

    let mut transaction = write_pool.begin().await?;
    sqlx::query(
        "UPDATE erpx_organization_write.organization_node_authority
         SET lineage_node_ids=$1
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND node_id=$2",
    )
    .bind(vec![holding_id, company_id, department_id])
    .bind(team_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    assert_eq!(
        resolver.resolve_batch(&[team_key]).await,
        Err(OrganizationLineageResolverError::Corrupt(team_key)),
        "the resolver re-reads authority and rejects a stale/corrupt lineage instead of using a persistent positive cache"
    );

    database.close().await;
    Ok(())
}

fn key(holding_id: Uuid, node_id: Uuid) -> Result<OrganizationLineageKey, Box<dyn Error>> {
    Ok(OrganizationLineageKey::try_new(holding_id, node_id)?)
}

struct AuthorityFixture {
    node_id: Uuid,
    kind: &'static str,
    parent_node_id: Option<Uuid>,
    lineage_node_ids: Vec<Uuid>,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    department_id: Option<Uuid>,
    code: &'static str,
}

async fn insert_authority(
    pool: &WritePool,
    fixture: AuthorityFixture,
) -> Result<(), Box<dyn Error>> {
    let now = Utc::now();
    let mut transaction = pool.begin().await?;
    sqlx::query(
        "INSERT INTO erpx_organization_write.organization_node_authority
            (holding_id,node_id,aggregate_version,node_kind,parent_node_id,
             lineage_node_ids,company_id,branch_id,department_id,node_code,
             node_name,node_status,created_at,updated_at)
         VALUES (erpx_cqrs_write.current_holding_id(),$1,1,$2,$3,$4,$5,$6,$7,
                 $8,$9,'active',$10,$10)",
    )
    .bind(fixture.node_id)
    .bind(fixture.kind)
    .bind(fixture.parent_node_id)
    .bind(fixture.lineage_node_ids)
    .bind(fixture.company_id)
    .bind(fixture.branch_id)
    .bind(fixture.department_id)
    .bind(fixture.code)
    .bind(format!("Lineage resolver {}", fixture.code))
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn set_authority_status(
    pool: &WritePool,
    node_id: Uuid,
    status: &str,
) -> Result<(), Box<dyn Error>> {
    let mut transaction = pool.begin().await?;
    sqlx::query(
        "UPDATE erpx_organization_write.organization_node_authority
         SET node_status=$1,updated_at=$2
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND node_id=$3",
    )
    .bind(status)
    .bind(Utc::now())
    .bind(node_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}
