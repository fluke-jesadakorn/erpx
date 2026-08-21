//! Deterministic proof that Item Directory resolve binds its effective-grant
//! proof and descriptor payload to one PostgreSQL statement snapshot.

use std::{env, error::Error, str::FromStr, time::Duration};

use chrono::{Duration as ChronoDuration, Utc};
use erpx_collaboration_sqlx::{
    DirectoryAccess, DirectoryDescriptorPublication, DirectoryGrantPublication, DirectoryLookup,
    DirectoryRelationPublication, DirectorySearchRequest, ItemDirectoryProjectionAdapter,
    SqlxItemDirectory,
};
use erpx_integration_contracts::{
    ItemDescriptor, RecordTypeId, StableRecordRef, catalog::ContextId, record_policy,
};
use erpx_kernel::{AuthorityScope, DataClassification, HoldingId, PrincipalId, RecordId};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use uuid::Uuid;

const READ_DATABASE_URL: &str = "ERPX_TEST_HOLDING_READ_DATABASE_URL";
const READER_APPLICATION_NAME: &str = "erpx_directory_atomic_reader";
const TEST_ADVISORY_LOCK: i64 = 9_181_004_201;
const ORIGINAL_SCOPE_FUNCTION: &str = "scope_contains_atomic_snapshot_test_base";

#[test]
#[ignore = "requires the disposable PostgreSQL database prepared by cargo xtask verify-sqlx"]
fn directory_resolve_uses_one_authorized_statement_snapshot() {
    let Some(read_url) = env::var(READ_DATABASE_URL)
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        eprintln!("Directory atomic snapshot acceptance skipped: {READ_DATABASE_URL} is not set");
        return;
    };
    require_verify_database(&read_url)
        .expect("the read URL must target the verify-sqlx disposable database");

    sqlx::__rt::test_block_on(async move {
        run_acceptance(&read_url)
            .await
            .expect("Directory atomic snapshot SQLx acceptance must pass");
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
    let control_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(read_url)
        .await?;
    let reader_options = PgConnectOptions::from_str(read_url)?
        .application_name(READER_APPLICATION_NAME)
        .statement_cache_capacity(0);
    let reader_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(reader_options)
        .await?;

    let fixture = Fixture::new()?;
    seed_directory(&reader_pool, &fixture).await?;
    prove_future_facts_are_checkpoint_fenced(&reader_pool, &fixture).await?;
    install_blocking_scope_wrapper(&control_pool).await?;
    let race_result = run_resolve_replacement_race(&control_pool, &reader_pool, &fixture).await;
    let restore_result = restore_scope_function(&control_pool).await;

    reader_pool.close().await;
    control_pool.close().await;
    race_result?;
    restore_result?;
    Ok(())
}

struct Fixture {
    holding_id: HoldingId,
    principal_id: PrincipalId,
    access: DirectoryAccess,
    record: StableRecordRef,
    future_record: StableRecordRef,
    relation_id: Uuid,
    original_descriptor: ItemDescriptor,
    future_descriptor: ItemDescriptor,
    replacement_descriptor: ItemDescriptor,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        let holding_id = HoldingId::try_from_uuid(Uuid::new_v4())?;
        let principal_id = PrincipalId::try_from_uuid(Uuid::new_v4())?;
        let scope = AuthorityScope::Holding {
            holding_id: holding_id.as_uuid(),
        };
        let access =
            DirectoryAccess::try_from_authorization(holding_id, principal_id, scope.clone())?;
        let record = StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4())?,
            ContextId::Procurement,
            RecordTypeId::try_new("procurement.purchase-order")?,
        )?;
        let policy = record_policy(record.record_type().as_str())
            .ok_or("purchase-order record policy must exist")?;
        let future_record = StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4())?,
            ContextId::Procurement,
            RecordTypeId::try_new("procurement.purchase-order")?,
        )?;
        let original_descriptor = ItemDescriptor::try_new(
            record.clone(),
            scope.clone(),
            "open",
            policy.policy_id(),
            DataClassification::Confidential,
            1,
            10,
        )?;
        let future_descriptor = ItemDescriptor::try_new(
            future_record.clone(),
            scope.clone(),
            "future",
            policy.policy_id(),
            DataClassification::Confidential,
            1,
            30,
        )?;
        let replacement_descriptor = ItemDescriptor::try_new(
            record.clone(),
            scope,
            "replacement",
            policy.policy_id(),
            DataClassification::Confidential,
            2,
            60,
        )?;
        Ok(Self {
            holding_id,
            principal_id,
            access,
            record,
            future_record,
            relation_id: Uuid::new_v4(),
            original_descriptor,
            future_descriptor,
            replacement_descriptor,
        })
    }
}

