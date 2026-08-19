//! Disposable acceptance evidence for the HR write-side outbox envelope.
//!
//! This test uses a real holding-write PostgreSQL database. It proves that an
//! HR source mutation and its complete trace/holding envelope are committed
//! together, and that an idempotent replay does not append a second event.

use chrono::NaiveDate;
use chrono::Utc;
use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext};
use erpx_domain::hr::{
    EmployeeRecord, EmploymentScope, EmploymentStatus, ExpenseClaim, ExpenseStatus, LeaveRequest,
    LeaveStatus, PayrollLine, PayrollRun, PayrollStatus,
};
use erpx_infrastructure::SqlxHrPersistence;
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires a disposable holding-write PostgreSQL database"]
async fn live_hr_persistence_writes_complete_outbox_envelope_atomically() {
    let write_url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding write migration");

    let data_profile_key = format!("holding-hr-envelope-{}", Uuid::new_v4());
    let employee = EmployeeRecord {
        id: Uuid::new_v4(),
        employee_ref: "EMP-ENVELOPE-001".into(),
        display_name: "Envelope Acceptance Employee".into(),
        scope: EmploymentScope::new(Uuid::new_v4(), Uuid::new_v4(), Some(Uuid::new_v4()))
            .expect("scope"),
        status: EmploymentStatus::Active,
        hired_on: NaiveDate::from_ymd_opt(2026, 1, 2).expect("date"),
        terminated_on: None,
        version: 1,
        last_command_id: Uuid::new_v4(),
        last_idempotency_key: "hr-envelope-employee-1".into(),
    };

    let write = SqlxHrPersistence::new(write_pool.clone(), &data_profile_key).expect("write store");
    let first = write
        .save_employee(&employee, None)
        .await
        .expect("first employee write");
    let replay = write
        .save_employee(&employee, None)
        .await
        .expect("idempotent employee replay");
    assert!(!first.replayed);
    assert!(replay.replayed);

    let mut tx = write_pool.begin().await.expect("inspection transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    let row = sqlx::query(
        "SELECT contract_version, event_version, active_scope, actor,
                correlation_id, causation_id, idempotency_key, payload
         FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_id=$2",
    )
    .bind(&data_profile_key)
    .bind(employee.id)
    .fetch_one(&mut *tx)
    .await
    .expect("HR outbox envelope");
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_id=$2",
    )
    .bind(&data_profile_key)
    .bind(employee.id)
    .fetch_one(&mut *tx)
    .await
    .expect("HR outbox count");

    assert_eq!(
        row.try_get::<Option<i32>, _>("contract_version").unwrap(),
        Some(1)
    );
    assert_eq!(
        row.try_get::<Option<i32>, _>("event_version").unwrap(),
        Some(1)
    );
    assert_eq!(
        row.try_get::<Option<Value>, _>("active_scope").unwrap(),
        Some(serde_json::to_value(employee.scope.active_scope()).expect("holding scope JSON"))
    );
    assert_eq!(row.try_get::<Option<Value>, _>("actor").unwrap(), None);
    assert_eq!(
        row.try_get::<Option<Uuid>, _>("correlation_id").unwrap(),
        Some(employee.last_command_id)
    );
    assert_eq!(
        row.try_get::<Option<Uuid>, _>("causation_id").unwrap(),
        None
    );
    assert_eq!(
        row.try_get::<Option<String>, _>("idempotency_key").unwrap(),
        Some(employee.last_idempotency_key.clone())
    );
    assert_eq!(
        row.try_get::<Value, _>("payload")
            .unwrap()
            .get("version")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(event_count, 1);
    tx.commit().await.expect("inspection transaction commit");
}

