//! Actual Platform Administration control-plane lifecycle over leased PostgreSQL.
//!
//! These tests use the real Control and Holding migration tracks, owner tables,
//! persisted receipts/events/audit rows and the owner HTTP router. They do not
//! exercise the shared composition backend or an authenticated browser.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_platform_administration::public::platform_operations::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, inspect_migrations, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Row;
use std::sync::Arc;

const NOW: i64 = 1_800_000_000_000;

fn actor(role: &str) -> PlatformActor {
    PlatformActor {
        user_id: "platform-operator".into(),
        assignment_id: "platform-assignment".into(),
        role: role.into(),
    }
}
fn admin() -> PlatformActor {
    actor("platform_admin")
}
fn auditor() -> PlatformActor {
    actor("platform_auditor")
}
fn resource_slot(id: &str) -> PlatformResourceSlot {
    let upper = id.to_ascii_uppercase();
    PlatformResourceSlot {
        id: id.into(),
        write: format!("{upper}_DB"),
        read: format!("{upper}_READ_DB"),
        files: format!("{upper}_FILES"),
        beta: false,
        internal: false,
    }
}
fn input(slot: &PlatformResourceSlot, slug: &str) -> ProvisionHoldingInput {
    ProvisionHoldingInput {
        holding_id: slot.id.clone(),
        slug: slug.into(),
        display_name: format!("Holding {slug}"),
        write_binding: slot.write.clone(),
        read_binding: slot.read.clone(),
        files_binding: slot.files.clone(),
        reason: "Reviewed onboarding and verified the isolated resource health.".into(),
    }
}
fn holding_identity(id: &str, slug: &str) -> PlatformHoldingIdentity {
    PlatformHoldingIdentity {
        id: id.into(),
        slug: slug.into(),
        display_name: format!("Holding {slug}"),
    }
}
fn migration_input(holding_id: &str) -> RunHoldingMigrationInput {
    RunHoldingMigrationInput {
        holding_id: holding_id.into(),
        track: MIGRATION_TRACK.into(),
        reason: "Apply the reviewed release schema before enabling the next workflow.".into(),
    }
}
async fn migrate(pool: &DatabasePool, track: MigrationTrack) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, track).await.unwrap();
    migrate_database(pool, track, &files).await.unwrap();
}
async fn owner(
    control: &DatabasePool,
    holdings: Vec<PlatformHoldingDatabase>,
) -> PgPlatformAdministration {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    PgPlatformAdministration::new(control.clone(), holdings, files).unwrap()
}
async fn seed_binding(control: &DatabasePool, holding_id: &str, slug: &str) {
    let upper = holding_id.to_ascii_uppercase();
    sqlx::query(
        "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) \
         VALUES($1,$2,$3,$4,$5,$6,'active')",
    )
    .bind(holding_id)
    .bind(slug)
    .bind(format!("Holding {slug}"))
    .bind(format!("{upper}_DB"))
    .bind(format!("{upper}_READ_DB"))
    .bind(format!("{upper}_FILES"))
    .execute(control.raw())
    .await
    .unwrap();
}
async fn seed_event(holding: &DatabasePool, id: &str, now: i64) {
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
         payload_json,created_at,next_attempt_at) VALUES($1,$2,'test.operation','test','aggregate',1,'{}',$3,$3)",
    )
    .bind(id)
    .bind(format!("command-{id}"))
    .bind(now)
    .execute(holding.raw())
    .await
    .unwrap();
}
async fn count(pool: &DatabasePool, query: &'static str) -> i64 {
    sqlx::query_scalar(query)
        .fetch_one(pool.raw())
        .await
        .unwrap()
}

#[tokio::test]
async fn provisioning_activates_once_retains_history_and_rolls_back_a_failed_binding() {
    fixture::with_database(3, |control| async move {
        fixture::with_database(2, move |holding| async move {
            migrate(&control, MigrationTrack::Control).await;
            migrate(&holding, MigrationTrack::HoldingWrite).await;
            seed_binding(&control, "h001", "acme").await;
            let store = owner(
                &control,
                vec![PlatformHoldingDatabase {
                    identity: holding_identity("h001", "acme"),
                    pool: holding.clone(),
                }],
            )
            .await;
            let slot = resource_slot("h004");
            let before = provisioning_snapshot(&admin(), &store, std::slice::from_ref(&slot))
                .await
                .unwrap();
            assert!(before.rows.is_empty());
            assert_eq!(before.available_slots, vec![slot.clone()]);
            let first = provision_holding(
                &admin(),
                &store,
                &slot,
                &input(&slot, "northstar"),
                "platform-provisioning-key-1",
                NOW,
            )
            .await
            .unwrap();
            assert_eq!(first.holding_id, "h004");
            assert_eq!(first.slug, "northstar");
            assert_eq!(first.status, "active");
            assert_eq!(first.version, 2);
            let binding = sqlx::query(
                "SELECT status,slug,write_binding FROM holding_bindings WHERE holding_id='h004'",
            )
            .fetch_one(control.raw())
            .await
            .unwrap();
            assert_eq!(binding.get::<String, _>("status"), "active");
            assert_eq!(binding.get::<String, _>("slug"), "northstar");
            assert_eq!(binding.get::<String, _>("write_binding"), "H004_DB");
            let actions: Vec<String> = sqlx::query_scalar(
                "SELECT action FROM platform_holding_provisioning_events WHERE holding_id='h004' ORDER BY created_at,id",
            )
            .fetch_all(control.raw())
            .await
            .unwrap();
            assert_eq!(actions, ["started", "validated", "activated"]);
            let replay = provision_holding(
                &admin(),
                &store,
                &slot,
                &input(&slot, "northstar"),
                "platform-provisioning-key-1",
                NOW + 1,
            )
            .await
            .unwrap();
            assert_eq!(replay, first);
            assert_eq!(
                count(
                    &control,
                    "SELECT count(*)::bigint FROM platform_holding_provisioning_events WHERE holding_id='h004'"
                )
                .await,
                3
            );
            let after = provisioning_snapshot(&admin(), &store, std::slice::from_ref(&slot))
                .await
                .unwrap();
            assert_eq!(after.rows.len(), 1);
            assert!(after.available_slots.is_empty());
            let conflict = provision_holding(
                &admin(),
                &store,
                &slot,
                &input(&slot, "different"),
                "platform-provisioning-key-2",
                NOW + 2,
            )
            .await
            .unwrap_err();
            assert_eq!(conflict.code, "HOLDING_ALREADY_BOUND");
            let denied = provision_holding(
                &auditor(),
                &store,
                &resource_slot("h005"),
                &input(&resource_slot("h005"), "auditor"),
                "platform-provisioning-key-3",
                NOW,
            )
            .await
            .unwrap_err();
            assert_eq!(denied.code, "FORBIDDEN");

            // A late event failure must roll back the receipt, events and binding.
            let rollback = resource_slot("h005");
            sqlx::raw_sql(
                "CREATE FUNCTION platform_provisioning_fault() RETURNS trigger LANGUAGE plpgsql AS $$ \
                 BEGIN IF NEW.action='activated' THEN RAISE EXCEPTION 'late provisioning failure'; END IF; RETURN NEW; END $$;\
                 CREATE TRIGGER platform_provisioning_fault BEFORE INSERT ON platform_holding_provisioning_events \
                 FOR EACH ROW WHEN (NEW.holding_id='h005') EXECUTE FUNCTION platform_provisioning_fault();",
            )
            .execute(control.raw())
            .await
            .unwrap();
            let failed = provision_holding(
                &admin(),
                &store,
                &rollback,
                &input(&rollback, "rollback"),
                "platform-provisioning-key-4",
                NOW,
            )
            .await
            .unwrap_err();
            assert_eq!(failed.kind, FailureKind::Unavailable);
            assert_eq!(
                count(
                    &control,
                    "SELECT count(*)::bigint FROM platform_holding_provisioning WHERE holding_id='h005'"
                )
                .await,
                0
            );
            assert_eq!(
                count(
                    &control,
                    "SELECT count(*)::bigint FROM holding_bindings WHERE holding_id='h005'"
                )
                .await,
                0
            );
            assert_eq!(
                count(
                    &control,
                    "SELECT count(*)::bigint FROM platform_holding_provisioning_events WHERE holding_id='h005'"
                )
                .await,
                0
            );
            sqlx::raw_sql(
                "DROP TRIGGER platform_provisioning_fault ON platform_holding_provisioning_events;\
                 DROP FUNCTION platform_provisioning_fault();",
            )
            .execute(control.raw())
            .await
            .unwrap();
        })
        .await;
    })
    .await;
}

