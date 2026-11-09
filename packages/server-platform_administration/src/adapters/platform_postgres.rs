//! PostgreSQL adapter for Platform Administration provisioning and operational reads.
//!
//! Control-plane receipts and bindings live in the Control database. Holding
//! queue/receipt facts are read only from the explicitly configured Holding
//! pool; a missing or unconfigured Holding is a scoped not-found, never a
//! cross-Holding fallback.
use crate::application::platform_operations::{PlatformAdministrationStore, PlatformFuture};
use crate::domain::platform_operations::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationFile, MigrationTrack, PgTransactionError, PgUnitOfWork, SessionLock,
    TransactionOptions, inspect_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{PgConnection, Row};
use std::{collections::BTreeMap, sync::Arc};

pub fn platform_business_error(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "PLATFORM_DATABASE_UNAVAILABLE",
            "Platform Administration persistence is unavailable.",
        ),
    }
}

fn database_error() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLATFORM_DATABASE_UNAVAILABLE",
        "Platform Administration persistence is unavailable.",
    )
}

fn configuration_error() -> BusinessError {
    BusinessError::invalid(
        "PLATFORM_OPERATIONS_CONFIGURATION_INVALID",
        "Use distinct configured Control and Holding sources.",
    )
}

fn receipt_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MIGRATION_RECEIPT_UNAVAILABLE",
        "Migration receipts are unavailable for this Holding.",
    )
}

fn migration_failed() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MIGRATION_FAILED",
        "The Holding migration failed. Review the platform migration receipt and database logs.",
    )
}

#[derive(Clone)]
pub struct PlatformHoldingDatabase {
    pub identity: PlatformHoldingIdentity,
    pub pool: DatabasePool,
}

pub struct PgPlatformAdministration {
    control: DatabasePool,
    holdings: BTreeMap<String, PlatformHoldingDatabase>,
    migration_files: Arc<Vec<MigrationFile>>,
}

impl PgPlatformAdministration {
    pub fn new(
        control: DatabasePool,
        holdings: Vec<PlatformHoldingDatabase>,
        migration_files: Vec<MigrationFile>,
    ) -> Result<Self, BusinessError> {
        if holdings.len() > 64 {
            return Err(configuration_error());
        }
        let mut configured = BTreeMap::new();
        let mut slugs = std::collections::BTreeSet::new();
        for holding in holdings {
            if holding.identity.id.is_empty()
                || holding.identity.slug.is_empty()
                || holding.identity.display_name.is_empty()
                || matches!(holding.identity.id.as_str(), "erpx" | "h003")
                || !slugs.insert(holding.identity.slug.clone())
                || configured
                    .insert(holding.identity.id.clone(), holding)
                    .is_some()
            {
                return Err(configuration_error());
            }
        }
        Ok(Self {
            control,
            holdings: configured,
            migration_files: Arc::new(migration_files),
        })
    }

    pub fn holdings(&self) -> impl Iterator<Item = &PlatformHoldingIdentity> {
        self.holdings.values().map(|holding| &holding.identity)
    }

    fn holding(&self, id: &str) -> Result<&PlatformHoldingDatabase, BusinessError> {
        self.holdings.get(id).ok_or_else(|| {
            BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Holding not found.")
        })
    }

    async fn read_provisioning_snapshot(
        &self,
        slots: &[PlatformResourceSlot],
    ) -> Result<PlatformProvisioningSnapshot, BusinessError> {
        let unit = PgUnitOfWork::new(self.control.clone());
        let (rows, used) = unit
            .run(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let rows = sqlx::query(
                        "SELECT id,holding_id,slug,display_name,write_binding,read_binding,files_binding,status,\
                         requested_by,reason,error,version,requested_at,activated_at,updated_at \
                         FROM platform_holding_provisioning ORDER BY updated_at DESC,id DESC",
                    )
                    .fetch_all(&mut *connection)
                    .await?
                    .into_iter()
                    .map(|row| provisioning_row(&row))
                    .collect::<Result<Vec<_>, _>>()?;
                    let used = sqlx::query(
                        "SELECT holding_id,write_binding,read_binding,files_binding FROM holding_bindings \
                         WHERE status IN ('active','suspended')",
                    )
                    .fetch_all(&mut *connection)
                    .await?
                    .into_iter()
                    .flat_map(|row| {
                        let holding_id: String = row.get("holding_id");
                        let write: String = row.get("write_binding");
                        let read: String = row.get("read_binding");
                        let files: String = row.get("files_binding");
                        [holding_id, write, read, files]
                    })
                    .collect::<Vec<_>>();
                    Ok::<_, PgTransactionError<BusinessError>>((rows, used))
                })
            })
            .await
            .map_err(platform_business_error)?;
        Ok(PlatformProvisioningSnapshot {
            available_slots: available_resource_slots(slots, &rows, &used),
            rows,
        })
    }
}

