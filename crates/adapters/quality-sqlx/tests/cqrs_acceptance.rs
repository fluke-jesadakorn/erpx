use std::{env, error::Error};

use async_trait::async_trait;
use chrono::Utc;
use erpx_cqrs::{
    CONTRACT_VERSION, CommandEnvelope, CommandHandler, CqrsError, IdempotencyKey, QueryConsistency,
    QueryEnvelope, QueryHandler, ReadYourWrites,
};
use erpx_cqrs_sqlx::{
    ProjectionPool, ProjectionRunner, ProjectionRunnerConfig, ProjectionSource, ProjectionStore,
    ReadPool, WritePool,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, FixedClock, QueryId,
    TenantScope,
};
use erpx_quality::{
    application::{CreateInspection, GetInspection, QualityCommandHandler, QualityQueryHandler},
    domain::{InspectionId, InspectionKind},
    ports::{
        AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, EntitlementPort, PortError, QualityAction, QualityCommit,
        WriteRepository,
    },
};
use erpx_quality_sqlx::{
    PROJECTION_NAME, QualityCommandAdapter, QualityInspectionProjector, QualityReadRepository,
    QualityWriteRepository, inspection_partition,
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

const WRITE_DATABASE_URL: &str = "ERPX_TEST_TENANT_WRITE_DATABASE_URL";
const READ_DATABASE_URL: &str = "ERPX_TEST_TENANT_READ_DATABASE_URL";

#[test]
fn quality_cqrs_sqlx_acceptance() {
    let Some(write_url) = configured_url(WRITE_DATABASE_URL) else {
        eprintln!("Quality CQRS SQLx acceptance skipped: {WRITE_DATABASE_URL} is not set");
        return;
    };
    let Some(read_url) = configured_url(READ_DATABASE_URL) else {
        eprintln!("Quality CQRS SQLx acceptance skipped: {READ_DATABASE_URL} is not set");
        return;
    };

    sqlx::__rt::test_block_on(async move {
        run_acceptance(&write_url, &read_url)
            .await
            .expect("Quality CQRS SQLx acceptance must pass");
    });
}

fn configured_url(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

async fn run_acceptance(write_url: &str, read_url: &str) -> Result<(), Box<dyn Error>> {
    let write_database = PgPoolOptions::new()
        .max_connections(4)
        .connect(write_url)
        .await?;
    let read_database = PgPoolOptions::new()
        .max_connections(4)
        .connect(read_url)
        .await?;
    install_contracts(&write_database, &read_database).await?;

    let tenant_id = Uuid::new_v4();
    let write_pool = WritePool::for_tenant(write_database.clone(), tenant_id)?;
    let projection_pool = ProjectionPool::for_tenant(read_database.clone(), tenant_id)?;
    let read_pool = ReadPool::for_tenant(read_database.clone(), tenant_id)?;
    let write_repository = QualityWriteRepository::new(write_pool.clone());
    let command_adapter = QualityCommandAdapter::new(write_pool.clone());

    let inspection_uuid = Uuid::new_v4();
    let inspection_id = InspectionId::try_from_uuid(inspection_uuid)?;
    let command_id = CommandId::try_from_uuid(Uuid::new_v4())?;
    let correlation_id = CorrelationId::try_from_uuid(Uuid::new_v4())?;
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let product_id = Uuid::new_v4();
    let scope = TenantScope::Company {
        holding_id,
        company_id,
    };
    let actor = actor();
    let audit = audit();
    let now = Utc::now();
    let create = CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id,
        idempotency_key: IdempotencyKey::try_new(format!("quality-acceptance-{command_id}"))?,
        correlation_id,
        causation_id: None,
        actor: actor.clone(),
        tenant_scope: scope.clone(),
        audit: audit.clone(),
        expected_aggregate_version: None,
        command: CreateInspection {
            inspection_id,
            scope: scope.clone(),
            kind: InspectionKind::Incoming,
            product_id,
            lot_reference: Some(format!("acceptance-{inspection_uuid}")),
            quantity: 10,
        },
    };
    let command_handler = QualityCommandHandler::new(
        write_repository.clone(),
        command_adapter.clone(),
        AcceptanceAccess,
        AcceptanceAccess,
        FixedClock::new(now),
    );

    let first = command_handler.handle(create.clone()).await?;
    assert!(!first.replayed);
    assert_eq!(first.aggregate.version, 1);
    let projection_target = first
        .projection_target()
        .expect("the private projection event is the projection target");
    let partition = inspection_partition(inspection_id);

    let committed = command_counts(&write_pool, &partition).await?;
    assert_eq!(committed.authorities, 1);
    assert_eq!(committed.receipts, 1);
    assert_eq!(committed.audits, 1);
    assert!(committed.outbox >= 1);
    assert_eq!(
        completed_receipts(&write_pool, &partition, command_id).await?,
        1
    );

    let replay = command_handler.handle(create).await?;
    assert!(replay.replayed);
    assert_eq!(replay.response, first.response);
    assert_eq!(command_counts(&write_pool, &partition).await?, committed);

    // Exercise the store's transactional conflict path, not only the
    // application handler's preflight version check. The receipt reservation
    // happens first and must roll back with the rejected authority update.
    let failed_command_id = CommandId::try_from_uuid(Uuid::new_v4())?;
    let mut conflicting_aggregate = write_repository
        .load(&scope, inspection_id)
        .await?
        .expect("the command-side aggregate must exist");
    conflicting_aggregate.start(now + chrono::Duration::seconds(1))?;
    let conflict = command_adapter
        .commit(QualityCommit {
            identity: CommandIdentity {
                command_id: failed_command_id,
                idempotency_key: IdempotencyKey::try_new(format!(
                    "quality-acceptance-conflict-{failed_command_id}"
                ))?,
                fingerprint: CommandFingerprint::Start { inspection_id },
            },
            correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4())?,
            causation_id: None,
            actor: actor.clone(),
            authority_scope: scope.clone(),
            audit: audit.clone(),
            expected_version: Some(99),
            aggregate: conflicting_aggregate,
            integration_events: Vec::new(),
        })
        .await;
    assert!(matches!(
        conflict,
        Err(PortError::Conflict {
            expected: 99,
            actual: 1
        })
    ));
    assert_eq!(command_counts(&write_pool, &partition).await?, committed);
    assert_eq!(
        command_rows(&write_pool, &partition, failed_command_id).await?,
        0,
        "the failed transaction must not leave a receipt, audit, or outbox row"
    );

    let read_repository = QualityReadRepository::new(read_pool.clone())?;
    let query_handler =
        QualityQueryHandler::new(read_repository.clone(), AcceptanceAccess, AcceptanceAccess);

    // The aggregate exists on the write side but not in the read projection.
    // An eventual query returning None proves that this query path does not
    // fall back to the write database.
    let before_projection = query_handler
        .handle(get_query(
            inspection_id,
            &scope,
            &actor,
            &audit,
            QueryConsistency::Eventual,
        ))
        .await?;
    assert!(before_projection.value.inspection.is_none());

    let requirement = ReadYourWrites {
        projection: PROJECTION_NAME.into(),
        partition: partition.clone(),
        target: projection_target,
        timeout_millis: 5,
    };
    let not_ready = query_handler
        .handle(get_query(
            inspection_id,
            &scope,
            &actor,
            &audit,
            QueryConsistency::ReadYourWrites(requirement.clone()),
        ))
        .await;
    assert!(matches!(not_ready, Err(CqrsError::NotReady(_))));

    let projector = QualityInspectionProjector::new();
    let projection_store = ProjectionStore::new(projection_pool.clone());
    let runner = ProjectionRunner::new(
        ProjectionSource::new(write_pool),
        projection_store.clone(),
        ProjectionRunnerConfig {
            page_size: 1_000,
            ..ProjectionRunnerConfig::default()
        },
    )?;
    run_active_through(&runner, &projector, projection_target.global_sequence).await?;

    let projected = query_handler
        .handle(get_query(
            inspection_id,
            &scope,
            &actor,
            &audit,
            QueryConsistency::ReadYourWrites(ReadYourWrites {
                timeout_millis: 1_000,
                ..requirement
            }),
        ))
        .await?;
    let projected_inspection = projected
        .value
        .inspection
        .expect("the read-side projection must contain the command result");
    assert_eq!(projected_inspection.inspection_id, inspection_id);
    assert_eq!(projected_inspection.version, 1);
    assert_eq!(
        projected
            .metadata
            .projection
            .checkpoint
            .expect("ReadYourWrites returns its checkpoint")
            .global_sequence,
        projection_target.global_sequence
    );

    // Build a complete shadow generation, replay from zero to prove duplicate
    // suppression, then atomically switch the active generation.
    let rebuild = projection_store.begin_rebuild(&projector).await?;
    run_rebuild_through(
        &runner,
        &projector,
        &rebuild,
        projection_target.global_sequence,
    )
    .await?;
    let duplicate = runner.run_rebuild(&projector, &rebuild, 0).await?;
    assert!(duplicate.duplicates > 0);
    assert_eq!(duplicate.applied, 0);
    projection_store.activate_rebuild(&rebuild).await?;
    let active = projection_store
        .active_generation(PROJECTION_NAME)
        .await?
        .expect("the rebuilt projection must be active");
    assert_eq!(active.generation, rebuild.generation);
    assert!(active.source_sequence >= projection_target.global_sequence);

    let after_switch = query_handler
        .handle(get_query(
            inspection_id,
            &scope,
            &actor,
            &audit,
            QueryConsistency::Eventual,
        ))
        .await?;
    assert_eq!(
        after_switch
            .value
            .inspection
            .expect("the shadow projection survives activation")
            .inspection_id,
        inspection_id
    );

    write_database.close().await;
    read_database.close().await;
    Ok(())
}