#[tokio::test]
async fn operations_summary_reads_real_queues_and_recovery_is_reviewed_idempotent_and_rollback_safe()
 {
    fixture::with_database(3, |control| async move {
        fixture::with_database(2, move |holding| async move {
            migrate(&control, MigrationTrack::Control).await;
            migrate(&holding, MigrationTrack::HoldingWrite).await;
            seed_binding(&control, "h001", "acme").await;
            let store = owner(
                &control,
                vec![PlatformHoldingDatabase {
                    identity: holding_identity("h001", "acme"),
                    pool: holding.clone(),
                }],
            )
            .await;
            sqlx::query("INSERT INTO runtime_health(service,heartbeat_at,details_json) VALUES('jobs',$1,'{}')")
                .bind(NOW - 500)
                .execute(control.raw())
                .await
                .unwrap();
            sqlx::query("INSERT INTO runtime_health(service,heartbeat_at,details_json) VALUES('platform-test',$1,'{\"maintenance\":false}')")
                .bind(NOW)
                .execute(control.raw())
                .await
                .unwrap();
            seed_event(&holding, "platform-test-pending", NOW - 20_000).await;
            seed_event(&holding, "platform-test-dead", NOW - 10_000).await;
            seed_event(&holding, "platform-test-claimed", NOW - 5_000).await;
            sqlx::query("UPDATE outbox_events SET dead_letter_at=$1,attempts=6,last_error='PROJECTOR_TIMEOUT' WHERE id='platform-test-dead'")
                .bind(NOW - 1_000)
                .execute(holding.raw())
                .await
                .unwrap();
            sqlx::query("UPDATE outbox_events SET claim_until=$1 WHERE id='platform-test-claimed'")
                .bind(NOW + 60_000)
                .execute(holding.raw())
                .await
                .unwrap();
            let summary = platform_operations_summary(&admin(), &store, &[holding_identity("h001", "acme")], NOW)
                .await
                .unwrap();
            assert_eq!(summary.actor_role, "platform_admin");
            assert!(summary.control_health.iter().any(|health| health.service == "jobs" && health.heartbeat_at == NOW - 500));
            assert_eq!(summary.holdings[0].status, "attention");
            assert_eq!(summary.holdings[0].pending_events, 1);
            assert_eq!(summary.holdings[0].dead_letter_events, 1);
            assert_eq!(summary.holdings[0].claimed_events, 1);
            assert_eq!(summary.holdings[0].oldest_pending_at, Some(NOW - 20_000));
            assert_eq!(summary.holdings[0].dead_letters.len(), 1);
            assert_eq!(summary.holdings[0].migrations.error, None);
            assert_eq!(summary.totals.pending_events, 1);
            assert_eq!(summary.totals.dead_letter_events, 1);
            assert_eq!(summary.totals.claimed_events, 1);
            let unavailable = platform_operations_summary(
                &auditor(),
                &store,
                &[holding_identity("h001", "acme"), holding_identity("missing", "missing")],
                NOW,
            )
            .await
            .unwrap();
            assert_eq!(unavailable.holdings[1].status, "unavailable");
            assert_eq!(unavailable.holdings[1].job_heartbeat_at, Some(NOW - 500));
            let denied = platform_operations_summary(
                &PlatformActor {
                    role: "legal_editor".into(),
                    ..admin()
                },
                &store,
                &[holding_identity("h001", "acme")],
                NOW,
            )
            .await
            .unwrap_err();
            assert_eq!(denied.code, "PLATFORM_OPERATIONS_FORBIDDEN");

            let recovery = RecoverDeadLetterInput {
                event_id: "platform-test-dead".into(),
                expected_attempts: 6,
                expected_last_error: Some("PROJECTOR_TIMEOUT".into()),
                reason: "Provider is healthy and the projection is safe to replay.".into(),
            };
            let first = recover_dead_letter_job(
                &admin(),
                &store,
                &holding_identity("h001", "acme"),
                &recovery,
                "platform-recovery-key-1",
                NOW,
            )
            .await
            .unwrap();
            assert_eq!(first.status, "queued");
            assert_eq!(first.previous_attempts, 6);
            let cleared = sqlx::query("SELECT attempts,last_error,dead_letter_at,next_attempt_at FROM outbox_events WHERE id='platform-test-dead'")
                .fetch_one(holding.raw())
                .await
                .unwrap();
            assert_eq!(cleared.get::<i64, _>("attempts"), 0);
            assert_eq!(cleared.get::<Option<String>, _>("last_error"), None);
            assert_eq!(cleared.get::<Option<i64>, _>("dead_letter_at"), None);
            assert_eq!(cleared.get::<i64, _>("next_attempt_at"), NOW);
            let audit = sqlx::query("SELECT action,purpose FROM audit_events WHERE command_id='platform-recovery-key-1'")
                .fetch_one(holding.raw())
                .await
                .unwrap();
            assert_eq!(audit.get::<String, _>("action"), "platform.job.retry");
            assert_eq!(
                audit.get::<String, _>("purpose"),
                "Provider is healthy and the projection is safe to replay."
            );
            let replay = recover_dead_letter_job(
                &admin(),
                &store,
                &holding_identity("h001", "acme"),
                &recovery,
                "platform-recovery-key-1",
                NOW + 1,
            )
            .await
            .unwrap();
            assert_eq!(replay, first);
            assert_eq!(
                count(
                    &holding,
                    "SELECT count(*)::bigint FROM command_receipts WHERE command_id='platform-recovery-key-1'"
                )
                .await,
                1
            );
            assert_eq!(
                count(
                    &holding,
                    "SELECT count(*)::bigint FROM audit_events WHERE command_id='platform-recovery-key-1'"
                )
                .await,
                1
            );
            sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at,next_attempt_at,attempts,last_error,dead_letter_at) VALUES('platform-test-native-dead','command-native','field.order.created','field_order','order-native',1,'{}',$1,$1,6,'NATIVE_TIMEOUT',$2)").bind(NOW-500).bind(NOW-500).execute(holding.raw()).await.unwrap();
            let native_recovery=recover_dead_letter_job(&admin(),&store,&holding_identity("h001","acme"),&RecoverDeadLetterInput{event_id:"platform-test-native-dead".into(),expected_attempts:6,expected_last_error:Some("NATIVE_TIMEOUT".into()),reason:"Native projector is healthy; replay the owned event.".into()},"platform-recovery-key-native",NOW).await.unwrap();
            assert_eq!(native_recovery.status,"queued");
            let native_cleared=sqlx::query("SELECT attempts,last_error,dead_letter_at,next_attempt_at FROM outbox_events WHERE id='platform-test-native-dead'").fetch_one(holding.raw()).await.unwrap();
            assert_eq!(native_cleared.get::<i64,_>("attempts"),0);assert_eq!(native_cleared.get::<Option<String>,_>("last_error"),None);assert_eq!(native_cleared.get::<Option<i64>,_>("dead_letter_at"),None);assert_eq!(native_cleared.get::<i64,_>("next_attempt_at"),NOW);
            let stale = recover_dead_letter_job(
                &admin(),
                &store,
                &holding_identity("h001", "acme"),
                &RecoverDeadLetterInput {
                    event_id: "platform-test-dead".into(),
                    expected_attempts: 5,
                    expected_last_error: None,
                    reason: "Review the failed provider response before retrying.".into(),
                },
                "platform-recovery-key-2",
                NOW + 2,
            )
            .await
            .unwrap_err();
            assert_eq!(stale.code, "VERSION_CONFLICT");
            let denied = recover_dead_letter_job(
                &auditor(),
                &store,
                &holding_identity("h001", "acme"),
                &recovery,
                "platform-recovery-key-3",
                NOW,
            )
            .await
            .unwrap_err();
            assert_eq!(denied.code, "FORBIDDEN");

            seed_event(&holding, "platform-test-rollback", NOW - 1_000).await;
            sqlx::query("UPDATE outbox_events SET dead_letter_at=$1,attempts=2,last_error='OLD' WHERE id='platform-test-rollback'")
                .bind(NOW - 500)
                .execute(holding.raw())
                .await
                .unwrap();
            sqlx::raw_sql(
                "CREATE FUNCTION platform_recovery_fault() RETURNS trigger LANGUAGE plpgsql AS $$ \
                 BEGIN RAISE EXCEPTION 'late recovery journal failure'; END $$;\
                 CREATE CONSTRAINT TRIGGER platform_recovery_fault AFTER INSERT ON audit_events \
                 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.command_id='platform-recovery-key-4') \
                 EXECUTE FUNCTION platform_recovery_fault();",
            )
            .execute(holding.raw())
            .await
            .unwrap();
            let failed = recover_dead_letter_job(
                &admin(),
                &store,
                &holding_identity("h001", "acme"),
                &RecoverDeadLetterInput {
                    event_id: "platform-test-rollback".into(),
                    expected_attempts: 2,
                    expected_last_error: Some("OLD".into()),
                    reason: "Review the failed provider response before retrying.".into(),
                },
                "platform-recovery-key-4",
                NOW,
            )
            .await
            .unwrap_err();
            assert_eq!(failed.kind, FailureKind::Unavailable);
            let unchanged = sqlx::query("SELECT attempts,last_error,dead_letter_at FROM outbox_events WHERE id='platform-test-rollback'")
                .fetch_one(holding.raw())
                .await
                .unwrap();
            assert_eq!(unchanged.get::<i64, _>("attempts"), 2);
            assert_eq!(unchanged.get::<Option<String>, _>("last_error"), Some("OLD".into()));
            assert_eq!(unchanged.get::<Option<i64>, _>("dead_letter_at"), Some(NOW - 500));
            assert_eq!(
                count(
                    &holding,
                    "SELECT count(*)::bigint FROM command_receipts WHERE command_id='platform-recovery-key-4'"
                )
                .await,
                0
            );
            assert_eq!(
                count(
                    &holding,
                    "SELECT count(*)::bigint FROM audit_events WHERE command_id='platform-recovery-key-4'"
                )
                .await,
                0
            );
            sqlx::raw_sql(
                "DROP TRIGGER platform_recovery_fault ON audit_events; DROP FUNCTION platform_recovery_fault();",
            )
            .execute(holding.raw())
            .await
            .unwrap();
        })
        .await;
    })
    .await;
}