async fn seed_directory(pool: &sqlx::PgPool, fixture: &Fixture) -> Result<(), Box<dyn Error>> {
    let now = Utc::now();
    let projection = ItemDirectoryProjectionAdapter::new(pool.clone());
    projection
        .begin_rebuild(fixture.holding_id, 1, 20, now)
        .await?;
    projection
        .apply_descriptor(&DirectoryDescriptorPublication {
            holding_id: fixture.holding_id,
            descriptor: fixture.original_descriptor.clone(),
            authorized_search_text: "atomic original purchase order".into(),
            projected_at: now,
        })
        .await?;
    projection
        .apply_grant(&DirectoryGrantPublication {
            holding_id: fixture.holding_id,
            record_id: fixture.record.id(),
            principal_id: fixture.principal_id,
            authorization_policy: fixture.original_descriptor.authorization_policy(),
            policy_version: fixture.original_descriptor.authorization_policy_version(),
            can_view: true,
            effective_at: now - ChronoDuration::minutes(1),
            expires_at: now + ChronoDuration::hours(1),
            source_event_position: 20,
            projected_at: now,
        })
        .await?;
    projection
        .complete_rebuild(fixture.holding_id, 1, 20, now)
        .await?;
    Ok(())
}

async fn prove_future_facts_are_checkpoint_fenced(
    pool: &sqlx::PgPool,
    fixture: &Fixture,
) -> Result<(), Box<dyn Error>> {
    let now = Utc::now();
    let projection = ItemDirectoryProjectionAdapter::new(pool.clone());
    projection
        .apply_descriptor(&DirectoryDescriptorPublication {
            holding_id: fixture.holding_id,
            descriptor: fixture.future_descriptor.clone(),
            authorized_search_text: "checkpoint future purchase order".into(),
            projected_at: now,
        })
        .await?;
    projection
        .apply_grant(&DirectoryGrantPublication {
            holding_id: fixture.holding_id,
            record_id: fixture.future_record.id(),
            principal_id: fixture.principal_id,
            authorization_policy: fixture.future_descriptor.authorization_policy(),
            policy_version: fixture.future_descriptor.authorization_policy_version(),
            can_view: true,
            effective_at: now - ChronoDuration::minutes(1),
            expires_at: now + ChronoDuration::hours(1),
            source_event_position: 40,
            projected_at: now,
        })
        .await?;
    projection
        .apply_relation(&DirectoryRelationPublication {
            holding_id: fixture.holding_id,
            relation_id: fixture.relation_id,
            source_record_id: fixture.record.id(),
            target_record_id: fixture.future_record.id(),
            relation_type: "relates-to".into(),
            relation_version: 1,
            source_event_position: 50,
            projected_at: now,
        })
        .await?;

    let directory = SqlxItemDirectory::new(pool.clone());
    if directory
        .resolve(&fixture.access, fixture.future_record.id(), now)
        .await?
        != DirectoryLookup::Neutral
    {
        return Err("a future descriptor/grant escaped the applied checkpoint".into());
    }
    let search_request = DirectorySearchRequest {
        query: "checkpoint future".into(),
        page_size: 10,
        cursor: None,
    };
    let search = directory
        .search(&fixture.access, &search_request, now)
        .await?;
    let DirectoryLookup::Visible { value: search, .. } = search else {
        return Err("fresh checkpoint search did not return its neutral visible page".into());
    };
    if !search.items.is_empty() || search.authorized_count != 0 || search.next_cursor.is_some() {
        return Err("future descriptor/grant affected search payload, count, or cursor".into());
    }
    let backlinks = directory
        .backlinks(&fixture.access, fixture.record.id(), 10, None, now)
        .await?;
    let DirectoryLookup::Visible {
        value: backlinks, ..
    } = backlinks
    else {
        return Err("fresh checkpoint backlinks did not return its neutral visible page".into());
    };
    if !backlinks.incoming.is_empty()
        || !backlinks.outgoing.is_empty()
        || backlinks.authorized_count != 0
        || backlinks.next_cursor.is_some()
    {
        return Err("future relation affected backlinks payload, count, or cursor".into());
    }

    let checkpoint = projection
        .begin_incremental(fixture.holding_id, 50, now)
        .await?;
    if checkpoint.generation != 1
        || checkpoint.applied_position != 20
        || checkpoint.source_high_watermark != 50
        || !checkpoint.rebuilding
    {
        return Err("incremental checkpoint did not retain the expected position fence".into());
    }
    projection
        .advance_checkpoint(fixture.holding_id, 1, 20, 50, now)
        .await?;
    projection
        .complete_rebuild(fixture.holding_id, 1, 50, now)
        .await?;

    let resolved = directory
        .resolve(&fixture.access, fixture.future_record.id(), Utc::now())
        .await?;
    let DirectoryLookup::Visible { value, .. } = resolved else {
        return Err("checkpointed descriptor/grant did not become visible".into());
    };
    if value != fixture.future_descriptor {
        return Err("checkpointed resolve returned the wrong descriptor".into());
    }
    let search = directory
        .search(&fixture.access, &search_request, Utc::now())
        .await?;
    let DirectoryLookup::Visible { value: search, .. } = search else {
        return Err("checkpointed search did not become visible".into());
    };
    if search.items != vec![fixture.future_descriptor.clone()]
        || search.authorized_count != 1
        || search.next_cursor.is_some()
    {
        return Err("checkpointed search payload/count is incorrect".into());
    }
    let backlinks = directory
        .backlinks(&fixture.access, fixture.record.id(), 10, None, Utc::now())
        .await?;
    let DirectoryLookup::Visible {
        value: backlinks, ..
    } = backlinks
    else {
        return Err("checkpointed backlinks did not become visible".into());
    };
    if !backlinks.incoming.is_empty()
        || backlinks.outgoing != vec![fixture.future_record.clone()]
        || backlinks.authorized_count != 1
        || backlinks.next_cursor.is_some()
    {
        return Err("checkpointed backlink payload/count is incorrect".into());
    }
    Ok(())
}