async fn install_contracts(write: &PgPool, read: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(erpx_cqrs_sqlx::WRITE_DDL)
        .execute(write)
        .await?;
    if erpx_quality_sqlx::WRITE_DDL.contains("CREATE ") {
        sqlx::raw_sql(erpx_quality_sqlx::WRITE_DDL)
            .execute(write)
            .await?;
    }
    sqlx::raw_sql(erpx_cqrs_sqlx::READ_DDL)
        .execute(read)
        .await?;
    sqlx::raw_sql(erpx_quality_sqlx::READ_DDL)
        .execute(read)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CommandCounts {
    authorities: i64,
    receipts: i64,
    audits: i64,
    outbox: i64,
}

async fn command_counts(
    pool: &WritePool,
    partition: &str,
) -> Result<CommandCounts, Box<dyn Error>> {
    let mut transaction = pool.begin().await?;
    let counts = CommandCounts {
        authorities: sqlx::query_scalar(
            "SELECT COUNT(*) FROM erpx_quality_write.inspection_authority",
        )
        .fetch_one(&mut *transaction)
        .await?,
        receipts: sqlx::query_scalar(
            "SELECT COUNT(*) FROM erpx_cqrs_write.command_receipts
             WHERE context_name = 'quality' AND partition_key = $1",
        )
        .bind(partition)
        .fetch_one(&mut *transaction)
        .await?,
        audits: sqlx::query_scalar(
            "SELECT COUNT(*) FROM erpx_cqrs_write.audit_log
             WHERE context_name = 'quality' AND partition_key = $1",
        )
        .bind(partition)
        .fetch_one(&mut *transaction)
        .await?,
        outbox: sqlx::query_scalar(
            "SELECT COUNT(*) FROM erpx_cqrs_write.outbox_events
             WHERE context_name = 'quality' AND partition_key = $1",
        )
        .bind(partition)
        .fetch_one(&mut *transaction)
        .await?,
    };
    transaction.commit().await?;
    Ok(counts)
}

async fn completed_receipts(
    pool: &WritePool,
    partition: &str,
    command_id: CommandId,
) -> Result<i64, Box<dyn Error>> {
    let mut transaction = pool.begin().await?;
    let count = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_cqrs_write.command_receipts
         WHERE context_name = 'quality' AND partition_key = $1
           AND command_id = $2 AND status = 'completed' AND receipt IS NOT NULL",
    )
    .bind(partition)
    .bind(command_id.as_uuid())
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(count)
}

