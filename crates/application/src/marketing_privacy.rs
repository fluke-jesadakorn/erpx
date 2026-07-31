//! Transport-neutral Marketing profile privacy commands and ports.
//!
//! This boundary deliberately contains only the profile identifier, tenant
//! scope, and audit correlation metadata. Contact values, persistence rows,
//! SQLx errors, and storage credentials cannot cross it. Callers should use
//! [`authorize_and_erase_marketing_profile`] or
//! [`AuthorizedMarketingProfilePrivacyPort`] so scope validation happens
//! before an implementation is invoked.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_domain::{
    contracts::TenantScope,
    marketing::{MarketingError, MarketingProfileErasure},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ApplicationError;

/// Maximum accepted byte length for a privacy erasure idempotency key.
pub const MAX_MARKETING_PROFILE_ERASURE_IDEMPOTENCY_KEY_BYTES: usize = 128;

const INVALID_COMMAND_MESSAGE: &str = "invalid marketing profile erasure command";
const AUTHORIZATION_DENIED_MESSAGE: &str =
    "marketing profile erasure is not authorized for the requested scope";
const CONFLICT_MESSAGE: &str = "marketing profile erasure conflict";
const UNAVAILABLE_MESSAGE: &str = "marketing profile erasure unavailable";

/// Correlation data carried through the application boundary.
///
/// Both identifiers are opaque UUIDs. A causation identifier is optional for
/// a root command, but a supplied value may never be nil.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingProfileErasureCorrelation {
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
}

impl MarketingProfileErasureCorrelation {
    pub fn new(correlation_id: Uuid, causation_id: Option<Uuid>) -> Result<Self, ApplicationError> {
        if correlation_id.is_nil() || causation_id.is_some_and(|id| id.is_nil()) {
            return Err(redacted_marketing_profile_privacy_error(
                MarketingProfilePrivacyFailure::InvalidCommand,
            ));
        }
        Ok(Self {
            correlation_id,
            causation_id,
        })
    }

    fn validate(&self) -> Result<(), ApplicationError> {
        Self::new(self.correlation_id, self.causation_id).map(|_| ())
    }
}

/// A contact-free, authoritatively scoped profile erasure command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EraseMarketingProfileCommand {
    pub erasure: MarketingProfileErasure,
    pub authorizing_scope: TenantScope,
    pub idempotency_key: String,
    pub correlation: MarketingProfileErasureCorrelation,
}

impl EraseMarketingProfileCommand {
    pub fn new(
        erasure: MarketingProfileErasure,
        authorizing_scope: TenantScope,
        idempotency_key: impl Into<String>,
        correlation: MarketingProfileErasureCorrelation,
    ) -> Result<Self, ApplicationError> {
        let command = Self {
            erasure,
            authorizing_scope,
            idempotency_key: idempotency_key.into(),
            correlation,
        };
        command.validate()?;
        Ok(command)
    }

    /// Re-validates public command fields immediately before delegation.
    /// Commands can be constructed by decoders or struct literals, so the
    /// port must not rely only on constructor validation.
    pub fn validate(&self) -> Result<(), ApplicationError> {
        match self.erasure.validate_for_scope(&self.authorizing_scope) {
            Ok(()) => {}
            Err(MarketingError::ErasureScopeMismatch) => {
                return Err(redacted_marketing_profile_privacy_error(
                    MarketingProfilePrivacyFailure::AuthorizationDenied,
                ));
            }
            Err(_) => {
                return Err(redacted_marketing_profile_privacy_error(
                    MarketingProfilePrivacyFailure::InvalidCommand,
                ));
            }
        }

        validate_idempotency_key(&self.idempotency_key)?;
        self.correlation.validate()
    }

    pub fn correlation_id(&self) -> Uuid {
        self.correlation.correlation_id
    }

    pub fn causation_id(&self) -> Option<Uuid> {
        self.correlation.causation_id
    }
}

/// A privacy-safe receipt that can be encoded by HTTP or Tonic adapters.
///
/// It intentionally excludes the erasure reason and every profile contact,
/// external-reference, dedupe, locale, and attribute value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingProfileErasureReceipt {
    pub profile_id: Uuid,
    pub scope: TenantScope,
    pub erased_at: DateTime<Utc>,
    pub idempotency_key: String,
    pub correlation: MarketingProfileErasureCorrelation,
    pub replayed: bool,
}

/// Stable, redacted failure classes for application and Tonic mapping.
///
/// Variants carry no provider, SQL, tenant, identifier, or contact detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketingProfilePrivacyFailure {
    InvalidCommand,
    AuthorizationDenied,
    NotFound,
    Conflict,
    Unavailable,
}