async fn install_blocking_scope_wrapper(pool: &sqlx::PgPool) -> Result<(), Box<dyn Error>> {
    let rename = format!(
        "ALTER FUNCTION erpx_collaboration_read.scope_contains(JSONB,JSONB) \
         RENAME TO {ORIGINAL_SCOPE_FUNCTION}"
    );
    sqlx::query(&rename).execute(pool).await?;
    let create = format!(
        "CREATE FUNCTION erpx_collaboration_read.scope_contains(authority JSONB,target JSONB) \
         RETURNS BOOLEAN LANGUAGE plpgsql VOLATILE AS $atomic$ \
         BEGIN \
           PERFORM pg_catalog.pg_advisory_xact_lock({TEST_ADVISORY_LOCK}::BIGINT); \
           RETURN erpx_collaboration_read.{ORIGINAL_SCOPE_FUNCTION}(authority,target); \
         END \
         $atomic$"
    );
    if let Err(error) = sqlx::query(&create).execute(pool).await {
        let restore = format!(
            "ALTER FUNCTION erpx_collaboration_read.{ORIGINAL_SCOPE_FUNCTION}(JSONB,JSONB) \
             RENAME TO scope_contains"
        );
        let _ = sqlx::query(&restore).execute(pool).await;
        return Err(error.into());
    }
    Ok(())
}

async fn restore_scope_function(pool: &sqlx::PgPool) -> Result<(), Box<dyn Error>> {
    sqlx::query("DROP FUNCTION erpx_collaboration_read.scope_contains(JSONB,JSONB)")
        .execute(pool)
        .await?;
    let restore = format!(
        "ALTER FUNCTION erpx_collaboration_read.{ORIGINAL_SCOPE_FUNCTION}(JSONB,JSONB) \
         RENAME TO scope_contains"
    );
    sqlx::query(&restore).execute(pool).await?;
    Ok(())
}

