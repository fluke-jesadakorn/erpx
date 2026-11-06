//! Native Records employee-request purchase-source witness over migrated
//! PostgreSQL. These prove the Records-owned guarded row, Company/scope
//! isolation and writer-first freshness for Procurement consumers. The
//! witness grants no `commerce.purchase` authority, reads no allocation
//! or order-link facts and claims no owner completion.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_records::public::employee_request_source::{
    EmployeeRequestScope, PgEmployeeRequestSource, lock_employee_request_sources,
    prepare_order_handoff_source, prepare_quotation_source,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};

fn holding_scope(company: &str) -> EmployeeRequestScope {
    EmployeeRequestScope {
        company_id: company.into(),
        branch_id: None,
        department_id: None,
    }
}

fn business(error: PgTransactionError<erpx_server_shared::public::BusinessError>) -> &'static str {
    match error {
        PgTransactionError::Business(error) => {
            assert!(
                error.kind == erpx_server_shared::public::FailureKind::NotFound
                    || error.kind == erpx_server_shared::public::FailureKind::Forbidden
                    || error.kind == erpx_server_shared::public::FailureKind::Conflict
                    || error.kind == erpx_server_shared::public::FailureKind::InvalidInput,
                "unexpected error kind {:?}",
                error.kind
            );
            error.code
        }
        other => panic!("expected an owner business failure, got {other:?}"),
    }
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
        "INSERT INTO employee_requests(id,company_id,branch_id,department_id,kind,category,title,description,location,impact,needed_date,items_json,currency,requester_user_id,created_by,parent_request_id,state,team_id,assignee_user_id,routing_json,approval_json,blocked_reason,resolution,purchase_order_id,handover_json,version,evidence_version,created_at,updated_at) VALUES \
         ('request-quoted','company','branch-1',NULL,'purchase','purchasing','Cables','Stock up','Warehouse','normal',NULL,'[{\"description\":\"Cable\",\"quantity\":\"10\"}]','THB','requester','creator',NULL,'in_progress',NULL,'handler','{}','{\"decision\":\"approved\"}',NULL,'',NULL,'{}',3,2,100,500),\
         ('request-unassigned','company',NULL,NULL,'purchase','purchasing','Bolts','Stock up','Warehouse','normal',NULL,'[]','THB','requester','creator',NULL,'queued',NULL,NULL,'{}','{\"decision\":\"approved\"}',NULL,'',NULL,'{}',1,1,100,500),\
         ('request-draft','company',NULL,NULL,'purchase','purchasing','Draft','New','Warehouse','normal',NULL,'[]','THB','requester','creator',NULL,'draft',NULL,'handler','{}','{\"decision\":\"pending\"}',NULL,'',NULL,'{}',1,1,100,500),\
         ('request-service','company',NULL,NULL,'service','it','Help','Fix','Office','normal',NULL,'[]','THB','requester','creator',NULL,'in_progress',NULL,'handler','{}','{\"decision\":\"approved\"}',NULL,'',NULL,'{}',2,1,100,500),\
         ('request-foreign','foreign',NULL,NULL,'purchase','purchasing','Foreign','Outside','Warehouse','normal',NULL,'[]','THB','requester','creator',NULL,'in_progress',NULL,'handler','{}','{\"decision\":\"approved\"}',NULL,'',NULL,'{}',1,1,100,500);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn quotation(
    pool: &DatabasePool,
    scope: &EmployeeRequestScope,
    actor: &str,
    id: &str,
) -> Result<serde_json::Value, PgTransactionError<erpx_server_shared::public::BusinessError>> {
    let mut tx = pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await
        .unwrap();
    lock_employee_request_sources(&mut tx).await?;
    let mut source = PgEmployeeRequestSource::new(&mut tx);
    let prepared = prepare_quotation_source(&mut source, scope, actor, id)
        .await
        .map_err(|error| match error {
            PgTransactionError::Business(error) => PgTransactionError::Business(error),
            other => other,
        });
    match prepared {
        Ok(prepared) => {
            let value = serde_json::json!({
                "id": prepared.request.id,
                "kind": prepared.request.kind,
                "state": prepared.request.state,
                "version": prepared.request.version,
                "evidence_version": prepared.request.evidence_version,
                "branch_id": prepared.request.branch_id,
                "title": prepared.request.title,
            });
            prepared.require_current(&mut source).await?;
            tx.commit().await.unwrap();
            Ok(value)
        }
        Err(error) => {
            tx.rollback().await.unwrap();
            Err(error)
        }
    }
}

#[tokio::test]
async fn quotation_source_reads_the_guarded_purchase_row() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let value = quotation(
            &pool,
            &holding_scope("company"),
            "handler",
            "request-quoted",
        )
        .await
        .unwrap();
        assert_eq!(value["id"], "request-quoted");
        assert_eq!(value["kind"], "purchase");
        assert_eq!(value["state"], "in_progress");
        assert_eq!(value["version"], 3);
        assert_eq!(value["evidence_version"], 2);
        assert_eq!(value["branch_id"], "branch-1");
        assert_eq!(value["title"], "Cables");
    })
    .await;
}

