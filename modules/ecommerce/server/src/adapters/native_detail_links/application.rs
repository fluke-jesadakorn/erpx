//! Native Ecommerce checkout detail-link application: the SQL-free read port
//! and the reply renderer.
//!
//! Composition injects the owner reader. The handler validates the checkout
//! identity, projects the rows and owns the exact `{fulfillmentLinks,
//! returnLinks}` shape the retired TypeScript repository emitted.

use crate::adapters::native_detail_links::domain::{
    DetailLinks, project, unavailable, validate_checkout,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::{future::Future, pin::Pin};

pub type DetailLinkFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub trait DetailLinkRead: Send + Sync {
    fn links<'a>(&'a self, checkout_id: &'a str) -> DetailLinkFuture<'a, DetailLinks>;
}

/// Validate, read and render the two owner-executed checkout link lists.
pub async fn read_detail_links(
    reader: &dyn DetailLinkRead,
    checkout_id: &str,
) -> Result<Value, BusinessError> {
    validate_checkout(checkout_id)?;
    let links = reader.links(checkout_id).await.map_err(|error| {
        if error.kind == FailureKind::InvalidInput {
            error
        } else {
            unavailable()
        }
    })?;
    let links = project(links)?;
    let fulfillment = links
        .fulfillment
        .into_iter()
        .map(|row| row.0)
        .collect::<Vec<Value>>();
    let returns = links
        .returns
        .into_iter()
        .map(|row| row.0)
        .collect::<Vec<Value>>();
    Ok(json!({ "fulfillmentLinks": fulfillment, "returnLinks": returns }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_detail_links::domain::LinkRow;
    use std::sync::{Arc, Mutex};

    struct Fake {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl DetailLinkRead for Fake {
        fn links<'a>(&'a self, checkout_id: &'a str) -> DetailLinkFuture<'a, DetailLinks> {
            *self.seen.lock().unwrap() = vec![checkout_id.to_owned()];
            Box::pin(async {
                Ok(DetailLinks {
                    fulfillment: vec![LinkRow(json!({
                        "id": "link-1",
                        "checkout_id": "checkout-1",
                        "fulfillment_id": "fulfillment-1",
                        "status": "confirmed",
                    }))],
                    returns: vec![LinkRow(json!({
                        "id": "return-link-1",
                        "checkout_id": "checkout-1",
                        "current_return_status": "approved",
                    }))],
                })
            })
        }
    }

    #[tokio::test]
    async fn handler_pins_the_retired_reply_shape_and_the_checkout_bind() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Fake { seen: seen.clone() };
        let value = read_detail_links(&reader, "checkout-1").await.unwrap();
        assert_eq!(*seen.lock().unwrap(), vec!["checkout-1".to_owned()]);
        assert_eq!(value["fulfillmentLinks"][0]["fulfillment_id"], json!("fulfillment-1"));
        assert_eq!(value["returnLinks"][0]["current_return_status"], json!("approved"));
        assert_eq!(value.as_object().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn handler_fails_closed_before_reading_on_bad_identity() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Fake { seen: seen.clone() };
        let error = read_detail_links(&reader, "bad id").await.unwrap_err();
        assert_eq!(error.code, "ECOMMERCE_DETAIL_LINKS_REQUEST_INVALID");
        assert!(seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn reader_failure_maps_to_the_named_owner_error() {
        struct Broken;
        impl DetailLinkRead for Broken {
            fn links<'a>(&'a self, _: &'a str) -> DetailLinkFuture<'a, DetailLinks> {
                Box::pin(async { Err(unavailable()) })
            }
        }
        let error = read_detail_links(&Broken, "checkout-1").await.unwrap_err();
        assert_eq!(error.code, "ECOMMERCE_DETAIL_LINKS_UNAVAILABLE");
    }
}
