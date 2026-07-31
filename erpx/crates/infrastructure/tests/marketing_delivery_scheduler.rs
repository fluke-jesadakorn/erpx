use chrono::{Duration, Utc};
use erpx_infrastructure::{
    MarketingDeliveryScheduleRequest, MarketingDeliverySchedulerPolicy,
    MarketingDeliverySchedulerState, SqlxMarketingDeliverySchedulerRepository,
};
use erpx_migration::migrate_control_plane;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

fn database_url() -> String {
    std::env::var("ERPX_TEST_MARKETING_SCHEDULER_DATABASE_URL")
        .expect("ERPX_TEST_MARKETING_SCHEDULER_DATABASE_URL is required for this ignored test")
}

#[tokio::test]
#[ignore = "requires one disposable PostgreSQL control database"]
async fn scheduler_claims_renews_repeats_dead_letters_and_replays() {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url())
        .await
        .expect("scheduler database");
    migrate_control_plane(&pool)
        .await
        .expect("control-plane migration");
    let repository = SqlxMarketingDeliverySchedulerRepository::new(pool.clone());
    let job_key = format!("marketing_delivery.test.{}", Uuid::new_v4());
    let now = Utc::now();
    let request = MarketingDeliveryScheduleRequest {
        job_key: job_key.clone(),
        available_at: now,
        policy: MarketingDeliverySchedulerPolicy {
            cadence_seconds: 60,
            max_attempts: 2,
            backoff_base_seconds: 1,
            backoff_max_seconds: 5,
        },
    };
    let first = repository
        .ensure_scheduled(request.clone(), now)
        .await
        .expect("schedule");
    let replayed_schedule = repository
        .ensure_scheduled(request.clone(), now + Duration::seconds(10))
        .await
        .expect("idempotent schedule");
    assert_eq!(first.id, replayed_schedule.id);

    let worker_id = Uuid::new_v4();
    let claim = repository
        .claim_due(worker_id, now + Duration::seconds(1), Duration::seconds(30))
        .await
        .expect("claim")
        .expect("due claim");
    repository
        .renew(&claim, now + Duration::seconds(2), Duration::seconds(30))
        .await
        .expect("renew");
    repository
        .complete(&claim, now + Duration::seconds(3))
        .await
        .expect("complete");
    assert_eq!(
        repository.get(first.id).await.expect("completed job").state,
        MarketingDeliverySchedulerState::Pending
    );

    let dead_key = format!("marketing_delivery.test.{}", Uuid::new_v4());
    let dead_job = repository
        .ensure_scheduled(
            MarketingDeliveryScheduleRequest {
                job_key: dead_key.clone(),
                available_at: now,
                policy: MarketingDeliverySchedulerPolicy {
                    max_attempts: 1,
                    ..MarketingDeliverySchedulerPolicy::default_policy()
                },
            },
            now,
        )
        .await
        .expect("dead-letter schedule");
    let dead_claim = repository
        .claim_due(
            Uuid::new_v4(),
            now + Duration::seconds(1),
            Duration::seconds(30),
        )
        .await
        .expect("dead-letter claim")
        .expect("due dead-letter claim");
    repository
        .fail(&dead_claim, now + Duration::seconds(2), "bounded_failure")
        .await
        .expect("dead-letter failure");
    assert_eq!(
        repository
            .get(dead_job.id)
            .await
            .expect("dead-letter job")
            .state,
        MarketingDeliverySchedulerState::DeadLetter
    );
    repository
        .replay_dead_letter(&dead_key, now + Duration::seconds(3))
        .await
        .expect("dead-letter replay");
    assert_eq!(
        repository
            .get(dead_job.id)
            .await
            .expect("replayed job")
            .state,
        MarketingDeliverySchedulerState::Pending
    );
    pool.close().await;
}
