//! Live proof that Collaboration full-text search authorizes each anchor in
//! SQL before matching, counting, paginating, or loading projection payloads.

use std::{env, error::Error, str::FromStr};

use chrono::{Duration, Utc};
use erpx_collaboration::{
    Audience, AuthorizedSearchAccess, PortError, PrincipalRef, ThreadId, ThreadProjection,
    ThreadReadRepository, ThreadSearchRequest, ThreadState,
};
use erpx_collaboration_sqlx::{SqlxThreadReadRepository, THREAD_PROJECTION};
use erpx_kernel::{AuthorityScope, HoldingId, PrincipalId, PrincipalKind, RecordId};
use serde_json::json;
use sqlx::{
    Postgres, Transaction,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

const READ_DATABASE_URL: &str = "ERPX_TEST_HOLDING_READ_DATABASE_URL";

#[test]
#[ignore = "requires the disposable PostgreSQL database prepared by cargo xtask verify-sqlx"]
fn collaboration_thread_search_sql_authorization_acceptance() {
    let Some(read_url) = env::var(READ_DATABASE_URL)
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        eprintln!("Collaboration thread search acceptance skipped: {READ_DATABASE_URL} is not set");
        return;
    };
    require_verify_database(&read_url)
        .expect("the read URL must target the verify-sqlx disposable database");

    sqlx::__rt::test_block_on(async move {
        run_acceptance(&read_url)
            .await
            .expect("Collaboration thread search SQL authorization must pass");
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
        .max_connections(2)
        .connect(read_url)
        .await?;
    let holding_uuid = Uuid::new_v4();
    let holding_id = HoldingId::try_from_uuid(holding_uuid)?;
    let principal = PrincipalRef::new(
        PrincipalId::try_from_uuid(Uuid::new_v4())?,
        PrincipalKind::Human,
    );
    let authority_scope = AuthorityScope::Holding {
        holding_id: holding_uuid,
    };
    let access = AuthorizedSearchAccess::try_from_authorization(
        holding_id,
        authority_scope.clone(),
        principal,
        None,
    )?;
    let generation = Uuid::new_v4();
    let authorized_record = RecordId::try_from_uuid(Uuid::new_v4())?;
    let denied_record = RecordId::try_from_uuid(Uuid::new_v4())?;
    let absent_record = RecordId::try_from_uuid(Uuid::new_v4())?;
    let authorized_thread = ThreadId::try_from_uuid(Uuid::new_v4())?;
    let denied_thread = ThreadId::try_from_uuid(Uuid::new_v4())?;
    let absent_thread = ThreadId::try_from_uuid(Uuid::new_v4())?;
    let now = Utc::now();

    let mut tx = pool.begin().await?;
    set_holding(&mut tx, holding_id).await?;
    sqlx::query(
        "INSERT INTO erpx_cqrs_read.projection_generations \
         (holding_id,projection_name,generation,status,source_sequence,created_at,activated_at) \
         VALUES ($1,$2,$3,'active',3,$4,$4)",
    )
    .bind(holding_uuid)
    .bind(THREAD_PROJECTION)
    .bind(generation)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO erpx_collaboration_read.item_directory_projection_state \
         (holding_id,rebuild_generation,source_high_watermark,applied_position,state, \
          last_success_at,updated_at) \
         VALUES ($1,1,100,100,'ready',$2,$2)",
    )
    .bind(holding_uuid)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let authorized_policy = Uuid::new_v4();
    let denied_policy = Uuid::new_v4();
    insert_descriptor(
        &mut tx,
        holding_id,
        authorized_record,
        authorized_policy,
        &authority_scope,
        10,
        now,
    )
    .await?;
    insert_descriptor(
        &mut tx,
        holding_id,
        denied_record,
        denied_policy,
        &authority_scope,
        20,
        now,
    )
    .await?;
    insert_grant(
        &mut tx,
        holding_id,
        authorized_record,
        principal.id(),
        authorized_policy,
        true,
        11,
        now,
    )
    .await?;
    // A current, exact grant row with can_view=false is a denied record. It
    // must be indistinguishable from the absent descriptor below.
    insert_grant(
        &mut tx,
        holding_id,
        denied_record,
        principal.id(),
        denied_policy,
        false,
        21,
        now,
    )
    .await?;

    let audience = Audience::Internal {
        scope: authority_scope.clone(),
        principals: Vec::new(),
    };
    insert_thread_projection(
        &mut tx,
        holding_id,
        generation,
        authorized_thread,
        authorized_record,
        &authority_scope,
        &audience,
        "sharedneedle authorizedonly",
        1,
        now,
    )
    .await?;
    insert_thread_projection(
        &mut tx,
        holding_id,
        generation,
        denied_thread,
        denied_record,
        &authority_scope,
        &audience,
        "sharedneedle onlyhidden",
        2,
        now - Duration::seconds(1),
    )
    .await?;
    // This thread has a valid audience/scope but no Item Directory descriptor
    // or grant for its anchor.
    insert_thread_projection(
        &mut tx,
        holding_id,
        generation,
        absent_thread,
        absent_record,
        &authority_scope,
        &audience,
        "absentanchoronly",
        3,
        now - Duration::seconds(2),
    )
    .await?;
    tx.commit().await?;

    let repository = SqlxThreadReadRepository::new(pool.clone());
    let first_page = repository
        .search(&access, &request("sharedneedle", 1))
        .await?;
    assert_eq!(first_page.items.len(), 1);
    assert_eq!(first_page.items[0].id, authorized_thread);
    assert_eq!(first_page.authorized_count, 1);
    assert_eq!(first_page.next_cursor, None);

    let denied = repository
        .search(&access, &request("onlyhidden", 10))
        .await?;
    let absent = repository
        .search(&access, &request("absentanchoronly", 10))
        .await?;
    let empty = repository
        .search(&access, &request("nothingmatches", 10))
        .await?;
    assert_eq!(denied, absent);
    assert_eq!(denied, empty);
    assert!(denied.items.is_empty());
    assert_eq!(denied.authorized_count, 0);
    assert_eq!(denied.next_cursor, None);

    // Freshness participates in the same SQL statement. A stale directory
    // fails closed instead of returning a successful empty result.
    let mut tx = pool.begin().await?;
    set_holding(&mut tx, holding_id).await?;
    sqlx::query(
        "UPDATE erpx_collaboration_read.item_directory_projection_state \
         SET state='rebuilding',last_success_at=NULL,updated_at=$2 WHERE holding_id=$1",
    )
    .bind(holding_uuid)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    assert_eq!(
        repository
            .search(&access, &request("sharedneedle", 1))
            .await,
        Err(PortError::Unavailable)
    );

    pool.close().await;
    Ok(())
}

fn request(query: &str, page_size: u16) -> ThreadSearchRequest {
    ThreadSearchRequest {
        query: query.into(),
        page_size,
        cursor: None,
    }
}

#[allow(clippy::too_many_arguments)]
async fn insert_descriptor(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    record_id: RecordId,
    policy_id: Uuid,
    scope: &AuthorityScope,
    source_event_position: i64,
    now: chrono::DateTime<Utc>,
) -> Result<(), Box<dyn Error>> {
    sqlx::query(
        "INSERT INTO erpx_collaboration_read.item_directory_descriptors \
         (holding_id,record_id,owner_context,record_type,record_scope, \
          authorization_policy_id,authorization_policy_version,descriptor,canonical_route, \
          authorized_search_text,descriptor_version,source_event_position,projected_at) \
         VALUES ($1,$2,'acceptance','acceptance.record',$3,$4,1,$5,$6,'',1,$7,$8)",
    )
    .bind(holding_id.as_uuid())
    .bind(record_id.as_uuid())
    .bind(serde_json::to_value(scope)?)
    .bind(policy_id)
    .bind(json!({"record_id": record_id.as_uuid()}))
    .bind(format!("/acceptance/{}", record_id.as_uuid()))
    .bind(source_event_position)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_grant(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    record_id: RecordId,
    principal_id: PrincipalId,
    policy_id: Uuid,
    can_view: bool,
    source_event_position: i64,
    now: chrono::DateTime<Utc>,
) -> Result<(), Box<dyn Error>> {
    sqlx::query(
        "INSERT INTO erpx_collaboration_read.item_directory_effective_grants \
         (holding_id,record_id,principal_id,authorization_policy_id,policy_version,can_view, \
          effective_at,expires_at,source_event_position,projected_at) \
         VALUES ($1,$2,$3,$4,1,$5,$6,$7,$8,$6)",
    )
    .bind(holding_id.as_uuid())
    .bind(record_id.as_uuid())
    .bind(principal_id.as_uuid())
    .bind(policy_id)
    .bind(can_view)
    .bind(now - Duration::minutes(1))
    .bind(now + Duration::hours(1))
    .bind(source_event_position)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_thread_projection(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    generation: Uuid,
    thread_id: ThreadId,
    record_id: RecordId,
    scope: &AuthorityScope,
    audience: &Audience,
    search_text: &str,
    source_event_cursor: i64,
    updated_at: chrono::DateTime<Utc>,
) -> Result<(), Box<dyn Error>> {
    let projection = ThreadProjection {
        id: thread_id,
        anchor_record_id: record_id,
        state: ThreadState::Open,
        pinned: false,
        external_portal: None,
        comments: Vec::new(),
        visible_backlinks: Vec::new(),
        projection_cursor: u64::try_from(source_event_cursor)?,
        stale: false,
    };
    sqlx::query(
        "INSERT INTO erpx_collaboration_read.thread_projection \
         (holding_id,projection_name,generation,thread_id,anchor_record_id,visibility_scope, \
          audience,projection,search_text,source_event_cursor,updated_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(holding_id.as_uuid())
    .bind(THREAD_PROJECTION)
    .bind(generation)
    .bind(thread_id.as_uuid())
    .bind(record_id.as_uuid())
    .bind(serde_json::to_value(scope)?)
    .bind(serde_json::to_value(audience)?)
    .bind(serde_json::to_value(projection)?)
    .bind(search_text)
    .bind(source_event_cursor)
    .bind(updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn set_holding(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
) -> Result<(), Box<dyn Error>> {
    let expected = holding_id.to_string();
    let bound = sqlx::query_scalar::<_, String>("SELECT set_config('app.holding_id', $1, true)")
        .bind(&expected)
        .fetch_one(&mut **tx)
        .await?;
    if bound != expected {
        return Err("PostgreSQL did not retain the exact holding binding".into());
    }
    Ok(())
}