fn provisioning_row(row: &sqlx::postgres::PgRow) -> Result<PlatformProvisioningRow, sqlx::Error> {
    Ok(PlatformProvisioningRow {
        id: row.try_get("id")?,
        holding_id: row.try_get("holding_id")?,
        slug: row.try_get("slug")?,
        display_name: row.try_get("display_name")?,
        write_binding: row.try_get("write_binding")?,
        read_binding: row.try_get("read_binding")?,
        files_binding: row.try_get("files_binding")?,
        status: row.try_get("status")?,
        requested_by: row.try_get("requested_by")?,
        reason: row.try_get("reason")?,
        error: row.try_get("error")?,
        version: row.try_get("version")?,
        requested_at: row.try_get("requested_at")?,
        activated_at: row.try_get("activated_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn migration_receipt(row: &sqlx::postgres::PgRow) -> Result<MigrationRunReceipt, BusinessError> {
    let status: String = row.try_get("status").map_err(|_| database_error())?;
    Ok(MigrationRunReceipt {
        request_hash: row.try_get("request_hash").map_err(|_| database_error())?,
        status: MigrationRunStatus::parse(&status).ok_or_else(receipt_unavailable)?,
        result_json: row.try_get("result_json").map_err(|_| database_error())?,
        error: row.try_get("error").map_err(|_| database_error())?,
    })
}

fn bounded_migration_error(
    error: &erpx_server_runtime::public::postgres::PersistenceError,
) -> String {
    let message = error.to_string();
    message.chars().take(MIGRATION_ERROR_MAX).collect()
}

enum MigrationStart {
    Started { run_id: String, version: i64 },
    Replay(RunHoldingMigrationResult),
}

impl PlatformAdministrationStore for PgPlatformAdministration {
    fn provisioning_snapshot<'a>(
        &'a self,
        slots: &'a [PlatformResourceSlot],
    ) -> PlatformFuture<'a, PlatformProvisioningSnapshot> {
        Box::pin(async move { self.read_provisioning_snapshot(slots).await })
    }

    fn provisioning_receipt<'a>(
        &'a self,
        key: &'a str,
    ) -> PlatformFuture<'a, Option<ProvisioningReceipt>> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT id,holding_id,slug,display_name,write_binding,read_binding,files_binding,status,\
                 requested_by,reason,error,version,requested_at,activated_at,updated_at,request_hash \
                 FROM platform_holding_provisioning WHERE idempotency_key=$1",
            )
            .bind(key)
            .fetch_optional(self.control.raw())
            .await
            .map_err(|_| database_error())?;
            row.map(|row| -> Result<ProvisioningReceipt, sqlx::Error> {
                Ok(ProvisioningReceipt {
                    request_hash: row.try_get("request_hash")?,
                    row: provisioning_row(&row)?,
                })
            })
            .transpose()
            .map_err(|_| database_error())
        })
    }

    fn provision<'a>(
        &'a self,
        plan: ProvisioningPlan,
    ) -> PlatformFuture<'a, ProvisionHoldingResult> {
        Box::pin(async move {
            let unit = PgUnitOfWork::new(self.control.clone());
            let key = plan.key.clone();
            let digest = plan.digest.clone();
            let options = TransactionOptions {
                session_lock: Some(SessionLock {
                    namespace: "platform-holding-provisioning".into(),
                    key: plan.slot.id.clone(),
                }),
                prefer_indexed_reads: false,
            };
            let result = unit
                .run(options, move |connection| {
                    let plan = plan.clone();
                    Box::pin(async move { provision_transaction(connection, &plan).await })
                })
                .await;
            match result {
                Ok(result) => Ok(result),
                Err(PgTransactionError::Persistence(error))
                    if error.sql_state() == Some("23505") =>
                {
                    if let Some(receipt) = self.provisioning_receipt(&key).await? {
                        if receipt.request_hash == digest {
                            return provisioning_result(&receipt.row);
                        }
                        return Err(BusinessError::new(
                            FailureKind::Conflict,
                            "IDEMPOTENCY_CONFLICT",
                            "This command key belongs to a different provisioning request.",
                        ));
                    }
                    Err(BusinessError::new(
                        FailureKind::Conflict,
                        "PROVISIONING_IN_PROGRESS",
                        "Another request is already provisioning this Holding. Refresh the Platform operations page and retry later.",
                    ))
                }
                Err(error) => Err(platform_business_error(error)),
            }
        })
    }

    fn control_health(&self) -> PlatformFuture<'_, Vec<ControlServiceHealth>> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT service,heartbeat_at,details_json FROM runtime_health ORDER BY service",
            )
            .fetch_all(self.control.raw())
            .await
            .map_err(|_| database_error())?;
            Ok(rows
                .into_iter()
                .map(|row| ControlServiceHealth {
                    service: row.get("service"),
                    heartbeat_at: row.get("heartbeat_at"),
                    details: row.get("details_json"),
                })
                .collect())
        })
    }

    fn holding_jobs<'a>(
        &'a self,
        holding: &'a PlatformHoldingIdentity,
        now: i64,
    ) -> PlatformFuture<'a, HoldingJobStatistics> {
        Box::pin(async move {
            let pool = &self.holding(&holding.id)?.pool;
            let counts = sqlx::query(
                "SELECT COUNT(*) FILTER (WHERE processed_at IS NULL AND dead_letter_at IS NULL AND (claim_until IS NULL OR claim_until<=$1))::bigint pending_events,\
                 COUNT(*) FILTER (WHERE dead_letter_at IS NOT NULL)::bigint dead_letter_events,\
                 COUNT(*) FILTER (WHERE processed_at IS NULL AND dead_letter_at IS NULL AND claim_until>$2)::bigint claimed_events,\
                 MIN(created_at) FILTER (WHERE processed_at IS NULL AND dead_letter_at IS NULL) oldest_pending_at \
                 FROM outbox_events",
            )
            .bind(now)
            .bind(now)
            .fetch_one(pool.raw())
            .await
            .map_err(|_| database_error())?;
            let dead_letters = sqlx::query(
                "SELECT id,event_type,aggregate_id,aggregate_version,attempts,last_error,dead_letter_at \
                 FROM outbox_events WHERE dead_letter_at IS NOT NULL \
                 ORDER BY dead_letter_at ASC,id ASC LIMIT $1",
            )
            .bind(DEAD_LETTER_LIMIT as i64)
            .fetch_all(pool.raw())
            .await
            .map_err(|_| database_error())?
            .into_iter()
            .map(|row| {
                Ok(PlatformDeadLetter {
                    id: row.try_get("id")?,
                    event_type: row.try_get("event_type")?,
                    aggregate_id: row.try_get("aggregate_id")?,
                    aggregate_version: row.try_get("aggregate_version")?,
                    attempts: row.try_get("attempts")?,
                    last_error: row.try_get("last_error")?,
                    dead_letter_at: row.try_get("dead_letter_at")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(|_| database_error())?;
            Ok(HoldingJobStatistics {
                pending_events: counts.get("pending_events"),
                dead_letter_events: counts.get("dead_letter_events"),
                claimed_events: counts.get("claimed_events"),
                oldest_pending_at: counts.get("oldest_pending_at"),
                dead_letters,
            })
        })
    }

    fn holding_migrations<'a>(
        &'a self,
        holding: &'a PlatformHoldingIdentity,
    ) -> PlatformFuture<'a, PlatformMigrationState> {
        Box::pin(async move {
            let pool = &self.holding(&holding.id)?.pool;
            let status = inspect_migrations(
                pool,
                MigrationTrack::HoldingWrite,
                self.migration_files.as_slice(),
            )
            .await
            .map_err(|_| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "MIGRATION_RECEIPTS_UNAVAILABLE",
                    "Migration receipts are unavailable for this Holding.",
                )
            })?;
            Ok(PlatformMigrationState {
                track: "holding-write".into(),
                applied: status.applied as i64,
                pending: status.pending,
                latest_applied: status.latest_applied,
                error: None,
            })
        })
    }

    fn recovery_receipt<'a>(
        &'a self,
        holding_id: &'a str,
        key: &'a str,
    ) -> PlatformFuture<'a, Option<RecoveryReceipt>> {
        Box::pin(async move {
            let pool = &self.holding(holding_id)?.pool;
            let row = sqlx::query(
                "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(pool.raw())
            .await
            .map_err(|_| database_error())?;
            row.map(|row| -> Result<RecoveryReceipt, sqlx::Error> {
                Ok(RecoveryReceipt {
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                })
            })
            .transpose()
            .map_err(|_| database_error())
        })
    }

    fn recover<'a>(&'a self, plan: RecoveryPlan) -> PlatformFuture<'a, RecoverDeadLetterResult> {
        Box::pin(async move {
            let pool = self.holding(&plan.holding_id)?.pool.clone();
            let holding_id = plan.holding_id.clone();
            let key = plan.key.clone();
            let digest = plan.digest.clone();
            let unit = PgUnitOfWork::new(pool);
            let options = TransactionOptions {
                session_lock: Some(SessionLock {
                    namespace: "platform-job-recovery".into(),
                    key: format!("{holding_id}:{key}"),
                }),
                prefer_indexed_reads: false,
            };
            let result = unit
                .run(options, move |connection| {
                    let plan = plan.clone();
                    Box::pin(async move { recover_transaction(connection, &plan).await })
                })
                .await;
            match result {
                Ok(result) => Ok(result),
                Err(error) => {
                    if let PgTransactionError::Persistence(_) = &error
                        && let Some(receipt) = self.recovery_receipt(&holding_id, &key).await?
                    {
                        if receipt.request_hash != digest {
                            return Err(BusinessError::new(
                                FailureKind::Conflict,
                                "IDEMPOTENCY_CONFLICT",
                                "This command key belongs to a different recovery request.",
                            ));
                        }
                        if let Some(raw) = receipt.result_json {
                            return serde_json::from_str(&raw).map_err(|_| {
                                BusinessError::new(
                                    FailureKind::Unavailable,
                                    "COMMAND_RECEIPT_INVALID",
                                    "The previous recovery receipt is unavailable.",
                                )
                            });
                        }
                    }
                    Err(platform_business_error(error))
                }
            }
        })
    }

    fn migration_run_receipt<'a>(
        &'a self,
        key: &'a str,
    ) -> PlatformFuture<'a, Option<MigrationRunReceipt>> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT request_hash,status,result_json,error FROM platform_migration_runs \
                 WHERE idempotency_key=$1",
            )
            .bind(key)
            .fetch_optional(self.control.raw())
            .await
            .map_err(|_| database_error())?;
            row.as_ref().map(migration_receipt).transpose()
        })
    }

    fn run_holding_migration<'a>(
        &'a self,
        plan: MigrationRunPlan,
    ) -> PlatformFuture<'a, RunHoldingMigrationResult> {
        Box::pin(async move {
            let pool = self.holding(&plan.holding.id)?.pool.clone();
            let files = self.migration_files.clone();
            let before = inspect_migrations(&pool, MigrationTrack::HoldingWrite, files.as_slice())
                .await
                .map_err(|_| receipt_unavailable())?;
            let started = self
                .start_migration_run(&plan, before.applied as i64)
                .await?;
            let (run_id, version) = match started {
                MigrationStart::Replay(result) => return Ok(result),
                MigrationStart::Started { run_id, version } => (run_id, version),
            };
            match migrate_database(&pool, MigrationTrack::HoldingWrite, files.as_slice()).await {
                Ok(_) => {
                    let after = match inspect_migrations(
                        &pool,
                        MigrationTrack::HoldingWrite,
                        files.as_slice(),
                    )
                    .await
                    {
                        Ok(after) => after,
                        Err(_) => {
                            self.fail_migration_run(
                                &plan,
                                &run_id,
                                version,
                                before.applied as i64,
                                "MIGRATION_RECEIPT_UNAVAILABLE",
                            )
                            .await;
                            return Err(receipt_unavailable());
                        }
                    };
                    let result = RunHoldingMigrationResult {
                        run_id: run_id.clone(),
                        holding_id: plan.holding.id.clone(),
                        track: MIGRATION_TRACK.into(),
                        status: MigrationRunStatus::Succeeded,
                        applied_before: before.applied as i64,
                        applied_after: after.applied as i64,
                        pending: after.pending,
                        finished_at: plan.now,
                    };
                    self.finish_migration_run(&plan, &run_id, version, &result)
                        .await
                }
                Err(error) => {
                    let applied_after =
                        inspect_migrations(&pool, MigrationTrack::HoldingWrite, files.as_slice())
                            .await
                            .map(|after| after.applied as i64)
                            .unwrap_or(before.applied as i64);
                    self.fail_migration_run(
                        &plan,
                        &run_id,
                        version,
                        applied_after,
                        &bounded_migration_error(&error),
                    )
                    .await;
                    Err(migration_failed())
                }
            }
        })
    }
}

