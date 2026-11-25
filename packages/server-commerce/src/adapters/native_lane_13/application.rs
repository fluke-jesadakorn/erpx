//! Owner query port and handler. This layer owns the decision and response
//! shape but never constructs SQL; `postgres.rs` owns execution.

use super::domain::{
    BillingFacts, BillingTestQuery, BillingTestReadKind, EntrypointFacts, EntrypointKind,
    EntrypointQuery, InvoiceFacts, InvoiceReadKind, InvoiceReadQuery, invalid,
};
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

pub type InvoiceReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner port for invoice/delivery facts. No SQL, Company scope or projection
/// text crosses this boundary; only the registered kind, Company and identity.
pub trait CommerceInvoiceRead: Send + Sync {
    fn read<'a>(
        &'a self,
        company: &'a str,
        kind: InvoiceReadKind,
        id: &'a str,
    ) -> InvoiceReadFuture<'a, InvoiceFacts>;
}

/// Validate then delegate. The reader owns both the query and the envelope
/// shape; the HTTP layer only transports the result.
pub async fn read_invoice<R: CommerceInvoiceRead + ?Sized>(
    reader: &R,
    company: &str,
    query: &InvoiceReadQuery,
) -> Result<InvoiceFacts, BusinessError> {
    reader.read(company, query.kind, &query.id).await
}

pub type BillingReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner port for the two plain billing-test entry reads. The registry Holding
/// is resolved by target; no caller SQL or table name crosses this boundary.
pub trait BillingTestRead: Send + Sync {
    fn read<'a>(
        &'a self,
        company: &'a str,
        kind: BillingTestReadKind,
        id: &'a str,
    ) -> BillingReadFuture<'a, BillingFacts>;
}

/// Validate then delegate. The fixed test company is carried as context, not
/// as a caller-supplied predicate.
pub async fn read_billing<R: BillingTestRead + ?Sized>(
    reader: &R,
    company: &str,
    query: &BillingTestQuery,
) -> Result<BillingFacts, BusinessError> {
    reader.read(company, query.kind, &query.id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct BillingProbe {
        seen: Arc<Mutex<Vec<(String, BillingTestReadKind, String)>>>,
    }

    impl BillingTestRead for BillingProbe {
        fn read<'a>(
            &'a self,
            company: &'a str,
            kind: BillingTestReadKind,
            id: &'a str,
        ) -> BillingReadFuture<'a, BillingFacts> {
            let seen = self.seen.clone();
            let company = company.to_owned();
            let id = id.to_owned();
            Box::pin(async move {
                seen.lock().expect("lock").push((company, kind, id));
                Ok(match kind {
                    BillingTestReadKind::EntryBySourceKey => BillingFacts::Entry(None),
                    BillingTestReadKind::EntryTotalsByStripe => BillingFacts::Totals(
                        super::super::domain::BillingTotals {
                            issued: 0,
                            received: 0,
                            reversed: 0,
                            credited: 0,
                        },
                    ),
                })
            })
        }
    }

    #[tokio::test]
    async fn billing_handler_forwards_registry_scope_and_kind() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe = BillingProbe { seen: seen.clone() };
        let query = BillingTestQuery {
            kind: BillingTestReadKind::EntryTotalsByStripe,
            id: "inv-1".to_owned(),
        };
        let facts = read_billing(&probe, "erpx_billing_sandbox", &query)
            .await
            .expect("read");
        assert_eq!(facts, BillingFacts::Totals(super::super::domain::BillingTotals {
            issued: 0,
            received: 0,
            reversed: 0,
            credited: 0,
        }));
        assert_eq!(
            *seen.lock().expect("lock"),
            vec![(
                "erpx_billing_sandbox".to_owned(),
                BillingTestReadKind::EntryTotalsByStripe,
                "inv-1".to_owned()
            )]
        );
    }

    #[derive(Clone)]
    struct InvoiceProbe {
        seen: Arc<Mutex<Vec<(String, InvoiceReadKind, String)>>>,
    }

    impl CommerceInvoiceRead for InvoiceProbe {
        fn read<'a>(
            &'a self,
            company: &'a str,
            kind: InvoiceReadKind,
            id: &'a str,
        ) -> InvoiceReadFuture<'a, InvoiceFacts> {
            let seen = self.seen.clone();
            let company = company.to_owned();
            let id = id.to_owned();
            Box::pin(async move {
                seen.lock().expect("lock").push((company, kind, id));
                Ok(match kind {
                    InvoiceReadKind::DeliveryTarget => InvoiceFacts::Target(None),
                    InvoiceReadKind::InvoiceSalesLines => InvoiceFacts::Lines(Vec::new()),
                })
            })
        }
    }

    #[tokio::test]
    async fn invoice_handler_forwards_owner_scope_and_kind() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe = InvoiceProbe { seen: seen.clone() };
        let query = InvoiceReadQuery {
            kind: InvoiceReadKind::InvoiceSalesLines,
            id: "inv-1".to_owned(),
        };
        let facts = read_invoice(&probe, "co-1", &query).await.expect("read");
        assert_eq!(facts, InvoiceFacts::Lines(Vec::new()));
        assert_eq!(
            *seen.lock().expect("lock"),
            vec![("co-1".to_owned(), InvoiceReadKind::InvoiceSalesLines, "inv-1".to_owned())]
        );
    }

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
