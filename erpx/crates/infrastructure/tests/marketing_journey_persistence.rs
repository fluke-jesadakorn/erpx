//! Disposable PostgreSQL evidence for the authoritative Marketing Journey
//! write boundary. The ignored test uses a unique holding reference and cleans
//! every row it creates; it never prints a database URL or credential.

use chrono::{Duration, Utc};
use erpx_domain::{
    contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope},
    marketing::MarketingChannel,
    marketing_journey::{
        Journey, JourneyChannelPayload, JourneyNode, JourneyNodeDefinition, JourneyStatus,
        JourneyTransitionCommand, JourneyTrigger,
    },
};
use erpx_infrastructure::SqlxMarketingJourneyStore;
use erpx_migration::migrate_tenant_write;
use sqlx::PgPool;
use uuid::Uuid;

fn context(scope: TenantScope, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "marketing-journey-test".into(),
            kind: ActorKind::TenantUser,
            session_id: Some(Uuid::new_v4()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "marketing.journey.persistence.test",
    )
    .expect("valid journey test context")
}

fn journey(scope: TenantScope) -> Journey {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let at = Utc::now();
    let result = Journey::new(
        Uuid::new_v4(),
        scope,
        "Bounded nurture journey",
        vec![
            JourneyNode::new(
                first,
                0,
                Some(second),
                JourneyNodeDefinition::Trigger {
                    trigger: JourneyTrigger::Enrollment,
                },
            )
            .expect("trigger node"),
            JourneyNode::new(
                second,
                1,
                None,
                JourneyNodeDefinition::Channel {
                    channel: MarketingChannel::Email,
                    payload: JourneyChannelPayload::Email {
                        template_ref: "welcome-v1".into(),
                        subject: "Welcome".into(),
                    },
                },
            )
            .expect("channel node"),
        ],
        at,
    )
    .expect("journey");
    // Keep the fixture timestamp comfortably before later commands.
    assert!(result.updated_at() <= at + Duration::seconds(1));
    result
}