#[tokio::test]
#[ignore = "requires a disposable holding-write PostgreSQL database"]
async fn live_hr_employee_transition_projects_authoritative_source_and_outbox() {
    let write_url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding write migration");
    let data_profile_key = format!("holding-hr-transition-{}", Uuid::new_v4());
    let scope = EmploymentScope::new(Uuid::new_v4(), Uuid::new_v4(), None).expect("scope");
    let employee = EmployeeRecord {
        id: Uuid::new_v4(),
        employee_ref: "EMP-TRANSITION-001".into(),
        display_name: "Transition Acceptance Employee".into(),
        scope: scope.clone(),
        status: EmploymentStatus::Active,
        hired_on: NaiveDate::from_ymd_opt(2026, 1, 2).expect("date"),
        terminated_on: None,
        version: 1,
        last_command_id: Uuid::new_v4(),
        last_idempotency_key: "hr-transition-create".into(),
    };
    let write = SqlxHrPersistence::new(write_pool.clone(), &data_profile_key).expect("write store");
    write
        .save_employee(&employee, None)
        .await
        .expect("employee write");
    let mut context = CommandContext::try_new(
        "hr-transition-suspend",
        ActorAttribution {
            principal_ref: "hr-transition-actor".into(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope.active_scope(),
        "hr.employee.transition",
    )
    .expect("command context");
    context.expected_aggregate_version = Some(1);
    let receipt = write
        .transition_employee(employee.id, "suspended", None, 1, &context)
        .await
        .expect("employee transition");
    assert_eq!(receipt.version, 2);
    assert!(
        write
            .transition_employee(employee.id, "active", None, 1, &context)
            .await
            .is_err()
    );
    let mut tx = write_pool.begin().await.expect("inspection transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    let row = sqlx::query(
        "SELECT status, version FROM erpx_holding.hr_employee_records
         WHERE data_profile_key=$1 AND employee_id=$2",
    )
    .bind(&data_profile_key)
    .bind(employee.id)
    .fetch_one(&mut *tx)
    .await
    .expect("employee source row");
    assert_eq!(row.try_get::<String, _>("status").unwrap(), "suspended");
    assert_eq!(row.try_get::<i64, _>("version").unwrap(), 2);
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_id=$2",
    )
    .bind(&data_profile_key)
    .bind(employee.id)
    .fetch_one(&mut *tx)
    .await
    .expect("outbox count");
    assert_eq!(event_count, 2);
    tx.commit().await.expect("inspection commit");
}

#[tokio::test]
#[ignore = "requires a disposable holding-write PostgreSQL database"]
async fn live_hr_rejects_employee_references_outside_record_scope_atomically() {
    let write_url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding write migration");

    let data_profile_key = format!("holding-hr-integrity-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let employee_company_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    let employee = EmployeeRecord {
        id: employee_id,
        employee_ref: "EMP-INTEGRITY-001".into(),
        display_name: "Integrity Employee".into(),
        scope: EmploymentScope::new(holding_id, employee_company_id, None).expect("scope"),
        status: EmploymentStatus::Active,
        hired_on: chrono::NaiveDate::from_ymd_opt(2026, 1, 2).expect("date"),
        terminated_on: None,
        version: 1,
        last_command_id: Uuid::new_v4(),
        last_idempotency_key: "hr-integrity-employee".into(),
    };
    let write = SqlxHrPersistence::new(write_pool.clone(), &data_profile_key).expect("write store");
    write
        .save_employee(&employee, None)
        .await
        .expect("employee write");

    let foreign_scope = EmploymentScope::new(holding_id, Uuid::new_v4(), None).expect("scope");
    let leave_id = Uuid::new_v4();
    let leave = LeaveRequest {
        id: leave_id,
        employee_id,
        scope: foreign_scope.clone(),
        starts_on: chrono::NaiveDate::from_ymd_opt(2026, 2, 1).expect("date"),
        ends_on: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).expect("date"),
        reason: "foreign employee binding".into(),
        status: LeaveStatus::Draft,
        requested_by: "hr-requester".into(),
        approved_by: None,
        version: 1,
        last_command_id: Uuid::new_v4(),
    };
    assert!(
        write
            .save_leave(&leave, "hr-integrity-leave", None)
            .await
            .is_err()
    );

    let expense_id = Uuid::new_v4();
    let expense = ExpenseClaim {
        id: expense_id,
        employee_id,
        scope: foreign_scope.clone(),
        currency: "THB".into(),
        amount_minor: 100,
        status: ExpenseStatus::Draft,
        submitted_by: "hr-requester".into(),
        approved_by: None,
        version: 1,
    };
    assert!(
        write
            .save_expense(&expense, "hr-integrity-expense", None)
            .await
            .is_err()
    );

    let payroll_id = Uuid::new_v4();
    let payroll = PayrollRun {
        id: payroll_id,
        scope: foreign_scope,
        starts_on: chrono::NaiveDate::from_ymd_opt(2026, 2, 1).expect("date"),
        ends_on: chrono::NaiveDate::from_ymd_opt(2026, 2, 28).expect("date"),
        lines: vec![PayrollLine {
            employee_id,
            gross_minor: 1_000,
            deductions_minor: 100,
            net_minor: 900,
        }],
        status: PayrollStatus::Draft,
        prepared_by: "hr-preparer".into(),
        reviewed_by: None,
        approved_by: None,
        total_gross_minor: 1_000,
        total_deductions_minor: 100,
        total_net_minor: 900,
        version: 1,
        created_at: Utc::now(),
    };
    assert!(
        write
            .save_payroll(&payroll, "hr-integrity-payroll", None)
            .await
            .is_err()
    );

    let mut tx = write_pool.begin().await.expect("inspection transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    for (table, id_column, aggregate_id) in [
        ("hr_leave_requests", "leave_id", leave_id),
        ("hr_expense_claims", "expense_id", expense_id),
        ("hr_payroll_runs", "payroll_id", payroll_id),
    ] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM erpx_holding.{table} WHERE data_profile_key=$1 AND {id_column}=$2"
        ))
        .bind(&data_profile_key)
        .bind(aggregate_id)
        .fetch_one(&mut *tx)
        .await
        .expect("aggregate absence");
        assert_eq!(count, 0, "rejected aggregate must not persist");
    }
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_id = ANY($2)",
    )
    .bind(&data_profile_key)
    .bind(vec![leave_id, expense_id, payroll_id])
    .fetch_one(&mut *tx)
    .await
    .expect("outbox absence");
    assert_eq!(outbox_count, 0);
    let idempotency_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_holding.hr_command_idempotency
         WHERE data_profile_key=$1 AND idempotency_key = ANY($2)",
    )
    .bind(&data_profile_key)
    .bind(vec![
        "hr-integrity-leave",
        "hr-integrity-expense",
        "hr-integrity-payroll",
    ])
    .fetch_one(&mut *tx)
    .await
    .expect("idempotency absence");
    assert_eq!(idempotency_count, 0);
    tx.commit().await.expect("inspection transaction commit");
}

