//! Disposable PostgreSQL evidence for durable Marketing Journey execution.
//!
//! The ignored test uses one unique holding reference, checks lease ownership
//! and replay behavior, and removes every row it creates.

use chrono::{Duration, Utc};
use erpx_domain::{
    contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext},
    marketing::{AudienceMembership, MarketingChannel, MarketingProfile},
    marketing_crm::{
        AccountReference, ContactReference, MarketingAcquisitionReference, MarketingLeadReference,
        MarketingQualificationReference, MarketingQualifiedSource,
    },
    marketing_journey::{
        Journey, JourneyAction, JourneyChannelPayload, JourneyExecutionCursor, JourneyNode,
        JourneyNodeDefinition, JourneyNodeOutcome, JourneyStatus, JourneyTransitionCommand,
        JourneyTrigger, SegmentChange,
    },
};
use erpx_infrastructure::{
    MarketingJourneyExecutionFailure, PersistenceError, SqlxMarketingDeliveryStore,
    SqlxMarketingJourneyExecutionStore, SqlxMarketingJourneyStore, SqlxMarketingStore,
};
use erpx_migration::{migrate_holding_read, migrate_holding_write};
use sqlx::PgPool;
use uuid::Uuid;

fn context(scope: AuthorityScope, key: impl Into<String>, expected: Option<u64>) -> CommandContext {
    let mut context = CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "marketing-journey-execution-test".into(),
            kind: ActorKind::HoldingUser,
            session_id: Some(Uuid::new_v4()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "marketing.journey.execution.persistence.test",
    )
    .expect("valid execution test context");
    context.expected_aggregate_version = expected;
    context
}

fn draft(scope: AuthorityScope, at: chrono::DateTime<Utc>) -> Journey {
    let trigger = Uuid::new_v4();
    let channel = Uuid::new_v4();
    Journey::new(
        Uuid::new_v4(),
        scope,
        "Durable execution journey",
        vec![
            JourneyNode::new(
                trigger,
                0,
                Some(channel),
                JourneyNodeDefinition::Trigger {
                    trigger: JourneyTrigger::Enrollment,
                },
            )
            .expect("trigger"),
            JourneyNode::new(
                channel,
                1,
                None,
                JourneyNodeDefinition::Channel {
                    channel: MarketingChannel::Email,
                    payload: JourneyChannelPayload::Email {
                        template_ref: "execution-v1".into(),
                        subject: "Execution".into(),
                    },
                },
            )
            .expect("channel"),
        ],
        at,
    )
    .expect("journey")
}

fn crm_draft(scope: AuthorityScope, at: chrono::DateTime<Utc>) -> Journey {
    let trigger = Uuid::new_v4();
    let handoff = Uuid::new_v4();
    let source = MarketingQualifiedSource {
        lead: MarketingLeadReference::new("journey-lead", scope.clone()).expect("lead reference"),
        acquisition: MarketingAcquisitionReference::new("journey-acquisition", scope.clone())
            .expect("acquisition reference"),
        qualification: MarketingQualificationReference::new("journey-qualification", scope.clone())
            .expect("qualification reference"),
    };
    Journey::new(
        Uuid::new_v4(),
        scope.clone(),
        "Durable CRM handoff journey",
        vec![
            JourneyNode::new(
                trigger,
                0,
                Some(handoff),
                JourneyNodeDefinition::Trigger {
                    trigger: JourneyTrigger::Enrollment,
                },
            )
            .expect("trigger"),
            JourneyNode::new(
                handoff,
                1,
                None,
                JourneyNodeDefinition::CrmHandoff {
                    source: Box::new(source),
                    account: AccountReference::new("journey-account", scope.clone())
                        .expect("account reference"),
                    contact: ContactReference::new("journey-contact", scope)
                        .expect("contact reference"),
                },
            )
            .expect("CRM handoff"),
        ],
        at,
    )
    .expect("CRM journey")
}

fn segment_draft(scope: AuthorityScope, audience_id: Uuid, at: chrono::DateTime<Utc>) -> Journey {
    let trigger = Uuid::new_v4();
    let segment = Uuid::new_v4();
    Journey::new(
        Uuid::new_v4(),
        scope,
        "Durable segment journey",
        vec![
            JourneyNode::new(
                trigger,
                0,
                Some(segment),
                JourneyNodeDefinition::Trigger {
                    trigger: JourneyTrigger::Enrollment,
                },
            )
            .expect("trigger"),
            JourneyNode::new(
                segment,
                1,
                None,
                JourneyNodeDefinition::SegmentChange {
                    segment_id: audience_id,
                    change: SegmentChange::Add,
                },
            )
            .expect("segment change"),
        ],
        at,
    )
    .expect("segment journey")
}

fn tag_draft(scope: AuthorityScope, at: chrono::DateTime<Utc>) -> Journey {
    let trigger = Uuid::new_v4();
    let action = Uuid::new_v4();
    Journey::new(
        Uuid::new_v4(),
        scope,
        "Durable profile tag journey",
        vec![
            JourneyNode::new(
                trigger,
                0,
                Some(action),
                JourneyNodeDefinition::Trigger {
                    trigger: JourneyTrigger::Enrollment,
                },
            )
            .expect("trigger"),
            JourneyNode::new(
                action,
                1,
                None,
                JourneyNodeDefinition::Action {
                    action: JourneyAction::AddProfileTag {
                        tag_ref: "journey-qualified".into(),
                    },
                },
            )
            .expect("profile tag action"),
        ],
        at,
    )
    .expect("profile tag journey")
}