#[tokio::test]
async fn holding_migrations_run_once_with_control_receipt_and_holdings_isolation() {
    fixture::with_database(3, |control| async move {
        fixture::with_database(2, move |h001| async move {
            fixture::with_database(2, move |h002| async move {
                migrate(&control, MigrationTrack::Control).await;
                let store = owner(
                    &control,
                    vec![
                        PlatformHoldingDatabase {
                            identity: holding_identity("h001", "acme"),
                            pool: h001.clone(),
                        },
                        PlatformHoldingDatabase {
                            identity: holding_identity("h002", "northstar"),
                            pool: h002.clone(),
                        },
                    ],
                )
                .await;
                let first = run_holding_migration(
                    &admin(),
                    &store,
                    &holding_identity("h001", "acme"),
                    &migration_input("h001"),
                    "platform-migration-key-1",
                    NOW,
                )
                .await
                .unwrap();
                assert_eq!(first.holding_id, "h001");
                assert_eq!(first.track, MIGRATION_TRACK);
                assert_eq!(first.status, MigrationRunStatus::Succeeded);
                assert_eq!(first.applied_before, 0);
                assert!(first.applied_after > 0);
                assert!(first.pending.is_empty());
                assert_eq!(first.finished_at, NOW);
                assert!(first.run_id.starts_with("platform-migration-"));

                let receipt = sqlx::query(
                    "SELECT status,holding_id,requested_by,assignment_id,reason,applied_before,\
                     applied_after,result_json,error,version,finished_at \
                     FROM platform_migration_runs WHERE idempotency_key='platform-migration-key-1'",
                )
                .fetch_one(control.raw())
                .await
                .unwrap();
                assert_eq!(receipt.get::<String, _>("status"), "succeeded");
                assert_eq!(receipt.get::<String, _>("holding_id"), "h001");
                assert_eq!(
                    receipt.get::<String, _>("requested_by"),
                    "platform-operator"
                );
                assert_eq!(
                    receipt.get::<String, _>("assignment_id"),
                    "platform-assignment"
                );
                assert_eq!(
                    receipt.get::<String, _>("reason"),
                    "Apply the reviewed release schema before enabling the next workflow."
                );
                assert_eq!(receipt.get::<i64, _>("applied_before"), 0);
                assert_eq!(receipt.get::<i64, _>("applied_after"), first.applied_after);
                assert_eq!(receipt.get::<Option<String>, _>("error"), None);
                assert_eq!(receipt.get::<i64, _>("version"), 2);
                assert_eq!(receipt.get::<i64, _>("finished_at"), NOW);
                let retained: RunHoldingMigrationResult =
                    serde_json::from_str(&receipt.get::<String, _>("result_json")).unwrap();
                assert_eq!(retained, first);

                let h001_applied =
                    count(&h001, "SELECT count(*)::bigint FROM erpx_migrations").await;
                assert_eq!(h001_applied, first.applied_after);
                let h002_files = load_migrations(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../migrations/postgres"),
                    MigrationTrack::HoldingWrite,
                )
                .await
                .unwrap();
                let h002_status =
                    inspect_migrations(&h002, MigrationTrack::HoldingWrite, &h002_files)
                        .await
                        .unwrap();
                assert_eq!(h002_status.applied, 0);
                assert_eq!(h002_status.pending.len(), h002_files.len());
                let summary = platform_operations_summary(
                    &admin(),
                    &store,
                    &[holding_identity("h001", "acme")],
                    NOW,
                )
                .await
                .unwrap();
                assert_eq!(summary.holdings[0].migrations.applied, first.applied_after);
                assert!(summary.holdings[0].migrations.pending.is_empty());
                assert_eq!(summary.holdings[0].migrations.error, None);

                let replay = run_holding_migration(
                    &admin(),
                    &store,
                    &holding_identity("h001", "acme"),
                    &migration_input("h001"),
                    "platform-migration-key-1",
                    NOW + 1,
                )
                .await
                .unwrap();
                assert_eq!(replay, first);
                assert_eq!(
                    count(
                        &control,
                        "SELECT count(*)::bigint FROM platform_migration_runs \
                         WHERE idempotency_key='platform-migration-key-1'"
                    )
                    .await,
                    1
                );
                assert_eq!(
                    count(&h001, "SELECT count(*)::bigint FROM erpx_migrations").await,
                    h001_applied
                );
                assert_eq!(
                    count(
                        &control,
                        "SELECT version FROM platform_migration_runs \
                         WHERE idempotency_key='platform-migration-key-1'"
                    )
                    .await,
                    2
                );

                let conflict = run_holding_migration(
                    &admin(),
                    &store,
                    &holding_identity("h001", "acme"),
                    &RunHoldingMigrationInput {
                        reason: "A different request using the same command key.".into(),
                        ..migration_input("h001")
                    },
                    "platform-migration-key-1",
                    NOW + 2,
                )
                .await
                .unwrap_err();
                assert_eq!(conflict.code, "IDEMPOTENCY_CONFLICT");
                let denied = run_holding_migration(
                    &auditor(),
                    &store,
                    &holding_identity("h001", "acme"),
                    &migration_input("h001"),
                    "platform-migration-key-2",
                    NOW,
                )
                .await
                .unwrap_err();
                assert_eq!(denied.code, "FORBIDDEN");
                let invalid = run_holding_migration(
                    &admin(),
                    &store,
                    &holding_identity("h001", "acme"),
                    &RunHoldingMigrationInput {
                        track: "holding-read".into(),
                        ..migration_input("h001")
                    },
                    "platform-migration-key-3",
                    NOW,
                )
                .await
                .unwrap_err();
                assert_eq!(invalid.code, "VALIDATION_FAILED");
                let missing = run_holding_migration(
                    &admin(),
                    &store,
                    &holding_identity("missing", "missing"),
                    &migration_input("missing"),
                    "platform-migration-key-4",
                    NOW,
                )
                .await
                .unwrap_err();
                assert_eq!(missing.code, "NOT_FOUND");
            })
            .await;
        })
        .await;
    })
    .await;
}