impl PgPlatformAdministration {
    async fn start_migration_run(
        &self,
        plan: &MigrationRunPlan,
        applied_before: i64,
    ) -> Result<MigrationStart, BusinessError> {
        let unit = PgUnitOfWork::new(self.control.clone());
        let options = TransactionOptions {
            session_lock: Some(SessionLock {
                namespace: "platform-migration-start".into(),
                key: plan.key.clone(),
            }),
            prefer_indexed_reads: false,
        };
        let plan = plan.clone();
        unit.run(options, move |connection| {
            let plan = plan.clone();
            Box::pin(
                async move { start_migration_transaction(connection, &plan, applied_before).await },
            )
        })
        .await
        .map_err(platform_business_error)
    }

    async fn finish_migration_run(
        &self,
        plan: &MigrationRunPlan,
        run_id: &str,
        version: i64,
        result: &RunHoldingMigrationResult,
    ) -> Result<RunHoldingMigrationResult, BusinessError> {
        let result_json = serde_json::to_string(result).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "MIGRATION_RECEIPT_INVALID",
                "The migration receipt could not be retained.",
            )
        })?;
        let unit = PgUnitOfWork::new(self.control.clone());
        let options = TransactionOptions {
            session_lock: Some(SessionLock {
                namespace: "platform-migration-finish".into(),
                key: run_id.to_owned(),
            }),
            prefer_indexed_reads: false,
        };
        let plan = plan.clone();
        let run_id = run_id.to_owned();
        let result = result.clone();
        unit.run(options, move |connection| {
            let plan = plan.clone();
            let run_id = run_id.clone();
            let result = result.clone();
            let result_json = result_json.clone();
            Box::pin(async move {
                finalize_migration_success(
                    connection,
                    &plan,
                    &run_id,
                    version,
                    &result_json,
                    result,
                )
                .await
            })
        })
        .await
        .map_err(platform_business_error)
    }

    async fn fail_migration_run(
        &self,
        plan: &MigrationRunPlan,
        run_id: &str,
        version: i64,
        applied_after: i64,
        error: &str,
    ) {
        let unit = PgUnitOfWork::new(self.control.clone());
        let options = TransactionOptions {
            session_lock: Some(SessionLock {
                namespace: "platform-migration-fail".into(),
                key: run_id.to_owned(),
            }),
            prefer_indexed_reads: false,
        };
        let run_id = run_id.to_owned();
        let error = error.to_owned();
        let now = plan.now;
        let _ = unit
            .run(options, move |connection| {
                let run_id = run_id.clone();
                let error = error.clone();
                Box::pin(async move {
                    finalize_migration_failure(
                        connection,
                        &run_id,
                        version,
                        applied_after,
                        now,
                        &error,
                    )
                    .await
                })
            })
            .await;
    }
}

