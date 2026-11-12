//! Native People roster Position-exit effects on migrated PostgreSQL.
//!
//! Proves the retained Organization position-save roster effects commit
//! together on one serializable transaction: the Company lock/week version
//! bumps, the released-position and invalidated-request history rows, the
//! future active assignment release, the pending request invalidation and both
//! Collaboration notices (`roster_week`, `roster_request`) with the retained
//! recipients, versions and messages. Past/cancelled/other-engagement
//! assignments and decided/foreign requests stay untouched, another Company
//! selects nothing, and staging without the caller's serializable transaction
//! fails closed. Browser authority and the Organization position save are out of
//! scope.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_collaboration::public::roster_position_notifications::{
    ROSTER_POSITION_REQUEST_MESSAGE, ROSTER_POSITION_WEEK_MESSAGE,
    write_roster_position_exit_request_notices, write_roster_position_exit_week_notices,
};
use erpx_server_people_payroll::public::roster_position_exit::{
    RosterPositionExitInvalidated, RosterPositionExitLocked, RosterPositionExitReleased,
    roster_position_exit_affected, roster_position_exit_plan,
    stage_roster_position_exit_invalidate_requests, stage_roster_position_exit_lock_and_history,
    stage_roster_position_exit_release_and_history,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::{AssertSqlSafe, Connection};

const NOW: i64 = 1_800_000_000_000;
const FUTURE: i64 = NOW + 86_400_000;
const PAST: i64 = NOW - 86_400_000;
const COMMAND: &str = "roster-exit-command-1";
const REASON: &str = "Position closed";

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES ('company','company','company','Company',NULL,'company',NULL,'active');
         INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES ('branch','branch','branch','Branch','company','company',NULL,'active');
         INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES ('department','department','department','Department','branch','company','branch','active');
         INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES ('foreign','company','foreign','Foreign',NULL,'foreign','foreign','active');
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,merged_into_id,created_by,created_at,updated_at) VALUES
           ('p1','u1','company','Alpha','TH','active',NULL,'fixture',0,0),
           ('p2','u2','company','Beta','TH','active',NULL,'fixture',0,0),
           ('p3','u3','foreign','Gamma','TH','active',NULL,'fixture',0,0);
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,end_date,job_title,currency,created_by,created_at,updated_at) VALUES
           ('e1','p1','company','branch','department','employee','salaried','active','2020-01-01',NULL,'Engineer','THB','fixture',0,0),
           ('e2','p2','company','branch','department','employee','salaried','active','2020-01-01',NULL,'Engineer','THB','fixture',0,0),
           ('e3','p3','foreign','foreign-branch','foreign-department','employee','salaried','active','2020-01-01',NULL,'Engineer','THB','fixture',0,0);
         INSERT INTO people_roster_locks(company_id,version) VALUES ('company',1);
         INSERT INTO people_roster_weeks(id,company_id,branch_id,department_id,week_start,timezone,state,version,created_by,published_by,created_at,updated_at) VALUES
           ('w-future','company','branch','department','2027-01-04','Asia/Bangkok','published',3,'fixture','fixture',100,100),
           ('w-past','company','branch','department','2020-01-06','Asia/Bangkok','published',1,'fixture','fixture',100,100);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::raw_sql(AssertSqlSafe(format!(
        "INSERT INTO people_roster_shifts(id,week_id,template_id,name,local_date,start_time,end_time,starts_at,ends_at,capacity,state,version) VALUES
           ('s-future','w-future',NULL,'Future','2027-01-04','06:00','14:00',{FUTURE},{},2,'active',1),
           ('s-cancelled','w-future',NULL,'Cancelled','2027-01-05','06:00','14:00',{FUTURE},{},2,'cancelled',1),
           ('s-past','w-past',NULL,'Past','2020-01-06','06:00','14:00',{PAST},{},2,'active',1);
         INSERT INTO people_roster_assignments(id,shift_id,person_id,engagement_id,state,version) VALUES
           ('a-future','s-future','p1','e1','assigned',1),
           ('a-cancelled','s-cancelled','p1','e1','assigned',1),
           ('a-past','s-past','p1','e1','assigned',1),
           ('a-foreign','s-future','p3','e3','assigned',1);
         INSERT INTO people_roster_requests(id,company_id,department_id,kind,requester_person_id,coworker_person_id,source_assignment_id,source_version,target_shift_id,target_assignment_id,target_version,state,reason,decision_note,version,created_by,created_at,updated_at) VALUES
           ('r-source','company','department','move','p1',NULL,'a-future',1,'s-past',NULL,NULL,'pending_manager','Prefer the earlier shift','',1,'fixture',100,100),
           ('r-coworker','company','department','swap','p2','p1','a-past',1,'s-future','a-foreign',1,'pending_coworker','Swap shifts','',1,'fixture',100,100),
           ('r-approved','company','department','extra','p1',NULL,NULL,NULL,'s-future',NULL,NULL,'approved','Cover the shift','Approved',2,'fixture',100,100),
           ('r-foreign','company','department','extra','p3',NULL,NULL,NULL,'s-future',NULL,NULL,'pending_manager','Foreign request','',1,'fixture',100,100)",
        FUTURE + 3_600_000,
        FUTURE + 3_600_000,
        PAST + 3_600_000,
    )))
    .execute(pool.raw())
    .await
    .unwrap();
}

