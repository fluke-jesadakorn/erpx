//! Disposable end-to-end evidence for the HR write/read split.
//!
//! The test is ignored in the normal workspace run because it requires two
//! disposable databases. CI or a local verification run supplies both URLs.

use chrono::NaiveDate;
use erpx_domain::hr::{EmployeeRecord, EmploymentScope, EmploymentStatus};
use erpx_infrastructure::{HrReadScope, SqlxHrPersistence, SqlxHrProjectionStore, SqlxHrReadStore};
use sqlx::PgPool;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases"]
async fn live_hr_write_projection_and_read_are_idempotent_and_separate() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("read pool");
    erpx_migration::migrate_tenant_write(&write_pool)
        .await
        .expect("tenant write migration");
    erpx_migration::migrate_tenant_read(&read_pool)
        .await
        .expect("tenant read migration");

    let holding_ref = format!("holding-hr-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    let command_id = Uuid::new_v4();
    let employee = EmployeeRecord {
        id: employee_id,
        employee_ref: "EMP-001".into(),
        display_name: "Anonymized Employee".into(),
        scope: EmploymentScope::new(holding_id, company_id, None).expect("scope"),
        status: EmploymentStatus::Active,
        hired_on: NaiveDate::from_ymd_opt(2026, 1, 2).expect("date"),
        terminated_on: None,
        version: 1,
        last_command_id: command_id,
        last_idempotency_key: "hr-employee-1".into(),
    };

    let write = SqlxHrPersistence::new(write_pool.clone(), &holding_ref).expect("write store");
    let first = write
        .save_employee(&employee, None)
        .await
        .expect("first employee write");
    assert_eq!(first.version, 1);
    assert!(!first.replayed);
    let replay = write
        .save_employee(&employee, None)
        .await
        .expect("idempotent employee replay");
    assert_eq!(
        replay,
        erpx_infrastructure::HrWriteReceipt {
            replayed: true,
            ..first.clone()
        }
    );

    let projector = SqlxHrProjectionStore::from_pools(&write_pool, &read_pool, &holding_ref)
        .expect("projector");
    let page = projector
        .read_source_event_page(None, 10)
        .await
        .expect("source page");
    assert_eq!(page.events.len(), 1);
    let mut forged = page.events[0].clone();
    forged.payload["aggregate"]["display_name"] = serde_json::json!("forged before projection");
    assert!(
        projector.project_event(&forged).await.is_err(),
        "a valid but forged HR payload must be rejected before projection"
    );
    let applied = projector
        .project_event(&page.events[0])
        .await
        .expect("project event");
    assert!(applied.applied);
    let duplicate = projector
        .project_event(&page.events[0])
        .await
        .expect("duplicate project event");
    assert!(!duplicate.applied);

    let healthy = projector
        .reconcile_projection()
        .await
        .expect("healthy HR reconciliation");
    assert!(healthy.is_consistent());

    let mut payload_tamper = read_pool.begin().await.expect("payload tamper transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *payload_tamper)
        .await
        .expect("read holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.hr_employee_read_rows
         SET display_name=$1
         WHERE holding_ref=$2 AND employee_id=$3",
    )
    .bind("Tampered projected employee")
    .bind(&holding_ref)
    .bind(employee_id)
    .execute(&mut *payload_tamper)
    .await
    .expect("tamper projected employee payload");
    payload_tamper
        .commit()
        .await
        .expect("commit payload tamper");

    let payload_drifted = projector
        .reconcile_projection()
        .await
        .expect("payload-drifted HR reconciliation");
    assert_eq!(payload_drifted.stale_aggregate_count, 1);
    assert!(!payload_drifted.is_consistent());

    let mut payload_repair = read_pool.begin().await.expect("payload repair transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *payload_repair)
        .await
        .expect("read holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.hr_employee_read_rows
         SET display_name=$1
         WHERE holding_ref=$2 AND employee_id=$3",
    )
    .bind(&employee.display_name)
    .bind(&holding_ref)
    .bind(employee_id)
    .execute(&mut *payload_repair)
    .await
    .expect("repair projected employee payload");
    payload_repair
        .commit()
        .await
        .expect("commit payload repair");

    let payload_repaired = projector
        .reconcile_projection()
        .await
        .expect("repaired-payload HR reconciliation");
    assert!(payload_repaired.is_consistent());

    let forged_company_id = Uuid::new_v4();
    let mut scope_tamper = read_pool.begin().await.expect("scope tamper transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *scope_tamper)
        .await
        .expect("read holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.hr_employee_read_rows
         SET company_id=$1
         WHERE holding_ref=$2 AND employee_id=$3",
    )
    .bind(forged_company_id)
    .bind(&holding_ref)
    .bind(employee_id)
    .execute(&mut *scope_tamper)
    .await
    .expect("tamper projected employee scope");
    scope_tamper.commit().await.expect("commit scope tamper");

    let scope_drifted = projector
        .reconcile_projection()
        .await
        .expect("scope-drifted HR reconciliation");
    assert_eq!(scope_drifted.stale_aggregate_count, 1);
    assert!(!scope_drifted.is_consistent());

    let mut scope_repair = read_pool.begin().await.expect("scope repair transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *scope_repair)
        .await
        .expect("read holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.hr_employee_read_rows
         SET company_id=$1
         WHERE holding_ref=$2 AND employee_id=$3",
    )
    .bind(company_id)
    .bind(&holding_ref)
    .bind(employee_id)
    .execute(&mut *scope_repair)
    .await
    .expect("repair projected employee scope");
    scope_repair.commit().await.expect("commit scope repair");

    let scope_repaired = projector
        .reconcile_projection()
        .await
        .expect("repaired-scope HR reconciliation");
    assert!(scope_repaired.is_consistent());

    let forged_checkpoint_event_id = Uuid::new_v4();
    let mut tamper = read_pool
        .begin()
        .await
        .expect("checkpoint tamper transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tamper)
        .await
        .expect("read holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.hr_projection_checkpoints
         SET last_event_id=$1
         WHERE holding_ref=$2 AND projection_name=$3
           AND aggregate_type='hr_employee' AND aggregate_id=$4",
    )
    .bind(forged_checkpoint_event_id)
    .bind(&holding_ref)
    .bind(erpx_infrastructure::HR_PROJECTION_NAME)
    .bind(employee_id)
    .execute(&mut *tamper)
    .await
    .expect("tamper checkpoint identity");
    tamper.commit().await.expect("commit checkpoint tamper");

    let drifted = projector
        .reconcile_projection()
        .await
        .expect("drifted HR reconciliation");
    assert_eq!(drifted.stale_aggregate_count, 1);
    assert!(!drifted.is_consistent());

    let mut repair = read_pool
        .begin()
        .await
        .expect("checkpoint repair transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *repair)
        .await
        .expect("read holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.hr_projection_checkpoints
         SET last_event_id=$1
         WHERE holding_ref=$2 AND projection_name=$3
           AND aggregate_type='hr_employee' AND aggregate_id=$4",
    )
    .bind(page.events[0].event_id)
    .bind(&holding_ref)
    .bind(erpx_infrastructure::HR_PROJECTION_NAME)
    .bind(employee_id)
    .execute(&mut *repair)
    .await
    .expect("repair checkpoint identity");
    repair.commit().await.expect("commit checkpoint repair");

    let repaired = projector
        .reconcile_projection()
        .await
        .expect("repaired HR reconciliation");
    assert!(repaired.is_consistent());

    let read = SqlxHrReadStore::new(read_pool, &holding_ref).expect("read store");
    let employees = read
        .list_employees(
            HrReadScope {
                holding_id,
                company_id,
                branch_id: None,
            },
            10,
            None,
        )
        .await
        .expect("employee read");
    assert_eq!(employees.rows.len(), 1);
    assert_eq!(employees.rows[0].employee_id, employee_id);
    assert_eq!(employees.rows[0].source_version, 1);
    assert!(!employees.rows[0].source_event_id.is_nil());
}