async fn start_migration_transaction(
    connection: &mut PgConnection,
    plan: &MigrationRunPlan,
    applied_before: i64,
) -> Result<MigrationStart, PgTransactionError<BusinessError>> {
    if let Some(row) = sqlx::query(
        "SELECT request_hash,status,result_json,error FROM platform_migration_runs \
         WHERE idempotency_key=$1 FOR UPDATE",
    )
    .bind(&plan.key)
    .fetch_optional(&mut *connection)
    .await?
    {
        let receipt = migration_receipt(&row).map_err(PgTransactionError::Business)?;
        if receipt.request_hash != plan.digest {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different migration request.",
            )
            .into());
        }
        return migration_run_result(&receipt)
            .map(MigrationStart::Replay)
            .map_err(PgTransactionError::Business);
    }
    let row = sqlx::query(
        "INSERT INTO platform_migration_runs(id,idempotency_key,request_hash,holding_id,track,status,\
         requested_by,assignment_id,reason,applied_before,applied_after,requested_at,started_at,version) \
         VALUES('platform-migration-'||gen_random_uuid()::text,$1,$2,$3,$4,'running',$5,$6,$7,$8,$8,$9,$9,1) \
         RETURNING id,version",
    )
    .bind(&plan.key)
    .bind(&plan.digest)
    .bind(&plan.holding.id)
    .bind(MIGRATION_TRACK)
    .bind(&plan.actor.user_id)
    .bind(&plan.actor.assignment_id)
    .bind(&plan.input.reason)
    .bind(applied_before)
    .bind(plan.now)
    .fetch_one(&mut *connection)
    .await?;
    Ok(MigrationStart::Started {
        run_id: row.try_get("id")?,
        version: row.try_get("version")?,
    })
}

