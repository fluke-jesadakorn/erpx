use std::{env, error::Error, str::FromStr, time::Duration};

use erpx_cqrs_sqlx::{ProjectionPool, ProjectionSource, WritePool};
use erpx_kernel::{ActorAttribution, ActorKind, AuthorityScope};
use sqlx::{
    PgPool, Postgres, Row, Transaction,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

const WRITE_DATABASE_URL: &str = "ERPX_TEST_RLS_HOLDING_WRITE_DATABASE_URL";
const READ_DATABASE_URL: &str = "ERPX_TEST_RLS_HOLDING_READ_DATABASE_URL";
const CONTROL_WRITE_DATABASE_URL: &str = "ERPX_TEST_CONTROL_WRITE_DATABASE_URL";

#[test]
#[ignore = "requires disposable current holding write/read PostgreSQL URLs"]
fn holding_cqrs_rls_acceptance() {
    let (Some(write_url), Some(read_url)) = (
        configured_url(WRITE_DATABASE_URL),
        configured_url(READ_DATABASE_URL),
    ) else {
        panic!(
            "set ERPX_TEST_RLS_HOLDING_WRITE_DATABASE_URL and \
             ERPX_TEST_RLS_HOLDING_READ_DATABASE_URL to certify holding RLS"
        );
    };

    sqlx::__rt::test_block_on(async move {
        run_acceptance(&write_url, &read_url)
            .await
            .expect("holding CQRS RLS acceptance must pass");
    });
}

#[test]
#[ignore = "requires the disposable control-write PostgreSQL URL prepared by verify-sqlx"]
fn control_cqrs_outbox_commit_order_acceptance() {
    let write_url = configured_url(CONTROL_WRITE_DATABASE_URL)
        .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL must be configured");
    require_verify_database(&write_url, "erpx_verify_cw_")
        .expect("the control-write URL must target the verify-sqlx disposable database");

    sqlx::__rt::test_block_on(async move {
        let write = PgPoolOptions::new()
            .max_connections(4)
            .connect(&write_url)
            .await
            .expect("control-write acceptance pool");
        assert_control_outbox_sequences_follow_commit_order(&write)
            .await
            .expect("control outbox sequences must follow commit order");
        write.close().await;
    });
}

fn configured_url(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn require_verify_database(url: &str, prefix: &str) -> Result<(), Box<dyn Error>> {
    let options = PgConnectOptions::from_str(url)?;
    let database = options
        .get_database()
        .ok_or("the PostgreSQL URL does not name a database")?;
    if !database.starts_with(prefix)
        || !database
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(format!("refusing non-disposable CQRS acceptance database {database}").into());
    }
    Ok(())
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
        return Err("current holding CQRS contracts are not installed".into());
    }
    if current_role_bypasses_rls(&write).await? || current_role_bypasses_rls(&read).await? {
        return Err("holding RLS certification role must not bypass RLS".into());
    }

    let holding_a = Uuid::new_v4();
    let holding_b = Uuid::new_v4();
    let command_id = Uuid::new_v4();
    let correlation_id = Uuid::new_v4();
    let projection = format!("cqrs.rls.acceptance.{command_id}");
    let generation_a = Uuid::new_v4();
    let generation_b = Uuid::new_v4();

    insert_receipt(&write, holding_a, command_id, correlation_id).await?;
    insert_receipt(&write, holding_b, command_id, correlation_id).await?;
    assert_eq!(
        receipt_count(&write, holding_a, holding_a, command_id).await?,
        1
    );
    assert_eq!(
        receipt_count(&write, holding_b, holding_a, command_id).await?,
        0
    );
    assert_eq!(
        missing_holding_receipt_count(&write, holding_a, command_id).await?,
        0
    );
    assert_outbox_sequences_follow_commit_order(&write, holding_a).await?;

    insert_generation(&read, holding_a, &projection, generation_a).await?;
    insert_generation(&read, holding_b, &projection, generation_b).await?;
    assert_eq!(
        generation_count(&read, holding_a, holding_a, &projection).await?,
        1
    );
    assert_eq!(
        generation_count(&read, holding_b, holding_a, &projection).await?,
        0
    );
    assert!(
        insert_generation(&read, holding_a, &projection, Uuid::new_v4())
            .await
            .is_err()
    );

    delete_receipt(&write, holding_a, command_id).await?;
    delete_receipt(&write, holding_b, command_id).await?;
    delete_generation(&read, holding_a, &projection).await?;
    delete_generation(&read, holding_b, &projection).await?;
    write.close().await;
    read.close().await;
    Ok(())
}