fn conversion_draft(
    scope: AuthorityScope,
    conversion_ref: Uuid,
    at: chrono::DateTime<Utc>,
) -> Journey {
    let trigger = Uuid::new_v4();
    let conversion = Uuid::new_v4();
    Journey::new(
        Uuid::new_v4(),
        scope,
        "Durable conversion journey",
        vec![
            JourneyNode::new(
                trigger,
                0,
                Some(conversion),
                JourneyNodeDefinition::Trigger {
                    trigger: JourneyTrigger::Enrollment,
                },
            )
            .expect("trigger"),
            JourneyNode::new(
                conversion,
                1,
                None,
                JourneyNodeDefinition::Conversion {
                    conversion_ref,
                    revenue_minor: Some(12_500),
                    currency: Some("USD".into()),
                },
            )
            .expect("conversion node"),
        ],
        at,
    )
    .expect("conversion journey")
}

fn owner_notification_draft(scope: AuthorityScope, at: chrono::DateTime<Utc>) -> Journey {
    let trigger = Uuid::new_v4();
    let action = Uuid::new_v4();
    Journey::new(
        Uuid::new_v4(),
        scope,
        "Durable owner notification journey",
        vec![
            JourneyNode::new(
                trigger,
                0,
                Some(action),
                JourneyNodeDefinition::Trigger {
                    trigger: JourneyTrigger::Enrollment,
                },
            )
            .expect("trigger"),
            JourneyNode::new(
                action,
                1,
                None,
                JourneyNodeDefinition::Action {
                    action: JourneyAction::NotifyOwner {
                        owner_ref: "operations-owner".into(),
                    },
                },
            )
            .expect("owner notification action"),
        ],
        at,
    )
    .expect("owner notification journey")
}

async fn running_journey(store: &SqlxMarketingJourneyStore, mut journey: Journey) -> Journey {
    let base = journey.updated_at();
    let first = store
        .create_journey(&journey, &context(journey.scope().clone(), "create", None))
        .await
        .expect("create");
    assert_eq!(first.version, 1);
    for (index, status) in [
        JourneyStatus::Review,
        JourneyStatus::Approved,
        JourneyStatus::Scheduled,
        JourneyStatus::Running,
    ]
    .into_iter()
    .enumerate()
    {
        let expected = journey.version();
        let at = base + Duration::seconds(i64::try_from(index + 1).expect("small index"));
        let command = JourneyTransitionCommand {
            journey_id: journey.id(),
            next_status: status,
            at,
            context: context(
                journey.scope().clone(),
                format!("transition-{index}"),
                Some(expected),
            ),
        };
        journey.transition(&command).expect("domain transition");
        store
            .transition_journey(&command)
            .await
            .expect("persist transition");
    }
    journey
}

