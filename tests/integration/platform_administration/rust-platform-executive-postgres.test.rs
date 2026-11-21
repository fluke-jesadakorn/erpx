//! Actual Platform Executive reads over leased Control and Control-read PostgreSQL.
//!
//! The fixture uses the real migration tracks and owner tables. It does not
//! exercise the shared composition authority or an authenticated browser.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_platform_administration::public::platform_executive::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::FailureKind;

const JUNE: i64 = 1_780_272_000_000;
const JULY: i64 = 1_782_864_000_000;

fn actor(permission: bool) -> PlatformExecutiveActor {
    PlatformExecutiveActor {
        user_id: "platform-auditor".into(),
        assignment_id: "platform-assignment".into(),
        role: "platform_auditor".into(),
        permissions: if permission {
            vec![EXECUTIVE_PERMISSION.into()]
        } else {
            Vec::new()
        },
    }
}

async fn migrate(pool: &DatabasePool, track: MigrationTrack) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, track).await.unwrap();
    migrate_database(pool, track, &files).await.unwrap();
}

async fn seed_provisioning_failure(control: &DatabasePool) {
    sqlx::query(
        "INSERT INTO platform_holding_provisioning(id,holding_id,slug,display_name,write_binding,\
         read_binding,files_binding,status,requested_by,assignment_id,reason,request_hash,\
         idempotency_key,version,error,requested_at,activated_at,updated_at) \
         VALUES('platform-executive-test','test-holding','test-holding','Private Holding Name',\
         'TEST_DB','TEST_READ_DB','TEST_FILES','failed','operator','assignment',\
         'private failure reason','request-hash','platform-executive-test-key',1,\
         'PRIVATE_ERROR',$1,NULL,$1)",
    )
    .bind(JUNE)
    .execute(control.raw())
    .await
    .unwrap();
}

async fn seed_failed_event(control: &DatabasePool, id: &str, created_at: i64) {
    sqlx::query(
        "INSERT INTO platform_holding_provisioning_events(id,provisioning_id,holding_id,action,\
         actor_id,assignment_id,reason,detail_json,created_at) \
         VALUES($1,'platform-executive-test','test-holding','failed','operator','assignment',\
         'private failure reason','{\"private\":\"failure detail\"}',$2)",
    )
    .bind(id)
    .bind(created_at)
    .execute(control.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn executive_snapshot_and_sources_read_only_bounded_owner_facts() {
    fixture::with_database(3, |control| async move {
        fixture::with_database(2, move |control_read| async move {
            migrate(&control, MigrationTrack::Control).await;
            migrate(&control_read, MigrationTrack::ControlRead).await;
            seed_provisioning_failure(&control).await;
            for index in 0..31 {
                seed_failed_event(&control, &format!("june-{index}"), JUNE + index).await;
            }
            seed_failed_event(&control, "july-1", JULY).await;
            sqlx::query(
                "INSERT INTO platform_health(component,status,checked_at,detail) \
                 VALUES('jobs','degraded',$1,'private health detail'),\
                 ('api','timeout',$1,'private health detail'),\
                 ('database','healthy',$1,'private health detail')",
            )
            .bind(JUNE)
            .execute(control_read.raw())
            .await
            .unwrap();
            let store = PgPlatformExecutive::new(control.clone(), control_read.clone());
            let filters = ExecutiveDateRangeInput {
                from: Some("2026-06-01".into()),
                to: Some("2026-06-30".into()),
            };
            let snapshot = platform_executive_snapshot(&actor(true), &store, Some(&filters), JUNE)
                .await
                .unwrap();
            assert_eq!(snapshot.scope, "platform");
            assert_eq!(snapshot.metrics[0].total, "31");
            assert_eq!(snapshot.metrics[0].rows.len(), 1);
            assert_eq!(snapshot.metrics[1].total, "2");
            assert_eq!(
                snapshot.metrics[1]
                    .rows
                    .iter()
                    .map(|row| row.label.as_str())
                    .collect::<Vec<_>>(),
                ["api · timeout", "jobs · degraded"]
            );
            assert_eq!(snapshot.source_revision.len(), 64);
            let serialized = serde_json::to_string(&snapshot).unwrap();
            for private in [
                "Private Holding Name",
                "private failure reason",
                "PRIVATE_ERROR",
                "private health detail",
                "failure detail",
            ] {
                assert!(!serialized.contains(private), "{private}");
            }
            let request = validate_source_request(
                "failed-provisioning",
                "2026-06",
                "2026-06-01",
                "2026-06-30",
                Some("1"),
            )
            .unwrap();
            let first = platform_executive_sources(&actor(true), &store, &request)
                .await
                .unwrap();
            assert_eq!(first.rows.len(), EXECUTIVE_SOURCE_PAGE_SIZE);
            assert!(first.has_next);
            assert_eq!(first.page, 1);
            let request = validate_source_request(
                "failed-provisioning",
                "2026-06",
                "2026-06-01",
                "2026-06-30",
                Some("2"),
            )
            .unwrap();
            let second = platform_executive_sources(&actor(true), &store, &request)
                .await
                .unwrap();
            assert_eq!(second.rows.len(), 1);
            assert!(!second.has_next);
            let request = validate_source_request(
                "abnormal-platform-services",
                "jobs · degraded",
                "2026-06-01",
                "2026-06-30",
                None,
            )
            .unwrap();
            let health = platform_executive_sources(&actor(true), &store, &request)
                .await
                .unwrap();
            assert_eq!(health.rows.len(), 1);
            assert!(matches!(
                &health.rows[0],
                PlatformExecutiveSourceRow::AbnormalHealth(row)
                    if row.component == "jobs" && row.status == "degraded"
            ));
            assert_eq!(
                platform_executive_snapshot(&actor(false), &store, Some(&filters), JUNE)
                    .await
                    .unwrap_err()
                    .code,
                "PLATFORM_EXECUTIVE_FORBIDDEN"
            );
        })
        .await;
    })
    .await;
}

#[tokio::test]
async fn executive_read_fails_closed_when_the_control_read_source_is_missing() {
    fixture::with_database(3, |control| async move {
        fixture::with_database(1, move |control_read| async move {
            migrate(&control, MigrationTrack::Control).await;
            // Deliberately leave the Control-read track unmigrated.
            let store = PgPlatformExecutive::new(control.clone(), control_read.clone());
            let filters = ExecutiveDateRangeInput {
                from: Some("2026-06-01".into()),
                to: Some("2026-06-30".into()),
            };
            let error = platform_executive_snapshot(&actor(true), &store, Some(&filters), JUNE)
                .await
                .unwrap_err();
            assert_eq!(error.code, "PLATFORM_EXECUTIVE_UNAVAILABLE");
            assert_eq!(error.kind, FailureKind::Unavailable);
        })
        .await;
    })
    .await;
}
