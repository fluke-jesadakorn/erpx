//! Application port and handler for the native beta feedback read.
//!
//! No SQL lives here: the sqlx adapter implements the owner port and this
//! handler only enforces the Company boundary and the response invariants.

use crate::adapters::native_lane_34::domain::{
    EmailReceipt, FeedbackPage, InvitationPage, invalid_request, valid_company_id,
    valid_delivery_key,
};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type FeedbackFuture<'a> =
    Pin<Box<dyn Future<Output = Result<FeedbackPage, BusinessError>> + Send + 'a>>;

/// Owner read port. The Postgres adapter owns the SQL, the bind order and the
/// transaction; the HTTP router only transports the validated result.
pub trait BetaFeedbackQuery: Send + Sync {
    fn list<'a>(&'a self, company: &'a str) -> FeedbackFuture<'a>;
}

/// Validate the Company scope, run the owner read and pin the projection.
pub async fn load(
    query: &dyn BetaFeedbackQuery,
    company: &str,
) -> Result<FeedbackPage, BusinessError> {
    if !valid_company_id(company) {
        return Err(invalid_request());
    }
    query.list(company).await?.validated()
}

pub type InvitationFuture<'a> =
    Pin<Box<dyn Future<Output = Result<InvitationPage, BusinessError>> + Send + 'a>>;

/// Owner invitation-list port. The sqlx adapter owns the company-scoped join,
/// bind order and transaction; this handler only enforces the Company boundary.
pub trait BetaInvitationQuery: Send + Sync {
    fn list<'a>(&'a self, company: &'a str) -> InvitationFuture<'a>;
}

/// Validate the Company scope, run the owner read and pin the invitation shape.
pub async fn load_invitations(
    query: &dyn BetaInvitationQuery,
    company: &str,
) -> Result<InvitationPage, BusinessError> {
    if !valid_company_id(company) {
        return Err(invalid_request());
    }
    query.list(company).await?.validated()
}

pub type EmailReceiptFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<EmailReceipt>, BusinessError>> + Send + 'a>>;

/// Owner sandbox email receipt port. The sqlx adapter owns the single
/// `delivery_key` bind; this handler only enforces the key boundary and pins
/// the returned projection.
pub trait EmailReceiptQuery: Send + Sync {
    fn receipt<'a>(&'a self, delivery_key: &'a str) -> EmailReceiptFuture<'a>;
}

