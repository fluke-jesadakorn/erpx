//! Platform Administration use cases.
//!
//! Authorization is limited to the current Platform role supplied by the
//! composition backend. Every command validates the exact typed input before
//! replay, then defers the single transaction to the owner store.
use crate::domain::platform_operations::*;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::future::Future;
use std::pin::Pin;

pub type PlatformFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner store boundary. The Control adapter owns provisioning and health
/// reads; each configured Holding adapter owns its job, receipt and migration
/// reads inside the caller's selected Holding scope.
pub trait PlatformAdministrationStore: Send + Sync {
    fn provisioning_snapshot<'a>(
        &'a self,
        slots: &'a [PlatformResourceSlot],
    ) -> PlatformFuture<'a, PlatformProvisioningSnapshot>;
    fn provisioning_receipt<'a>(
        &'a self,
        key: &'a str,
    ) -> PlatformFuture<'a, Option<ProvisioningReceipt>>;
    fn provision<'a>(
        &'a self,
        plan: ProvisioningPlan,
    ) -> PlatformFuture<'a, ProvisionHoldingResult>;
    fn control_health(&self) -> PlatformFuture<'_, Vec<ControlServiceHealth>>;
    fn holding_jobs<'a>(
        &'a self,
        holding: &'a PlatformHoldingIdentity,
        now: i64,
    ) -> PlatformFuture<'a, HoldingJobStatistics>;
    fn holding_migrations<'a>(
        &'a self,
        holding: &'a PlatformHoldingIdentity,
    ) -> PlatformFuture<'a, PlatformMigrationState>;
    fn recovery_receipt<'a>(
        &'a self,
        holding_id: &'a str,
        key: &'a str,
    ) -> PlatformFuture<'a, Option<RecoveryReceipt>>;
    fn recover<'a>(&'a self, plan: RecoveryPlan) -> PlatformFuture<'a, RecoverDeadLetterResult>;
    fn migration_run_receipt<'a>(
        &'a self,
        key: &'a str,
    ) -> PlatformFuture<'a, Option<MigrationRunReceipt>>;
    fn run_holding_migration<'a>(
        &'a self,
        plan: MigrationRunPlan,
    ) -> PlatformFuture<'a, RunHoldingMigrationResult>;
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

pub async fn provisioning_snapshot<S: PlatformAdministrationStore + ?Sized>(
    actor: &PlatformActor,
    store: &S,
    slots: &[PlatformResourceSlot],
) -> Result<PlatformProvisioningSnapshot, BusinessError> {
    require_platform_read(actor)?;
    store.provisioning_snapshot(slots).await
}

pub async fn platform_operations_summary<S: PlatformAdministrationStore + ?Sized>(
    actor: &PlatformActor,
    store: &S,
    holdings: &[PlatformHoldingIdentity],
    now: i64,
) -> Result<PlatformOperationsSummary, BusinessError> {
    require_platform_read(actor)?;
    let control_health = store.control_health().await?;
    let job_heartbeat_at = control_health
        .iter()
        .find(|health| health.service == "jobs")
        .map(|health| health.heartbeat_at);
    let mut rows = Vec::with_capacity(holdings.len());
    for holding in holdings {
        let statistics = match store.holding_jobs(holding, now).await {
            Ok(statistics) => statistics,
            Err(_) => {
                rows.push(unavailable_holding_operation(holding, job_heartbeat_at));
                continue;
            }
        };
        let migrations = match store.holding_migrations(holding).await {
            Ok(state) => state,
            Err(_) => PlatformMigrationState {
                track: "holding-write".into(),
                applied: 0,
                pending: Vec::new(),
                latest_applied: None,
                error: Some("Migration receipts are unavailable for this Holding.".into()),
            },
        };
        rows.push(PlatformHoldingOperation {
            id: holding.id.clone(),
            slug: holding.slug.clone(),
            display_name: holding.display_name.clone(),
            status: holding_operation_status(&statistics).into(),
            pending_events: statistics.pending_events,
            dead_letter_events: statistics.dead_letter_events,
            claimed_events: statistics.claimed_events,
            oldest_pending_at: statistics.oldest_pending_at,
            job_heartbeat_at,
            error: None,
            dead_letters: statistics.dead_letters,
            migrations,
        });
    }
    let totals = operation_totals(&rows);
    Ok(PlatformOperationsSummary {
        generated_at: now,
        actor_role: actor.role.clone(),
        control_health,
        holdings: rows,
        totals,
    })
}