#[test]
fn execution_migration_has_claim_retry_and_forced_rls_contract() {
    let sql = include_str!("../../migration/migrations_holding_write/0001_baseline.sql");
    for required in [
        "marketing_journey_execution_cursors",
        "marketing_journey_execution_idempotency",
        "current_claim_id UUID",
        "claim_until TIMESTAMPTZ",
        "dead_lettered_at TIMESTAMPTZ",
        "next_attempt_at TIMESTAMPTZ NOT NULL",
        "FORCE ROW LEVEL SECURITY",
    ] {
        assert!(
            sql.contains(required),
            "missing execution contract: {required}"
        );
    }
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn execution_claim_resume_replay_and_scope_are_durable() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("holding write pool");
    migrate_holding_write(&pool)
        .await
        .expect("holding-write migrations");

    let data_profile_key = format!("marketing-execution-{}", Uuid::new_v4());
    let scope = AuthorityScope::Holding {
        holding_id: Uuid::new_v4(),
    };
    let at = Utc::now() - Duration::minutes(2);
    let journey_store =
        SqlxMarketingJourneyStore::new(pool.clone(), &data_profile_key).expect("journey store");
    let mut journey = running_journey(&journey_store, draft(scope.clone(), at)).await;
    let execution_store = SqlxMarketingJourneyExecutionStore::new(pool.clone(), &data_profile_key)
        .expect("execution store");
    let cursor = JourneyExecutionCursor::start(
        &journey,
        Uuid::new_v4(),
        Uuid::new_v4(),
        journey.updated_at() + Duration::seconds(1),
    )
    .expect("cursor");
    let run_id = cursor.run_id();
    execution_store
        .start_cursor(&journey, &cursor)
        .await
        .expect("start cursor");

    let worker_one = Uuid::new_v4();
    let stale_claims = execution_store
        .claim_due(
            worker_one,
            cursor.updated_at() + Duration::seconds(1),
            1,
            Duration::seconds(4),
        )
        .await
        .expect("first claim");
    assert_eq!(stale_claims.len(), 1);

    assert!(matches!(
        execution_store
            .release_claim(
                &stale_claims[0],
                worker_one,
                cursor.updated_at() + Duration::seconds(6),
                MarketingJourneyExecutionFailure::Retryable,
            )
            .await,
        Err(PersistenceError::ClaimLost { worker_id, event_id })
            if worker_id == worker_one && event_id == run_id
    ));

    let pause_at = cursor.updated_at() + Duration::seconds(2);
    let pause_command = JourneyTransitionCommand {
        journey_id: journey.id(),
        next_status: JourneyStatus::Paused,
        at: pause_at,
        context: context(
            scope.clone(),
            "journey-pause-during-claim",
            Some(journey.version()),
        ),
    };
    journey
        .transition(&pause_command)
        .expect("pause journey domain");
    journey_store
        .transition_journey(&pause_command)
        .await
        .expect("pause journey authoritatively");

    let worker_two = Uuid::new_v4();
    let paused_claims = execution_store
        .claim_due(
            worker_two,
            cursor.updated_at() + Duration::seconds(6),
            1,
            Duration::seconds(30),
        )
        .await
        .expect("paused journey claim check");
    assert!(paused_claims.is_empty());

    assert!(matches!(
        execution_store
            .apply_claim(
                &stale_claims[0],
                worker_one,
                JourneyNodeOutcome::Complete,
                cursor.updated_at() + Duration::seconds(3),
            )
        .await,
        Err(erpx_infrastructure::PersistenceError::InvalidData(message))
            if message == "journey is not running"
    ));
    assert_eq!(
        execution_store
            .load_cursor(run_id)
            .await
            .expect("read cursor after rejected stale claim")
            .expect("cursor remains durable")
            .version(),
        0
    );

    let resume_at = cursor.updated_at() + Duration::seconds(7);
    let resume_command = JourneyTransitionCommand {
        journey_id: journey.id(),
        next_status: JourneyStatus::Running,
        at: resume_at,
        context: context(
            scope.clone(),
            "journey-resume-during-claim",
            Some(journey.version()),
        ),
    };
    journey
        .transition(&resume_command)
        .expect("resume journey domain");
    journey_store
        .transition_journey(&resume_command)
        .await
        .expect("resume journey authoritatively");

    let resumed_claims = execution_store
        .claim_due(
            worker_two,
            cursor.updated_at() + Duration::seconds(8),
            1,
            Duration::seconds(30),
        )
        .await
        .expect("expired claim can resume");
    assert_eq!(resumed_claims.len(), 1);
    let receipt = execution_store
        .apply_claim(
            &resumed_claims[0],
            worker_two,
            JourneyNodeOutcome::Complete,
            cursor.updated_at() + Duration::seconds(9),
        )
        .await
        .expect("apply resumed claim");
    assert_eq!(receipt.cursor_version, 1);
    assert!(!receipt.replayed);
    let replay = execution_store
        .apply_claim(
            &resumed_claims[0],
            worker_two,
            JourneyNodeOutcome::Complete,
            cursor.updated_at() + Duration::seconds(10),
        )
        .await
        .expect("exact worker replay");
    assert!(replay.replayed);
    assert_eq!(replay.event_id, receipt.event_id);

    let mut pause_context = context(scope.clone(), "pause", Some(1));
    pause_context.actor.kind = ActorKind::SystemWorker;
    let paused = execution_store
        .pause_cursor(
            &journey,
            run_id,
            1,
            cursor.updated_at() + Duration::seconds(11),
            &pause_context,
        )
        .await
        .expect("pause cursor");
    assert_eq!(paused.cursor_version, 2);
    let mut resume_context = context(scope.clone(), "resume", Some(2));
    resume_context.actor.kind = ActorKind::SystemWorker;
    let resumed = execution_store
        .resume_cursor(
            &journey,
            run_id,
            2,
            cursor.updated_at() + Duration::seconds(12),
            &resume_context,
        )
        .await
        .expect("resume cursor");
    assert_eq!(resumed.cursor_version, 3);

    let mut tx = pool.begin().await.expect("inspection transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_holding.marketing_journey_execution_cursors WHERE data_profile_key=$1),
            (SELECT count(*) FROM erpx_holding.marketing_journey_execution_idempotency WHERE data_profile_key=$1),
            (SELECT count(*) FROM erpx_holding.outbox_events
              WHERE data_profile_key=$1 AND aggregate_type='marketing_journey_execution')",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *tx)
    .await
    .expect("execution counts");
    assert_eq!(counts, (1, 3, 3));
    let cursor_version: i64 = sqlx::query_scalar(
        "SELECT version FROM erpx_holding.marketing_journey_execution_cursors
          WHERE data_profile_key=$1 AND run_id=$2",
    )
    .bind(&data_profile_key)
    .bind(run_id)
    .fetch_one(&mut *tx)
    .await
    .expect("cursor version");
    assert_eq!(cursor_version, 3);
    let forced: Vec<(String, bool)> = sqlx::query_as(
        "SELECT relname, relforcerowsecurity FROM pg_class WHERE oid IN (
            'erpx_holding.marketing_journey_execution_cursors'::regclass,
            'erpx_holding.marketing_journey_execution_idempotency'::regclass
          ) ORDER BY relname",
    )
    .fetch_all(&mut *tx)
    .await
    .expect("forced RLS metadata");
    assert_eq!(forced.len(), 2);
    assert!(forced.iter().all(|(_, forced)| *forced));
    tx.commit().await.expect("inspection commit");

    let other = SqlxMarketingJourneyExecutionStore::new(
        pool.clone(),
        format!("marketing-execution-other-{}", Uuid::new_v4()),
    )
    .expect("other execution store");
    assert!(
        other
            .load_cursor(run_id)
            .await
            .expect("isolated read")
            .is_none()
    );

    let mut cleanup = pool.begin().await.expect("cleanup transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup scope");
    for table in [
        "erpx_holding.outbox_events",
        "erpx_holding.marketing_journey_execution_idempotency",
        "erpx_holding.marketing_journey_execution_cursors",
        "erpx_holding.marketing_journey_command_idempotency",
        "erpx_holding.marketing_journeys",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE data_profile_key=$1"))
            .bind(&data_profile_key)
            .execute(&mut *cleanup)
            .await
            .expect("cleanup row");
    }
    cleanup.commit().await.expect("cleanup commit");
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn journey_owner_notification_queues_projects_and_replays_exactly() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_READ_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("read pool");
    migrate_holding_write(&pool)
        .await
        .expect("holding-write migrations");
    migrate_holding_read(&read_pool)
        .await
        .expect("holding-read migrations");

    let data_profile_key = format!("marketing-journey-owner-{}", Uuid::new_v4());
    let scope = AuthorityScope::Company {
        holding_id: Uuid::new_v4(),
        company_id: Uuid::new_v4(),
    };
    let profile_id = Uuid::new_v4();
    let base = Utc::now() - Duration::minutes(2);
    let marketing = SqlxMarketingStore::new(pool.clone(), read_pool.clone(), &data_profile_key)
        .expect("marketing");
    marketing
        .upsert_profile(
            &MarketingProfile::new(
                profile_id,
                scope.clone(),
                "journey-owner-profile",
                Some("owner@example.invalid".into()),
                None,
                "en-US",
                std::collections::BTreeMap::new(),
                base,
            )
            .expect("profile"),
            None,
        )
        .await
        .expect("profile persistence");
    let journey_store =
        SqlxMarketingJourneyStore::new(pool.clone(), &data_profile_key).expect("journey store");
    let journey = running_journey(
        &journey_store,
        owner_notification_draft(scope.clone(), base),
    )
    .await;
    let execution_store = SqlxMarketingJourneyExecutionStore::new(pool.clone(), &data_profile_key)
        .expect("execution store");
    let cursor = JourneyExecutionCursor::start(
        &journey,
        Uuid::new_v4(),
        profile_id,
        base + Duration::seconds(1),
    )
    .expect("cursor");
    execution_store
        .start_cursor(&journey, &cursor)
        .await
        .expect("start cursor");
    let worker_id = Uuid::new_v4();
    let trigger_claim = execution_store
        .claim_due(
            worker_id,
            base + Duration::seconds(2),
            1,
            Duration::seconds(30),
        )
        .await
        .expect("trigger claim");
    execution_store
        .apply_claim(
            &trigger_claim[0],
            worker_id,
            JourneyNodeOutcome::Complete,
            base + Duration::seconds(3),
        )
        .await
        .expect("trigger execution");
    let action_claim = execution_store
        .claim_due(
            worker_id,
            base + Duration::seconds(4),
            1,
            Duration::seconds(30),
        )
        .await
        .expect("owner claim");
    let receipt = execution_store
        .apply_claim(
            &action_claim[0],
            worker_id,
            JourneyNodeOutcome::Complete,
            base + Duration::seconds(5),
        )
        .await
        .expect("owner notification execution");
    let replay = execution_store
        .apply_claim(
            &action_claim[0],
            worker_id,
            JourneyNodeOutcome::Complete,
            base + Duration::seconds(6),
        )
        .await
        .expect("owner notification replay");
    assert!(replay.replayed);
    assert_eq!(replay.event_id, receipt.event_id);

    let mut verify = pool.begin().await.expect("write verification");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *verify)
        .await
        .expect("write scope");
    let notification: (Uuid, String) = sqlx::query_as(
        "SELECT notification_ref, owner_ref
           FROM erpx_holding.marketing_journey_owner_notifications
          WHERE data_profile_key=$1",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *verify)
    .await
    .expect("owner notification source");
    assert_eq!(notification.1, "operations-owner");
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.outbox_events
          WHERE data_profile_key=$1 AND aggregate_type='marketing_journey_owner_notification'",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *verify)
    .await
    .expect("owner notification event");
    assert_eq!(events, 1);
    let delivery: (Uuid, String, String) = sqlx::query_as(
        "SELECT delivery_id, state, payload->>'logical_message_ref'
           FROM erpx_holding.marketing_deliveries
          WHERE data_profile_key=$1",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *verify)
    .await
    .expect("owner notification delivery source");
    assert_eq!(delivery.1, "queued");
    assert_eq!(
        delivery.2,
        format!("journey-owner-notification:{}", notification.0)
    );
    verify.commit().await.expect("write verification commit");

    let page = marketing
        .read_source_event_page(None, 10)
        .await
        .expect("source page");
    for event in &page.events {
        marketing
            .project_event(
                event.event_id,
                &event.event_type,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("project source event");
    }
    let delivery_store =
        SqlxMarketingDeliveryStore::new(pool.clone(), read_pool.clone(), &data_profile_key)
            .expect("delivery store");
    assert_eq!(
        delivery_store
            .project_pending_read_model(10)
            .await
            .expect("project owner notification delivery"),
        1
    );
    let mut read_verify = read_pool.begin().await.expect("read verification");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *read_verify)
        .await
        .expect("read scope");
    let owner_ref: String = sqlx::query_scalar(
        "SELECT owner_ref FROM erpx_holding.marketing_journey_owner_notification_read_rows
          WHERE data_profile_key=$1 AND notification_ref=$2",
    )
    .bind(&data_profile_key)
    .bind(notification.0)
    .fetch_one(&mut *read_verify)
    .await
    .expect("owner notification read row");
    assert_eq!(owner_ref, "operations-owner");
    read_verify
        .commit()
        .await
        .expect("read verification commit");
    let joined = marketing
        .read_journey_owner_notification_delivery_page(&scope, None, 10)
        .await
        .expect("owner notification delivery read seam");
    assert_eq!(joined.rows.len(), 1);
    assert_eq!(joined.rows[0].notification_ref, notification.0);
    assert_eq!(joined.rows[0].delivery_id, Some(delivery.0));
    assert_eq!(joined.rows[0].delivery_state.as_deref(), Some("queued"));
    assert_eq!(joined.rows[0].delivery_attempt_count, Some(0));
    let reconciliation = marketing
        .reconcile_projection()
        .await
        .expect("owner reconciliation");
    assert_eq!(reconciliation.missing_aggregate_count, 0);
    assert_eq!(reconciliation.stale_aggregate_count, 0);
    assert_eq!(reconciliation.payload_mismatch_count, 0);

    let mut cleanup = read_pool.begin().await.expect("read cleanup");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *cleanup)
        .await
        .expect("read cleanup scope");
    for table in [
        "erpx_holding.marketing_journey_owner_notification_read_rows",
        "erpx_holding.marketing_delivery_read_rows",
        "erpx_holding.marketing_projection_checkpoints",
        "erpx_holding.marketing_delivery_projection_checkpoints",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE data_profile_key=$1"))
            .bind(&data_profile_key)
            .execute(&mut *cleanup)
            .await
            .expect("read cleanup row");
    }
    cleanup.commit().await.expect("read cleanup commit");
    let mut cleanup = pool.begin().await.expect("write cleanup");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *cleanup)
        .await
        .expect("write cleanup scope");
    for table in [
        "erpx_holding.marketing_journey_owner_notifications",
        "erpx_holding.marketing_deliveries",
        "erpx_holding.marketing_delivery_idempotency",
        "erpx_holding.marketing_profiles",
        "erpx_holding.outbox_events",
        "erpx_holding.marketing_journey_execution_idempotency",
        "erpx_holding.marketing_journey_execution_cursors",
        "erpx_holding.marketing_journey_command_idempotency",
        "erpx_holding.marketing_journeys",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE data_profile_key=$1"))
            .bind(&data_profile_key)
            .execute(&mut *cleanup)
            .await
            .expect("write cleanup row");
    }
    cleanup.commit().await.expect("write cleanup commit");
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn journey_conversion_persists_projects_and_replays_exactly() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    let read_url = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_READ_DATABASE_URL is required");
    let read_pool = PgPool::connect(&read_url).await.expect("read pool");
    migrate_holding_write(&pool)
        .await
        .expect("holding-write migrations");
    migrate_holding_read(&read_pool)
        .await
        .expect("holding-read migrations");

    let data_profile_key = format!("marketing-journey-conversion-{}", Uuid::new_v4());
    let scope = AuthorityScope::Company {
        holding_id: Uuid::new_v4(),
        company_id: Uuid::new_v4(),
    };
    let profile_id = Uuid::new_v4();
    let conversion_ref = Uuid::new_v4();
    let base = Utc::now() - Duration::minutes(2);
    let marketing = SqlxMarketingStore::new(pool.clone(), read_pool.clone(), &data_profile_key)
        .expect("marketing store");
    marketing
        .upsert_profile(
            &MarketingProfile::new(
                profile_id,
                scope.clone(),
                "journey-conversion-profile",
                Some("conversion@example.invalid".into()),
                None,
                "en-US",
                std::collections::BTreeMap::new(),
                base,
            )
            .expect("profile"),
            None,
        )
        .await
        .expect("profile persistence");
    let journey_store =
        SqlxMarketingJourneyStore::new(pool.clone(), &data_profile_key).expect("journey store");
    let journey = running_journey(
        &journey_store,
        conversion_draft(scope.clone(), conversion_ref, base),
    )
    .await;
    let execution_store = SqlxMarketingJourneyExecutionStore::new(pool.clone(), &data_profile_key)
        .expect("execution store");
    let cursor = JourneyExecutionCursor::start(
        &journey,
        Uuid::new_v4(),
        profile_id,
        base + Duration::seconds(1),
    )
    .expect("cursor");
    execution_store
        .start_cursor(&journey, &cursor)
        .await
        .expect("start cursor");
    let worker_id = Uuid::new_v4();
    let trigger_claim = execution_store
        .claim_due(
            worker_id,
            base + Duration::seconds(2),
            1,
            Duration::seconds(30),
        )
        .await
        .expect("trigger claim");
    execution_store
        .apply_claim(
            &trigger_claim[0],
            worker_id,
            JourneyNodeOutcome::Complete,
            base + Duration::seconds(3),
        )
        .await
        .expect("trigger execution");
    let conversion_claim = execution_store
        .claim_due(
            worker_id,
            base + Duration::seconds(4),
            1,
            Duration::seconds(30),
        )
        .await
        .expect("conversion claim");
    let receipt = execution_store
        .apply_claim(
            &conversion_claim[0],
            worker_id,
            JourneyNodeOutcome::Complete,
            base + Duration::seconds(5),
        )
        .await
        .expect("conversion execution");
    let replay = execution_store
        .apply_claim(
            &conversion_claim[0],
            worker_id,
            JourneyNodeOutcome::Complete,
            base + Duration::seconds(6),
        )
        .await
        .expect("conversion replay");
    assert!(replay.replayed);
    assert_eq!(replay.event_id, receipt.event_id);

    let mut source_tx = pool.begin().await.expect("source transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *source_tx)
        .await
        .expect("source scope");
    let conversion_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.marketing_journey_conversions
         WHERE data_profile_key=$1 AND conversion_ref=$2",
    )
    .bind(&data_profile_key)
    .bind(conversion_ref)
    .fetch_one(&mut *source_tx)
    .await
    .expect("conversion count");
    assert_eq!(conversion_count, 1);
    let conversion_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_type='marketing_journey_conversion'
           AND aggregate_id=$2",
    )
    .bind(&data_profile_key)
    .bind(conversion_ref)
    .fetch_one(&mut *source_tx)
    .await
    .expect("conversion events");
    assert_eq!(conversion_events, 1);
    source_tx.commit().await.expect("source commit");

    let page = marketing
        .read_source_event_page(None, 10)
        .await
        .expect("source page");
    assert!(
        page.events.iter().any(|event| event.aggregate_type
            == erpx_infrastructure::MARKETING_JOURNEY_CONVERSION_AGGREGATE)
    );
    for event in &page.events {
        marketing
            .project_event(
                event.event_id,
                &event.event_type,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("project source event");
    }
    let read = marketing
        .read_journey_conversion_page(&scope, None, 10)
        .await
        .expect("conversion read page");
    assert_eq!(read.rows.len(), 1);
    assert_eq!(read.rows[0].conversion_ref, conversion_ref);
    assert_eq!(read.rows[0].revenue_minor, Some(12_500));
    assert_eq!(read.rows[0].currency.as_deref(), Some("USD"));
    let reconciliation = marketing
        .reconcile_projection()
        .await
        .expect("conversion reconciliation");
    assert_eq!(reconciliation.missing_aggregate_count, 0);
    assert_eq!(reconciliation.stale_aggregate_count, 0);
    assert_eq!(reconciliation.payload_mismatch_count, 0);

    let mut cleanup = read_pool.begin().await.expect("read cleanup transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup scope");
    for table in [
        "erpx_holding.marketing_journey_conversion_read_rows",
        "erpx_holding.marketing_projection_checkpoints",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE data_profile_key=$1"))
            .bind(&data_profile_key)
            .execute(&mut *cleanup)
            .await
            .expect("cleanup row");
    }
    cleanup.commit().await.expect("cleanup commit");

    let mut cleanup = pool.begin().await.expect("write cleanup transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *cleanup)
        .await
        .expect("write cleanup scope");
    for table in [
        "erpx_holding.marketing_journey_conversions",
        "erpx_holding.marketing_profiles",
        "erpx_holding.outbox_events",
        "erpx_holding.marketing_journey_execution_idempotency",
        "erpx_holding.marketing_journey_execution_cursors",
        "erpx_holding.marketing_journey_command_idempotency",
        "erpx_holding.marketing_journeys",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE data_profile_key=$1"))
            .bind(&data_profile_key)
            .execute(&mut *cleanup)
            .await
            .expect("write cleanup row");
    }
    cleanup.commit().await.expect("write cleanup commit");
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn journey_profile_tag_action_updates_authoritative_profile_and_replays_exactly() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    migrate_holding_write(&pool)
        .await
        .expect("holding-write migrations");

    let data_profile_key = format!("marketing-journey-tag-{}", Uuid::new_v4());
    let scope = AuthorityScope::Company {
        holding_id: Uuid::new_v4(),
        company_id: Uuid::new_v4(),
    };
    let profile_id = Uuid::new_v4();
    let base = Utc::now() - Duration::minutes(2);
    let marketing = SqlxMarketingStore::new(pool.clone(), pool.clone(), &data_profile_key)
        .expect("marketing store");
    marketing
        .upsert_profile(
            &MarketingProfile::new(
                profile_id,
                scope.clone(),
                "journey-tag-profile",
                Some("tag@example.invalid".into()),
                None,
                "en-US",
                std::collections::BTreeMap::new(),
                base,
            )
            .expect("profile"),
            None,
        )
        .await
        .expect("profile persistence");
    let journey_store =
        SqlxMarketingJourneyStore::new(pool.clone(), &data_profile_key).expect("journey store");
    let journey = running_journey(&journey_store, tag_draft(scope.clone(), base)).await;
    let execution_store = SqlxMarketingJourneyExecutionStore::new(pool.clone(), &data_profile_key)
        .expect("execution store");
    let cursor = JourneyExecutionCursor::start(
        &journey,
        Uuid::new_v4(),
        profile_id,
        journey.updated_at() + Duration::seconds(1),
    )
    .expect("cursor");
    execution_store
        .start_cursor(&journey, &cursor)
        .await
        .expect("start cursor");
    let worker_id = Uuid::new_v4();
    let trigger_claim = execution_store
        .claim_due(
            worker_id,
            base + Duration::seconds(6),
            1,
            Duration::seconds(30),
        )
        .await
        .expect("trigger claim");
    assert_eq!(trigger_claim.len(), 1);
    execution_store
        .apply_claim(
            &trigger_claim[0],
            worker_id,
            JourneyNodeOutcome::Complete,
            base + Duration::seconds(7),
        )
        .await
        .expect("trigger execution");
    let action_claim = execution_store
        .claim_due(
            worker_id,
            base + Duration::seconds(8),
            1,
            Duration::seconds(30),
        )
        .await
        .expect("action claim");
    assert_eq!(action_claim.len(), 1);
    let receipt = execution_store
        .apply_claim(
            &action_claim[0],
            worker_id,
            JourneyNodeOutcome::Complete,
            base + Duration::seconds(9),
        )
        .await
        .expect("action execution");
    let replay = execution_store
        .apply_claim(
            &action_claim[0],
            worker_id,
            JourneyNodeOutcome::Complete,
            base + Duration::seconds(10),
        )
        .await
        .expect("action replay");
    assert!(replay.replayed);
    assert_eq!(replay.event_id, receipt.event_id);

    let mut verify = pool.begin().await.expect("verification transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *verify)
        .await
        .expect("verification scope");
    let attributes: serde_json::Value = sqlx::query_scalar(
        "SELECT attributes FROM erpx_holding.marketing_profiles
         WHERE data_profile_key=$1 AND profile_id=$2",
    )
    .bind(&data_profile_key)
    .bind(profile_id)
    .fetch_one(&mut *verify)
    .await
    .expect("profile attributes");
    assert_eq!(attributes["tag:journey-qualified"], "true");
    let profile_version: i64 = sqlx::query_scalar(
        "SELECT version FROM erpx_holding.marketing_profiles
         WHERE data_profile_key=$1 AND profile_id=$2",
    )
    .bind(&data_profile_key)
    .bind(profile_id)
    .fetch_one(&mut *verify)
    .await
    .expect("profile version");
    assert_eq!(profile_version, 2);
    let profile_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_type='marketing_profile'
           AND aggregate_id=$2",
    )
    .bind(&data_profile_key)
    .bind(profile_id)
    .fetch_one(&mut *verify)
    .await
    .expect("profile events");
    assert_eq!(profile_events, 2);
    verify.commit().await.expect("verification commit");

    let mut cleanup = pool.begin().await.expect("cleanup transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup scope");
    for table in [
        "erpx_holding.marketing_profiles",
        "erpx_holding.outbox_events",
        "erpx_holding.marketing_journey_execution_idempotency",
        "erpx_holding.marketing_journey_execution_cursors",
        "erpx_holding.marketing_journey_command_idempotency",
        "erpx_holding.marketing_journeys",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE data_profile_key=$1"))
            .bind(&data_profile_key)
            .execute(&mut *cleanup)
            .await
            .expect("cleanup row");
    }
    cleanup.commit().await.expect("cleanup commit");
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn journey_crm_handoff_commits_with_execution_and_replays_exactly() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    migrate_holding_write(&pool)
        .await
        .expect("holding-write migrations");

    let data_profile_key = format!("marketing-journey-crm-{}", Uuid::new_v4());
    let scope = AuthorityScope::Company {
        holding_id: Uuid::new_v4(),
        company_id: Uuid::new_v4(),
    };
    let base = Utc::now() - Duration::minutes(2);
    let journey_store =
        SqlxMarketingJourneyStore::new(pool.clone(), &data_profile_key).expect("journey store");
    let journey = running_journey(&journey_store, crm_draft(scope.clone(), base)).await;
    let execution_store = SqlxMarketingJourneyExecutionStore::new(pool.clone(), &data_profile_key)
        .expect("execution store");
    let cursor = JourneyExecutionCursor::start(
        &journey,
        Uuid::new_v4(),
        Uuid::new_v4(),
        base + Duration::seconds(1),
    )
    .expect("cursor");
    let run_id = cursor.run_id();
    execution_store
        .start_cursor(&journey, &cursor)
        .await
        .expect("start cursor");

    let worker_id = Uuid::new_v4();
    let first_claim = execution_store
        .claim_due(
            worker_id,
            base + Duration::seconds(2),
            1,
            Duration::seconds(30),
        )
        .await
        .expect("trigger claim");
    assert_eq!(first_claim.len(), 1);
    execution_store
        .apply_claim(
            &first_claim[0],
            worker_id,
            JourneyNodeOutcome::Complete,
            base + Duration::seconds(3),
        )
        .await
        .expect("trigger execution");

    let handoff_claim = execution_store
        .claim_due(
            worker_id,
            base + Duration::seconds(4),
            1,
            Duration::seconds(30),
        )
        .await
        .expect("CRM handoff claim");
    assert_eq!(handoff_claim.len(), 1);
    let receipt = execution_store
        .apply_claim(
            &handoff_claim[0],
            worker_id,
            JourneyNodeOutcome::Complete,
            base + Duration::seconds(5),
        )
        .await
        .expect("CRM handoff execution");
    assert!(!receipt.replayed);
    let replay = execution_store
        .apply_claim(
            &handoff_claim[0],
            worker_id,
            JourneyNodeOutcome::Complete,
            base + Duration::seconds(6),
        )
        .await
        .expect("CRM handoff replay");
    assert!(replay.replayed);
    assert_eq!(replay.event_id, receipt.event_id);

    let mut verify = pool.begin().await.expect("verification transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *verify)
        .await
        .expect("verification scope");
    let handoff_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.crm_accepted_lead_handoffs
         WHERE data_profile_key=$1",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *verify)
    .await
    .expect("CRM handoff count");
    assert_eq!(handoff_count, 1);
    let crm_event_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_type='marketing_crm_accepted_lead_handoff'",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *verify)
    .await
    .expect("CRM event count");
    assert_eq!(crm_event_count, 1);
    let execution_event_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_type='marketing_journey_execution'",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *verify)
    .await
    .expect("execution event count");
    assert_eq!(execution_event_count, 2);
    let idempotency_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.crm_idempotency_bindings
         WHERE data_profile_key=$1",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *verify)
    .await
    .expect("CRM idempotency count");
    assert_eq!(idempotency_count, 1);
    verify.commit().await.expect("verification commit");

    let mut cleanup = pool.begin().await.expect("cleanup transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup scope");
    for (table, trigger) in [
        (
            "erpx_holding.crm_idempotency_bindings",
            "crm_idempotency_bindings_immutable",
        ),
        (
            "erpx_holding.crm_accepted_lead_handoffs",
            "crm_handoffs_immutable",
        ),
    ] {
        sqlx::query(&format!("ALTER TABLE {table} DISABLE TRIGGER {trigger}"))
            .execute(&mut *cleanup)
            .await
            .expect("disable cleanup trigger");
    }
    for table in [
        "erpx_holding.crm_idempotency_bindings",
        "erpx_holding.crm_accepted_lead_handoffs",
        "erpx_holding.outbox_events",
        "erpx_holding.marketing_journey_execution_idempotency",
        "erpx_holding.marketing_journey_execution_cursors",
        "erpx_holding.marketing_journey_command_idempotency",
        "erpx_holding.marketing_journeys",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE data_profile_key=$1"))
            .bind(&data_profile_key)
            .execute(&mut *cleanup)
            .await
            .expect("cleanup row");
    }
    for (table, trigger) in [
        (
            "erpx_holding.crm_idempotency_bindings",
            "crm_idempotency_bindings_immutable",
        ),
        (
            "erpx_holding.crm_accepted_lead_handoffs",
            "crm_handoffs_immutable",
        ),
    ] {
        sqlx::query(&format!("ALTER TABLE {table} ENABLE TRIGGER {trigger}"))
            .execute(&mut *cleanup)
            .await
            .expect("enable cleanup trigger");
    }
    cleanup.commit().await.expect("cleanup commit");
    assert!(
        !run_id.is_nil(),
        "the committed execution should retain a durable run identity"
    );
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn journey_segment_change_updates_authoritative_audience_and_replays_exactly() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    migrate_holding_write(&pool)
        .await
        .expect("holding-write migrations");

    let data_profile_key = format!("marketing-journey-segment-{}", Uuid::new_v4());
    let scope = AuthorityScope::Company {
        holding_id: Uuid::new_v4(),
        company_id: Uuid::new_v4(),
    };
    let profile_id = Uuid::new_v4();
    let audience_id = Uuid::new_v4();
    let base = Utc::now() - Duration::minutes(2);
    let marketing = SqlxMarketingStore::new(pool.clone(), pool.clone(), &data_profile_key)
        .expect("marketing store");
    let profile = MarketingProfile::new(
        profile_id,
        scope.clone(),
        "journey-segment-profile",
        Some("segment@example.invalid".into()),
        None,
        "en-US",
        std::collections::BTreeMap::new(),
        base,
    )
    .expect("profile");
    marketing
        .upsert_profile(&profile, None)
        .await
        .expect("profile persistence");
    marketing
        .upsert_audience_membership(
            &AudienceMembership {
                audience_id,
                scope: scope.clone(),
                profile_ids: Vec::new(),
                materialized_at: base,
            },
            "journey-segment-seed",
        )
        .await
        .expect("audience persistence");

    let journey_store =
        SqlxMarketingJourneyStore::new(pool.clone(), &data_profile_key).expect("journey store");
    let journey = running_journey(
        &journey_store,
        segment_draft(scope.clone(), audience_id, base),
    )
    .await;
    let execution_store = SqlxMarketingJourneyExecutionStore::new(pool.clone(), &data_profile_key)
        .expect("execution store");
    let cursor = JourneyExecutionCursor::start(
        &journey,
        Uuid::new_v4(),
        profile_id,
        journey.updated_at() + Duration::seconds(1),
    )
    .expect("cursor");
    execution_store
        .start_cursor(&journey, &cursor)
        .await
        .expect("start cursor");
    let worker_id = Uuid::new_v4();
    let trigger_claim = execution_store
        .claim_due(
            worker_id,
            base + Duration::seconds(6),
            1,
            Duration::seconds(30),
        )
        .await
        .expect("trigger claim");
    assert_eq!(trigger_claim.len(), 1);
    execution_store
        .apply_claim(
            &trigger_claim[0],
            worker_id,
            JourneyNodeOutcome::Complete,
            base + Duration::seconds(7),
        )
        .await
        .expect("trigger execution");
    let segment_claim = execution_store
        .claim_due(
            worker_id,
            base + Duration::seconds(8),
            1,
            Duration::seconds(30),
        )
        .await
        .expect("segment claim");
    assert_eq!(segment_claim.len(), 1);
    let receipt = execution_store
        .apply_claim(
            &segment_claim[0],
            worker_id,
            JourneyNodeOutcome::Complete,
            base + Duration::seconds(9),
        )
        .await
        .expect("segment execution");
    let replay = execution_store
        .apply_claim(
            &segment_claim[0],
            worker_id,
            JourneyNodeOutcome::Complete,
            base + Duration::seconds(10),
        )
        .await
        .expect("segment replay");
    assert!(replay.replayed);
    assert_eq!(replay.event_id, receipt.event_id);

    let mut verify = pool.begin().await.expect("verification transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *verify)
        .await
        .expect("verification scope");
    let member_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.marketing_audience_memberships
         WHERE data_profile_key=$1 AND audience_id=$2 AND profile_id=$3",
    )
    .bind(&data_profile_key)
    .bind(audience_id)
    .bind(profile_id)
    .fetch_one(&mut *verify)
    .await
    .expect("membership count");
    assert_eq!(member_count, 1);
    let audience_version: i64 = sqlx::query_scalar(
        "SELECT version FROM erpx_holding.marketing_audiences
         WHERE data_profile_key=$1 AND audience_id=$2",
    )
    .bind(&data_profile_key)
    .bind(audience_id)
    .fetch_one(&mut *verify)
    .await
    .expect("audience version");
    assert_eq!(audience_version, 2);
    let audience_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_type='marketing_audience'
           AND aggregate_id=$2",
    )
    .bind(&data_profile_key)
    .bind(audience_id)
    .fetch_one(&mut *verify)
    .await
    .expect("audience events");
    assert_eq!(audience_events, 2);
    verify.commit().await.expect("verification commit");

    let mut cleanup = pool.begin().await.expect("cleanup transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup scope");
    for table in [
        "erpx_holding.marketing_audience_memberships",
        "erpx_holding.marketing_audiences",
        "erpx_holding.marketing_command_idempotency",
        "erpx_holding.marketing_profiles",
        "erpx_holding.outbox_events",
        "erpx_holding.marketing_journey_execution_idempotency",
        "erpx_holding.marketing_journey_execution_cursors",
        "erpx_holding.marketing_journey_command_idempotency",
        "erpx_holding.marketing_journeys",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE data_profile_key=$1"))
            .bind(&data_profile_key)
            .execute(&mut *cleanup)
            .await
            .expect("cleanup row");
    }
    cleanup.commit().await.expect("cleanup commit");
}