/// Validate the delivery key, run the owner read and pin the receipt shape.
pub async fn load_email_receipt(
    query: &dyn EmailReceiptQuery,
    delivery_key: &str,
) -> Result<Option<EmailReceipt>, BusinessError> {
    if !valid_delivery_key(delivery_key) {
        return Err(invalid_request());
    }
    query
        .receipt(delivery_key)
        .await?
        .map(|receipt| receipt.validated())
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_34::domain::{FeedbackItem, unavailable};

    struct Stub {
        fail: bool,
    }

    impl BetaFeedbackQuery for Stub {
        fn list<'a>(&'a self, company: &'a str) -> FeedbackFuture<'a> {
            let company = company.to_owned();
            let fail = self.fail;
            Box::pin(async move {
                if fail {
                    return Err(unavailable());
                }
                Ok(FeedbackPage {
                    items: vec![FeedbackItem {
                        id: format!("fb-{company}"),
                        body: "hello".to_owned(),
                        created_at: 1_700_000_000_000,
                    }],
                })
            })
        }
    }

    fn stub(fail: bool) -> Stub {
        Stub { fail }
    }

    #[tokio::test]
    async fn load_returns_the_pinned_owner_projection() {
        let page = load(&stub(false), "acme:co").await.expect("page");
        assert_eq!(
            page,
            FeedbackPage {
                items: vec![FeedbackItem {
                    id: "fb-acme:co".to_owned(),
                    body: "hello".to_owned(),
                    created_at: 1_700_000_000_000,
                }],
            }
        );
    }

    #[tokio::test]
    async fn load_fails_closed_on_invalid_company() {
        assert_eq!(
            load(&stub(false), "bad/id").await.unwrap_err(),
            invalid_request()
        );
        assert_eq!(load(&stub(false), "").await.unwrap_err(), invalid_request());
    }

    #[tokio::test]
    async fn load_fails_closed_when_owner_read_fails() {
        assert_eq!(
            load(&stub(true), "acme:co").await.unwrap_err(),
            unavailable()
        );
    }

    struct InvitationStub {
        fail: bool,
    }

    impl BetaInvitationQuery for InvitationStub {
        fn list<'a>(&'a self, company: &'a str) -> InvitationFuture<'a> {
            let company = company.to_owned();
            let fail = self.fail;
            Box::pin(async move {
                if fail {
                    return Err(unavailable());
                }
                Ok(InvitationPage {
                    items: vec![crate::adapters::native_lane_34::domain::InvitationItem {
                        id: format!("inv-{company}"),
                        email: "owner@example.test".to_owned(),
                        branch_id: "branch-1".to_owned(),
                        expires_at: 1_700_000_000_000,
                        accepted_at: None,
                        revoked_at: None,
                        invitation_state: "Invited".to_owned(),
                    }],
                })
            })
        }
    }

    #[tokio::test]
    async fn load_invitations_returns_the_pinned_projection() {
        let page = load_invitations(&InvitationStub { fail: false }, "acme:co")
            .await
            .expect("page");
        assert_eq!(page.items[0].id, "inv-acme:co");
        assert_eq!(page.items[0].invitation_state, "Invited");
    }

    #[tokio::test]
    async fn load_invitations_fails_closed_on_bad_company_and_owner_failure() {
        assert_eq!(
            load_invitations(&InvitationStub { fail: false }, "bad/id")
                .await
                .unwrap_err(),
            invalid_request()
        );
        assert_eq!(
            load_invitations(&InvitationStub { fail: true }, "acme:co")
                .await
                .unwrap_err(),
            unavailable()
        );
    }

    struct ReceiptStub {
        fail: bool,
    }

    impl EmailReceiptQuery for ReceiptStub {
        fn receipt<'a>(&'a self, key: &'a str) -> EmailReceiptFuture<'a> {
            let fail = self.fail;
            let bad = key == "badshape:receipt";
            Box::pin(async move {
                if fail {
                    return Err(unavailable());
                }
                if bad {
                    return Ok(Some(EmailReceipt {
                        delivery_key: String::new(),
                        recipient: "to@example.test".to_owned(),
                        subject: "S".to_owned(),
                        body_text: "body".to_owned(),
                        body_ciphertext: None,
                        provider_id: "sandbox-key-1".to_owned(),
                    }));
                }
                Ok(Some(EmailReceipt {
                    delivery_key: format!("{key}-receipt"),
                    recipient: "to@example.test".to_owned(),
                    subject: "S".to_owned(),
                    body_text: "body".to_owned(),
                    body_ciphertext: None,
                    provider_id: "sandbox-key-1".to_owned(),
                }))
            })
        }
    }

    #[tokio::test]
    async fn load_email_receipt_returns_the_pinned_projection() {
        let receipt = load_email_receipt(&ReceiptStub { fail: false }, "portal:key")
            .await
            .expect("receipt")
            .expect("some");
        assert_eq!(receipt.delivery_key, "portal:key-receipt");
        assert_eq!(receipt.recipient, "to@example.test");
        assert_eq!(receipt.body_ciphertext, None);
    }

    #[tokio::test]
    async fn load_email_receipt_fails_closed_on_bad_key_owner_failure_and_shape() {
        assert_eq!(
            load_email_receipt(&ReceiptStub { fail: false }, "bad/key")
                .await
                .unwrap_err(),
            invalid_request()
        );
        assert_eq!(
            load_email_receipt(&ReceiptStub { fail: true }, "portal:key")
                .await
                .unwrap_err(),
            unavailable()
        );
        assert_eq!(
            load_email_receipt(&ReceiptStub { fail: false }, "badshape:receipt")
                .await
                .unwrap_err(),
            unavailable()
        );
    }
}
