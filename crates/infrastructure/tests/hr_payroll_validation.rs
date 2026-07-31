//! Regression for authoritative HR payroll validation before source writes.

use chrono::{NaiveDate, Utc};
use erpx_domain::hr::{EmploymentScope, PayrollRun, PayrollStatus};
use erpx_infrastructure::SqlxHrPersistence;
use sqlx::PgPool;
use uuid::Uuid;

#[tokio::test]
async fn empty_payroll_is_rejected_before_source_or_outbox_access() {
    let pool = PgPool::connect_lazy("postgres://unused").expect("lazy pool");
    let store = SqlxHrPersistence::new(pool, "holding-hr-payroll-validation")
        .expect("HR persistence store");
    let payroll = PayrollRun {
        id: Uuid::new_v4(),
        scope: EmploymentScope::new(Uuid::new_v4(), Uuid::new_v4(), None).expect("scope"),
        starts_on: NaiveDate::from_ymd_opt(2026, 7, 1).expect("start date"),
        ends_on: NaiveDate::from_ymd_opt(2026, 7, 31).expect("end date"),
        lines: Vec::new(),
        status: PayrollStatus::Draft,
        prepared_by: "payroll-preparer".into(),
        reviewed_by: None,
        approved_by: None,
        total_gross_minor: 0,
        total_deductions_minor: 0,
        total_net_minor: 0,
        version: 1,
        created_at: Utc::now(),
    };

    let error = store
        .save_payroll(&payroll, "hr-payroll-empty-lines", None)
        .await
        .expect_err("empty payroll must fail before opening a write transaction");
    assert!(
        error
            .to_string()
            .contains("payroll must contain at least one employee line")
    );
}
