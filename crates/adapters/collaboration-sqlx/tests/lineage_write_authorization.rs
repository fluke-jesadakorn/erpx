//! Live proof that Organization lineage gates Collaboration writes before a
//! thread snapshot is available to an ancestor-scoped caller.

use std::{env, error::Error, str::FromStr};

use chrono::{Duration, Utc};
use erpx_collaboration::{
    ActorContext, AnchorPolicy, Audience, AuthorizationPort, CollaborationAction, CommentDocument,
    CommentId, MutationIdentity, PortError, PrincipalRef, RetentionPolicy, Thread, ThreadAnchor,
    ThreadCommit, ThreadId, ThreadWriteRepository,
};
use erpx_collaboration_sqlx::{
    DirectoryDescriptorPublication, DirectoryGrantPublication, ItemDirectoryProjectionAdapter,
    SqlxCollaborationAuthorization, SqlxThreadWriteRepository,
};
use erpx_integration_contracts::{
    ItemDescriptor, RecordTypeId, StableRecordRef, catalog::ContextId, record_policy,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuthorityScope, CommandId, CorrelationId, DataClassification,
    HoldingId, PrincipalId, PrincipalKind, RecordId, SessionId,
};
use serde_json::json;
use sqlx::{
    Postgres, Transaction,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

const WRITE_DATABASE_URL: &str = "ERPX_TEST_HOLDING_WRITE_DATABASE_URL";
const READ_DATABASE_URL: &str = "ERPX_TEST_HOLDING_READ_DATABASE_URL";

#[test]
#[ignore = "requires disposable PostgreSQL databases prepared by cargo xtask verify-sqlx"]
fn collaboration_lineage_write_sqlx_acceptance() {
    let Some(write_url) = configured_url(WRITE_DATABASE_URL) else {
        eprintln!(
            "Collaboration lineage write acceptance skipped: {WRITE_DATABASE_URL} is not set"
        );
        return;
    };
    let Some(read_url) = configured_url(READ_DATABASE_URL) else {
        eprintln!("Collaboration lineage write acceptance skipped: {READ_DATABASE_URL} is not set");
        return;
    };
    require_verify_database(&write_url, "erpx_verify_tw_")
        .expect("the write URL must target the verify-sqlx disposable database");
    require_verify_database(&read_url, "erpx_verify_tr_")
        .expect("the read URL must target the verify-sqlx disposable database");

    sqlx::__rt::test_block_on(async move {
        run_acceptance(&write_url, &read_url)
            .await
            .expect("Collaboration lineage write SQLx acceptance must pass");
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
        return Err(format!(
            "refusing non-disposable Collaboration acceptance database {database}"
        )
        .into());
    }
    Ok(())
}

async fn run_acceptance(write_url: &str, read_url: &str) -> Result<(), Box<dyn Error>> {
    let write_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(write_url)
        .await?;
    let read_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(read_url)
        .await?;
    let fixture = Fixture::new()?;
    let now = Utc::now();

    let directory = ItemDirectoryProjectionAdapter::new(read_pool.clone());
    directory
        .begin_rebuild(fixture.holding_id, 1, 100, now)
        .await?;
    directory
        .apply_descriptor(&DirectoryDescriptorPublication {
            holding_id: fixture.holding_id,
            descriptor: fixture.descriptor.clone(),
            authorized_search_text: "lineage write authorization purchase order".into(),
            projected_at: now,
        })
        .await?;
    directory
        .apply_grant(&DirectoryGrantPublication {
            holding_id: fixture.holding_id,
            record_id: fixture.record.id(),
            principal_id: fixture.principal.id(),
            authorization_policy: fixture.descriptor.authorization_policy(),
            policy_version: fixture.descriptor.authorization_policy_version(),
            can_view: true,
            effective_at: now - Duration::minutes(1),
            expires_at: now + Duration::hours(1),
            source_event_position: 20,
            projected_at: now,
        })
        .await?;
    directory
        .complete_rebuild(fixture.holding_id, 1, 100, now)
        .await?;

    let company_authorization = SqlxCollaborationAuthorization::from_authorized_request(
        write_pool.clone(),
        read_pool.clone(),
        [CollaborationAction::CreateThread],
    );

    // A current record grant is insufficient when the Organization witness is
    // missing. No Collaboration thread row can be created through this denial.
    assert_eq!(
        company_authorization
            .authorize_new_thread(
                &fixture.company_actor,
                fixture.thread_id,
                &fixture.policy,
                &fixture.audience,
            )
            .await,
        Err(PortError::Denied)
    );
    assert_eq!(thread_count(&write_pool, &fixture).await?, 0);

    insert_active_lineage(&read_pool, &fixture, now).await?;
    let create_access = company_authorization
        .authorize_new_thread(
            &fixture.company_actor,
            fixture.thread_id,
            &fixture.policy,
            &fixture.audience,
        )
        .await?;
    assert_eq!(create_access.authority_scope(), &fixture.company_scope);
    assert_eq!(create_access.visibility_scope(), &fixture.team_scope);

    let mut thread = Thread::create(
        fixture.thread_id,
        fixture.policy.clone(),
        ThreadAnchor::try_new(fixture.record.clone(), None)?,
        fixture.audience.clone(),
        RetentionPolicy {
            parent_retain_until: now + Duration::days(30),
            comment_retain_until: now + Duration::days(30),
            jurisdiction_retain_until: now + Duration::days(30),
            legal_holds: Default::default(),
        },
        fixture.principal,
        now,
    )?;
    let created_events = thread.take_pending_events();
    let repository = SqlxThreadWriteRepository::new(write_pool.clone());
    repository
        .commit(
            &create_access,
            ThreadCommit {
                identity: mutation_identity(1)?,
                expected_version: None,
                thread,
                events: created_events,
                inbox_facts: Vec::new(),
            },
        )
        .await?;
    assert_eq!(thread_count(&write_pool, &fixture).await?, 1);

    let branch_authorization = SqlxCollaborationAuthorization::from_authorized_request(
        write_pool.clone(),
        read_pool.clone(),
        [CollaborationAction::AddComment],
    );

    set_node_status(&read_pool, &fixture, "suspended").await?;
    assert_eq!(
        branch_authorization
            .authorize_thread(
                &fixture.branch_actor,
                fixture.thread_id,
                CollaborationAction::AddComment,
            )
            .await,
        Err(PortError::Denied)
    );
    assert_eq!(comment_count(&write_pool, &fixture).await?, 0);

    set_node_status(&read_pool, &fixture, "active").await?;
    let comment_access = branch_authorization
        .authorize_thread(
            &fixture.branch_actor,
            fixture.thread_id,
            CollaborationAction::AddComment,
        )
        .await?;
    assert_eq!(comment_access.authority_scope(), &fixture.branch_scope);
    assert_eq!(comment_access.visibility_scope(), &fixture.team_scope);

    let mut loaded = repository
        .load(&comment_access)
        .await?
        .ok_or("authorized thread must exist")?;
    let expected_version = loaded.version();
    loaded.add_comment(
        expected_version,
        CommentId::try_from_uuid(Uuid::new_v4())?,
        fixture.principal,
        CommentDocument::try_new(
            1,
            json!({"type": "doc", "blocks": [{"type": "paragraph", "text": "approved lineage"}]}),
            "approved lineage".into(),
        )?,
        Vec::new(),
        Vec::new(),
        now + Duration::seconds(1),
    )?;
    let comment_events = loaded.take_pending_events();
    repository
        .commit(
            &comment_access,
            ThreadCommit {
                identity: mutation_identity(2)?,
                expected_version: Some(expected_version),
                thread: loaded,
                events: comment_events,
                inbox_facts: Vec::new(),
            },
        )
        .await?;
    assert_eq!(comment_count(&write_pool, &fixture).await?, 1);

    read_pool.close().await;
    write_pool.close().await;
    Ok(())
}

struct Fixture {
    holding_id: HoldingId,
    holding: Uuid,
    company: Uuid,
    branch: Uuid,
    department: Uuid,
    team: Uuid,
    principal: PrincipalRef,
    company_scope: AuthorityScope,
    branch_scope: AuthorityScope,
    team_scope: AuthorityScope,
    company_actor: ActorContext,
    branch_actor: ActorContext,
    record: StableRecordRef,
    descriptor: ItemDescriptor,
    policy: AnchorPolicy,
    audience: Audience,
    thread_id: ThreadId,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        let holding = Uuid::new_v4();
        let holding_id = HoldingId::try_from_uuid(holding)?;
        let company = Uuid::new_v4();
        let branch = Uuid::new_v4();
        let department = Uuid::new_v4();
        let team = Uuid::new_v4();
        let principal = PrincipalRef::new(
            PrincipalId::try_from_uuid(Uuid::new_v4())?,
            PrincipalKind::Human,
        );
        let company_scope = AuthorityScope::Company {
            holding_id: holding,
            company_id: company,
        };
        let branch_scope = AuthorityScope::Branch {
            holding_id: holding,
            company_id: company,
            branch_id: branch,
        };
        let team_scope = AuthorityScope::Subtree {
            holding_id: holding,
            organization_node_id: team,
        };
        let actor = |scope: AuthorityScope| ActorContext {
            holding_id,
            principal,
            authority_scope: scope,
            attribution: ActorAttribution {
                principal_ref: principal.id().to_string(),
                kind: ActorKind::HoldingUser,
                session_id: Some(SessionId::try_from_uuid(Uuid::new_v4()).expect("session id")),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            external_portal: None,
        };
        let record = StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4())?,
            ContextId::Procurement,
            RecordTypeId::try_new("procurement.purchase-order")?,
        )?;
        let record_policy = record_policy(record.record_type().as_str())
            .ok_or("purchase-order record policy must exist")?;
        let descriptor = ItemDescriptor::try_new(
            record.clone(),
            team_scope.clone(),
            "open",
            record_policy.policy_id(),
            DataClassification::Confidential,
            1,
            10,
        )?;
        let policy = AnchorPolicy::try_new(
            holding_id,
            record.clone(),
            team_scope.clone(),
            None,
            DataClassification::Confidential,
            record_policy.policy_id(),
        )?;
        let audience = Audience::Internal {
            scope: team_scope.clone(),
            principals: vec![principal],
        };
        Ok(Self {
            holding_id,
            holding,
            company,
            branch,
            department,
            team,
            principal,
            company_actor: actor(company_scope.clone()),
            branch_actor: actor(branch_scope.clone()),
            company_scope,
            branch_scope,
            team_scope,
            record,
            descriptor,
            policy,
            audience,
            thread_id: ThreadId::try_from_uuid(Uuid::new_v4())?,
        })
    }
}

