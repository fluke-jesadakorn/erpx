//! Application port for the native public Marketing form source read.
//!
//! No SQL and no HTTP here: domain validates the bounded request, then the
//! injected owner source executes the registered statement. The application
//! applies the 404 and malformed-row rules, so the transport cannot add or
//! drop a payload member and no TypeScript fallback is consulted.

use crate::adapters::native_public_form::domain::{self, FormQuery, FormRow};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type PublicFormFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition supplies the owner source; the router never builds SQL or
/// derives an untrusted form id itself.
pub trait PublicFormSource: Send + Sync {
    fn form<'a>(&'a self, form_id: &'a str) -> PublicFormFuture<'a, Option<FormRow>>;
}

/// Validate the id, execute the owner read and return the complete source row.
/// A malformed row never reaches the wire.
pub async fn read<S: PublicFormSource + ?Sized>(
    source: &S,
    query: &FormQuery,
) -> Result<Value, BusinessError> {
    domain::validate(query)?;
    let row = source
        .form(&query.form_id)
        .await?
        .ok_or_else(domain::missing)?;
    if !domain::valid_row(&row) {
        return Err(domain::unavailable());
    }
    Ok(row.value())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_public_form::domain::SOURCE_KEYS;
    use std::task::{Context, Poll, Waker};

    /// No direct tokio dev-dependency is needed for these pure async tests.
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
        row: Option<FormRow>,
    }

    impl PublicFormSource for Probe {
        fn form<'a>(&'a self, _: &'a str) -> PublicFormFuture<'a, Option<FormRow>> {
            let row = self.row.clone();
            Box::pin(async move { Ok(row) })
        }
    }

    fn row() -> FormRow {
        FormRow {
            id: "form-1".into(),
            company_id: "co-1".into(),
            name: "Contact".into(),
            version: 3,
            preview_json: "{\"headline\":\"Hi\"}".into(),
            configuration_json: "{\"consentRequired\":true}".into(),
        }
    }

    #[test]
    fn read_returns_the_full_source_row() {
        let source = Probe {
            row: Some(row()),
        };
        let query = FormQuery {
            form_id: "form-1".into(),
        };
        let value = block_on(read(&source, &query)).unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), SOURCE_KEYS.len());
        assert_eq!(value["company_id"], "co-1");
        assert_eq!(value["version"], 3);
    }

    #[test]
    fn missing_invalid_and_malformed_rows_fail_closed() {
        let source = Probe { row: None };
        let query = FormQuery {
            form_id: "form-1".into(),
        };
        assert_eq!(block_on(read(&source, &query)).unwrap_err().code, "FORM_NOT_FOUND");
        assert_eq!(
            block_on(read(
                &source,
                &FormQuery {
                    form_id: "bad id".into()
                }
            ))
            .unwrap_err()
            .code,
            "MARKETING_PUBLIC_FORM_INVALID"
        );
        let malformed = Probe {
            row: Some(FormRow {
                version: -1,
                ..row()
            }),
        };
        assert_eq!(
            block_on(read(&malformed, &query)).unwrap_err().code,
            "MARKETING_PUBLIC_FORM_UNAVAILABLE"
        );
    }
}
