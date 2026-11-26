//! Native PMS PromptPay payment-status use case.
//!
//! The repository is a bounded, read-only PMS port. Composition supplies the
//! Company/Payment selection and resolves the session, property scope and any
//! owner provider reconcile before this use case reads the authoritative row;
//! this use case applies validation and projects the complete `visible()`
//! payload. No SQL, lock, provider call or cross-owner value is involved.
use crate::domain::payment_status::{
    PmsPaymentStatusFacts, PmsPaymentStatusView, PaymentStatusRow, not_found, project, validate,
};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type PmsPaymentStatusFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Read-only PMS payment-status repository. One scoped method, no authority of
/// its own. A foreign Company yields `None`, which the use case maps to the
/// retained `PMS_PAYMENT_NOT_FOUND` failure.
pub trait PmsPaymentStatusRepository: Send {
    type Error: From<BusinessError> + Send;

    fn status<'a>(
        &'a mut self,
        facts: &'a PmsPaymentStatusFacts,
    ) -> PmsPaymentStatusFuture<'a, Option<PaymentStatusRow>, Self::Error>;
}

/// Retained `pmsStripePaymentStatus` owner read. The produced view is already
/// the complete response object; the caller does not append or reshape fields.
pub async fn pms_payment_status<R: PmsPaymentStatusRepository>(
    facts: &PmsPaymentStatusFacts,
    repository: &mut R,
) -> Result<PmsPaymentStatusView, R::Error> {
    validate(facts)?;
    let row = repository.status(facts).await?.ok_or_else(not_found)?;
    Ok(project(row))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake(Option<PaymentStatusRow>);

    impl PmsPaymentStatusRepository for Fake {
        type Error = BusinessError;

        fn status<'a>(
            &'a mut self,
            _facts: &'a PmsPaymentStatusFacts,
        ) -> PmsPaymentStatusFuture<'a, Option<PaymentStatusRow>, Self::Error> {
            let row = self.0.clone();
            Box::pin(async move { Ok(row) })
        }
    }

    fn facts() -> PmsPaymentStatusFacts {
        PmsPaymentStatusFacts {
            company_id: "company".into(),
            id: "payment-1".into(),
        }
    }

    fn row() -> PaymentStatusRow {
        PaymentStatusRow {
            id: "payment-1".into(),
            company_id: "company".into(),
            property_id: "property-1".into(),
            state: "pending".into(),
            kind: "deposit".into(),
            amount_minor: 2500,
            payer_email: Some("guest@example.test".into()),
            provider_id: Some("pi_1".into()),
            qr_data: Some("qr".into()),
            expires_at: 42,
            entry_id: None,
            refund_state: None,
        }
    }

    #[tokio::test]
    async fn full_payload_parity() {
        let mut repo = Fake(Some(row()));
        let view = pms_payment_status(&facts(), &mut repo).await.unwrap();
        assert_eq!(view.id, "payment-1");
        assert_eq!(view.state, "pending");
        assert_eq!(view.kind, "deposit");
        assert_eq!(view.amount_minor, 2500);
        assert_eq!(view.qr_data.as_deref(), Some("qr"));
        assert_eq!(view.expires_at, 42);
        assert!(view.payer_email_provided);
        let value = serde_json::to_value(&view).unwrap();
        assert_eq!(value["amountMinor"], 2500);
        assert!(value.get("payer_email_provided").is_none());
    }

    #[tokio::test]
    async fn missing_or_foreign_payment_fails_with_retained_code() {
        let mut repo = Fake(None);
        assert_eq!(
            pms_payment_status(&facts(), &mut repo).await.unwrap_err().code,
            "PMS_PAYMENT_NOT_FOUND"
        );
    }

    #[tokio::test]
    async fn invalid_id_is_rejected() {
        let mut repo = Fake(Some(row()));
        let bad = PmsPaymentStatusFacts {
            company_id: "company".into(),
            id: "x".repeat(201),
        };
        assert_eq!(
            pms_payment_status(&bad, &mut repo).await.unwrap_err().code,
            "PMS_PAYMENT_STATUS_REQUEST_INVALID"
        );
    }
}