#[tokio::test]
#[ignore = "requires a disposable holding-write PostgreSQL database"]
async fn live_hr_rejects_update_after_source_outbox_history_is_broken() {
    let write_url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding write migration");

    let data_profile_key = format!("holding-hr-continuity-{}", Uuid::new_v4());
    let employee = EmployeeRecord {
        id: Uuid::new_v4(),
        employee_ref: "EMP-CONTINUITY-001".into(),
        display_name: "Continuity Employee".into(),
        scope: EmploymentScope::new(Uuid::new_v4(), Uuid::new_v4(), None).expect("scope"),
        status: EmploymentStatus::Active,
        hired_on: NaiveDate::from_ymd_opt(2026, 1, 2).expect("date"),
        terminated_on: None,
        version: 1,
        last_command_id: Uuid::new_v4(),
        last_idempotency_key: "hr-continuity-employee-1".into(),
    };
    let write = SqlxHrPersistence::new(write_pool.clone(), &data_profile_key).expect("write store");
    write
        .save_employee(&employee, None)
        .await
        .expect("initial employee write");

    // Simulate a damaged source stream. The source row still claims version 1,
    // but the event that makes that version replayable is gone.
    let mut tamper = write_pool.begin().await.expect("tamper transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tamper)
        .await
        .expect("holding scope");
    sqlx::query(
        "DELETE FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_type=$2 AND aggregate_id=$3",
    )
    .bind(&data_profile_key)
    .bind("hr_employee")
    .bind(employee.id)
    .execute(&mut *tamper)
    .await
    .expect("delete damaged event");
    tamper.commit().await.expect("tamper commit");

    let mut update = employee.clone();
    update.display_name = "Continuity Employee Updated".into();
    update.version = 2;
    update.last_command_id = Uuid::new_v4();
    update.last_idempotency_key = "hr-continuity-employee-2".into();
    let error = write
        .save_employee(&update, Some(1))
        .await
        .expect_err("broken source history must block the update");
    assert!(error.to_string().contains("not contiguous"));

    let mut verify = write_pool.begin().await.expect("verification transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *verify)
        .await
        .expect("verification scope");
    let source_version: i64 = sqlx::query_scalar(
        "SELECT version FROM erpx_holding.hr_employee_records
         WHERE data_profile_key=$1 AND employee_id=$2",
    )
    .bind(&data_profile_key)
    .bind(employee.id)
    .fetch_one(&mut *verify)
    .await
    .expect("source row remains");
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_type=$2 AND aggregate_id=$3",
    )
    .bind(&data_profile_key)
    .bind("hr_employee")
    .bind(employee.id)
    .fetch_one(&mut *verify)
    .await
    .expect("outbox count");
    let rejected_idempotency_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_holding.hr_command_idempotency
         WHERE data_profile_key=$1 AND idempotency_key=$2",
    )
    .bind(&data_profile_key)
    .bind(&update.last_idempotency_key)
    .fetch_one(&mut *verify)
    .await
    .expect("idempotency count");
    assert_eq!(source_version, 1);
    assert_eq!(outbox_count, 0);
    assert_eq!(rejected_idempotency_count, 0);
    verify.commit().await.expect("verification commit");
}
