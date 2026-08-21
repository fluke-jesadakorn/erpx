use std::{env, error::Error, str::FromStr};

use chrono::Utc;
use erpx_kernel::AuthorityScope;
use serde_json::{Value, json};
use sqlx::{
    Postgres, Transaction,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

const READ_DATABASE_URL: &str = "ERPX_TEST_HOLDING_READ_DATABASE_URL";

#[test]
#[ignore = "requires the disposable PostgreSQL database prepared by cargo xtask verify-sqlx"]
fn directory_scope_containment_sqlx_acceptance() {
    let Some(read_url) = env::var(READ_DATABASE_URL)
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        eprintln!("Directory scope containment acceptance skipped: {READ_DATABASE_URL} is not set");
        return;
    };
    require_verify_database(&read_url)
        .expect("the read URL must target the verify-sqlx disposable database");

    sqlx::__rt::test_block_on(async move {
        run_acceptance(&read_url)
            .await
            .expect("Directory scope containment SQLx acceptance must pass");
    });
}

fn require_verify_database(url: &str) -> Result<(), Box<dyn Error>> {
    let options = PgConnectOptions::from_str(url)?;
    let database = options
        .get_database()
        .ok_or("the PostgreSQL URL does not name a database")?;
    if !database.starts_with("erpx_verify_tr_")
        || !database
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(format!(
            "refusing non-disposable Collaboration acceptance database {database}"
        )
        .into());
    }
    Ok(())
}

