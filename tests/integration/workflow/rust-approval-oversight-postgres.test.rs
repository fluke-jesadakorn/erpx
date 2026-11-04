//! Real migrated Holding PostgreSQL for the Workflow approval oversight read.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{AssignmentId, BusinessError, CompanyId, HoldingId, UserId};
use erpx_server_workflow::public::approval_oversight::{
    ApprovalEvidenceQuery, ApprovalOversightActor, PgApprovalOversight, approval_evidence,
    approval_queue,
};
use sqlx::Connection;

fn actor(company: &str) -> ApprovalOversightActor {
    ApprovalOversightActor {
        user_id: UserId::new("actor").unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: AssignmentId::new("assignment-actor").unwrap(),
        scope_kind: "holding".into(),
        permissions: vec!["organization.manage".into()],
    }
}
fn query() -> ApprovalEvidenceQuery {
    ApprovalEvidenceQuery::new("expense", "expense-1").unwrap()
}
async fn setup(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
}
async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO approval_instances(id,company_id,branch_id,department_id,record_type,record_id,phase,cycle,requester_user_id,assignee_user_id,state,current_step,blocked_reason,version,created_at,updated_at) VALUES \
         ('approval-1','company',NULL,NULL,'expense','expense-1','request',1,'requester',NULL,'pending',0,NULL,2,100,200), \
         ('approval-2','company',NULL,NULL,'leave','leave-1','request',1,'requester',NULL,'blocked',0,'No eligible reviewer',1,90,190), \
         ('approval-3','other',NULL,NULL,'expense','expense-9','request',1,'requester',NULL,'pending',0,NULL,1,80,180); \
         INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority,state) VALUES \
         ('step-0','approval-1',0,'approver','named','pending'), \
         ('step-1','approval-1',1,'manager','department','pending'); \
         INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,previous_approver_user_id,approver_user_id,reason,reviewed_version,command_id,created_at) VALUES \
         ('decision-1','approval-1',NULL,'initialize','system',NULL,NULL,'Initialize route',1,'approval:initialize:1',110); \
         INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES \
         ('approval:initialize:1','approval.initialize','system',repeat('a',64),'expense-1','{\"state\":\"pending\"}',110); \
         INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES \
         ('audit-1','approval:initialize:1','system','assignment-system','approval.initialize','expense_approval','expense-1',1,'approval.initialize',110); \
         INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES \
         ('outbox-1','approval:initialize:1','approval.initialize','expense_approval','expense-1',1,'{\"state\":\"pending\"}',110);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}
async fn counts(pool: &DatabasePool) -> (i64, i64, i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM approval_instances),(SELECT count(*) FROM approval_steps),\
         (SELECT count(*) FROM approval_decisions),(SELECT count(*) FROM command_receipts),\
         (SELECT count(*) FROM audit_events),(SELECT count(*) FROM outbox_events)",
    )
    .fetch_one(pool.raw())
    .await
    .unwrap()
}

#[tokio::test]
async fn queue_and_evidence_are_company_scoped_and_read_back() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        seed(&pool).await;
        let before = counts(&pool).await;
        PgUnitOfWork::new(pool.clone())
            .run(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    let mut repository = PgApprovalOversight::new(c);
                    let queue = approval_queue(&mut repository, &actor("company"), 500)
                        .await
                        .unwrap();
                    assert_eq!(queue.evaluated_at, 500);
                    assert_eq!(queue.approvals.len(), 2);
                    assert_eq!(queue.approvals[0].record_id, "expense-1");
                    assert_eq!(queue.approvals[1].record_id, "leave-1");
                    assert_eq!(
                        queue.approvals[1].blocked_reason.as_deref(),
                        Some("No eligible reviewer")
                    );

                    let value =
                        approval_evidence(&mut repository, &actor("company"), &query(), 500)
                            .await
                            .unwrap()
                            .unwrap();
                    assert_eq!(value["approval"]["instance"]["id"], "approval-1");
                    assert_eq!(value["approval"]["steps"].as_array().unwrap().len(), 2);
                    assert_eq!(value["approval"]["history"].as_array().unwrap().len(), 1);
                    assert_eq!(value["approval"]["receipts"].as_array().unwrap().len(), 1);
                    assert_eq!(value["approval"]["audit"].as_array().unwrap().len(), 1);
                    assert_eq!(value["approval"]["outbox"].as_array().unwrap().len(), 1);
                    assert_eq!(
                        value["approval"]["receipts"][0]["command_id"],
                        "approval:initialize:1"
                    );

                    let foreign =
                        approval_evidence(&mut repository, &actor("other"), &query(), 500)
                            .await
                            .unwrap();
                    assert!(foreign.is_none());
                    let missing = approval_evidence(
                        &mut repository,
                        &actor("company"),
                        &ApprovalEvidenceQuery::new("leave", "expense-1").unwrap(),
                        500,
                    )
                    .await
                    .unwrap();
                    assert!(missing.is_none());
                    Ok::<_, PgTransactionError<BusinessError>>(())
                })
            })
            .await
            .unwrap();
        assert_eq!(counts(&pool).await, before);
    })
    .await;
}