async fn command_rows(
    pool: &WritePool,
    partition: &str,
    command_id: CommandId,
) -> Result<i64, Box<dyn Error>> {
    let mut transaction = pool.begin().await?;
    let count = sqlx::query_scalar(
        "SELECT
            (SELECT COUNT(*) FROM erpx_cqrs_write.command_receipts
             WHERE context_name = 'quality' AND partition_key = $1 AND command_id = $2)
          + (SELECT COUNT(*) FROM erpx_cqrs_write.audit_log
             WHERE context_name = 'quality' AND partition_key = $1 AND command_id = $2)
          + (SELECT COUNT(*) FROM erpx_cqrs_write.outbox_events
             WHERE context_name = 'quality' AND partition_key = $1 AND command_id = $2)",
    )
    .bind(partition)
    .bind(command_id.as_uuid())
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(count)
}

async fn run_active_through(
    runner: &ProjectionRunner,
    projector: &QualityInspectionProjector,
    target: u64,
) -> Result<(), erpx_cqrs_sqlx::CqrsSqlxError> {
    for _ in 0..128 {
        let run = runner.run_active(projector).await?;
        if run.cursor >= target {
            return Ok(());
        }
        assert!(run.fetched > 0, "projection source ended before the target");
    }
    panic!("active Quality projection did not reach its bounded target");
}

