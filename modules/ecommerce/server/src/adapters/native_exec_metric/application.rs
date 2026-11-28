//! Native Ecommerce executive metric application: read port and renderer.
//!
//! Composition injects the owner reader. The handler owns validation, the
//! retired statement and the exact reply shape (raw scalar rows plus the
//! trusted statement and binds) consumed by the retained source drill-down.

use crate::adapters::native_exec_metric::domain::{
    BindValue, Breakdown, Metric, MetricRow, QuerySpec, Scope, build_query, unavailable,
    validate_range, validate_scope,
};
use erpx_platform_runtime_server::public::http_support::serde_json::{json, Value};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type ExecMetricFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub trait ExecMetricRead: Send + Sync {
    fn read<'a>(&'a self, spec: &'a QuerySpec) -> ExecMetricFuture<'a, Vec<MetricRow>>;
}

/// Validate, read and render one bounded owner-executed metric projection.
pub async fn read_exec_metric(
    reader: &dyn ExecMetricRead,
    metric: Metric,
    breakdown: Breakdown,
    scope: Scope<'_>,
    from_at: i64,
    to_exclusive: i64,
) -> Result<Value, BusinessError> {
    validate_scope(&scope)?;
    validate_range(from_at, to_exclusive)?;
    let spec = build_query(metric, breakdown, &scope, from_at, to_exclusive);
    let rows = reader.read(&spec).await.map_err(|error| {
        if error.kind == erpx_platform_shared_server::public::FailureKind::InvalidInput {
            error
        } else {
            unavailable()
        }
    })?;
    let raw = rows
        .into_iter()
        .map(|row| json!({ "label": row.label, "value": row.value }))
        .collect::<Vec<Value>>();
    let values = spec
        .values
        .iter()
        .map(|value| match value {
            BindValue::Text(Some(text)) => json!(text),
            BindValue::Text(None) => Value::Null,
            BindValue::Int(number) => json!(number),
        })
        .collect::<Vec<Value>>();
    Ok(json!({ "raw": raw, "sql": spec.sql, "values": values }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Fake {
        seen: Arc<Mutex<Option<QuerySpec>>>,
    }

    impl ExecMetricRead for Fake {
        fn read<'a>(&'a self, spec: &'a QuerySpec) -> ExecMetricFuture<'a, Vec<MetricRow>> {
            *self.seen.lock().unwrap() = Some(spec.clone());
            Box::pin(async {
                Ok(vec![MetricRow {
                    label: "open".into(),
                    value: "3".into(),
                }])
            })
        }
    }

    #[tokio::test]
    async fn handler_pins_the_reply_shape_and_the_trusted_statement() {
        let seen = Arc::new(Mutex::new(None));
        let reader = Fake { seen: seen.clone() };
        let value = read_exec_metric(
            &reader,
            Metric::OpenCarts,
            Breakdown::Status,
            Scope { company_id: "c", branch_id: Some("b"), department_id: None },
            1_767_225_600_000,
            1_769_904_000_000,
        )
        .await
        .unwrap();
        assert_eq!(value["raw"][0]["label"], json!("open"));
        assert_eq!(value["raw"][0]["value"], json!("3"));
        assert_eq!(value["values"][0], json!("c"));
        assert_eq!(value["values"][1], json!("b"));
        assert!(value["values"][3].is_null());
        assert_eq!(value.as_object().unwrap().len(), 3);
        let spec = seen.lock().unwrap().clone().unwrap();
        assert_eq!(spec.sql, value["sql"].as_str().unwrap());
        assert_eq!(spec.values.len(), 5);
    }

    #[tokio::test]
    async fn handler_fails_closed_before_reading_on_bad_scope_or_range() {
        let seen = Arc::new(Mutex::new(None));
        let reader = Fake { seen: seen.clone() };
        let error = read_exec_metric(
            &reader,
            Metric::OpenCarts,
            Breakdown::Status,
            Scope { company_id: "", branch_id: None, department_id: None },
            1_767_225_600_000,
            1_769_904_000_000,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "ECOMMERCE_EXEC_METRIC_REQUEST_INVALID");
        assert!(seen.lock().unwrap().is_none());
    }
}