async fn run_resolve_replacement_race(
    control_pool: &sqlx::PgPool,
    reader_pool: &sqlx::PgPool,
    fixture: &Fixture,
) -> Result<(), Box<dyn Error>> {
    let mut blocker = control_pool.begin().await?;
    let blocker_pid = sqlx::query_scalar::<_, i32>("SELECT pg_catalog.pg_backend_pid()")
        .fetch_one(&mut *blocker)
        .await?;
    sqlx::query("SELECT pg_catalog.pg_advisory_xact_lock($1)")
        .bind(TEST_ADVISORY_LOCK)
        .execute(&mut *blocker)
        .await?;

    let directory = SqlxItemDirectory::new(reader_pool.clone());
    let access = fixture.access.clone();
    let record_id = fixture.record.id();
    let observed_at = Utc::now();
    let resolve_task =
        tokio::spawn(async move { directory.resolve(&access, record_id, observed_at).await });

    if let Err(error) = wait_until_reader_is_blocked(control_pool, blocker_pid).await {
        let _ = blocker.rollback().await;
        resolve_task.abort();
        let _ = resolve_task.await;
        return Err(error);
    }

    if let Err(error) = replace_with_ungranted_descriptor(control_pool, fixture).await {
        let _ = blocker.rollback().await;
        resolve_task.abort();
        let _ = resolve_task.await;
        return Err(error);
    }
    blocker.commit().await?;

    let lookup = tokio::time::timeout(Duration::from_secs(5), resolve_task)
        .await
        .map_err(|_| "timed out waiting for the blocked Directory resolve")??
        .map_err(|error| format!("Directory resolve failed: {error}"))?;
    let DirectoryLookup::Visible { value, .. } = lookup else {
        return Err("the in-flight resolve did not return its original authorized snapshot".into());
    };
    if value.source_event_position() != fixture.original_descriptor.source_event_position()
        || value.descriptor_version() != fixture.original_descriptor.descriptor_version()
        || value.lifecycle_state() != fixture.original_descriptor.lifecycle_state()
    {
        return Err("resolve leaked the concurrently committed ungranted descriptor".into());
    }

    let current = SqlxItemDirectory::new(reader_pool.clone())
        .resolve(&fixture.access, fixture.record.id(), Utc::now())
        .await?;
    if current != DirectoryLookup::Neutral {
        return Err(
            "the replacement descriptor must remain hidden until a covering grant arrives".into(),
        );
    }
    Ok(())
}

async fn wait_until_reader_is_blocked(
    pool: &sqlx::PgPool,
    blocker_pid: i32,
) -> Result<(), Box<dyn Error>> {
    for _ in 0..100 {
        let blocked = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (\
               SELECT 1 FROM pg_catalog.pg_stat_activity activity \
               WHERE activity.application_name=$1 \
                 AND $2::INTEGER=ANY(pg_catalog.pg_blocking_pids(activity.pid))\
             )",
        )
        .bind(READER_APPLICATION_NAME)
        .bind(blocker_pid)
        .fetch_one(pool)
        .await?;
        if blocked {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Directory resolve never reached the coordinated authorization barrier".into())
}

async fn replace_with_ungranted_descriptor(
    pool: &sqlx::PgPool,
    fixture: &Fixture,
) -> Result<(), Box<dyn Error>> {
    let now = Utc::now();
    let mut tx = pool.begin().await?;
    let bound = sqlx::query_scalar::<_, String>("SELECT set_config('app.holding_id',$1,true)")
        .bind(fixture.holding_id.to_string())
        .fetch_one(&mut *tx)
        .await?;
    if bound != fixture.holding_id.to_string() {
        return Err("failed to bind the replacement transaction to its exact holding".into());
    }
    let updated = sqlx::query(
        "UPDATE erpx_collaboration_read.item_directory_descriptors \
         SET descriptor=$3,authorized_search_text=$4,descriptor_version=2, \
             source_event_position=60,projected_at=$5 \
         WHERE holding_id=$1 AND record_id=$2",
    )
    .bind(fixture.holding_id.as_uuid())
    .bind(fixture.record.id().as_uuid())
    .bind(serde_json::to_value(&fixture.replacement_descriptor)?)
    .bind("atomic replacement purchase order")
    .bind(now)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err("the replacement descriptor row was not updated".into());
    }
    let state = sqlx::query(
        "UPDATE erpx_collaboration_read.item_directory_projection_state \
         SET source_high_watermark=60,applied_position=60,state='ready', \
             last_success_at=$2,updated_at=$2 WHERE holding_id=$1",
    )
    .bind(fixture.holding_id.as_uuid())
    .bind(now)
    .execute(&mut *tx)
    .await?;
    if state.rows_affected() != 1 {
        return Err("the replacement projection state was not updated".into());
    }
    tx.commit().await?;
    Ok(())
}