#[tokio::test]
async fn foreign_out_of_scope_and_non_quotation_rows_are_denied() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        // Foreign company: the row is invisible, not reported present.
        assert_eq!(
            business(
                quotation(
                    &pool,
                    &holding_scope("foreign"),
                    "handler",
                    "request-quoted"
                )
                .await
                .unwrap_err()
            ),
            "REQUEST_NOT_FOUND"
        );
        // Branch-containing row is outside a different branch selection.
        let branch = EmployeeRequestScope {
            company_id: "company".into(),
            branch_id: Some("branch-2".into()),
            department_id: None,
        };
        assert_eq!(
            business(
                quotation(&pool, &branch, "handler", "request-quoted")
                    .await
                    .unwrap_err()
            ),
            "REQUEST_NOT_FOUND"
        );
        // Missing id.
        assert_eq!(
            business(
                quotation(
                    &pool,
                    &holding_scope("company"),
                    "handler",
                    "request-missing"
                )
                .await
                .unwrap_err()
            ),
            "REQUEST_NOT_FOUND"
        );
        // Wrong assignee.
        assert_eq!(
            business(
                quotation(&pool, &holding_scope("company"), "other", "request-quoted")
                    .await
                    .unwrap_err()
            ),
            "PURCHASE_REQUEST_REQUIRED"
        );
        // Service request is never a purchase quotation source.
        assert_eq!(
            business(
                quotation(
                    &pool,
                    &holding_scope("company"),
                    "handler",
                    "request-service"
                )
                .await
                .unwrap_err()
            ),
            "PURCHASE_REQUEST_REQUIRED"
        );
        // Draft purchase request is not yet approved.
        assert_eq!(
            business(
                quotation(&pool, &holding_scope("company"), "handler", "request-draft")
                    .await
                    .unwrap_err()
            ),
            "PURCHASE_REQUEST_REQUIRED"
        );
    })
    .await;
}

#[tokio::test]
async fn order_handoff_source_allows_unassigned_approved_purchase() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let mut source = PgEmployeeRequestSource::new(&mut tx);
        let prepared = prepare_order_handoff_source(
            &mut source,
            &holding_scope("company"),
            "handler",
            "request-unassigned",
            false,
        )
        .await
        .unwrap();
        assert_eq!(prepared.request.id, "request-unassigned");
        prepared.require_current(&mut source).await.unwrap();
        tx.commit().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn guard_detects_version_and_items_drift_in_the_same_unit() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        // Version drift: prepare, bump the row in the same unit, revalidate.
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let prepared = {
            let mut source = PgEmployeeRequestSource::new(&mut tx);
            prepare_quotation_source(&mut source, &holding_scope("company"), "handler", "request-quoted")
                .await
                .unwrap()
        };
        sqlx::query("UPDATE employee_requests SET version=version+1, updated_at=600 WHERE id='request-quoted'")
            .execute(&mut *tx)
            .await
            .unwrap();
        {
            let mut source = PgEmployeeRequestSource::new(&mut tx);
            assert_eq!(
                business(prepared.require_current(&mut source).await.unwrap_err()),
                "VERSION_CONFLICT"
            );
        }
        tx.rollback().await.unwrap();
        // Items drift: same unit, only items_json changes.
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let prepared = {
            let mut source = PgEmployeeRequestSource::new(&mut tx);
            prepare_quotation_source(&mut source, &holding_scope("company"), "handler", "request-quoted")
                .await
                .unwrap()
        };
        sqlx::query("UPDATE employee_requests SET items_json='[]', updated_at=600 WHERE id='request-quoted'")
            .execute(&mut *tx)
            .await
            .unwrap();
        {
            let mut source = PgEmployeeRequestSource::new(&mut tx);
            assert_eq!(
                business(prepared.require_current(&mut source).await.unwrap_err()),
                "VERSION_CONFLICT"
            );
        }
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn writer_first_reader_sees_the_fresh_committed_row() {
    use sqlx::Connection;
    fixture::with_database(6, |pool| async move {
        setup(&pool).await;
        // Writer holds an uncommitted row update.
        let mut writer = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let writer_pid: i32 =
            sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        sqlx::query("UPDATE employee_requests SET version=version+1, updated_at=600 WHERE id='request-quoted'")
            .execute(&mut *writer)
            .await
            .unwrap();
        let other_pool = pool.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        let reader_task = tokio::spawn(async move {
            // Own connection, pid before BEGIN so the guarded FOR SHARE
            // read is the first transactional statement.
            let mut connection = other_pool.raw().acquire().await.unwrap();
            let pid: i32 =
                sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *connection).await.unwrap();
            let mut reader = connection.begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *reader).await.unwrap();
            send.send(pid).unwrap();
            // Table lock first: SHARE ROW EXCLUSIVE conflicts with the
            // writer's ROW EXCLUSIVE, so the lock queues behind the
            // writer and the snapshot starts after the writer commits.
            lock_employee_request_sources(&mut reader).await.unwrap();
            let mut source = PgEmployeeRequestSource::new(&mut reader);
            prepare_quotation_source(&mut source, &holding_scope("company"), "handler", "request-quoted")
                .await
                .map(|prepared| prepared.request.version)
        });
        let reader_pid = receive.await.unwrap();
        let mut blocked = false;
        for _ in 0..200 {
            blocked = sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))")
                .bind(writer_pid)
                .bind(reader_pid)
                .fetch_one(pool.raw())
                .await
                .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(blocked, "guarded read must wait on the writer before its snapshot");
        writer.commit().await.unwrap();
        let version = reader_task
            .await
            .unwrap()
            .expect("reader sees the committed row");
        assert_eq!(version, 4);
    })
    .await;
}
