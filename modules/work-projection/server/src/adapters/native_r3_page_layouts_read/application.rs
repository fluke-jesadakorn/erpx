//! Owner query port and handler for the R3 page-layout read. Validation and
//! projection are owned by the domain layer; this layer only dispatches to the
//! injected owner reader. No SQL, transaction or authority lives here.

use crate::adapters::native_r3_page_layouts_read::domain::{project_snapshot, validate, ReadQuery};
use crate::adapters::page_layouts_read::PageLayoutRow;
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type ReadFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The owner executes the read on its own Holding-scoped pool. Composition
/// injects the Pg implementation; the HTTP adapter never sees a connection.
pub trait PageLayoutsReadPort: Send + Sync {
    fn read(&self, query: ReadQuery) -> ReadFuture<'_, Vec<PageLayoutRow>>;
}

/// Validate once at the application boundary, then read and project. The port
/// cannot be reached with an unvalidated page key, scope or widget allow-list.
pub async fn handle(
    port: &dyn PageLayoutsReadPort,
    query: ReadQuery,
) -> Result<Value, BusinessError> {
    validate(&query)?;
    let rows = port.read(query.clone()).await?;
    project_snapshot(&rows, &query)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::sync::{Arc, Mutex};

    struct Fixed {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl PageLayoutsReadPort for Fixed {
        fn read(&self, query: ReadQuery) -> ReadFuture<'_, Vec<PageLayoutRow>> {
            let seen = self.seen.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(query.page_key.clone());
                Ok(vec![PageLayoutRow {
                    owner_id: "organization".into(),
                    layout_json: Some(
                        r#"{"schemaVersion":1,"widgets":[{"id":"w1","columns":6,"height":null,"hidden":false}]}"#
                            .into(),
                    ),
                    version: 4,
                }])
            })
        }
    }

    fn query() -> ReadQuery {
        ReadQuery {
            company: "acme-co".into(),
            page_key: "people-directory".into(),
            user: "user-1".into(),
            base: "/holdings/acme/people".into(),
            allowed_widget_ids: BTreeSet::from(["w1".to_owned()]),
            can_publish: false,
        }
    }

    #[tokio::test]
    async fn handler_validates_then_projects() {
        let port = Fixed {
            seen: Arc::new(Mutex::new(Vec::new())),
        };
        let value = handle(&port, query()).await.unwrap();
        assert_eq!(value["organization"]["widgets"][0]["id"], "w1");
        assert_eq!(value["organizationVersion"], 4);
        assert_eq!(value["canPublish"], false);
        assert_eq!(port.seen.lock().unwrap().as_slice(), ["people-directory"]);
    }

    #[tokio::test]
    async fn handler_rejects_unknown_page_before_reading() {
        let port = Fixed {
            seen: Arc::new(Mutex::new(Vec::new())),
        };
        let mut bad = query();
        bad.page_key = "not-a-page".into();
        assert!(handle(&port, bad).await.is_err());
        assert!(port.seen.lock().unwrap().is_empty());
    }
}
