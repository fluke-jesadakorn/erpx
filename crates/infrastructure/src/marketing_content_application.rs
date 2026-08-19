//! Application-layer adapter for authoritative Marketing content writes.
//!
//! This module is intentionally an adapter only. The SQLx store remains the
//! owner of holding routing, forced-RLS transaction setup, aggregate re-reads,
//! idempotency, optimistic concurrency, and atomic outbox persistence. The
//! application port sees only transport-neutral commands, receipts, and a
//! deliberately redacted repository error.

use async_trait::async_trait;
use erpx_application::{
    ApplicationError, BindDomainCommand, ExpirePreviewCommand, IssueDomainVerificationCommand,
    IssuePreviewCommand, MarketingContentWritePort, MarketingContentWriteReceipt,
    RevokePreviewCommand, SubmitPublishedFormCommand, VerifyDomainCommand,
};

use crate::{
    PersistenceError, SqlxMarketingContentStore,
    marketing_content_persistence::MarketingContentWriteReceipt as PersistenceWriteReceipt,
};

const REDACTED_REPOSITORY_ERROR: &str = "marketing content write failed";

/// SQLx implementation of the application Marketing content write port.
///
/// The adapter owns its own clone of the store so it can be composed as an
/// object-safe application dependency without borrowing a server/router
/// lifetime. `SqlxMarketingContentStore` contains cloneable pool handles; no
/// database connection or credential is copied into command or error values.
#[derive(Clone)]
pub struct SqlxMarketingContentWriteAdapter {
    store: SqlxMarketingContentStore,
}

impl SqlxMarketingContentWriteAdapter {
    /// Construct an adapter from a store clone. The caller retains ownership
    /// of the original store for other read or write paths.
    pub fn new(store: &SqlxMarketingContentStore) -> Self {
        Self {
            store: store.clone(),
        }
    }

    /// Construct an adapter when ownership of an already-cloned store is
    /// available at composition time.
    pub fn from_store(store: SqlxMarketingContentStore) -> Self {
        Self { store }
    }
}

fn repository_error(_: PersistenceError) -> ApplicationError {
    // Do not format PersistenceError here. SQLx errors can contain database
    // URLs, SQL text, bound values, schema details, or other holding data; the
    // application port deliberately exposes none of that information.
    ApplicationError::Repository(REDACTED_REPOSITORY_ERROR.into())
}

fn receipt(
    value: PersistenceWriteReceipt,
) -> Result<MarketingContentWriteReceipt, ApplicationError> {
    if value.aggregate_type.trim().is_empty()
        || value.aggregate_id.is_nil()
        || value.version == 0
        || value.idempotency_key.trim().is_empty()
        || value.event_id.is_nil()
    {
        return Err(ApplicationError::Repository(
            REDACTED_REPOSITORY_ERROR.into(),
        ));
    }

    Ok(MarketingContentWriteReceipt {
        aggregate_type: value.aggregate_type,
        aggregate_id: value.aggregate_id,
        version: value.version,
        idempotency_key: value.idempotency_key,
        event_id: value.event_id,
        replayed: value.replayed,
    })
}

#[async_trait]
impl MarketingContentWritePort for SqlxMarketingContentWriteAdapter {
    async fn submit_published_form(
        &self,
        command: SubmitPublishedFormCommand,
    ) -> Result<MarketingContentWriteReceipt, ApplicationError> {
        self.store
            .submit_form(
                &command.form,
                command.submission_id,
                &command.values,
                command.received_at,
                &command.context,
            )
            .await
            .map_err(repository_error)
            .and_then(receipt)
    }

    async fn issue_preview(
        &self,
        command: IssuePreviewCommand,
    ) -> Result<MarketingContentWriteReceipt, ApplicationError> {
        self.store
            .issue_preview(&command.preview, &command.token_hash, &command.context)
            .await
            .map_err(repository_error)
            .and_then(receipt)
    }

    async fn revoke_preview(
        &self,
        command: RevokePreviewCommand,
    ) -> Result<MarketingContentWriteReceipt, ApplicationError> {
        self.store
            .revoke_preview(
                command.preview_id,
                command.scope,
                command.revoked_at,
                &command.context,
            )
            .await
            .map_err(repository_error)
            .and_then(receipt)
    }