fn mutation_identity(marker: u8) -> Result<MutationIdentity, Box<dyn Error>> {
    Ok(MutationIdentity {
        command_id: CommandId::try_from_uuid(Uuid::new_v4())?,
        correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4())?,
        idempotency_key: Uuid::new_v4().to_string(),
        request_digest: [marker; 32],
    })
}

async fn insert_active_lineage(
    read_pool: &sqlx::PgPool,
    fixture: &Fixture,
    now: chrono::DateTime<Utc>,
) -> Result<(), Box<dyn Error>> {
    let mut tx = read_pool.begin().await?;
    set_holding(&mut tx, fixture.holding_id).await?;
    for (node_id, kind, parent, lineage, position) in [
        (fixture.holding, "holding", None, vec![], 1_i64),
        (
            fixture.company,
            "company",
            Some(fixture.holding),
            vec![fixture.holding],
            2,
        ),
        (
            fixture.branch,
            "branch",
            Some(fixture.company),
            vec![fixture.holding, fixture.company],
            3,
        ),
        (
            fixture.department,
            "department",
            Some(fixture.branch),
            vec![fixture.holding, fixture.company, fixture.branch],
            4,
        ),
        (
            fixture.team,
            "team",
            Some(fixture.department),
            vec![
                fixture.holding,
                fixture.company,
                fixture.branch,
                fixture.department,
            ],
            5,
        ),
    ] {
        sqlx::query(
            "INSERT INTO erpx_authority_read.organization_nodes \
             (holding_id,node_id,node_kind,node_status,parent_node_id,lineage_node_ids, \
              aggregate_version,source_event_position,changed_at,projected_at) \
             VALUES ($1,$2,$3,'active',$4,$5,1,$6,$7,$7)",
        )
        .bind(fixture.holding)
        .bind(node_id)
        .bind(kind)
        .bind(parent)
        .bind(lineage)
        .bind(position)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn set_node_status(
    read_pool: &sqlx::PgPool,
    fixture: &Fixture,
    status: &str,
) -> Result<(), Box<dyn Error>> {
    let mut tx = read_pool.begin().await?;
    set_holding(&mut tx, fixture.holding_id).await?;
    sqlx::query(
        "UPDATE erpx_authority_read.organization_nodes SET node_status=$3 \
         WHERE holding_id=$1 AND node_id=$2",
    )
    .bind(fixture.holding)
    .bind(fixture.team)
    .bind(status)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn thread_count(write_pool: &sqlx::PgPool, fixture: &Fixture) -> Result<i64, Box<dyn Error>> {
    let mut tx = write_pool.begin().await?;
    set_holding(&mut tx, fixture.holding_id).await?;
    let count = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_collaboration.threads WHERE holding_id=$1 AND thread_id=$2",
    )
    .bind(fixture.holding)
    .bind(fixture.thread_id.as_uuid())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(count)
}

async fn comment_count(
    write_pool: &sqlx::PgPool,
    fixture: &Fixture,
) -> Result<i64, Box<dyn Error>> {
    let mut tx = write_pool.begin().await?;
    set_holding(&mut tx, fixture.holding_id).await?;
    let count = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_collaboration.comments WHERE holding_id=$1 AND thread_id=$2",
    )
    .bind(fixture.holding)
    .bind(fixture.thread_id.as_uuid())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(count)
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
        return Err("PostgreSQL holding binding mismatch".into());
    }
    Ok(())
}