#[derive(Debug)]
struct Outcome {
    affected: Vec<String>,
    locked: RosterPositionExitLocked,
    week_notices: u64,
    duplicate_week_notices: u64,
    released: RosterPositionExitReleased,
    request_notices: u64,
    invalidated: RosterPositionExitInvalidated,
}

#[tokio::test]
async fn position_exit_stages_the_retained_roster_effects_and_notices_together() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let uow = PgUnitOfWork::new(pool.clone());
        let outcome = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    let plan = roster_position_exit_plan(
                        &company(),
                        "owner",
                        &["e1".to_owned()],
                        COMMAND,
                        REASON,
                        TimestampMillis::new(NOW),
                    )
                    .unwrap()
                    .unwrap();
                    let affected = roster_position_exit_affected(c, &plan).await?;
                    // Retained order: people phases and notices interleaved.
                    let locked = stage_roster_position_exit_lock_and_history(c, &plan).await?;
                    let week_notices = write_roster_position_exit_week_notices(
                        c,
                        &plan.company_id,
                        &plan.engagement_ids,
                        plan.now.get(),
                    )
                    .await?;
                    let duplicate_week_notices = write_roster_position_exit_week_notices(
                        c,
                        &plan.company_id,
                        &plan.engagement_ids,
                        plan.now.get(),
                    )
                    .await?;
                    let released = stage_roster_position_exit_release_and_history(c, &plan).await?;
                    let request_notices = write_roster_position_exit_request_notices(
                        c,
                        &plan.company_id,
                        &plan.engagement_ids,
                        plan.now.get(),
                    )
                    .await?;
                    let invalidated =
                        stage_roster_position_exit_invalidate_requests(c, &plan).await?;
                    Ok(Outcome {
                        affected: affected
                            .into_iter()
                            .map(|row| row.assignment_id)
                            .collect(),
                        locked,
                        week_notices,
                        duplicate_week_notices,
                        released,
                        request_notices,
                        invalidated,
                    })
                })
            })
            .await
            .unwrap();
        assert_eq!(
            outcome.affected,
            vec!["a-future".to_owned()],
            "only the future active assignment of the exiting engagement is affected"
        );
        assert_eq!(
            outcome.locked,
            RosterPositionExitLocked {
                locks: 1,
                weeks: 1,
                history: 1,
            }
        );
        assert_eq!((outcome.week_notices, outcome.duplicate_week_notices), (1, 0));
        assert_eq!(
            outcome.released,
            RosterPositionExitReleased {
                assignments: 1,
                history: 2,
            }
        );
        assert_eq!(outcome.request_notices, 3);
        assert_eq!(
            outcome.invalidated,
            RosterPositionExitInvalidated { requests: 2 }
        );

        // Roster lock/week versions and the exact week timestamp.
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT version FROM people_roster_locks WHERE company_id='company'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_as::<_, (i32, i64)>(
                "SELECT version,updated_at FROM people_roster_weeks WHERE id='w-future'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            (4, NOW)
        );
        assert_eq!(
            sqlx::query_as::<_, (i32, i64)>(
                "SELECT version,updated_at FROM people_roster_weeks WHERE id='w-past'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            (1, 100),
            "a shift that already started is never part of a Position exit"
        );

        // Exactly the future active assignment is released.
        for (assignment, state, version) in [
            ("a-future", "released", 2),
            ("a-cancelled", "assigned", 1),
            ("a-past", "assigned", 1),
            ("a-foreign", "assigned", 1),
        ] {
            assert_eq!(
                sqlx::query_as::<_, (String, i32)>(
                    "SELECT state,version FROM people_roster_assignments WHERE id=$1"
                )
                .bind(assignment)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
                (state.to_owned(), version),
                "{assignment}"
            );
        }

        // Both retained history rows, ids and evidence payloads.
        assert_eq!(
            sqlx::query_as::<_, (String, String, String, String, String, String)>(
                "SELECT id,company_id,department_id,week_id,action,actor_id FROM people_roster_history WHERE record_id='a-future'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            (
                format!("{COMMAND}-release-a-future"),
                "company".into(),
                "department".into(),
                "w-future".into(),
                "assignment.released.position".into(),
                "owner".into(),
            )
        );
        assert_eq!(
            sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT evidence_json::jsonb FROM people_roster_history WHERE record_id='a-future'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            serde_json::json!({"engagementId": "e1"})
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM people_roster_history WHERE action='request.invalidated.position' AND request_id IS NOT NULL"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT evidence_json::jsonb FROM people_roster_history WHERE record_id='r-source'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            serde_json::json!({"positionChange": true})
        );

        // Pending requests invalidated with the Position reason; decided and
        // foreign requests retain their state.
        for (request, state, version) in [
            ("r-source", "invalidated", 2),
            ("r-coworker", "invalidated", 2),
            ("r-approved", "approved", 2),
            ("r-foreign", "pending_manager", 1),
        ] {
            assert_eq!(
                sqlx::query_as::<_, (String, i32)>(
                    "SELECT state,version FROM people_roster_requests WHERE id=$1"
                )
                .bind(request)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
                (state.to_owned(), version),
                "{request}"
            );
        }
        assert_eq!(
            sqlx::query_as::<_, (String, i32, i64)>(
                "SELECT decision_note,version,updated_at FROM people_roster_requests WHERE id='r-source'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            (REASON.to_owned(), 2, NOW)
        );

        // One bumped-week notice, then three request notices for u1/u2/u1.
        assert_eq!(
            sqlx::query_as::<_, (String, String, i64, String, i64)>(
                "SELECT user_id,record_id,record_version,message,created_at FROM staff_notifications WHERE record_type='roster_week'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            (
                "u1".into(),
                "w-future".into(),
                4,
                ROSTER_POSITION_WEEK_MESSAGE.into(),
                NOW,
            )
        );
        assert_eq!(
            sqlx::query_as::<_, (String, String, i64)>(
                "SELECT user_id,record_id,record_version FROM staff_notifications WHERE record_type='roster_request' ORDER BY user_id,record_id"
            )
            .fetch_all(pool.raw())
            .await
            .unwrap(),
            vec![
                ("u1".to_owned(), "r-coworker".to_owned(), 2),
                ("u1".to_owned(), "r-source".to_owned(), 2),
                ("u2".to_owned(), "r-coworker".to_owned(), 2),
            ]
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT DISTINCT message FROM staff_notifications WHERE record_type='roster_request'"
            )
            .fetch_all(pool.raw())
            .await
            .unwrap(),
            vec![ROSTER_POSITION_REQUEST_MESSAGE.to_owned()]
        );

        // Another Company selects nothing even for the same engagement text.
        let foreign = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    let plan = roster_position_exit_plan(
                        &CompanyId::new("foreign").unwrap(),
                        "owner",
                        &["e1".to_owned()],
                        "roster-exit-command-2",
                        REASON,
                        TimestampMillis::new(NOW),
                    )
                    .unwrap()
                    .unwrap();
                    let locked = stage_roster_position_exit_lock_and_history(c, &plan).await?;
                    let released = stage_roster_position_exit_release_and_history(c, &plan).await?;
                    let invalidated =
                        stage_roster_position_exit_invalidate_requests(c, &plan).await?;
                    let week = write_roster_position_exit_week_notices(
                        c,
                        &plan.company_id,
                        &plan.engagement_ids,
                        plan.now.get(),
                    )
                    .await?;
                    let request = write_roster_position_exit_request_notices(
                        c,
                        &plan.company_id,
                        &plan.engagement_ids,
                        plan.now.get(),
                    )
                    .await?;
                    Ok((locked, released, invalidated, week, request))
                })
            })
            .await
            .unwrap();
        assert_eq!(
            foreign.0,
            RosterPositionExitLocked {
                locks: 0,
                weeks: 0,
                history: 0,
            }
        );
        assert_eq!(
            foreign.1,
            RosterPositionExitReleased {
                assignments: 0,
                history: 0,
            }
        );
        assert_eq!(
            foreign.2,
            RosterPositionExitInvalidated { requests: 0 }
        );
        assert_eq!((foreign.3, foreign.4), (0, 0));
    })
    .await;
}

