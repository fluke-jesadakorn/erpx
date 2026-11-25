//! Query port for the native Finance reference-option reads. This module owns
//! the orchestration and the response envelope, never SQL or a database.
//!
//! Composition injects the owner reader behind this port; the axum router only
//! transports a validated Company identity and a registered read kind.

use super::domain::{ReferenceKind, ReferenceRow, unavailable};
use erpx_server_runtime::public::http_support::serde_json::{self, Value};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type ReferenceReadFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<ReferenceRow>, BusinessError>> + Send + 'a>>;

/// The owner owns the statement, the Company bind and the projection. The
/// router transports facts only; no caller SQL or predicate crosses this port.
pub trait FinanceReferenceRead: Send + Sync {
    fn read<'a>(&'a self, kind: ReferenceKind, company: &'a str) -> ReferenceReadFuture<'a>;
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
