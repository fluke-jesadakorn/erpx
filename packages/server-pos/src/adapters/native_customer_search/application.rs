//! Application port for the native POS customer-search read.
//!
//! No SQL and no HTTP here: domain validates the request and the composition
//! decisions, then delegates to the injected owner reader. The handler owns
//! the authority/package gates and the complete response row, so the transport
//! cannot add or drop a payload member.

use crate::adapters::native_customer_search::domain::{
    CustomerRow, CustomerSearchQuery, forbidden, validate,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type CustomerFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition supplies the pool-backed owner reader; the router never builds
/// SQL or derives an untrusted Company scope itself.
pub trait PosCustomerReader: Send + Sync {
    fn rows<'a>(&'a self, query: &'a CustomerSearchQuery) -> CustomerFuture<'a, Vec<CustomerRow>>;
}

/// Validate authority, scope and package gate before any owner read, then
/// return the complete row payload. A denied caller never reaches the pool.
pub async fn search<R: PosCustomerReader + ?Sized>(
    reader: &R,
    query: &CustomerSearchQuery,
) -> Result<Vec<Value>, BusinessError> {
    if query.actor.department {
        return Err(forbidden(
            "POS_BRANCH_REQUIRED",
            "Select a Company or Branch",
        ));
    }
    if !query.actor.read {
        return Err(forbidden(
            "PERMISSION_REQUIRED",
            "Your selected role does not grant this permission.",
        ));
    }
    validate(query)?;
    if query.package.enforced && !query.package.history {
        return Err(forbidden(
            "PACKAGE_REQUIRED",
            "This package does not permit the requested work.",
        ));
    }
    let rows = reader.rows(query).await?;
    Ok(rows.into_iter().map(CustomerRow::value).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_customer_search::domain::{
        CustomerActorDecision, CustomerPackageDecision, ROW_KEYS,
    };
    use std::task::{Context, Poll, Waker};

    /// The crate intentionally declares no direct tokio dev-dependency, so
    /// tests use a std-only executor rather than widening Cargo.toml.
    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = Box::pin(future);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    struct Probe {
        result: Result<Vec<CustomerRow>, BusinessError>,
    }

    impl PosCustomerReader for Probe {
        fn rows<'a>(&'a self, _: &'a CustomerSearchQuery) -> CustomerFuture<'a, Vec<CustomerRow>> {
            Box::pin(async move { self.result.clone() })
        }
    }

    fn query() -> CustomerSearchQuery {
        CustomerSearchQuery {
            company: "co".into(),
            q: "acme".into(),
            actor: CustomerActorDecision {
                read: true,
                department: false,
            },
            package: CustomerPackageDecision {
                enforced: false,
                history: false,
            },
        }
    }

    fn row() -> CustomerRow {
        CustomerRow {
            id: "party1".into(),
            name: "Acme".into(),
            phone: None,
            notification_email: Some("billing@example.test".into()),
            address_json: "{}".into(),
            version: 2,
        }
    }

    #[test]
    fn search_returns_full_payload_without_a_reader_scope() {
        let reader = Probe {
            result: Ok(vec![row()]),
        };
        let rows = block_on(search(&reader, &query())).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].as_object().unwrap().len(), ROW_KEYS.len());
        assert_eq!(rows[0]["id"], "party1");
    }

    #[test]
    fn denied_authority_and_package_never_read() {
        let reader = Probe {
            result: Ok(vec![row()]),
        };
        let mut denied = query();
        denied.actor.department = true;
        assert_eq!(
            block_on(search(&reader, &denied)).unwrap_err().code,
            "POS_BRANCH_REQUIRED"
        );

        let mut denied = query();
        denied.actor.read = false;
        assert_eq!(
            block_on(search(&reader, &denied)).unwrap_err().code,
            "PERMISSION_REQUIRED"
        );

        let gated = CustomerSearchQuery {
            package: CustomerPackageDecision {
                enforced: true,
                history: false,
            },
            ..query()
        };
        assert_eq!(
            block_on(search(&reader, &gated)).unwrap_err().code,
            "PACKAGE_REQUIRED"
        );
    }
}