async fn assert_outbox_sequences_follow_commit_order(
    pool: &PgPool,
    holding_id: Uuid,
) -> Result<(), Box<dyn Error>> {
    let capability = WritePool::for_holding(pool.clone(), holding_id)?;
    let source = ProjectionSource::new(capability.clone());
    let event_type = format!("cqrs.commit_order.{}", Uuid::new_v4());
    let event_types = vec![event_type.clone()];
    let first_event_id = Uuid::new_v4();
    let second_event_id = Uuid::new_v4();
    assert_eq!(source.durable_high_watermark(&event_types).await?, 0);

    let mut first = capability.begin().await?;
    let first_backend_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *first)
        .await?;
    let first_sequence =
        insert_outbox_event(&mut first, holding_id, first_event_id, &event_type).await?;

    let second_capability = capability.clone();
    let second_event_type = event_type.clone();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let second = tokio::spawn(async move {
        let mut transaction = second_capability
            .begin()
            .await
            .map_err(|error| error.to_string())?;
        let backend_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| error.to_string())?;
        started_tx
            .send(backend_pid)
            .map_err(|_| "commit-order test coordinator stopped".to_owned())?;
        let sequence = insert_outbox_event(
            &mut transaction,
            holding_id,
            second_event_id,
            &second_event_type,
        )
        .await
        .map_err(|error| error.to_string())?;
        transaction
            .commit()
            .await
            .map_err(|error| error.to_string())?;
        Ok::<_, String>(sequence)
    });
    let second_backend_pid = tokio::time::timeout(Duration::from_secs(5), started_rx)
        .await
        .map_err(|_| "second outbox transaction did not start")??;

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let blocked_by_first: bool = sqlx::query_scalar(
                "SELECT $1::INTEGER = ANY(pg_catalog.pg_blocking_pids($2::INTEGER))",
            )
            .bind(first_backend_pid)
            .bind(second_backend_pid)
            .fetch_one(pool)
            .await?;
            if blocked_by_first {
                return Ok::<_, sqlx::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| "later outbox transaction was not blocked by the sequence allocator")??;
    assert!(!second.is_finished());
    assert_eq!(source.durable_high_watermark(&event_types).await?, 0);

    first.commit().await?;
    let second_sequence = tokio::time::timeout(Duration::from_secs(5), second)
        .await
        .map_err(|_| "later outbox transaction did not finish after the first commit")???;
    assert!(first_sequence < second_sequence);

    let high_watermark = source.durable_high_watermark(&event_types).await?;
    assert_eq!(high_watermark, second_sequence);
    let page = source
        .page_durable_between(0, Some(high_watermark), &event_types, 10)
        .await?;
    assert_eq!(
        page.events
            .iter()
            .map(|event| event.event_id.as_uuid())
            .collect::<Vec<_>>(),
        vec![first_event_id, second_event_id]
    );

    let mut cleanup = capability.begin().await?;
    sqlx::query(
        "DELETE FROM erpx_cqrs_write.outbox_events
         WHERE holding_id=$1 AND event_id=ANY($2::UUID[])",
    )
    .bind(holding_id)
    .bind([first_event_id, second_event_id])
    .execute(&mut *cleanup)
    .await?;
    cleanup.commit().await?;
    Ok(())
}