async fn run_rebuild_through(
    runner: &ProjectionRunner,
    projector: &QualityInspectionProjector,
    rebuild: &erpx_cqrs_sqlx::RebuildHandle,
    target: u64,
) -> Result<(), erpx_cqrs_sqlx::CqrsSqlxError> {
    let mut cursor = 0;
    for _ in 0..128 {
        let run = runner.run_rebuild(projector, rebuild, cursor).await?;
        cursor = run.cursor;
        if cursor >= target {
            return Ok(());
        }
        assert!(run.fetched > 0, "rebuild source ended before the target");
    }
    panic!("Quality shadow rebuild did not reach its bounded target");
}

fn get_query(
    inspection_id: InspectionId,
    scope: &TenantScope,
    actor: &ActorAttribution,
    audit: &AuditContext,
    consistency: QueryConsistency,
) -> QueryEnvelope<GetInspection> {
    QueryEnvelope {
        contract_version: CONTRACT_VERSION,
        query_id: QueryId::try_from_uuid(Uuid::new_v4()).expect("non-nil query id"),
        correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4())
            .expect("non-nil correlation id"),
        actor: actor.clone(),
        tenant_scope: scope.clone(),
        audit: audit.clone(),
        consistency,
        query: GetInspection { inspection_id },
    }
}

fn actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "quality-acceptance".into(),
        kind: ActorKind::SystemWorker,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}

fn audit() -> AuditContext {
    AuditContext {
        purpose: "Quality CQRS SQLx acceptance".into(),
        customer_visible: false,
        module: Some("quality".into()),
    }
}

#[derive(Debug, Clone, Copy)]
struct AcceptanceAccess;

#[async_trait]
impl AuthorizationPort for AcceptanceAccess {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError> {
        if request.authority_scope.contains(request.target_scope)
            && matches!(
                request.action,
                QualityAction::Create
                    | QualityAction::Start
                    | QualityAction::RecordResult
                    | QualityAction::Release
                    | QualityAction::Cancel
                    | QualityAction::View
                    | QualityAction::List
            )
        {
            Ok(())
        } else {
            Err(PortError::Denied)
        }
    }
}

#[async_trait]
impl EntitlementPort for AcceptanceAccess {
    async fn ensure_quality_enabled(
        &self,
        scope: &TenantScope,
        _write: bool,
    ) -> Result<(), PortError> {
        scope.validate().map_err(|_| PortError::EntitlementDenied)
    }
}