    async fn expire_preview(
        &self,
        command: ExpirePreviewCommand,
    ) -> Result<MarketingContentWriteReceipt, ApplicationError> {
        self.store
            .expire_preview(
                command.preview_id,
                command.scope,
                command.expired_at,
                &command.context,
            )
            .await
            .map_err(repository_error)
            .and_then(receipt)
    }

    async fn issue_domain_verification(
        &self,
        command: IssueDomainVerificationCommand,
    ) -> Result<MarketingContentWriteReceipt, ApplicationError> {
        self.store
            .issue_domain_verification(
                command.domain_id,
                command.scope,
                &command.verification_hash,
                &command.context,
            )
            .await
            .map_err(repository_error)
            .and_then(receipt)
    }

    async fn verify_domain(
        &self,
        command: VerifyDomainCommand,
    ) -> Result<MarketingContentWriteReceipt, ApplicationError> {
        self.store
            .verify_domain(
                command.domain_id,
                command.scope,
                &command.proof_hash,
                &command.context,
            )
            .await
            .map_err(repository_error)
            .and_then(receipt)
    }

    async fn bind_domain(
        &self,
        command: BindDomainCommand,
    ) -> Result<MarketingContentWriteReceipt, ApplicationError> {
        self.store
            .bind_domain(&command.binding, &command.context)
            .await
            .map_err(repository_error)
            .and_then(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn persistence_errors_map_to_one_transport_neutral_redacted_message() {
        let errors = [
            PersistenceError::Configuration("postgres://user:password@host/db"),
            PersistenceError::InvalidData("raw-submitted-value".into()),
            PersistenceError::IdempotencyConflict {
                key: "secret-idempotency-key".into(),
            },
            PersistenceError::OptimisticConcurrency {
                aggregate_type: "marketing_content_form".into(),
                aggregate_id: Uuid::new_v4(),
                expected: Some(1),
                actual: Some(2),
            },
        ];

        for error in errors {
            let mapped = repository_error(error);
            assert_eq!(
                mapped.to_string(),
                "repository error: marketing content write failed"
            );
            assert!(!mapped.to_string().contains("postgres://"));
            assert!(!mapped.to_string().contains("password"));
            assert!(!mapped.to_string().contains("raw-submitted-value"));
            assert!(!mapped.to_string().contains("secret-idempotency-key"));
        }
    }

    #[test]
    fn receipt_conversion_preserves_only_stable_metadata() {
        let event_id = Uuid::new_v4();
        let aggregate_id = Uuid::new_v4();
        let converted = receipt(PersistenceWriteReceipt {
            aggregate_type: "marketing_content_form_submission".into(),
            aggregate_id,
            version: 3,
            idempotency_key: "form-submit-3".into(),
            event_id,
            replayed: true,
        })
        .expect("valid authoritative receipt");

        assert_eq!(
            converted.aggregate_type,
            "marketing_content_form_submission"
        );
        assert_eq!(converted.aggregate_id, aggregate_id);
        assert_eq!(converted.version, 3);
        assert_eq!(converted.idempotency_key, "form-submit-3");
        assert_eq!(converted.event_id, event_id);
        assert!(converted.replayed);
    }

    #[test]
    fn malformed_authoritative_receipts_fail_closed() {
        let valid_id = Uuid::new_v4();
        let valid_receipt =
            |aggregate_id: Uuid, event_id: Uuid, version: u64, idempotency_key: &str| {
                receipt(PersistenceWriteReceipt {
                    aggregate_type: "marketing_content_preview".into(),
                    aggregate_id,
                    version,
                    idempotency_key: idempotency_key.into(),
                    event_id,
                    replayed: false,
                })
            };

        for malformed in [
            valid_receipt(Uuid::nil(), valid_id, 1, "key"),
            valid_receipt(valid_id, Uuid::nil(), 1, "key"),
            valid_receipt(valid_id, valid_id, 0, "key"),
            valid_receipt(valid_id, valid_id, 1, "   "),
            receipt(PersistenceWriteReceipt {
                aggregate_type: "  ".into(),
                aggregate_id: valid_id,
                version: 1,
                idempotency_key: "key".into(),
                event_id: valid_id,
                replayed: false,
            }),
        ] {
            let error = malformed.expect_err("malformed receipt must be rejected");
            assert_eq!(
                error.to_string(),
                "repository error: marketing content write failed"
            );
        }
    }

    #[test]
    fn marketing_write_port_is_object_safe() {
        fn accepts_port(_: &dyn MarketingContentWritePort) {}

        let _ = accepts_port;
    }
}
