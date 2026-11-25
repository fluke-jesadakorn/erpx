//! Owner query port and handler. This layer owns the decision and response
//! shape but never constructs SQL; `postgres.rs` owns execution.

use super::domain::{EntrypointFacts, EntrypointKind, EntrypointQuery, invalid};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type EntrypointReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Injected by composition behind the owner adapter. The Company scope and the
/// registered discriminator are the only inputs; no predicate text crosses it.
pub trait CommerceEntrypointRead: Send + Sync {
    fn read<'a>(
        &'a self,
        company: &'a str,
        kind: EntrypointKind,
        id: &'a str,
    ) -> EntrypointReadFuture<'a, Option<EntrypointFacts>>;
}

/// Validate then delegate. `order_facts` is the only capability that carries
/// mutable order state; all other kinds project an identity kind only.
pub async fn read_facts<R: CommerceEntrypointRead + ?Sized>(
    reader: &R,
    company: &str,
    query: &EntrypointQuery,
) -> Result<Option<EntrypointFacts>, BusinessError> {
    let facts = reader.read(company, query.kind, &query.id).await?;
    match facts {
        Some(facts)
            if query.kind.carries_state() && !matches!(facts.state, Some(ref state) if !state.is_empty()) =>
        {
            Err(invalid())
        }
        Some(facts) if query.kind.carries_state() => Ok(Some(facts)),
        Some(facts) if facts.state.is_some() => Err(invalid()),
        other => Ok(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct Probe {
        rows: BTreeMap<(String, EntrypointKind, String), EntrypointFacts>,
        seen: Arc<Mutex<Vec<(String, EntrypointKind, String)>>>,
    }

    impl CommerceEntrypointRead for Probe {
        fn read<'a>(
            &'a self,
            company: &'a str,
            kind: EntrypointKind,
            id: &'a str,
        ) -> EntrypointReadFuture<'a, Option<EntrypointFacts>> {
            let seen = self.seen.clone();
            let rows = self.rows.clone();
            let company = company.to_owned();
            let id = id.to_owned();
            Box::pin(async move {
                seen.lock().expect("lock").push((company.clone(), kind, id.clone()));
                Ok(rows.get(&(company, kind, id)).cloned())
            })
        }
    }

    #[tokio::test]
    async fn handler_forwards_scope_and_preserves_shape() {
        let key = ("co-1".to_owned(), EntrypointKind::OrderFacts, "o-1".to_owned());
        let mut rows = BTreeMap::new();
        rows.insert(
            key.clone(),
            EntrypointFacts {
                kind: "sales".to_owned(),
                state: Some("draft".to_owned()),
            },
        );
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe = Probe { rows, seen: seen.clone() };
        let query = EntrypointQuery { kind: EntrypointKind::OrderFacts, id: "o-1".to_owned() };
        let facts = read_facts(&probe, "co-1", &query)
            .await
            .expect("read")
            .expect("facts");
        assert_eq!(facts.kind, "sales");
        assert_eq!(facts.state.as_deref(), Some("draft"));
        assert_eq!(*seen.lock().expect("lock"), vec![key]);
    }

    #[tokio::test]
    async fn handler_rejects_missing_state_for_order_facts() {
        let mut rows = BTreeMap::new();
        rows.insert(
            ("co-1".to_owned(), EntrypointKind::OrderFacts, "o-1".to_owned()),
            EntrypointFacts { kind: "sales".to_owned(), state: None },
        );
        let probe = Probe { rows, seen: Arc::new(Mutex::new(Vec::new())) };
        let query = EntrypointQuery { kind: EntrypointKind::OrderFacts, id: "o-1".to_owned() };
        assert!(read_facts(&probe, "co-1", &query).await.is_err());
    }

    #[tokio::test]
    async fn handler_rejects_state_for_kind_reads() {
        let mut rows = BTreeMap::new();
        rows.insert(
            ("co-1".to_owned(), EntrypointKind::OrderKind, "o-1".to_owned()),
            EntrypointFacts {
                kind: "sales".to_owned(),
                state: Some("draft".to_owned()),
            },
        );
        let probe = Probe { rows, seen: Arc::new(Mutex::new(Vec::new())) };
        let query = EntrypointQuery { kind: EntrypointKind::OrderKind, id: "o-1".to_owned() };
        assert!(read_facts(&probe, "co-1", &query).await.is_err());
    }
}