pub async fn platform_operations_snapshot<S: PlatformAdministrationStore + ?Sized>(
    actor: &PlatformActor,
    store: &S,
    holdings: &[PlatformHoldingIdentity],
    slots: &[PlatformResourceSlot],
    now: i64,
) -> Result<PlatformOperationsSnapshot, BusinessError> {
    require_platform_read(actor)?;
    let provisioning = store.provisioning_snapshot(slots).await?;
    let operations = platform_operations_summary(actor, store, holdings, now).await?;
    Ok(PlatformOperationsSnapshot {
        provisioning,
        operations,
    })
}

/// The admin operations route consumes only the operational projection; this
/// explicit contract lets it replace the retained TypeScript summary without
/// also reading the provisioning page payload.
pub async fn platform_operations_projection<S: PlatformAdministrationStore + ?Sized>(
    actor: &PlatformActor,
    store: &S,
    holdings: &[PlatformHoldingIdentity],
    now: i64,
) -> Result<PlatformOperationsProjection, BusinessError> {
    platform_operations_summary(actor, store, holdings, now).await
}

pub async fn provision_holding<S: PlatformAdministrationStore + ?Sized>(
    actor: &PlatformActor,
    store: &S,
    slot: &PlatformResourceSlot,
    raw: &ProvisionHoldingInput,
    key: &str,
    now: i64,
) -> Result<ProvisionHoldingResult, BusinessError> {
    require_platform_admin(actor)?;
    validate_command_key(key)?;
    let input = validate_provisioning(actor, slot, raw)?;
    let digest = provisioning_digest(actor, key, &input);
    if let Some(receipt) = store.provisioning_receipt(key).await? {
        if receipt.request_hash != digest {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different provisioning request.",
            ));
        }
        return provisioning_result(&receipt.row);
    }
    store
        .provision(ProvisioningPlan {
            actor: actor.clone(),
            slot: slot.clone(),
            input,
            key: key.to_owned(),
            digest,
            now,
        })
        .await
}

pub async fn recover_dead_letter_job<S: PlatformAdministrationStore + ?Sized>(
    actor: &PlatformActor,
    store: &S,
    holding: &PlatformHoldingIdentity,
    raw: &RecoverDeadLetterInput,
    key: &str,
    now: i64,
) -> Result<RecoverDeadLetterResult, BusinessError> {
    require_platform_admin(actor)?;
    validate_command_key(key)?;
    let input = validate_recovery(actor, &holding.id, raw)?;
    let digest = recovery_digest(actor, key, &holding.id, &input);
    if let Some(receipt) = store.recovery_receipt(&holding.id, key).await? {
        if receipt.request_hash != digest {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different recovery request.",
            ));
        }
        let retained = receipt.result_json.ok_or_else(|| {
            conflict(
                "COMMAND_INCOMPLETE",
                "The previous recovery is still being finalized. Retry the same command.",
            )
        })?;
        return serde_json::from_str(&retained).map_err(|_| {
            unavailable(
                "COMMAND_RECEIPT_INVALID",
                "The previous recovery receipt is unavailable.",
            )
        });
    }
    store
        .recover(RecoveryPlan {
            actor: actor.clone(),
            holding_id: holding.id.clone(),
            input,
            key: key.to_owned(),
            digest,
            now,
        })
        .await
}

