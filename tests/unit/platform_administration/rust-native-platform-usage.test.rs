//! Native Platform usage read: owner application behaviour and the executable
//! SQL contract. The SQL execution itself is in the owner's sqlx adapter.
#![allow(dead_code)]
use erpx_server_platform_administration::public::native_platform_usage::*;
use erpx_server_shared::public::FailureKind;
use std::sync::Mutex;

struct Store {
    count: i64,
    seen: Mutex<Vec<(String, String)>>,
}

impl ExecutiveUsageRead for Store {
    fn generation_jobs<'a>(
        &'a self,
        company: &'a str,
        month: &'a str,
    ) -> UsageFuture<'a, i64> {
        let company = company.to_owned();
        let month = month.to_owned();
        Box::pin(async move {
            self.seen.lock().unwrap().push((company, month));
            Ok(self.count)
        })
    }
}

fn permissions(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[tokio::test]
async fn usage_query_returns_full_payload_and_binds_company_and_month() {
    let store = Store {
        count: 12,
        seen: Mutex::new(Vec::new()),
    };
    let grants = permissions(&["billing.usage.read", "platform.read"]);
    let query =
        ExecutiveUsageQuery::parse("company-001", "2026-06", "platform_admin", &grants).unwrap();
    let usage = executive_usage(&query, &store).await.unwrap();
    assert_eq!(usage.company_id, "company-001");
    assert_eq!(usage.month, "2026-06");
    assert_eq!(usage.generation_jobs, 12);
    assert_eq!(
        *store.seen.lock().unwrap(),
        vec![("company-001".to_owned(), "2026-06".to_owned())]
    );
    assert!(serde_json::to_string(&usage)
        .unwrap()
        .contains("\"generation_jobs\":12"));
}

#[tokio::test]
async fn usage_query_requires_platform_billing_authority() {
    let store = Store {
        count: 1,
        seen: Mutex::new(Vec::new()),
    };
    let grants = permissions(&["billing.usage.read"]);
    for role in ["platform_owner", "holding_owner", "operations_manager"] {
        let query = ExecutiveUsageQuery::parse("company-001", "2026-06", role, &grants).unwrap();
        let error = executive_usage(&query, &store).await.unwrap_err();
        assert_eq!(error.code, "PLATFORM_USAGE_FORBIDDEN");
        assert_eq!(error.kind, FailureKind::Forbidden);
    }
    let no_grant = ExecutiveUsageQuery::parse("company-001", "2026-06", "platform_auditor", &[])
        .unwrap();
    assert_eq!(
        executive_usage(&no_grant, &store).await.unwrap_err().code,
        "PLATFORM_USAGE_FORBIDDEN"
    );
    assert!(store.seen.lock().unwrap().is_empty());
}

#[test]
fn executable_sql_is_the_owner_bound_render_of_the_retired_typescript_projection() {
    let rendered = rendered_usage_sql();
    assert_eq!(rendered.matches('?').count(), 2);
    assert!(rendered.contains("FROM executive_generation_jobs"));
    assert!(rendered.contains("to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"));
    assert!(rendered.contains("WHERE company_id=? AND month=?"));
    let mut executable = rendered.clone();
    let first = executable.find("=?").unwrap();
    executable.replace_range(first..first + 2, "=$1");
    let second = executable.find("=?").unwrap();
    executable.replace_range(second..second + 2, "=$2");
    assert_eq!(executable, EXECUTIVE_USAGE_SQL);
    assert!(!EXECUTIVE_USAGE_SQL.contains("FOR UPDATE"));
    assert!(!EXECUTIVE_USAGE_SQL.contains("INSERT"));
}

#[test]
fn month_and_company_validation_is_fail_closed() {
    assert!(valid_company("company-001"));
    assert!(!valid_company("' OR 1=1 --"));
    assert!(valid_month("2026-01"));
    for month in ["2026-13", "2026-00", "2025-12-01", "abcd-01", "2026/01"] {
        assert!(!valid_month(month));
    }
    assert!(ExecutiveUsage::new("company-001".into(), "2026-06".into(), 3).is_ok());
    assert!(ExecutiveUsage::new("company-001".into(), "2026-06".into(), MAX_USAGE + 1).is_err());
}

#[test]
fn router_configuration_is_closed() {
    let reader: SharedExecutiveUsageRead = std::sync::Arc::new(Store {
        count: 1,
        seen: Mutex::new(Vec::new()),
    });
    assert!(platform_usage_router_for_hosts(
        vec!["erpx-internal.example.test".into()],
        reader.clone(),
    )
    .is_ok());
    assert!(platform_usage_router_for_hosts(vec![], reader.clone()).is_err());
    assert!(platform_usage_router_for_hosts(
        vec!["a".into(), "a".into()],
        reader,
    )
    .is_err());
}