/// Maps a privacy failure to the existing application error conventions
/// without exposing persistence details or user-provided values.
pub fn redacted_marketing_profile_privacy_error(
    failure: MarketingProfilePrivacyFailure,
) -> ApplicationError {
    match failure {
        MarketingProfilePrivacyFailure::InvalidCommand => {
            ApplicationError::Repository(INVALID_COMMAND_MESSAGE.into())
        }
        MarketingProfilePrivacyFailure::AuthorizationDenied => {
            ApplicationError::Authorization(AUTHORIZATION_DENIED_MESSAGE.into())
        }
        MarketingProfilePrivacyFailure::NotFound => ApplicationError::NotFound,
        MarketingProfilePrivacyFailure::Conflict => {
            ApplicationError::Conflict(CONFLICT_MESSAGE.into())
        }
        MarketingProfilePrivacyFailure::Unavailable => {
            ApplicationError::Repository(UNAVAILABLE_MESSAGE.into())
        }
    }
}

/// The sole implementation hook for Marketing profile erasure.
///
/// Implementations should perform authoritative tenant routing,
/// transactionality, idempotency, audit, outbox, and redacted receipt
/// construction. They must return errors through the fixed application
/// mapping above rather than formatting persistence errors.
#[async_trait]
pub trait MarketingProfilePrivacyPort: Send + Sync {
    async fn erase_profile(
        &self,
        command: EraseMarketingProfileCommand,
    ) -> Result<MarketingProfileErasureReceipt, ApplicationError>;
}

/// Calls any object-safe privacy port only after validating actor scope and
/// all bounded correlation/idempotency fields.
pub async fn authorize_and_erase_marketing_profile(
    port: &dyn MarketingProfilePrivacyPort,
    command: EraseMarketingProfileCommand,
) -> Result<MarketingProfileErasureReceipt, ApplicationError> {
    command.validate()?;
    port.erase_profile(command).await
}

/// A validating decorator for composition roots that want the port itself to
/// enforce the application boundary before reaching a persistence adapter.
pub struct AuthorizedMarketingProfilePrivacyPort<P> {
    implementation: P,
}

impl<P> AuthorizedMarketingProfilePrivacyPort<P> {
    pub fn new(implementation: P) -> Self {
        Self { implementation }
    }
}

#[async_trait]
impl<P> MarketingProfilePrivacyPort for AuthorizedMarketingProfilePrivacyPort<P>
where
    P: MarketingProfilePrivacyPort,
{
    async fn erase_profile(
        &self,
        command: EraseMarketingProfileCommand,
    ) -> Result<MarketingProfileErasureReceipt, ApplicationError> {
        command.validate()?;
        self.implementation.erase_profile(command).await
    }
}

