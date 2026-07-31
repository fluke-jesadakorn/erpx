use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chrono::{Duration, Utc};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, G13_EXECUTE_PERMISSION, G13AuthorizationProof,
    G13PersistenceError, SqlxControlPlane, SqlxG13MigrationStore,
};
use erpx_migration::{
    LegacySourceSnapshot, MigrationPhase, ReconciliationReport,
    g13::{
        G13CheckpointRequest, G13CheckpointState, G13EvidencePayload, G13EvidenceRequest,
        G13StartRunRequest, G13TransitionRequest,
    },
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

fn config() -> ControlPlaneDatabaseConfig {
    ControlPlaneDatabaseConfig {
        write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
        read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
    }
}

fn proof(now: chrono::DateTime<Utc>) -> G13AuthorizationProof {
    G13AuthorizationProof {
        actor_ref: "local:g13-test".into(),
        permissions: BTreeSet::from([G13_EXECUTE_PERMISSION.into()]),
        mfa_verified: true,
        break_glass: false,
        expires_at: now + Duration::minutes(5),
        cutover_approval_ref: None,
    }
}

fn snapshot_hash(snapshot: &LegacySourceSnapshot) -> String {
    let mut canonical = snapshot.clone();
    canonical.generated_at = chrono::DateTime::<Utc>::UNIX_EPOCH;
    canonical.source_contract_hash.clear();
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(&canonical).expect("serialize snapshot contract"));
    hex::encode(digest.finalize())
}

