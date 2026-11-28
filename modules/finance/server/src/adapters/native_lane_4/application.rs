//! Query port for the native Finance reference-option reads. This module owns
//! the orchestration and the response envelope, never SQL or a database.
//!
//! Composition injects the owner reader behind this port; the axum router only
//! transports a validated Company identity and a registered read kind.

use super::domain::{
    GuardKind, GuardRow, ReferenceKind, ReferenceRow, project_guard, unavailable,
};
use erpx_platform_runtime_server::public::http_support::serde_json::{self, Value};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type ReferenceReadFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<ReferenceRow>, BusinessError>> + Send + 'a>>;
pub type GuardReadFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<GuardRow>, BusinessError>> + Send + 'a>>;

/// The owner owns the statement, the Company bind and the projection. The
/// router transports facts only; no caller SQL or predicate crosses this port.
pub trait FinanceReferenceRead: Send + Sync {
    fn read<'a>(&'a self, kind: ReferenceKind, company: &'a str) -> ReferenceReadFuture<'a>;

    /// Lock-free finance posting guard probe. The default keeps older injected
    /// readers fail-closed; composition's Pg reader overrides it.
    fn guard<'a>(
        &'a self,
        _kind: GuardKind,
        _company: &'a str,
        _id: &'a str,
        _chart: Option<&'a str>,
        _currency: &'a str,
    ) -> GuardReadFuture<'a> {
        Box::pin(async { Err(unavailable()) })
    }
}

/// Run one registered read and build the bounded wire envelope. Row projection
/// is enforced here so a malformed owner row cannot produce a partial reply.
pub async fn read_reference(
    reader: &dyn FinanceReferenceRead,
    kind: ReferenceKind,
    company: &str,
) -> Result<Value, BusinessError> {
    let rows = reader.read(kind, company).await?;
    let items = rows
        .iter()
        .map(|row| super::domain::project(kind, row))
        .collect::<Result<Vec<Value>, BusinessError>>()?;
    serde_json::to_value(items).map_err(|_| unavailable())
}

/// Run one lock-free posting guard probe and build the bounded envelope.
pub async fn read_guard(
    reader: &dyn FinanceReferenceRead,
    kind: GuardKind,
    company: &str,
    id: &str,
    chart: Option<&str>,
    currency: &str,
) -> Result<Value, BusinessError> {
    let row = reader.guard(kind, company, id, chart, currency).await?;
    Ok(serde_json::json!({"data": row.as_ref().map(project_guard)}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeReader {
        calls: Arc<Mutex<Vec<(ReferenceKind, String)>>>,
        rows: Vec<ReferenceRow>,
    }

    impl FinanceReferenceRead for FakeReader {
        fn read<'a>(&'a self, kind: ReferenceKind, company: &'a str) -> ReferenceReadFuture<'a> {
            let calls = Arc::clone(&self.calls);
            let rows = self.rows.clone();
            Box::pin(async move {
                calls
                    .lock()
                    .map_err(|_| unavailable())?
                    .push((kind, company.to_owned()));
                Ok(rows)
            })
        }
    }

    fn account_row() -> ReferenceRow {
        ReferenceRow {
            id: "acct-1".to_owned(),
            name: "Cash".to_owned(),
            code: Some("100".to_owned()),
            chart_id: Some("chart-1".to_owned()),
            category: Some("asset".to_owned()),
        }
    }

    #[tokio::test]
    async fn envelope_is_bounded_and_reports_the_reader_call() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let reader = FakeReader {
            calls: Arc::clone(&calls),
            rows: vec![account_row()],
        };
        let value = read_reference(&reader, ReferenceKind::Accounts, "company-1")
            .await
            .unwrap();
        assert_eq!(
            value,
            serde_json::json!([{
                "id": "acct-1", "name": "Cash", "code": "100",
                "chart_id": "chart-1", "category": "asset"
            }])
        );
        assert_eq!(
            calls.lock().unwrap().clone(),
            vec![(ReferenceKind::Accounts, "company-1".to_owned())]
        );
    }

    struct GuardReader;

    impl FinanceReferenceRead for GuardReader {
        fn read<'a>(&'a self, _kind: ReferenceKind, _company: &'a str) -> ReferenceReadFuture<'a> {
            Box::pin(async { Err(unavailable()) })
        }
        fn guard<'a>(
            &'a self,
            kind: GuardKind,
            company: &'a str,
            id: &'a str,
            _chart: Option<&'a str>,
            _currency: &'a str,
        ) -> GuardReadFuture<'a> {
            let id = id.to_owned();
            let company = company.to_owned();
            Box::pin(async move {
                if company != "company-1" {
                    return Ok(None);
                }
                Ok(Some(GuardRow {
                    id: format!("{}-{id}", kind.as_str()),
                }))
            })
        }
    }

    #[tokio::test]
    async fn guard_envelope_pins_the_row_or_null() {
        let reader = GuardReader;
        assert_eq!(
            read_guard(
                &reader,
                GuardKind::AccountLiability,
                "company-1",
                "acct-1",
                Some("chart-1"),
                "THB",
            )
            .await
            .unwrap(),
            serde_json::json!({"data": {"id": "account_liability-acct-1"}})
        );
        assert_eq!(
            read_guard(
                &reader,
                GuardKind::PartyCustomer,
                "company-2",
                "party-1",
                None,
                "THB",
            )
            .await
            .unwrap(),
            serde_json::json!({"data": null})
        );
    }

    #[test]
    fn no_kind_leaks_a_predicate_or_sql() {
        let kinds: BTreeMap<&str, ReferenceKind> = [
            ("accounts", ReferenceKind::Accounts),
            ("charts", ReferenceKind::Charts),
            ("parties", ReferenceKind::Parties),
        ]
        .into_iter()
        .collect();
        assert_eq!(kinds.len(), 3);
        for (name, kind) in kinds {
            assert!(!name.contains(' '));
            assert_eq!(ReferenceKind::parse(name), Some(kind));
        }
    }

}