#[test]
fn migration_defines_journey_state_idempotency_and_forced_rls() {
    let sql = include_str!("../../migration/migrations_tenant_write/0044_marketing_journeys.sql");
    for required in [
        "marketing_journeys",
        "marketing_journey_command_idempotency",
        "version BIGINT NOT NULL CHECK (version > 0)",
        "FORCE ROW LEVEL SECURITY",
        "tenant_scope_marketing_journeys",
    ] {
        assert!(
            sql.contains(required),
            "missing journey migration contract: {required}"
        );
    }
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL write database"]
async fn journey_write_is_atomic_idempotent_versioned_and_scope_bound() {
    let url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("tenant write pool");
    migrate_tenant_write(&pool)
        .await
        .expect("tenant-write migrations");

    let holding_ref = format!("marketing-journey-{}", Uuid::new_v4());
    let scope = TenantScope::Company {
        holding_id: Uuid::new_v4(),
        company_id: Uuid::new_v4(),
    };
    let draft = journey(scope.clone());
    let create_context = context(scope.clone(), "journey-create");
    let store = SqlxMarketingJourneyStore::new(pool.clone(), &holding_ref).expect("journey store");

    let first = store
        .create_journey(&draft, &create_context)
        .await
        .expect("create journey");
    assert_eq!(first.version, 1);
    assert!(!first.replayed);

    let replay = store
        .create_journey(&draft, &create_context)
        .await
        .expect("replay journey creation");
    assert!(replay.replayed);
    assert_eq!(replay.event_id, first.event_id);
    assert_eq!(
        store.load_journey(draft.id()).await.expect("load"),
        Some(draft.clone())
    );

    let mut transition_context = context(scope, "journey-review");
    transition_context.expected_aggregate_version = Some(1);
    let command = JourneyTransitionCommand {
        journey_id: draft.id(),
        next_status: JourneyStatus::Review,
        at: draft.updated_at() + Duration::seconds(1),
        context: transition_context.clone(),
    };
    let changed = store
        .transition_journey(&command)
        .await
        .expect("transition journey");
    assert_eq!(changed.version, 2);
    assert!(!changed.replayed);
    let transition_replay = store
        .transition_journey(&command)
        .await
        .expect("replay transition");
    assert!(transition_replay.replayed);
    assert_eq!(transition_replay.event_id, changed.event_id);
    let loaded = store
        .load_journey(draft.id())
        .await
        .expect("load transitioned journey")
        .expect("journey exists");
    assert_eq!(loaded.status(), JourneyStatus::Review);
    assert_eq!(loaded.version(), 2);

    let mut tx = pool.begin().await.expect("inspection transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_tenant.marketing_journeys WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.marketing_journey_command_idempotency WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.aggregate_snapshots
              WHERE holding_ref=$1 AND aggregate_type='marketing_journey'),
            (SELECT count(*) FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1 AND aggregate_type='marketing_journey')",
    )
    .bind(&holding_ref)
    .fetch_one(&mut *tx)
    .await
    .expect("durable counts");
    assert_eq!(counts, (1, 2, 1, 2));
    let envelope: (
        i32,
        i32,
        serde_json::Value,
        serde_json::Value,
        Uuid,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT contract_version, event_version, tenant_scope, actor,
                    correlation_id, idempotency_key
               FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1 AND event_id=$2",
    )
    .bind(&holding_ref)
    .bind(first.event_id)
    .fetch_one(&mut *tx)
    .await
    .expect("complete event envelope");
    assert_eq!(envelope.0, 1);
    assert_eq!(envelope.1, 1);
    assert_eq!(
        envelope.2,
        serde_json::to_value(&create_context.tenant_scope).expect("scope")
    );
    assert_eq!(
        envelope.3,
        serde_json::to_value(&create_context.actor).expect("actor")
    );
    assert_eq!(envelope.4, create_context.correlation_id);
    assert_eq!(
        envelope.5.as_deref(),
        Some(create_context.idempotency_key.as_str())
    );
    let forced: Vec<(String, bool)> = sqlx::query_as(
        "SELECT relname, relforcerowsecurity
           FROM pg_class
          WHERE oid IN (
            'erpx_tenant.marketing_journeys'::regclass,
            'erpx_tenant.marketing_journey_command_idempotency'::regclass
          ) ORDER BY relname",
    )
    .fetch_all(&mut *tx)
    .await
    .expect("forced RLS metadata");
    assert_eq!(forced.len(), 2);
    assert!(forced.iter().all(|(_, enabled)| *enabled));
    tx.commit().await.expect("inspection commit");

    let other_store = SqlxMarketingJourneyStore::new(
        pool.clone(),
        format!("marketing-journey-other-{}", Uuid::new_v4()),
    )
    .expect("other holding store");
    assert!(
        other_store
            .load_journey(draft.id())
            .await
            .expect("other holding read")
            .is_none()
    );

    let mut cleanup = pool.begin().await.expect("cleanup transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup scope");
    sqlx::query("DELETE FROM erpx_tenant.outbox_events WHERE holding_ref=$1")
        .bind(&holding_ref)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup outbox");
    sqlx::query("DELETE FROM erpx_tenant.aggregate_snapshots WHERE holding_ref=$1")
        .bind(&holding_ref)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup snapshots");
    sqlx::query(
        "DELETE FROM erpx_tenant.marketing_journey_command_idempotency WHERE holding_ref=$1",
    )
    .bind(&holding_ref)
    .execute(&mut *cleanup)
    .await
    .expect("cleanup idempotency");
    sqlx::query("DELETE FROM erpx_tenant.marketing_journeys WHERE holding_ref=$1")
        .bind(&holding_ref)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup journeys");
    cleanup.commit().await.expect("cleanup commit");
}