async fn run_acceptance(read_url: &str) -> Result<(), Box<dyn Error>> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(read_url)
        .await?;
    let mut tx = pool.begin().await?;

    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let branch_id = Uuid::new_v4();
    let department_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let sibling_department_id = Uuid::new_v4();
    let direct_department_id = Uuid::new_v4();
    let direct_team_id = Uuid::new_v4();
    let other_holding_id = Uuid::new_v4();
    let other_company_id = Uuid::new_v4();
    let now = Utc::now();

    let bound = sqlx::query_scalar::<_, String>("SELECT set_config('app.holding_id', $1, true)")
        .bind(holding_id.to_string())
        .fetch_one(&mut *tx)
        .await?;
    assert_eq!(bound, holding_id.to_string());

    insert_node(
        &mut tx,
        holding_id,
        holding_id,
        "holding",
        "active",
        None,
        &[],
        1,
        now,
    )
    .await?;
    insert_node(
        &mut tx,
        holding_id,
        direct_department_id,
        "department",
        "active",
        Some(company_id),
        &[holding_id, company_id],
        7,
        now,
    )
    .await?;
    insert_node(
        &mut tx,
        holding_id,
        direct_team_id,
        "team",
        "active",
        Some(direct_department_id),
        &[holding_id, company_id, direct_department_id],
        8,
        now,
    )
    .await?;
    insert_node(
        &mut tx,
        holding_id,
        company_id,
        "company",
        "active",
        Some(holding_id),
        &[holding_id],
        2,
        now,
    )
    .await?;
    insert_node(
        &mut tx,
        holding_id,
        branch_id,
        "branch",
        "active",
        Some(company_id),
        &[holding_id, company_id],
        3,
        now,
    )
    .await?;
    insert_node(
        &mut tx,
        holding_id,
        department_id,
        "department",
        "active",
        Some(branch_id),
        &[holding_id, company_id, branch_id],
        4,
        now,
    )
    .await?;
    insert_node(
        &mut tx,
        holding_id,
        team_id,
        "team",
        "active",
        Some(department_id),
        &[holding_id, company_id, branch_id, department_id],
        5,
        now,
    )
    .await?;
    insert_node(
        &mut tx,
        holding_id,
        sibling_department_id,
        "department",
        "active",
        Some(branch_id),
        &[holding_id, company_id, branch_id],
        6,
        now,
    )
    .await?;

    let holding = AuthorityScope::Holding { holding_id };
    let company = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let branch = AuthorityScope::Branch {
        holding_id,
        company_id,
        branch_id,
    };
    let department = AuthorityScope::Subtree {
        holding_id,
        organization_node_id: department_id,
    };
    let team = AuthorityScope::Subtree {
        holding_id,
        organization_node_id: team_id,
    };
    let sibling_department = AuthorityScope::Subtree {
        holding_id,
        organization_node_id: sibling_department_id,
    };

    assert!(contains(&mut tx, &company, &department).await?);
    assert!(contains(&mut tx, &branch, &team).await?);
    assert!(contains(&mut tx, &department, &team).await?);
    assert!(contains(&mut tx, &department, &department).await?);
    assert!(contains(&mut tx, &team, &team).await?);
    assert!(contains(&mut tx, &holding, &team).await?);
    assert!(!contains(&mut tx, &sibling_department, &team).await?);
    let direct_team = AuthorityScope::Subtree {
        holding_id,
        organization_node_id: direct_team_id,
    };
    let department_forged_as_branch = AuthorityScope::Branch {
        holding_id,
        company_id,
        branch_id: direct_department_id,
    };
    assert!(contains(&mut tx, &company, &direct_team).await?);
    assert!(!contains(&mut tx, &department_forged_as_branch, &direct_team).await?);

    let company_as_subtree = AuthorityScope::Subtree {
        holding_id,
        organization_node_id: company_id,
    };
    let branch_as_subtree = AuthorityScope::Subtree {
        holding_id,
        organization_node_id: branch_id,
    };
    let platform = AuthorityScope::Platform;
    assert!(!contains(&mut tx, &company_as_subtree, &team).await?);
    assert!(!contains(&mut tx, &branch_as_subtree, &team).await?);
    assert!(!contains(&mut tx, &platform, &company_as_subtree).await?);
    assert!(!contains(&mut tx, &platform, &branch_as_subtree).await?);
    assert!(!contains(&mut tx, &department, &company).await?);

    let cross_holding_company = AuthorityScope::Company {
        holding_id: other_holding_id,
        company_id: other_company_id,
    };
    assert!(!contains(&mut tx, &cross_holding_company, &team).await?);
    let missing_target = AuthorityScope::Subtree {
        holding_id,
        organization_node_id: Uuid::new_v4(),
    };
    assert!(!contains(&mut tx, &holding, &missing_target).await?);

    set_node_status(&mut tx, holding_id, team_id, "suspended").await?;
    assert!(!contains(&mut tx, &holding, &team).await?);
    set_node_status(&mut tx, holding_id, team_id, "active").await?;

    set_node_status(&mut tx, holding_id, branch_id, "archived").await?;
    assert!(!contains(&mut tx, &company, &team).await?);
    set_node_status(&mut tx, holding_id, branch_id, "active").await?;

    sqlx::query(
        "UPDATE erpx_authority_read.organization_nodes \
         SET node_kind='headquarters' WHERE holding_id=$1 AND node_id=$2",
    )
    .bind(holding_id)
    .bind(branch_id)
    .execute(&mut *tx)
    .await?;
    assert!(!contains(&mut tx, &company, &team).await?);
    sqlx::query(
        "UPDATE erpx_authority_read.organization_nodes \
         SET node_kind='branch' WHERE holding_id=$1 AND node_id=$2",
    )
    .bind(holding_id)
    .bind(branch_id)
    .execute(&mut *tx)
    .await?;

    let contradictory_company_id = Uuid::new_v4();
    sqlx::query(
        "UPDATE erpx_authority_read.organization_nodes \
         SET parent_node_id=$3, lineage_node_ids=ARRAY[$1,$3]::UUID[] \
         WHERE holding_id=$1 AND node_id=$2",
    )
    .bind(holding_id)
    .bind(branch_id)
    .bind(contradictory_company_id)
    .execute(&mut *tx)
    .await?;
    assert!(!contains(&mut tx, &company, &team).await?);
    sqlx::query(
        "UPDATE erpx_authority_read.organization_nodes \
         SET parent_node_id=$3, lineage_node_ids=ARRAY[$1,$3]::UUID[] \
         WHERE holding_id=$1 AND node_id=$2",
    )
    .bind(holding_id)
    .bind(branch_id)
    .bind(company_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "DELETE FROM erpx_authority_read.organization_nodes \
         WHERE holding_id=$1 AND node_id=$2",
    )
    .bind(holding_id)
    .bind(branch_id)
    .execute(&mut *tx)
    .await?;
    assert!(!contains(&mut tx, &company, &team).await?);
    insert_node(
        &mut tx,
        holding_id,
        branch_id,
        "branch",
        "active",
        Some(company_id),
        &[holding_id, company_id],
        9,
        now,
    )
    .await?;
    assert!(contains(&mut tx, &company, &team).await?);

    let branch_scope = AuthorityScope::Branch {
        holding_id,
        company_id,
        branch_id,
    };
    assert!(contains(&mut tx, &company, &branch_scope).await?);
    assert!(!contains(&mut tx, &department, &company).await?);

    let mut over_posted = serde_json::to_value(&company)?;
    over_posted["Company"]["extra"] = json!(true);
    assert!(!contains_json(&mut tx, &over_posted, &serde_json::to_value(&department)?,).await?);
    assert!(
        !contains_json(
            &mut tx,
            &Value::String("Platform".into()),
            &json!({"Subtree": {"holding_id": holding_id, "organization_node_id": "bad"}}),
        )
        .await?
    );
    assert!(
        !contains_json(
            &mut tx,
            &Value::String("Platform".into()),
            &json!({"Unknown": {}}),
        )
        .await?
    );

    tx.rollback().await?;
    pool.close().await;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_node(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: Uuid,
    node_id: Uuid,
    node_kind: &str,
    node_status: &str,
    parent_node_id: Option<Uuid>,
    lineage_node_ids: &[Uuid],
    source_event_position: i64,
    now: chrono::DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO erpx_authority_read.organization_nodes \
         (holding_id,node_id,node_kind,node_status,parent_node_id,lineage_node_ids, \
          aggregate_version,source_event_position,changed_at,projected_at) \
         VALUES ($1,$2,$3,$4,$5,$6,1,$7,$8,$8)",
    )
    .bind(holding_id)
    .bind(node_id)
    .bind(node_kind)
    .bind(node_status)
    .bind(parent_node_id)
    .bind(lineage_node_ids)
    .bind(source_event_position)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn set_node_status(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: Uuid,
    node_id: Uuid,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE erpx_authority_read.organization_nodes SET node_status=$3 \
         WHERE holding_id=$1 AND node_id=$2",
    )
    .bind(holding_id)
    .bind(node_id)
    .bind(status)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn contains(
    tx: &mut Transaction<'_, Postgres>,
    authority: &AuthorityScope,
    target: &AuthorityScope,
) -> Result<bool, Box<dyn Error>> {
    contains_json(
        tx,
        &serde_json::to_value(authority)?,
        &serde_json::to_value(target)?,
    )
    .await
}

async fn contains_json(
    tx: &mut Transaction<'_, Postgres>,
    authority: &Value,
    target: &Value,
) -> Result<bool, Box<dyn Error>> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT erpx_collaboration_read.scope_contains($1::JSONB,$2::JSONB)",
    )
    .bind(authority)
    .bind(target)
    .fetch_one(&mut **tx)
    .await?)
}