async fn finalize_migration_success(
    connection: &mut PgConnection,
    plan: &MigrationRunPlan,
    run_id: &str,
    version: i64,
    result_json: &str,
    result: RunHoldingMigrationResult,
) -> Result<RunHoldingMigrationResult, PgTransactionError<BusinessError>> {
    let updated = sqlx::query(
        "UPDATE platform_migration_runs SET status='succeeded',applied_after=$1,result_json=$2,\
         finished_at=$3,version=version+1 WHERE id=$4 AND status='running' AND version=$5 \
         RETURNING version",
    )
    .bind(result.applied_after)
    .bind(result_json)
    .bind(plan.now)
    .bind(run_id)
    .bind(version)
    .fetch_optional(&mut *connection)
    .await?;
    if updated.is_none() {
        let row = sqlx::query(
            "SELECT request_hash,status,result_json,error FROM platform_migration_runs \
             WHERE idempotency_key=$1",
        )
        .bind(&plan.key)
        .fetch_optional(&mut *connection)
        .await?;
        let Some(row) = row else {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "MIGRATION_STATE_CHANGED",
                "This migration receipt changed. Refresh the operations page before retrying.",
            )
            .into());
        };
        let receipt = migration_receipt(&row).map_err(PgTransactionError::Business)?;
        if receipt.request_hash != plan.digest {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different migration request.",
            )
            .into());
        }
        return migration_run_result(&receipt).map_err(PgTransactionError::Business);
    }
    Ok(result)
}