#[tokio::test]
async fn a_failed_holding_migration_is_retained_as_failed_without_a_success_result() {
    fixture::with_database(3, |control| async move {
        fixture::with_database(2, move |holding| async move {
            migrate(&control, MigrationTrack::Control).await;
            let root =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
            let mut files = load_migrations(root, MigrationTrack::HoldingWrite)
                .await
                .unwrap();
            files.push(
                erpx_server_runtime::public::postgres::MigrationFile::from_sql(
                    "9999_platform_migration_fault.sql",
                    "SELECT * FROM erpx_missing_platform_migration_table",
                )
                .unwrap(),
            );
            let store = PgPlatformAdministration::new(
                control.clone(),
                vec![PlatformHoldingDatabase {
                    identity: holding_identity("h001", "acme"),
                    pool: holding.clone(),
                }],
                files,
            )
            .unwrap();
            let failed = run_holding_migration(
                &admin(),
                &store,
                &holding_identity("h001", "acme"),
                &migration_input("h001"),
                "platform-migration-failed-1",
                NOW,
            )
            .await
            .unwrap_err();
            assert_eq!(failed.kind, FailureKind::Unavailable);
            assert_eq!(failed.code, "MIGRATION_FAILED");
            let receipt = sqlx::query(
                "SELECT status,result_json,error,applied_after,version,finished_at \
                 FROM platform_migration_runs WHERE idempotency_key='platform-migration-failed-1'",
            )
            .fetch_one(control.raw())
            .await
            .unwrap();
            assert_eq!(receipt.get::<String, _>("status"), "failed");
            assert_eq!(receipt.get::<Option<String>, _>("result_json"), None);
            assert!(
                receipt
                    .get::<Option<String>, _>("error")
                    .is_some_and(|error| !error.is_empty() && error.len() <= MIGRATION_ERROR_MAX)
            );
            assert!(receipt.get::<i64, _>("applied_after") > 0);
            assert_eq!(receipt.get::<i64, _>("version"), 2);
            assert_eq!(receipt.get::<i64, _>("finished_at"), NOW);
            let replay = run_holding_migration(
                &admin(),
                &store,
                &holding_identity("h001", "acme"),
                &migration_input("h001"),
                "platform-migration-failed-1",
                NOW + 1,
            )
            .await
            .unwrap_err();
            assert_eq!(replay.code, "MIGRATION_FAILED");
            assert_eq!(
                count(
                    &control,
                    "SELECT count(*)::bigint FROM platform_migration_runs \
                     WHERE idempotency_key='platform-migration-failed-1'"
                )
                .await,
                1
            );
        })
        .await;
    })
    .await;
}