#[tokio::test]
async fn position_exit_requires_the_callers_serializable_transaction() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let plan = roster_position_exit_plan(
            &company(),
            "owner",
            &["e1".to_owned()],
            COMMAND,
            REASON,
            TimestampMillis::new(NOW),
        )
        .unwrap()
        .unwrap();
        let mut connection = pool.raw().acquire().await.unwrap();
        let errors = [
            stage_roster_position_exit_lock_and_history(&mut connection, &plan)
                .await
                .map(|_| ())
                .unwrap_err(),
            stage_roster_position_exit_release_and_history(&mut connection, &plan)
                .await
                .map(|_| ())
                .unwrap_err(),
            stage_roster_position_exit_invalidate_requests(&mut connection, &plan)
                .await
                .map(|_| ())
                .unwrap_err(),
            write_roster_position_exit_week_notices(
                &mut connection,
                &plan.company_id,
                &plan.engagement_ids,
                plan.now.get(),
            )
            .await
            .map(|_| ())
            .unwrap_err(),
            write_roster_position_exit_request_notices(
                &mut connection,
                &plan.company_id,
                &plan.engagement_ids,
                plan.now.get(),
            )
            .await
            .map(|_| ())
            .unwrap_err(),
        ];
        for error in errors {
            let PgTransactionError::Persistence(error) = error else {
                panic!("expected a persistence precondition");
            };
            assert!(matches!(
                error.code,
                "ROSTER_POSITION_EXIT_TRANSACTION_REQUIRED"
                    | "ROSTER_POSITION_NOTICE_TRANSACTION_REQUIRED"
            ));
        }
        // A plain READ COMMITTED transaction is not the retained owner unit.
        let mut transaction = connection.begin().await.unwrap();
        let error = stage_roster_position_exit_lock_and_history(&mut transaction, &plan)
            .await
            .map(|_| ())
            .unwrap_err();
        let PgTransactionError::Persistence(error) = error else {
            panic!("expected a persistence precondition");
        };
        assert_eq!(error.code, "ROSTER_POSITION_EXIT_SERIALIZABLE_REQUIRED");
        transaction.rollback().await.unwrap();
    })
    .await;
}

#[test]
fn empty_engagement_planning_is_still_a_no_op() {
    assert!(
        roster_position_exit_plan(
            &company(),
            "owner",
            &[],
            COMMAND,
            REASON,
            TimestampMillis::new(NOW)
        )
        .unwrap()
        .is_none()
    );
}