async fn finalize_migration_failure(
    connection: &mut PgConnection,
    run_id: &str,
    version: i64,
    applied_after: i64,
    now: i64,
    error: &str,
) -> Result<(), PgTransactionError<BusinessError>> {
    sqlx::query(
        "UPDATE platform_migration_runs SET status='failed',error=$1,applied_after=$2,finished_at=$3,\
         version=version+1 WHERE id=$4 AND status='running' AND version=$5",
    )
    .bind(error)
    .bind(applied_after)
    .bind(now)
    .bind(run_id)
    .bind(version)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn provision_transaction(
    connection: &mut PgConnection,
    plan: &ProvisioningPlan,
) -> Result<ProvisionHoldingResult, PgTransactionError<BusinessError>> {
    if let Some(row) = sqlx::query(
        "SELECT id,holding_id,slug,display_name,write_binding,read_binding,files_binding,status,\
         requested_by,reason,error,version,requested_at,activated_at,updated_at,request_hash \
         FROM platform_holding_provisioning WHERE idempotency_key=$1 FOR UPDATE",
    )
    .bind(&plan.key)
    .fetch_optional(&mut *connection)
    .await?
    {
        let request_hash: String = row.try_get("request_hash")?;
        if request_hash != plan.digest {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different provisioning request.",
            )
            .into());
        }
        return provisioning_result(&provisioning_row(&row)?).map_err(PgTransactionError::Business);
    }
    let existing = sqlx::query(
        "SELECT id,holding_id,slug,display_name,write_binding,read_binding,files_binding,status,\
         requested_by,reason,error,version,requested_at,activated_at,updated_at,request_hash \
         FROM platform_holding_provisioning WHERE holding_id=$1 FOR UPDATE",
    )
    .bind(&plan.input.holding_id)
    .fetch_optional(&mut *connection)
    .await?;
    let existing = existing.as_ref().map(provisioning_row).transpose()?;
    if let Some(existing) = existing.as_ref().filter(|row| row.status == "active") {
        if existing.slug == plan.input.slug && existing.display_name == plan.input.display_name {
            return provisioning_result(existing).map_err(PgTransactionError::Business);
        }
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "HOLDING_ALREADY_BOUND",
            "This resource slot is already bound to a different active Holding or provisioning request.",
        )
        .into());
    }
    let slug_conflict =
        sqlx::query("SELECT holding_id FROM holding_bindings WHERE slug=$1 AND holding_id<>$2")
            .bind(&plan.input.slug)
            .bind(&plan.input.holding_id)
            .fetch_optional(&mut *connection)
            .await?;
    if slug_conflict.is_some() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SLUG_UNAVAILABLE",
            "Choose another company URL; it is already assigned to a Holding.",
        )
        .into());
    }
    let binding_conflict = sqlx::query(
        "SELECT holding_id FROM holding_bindings WHERE holding_id<>$1 AND \
         (write_binding IN ($2,$3,$4) OR read_binding IN ($2,$3,$4) OR files_binding IN ($2,$3,$4)) LIMIT 1",
    )
    .bind(&plan.input.holding_id)
    .bind(&plan.slot.write)
    .bind(&plan.slot.read)
    .bind(&plan.slot.files)
    .fetch_optional(&mut *connection)
    .await?;
    if binding_conflict.is_some() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "RESOURCE_ALREADY_BOUND",
            "This resource slot is already bound to another Holding.",
        )
        .into());
    }
    let id: String = match existing.as_ref() {
        Some(row) if row.status == "failed" => row.id.clone(),
        Some(_) => {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "PROVISIONING_IN_PROGRESS",
                "Another request is already provisioning this Holding. Refresh the Platform operations page and retry later.",
            )
            .into());
        }
        None => {
            sqlx::query_scalar("SELECT 'holding-provisioning-'||gen_random_uuid()::text")
                .fetch_one(&mut *connection)
                .await?
        }
    };
    let prior_version = existing.as_ref().map(|row| row.version).unwrap_or(0);
    let version = prior_version + 1;
    let retry = existing.as_ref().is_some_and(|row| row.status == "failed");
    if retry {
        sqlx::query(
            "UPDATE platform_holding_provisioning SET slug=$1,display_name=$2,write_binding=$3,read_binding=$4,\
             files_binding=$5,status='provisioning',requested_by=$6,assignment_id=$7,reason=$8,request_hash=$9,\
             idempotency_key=$10,version=$11,error=NULL,requested_at=$12,updated_at=$12 WHERE id=$13",
        )
        .bind(&plan.input.slug)
        .bind(&plan.input.display_name)
        .bind(&plan.input.write_binding)
        .bind(&plan.input.read_binding)
        .bind(&plan.input.files_binding)
        .bind(&plan.actor.user_id)
        .bind(&plan.actor.assignment_id)
        .bind(&plan.input.reason)
        .bind(&plan.digest)
        .bind(&plan.key)
        .bind(version)
        .bind(plan.now)
        .bind(&id)
        .execute(&mut *connection)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO platform_holding_provisioning(id,holding_id,slug,display_name,write_binding,\
             read_binding,files_binding,status,requested_by,assignment_id,reason,request_hash,idempotency_key,\
             version,requested_at,updated_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,'provisioning',$8,$9,$10,$11,$12,$13,$14,$14)",
        )
        .bind(&id)
        .bind(&plan.input.holding_id)
        .bind(&plan.input.slug)
        .bind(&plan.input.display_name)
        .bind(&plan.input.write_binding)
        .bind(&plan.input.read_binding)
        .bind(&plan.input.files_binding)
        .bind(&plan.actor.user_id)
        .bind(&plan.actor.assignment_id)
        .bind(&plan.input.reason)
        .bind(&plan.digest)
        .bind(&plan.key)
        .bind(version)
        .bind(plan.now)
        .execute(&mut *connection)
        .await?;
    }
    let detail = serde_json::json!({
        "slug": plan.input.slug,
        "resourceSlot": plan.slot.id,
    });
    event(
        connection,
        &id,
        &plan.input.holding_id,
        if retry { "retry_started" } else { "started" },
        plan,
        &detail,
        plan.now,
    )
    .await?;
    event(
        connection,
        &id,
        &plan.input.holding_id,
        "validated",
        plan,
        &serde_json::json!({"bindings": [plan.slot.write, plan.slot.read, plan.slot.files]}),
        plan.now + 1,
    )
    .await?;
    sqlx::query(
        "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) \
         VALUES($1,$2,$3,$4,$5,$6,'active') \
         ON CONFLICT(holding_id) DO UPDATE SET slug=excluded.slug,display_name=excluded.display_name,\
         write_binding=excluded.write_binding,read_binding=excluded.read_binding,files_binding=excluded.files_binding,status='active'",
    )
    .bind(&plan.input.holding_id)
    .bind(&plan.input.slug)
    .bind(&plan.input.display_name)
    .bind(&plan.input.write_binding)
    .bind(&plan.input.read_binding)
    .bind(&plan.input.files_binding)
    .execute(&mut *connection)
    .await?;
    let activated_at = plan.now + 2;
    let activated_version = version + 1;
    let updated = sqlx::query(
        "UPDATE platform_holding_provisioning SET status='active',activated_at=$1,updated_at=$1,version=$2 \
         WHERE id=$3 AND version=$4 AND status='provisioning'",
    )
    .bind(activated_at)
    .bind(activated_version)
    .bind(&id)
    .bind(version)
    .execute(&mut *connection)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "PROVISIONING_IN_PROGRESS",
            "Another request is already provisioning this Holding. Refresh the Platform operations page and retry later.",
        )
        .into());
    }
    event(
        connection,
        &id,
        &plan.input.holding_id,
        "activated",
        plan,
        &serde_json::json!({"version": activated_version}),
        activated_at,
    )
    .await?;
    Ok(ProvisionHoldingResult {
        id,
        holding_id: plan.input.holding_id.clone(),
        slug: plan.input.slug.clone(),
        display_name: plan.input.display_name.clone(),
        status: "active".into(),
        version: activated_version,
        activated_at,
    })
}