#[tokio::test]
async fn transaction_and_serializable_guards_precede_source_reads() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        {
            let mut repository = PgApprovalOversight::new(&mut connection);
            let error = approval_queue(&mut repository, &actor("company"), 500)
                .await
                .unwrap_err();
            match error {
                PgTransactionError::Persistence(error) => {
                    assert_eq!(error.code, "APPROVAL_EVIDENCE_TRANSACTION_REQUIRED")
                }
                PgTransactionError::Business(_) => panic!("unexpected business error"),
            }
        }

        let mut transaction = connection
            .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
            .await
            .unwrap();
        {
            let mut repository = PgApprovalOversight::new(&mut transaction);
            let error = approval_queue(&mut repository, &actor("company"), 500)
                .await
                .unwrap_err();
            match error {
                PgTransactionError::Persistence(error) => {
                    assert_eq!(error.code, "APPROVAL_EVIDENCE_SERIALIZABLE_REQUIRED")
                }
                PgTransactionError::Business(_) => panic!("unexpected business error"),
            }
        }
        transaction.rollback().await.unwrap();
        connection.close().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn uncommitted_changes_roll_back_and_committed_changes_read_back() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed(&pool).await;
        let before = counts(&pool).await;
        PgUnitOfWork::new(pool.clone())
            .run(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority,state) \
                         VALUES('step-uncommitted','approval-1',2,'approver-2','named','pending')",
                    )
                    .execute(&mut *c)
                    .await?;
                    let mut repository = PgApprovalOversight::new(c);
                    let value = approval_evidence(
                        &mut repository,
                        &actor("company"),
                        &query(),
                        500,
                    )
                    .await
                    .unwrap()
                    .unwrap();
                    assert_eq!(value["approval"]["steps"].as_array().unwrap().len(), 3);
                    Err::<(), PgTransactionError<BusinessError>>(
                        BusinessError::invalid("ROLLBACK", "Rollback the synthetic unit.").into(),
                    )
                })
            })
            .await
            .unwrap_err();
        assert_eq!(counts(&pool).await, before);

        PgUnitOfWork::new(pool.clone())
            .run(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority,state) \
                         VALUES('step-committed','approval-1',2,'approver-2','named','pending')",
                    )
                    .execute(&mut *c)
                    .await?;
                    Ok::<_, PgTransactionError<BusinessError>>(())
                })
            })
            .await
            .unwrap();
        PgUnitOfWork::new(pool.clone())
            .run(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    let mut repository = PgApprovalOversight::new(c);
                    let value = approval_evidence(
                        &mut repository,
                        &actor("company"),
                        &query(),
                        500,
                    )
                    .await
                    .unwrap()
                    .unwrap();
                    assert_eq!(value["approval"]["steps"].as_array().unwrap().len(), 3);
                    assert_eq!(value["approval"]["steps"][2]["id"], "step-committed");
                    Ok::<_, PgTransactionError<BusinessError>>(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn foreign_company_rows_are_never_visible() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed(&pool).await;
        PgUnitOfWork::new(pool.clone())
            .run(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    let mut repository = PgApprovalOversight::new(c);
                    let queue = approval_queue(&mut repository, &actor("other"), 500)
                        .await
                        .unwrap();
                    assert_eq!(queue.approvals.len(), 1);
                    assert_eq!(queue.approvals[0].record_id, "expense-9");
                    let evidence = approval_evidence(
                        &mut repository,
                        &actor("other"),
                        &ApprovalEvidenceQuery::new("expense", "expense-9").unwrap(),
                        500,
                    )
                    .await
                    .unwrap()
                    .unwrap();
                    assert_eq!(evidence["approval"]["instance"]["company_id"], "other");
                    Ok::<_, PgTransactionError<BusinessError>>(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}
