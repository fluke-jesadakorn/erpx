//! SQLx adapter for the application Marketing profile privacy port.
//!
//! The adapter owns no authorization policy beyond re-validating the
//! transport-neutral command. The application command has already proven that
//! the actor scope contains the target scope; this layer supplies the routed
//! write transaction and maps persistence details to redacted failures.

use async_trait::async_trait;
use erpx_application::{
    ApplicationError, EraseMarketingProfileCommand, MarketingProfileErasureReceipt,
    MarketingProfilePrivacyFailure, MarketingProfilePrivacyPort,
    redacted_marketing_profile_privacy_error,
};

use crate::marketing_persistence::MarketingMutationReceipt;
use crate::{PersistenceError, SqlxMarketingStore};
use uuid::Uuid;

const REDACTED_PROFILE_PRIVACY_ERROR: &str = "marketing profile privacy operation failed";

#[derive(Clone)]
pub struct SqlxMarketingProfilePrivacyAdapter {
    store: SqlxMarketingStore,
}

impl SqlxMarketingProfilePrivacyAdapter {
    pub fn new(store: &SqlxMarketingStore) -> Self {
        Self {
            store: store.clone(),
        }
    }

    pub fn from_store(store: SqlxMarketingStore) -> Self {
        Self { store }
    }
}

fn repository_error(error: PersistenceError) -> ApplicationError {
    let failure = match error {
        PersistenceError::NotFound { .. } => MarketingProfilePrivacyFailure::NotFound,
        PersistenceError::IdempotencyConflict { .. }
        | PersistenceError::OptimisticConcurrency { .. }
        | PersistenceError::InvalidData(_) => MarketingProfilePrivacyFailure::Conflict,
        PersistenceError::Database(_)
        | PersistenceError::Migration(_)
        | PersistenceError::Serialization(_)
        | PersistenceError::ClaimLost { .. }
        | PersistenceError::Configuration(_) => MarketingProfilePrivacyFailure::Unavailable,
    };
    let mapped = redacted_marketing_profile_privacy_error(failure);
    if matches!(failure, MarketingProfilePrivacyFailure::Unavailable) {
        ApplicationError::Repository(REDACTED_PROFILE_PRIVACY_ERROR.into())
    } else {
        mapped
    }
}

#[async_trait]
impl MarketingProfilePrivacyPort for SqlxMarketingProfilePrivacyAdapter {
    async fn erase_profile(
        &self,
        command: EraseMarketingProfileCommand,
    ) -> Result<MarketingProfileErasureReceipt, ApplicationError> {
        command.validate()?;
        let receipt = self
            .store
            .erase_profile(&command.erasure, &command.idempotency_key)
            .await
            .map_err(repository_error)?;
        validate_erasure_receipt(&receipt, command.erasure.profile_id)?;
        Ok(MarketingProfileErasureReceipt {
            profile_id: command.erasure.profile_id,
            scope: command.erasure.scope,
            erased_at: command.erasure.erased_at,
            idempotency_key: command.idempotency_key,
            correlation: command.correlation,
            replayed: receipt.replayed,
        })
    }
}

/// A privacy adapter must never turn a misrouted or malformed persistence
/// response into a successful erasure receipt. The store owns the source
/// transaction; this boundary still verifies the immutable resource identity
/// and positive source version before returning to the application layer.
fn validate_erasure_receipt(
    receipt: &MarketingMutationReceipt,
    expected_profile_id: Uuid,
) -> Result<(), ApplicationError> {
    if receipt.resource_id != expected_profile_id
        || receipt.version.is_none_or(|version| version == 0)
    {
        return Err(ApplicationError::Repository(
            REDACTED_PROFILE_PRIVACY_ERROR.into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistence_details_never_cross_the_privacy_adapter() {
        let error = repository_error(PersistenceError::Database(sqlx::Error::Protocol(
            "postgres://user:secret@db/internal".into(),
        )));
        assert_eq!(
            error.to_string(),
            "repository error: marketing profile privacy operation failed"
        );
        assert!(!error.to_string().contains("postgres"));
        assert!(!error.to_string().contains("secret"));
    }

    #[test]
    fn state_conflicts_map_to_stable_conflict() {
        let error = repository_error(PersistenceError::InvalidData(
            "raw customer email must not escape".into(),
        ));
        assert_eq!(
            error.to_string(),
            "conflict: marketing profile erasure conflict"
        );
        assert!(!error.to_string().contains("email"));
    }

    #[test]
    fn erasure_receipts_fail_closed_on_identity_or_version_drift() {
        let profile_id = Uuid::from_u128(1);
        let wrong_resource = MarketingMutationReceipt {
            resource_id: Uuid::from_u128(2),
            version: Some(1),
            replayed: false,
        };
        let missing_version = MarketingMutationReceipt {
            resource_id: profile_id,
            version: None,
            replayed: true,
        };

        for receipt in [&wrong_resource, &missing_version] {
            let error = validate_erasure_receipt(receipt, profile_id)
                .expect_err("receipt integrity drift must fail closed");
            assert_eq!(
                error.to_string(),
                "repository error: marketing profile privacy operation failed"
            );
        }

        let valid = MarketingMutationReceipt {
            resource_id: profile_id,
            version: Some(1),
            replayed: true,
        };
        validate_erasure_receipt(&valid, profile_id).expect("valid replay receipt");
    }
}