fn validate_idempotency_key(value: &str) -> Result<(), ApplicationError> {
    if value.trim().is_empty()
        || value.len() > MAX_MARKETING_PROFILE_ERASURE_IDEMPOTENCY_KEY_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(redacted_marketing_profile_privacy_error(
            MarketingProfilePrivacyFailure::InvalidCommand,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;
    use erpx_domain::contracts::TenantScope;

    fn ids() -> (Uuid, Uuid, Uuid) {
        (
            Uuid::from_u128(0x6301),
            Uuid::from_u128(0x6302),
            Uuid::from_u128(0x6303),
        )
    }

    fn target_scope() -> TenantScope {
        let (_, company_id, branch_id) = ids();
        TenantScope::Branch {
            holding_id: ids().0,
            company_id,
            branch_id,
        }
    }

    fn erasure() -> MarketingProfileErasure {
        MarketingProfileErasure::new(
            Uuid::from_u128(0x6304),
            target_scope(),
            Utc::now(),
            "customer privacy request",
        )
        .expect("valid erasure")
    }

    fn correlation() -> MarketingProfileErasureCorrelation {
        MarketingProfileErasureCorrelation::new(
            Uuid::from_u128(0x6305),
            Some(Uuid::from_u128(0x6306)),
        )
        .expect("valid correlation")
    }

    fn command(authorizing_scope: TenantScope) -> EraseMarketingProfileCommand {
        EraseMarketingProfileCommand {
            erasure: erasure(),
            authorizing_scope,
            idempotency_key: "marketing-profile-erasure-6307".into(),
            correlation: correlation(),
        }
    }

    #[test]
    fn actor_scope_must_contain_erasure_scope() {
        let (_, company_id, _) = ids();
        let holding = TenantScope::Holding {
            holding_id: ids().0,
        };
        assert!(command(holding).validate().is_ok());

        let company = TenantScope::Company {
            holding_id: ids().0,
            company_id: Uuid::from_u128(0x630a),
        };
        assert!(command(company).validate().is_err());

        let sibling = TenantScope::Branch {
            holding_id: ids().0,
            company_id,
            branch_id: Uuid::from_u128(0x6308),
        };
        let error = command(sibling)
            .validate()
            .expect_err("sibling scope denied");
        assert!(matches!(error, ApplicationError::Authorization(_)));
        assert!(error.to_string().contains(AUTHORIZATION_DENIED_MESSAGE));
    }

    #[test]
    fn invalid_command_bounds_are_redacted() {
        let mut invalid = command(TenantScope::Holding {
            holding_id: ids().0,
        });
        invalid.idempotency_key = " ".into();
        let error = invalid
            .validate()
            .expect_err("blank idempotency key denied");
        assert!(error.to_string().contains(INVALID_COMMAND_MESSAGE));

        let mut invalid = command(TenantScope::Holding {
            holding_id: ids().0,
        });
        invalid.idempotency_key =
            "x".repeat(MAX_MARKETING_PROFILE_ERASURE_IDEMPOTENCY_KEY_BYTES + 1);
        let error = invalid.validate().expect_err("oversized key denied");
        assert!(error.to_string().contains(INVALID_COMMAND_MESSAGE));

        let mut invalid = command(TenantScope::Holding {
            holding_id: ids().0,
        });
        invalid.idempotency_key = "privacy\nrequest".into();
        let error = invalid.validate().expect_err("control character denied");
        assert!(error.to_string().contains(INVALID_COMMAND_MESSAGE));
    }

    #[test]
    fn idempotency_and_correlation_metadata_are_preserved_and_validated() {
        let command = command(TenantScope::Holding {
            holding_id: ids().0,
        });
        command.validate().expect("command validates");
        assert_eq!(command.idempotency_key, "marketing-profile-erasure-6307");
        assert_eq!(command.correlation_id(), Uuid::from_u128(0x6305));
        assert_eq!(command.causation_id(), Some(Uuid::from_u128(0x6306)));

        assert!(MarketingProfileErasureCorrelation::new(Uuid::nil(), None).is_err());
        assert!(
            MarketingProfileErasureCorrelation::new(Uuid::from_u128(0x6309), Some(Uuid::nil()),)
                .is_err()
        );
    }

    #[test]
    fn receipts_and_failures_never_echo_profile_data() {
        let receipt = MarketingProfileErasureReceipt {
            profile_id: Uuid::from_u128(0x6304),
            scope: target_scope(),
            erased_at: Utc::now(),
            idempotency_key: "privacy-erasure-1".into(),
            correlation: correlation(),
            replayed: false,
        };
        let encoded = serde_json::to_string(&receipt).expect("receipt serializes");
        assert!(!encoded.contains("email"));
        assert!(!encoded.contains("phone"));
        assert!(!encoded.contains("reason"));

        let error =
            redacted_marketing_profile_privacy_error(MarketingProfilePrivacyFailure::Unavailable);
        assert!(error.to_string().contains(UNAVAILABLE_MESSAGE));
        assert!(!error.to_string().contains("customer@example.com"));
    }

    struct RecordingPort {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl MarketingProfilePrivacyPort for RecordingPort {
        async fn erase_profile(
            &self,
            command: EraseMarketingProfileCommand,
        ) -> Result<MarketingProfileErasureReceipt, ApplicationError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(MarketingProfileErasureReceipt {
                profile_id: command.erasure.profile_id,
                scope: command.erasure.scope,
                erased_at: command.erasure.erased_at,
                idempotency_key: command.idempotency_key,
                correlation: command.correlation,
                replayed: false,
            })
        }
    }

    #[tokio::test]
    async fn authorized_port_validates_before_delegate() {
        let calls = Arc::new(AtomicUsize::new(0));
        let port = AuthorizedMarketingProfilePrivacyPort::new(RecordingPort {
            calls: calls.clone(),
        });
        let unauthorized = command(TenantScope::Company {
            holding_id: ids().0,
            company_id: Uuid::from_u128(0x630a),
        });
        assert!(port.erase_profile(unauthorized).await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        let authorized = command(TenantScope::Holding {
            holding_id: ids().0,
        });
        let receipt = port
            .erase_profile(authorized)
            .await
            .expect("authorized delegate");
        assert_eq!(receipt.profile_id, Uuid::from_u128(0x6304));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn privacy_port_is_object_safe() {
        fn accepts_object_safe_port(_: &dyn MarketingProfilePrivacyPort) {}
        let port = RecordingPort {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        accepts_object_safe_port(&port);
    }
}