async fn assert_control_outbox_sequences_follow_commit_order(
    pool: &PgPool,
) -> Result<(), Box<dyn Error>> {
    let capability = WritePool::from_control_pg_pool(pool.clone());
    let source = ProjectionSource::new(capability.clone());
    let event_type = format!("cqrs.control.commit_order.{}", Uuid::new_v4());
    let event_types = vec![event_type.clone()];
    let first_event_id = Uuid::new_v4();
    let second_event_id = Uuid::new_v4();
    assert_eq!(source.durable_high_watermark(&event_types).await?, 0);

    let mut first = capability.begin().await?;
    let first_backend_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *first)
        .await?;
    let first_sequence =
        insert_control_outbox_event(&mut first, first_event_id, &event_type).await?;

    let second_capability = capability.clone();
    let second_event_type = event_type.clone();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let second = tokio::spawn(async move {
        let mut transaction = second_capability
            .begin()
            .await
            .map_err(|error| error.to_string())?;
        let backend_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| error.to_string())?;
        started_tx
            .send(backend_pid)
            .map_err(|_| "control commit-order test coordinator stopped".to_owned())?;
        let sequence =
            insert_control_outbox_event(&mut transaction, second_event_id, &second_event_type)
                .await
                .map_err(|error| error.to_string())?;
        transaction
            .commit()
            .await
            .map_err(|error| error.to_string())?;
        Ok::<_, String>(sequence)
    });
    let second_backend_pid = tokio::time::timeout(Duration::from_secs(5), started_rx)
        .await
        .map_err(|_| "second control outbox transaction did not start")??;

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let blocked_by_first: bool = sqlx::query_scalar(
                "SELECT $1::INTEGER = ANY(pg_catalog.pg_blocking_pids($2::INTEGER))",
            )
            .bind(first_backend_pid)
            .bind(second_backend_pid)
            .fetch_one(pool)
            .await?;
            if blocked_by_first {
                return Ok::<_, sqlx::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| "later control outbox transaction was not blocked by the allocator")??;
    assert!(!second.is_finished());
    assert_eq!(source.durable_high_watermark(&event_types).await?, 0);

    first.commit().await?;
    let second_sequence = tokio::time::timeout(Duration::from_secs(5), second)
        .await
        .map_err(|_| "later control outbox transaction did not finish")???;
    assert!(first_sequence < second_sequence);

    let high_watermark = source.durable_high_watermark(&event_types).await?;
    assert_eq!(high_watermark, second_sequence);
    let page = source
        .page_durable_between(0, Some(high_watermark), &event_types, 10)
        .await?;
    assert_eq!(
        page.events
            .iter()
            .map(|event| event.event_id.as_uuid())
            .collect::<Vec<_>>(),
        vec![first_event_id, second_event_id]
    );

    let mut cleanup = capability.begin().await?;
    sqlx::query("DELETE FROM erpx_cqrs_write.outbox_events WHERE event_id=ANY($1::UUID[])")
        .bind([first_event_id, second_event_id])
        .execute(&mut *cleanup)
        .await?;
    cleanup.commit().await?;
    Ok(())
}

async fn insert_outbox_event(
    transaction: &mut Transaction<'_, Postgres>,
    holding_id: Uuid,
    event_id: Uuid,
    event_type: &str,
) -> Result<u64, String> {
    let command_id = Uuid::new_v4();
    let aggregate_id = Uuid::new_v4();
    let active_scope = serde_json::to_value(AuthorityScope::Holding { holding_id })
        .map_err(|error| error.to_string())?;
    let actor = serde_json::to_value(ActorAttribution {
        principal_ref: format!("principal:{holding_id}"),
        kind: ActorKind::HoldingUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    })
    .map_err(|error| error.to_string())?;
    let sequence: i64 = sqlx::query_scalar(
        "INSERT INTO erpx_cqrs_write.outbox_events
            (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,
             aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,
             causation_id,payload,occurred_at,available_at)
         VALUES ($1,$2,'cqrs_commit_order',$3,$4,$5,1,'commit_order_probe',$6,1,$7,$8,$9,
                 NULL,'{}'::JSONB,clock_timestamp(),clock_timestamp())
         RETURNING global_sequence",
    )
    .bind(holding_id)
    .bind(event_id)
    .bind(format!("commit-order:{event_id}"))
    .bind(command_id)
    .bind(event_type)
    .bind(aggregate_id)
    .bind(active_scope)
    .bind(actor)
    .bind(Uuid::new_v4())
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| error.to_string())?;
    u64::try_from(sequence).map_err(|error| error.to_string())
}

