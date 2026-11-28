//! Owner query port and handler. Validation and response shape live here;
//! SQL is confined to `postgres.rs`.

use super::domain::{EmployeeOrder, invalid, valid_id};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type EmployeeOrderFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Injected by composition behind the owner adapter. The Company scope and
/// the purchase-order identity are the only inputs; no predicate text crosses
/// this port.
pub trait CommerceEmployeeOrderRead: Send + Sync {
    fn read<'a>(
        &'a self,
        company: &'a str,
        order_id: &'a str,
    ) -> EmployeeOrderFuture<'a, Option<EmployeeOrder>>;
}

/// Bound the scope before delegating. No SQL and no caller-supplied projection
/// is accepted by this layer.
pub async fn read_order<R: CommerceEmployeeOrderRead + ?Sized>(
    reader: &R,
    company: &str,
    order_id: &str,
) -> Result<Option<EmployeeOrder>, BusinessError> {
    if !valid_id(company) || !valid_id(order_id) {
        return Err(invalid());
    }
    reader.read(company, order_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct Probe {
        seen: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl CommerceEmployeeOrderRead for Probe {
        fn read<'a>(
            &'a self,
            company: &'a str,
            order_id: &'a str,
        ) -> EmployeeOrderFuture<'a, Option<EmployeeOrder>> {
            let seen = self.seen.clone();
            let company = company.to_owned();
            let order_id = order_id.to_owned();
            Box::pin(async move {
                seen.lock().expect("lock").push((company, order_id));
                Ok(None)
            })
        }
    }

    #[tokio::test]
    async fn handler_forwards_company_and_order_identity() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe = Probe { seen: seen.clone() };
        let order = read_order(&probe, "co-1", "o-1").await.expect("read");
        assert!(order.is_none());
        assert_eq!(
            *seen.lock().expect("lock"),
            vec![("co-1".to_owned(), "o-1".to_owned())]
        );
    }

    #[tokio::test]
    async fn handler_rejects_unbounded_identities_before_reading() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe = Probe { seen: seen.clone() };
        assert!(read_order(&probe, "bad company!", "o-1").await.is_err());
        assert!(read_order(&probe, "co-1", "bad id!").await.is_err());
        assert!(seen.lock().expect("lock").is_empty());
    }
}
