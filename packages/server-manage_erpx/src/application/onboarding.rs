//! Manage ERPX company-onboarding read use case.
//!
//! The owner read is scoped only by the verified Account's user identifier.
//! It never creates a Holding, Company, assignment or session.
use crate::domain::onboarding::{CompanySlot, OnboardingActor, OnboardingView};
use erpx_server_shared::public::{BusinessError, UserId};
use std::future::Future;

pub trait OnboardingViewReadPort {
    type Error: From<BusinessError> + Send;
    fn owner_slot(
        &mut self,
        user_id: &UserId,
    ) -> impl Future<Output = Result<Option<CompanySlot>, Self::Error>> + Send;
}

pub async fn onboarding_view<R: OnboardingViewReadPort>(
    actor: &OnboardingActor,
    enabled: bool,
    reader: &mut R,
) -> Result<OnboardingView, R::Error> {
    actor.require_verified()?;
    let slot = reader.owner_slot(&actor.user_id).await?;
    Ok(OnboardingView { enabled, slot })
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_shared::public::FailureKind;

    struct Probe {
        calls: usize,
        slot: Option<CompanySlot>,
    }

    impl OnboardingViewReadPort for Probe {
        type Error = BusinessError;
        async fn owner_slot(
            &mut self,
            user_id: &UserId,
        ) -> Result<Option<CompanySlot>, Self::Error> {
            self.calls += 1;
            assert_eq!(user_id.as_str(), "owner");
            Ok(self.slot.clone())
        }
    }

    fn actor(verified: bool) -> OnboardingActor {
        OnboardingActor {
            user_id: UserId::new("owner").unwrap(),
            email_verified: verified,
        }
    }

    #[test]
    fn verified_owner_read_returns_only_the_bound_slot() {
        sqlx::test_block_on(async {
            let slot = CompanySlot::new(
                "acme".into(),
                "Acme Company".into(),
                "Main".into(),
                "active",
            )
            .unwrap();
            let mut reader = Probe {
                calls: 0,
                slot: Some(slot),
            };
            let view = onboarding_view(&actor(true), true, &mut reader)
                .await
                .unwrap();
            assert!(view.enabled);
            assert_eq!(view.slot.unwrap().slug, "acme");
            assert_eq!(reader.calls, 1);
        });
    }

    #[test]
    fn unverified_owner_is_denied_before_the_repository() {
        sqlx::test_block_on(async {
            let mut reader = Probe {
                calls: 0,
                slot: None,
            };
            let error = onboarding_view(&actor(false), true, &mut reader)
                .await
                .unwrap_err();
            assert_eq!(error.kind, FailureKind::Forbidden);
            assert_eq!(error.code, "VERIFY_EMAIL");
            assert_eq!(reader.calls, 0);
        });
    }

    #[test]
    fn an_unbound_owner_preserves_the_closed_public_flag_and_no_slot() {
        sqlx::test_block_on(async {
            let mut reader = Probe {
                calls: 0,
                slot: None,
            };
            let view = onboarding_view(&actor(true), false, &mut reader)
                .await
                .unwrap();
            assert!(!view.enabled);
            assert!(view.slot.is_none());
            assert_eq!(reader.calls, 1);
        });
    }
}