#[tokio::test]
async fn a_rolled_back_migration_finalize_never_claims_success() {
    fixture::with_database(3, |control| async move {
        fixture::with_database(2, move |holding| async move {
            migrate(&control, MigrationTrack::Control).await;
            let store = owner(
                &control,
                vec![PlatformHoldingDatabase {
                    identity: holding_identity("h001", "acme"),
                    pool: holding.clone(),
                }],
            )
            .await;
            sqlx::raw_sql(
                "CREATE FUNCTION platform_migration_finalize_fault() RETURNS trigger LANGUAGE plpgsql AS $$ \
                 BEGIN IF NEW.status='succeeded' THEN RAISE EXCEPTION 'late migration receipt failure'; END IF; RETURN NEW; END $$;\
                 CREATE TRIGGER platform_migration_finalize_fault BEFORE UPDATE ON platform_migration_runs \
                 FOR EACH ROW EXECUTE FUNCTION platform_migration_finalize_fault();",
            )
            .execute(control.raw())
            .await
            .unwrap();
            let failed = run_holding_migration(
                &admin(),
                &store,
                &holding_identity("h001", "acme"),
                &migration_input("h001"),
                "platform-migration-rollback-1",
                NOW,
            )
            .await
            .unwrap_err();
            assert_eq!(failed.kind, FailureKind::Unavailable);
            let receipt = sqlx::query(
                "SELECT status,result_json,error,version,finished_at \
                 FROM platform_migration_runs WHERE idempotency_key='platform-migration-rollback-1'",
            )
            .fetch_one(control.raw())
            .await
            .unwrap();
            assert_eq!(receipt.get::<String, _>("status"), "running");
            assert_eq!(receipt.get::<Option<String>, _>("result_json"), None);
            assert_eq!(receipt.get::<Option<String>, _>("error"), None);
            assert_eq!(receipt.get::<i64, _>("version"), 1);
            assert_eq!(receipt.get::<Option<i64>, _>("finished_at"), None);
            let replay = run_holding_migration(
                &admin(),
                &store,
                &holding_identity("h001", "acme"),
                &migration_input("h001"),
                "platform-migration-rollback-1",
                NOW + 1,
            )
            .await
            .unwrap_err();
            assert_eq!(replay.code, "MIGRATION_IN_PROGRESS");
            sqlx::raw_sql(
                "DROP TRIGGER platform_migration_finalize_fault ON platform_migration_runs;\
                 DROP FUNCTION platform_migration_finalize_fault();",
            )
            .execute(control.raw())
            .await
            .unwrap();
        })
        .await;
    })
    .await;
}

struct HttpBackend {
    store: Arc<PgPlatformAdministration>,
    actor: PlatformActor,
    holding: PlatformHoldingIdentity,
    slots: Vec<PlatformResourceSlot>,
    now: i64,
}

fn rendered(backend: &HttpBackend) -> serde_json::Value {
    serde_json::json!({
        "userId": backend.actor.user_id,
        "holdingId": "erpx",
        "companyId": "erpx",
        "branchId": null,
        "departmentId": null,
        "assignmentId": backend.actor.assignment_id,
        "assignmentVersion": 1,
        "revision": 1,
        "role": backend.actor.role,
        "modules": [],
        "permissions": [],
        "expiresAt": 9_007_199_254_740_991i64,
    })
}

