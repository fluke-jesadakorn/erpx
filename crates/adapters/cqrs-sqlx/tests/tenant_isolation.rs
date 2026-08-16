use std::{env, error::Error};

use erpx_cqrs_sqlx::{ProjectionPool, WritePool};
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use uuid::Uuid;

const WRITE_DATABASE_URL: &str = "ERPX_TEST_TENANT_WRITE_DATABASE_URL";
const READ_DATABASE_URL: &str = "ERPX_TEST_TENANT_READ_DATABASE_URL";

#[test]
#[ignore = "requires disposable generation-v2 tenant write/read PostgreSQL URLs"]
fn tenant_cqrs_rls_acceptance() {
    let (Some(write_url), Some(read_url)) = (
        configured_url(WRITE_DATABASE_URL),
        configured_url(READ_DATABASE_URL),
    ) else {
        panic!(
            "set ERPX_TEST_TENANT_WRITE_DATABASE_URL and \
             ERPX_TEST_TENANT_READ_DATABASE_URL to certify tenant RLS"
        );
    };

    sqlx::__rt::test_block_on(async move {
        run_acceptance(&write_url, &read_url)
            .await
            .expect("tenant CQRS RLS acceptance must pass");
    });
}

fn configured_url(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

async fn run_acceptance(write_url: &str, read_url: &str) -> Result<(), Box<dyn Error>> {
    let write = PgPoolOptions::new()
        .max_connections(4)
        .connect(write_url)
        .await?;
    let read = PgPoolOptions::new()
        .max_connections(4)
        .connect(read_url)
        .await?;
    if !contracts_installed(&write, &read).await? {
        return Err("generation-two tenant CQRS contracts are not installed".into());
    }
    if current_role_bypasses_rls(&write).await? || current_role_bypasses_rls(&read).await? {
        return Err("tenant RLS certification role must not bypass RLS".into());
    }

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let command_id = Uuid::new_v4();
    let correlation_id = Uuid::new_v4();
    let projection = format!("cqrs.rls.acceptance.{command_id}");
    let generation_a = Uuid::new_v4();
    let generation_b = Uuid::new_v4();

    insert_receipt(&write, tenant_a, command_id, correlation_id).await?;
    insert_receipt(&write, tenant_b, command_id, correlation_id).await?;
    assert_eq!(
        receipt_count(&write, tenant_a, tenant_a, command_id).await?,
        1
    );
    assert_eq!(
        receipt_count(&write, tenant_b, tenant_a, command_id).await?,
        0
    );
    assert_eq!(
        missing_tenant_receipt_count(&write, tenant_a, command_id).await?,
        0
    );

    insert_generation(&read, tenant_a, &projection, generation_a).await?;
    insert_generation(&read, tenant_b, &projection, generation_b).await?;
    assert_eq!(
        generation_count(&read, tenant_a, tenant_a, &projection).await?,
        1
    );
    assert_eq!(
        generation_count(&read, tenant_b, tenant_a, &projection).await?,
        0
    );
    assert!(
        insert_generation(&read, tenant_a, &projection, Uuid::new_v4())
            .await
            .is_err()
    );

    delete_receipt(&write, tenant_a, command_id).await?;
    delete_receipt(&write, tenant_b, command_id).await?;
    delete_generation(&read, tenant_a, &projection).await?;
    delete_generation(&read, tenant_b, &projection).await?;
    write.close().await;
    read.close().await;
    Ok(())
}

async fn contracts_installed(write: &PgPool, read: &PgPool) -> Result<bool, sqlx::Error> {
    let write_ready: bool =
        sqlx::query_scalar("SELECT to_regclass('erpx_cqrs_write.command_receipts') IS NOT NULL")
            .fetch_one(write)
            .await?;
    let read_ready: bool = sqlx::query_scalar(
        "SELECT to_regclass('erpx_cqrs_read.projection_generations') IS NOT NULL",
    )
    .fetch_one(read)
    .await?;
    Ok(write_ready && read_ready)
}

async fn current_role_bypasses_rls(pool: &PgPool) -> Result<bool, sqlx::Error> {
    let row =
        sqlx::query("SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user")
            .fetch_one(pool)
            .await?;
    Ok(row.try_get::<bool, _>("rolsuper")? || row.try_get::<bool, _>("rolbypassrls")?)
}

async fn insert_receipt(
    pool: &PgPool,
    tenant_id: Uuid,
    command_id: Uuid,
    correlation_id: Uuid,
) -> Result<(), Box<dyn Error>> {
    let capability = WritePool::for_tenant(pool.clone(), tenant_id)?;
    let mut transaction = capability.begin().await?;
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.command_receipts
            (tenant_id, context_name, partition_key, idempotency_key, command_id,
             request_hash, correlation_id, status, created_at)
         VALUES ($1, 'rls_acceptance', 'shared', 'shared', $2,
                 decode(repeat('01', 32), 'hex'), $3, 'reserved', now())",
    )
    .bind(tenant_id)
    .bind(command_id)
    .bind(correlation_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn receipt_count(
    pool: &PgPool,
    authority: Uuid,
    target: Uuid,
    command_id: Uuid,
) -> Result<i64, Box<dyn Error>> {
    let capability = WritePool::for_tenant(pool.clone(), authority)?;
    let mut transaction = capability.begin().await?;
    let count = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_cqrs_write.command_receipts
         WHERE tenant_id = $1 AND command_id = $2",
    )
    .bind(target)
    .bind(command_id)
    .fetch_one(&mut *transaction)
    .await?;
    Ok(count)
}

async fn missing_tenant_receipt_count(
    pool: &PgPool,
    target: Uuid,
    command_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_cqrs_write.command_receipts
         WHERE tenant_id = $1 AND command_id = $2",
    )
    .bind(target)
    .bind(command_id)
    .fetch_one(pool)
    .await
}