#[tokio::test]
#[ignore = "requires two disposable control PostgreSQL databases"]
async fn g13_run_checkpoint_evidence_and_read_projection_are_durable() {
    let pools = ControlPlanePools::connect(&config()).await.expect("pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control write migrations");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migrations");
    let store = SqlxG13MigrationStore::new(Arc::new(SqlxControlPlane::new(pools.clone())));
    let now = Utc::now();
    let run_id = Uuid::new_v4();
    let actor = proof(now);

    let mut source_snapshot = LegacySourceSnapshot {
        extractor_version: "g13-test-extractor".into(),
        generated_at: now,
        source_contract_hash: String::new(),
        missing_tables: vec!["legacy.optional_legacy_table".into()],
        counts: BTreeMap::from([(String::from("customers"), 10_u64)]),
        statuses: BTreeMap::new(),
        totals_minor: BTreeMap::from([(String::from("customers"), 0_i64)]),
        object_bytes: BTreeMap::new(),
        id_mappings: BTreeMap::new(),
    };
    source_snapshot.source_contract_hash = snapshot_hash(&source_snapshot);
    let start_request = G13StartRunRequest {
        run_id,
        cohort: "g13-disposable-cohort".into(),
        source_contract_hash: source_snapshot.source_contract_hash.clone(),
        id_namespace: "g13-disposable".into(),
        actor_ref: actor.actor_ref.clone(),
        correlation_id: Uuid::new_v4(),
        idempotency_key: "g13-start-1".into(),
        now,
    };
    let run = store
        .start_run(&start_request, &actor)
        .await
        .expect("start run");
    assert_eq!(run.run.phase, MigrationPhase::SourceFrozen);
    assert_eq!(run.aggregate_version, 1);
    let replayed_start = store
        .start_run(&start_request, &actor)
        .await
        .expect("idempotent start replay");
    assert_eq!(replayed_start, run);

    let snapshot_id = Uuid::new_v4();
    let snapshot_correlation_id = Uuid::new_v4();
    let source_record = store
        .record_source_snapshot(
            run_id,
            snapshot_id,
            &source_snapshot,
            &actor.actor_ref,
            snapshot_correlation_id,
            "g13-source-snapshot-1",
            &actor,
        )
        .await
        .expect("source snapshot");
    assert_eq!(source_record.snapshot_id, snapshot_id);
    assert_eq!(source_record.total_rows, 10);
    assert_eq!(
        store
            .record_source_snapshot(
                run_id,
                snapshot_id,
                &source_snapshot,
                &actor.actor_ref,
                snapshot_correlation_id,
                "g13-source-snapshot-1",
                &actor,
            )
            .await
            .expect("idempotent source snapshot replay"),
        source_record
    );
    let mut forged_snapshot = source_snapshot.clone();
    forged_snapshot.counts.insert("customers".into(), 11);
    assert!(matches!(
        store
            .record_source_snapshot(
                run_id,
                Uuid::new_v4(),
                &forged_snapshot,
                &actor.actor_ref,
                Uuid::new_v4(),
                "g13-source-snapshot-forged-content",
                &actor,
            )
            .await,
        Err(G13PersistenceError::Conflict)
    ));

    let transition_request = G13TransitionRequest {
        run_id,
        expected_aggregate_version: 1,
        expected_phase: MigrationPhase::SourceFrozen,
        actor_ref: actor.actor_ref.clone(),
        correlation_id: Uuid::new_v4(),
        idempotency_key: "g13-transition-1".into(),
        now: now + Duration::seconds(1),
    };
    let transitioned = store
        .transition(&transition_request, &actor)
        .await
        .expect("transition");
    assert_eq!(transitioned.run.phase, MigrationPhase::IdsMapped);
    assert_eq!(transitioned.aggregate_version, 2);
    assert_eq!(
        store
            .current_run_version(run_id)
            .await
            .expect("run version"),
        2
    );
    assert_eq!(
        store
            .transition(&transition_request, &actor)
            .await
            .expect("idempotent transition replay"),
        transitioned
    );
    let source_fence = run.run.source_contract_hash.clone();
    let mismatched_source_fence =
        "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    let checkpoint_request = G13CheckpointRequest {
        run_id,
        expected_run_version: 2,
        expected_checkpoint_version: None,
        checkpoint_key: "holding-001".into(),
        source_contract_hash: run.run.source_contract_hash.clone(),
        cursor: Some("legacy-cursor-1".into()),
        source_position: 10,
        processed_rows: 10,
        state: G13CheckpointState::Complete,
        last_error: None,
        actor_ref: actor.actor_ref.clone(),
        correlation_id: Uuid::new_v4(),
        idempotency_key: "g13-checkpoint-1".into(),
        now: now + Duration::seconds(2),
    };
    store
        .save_checkpoint(&checkpoint_request, &actor)
        .await
        .expect("checkpoint");
    store
        .save_checkpoint(&checkpoint_request, &actor)
        .await
        .expect("idempotent checkpoint replay");

    let stale_checkpoint = G13CheckpointRequest {
        expected_run_version: 1,
        idempotency_key: "g13-checkpoint-stale-version".into(),
        correlation_id: Uuid::new_v4(),
        ..checkpoint_request.clone()
    };
    assert!(matches!(
        store.save_checkpoint(&stale_checkpoint, &actor).await,
        Err(erpx_infrastructure::G13PersistenceError::Conflict)
    ));

    let shadow_checkpoint = G13CheckpointRequest {
        checkpoint_key: "g13-shadow:holding-001".into(),
        cursor: Some(run.run.source_contract_hash.clone()),
        source_position: 10,
        processed_rows: 10,
        state: G13CheckpointState::Complete,
        idempotency_key: "g13-shadow-checkpoint-1".into(),
        correlation_id: Uuid::new_v4(),
        ..checkpoint_request.clone()
    };
    store
        .save_work_checkpoint(
            "shadow_replay",
            &shadow_checkpoint,
            10,
            &run.run.source_contract_hash,
            "succeeded",
            &actor,
        )
        .await
        .expect("shadow checkpoint");
    let shadow_checkpoint_row = sqlx::query(
        "SELECT work_kind, rows_replayed, source_digest, state
         FROM erpx_control.migration_checkpoints
         WHERE run_id=$1 AND work_kind='shadow_replay' AND scope_key=$2",
    )
    .bind(run_id)
    .bind("g13-shadow:holding-001")
    .fetch_one(&pools.write)
    .await
    .expect("shadow checkpoint row");
    assert_eq!(
        shadow_checkpoint_row
            .try_get::<String, _>("work_kind")
            .expect("work kind"),
        "shadow_replay"
    );
    assert_eq!(
        shadow_checkpoint_row
            .try_get::<i64, _>("rows_replayed")
            .expect("rows replayed"),
        10
    );
    assert_eq!(
        shadow_checkpoint_row
            .try_get::<String, _>("source_digest")
            .expect("source digest"),
        run.run.source_contract_hash
    );
    assert_eq!(
        shadow_checkpoint_row
            .try_get::<String, _>("state")
            .expect("checkpoint state"),
        "succeeded"
    );

    let evidence_request = G13EvidenceRequest {
        run_id,
        expected_run_version: 2,
        evidence_id: Uuid::new_v4(),
        payload: G13EvidencePayload::Reconciliation(ReconciliationReport {
            differences: Vec::new(),
            unexplained: Vec::new(),
        }),
        actor_ref: actor.actor_ref.clone(),
        correlation_id: Uuid::new_v4(),
        idempotency_key: "g13-evidence-1".into(),
        now: now + Duration::seconds(3),
    };
    let evidence_hash = store
        .record_evidence(&evidence_request, &actor)
        .await
        .expect("evidence");
    assert_eq!(evidence_hash.len(), 64);
    assert_eq!(
        store
            .record_evidence(&evidence_request, &actor)
            .await
            .expect("idempotent evidence replay"),
        evidence_hash
    );

    assert_eq!(store.project_read_model().await.expect("projection"), 1);
    let statuses = store.list_read_model(10).await.expect("read model");
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].run_id, run_id);
    assert_eq!(statuses[0].phase, "ids_mapped");
    assert_eq!(statuses[0].reconciliation_state, "matched");
    assert_eq!(
        statuses[0].source_snapshot_hash,
        run.run.source_contract_hash
    );
    assert_eq!(statuses[0].source_missing_table_count, 1);
    assert_eq!(statuses[0].source_total_rows, 10);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_control.migration_reconciliation_read_model
             WHERE run_id=$1",
        )
        .bind(run_id)
        .fetch_one(&pools.read)
        .await
        .expect("reconciliation read projection"),
        1
    );
    assert!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_control.outbox_events
             WHERE aggregate_id=$1 AND event_type IN
                 ('migration.cohort_started','migration.phase_transitioned',
                  'migration.checkpoint_saved','migration.evidence_recorded')",
        )
        .bind(run_id)
        .fetch_one(&pools.write)
        .await
        .expect("G13 control outbox evidence")
            >= 4
    );

    let work_id = Uuid::new_v4();
    let work_hash = "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd";
    assert!(matches!(
        store
            .enqueue_work(
                run_id,
                Uuid::new_v4(),
                "backfill",
                "holding-mismatched-fence",
                mismatched_source_fence,
                work_hash,
                3,
                &actor.actor_ref,
                &actor,
            )
            .await,
        Err(G13PersistenceError::Conflict)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_control.migration_work_items
             WHERE run_id=$1 AND scope_key=$2",
        )
        .bind(run_id)
        .bind("holding-mismatched-fence")
        .fetch_one(&pools.write)
        .await
        .expect("mismatched fence leaves no work row"),
        0
    );
    assert!(
        store
            .enqueue_work(
                run_id,
                work_id,
                "backfill",
                "holding-001",
                &source_fence,
                work_hash,
                3,
                &actor.actor_ref,
                &actor,
            )
            .await
            .expect("enqueue work")
    );
    assert!(
        !store
            .enqueue_work(
                run_id,
                work_id,
                "backfill",
                "holding-001",
                &source_fence,
                work_hash,
                3,
                &actor.actor_ref,
                &actor
            )
            .await
            .expect("idempotent work enqueue")
    );
    assert!(
        store
            .enqueue_work(
                run_id,
                Uuid::new_v4(),
                "backfill",
                "holding-001",
                &source_fence,
                "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
                3,
                &actor.actor_ref,
                &actor,
            )
            .await
            .is_err(),
        "a changed request for one lifecycle scope must not create a second work item"
    );
    let claims = store
        .claim_work(run_id, Uuid::new_v4(), 10, Duration::minutes(5), &actor)
        .await
        .expect("claim work");
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].attempt, 1);
    store
        .complete_work(&claims[0], serde_json::json!({"rows_applied": 10}), &actor)
        .await
        .expect("complete work");
    store
        .complete_work(&claims[0], serde_json::json!({"rows_applied": 10}), &actor)
        .await
        .expect("idempotent completion replay");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_control.outbox_events
             WHERE idempotency_key=$1",
        )
        .bind(format!("migration-work-succeeded:{}", claims[0].work_id))
        .fetch_one(&pools.write)
        .await
        .expect("stable completion event"),
        1
    );
    assert!(
        store
            .claim_work(run_id, Uuid::new_v4(), 10, Duration::minutes(5), &actor)
            .await
            .expect("no completed work")
            .is_empty()
    );

    store
        .transition(
            &G13TransitionRequest {
                run_id,
                expected_aggregate_version: 2,
                expected_phase: MigrationPhase::IdsMapped,
                actor_ref: actor.actor_ref.clone(),
                correlation_id: Uuid::new_v4(),
                idempotency_key: "g13-transition-write-backfilled".into(),
                now: now + Duration::seconds(4),
            },
            &actor,
        )
        .await
        .expect("advance to projectable migration phase");

    let command_work_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"g13-command-work-idempotency-1");
    let command_request_hash = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    assert!(matches!(
        store
            .enqueue_work_command(
                run_id,
                Uuid::new_v4(),
                "project",
                "holding-mismatched-command-fence",
                mismatched_source_fence,
                command_request_hash,
                "g13-command-mismatched-fence",
                3,
                &actor.actor_ref,
                Uuid::new_v4(),
                &actor,
            )
            .await,
        Err(G13PersistenceError::Conflict)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_control.migration_work_items
             WHERE run_id=$1 AND scope_key=$2",
        )
        .bind(run_id)
        .bind("holding-mismatched-command-fence")
        .fetch_one(&pools.write)
        .await
        .expect("mismatched command fence leaves no work row"),
        0
    );
    let command_receipt = store
        .enqueue_work_command(
            run_id,
            command_work_id,
            "project",
            "holding-001",
            &source_fence,
            command_request_hash,
            "g13-command-work-1",
            3,
            &actor.actor_ref,
            Uuid::new_v4(),
            &actor,
        )
        .await
        .expect("authorized command work enqueue");
    assert!(command_receipt.queued);
    let command_replay = store
        .enqueue_work_command(
            run_id,
            command_work_id,
            "project",
            "holding-001",
            &source_fence,
            command_request_hash,
            "g13-command-work-1",
            3,
            &actor.actor_ref,
            Uuid::new_v4(),
            &actor,
        )
        .await
        .expect("idempotent authorized command work enqueue");
    assert!(!command_replay.queued);
    assert_eq!(command_replay.work_id, command_work_id);
    let command_scope_replay = store
        .enqueue_work_command(
            run_id,
            Uuid::new_v4(),
            "project",
            "holding-001",
            &source_fence,
            command_request_hash,
            "g13-command-work-1-new-idempotency-key",
            3,
            &actor.actor_ref,
            Uuid::new_v4(),
            &actor,
        )
        .await
        .expect("scope-idempotent authorized command work enqueue");
    assert!(!command_scope_replay.queued);
    assert_eq!(command_scope_replay.work_id, command_work_id);
    let command_claim = store
        .claim_work(run_id, Uuid::new_v4(), 1, Duration::minutes(5), &actor)
        .await
        .expect("claim authorized command work")
        .pop()
        .expect("authorized command work claim");
    assert_eq!(command_claim.work_id, command_work_id);
    store
        .complete_work(
            &command_claim,
            serde_json::json!({"projected": true}),
            &actor,
        )
        .await
        .expect("complete authorized command work");

    let retry_work_id = Uuid::new_v4();
    assert!(
        store
            .enqueue_work(
                run_id,
                retry_work_id,
                "project",
                "holding-002",
                &source_fence,
                "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
                2,
                &actor.actor_ref,
                &actor
            )
            .await
            .expect("enqueue retry work")
    );
    let retry_claim = store
        .claim_work(run_id, Uuid::new_v4(), 1, Duration::minutes(5), &actor)
        .await
        .expect("claim retry work")
        .pop()
        .expect("retry claim");
    assert!(
        store
            .retry_work(&retry_claim, "temporary source unavailable", &actor)
            .await
            .expect("retry work")
    );
    sqlx::query(
        "UPDATE erpx_control.migration_work_items
         SET available_at=now() WHERE work_id=$1",
    )
    .bind(retry_work_id)
    .execute(&pools.write)
    .await
    .expect("make retry work due for the bounded acceptance");
    let retry_reclaimed = store
        .claim_work(run_id, Uuid::new_v4(), 1, Duration::minutes(5), &actor)
        .await
        .expect("reclaim retry work")
        .pop()
        .expect("reclaimed retry work");
    store
        .complete_work(
            &retry_reclaimed,
            serde_json::json!({"retried": true}),
            &actor,
        )
        .await
        .expect("complete reclaimed retry work");
    store
        .project_read_model()
        .await
        .expect("project work state");
    let status_after_work = store
        .list_read_model(10)
        .await
        .expect("read projected work state");
    assert_eq!(status_after_work[0].work_pending, 0);

    let lease_work_id = Uuid::new_v4();
    assert!(
        store
            .enqueue_work(
                run_id,
                lease_work_id,
                "project",
                "holding-lease-reclaim",
                &source_fence,
                "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
                2,
                &actor.actor_ref,
                &actor,
            )
            .await
            .expect("enqueue lease-reclaim work")
    );
    let crashed_worker = Uuid::new_v4();
    let first_lease = store
        .claim_work(run_id, crashed_worker, 1, Duration::minutes(5), &actor)
        .await
        .expect("claim lease-reclaim work")
        .pop()
        .expect("lease-reclaim claim");
    assert_eq!(first_lease.work_id, lease_work_id);
    sqlx::query(
        "UPDATE erpx_control.migration_work_items
         SET lease_until=now()-interval '1 second'
         WHERE work_id=$1",
    )
    .bind(lease_work_id)
    .execute(&pools.write)
    .await
    .expect("expire the simulated crashed-worker lease");

    let restarted_worker = Uuid::new_v4();
    let reclaimed = store
        .claim_work(run_id, restarted_worker, 1, Duration::minutes(5), &actor)
        .await
        .expect("reclaim expired lease")
        .pop()
        .expect("reclaimed lease claim");
    assert_eq!(reclaimed.work_id, lease_work_id);
    assert_eq!(reclaimed.attempt, 2);
    assert_ne!(reclaimed.worker_id, first_lease.worker_id);
    assert_ne!(reclaimed.claim_id, first_lease.claim_id);
    assert!(
        store
            .complete_work(
                &first_lease,
                serde_json::json!({"stale_worker": true}),
                &actor,
            )
            .await
            .is_err(),
        "a stale crashed-worker claim must not complete after lease reclamation"
    );
    assert!(
        !store
            .retry_work(
                &reclaimed,
                "restarted worker exhausted retry budget",
                &actor,
            )
            .await
            .expect("dead-letter exhausted retry")
    );
    store
        .project_read_model()
        .await
        .expect("project reclaimed dead-letter state");
    let status_after_reclaim = store
        .list_read_model(10)
        .await
        .expect("read reclaimed work state");
    assert_eq!(status_after_reclaim[0].work_pending, 0);
    assert_eq!(status_after_reclaim[0].work_failed, 1);
    pools.write.close().await;
    pools.read.close().await;
}