async fn event(
    connection: &mut PgConnection,
    provisioning_id: &str,
    holding_id: &str,
    action: &str,
    plan: &ProvisioningPlan,
    detail: &serde_json::Value,
    now: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO platform_holding_provisioning_events(id,provisioning_id,holding_id,action,actor_id,\
         assignment_id,reason,detail_json,created_at) \
         VALUES('platform-provisioning-event-'||gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(provisioning_id)
    .bind(holding_id)
    .bind(action)
    .bind(&plan.actor.user_id)
    .bind(&plan.actor.assignment_id)
    .bind(&plan.input.reason)
    .bind(detail.to_string())
    .bind(now)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn recover_transaction(
    connection: &mut PgConnection,
    plan: &RecoveryPlan,
) -> Result<RecoverDeadLetterResult, PgTransactionError<BusinessError>> {
    if let Some(row) = sqlx::query(
        "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
    )
    .bind(&plan.key)
    .fetch_optional(&mut *connection)
    .await?
    {
        let request_hash: String = row.try_get("request_hash")?;
        if request_hash != plan.digest {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different recovery request.",
            )
            .into());
        }
        let raw: Option<String> = row.try_get("result_json")?;
        let raw = raw.ok_or_else(|| {
            BusinessError::new(
                FailureKind::Conflict,
                "COMMAND_INCOMPLETE",
                "The previous recovery is still being finalized. Retry the same command.",
            )
        })?;
        let retained: RecoverDeadLetterResult = serde_json::from_str(&raw).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "COMMAND_RECEIPT_INVALID",
                "The previous recovery receipt is unavailable.",
            )
        })?;
        return Ok(retained);
    }
    let requeued = sqlx::query(
        "WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) \
         UPDATE outbox_events SET processed_at=NULL,published_at=NULL,claim_until=NULL,lease_token=NULL,\
         next_attempt_at=$1,last_error=NULL,dead_letter_at=NULL,attempts=0 FROM runtime_owner \
         WHERE id=$2 AND dead_letter_at IS NOT NULL AND attempts=$3 AND last_error IS NOT DISTINCT FROM $4 \
         RETURNING aggregate_version",
    )
    .bind(plan.now)
    .bind(&plan.input.event_id)
    .bind(plan.input.expected_attempts)
    .bind(&plan.input.expected_last_error)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(requeued) = requeued else {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "VERSION_CONFLICT",
            "This failed job changed. Refresh the operations page before retrying.",
        )
        .into());
    };
    let aggregate_version: i64 = requeued.try_get("aggregate_version")?;
    let result = RecoverDeadLetterResult {
        event_id: plan.input.event_id.clone(),
        holding_id: plan.holding_id.clone(),
        status: "queued".into(),
        previous_attempts: plan.input.expected_attempts,
        previous_error: plan.input.expected_last_error.clone(),
        recovered_at: plan.now,
    };
    let result_json = serde_json::to_string(&result).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "COMMAND_RECEIPT_INVALID",
            "The recovery receipt could not be retained.",
        )
    })?;
    let authority = serde_json::json!({
        "role": plan.actor.role,
        "assignmentId": plan.actor.assignment_id,
        "holdingId": plan.holding_id,
        "reason": plan.input.reason,
    })
    .to_string();
    sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,\
         created_at,authority_json) VALUES($1,'platform.job.retry',$2,$3,$4,$5,$6,$7)",
    )
    .bind(&plan.key)
    .bind(&plan.actor.user_id)
    .bind(&plan.digest)
    .bind(&plan.input.event_id)
    .bind(&result_json)
    .bind(plan.now)
    .bind(&authority)
    .execute(&mut *connection)
    .await?;
    let audit_authority = serde_json::json!({
        "role": plan.actor.role,
        "assignmentId": plan.actor.assignment_id,
        "holdingId": plan.holding_id,
    })
    .to_string();
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,\
         record_version,purpose,created_at,authority_json) \
         VALUES('platform-recovery-audit-'||gen_random_uuid()::text,$1,$2,$3,'platform.job.retry',\
         'runtime_job',$4,$5,$6,$7,$8)",
    )
    .bind(&plan.key)
    .bind(&plan.actor.user_id)
    .bind(&plan.actor.assignment_id)
    .bind(&plan.input.event_id)
    .bind(aggregate_version)
    .bind(&plan.input.reason)
    .bind(plan.now)
    .bind(&audit_authority)
    .execute(&mut *connection)
    .await?;
    Ok(result)
}