async fn delete_receipt(
    pool: &PgPool,
    tenant_id: Uuid,
    command_id: Uuid,
) -> Result<(), Box<dyn Error>> {
    let capability = WritePool::for_tenant(pool.clone(), tenant_id)?;
    let mut transaction = capability.begin().await?;
    sqlx::query(
        "DELETE FROM erpx_cqrs_write.command_receipts
         WHERE tenant_id = $1 AND command_id = $2",
    )
    .bind(tenant_id)
    .bind(command_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn insert_generation(
    pool: &PgPool,
    tenant_id: Uuid,
    projection: &str,
    generation: Uuid,
) -> Result<(), Box<dyn Error>> {
    let capability = ProjectionPool::for_tenant(pool.clone(), tenant_id)?;
    let mut transaction = capability.begin().await?;
    sqlx::query(
        "INSERT INTO erpx_cqrs_read.projection_generations
            (tenant_id, projection_name, generation, status, created_at)
         VALUES ($1, $2, $3, 'active', now())",
    )
    .bind(tenant_id)
    .bind(projection)
    .bind(generation)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn generation_count(
    pool: &PgPool,
    authority: Uuid,
    target: Uuid,
    projection: &str,
) -> Result<i64, Box<dyn Error>> {
    let capability = ProjectionPool::for_tenant(pool.clone(), authority)?;
    let mut transaction = capability.begin().await?;
    let count = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_cqrs_read.projection_generations
         WHERE tenant_id = $1 AND projection_name = $2",
    )
    .bind(target)
    .bind(projection)
    .fetch_one(&mut *transaction)
    .await?;
    Ok(count)
}

async fn delete_generation(
    pool: &PgPool,
    tenant_id: Uuid,
    projection: &str,
) -> Result<(), Box<dyn Error>> {
    let capability = ProjectionPool::for_tenant(pool.clone(), tenant_id)?;
    let mut transaction = capability.begin().await?;
    sqlx::query(
        "DELETE FROM erpx_cqrs_read.projection_generations
         WHERE tenant_id = $1 AND projection_name = $2",
    )
    .bind(tenant_id)
    .bind(projection)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}