impl PlatformHttpBackend for HttpBackend {
    fn read<'a>(
        &'a self,
        _: PlatformHttpReadRequest<'a>,
    ) -> PlatformHttpFuture<'a, PlatformHttpResponse> {
        Box::pin(async move {
            let snapshot = platform_operations_snapshot(
                &self.actor,
                self.store.as_ref(),
                std::slice::from_ref(&self.holding),
                &self.slots,
                self.now,
            )
            .await?;
            Ok(PlatformHttpResponse {
                data: serde_json::to_value(snapshot).map_err(|_| {
                    BusinessError::new(
                        FailureKind::Unavailable,
                        "PLATFORM_REPLY_INVALID",
                        "The Platform reply could not be rendered.",
                    )
                })?,
                context: rendered(self),
            })
        })
    }

    fn command<'a>(
        &'a self,
        request: PlatformHttpCommandRequest<'a>,
    ) -> PlatformHttpFuture<'a, PlatformHttpResponse> {
        Box::pin(async move {
            let data = match request.command {
                PlatformHttpCommand::Provision(input) => {
                    let slot = self
                        .slots
                        .iter()
                        .find(|slot| slot.id == input.holding_id)
                        .ok_or_else(|| {
                            BusinessError::new(
                                FailureKind::NotFound,
                                "RESOURCE_SLOT_NOT_FOUND",
                                "This Holding resource slot is not configured.",
                            )
                        })?;
                    serde_json::to_value(
                        provision_holding(
                            &self.actor,
                            self.store.as_ref(),
                            slot,
                            &input,
                            request.key.as_str(),
                            self.now,
                        )
                        .await?,
                    )
                    .map_err(|_| {
                        BusinessError::new(
                            FailureKind::Unavailable,
                            "PLATFORM_REPLY_INVALID",
                            "The Platform reply could not be rendered.",
                        )
                    })?
                }
                PlatformHttpCommand::Recover { holding_id, input } => {
                    if holding_id != self.holding.id {
                        return Err(BusinessError::new(
                            FailureKind::NotFound,
                            "NOT_FOUND",
                            "Holding not found.",
                        ));
                    }
                    serde_json::to_value(
                        recover_dead_letter_job(
                            &self.actor,
                            self.store.as_ref(),
                            &self.holding,
                            &input,
                            request.key.as_str(),
                            self.now,
                        )
                        .await?,
                    )
                    .map_err(|_| {
                        BusinessError::new(
                            FailureKind::Unavailable,
                            "PLATFORM_REPLY_INVALID",
                            "The Platform reply could not be rendered.",
                        )
                    })?
                }
                PlatformHttpCommand::Migrate(input) => serde_json::to_value(
                    run_holding_migration(
                        &self.actor,
                        self.store.as_ref(),
                        &self.holding,
                        &input,
                        request.key.as_str(),
                        self.now,
                    )
                    .await?,
                )
                .map_err(|_| {
                    BusinessError::new(
                        FailureKind::Unavailable,
                        "PLATFORM_REPLY_INVALID",
                        "The Platform reply could not be rendered.",
                    )
                })?,
            };
            Ok(PlatformHttpResponse {
                data,
                context: rendered(self),
            })
        })
    }
}

async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let text = String::from_utf8_lossy(&bytes);
    let status = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .unwrap();
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
    (
        status,
        serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
async fn actual_router_reads_and_provisions_over_postgres_with_target_and_key_negatives() {
    fixture::with_database(3, |control| async move {
        fixture::with_database(2, move |holding| async move {
            migrate(&control, MigrationTrack::Control).await;
            migrate(&holding, MigrationTrack::HoldingWrite).await;
            seed_binding(&control, "h001", "acme").await;
            let store = Arc::new(
                owner(
                    &control,
                    vec![PlatformHoldingDatabase {
                        identity: holding_identity("h001", "acme"),
                        pool: holding.clone(),
                    }],
                )
                .await,
            );
            let slots = vec![resource_slot("h004")];
            let backend = Arc::new(HttpBackend {
                store,
                actor: admin(),
                holding: holding_identity("h001", "acme"),
                slots: slots.clone(),
                now: NOW,
            });
            let router = platform_router(vec![PlatformHttpTarget {
                host: "erpx-internal.example.test".into(),
                origin: "https://erpx-internal.example.test".into(),
                backend,
            }])
            .unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
            });
            let (status, body) = http(
                addr,
                "GET /api/v1/platform/operations HTTP/1.1\r\nHost: erpx-internal.example.test\r\nConnection: close\r\n\r\n".into(),
            )
            .await;
            assert_eq!(status, 200);
            assert_eq!(body["data"]["provisioning"]["rows"], serde_json::json!([]));
            assert_eq!(body["data"]["provisioning"]["availableSlots"][0]["id"], "h004");
            assert_eq!(body["context"]["holdingId"], "erpx");

            let provision = serde_json::json!({
                "holdingId": "h004",
                "slug": "northstar",
                "displayName": "Northstar",
                "writeBinding": "H004_DB",
                "readBinding": "H004_READ_DB",
                "filesBinding": "H004_FILES",
                "reason": "Reviewed onboarding and verified the isolated resource health.",
            })
            .to_string();
            let (status, body) = http(
                addr,
                format!(
                    "POST /api/v1/platform/operations/provision HTTP/1.1\r\nHost: erpx-internal.example.test\r\nOrigin: https://erpx-internal.example.test\r\nContent-Type: application/json\r\nIdempotency-Key: platform-http-provision-1\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{provision}",
                    provision.len()
                ),
            )
            .await;
            assert_eq!(status, 200);
            assert_eq!(body["data"]["slug"], "northstar");
            assert_eq!(body["data"]["status"], "active");
            let (status, body) = http(
                addr,
                "GET /api/v1/platform/operations HTTP/1.1\r\nHost: erpx-internal.example.test\r\nConnection: close\r\n\r\n".into(),
            )
            .await;
            assert_eq!(status, 200);
            assert_eq!(body["data"]["provisioning"]["rows"][0]["slug"], "northstar");
            assert_eq!(
                body["data"]["provisioning"]["availableSlots"],
                serde_json::json!([])
            );
            let migration = serde_json::json!({
                "holdingId": "h001",
                "track": "holding-write",
                "reason": "Apply the reviewed release schema before enabling the next workflow.",
            })
            .to_string();
            let (status, body) = http(
                addr,
                format!(
                    "POST /api/v1/platform/operations/migrate HTTP/1.1\r\nHost: erpx-internal.example.test\r\nOrigin: https://erpx-internal.example.test\r\nContent-Type: application/json\r\nIdempotency-Key: platform-http-migration-1\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{migration}",
                    migration.len()
                ),
            )
            .await;
            assert_eq!(status, 200);
            assert_eq!(body["data"]["holdingId"], "h001");
            assert_eq!(body["data"]["track"], "holding-write");
            assert_eq!(body["data"]["status"], "succeeded");
            assert!(body["data"]["appliedBefore"].as_i64().unwrap() > 0);
            assert_eq!(
                body["data"]["appliedBefore"],
                body["data"]["appliedAfter"]
            );
            assert_eq!(body["data"]["pending"], serde_json::json!([]));
            let (status, body) = http(
                addr,
                format!(
                    "POST /api/v1/platform/operations/migrate HTTP/1.1\r\nHost: erpx-internal.example.test\r\nOrigin: https://evil.example.test\r\nContent-Type: application/json\r\nIdempotency-Key: platform-http-migration-2\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{migration}",
                    migration.len()
                ),
            )
            .await;
            assert_eq!(status, 403);
            assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");
            let (status, body) = http(
                addr,
                format!(
                    "POST /api/v1/platform/operations/provision HTTP/1.1\r\nHost: erpx-internal.example.test\r\nOrigin: https://evil.example.test\r\nContent-Type: application/json\r\nIdempotency-Key: platform-http-provision-2\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{provision}",
                    provision.len()
                ),
            )
            .await;
            assert_eq!(status, 403);
            assert_eq!(body["error"]["code"], "ORIGIN_FORBIDDEN");
            let (status, body) = http(
                addr,
                format!(
                    "POST /api/v1/platform/operations/provision HTTP/1.1\r\nHost: erpx-internal.example.test\r\nOrigin: https://erpx-internal.example.test\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{provision}",
                    provision.len()
                ),
            )
            .await;
            assert_eq!(status, 422);
            assert_eq!(body["error"]["code"], "COMMAND_KEY_REQUIRED");
            let (status, _) = http(
                addr,
                "GET /api/v1/platform/operations HTTP/1.1\r\nHost: missing.example.test\r\nConnection: close\r\n\r\n".into(),
            )
            .await;
            assert_eq!(status, 404);
            server.abort();
        })
        .await;
    })
    .await;
}