pub async fn run_holding_migration<S: PlatformAdministrationStore + ?Sized>(
    actor: &PlatformActor,
    store: &S,
    holding: &PlatformHoldingIdentity,
    raw: &RunHoldingMigrationInput,
    key: &str,
    now: i64,
) -> Result<RunHoldingMigrationResult, BusinessError> {
    require_platform_admin(actor)?;
    validate_command_key(key)?;
    let input = validate_migration_run(actor, &holding.id, raw)?;
    let digest = migration_digest(actor, key, &holding.id, &input);
    if let Some(receipt) = store.migration_run_receipt(key).await? {
        if receipt.request_hash != digest {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different migration request.",
            ));
        }
        return migration_run_result(&receipt);
    }
    store
        .run_holding_migration(MigrationRunPlan {
            actor: actor.clone(),
            holding: holding.clone(),
            input,
            key: key.to_owned(),
            digest,
            now,
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Store {
        receipt: Mutex<Option<ProvisioningReceipt>>,
        recovery: Mutex<Option<RecoveryReceipt>>,
        migration: Mutex<Option<MigrationRunReceipt>>,
        provisioning_error: Mutex<Option<BusinessError>>,
        jobs_error: Mutex<Option<BusinessError>>,
        migration_error: Mutex<Option<BusinessError>>,
        calls: Mutex<Vec<String>>,
    }
    impl PlatformAdministrationStore for Store {
        fn provisioning_snapshot<'a>(
            &'a self,
            slots: &'a [PlatformResourceSlot],
        ) -> PlatformFuture<'a, PlatformProvisioningSnapshot> {
            Box::pin(async move {
                if let Some(error) = self.provisioning_error.lock().unwrap().clone() {
                    return Err(error);
                }
                Ok(PlatformProvisioningSnapshot {
                    rows: Vec::new(),
                    available_slots: slots.to_vec(),
                })
            })
        }
        fn provisioning_receipt<'a>(
            &'a self,
            _: &'a str,
        ) -> PlatformFuture<'a, Option<ProvisioningReceipt>> {
            Box::pin(async move { Ok(self.receipt.lock().unwrap().clone()) })
        }
        fn provision<'a>(
            &'a self,
            plan: ProvisioningPlan,
        ) -> PlatformFuture<'a, ProvisionHoldingResult> {
            Box::pin(async move {
                self.calls
                    .lock()
                    .unwrap()
                    .push(format!("provision:{}", plan.key));
                Ok(ProvisionHoldingResult {
                    id: "provisioning".into(),
                    holding_id: plan.input.holding_id,
                    slug: plan.input.slug,
                    display_name: plan.input.display_name,
                    status: "active".into(),
                    version: 2,
                    activated_at: plan.now,
                })
            })
        }
        fn control_health(&self) -> PlatformFuture<'_, Vec<ControlServiceHealth>> {
            Box::pin(async {
                Ok(vec![ControlServiceHealth {
                    service: "jobs".into(),
                    heartbeat_at: 3,
                    details: "{}".into(),
                }])
            })
        }
        fn holding_jobs<'a>(
            &'a self,
            holding: &'a PlatformHoldingIdentity,
            _: i64,
        ) -> PlatformFuture<'a, HoldingJobStatistics> {
            Box::pin(async move {
                if let Some(error) = self.jobs_error.lock().unwrap().clone() {
                    return Err(error);
                }
                Ok(HoldingJobStatistics {
                    pending_events: 1,
                    dead_letter_events: 0,
                    claimed_events: 2,
                    oldest_pending_at: Some(4),
                    dead_letters: vec![PlatformDeadLetter {
                        id: format!("{}-event", holding.id),
                        event_type: "test".into(),
                        aggregate_id: "aggregate".into(),
                        aggregate_version: 1,
                        attempts: 1,
                        last_error: None,
                        dead_letter_at: 5,
                    }],
                })
            })
        }
        fn holding_migrations<'a>(
            &'a self,
            _: &'a PlatformHoldingIdentity,
        ) -> PlatformFuture<'a, PlatformMigrationState> {
            Box::pin(async move {
                if let Some(error) = self.migration_error.lock().unwrap().clone() {
                    return Err(error);
                }
                Ok(PlatformMigrationState {
                    track: "holding-write".into(),
                    applied: 1,
                    pending: Vec::new(),
                    latest_applied: Some("0001".into()),
                    error: None,
                })
            })
        }
        fn recovery_receipt<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
        ) -> PlatformFuture<'a, Option<RecoveryReceipt>> {
            Box::pin(async move { Ok(self.recovery.lock().unwrap().clone()) })
        }
        fn recover<'a>(
            &'a self,
            plan: RecoveryPlan,
        ) -> PlatformFuture<'a, RecoverDeadLetterResult> {
            Box::pin(async move {
                self.calls
                    .lock()
                    .unwrap()
                    .push(format!("recover:{}", plan.key));
                Ok(RecoverDeadLetterResult {
                    event_id: plan.input.event_id,
                    holding_id: plan.holding_id,
                    status: "queued".into(),
                    previous_attempts: plan.input.expected_attempts,
                    previous_error: plan.input.expected_last_error,
                    recovered_at: plan.now,
                })
            })
        }
        fn migration_run_receipt<'a>(
            &'a self,
            _: &'a str,
        ) -> PlatformFuture<'a, Option<MigrationRunReceipt>> {
            Box::pin(async move { Ok(self.migration.lock().unwrap().clone()) })
        }
        fn run_holding_migration<'a>(
            &'a self,
            plan: MigrationRunPlan,
        ) -> PlatformFuture<'a, RunHoldingMigrationResult> {
            Box::pin(async move {
                self.calls
                    .lock()
                    .unwrap()
                    .push(format!("migrate:{}", plan.key));
                Ok(RunHoldingMigrationResult {
                    run_id: format!("migration-{}", plan.holding.id),
                    holding_id: plan.holding.id,
                    track: plan.input.track,
                    status: MigrationRunStatus::Succeeded,
                    applied_before: 3,
                    applied_after: 4,
                    pending: Vec::new(),
                    finished_at: plan.now,
                })
            })
        }
    }

    fn actor(role: &str) -> PlatformActor {
        PlatformActor {
            user_id: "operator".into(),
            assignment_id: "platform-assignment".into(),
            role: role.into(),
        }
    }
    fn slot() -> PlatformResourceSlot {
        PlatformResourceSlot {
            id: "h004".into(),
            write: "H004_DB".into(),
            read: "H004_READ_DB".into(),
            files: "H004_FILES".into(),
            beta: false,
            internal: false,
        }
    }
    fn input() -> ProvisionHoldingInput {
        ProvisionHoldingInput {
            holding_id: "h004".into(),
            slug: "northstar".into(),
            display_name: "Northstar".into(),
            write_binding: "H004_DB".into(),
            read_binding: "H004_READ_DB".into(),
            files_binding: "H004_FILES".into(),
            reason: "Reviewed onboarding and verified the isolated resource health.".into(),
        }
    }
    fn holding() -> PlatformHoldingIdentity {
        PlatformHoldingIdentity {
            id: "h001".into(),
            slug: "acme".into(),
            display_name: "Acme".into(),
        }
    }

    #[tokio::test]
    async fn a_provisioning_replay_returns_the_retained_result_before_a_new_write() {
        let store = Store::default();
        let first = provision_holding(
            &actor("platform_admin"),
            &store,
            &slot(),
            &input(),
            "provision-key-01",
            10,
        )
        .await
        .unwrap();
        let digest = provisioning_digest(
            &actor("platform_admin"),
            "provision-key-01",
            &validate_provisioning(&actor("platform_admin"), &slot(), &input()).unwrap(),
        );
        *store.receipt.lock().unwrap() = Some(ProvisioningReceipt {
            request_hash: digest,
            row: PlatformProvisioningRow {
                id: first.id.clone(),
                holding_id: first.holding_id.clone(),
                slug: first.slug.clone(),
                display_name: first.display_name.clone(),
                write_binding: "H004_DB".into(),
                read_binding: "H004_READ_DB".into(),
                files_binding: "H004_FILES".into(),
                status: "active".into(),
                requested_by: "operator".into(),
                reason: "reason".into(),
                error: None,
                version: 2,
                requested_at: 10,
                activated_at: Some(10),
                updated_at: 10,
            },
        });
        let replay = provision_holding(
            &actor("platform_admin"),
            &store,
            &slot(),
            &input(),
            "provision-key-01",
            11,
        )
        .await
        .unwrap();
        assert_eq!(replay.id, first.id);
        assert_eq!(store.calls.lock().unwrap().len(), 1);
        assert!(
            provision_holding(
                &actor("platform_auditor"),
                &store,
                &slot(),
                &input(),
                "provision-key-01",
                11,
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn a_changed_key_or_retained_incomplete_state_fails_closed() {
        let store = Store::default();
        let actor = actor("platform_admin");
        *store.receipt.lock().unwrap() = Some(ProvisioningReceipt {
            request_hash: "different".into(),
            row: PlatformProvisioningRow {
                id: "provisioning".into(),
                holding_id: "h004".into(),
                slug: "northstar".into(),
                display_name: "Northstar".into(),
                write_binding: "H004_DB".into(),
                read_binding: "H004_READ_DB".into(),
                files_binding: "H004_FILES".into(),
                status: "active".into(),
                requested_by: "operator".into(),
                reason: "reason".into(),
                error: None,
                version: 2,
                requested_at: 10,
                activated_at: Some(10),
                updated_at: 10,
            },
        });
        let error = provision_holding(&actor, &store, &slot(), &input(), "provision-key-01", 10)
            .await
            .unwrap_err();
        assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");
        *store.receipt.lock().unwrap() = Some(ProvisioningReceipt {
            request_hash: provisioning_digest(
                &actor,
                "provision-key-01",
                &validate_provisioning(&actor, &slot(), &input()).unwrap(),
            ),
            row: PlatformProvisioningRow {
                id: "provisioning".into(),
                holding_id: "h004".into(),
                slug: "northstar".into(),
                display_name: "Northstar".into(),
                write_binding: "H004_DB".into(),
                read_binding: "H004_READ_DB".into(),
                files_binding: "H004_FILES".into(),
                status: "provisioning".into(),
                requested_by: "operator".into(),
                reason: "reason".into(),
                error: None,
                version: 1,
                requested_at: 10,
                activated_at: None,
                updated_at: 10,
            },
        });
        let error = provision_holding(&actor, &store, &slot(), &input(), "provision-key-01", 10)
            .await
            .unwrap_err();
        assert_eq!(error.code, "PROVISIONING_INCOMPLETE");
    }

    #[tokio::test]
    async fn operations_summary_keeps_unavailable_holdings_and_reports_migration_failure() {
        let store = Store::default();
        *store.jobs_error.lock().unwrap() = Some(BusinessError::new(
            FailureKind::Unavailable,
            "DATABASE_UNAVAILABLE",
            "offline",
        ));
        let summary =
            platform_operations_summary(&actor("platform_auditor"), &store, &[holding()], 10)
                .await
                .unwrap();
        assert_eq!(summary.holdings[0].status, "unavailable");
        assert_eq!(summary.totals.holdings, 1);
        assert_eq!(summary.control_health[0].service, "jobs");
        assert_eq!(summary.holdings[0].job_heartbeat_at, Some(3));

        let store = Store::default();
        *store.migration_error.lock().unwrap() = Some(BusinessError::new(
            FailureKind::Unavailable,
            "MIGRATION_UNAVAILABLE",
            "offline",
        ));
        let summary =
            platform_operations_summary(&actor("platform_admin"), &store, &[holding()], 10)
                .await
                .unwrap();
        assert_eq!(summary.holdings[0].status, "pending");
        assert_eq!(
            summary.holdings[0].migrations.error.as_deref(),
            Some("Migration receipts are unavailable for this Holding.")
        );
    }

    #[tokio::test]
    async fn recovery_replay_and_stale_identity_fail_closed() {
        let store = Store::default();
        let holding = holding();
        let input = RecoverDeadLetterInput {
            event_id: "platform-job-recovery-test-1".into(),
            expected_attempts: 6,
            expected_last_error: Some("PROJECTOR_TIMEOUT".into()),
            reason: "Provider is healthy and the projection is safe to replay.".into(),
        };
        let result = recover_dead_letter_job(
            &actor("platform_admin"),
            &store,
            &holding,
            &input,
            "recovery-key-01",
            10,
        )
        .await
        .unwrap();
        assert_eq!(result.status, "queued");
        assert_eq!(store.calls.lock().unwrap().len(), 1);
        let digest = recovery_digest(
            &actor("platform_admin"),
            "recovery-key-01",
            &holding.id,
            &validate_recovery(&actor("platform_admin"), &holding.id, &input).unwrap(),
        );
        *store.recovery.lock().unwrap() = Some(RecoveryReceipt {
            request_hash: digest,
            result_json: Some(serde_json::to_string(&result).unwrap()),
        });
        let replay = recover_dead_letter_job(
            &actor("platform_admin"),
            &store,
            &holding,
            &input,
            "recovery-key-01",
            11,
        )
        .await
        .unwrap();
        assert_eq!(replay, result);
        let error = recover_dead_letter_job(
            &actor("platform_admin"),
            &store,
            &holding,
            &input,
            "different-key-01",
            11,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");
        assert!(
            recover_dead_letter_job(
                &actor("platform_auditor"),
                &store,
                &holding,
                &input,
                "recovery-key-02",
                11,
            )
            .await
            .is_err()
        );
    }

    fn migration_input() -> RunHoldingMigrationInput {
        RunHoldingMigrationInput {
            holding_id: "h001".into(),
            track: MIGRATION_TRACK.into(),
            reason: "Apply the reviewed release schema before enabling the next workflow.".into(),
        }
    }

    #[tokio::test]
    async fn a_migration_replay_returns_the_retained_result_before_a_new_run() {
        let store = Store::default();
        let holding = holding();
        let first = run_holding_migration(
            &actor("platform_admin"),
            &store,
            &holding,
            &migration_input(),
            "platform-migration-key-1",
            10,
        )
        .await
        .unwrap();
        assert_eq!(first.status, MigrationRunStatus::Succeeded);
        assert_eq!(first.applied_after, 4);
        assert_eq!(store.calls.lock().unwrap().len(), 1);
        let digest = migration_digest(
            &actor("platform_admin"),
            "platform-migration-key-1",
            &holding.id,
            &validate_migration_run(&actor("platform_admin"), &holding.id, &migration_input())
                .unwrap(),
        );
        *store.migration.lock().unwrap() = Some(MigrationRunReceipt {
            request_hash: digest,
            status: MigrationRunStatus::Succeeded,
            result_json: Some(serde_json::to_string(&first).unwrap()),
            error: None,
        });
        let replay = run_holding_migration(
            &actor("platform_admin"),
            &store,
            &holding,
            &migration_input(),
            "platform-migration-key-1",
            11,
        )
        .await
        .unwrap();
        assert_eq!(replay, first);
        assert_eq!(store.calls.lock().unwrap().len(), 1);
        let conflict = run_holding_migration(
            &actor("platform_admin"),
            &store,
            &holding,
            &RunHoldingMigrationInput {
                reason: "A different request using the same command key.".into(),
                ..migration_input()
            },
            "platform-migration-key-1",
            11,
        )
        .await
        .unwrap_err();
        assert_eq!(conflict.code, "IDEMPOTENCY_CONFLICT");
    }

    #[tokio::test]
    async fn migration_authority_track_and_identity_are_checked_before_the_store() {
        let store = Store::default();
        let holding = holding();
        for (actor, input) in [
            (actor("platform_auditor"), migration_input()),
            (
                actor("platform_admin"),
                RunHoldingMigrationInput {
                    track: "holding-read".into(),
                    ..migration_input()
                },
            ),
            (
                actor("platform_admin"),
                RunHoldingMigrationInput {
                    reason: " x ".into(),
                    ..migration_input()
                },
            ),
            (
                actor("platform_admin"),
                RunHoldingMigrationInput {
                    holding_id: "h002".into(),
                    ..migration_input()
                },
            ),
        ] {
            assert!(
                run_holding_migration(
                    &actor,
                    &store,
                    &holding,
                    &input,
                    "platform-migration-key-check",
                    10,
                )
                .await
                .is_err()
            );
        }
        assert!(store.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn running_failed_and_incomplete_migration_receipts_fail_closed_without_a_new_run() {
        let store = Store::default();
        let holding = holding();
        let actor = actor("platform_admin");
        let digest = migration_digest(
            &actor,
            "platform-migration-key-2",
            &holding.id,
            &validate_migration_run(&actor, &holding.id, &migration_input()).unwrap(),
        );
        for (status, result_json, code) in [
            (MigrationRunStatus::Running, None, "MIGRATION_IN_PROGRESS"),
            (MigrationRunStatus::Failed, None, "MIGRATION_FAILED"),
            (
                MigrationRunStatus::Succeeded,
                None,
                "COMMAND_RECEIPT_INVALID",
            ),
        ] {
            *store.migration.lock().unwrap() = Some(MigrationRunReceipt {
                request_hash: digest.clone(),
                status,
                result_json,
                error: None,
            });
            let error = run_holding_migration(
                &actor,
                &store,
                &holding,
                &migration_input(),
                "platform-migration-key-2",
                10,
            )
            .await
            .unwrap_err();
            assert_eq!(error.code, code);
        }
        assert!(store.calls.lock().unwrap().is_empty());
    }
}
