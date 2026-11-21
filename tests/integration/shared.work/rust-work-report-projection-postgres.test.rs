//! Native Work Projection report projection on a disposable PostgreSQL DB.
//!
//! Proves the retained `workReportUnit` mode predicates, counts, money,
//! estimates, stage counts, 50-row paging and CSV ordering run on the caller's
//! connection over an already-authorized scoped row set.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_work_projection::public::work_report_read::{
    WorkReportMode, WorkReportProjection, read_work_report_projection,
};
use serde_json::{Value, json};

fn row(
    id: &str,
    open: bool,
    draft: bool,
    submitted_at: Option<i64>,
    updated_at: i64,
    currency: &str,
    amount: i64,
    estimate: i64,
    units: f64,
    stage: &str,
    stage_wait_days: Option<i64>,
    blocker: Option<&str>,
) -> Value {
    json!({
        "id": id,
        "open": open,
        "draft": draft,
        "submittedAt": submitted_at,
        "updated_at": updated_at,
        "currency": currency,
        "amount": amount,
        "estimate": estimate,
        "units": units,
        "stage": stage,
        "stageWaitDays": stage_wait_days,
        "blocker": blocker,
    })
}

fn rows() -> Vec<Value> {
    vec![
        row(
            "a",
            true,
            false,
            Some(100),
            300,
            "THB",
            1000,
            700,
            2.5,
            "Approve",
            Some(3),
            Some("waiting"),
        ),
        row(
            "b", false, true, None, 200, "THB", 500, 0, 1.0, "Draft", None, None,
        ),
        row(
            "c",
            true,
            false,
            Some(20_000),
            100,
            "USD",
            200,
            0,
            0.5,
            "Approve",
            Some(7),
            None,
        ),
        row(
            "d", false, false, None, 400, "THB", 0, 0, 0.0, "Complete", None, None,
        ),
    ]
}

#[tokio::test]
async fn period_open_and_draft_projections_match_the_retained_queries() {
    fixture::with_database(1, |pool| async move {
        let scoped_rows = rows();
        let scoped = serde_json::to_string(&scoped_rows).unwrap();
        let mut connection = pool.raw().acquire().await.unwrap();
        let connection = &mut *connection;

        let period = WorkReportProjection {
            mode: WorkReportMode::Period,
            scoped: &scoped,
            start: 0,
            end: 10_000,
            page: 1,
        };
        let read = read_work_report_projection(connection, &period, true)
            .await
            .unwrap();
        assert_eq!(read.counts.total, 1);
        assert_eq!(read.counts.open, 2);
        assert_eq!(read.counts.drafts, 1);
        assert_eq!(read.counts.unknown, 1);
        assert_eq!(read.counts.blocked, 1);
        assert_eq!(read.counts.units, 2.5);
        assert_eq!(
            read.money,
            vec![
                erpx_server_work_projection::public::work_report_read::WorkReportCurrencyTotal {
                    currency: "THB".into(),
                    amount: 1000
                }
            ]
        );
        assert_eq!(read.estimates[0].amount, 700);
        assert_eq!(read.stage_counts[0].stage, "Approve");
        assert_eq!(read.stage_counts[0].count, 1);
        assert_eq!(read.stage_counts[0].oldest, Some(3));
        assert_eq!(read.rows.len(), 1);
        assert_eq!(read.rows[0]["id"], "a");
        assert_eq!(read.all_rows.len(), 1);

        let open = WorkReportProjection {
            mode: WorkReportMode::Open,
            scoped: &scoped,
            start: 0,
            end: 0,
            page: 1,
        };
        let read = read_work_report_projection(connection, &open, false)
            .await
            .unwrap();
        assert_eq!(read.counts.total, 2);
        assert_eq!(read.counts.units, 3.0);
        assert_eq!(read.counts.unknown, 1);
        assert_eq!(read.rows.len(), 2);
        assert_eq!(read.rows[0]["id"], "a");
        assert_eq!(read.rows[1]["id"], "c");
        assert!(read.all_rows.is_empty());
        assert_eq!(read.money.len(), 2);

        let drafts = WorkReportProjection {
            mode: WorkReportMode::Drafts,
            scoped: &scoped,
            start: 0,
            end: 0,
            page: 1,
        };
        let read = read_work_report_projection(connection, &drafts, false)
            .await
            .unwrap();
        assert_eq!(read.counts.total, 1);
        assert_eq!(read.rows[0]["id"], "b");
    })
    .await;
}

#[tokio::test]
async fn page_is_capped_at_fifty_and_orders_by_updated_at_then_id() {
    fixture::with_database(1, |pool| async move {
        let scoped_rows = (0..55)
            .map(|index| {
                row(
                    &format!("row-{index:02}"),
                    true,
                    false,
                    Some(100),
                    index,
                    "THB",
                    1,
                    0,
                    1.0,
                    "Approve",
                    Some(0),
                    None,
                )
            })
            .collect::<Vec<_>>();
        let scoped = serde_json::to_string(&scoped_rows).unwrap();
        let mut connection = pool.raw().acquire().await.unwrap();
        let connection = &mut *connection;

        let first = read_work_report_projection(
            connection,
            &WorkReportProjection {
                mode: WorkReportMode::Open,
                scoped: &scoped,
                start: 0,
                end: 0,
                page: 1,
            },
            false,
        )
        .await
        .unwrap();
        assert_eq!(first.rows.len(), 50);
        assert_eq!(first.rows[0]["id"], "row-54");
        assert_eq!(first.rows[49]["id"], "row-05");

        let second = read_work_report_projection(
            connection,
            &WorkReportProjection {
                mode: WorkReportMode::Open,
                scoped: &scoped,
                start: 0,
                end: 0,
                page: 2,
            },
            false,
        )
        .await
        .unwrap();
        assert_eq!(second.rows.len(), 5);
        assert_eq!(second.rows[4]["id"], "row-00");
    })
    .await;
}