// Synthetic Holding fixture tests the real Platform Administration release
// reader with an explicit test artifact. It is not a production Finance gate,
// release workflow acceptance, or proof that the compiled artifact is built.
const SYNTHETIC_RELEASE_ARTIFACT:&str="test-only-accepted-release";
async fn seed_synthetic_production_release(pool:&DatabasePool) {
    use erpx_server_platform_administration::public::production_release::current_production_release_catalogue;
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id,status) VALUES('release-company','company','release-company','Synthetic release company','release-company','active'); INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','release_test_reviewer','Synthetic release reviewer',false,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','release_test_reviewer',1,'[\"organization.manage\",\"pms.approve\",\"commerce.approve\",\"records.write\",\"pos.approve\",\"people.manage\",\"payroll.approve\",\"finance.approve\"]','[]','[\"holding\"]','synthetic',0); INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy) VALUES('release-company','release-requester','local:release-requester','requester-person','active','local'),('release-company','setup-reviewer','local:setup-reviewer','setup-person','active','local'); INSERT INTO company_business_acceptance(company_id,status,version,requested_by,reviewed_by,requested_at,reviewed_at,reason,review_reason,updated_at) VALUES('release-company','accepted',2,'release-requester','setup-reviewer',1,2,'Synthetic setup request','Synthetic independent review',2)").execute(pool.raw()).await.unwrap();
    let catalogue=current_production_release_catalogue().unwrap();assert_eq!(catalogue.criteria().len(),15);
    for (i,_) in catalogue.criteria().iter().enumerate() {
        let user=format!("release-reviewer-{i}");let assignment=format!("release-assignment-{i}");
        sqlx::query("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy) VALUES('release-company',$1,$2,$3,'active','local')").bind(&user).bind(format!("local:{user}")).bind(format!("person-{i}")).execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES($1,$2,'release_test_reviewer','[]','release-company',0,9007199254740991,1,'holding')").bind(&assignment).bind(&user).execute(pool.raw()).await.unwrap();
        // Gate rows are inserted after the acceptance below.
    }
    sqlx::query("INSERT INTO company_release_acceptances(id,company_id,scope_version,software_version,configuration_revision,setup_version,evidence_sha256,evidence_json,requested_by,reason,created_at,updated_at) SELECT 'synthetic-release','release-company',$1,$2,revision,2,$3,'{}','release-requester','Synthetic fixture only',1,1 FROM company_release_revisions WHERE company_id='release-company'")
        .bind(catalogue.scope_version()).bind(SYNTHETIC_RELEASE_ARTIFACT).bind("a".repeat(64)).execute(pool.raw()).await.unwrap();
    for (i,criterion) in catalogue.criteria().iter().enumerate() {
        sqlx::query("INSERT INTO company_release_gate_reviews(company_id,release_id,criterion_id,state,reviewer_id,assignment_id,permission,reason,reviewed_at) VALUES('release-company','synthetic-release',$1,'passed',$2,$3,$4,'Synthetic independent fixture review',1)")
            .bind(&criterion.id).bind(format!("release-reviewer-{i}")).bind(format!("release-assignment-{i}")).bind(&criterion.permission).execute(pool.raw()).await.unwrap();
    }
}
async fn synthetic_release_accepted(c:&mut sqlx::PgConnection,artifact:&str)->bool {
    use erpx_server_platform_administration::public::production_release::{current_production_release_catalogue,PgProductionReleaseReader,ProductionReleaseReadPort};
    use erpx_server_shared::public::{CompanyId,TimestampMillis};
    let catalogue=current_production_release_catalogue().unwrap();
    PgProductionReleaseReader::new(c,artifact,&catalogue).unwrap().inspect(&CompanyId::new("release-company").unwrap(),TimestampMillis::new(0)).await.unwrap().accepted
}

#[tokio::test]
async fn synthetic_release_permissions_first_writer_causes_retry_and_fresh_denial() {
    use erpx_server_platform_administration::public::production_release::lock_production_release_sources;
    use erpx_server_runtime::public::postgres::{PgTransactionError,PgUnitOfWork,TransactionOptions};
    use std::sync::atomic::{AtomicUsize,Ordering};
    fixture::with_database(3,|pool|async move {
        migrate(&pool,MigrationTrack::HoldingWrite).await;seed_synthetic_production_release(&pool).await;
        let mut baseline=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *baseline).await.unwrap();lock_production_release_sources(&mut baseline).await.unwrap();
        assert!(synthetic_release_accepted(&mut baseline,SYNTHETIC_RELEASE_ARTIFACT).await);baseline.rollback().await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::query("UPDATE iam_permissions SET definition=jsonb_set(definition,'{availability}','\"planned\"'::jsonb) WHERE id='finance.approve'").execute(&mut *writer).await.unwrap();
        let owner_pool=pool.clone();let attempts=Arc::new(AtomicUsize::new(0));let owner_attempts=attempts.clone();let (send,mut receive)=tokio::sync::mpsc::unbounded_channel();
        let reader=tokio::spawn(async move {PgUnitOfWork::new(owner_pool).run::<bool,BusinessError,_>(TransactionOptions::default(),move|c| {
            let attempts=owner_attempts.clone();let send=send.clone();Box::pin(async move {
                attempts.fetch_add(1,Ordering::SeqCst);
                if let Err(error)=lock_production_release_sources(c).await {
                    if matches!(&error,PgTransactionError::Persistence(e) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03")) {send.send(()).unwrap();}
                    return Err(error);
                }
                Ok(synthetic_release_accepted(c,SYNTHETIC_RELEASE_ARTIFACT).await)
            })
        }).await});
        tokio::time::timeout(std::time::Duration::from_secs(5),receive.recv()).await.unwrap().unwrap();
        // A permissions-first migration must be able to acquire its later
        // role write after the contended reader rolls its attempt back.
        sqlx::query("UPDATE iam_roles SET name=name WHERE id='release_test_reviewer'").execute(&mut *writer).await.unwrap();
        writer.commit().await.unwrap();
        assert!(!reader.await.unwrap().unwrap());assert!(attempts.load(Ordering::SeqCst)>=2);
    }).await;
}