async fn insert_control_outbox_event(
    transaction: &mut Transaction<'_, Postgres>,
    event_id: Uuid,
    event_type: &str,
) -> Result<u64, String> {
    let command_id = Uuid::new_v4();
    let aggregate_id = Uuid::new_v4();
    let active_scope =
        serde_json::to_value(AuthorityScope::Platform).map_err(|error| error.to_string())?;
    let actor = serde_json::to_value(ActorAttribution {
        principal_ref: "platform:commit-order-acceptance".into(),
        kind: ActorKind::PlatformUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    })
    .map_err(|error| error.to_string())?;
    let sequence: i64 = sqlx::query_scalar(
        "INSERT INTO erpx_cqrs_write.outbox_events
            (event_id,context_name,partition_key,command_id,event_type,event_version,
             aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,
             causation_id,payload,occurred_at,available_at)
         VALUES ($1,'cqrs_control_commit_order',$2,$3,$4,1,'commit_order_probe',$5,1,
                 $6,$7,$8,NULL,'{}'::JSONB,clock_timestamp(),clock_timestamp())
         RETURNING global_sequence",
    )
    .bind(event_id)
    .bind(format!("control-commit-order:{event_id}"))
    .bind(command_id)
    .bind(event_type)
    .bind(aggregate_id)
    .bind(active_scope)
    .bind(actor)
    .bind(Uuid::new_v4())
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| error.to_string())?;
    u64::try_from(sequence).map_err(|error| error.to_string())
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
    holding_id: Uuid,
    command_id: Uuid,
    correlation_id: Uuid,
) -> Result<(), Box<dyn Error>> {
    let capability = WritePool::for_holding(pool.clone(), holding_id)?;
    let mut transaction = capability.begin().await?;
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.command_receipts
            (holding_id, context_name, partition_key, idempotency_key, command_id,
             request_hash, correlation_id, status, created_at)
         VALUES ($1, 'rls_acceptance', 'shared', 'shared', $2,
                 decode(repeat('01', 32), 'hex'), $3, 'reserved', now())",
    )
    .bind(holding_id)
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
    let capability = WritePool::for_holding(pool.clone(), authority)?;
    let mut transaction = capability.begin().await?;
    let count = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_cqrs_write.command_receipts
         WHERE holding_id = $1 AND command_id = $2",
    )
    .bind(target)
    .bind(command_id)
    .fetch_one(&mut *transaction)
    .await?;
    Ok(count)
}

async fn missing_holding_receipt_count(
    pool: &PgPool,
    target: Uuid,
    command_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_cqrs_write.command_receipts
         WHERE holding_id = $1 AND command_id = $2",
    )
    .bind(target)
    .bind(command_id)
    .fetch_one(pool)
    .await
}

async fn delete_receipt(
    pool: &PgPool,
    holding_id: Uuid,
    command_id: Uuid,
) -> Result<(), Box<dyn Error>> {
    let capability = WritePool::for_holding(pool.clone(), holding_id)?;
    let mut transaction = capability.begin().await?;
    sqlx::query(
        "DELETE FROM erpx_cqrs_write.command_receipts
         WHERE holding_id = $1 AND command_id = $2",
    )
    .bind(holding_id)
    .bind(command_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn insert_generation(
    pool: &PgPool,
    holding_id: Uuid,
    projection: &str,
    generation: Uuid,
) -> Result<(), Box<dyn Error>> {
    let capability = ProjectionPool::for_holding(pool.clone(), holding_id)?;
    let mut transaction = capability.begin().await?;
    sqlx::query(
        "INSERT INTO erpx_cqrs_read.projection_generations
            (holding_id, projection_name, generation, status, created_at)
         VALUES ($1, $2, $3, 'active', now())",
    )
    .bind(holding_id)
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
    let capability = ProjectionPool::for_holding(pool.clone(), authority)?;
    let mut transaction = capability.begin().await?;
    let count = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_cqrs_read.projection_generations
         WHERE holding_id = $1 AND projection_name = $2",
    )
    .bind(target)
    .bind(projection)
    .fetch_one(&mut *transaction)
    .await?;
    Ok(count)
}

async fn delete_generation(
    pool: &PgPool,
    holding_id: Uuid,
    projection: &str,
) -> Result<(), Box<dyn Error>> {
    let capability = ProjectionPool::for_holding(pool.clone(), holding_id)?;
    let mut transaction = capability.begin().await?;
    sqlx::query(
        "DELETE FROM erpx_cqrs_read.projection_generations
         WHERE holding_id = $1 AND projection_name = $2",
    )
    .bind(holding_id)
    .bind(projection)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}