#[tokio::test]
async fn synthetic_release_reader_first_blocks_permission_withdrawal_until_commit() {
    use erpx_server_platform_administration::public::production_release::lock_production_release_sources;
    fixture::with_database(3,|pool|async move {
        migrate(&pool,MigrationTrack::HoldingWrite).await;seed_synthetic_production_release(&pool).await;
        let mut reader=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *reader).await.unwrap();lock_production_release_sources(&mut reader).await.unwrap();
        assert!(synthetic_release_accepted(&mut reader,SYNTHETIC_RELEASE_ARTIFACT).await);
        let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *reader).await.unwrap();let writer_pool=pool.clone();let (send,receive)=tokio::sync::oneshot::channel();
        let writer=tokio::spawn(async move {let mut tx=writer_pool.raw().begin().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();send.send(pid).unwrap();sqlx::query("UPDATE iam_permissions SET definition=jsonb_set(definition,'{availability}','\"planned\"'::jsonb) WHERE id='finance.approve'").execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();});
        let writer_pid=receive.await.unwrap();tokio::time::timeout(std::time::Duration::from_secs(5),async {loop {
            let blocked:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(reader_pid).bind(writer_pid).fetch_one(pool.raw()).await.unwrap();
            if blocked {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }}).await.expect("reviewer permission withdrawal did not wait for release source");
        assert!(synthetic_release_accepted(&mut reader,SYNTHETIC_RELEASE_ARTIFACT).await);reader.commit().await.unwrap();writer.await.unwrap();
        let mut fresh=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fresh).await.unwrap();lock_production_release_sources(&mut fresh).await.unwrap();
        assert!(!synthetic_release_accepted(&mut fresh,SYNTHETIC_RELEASE_ARTIFACT).await);fresh.rollback().await.unwrap();
    }).await;
}
#[tokio::test]
async fn synthetic_production_release_reader_requires_all_current_independent_gates() {
    use erpx_server_platform_administration::public::production_release::{current_production_release_catalogue,lock_production_release_sources};
    fixture::with_database(3,|pool|async move {
        migrate(&pool,MigrationTrack::HoldingWrite).await;seed_synthetic_production_release(&pool).await;
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_production_release_sources(&mut tx).await.unwrap();
        assert!(synthetic_release_accepted(&mut tx,SYNTHETIC_RELEASE_ARTIFACT).await);
        assert!(!synthetic_release_accepted(&mut tx,"unbuilt").await);
        assert!(!synthetic_release_accepted(&mut tx,"another-artifact").await);
        for gate in current_production_release_catalogue().unwrap().criteria() {
            sqlx::query("UPDATE company_release_gate_reviews SET state='pending' WHERE release_id='synthetic-release' AND criterion_id=$1").bind(&gate.id).execute(&mut *tx).await.unwrap();
            assert!(!synthetic_release_accepted(&mut tx,SYNTHETIC_RELEASE_ARTIFACT).await,"{}",gate.id);
            sqlx::query("UPDATE company_release_gate_reviews SET state='passed' WHERE release_id='synthetic-release' AND criterion_id=$1").bind(&gate.id).execute(&mut *tx).await.unwrap();
        }
        sqlx::query("UPDATE iam_members SET person_id='requester-person' WHERE user_id='release-reviewer-0' AND company_id='release-company'").execute(&mut *tx).await.unwrap();
        assert!(!synthetic_release_accepted(&mut tx,SYNTHETIC_RELEASE_ARTIFACT).await);
        tx.rollback().await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::query("UPDATE company_release_acceptances SET revoked_at=2 WHERE id='synthetic-release'").execute(&mut *writer).await.unwrap();
        let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();let other_pool=pool.clone();
        let reader=tokio::spawn(async move {
            let mut tx=other_pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            lock_production_release_sources(&mut tx).await.unwrap();let accepted=synthetic_release_accepted(&mut tx,SYNTHETIC_RELEASE_ARTIFACT).await;tx.rollback().await.unwrap();accepted
        });
        tokio::time::timeout(std::time::Duration::from_secs(5),async {loop {let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))").bind(writer_pid).fetch_one(pool.raw()).await.unwrap();if blocked {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.expect("release reader must wait for writer before snapshot");
        writer.commit().await.unwrap();assert!(!reader.await.unwrap());
        sqlx::query("UPDATE company_release_acceptances SET revoked_at=NULL WHERE id='synthetic-release'").execute(pool.raw()).await.unwrap();
        let mut reader=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *reader).await.unwrap();lock_production_release_sources(&mut reader).await.unwrap();
        assert!(synthetic_release_accepted(&mut reader,SYNTHETIC_RELEASE_ARTIFACT).await);
        let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *reader).await.unwrap();let writer_pool=pool.clone();
        let writer=tokio::spawn(async move {sqlx::query("UPDATE company_release_acceptances SET revoked_at=3 WHERE id='synthetic-release'").execute(writer_pool.raw()).await.unwrap();});
        tokio::time::timeout(std::time::Duration::from_secs(5),async {loop {let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))").bind(reader_pid).fetch_one(pool.raw()).await.unwrap();if blocked {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.expect("release withdrawal must wait for held source");
        assert!(synthetic_release_accepted(&mut reader,SYNTHETIC_RELEASE_ARTIFACT).await);reader.commit().await.unwrap();writer.await.unwrap();
        let mut fresh=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fresh).await.unwrap();lock_production_release_sources(&mut fresh).await.unwrap();assert!(!synthetic_release_accepted(&mut fresh,SYNTHETIC_RELEASE_ARTIFACT).await);fresh.rollback().await.unwrap();
    }).await;
}
