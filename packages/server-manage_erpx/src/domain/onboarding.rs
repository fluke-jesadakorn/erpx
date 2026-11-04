//! Company-onboarding facts owned by Manage ERPX.
//!
//! The onboarding view is a pre-Company read: the verified Account selects the
//! owner's reserved slot. It grants no Holding, Company or Platform authority.
use erpx_server_runtime::public::http_support::{Value, json};
use erpx_server_shared::public::{BusinessError, FailureKind, UserId};

const SLUG_MAXIMUM: usize = 64;
const NAME_MAXIMUM: usize = 200;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnboardingActor {
    pub user_id: UserId,
    pub email_verified: bool,
}

impl OnboardingActor {
    pub fn require_verified(&self) -> Result<(), BusinessError> {
        if !self.email_verified {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "VERIFY_EMAIL",
                "Verify your email before joining or creating a company.",
            ));
        }
        Ok(())
    }
}

/// The current reserved company slot state. `available` is retained in the
/// source schema but is never returned for an owner because no owner is bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompanySlotState {
    Available,
    Provisioning,
    Active,
}

impl CompanySlotState {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "available" => Some(Self::Available),
            "provisioning" => Some(Self::Provisioning),
            "active" => Some(Self::Active),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Provisioning => "provisioning",
            Self::Active => "active",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompanySlot {
    pub slug: String,
    pub company_name: String,
    pub branch_name: String,
    pub state: CompanySlotState,
}

impl CompanySlot {
    /// Strictly validate a persisted owner slot. Malformed source rows are a
    /// source failure, not an invitation to invent a default company.
    pub fn new(
        slug: String,
        company_name: String,
        branch_name: String,
        state: &str,
    ) -> Result<Self, BusinessError> {
        let valid_slug = !slug.is_empty()
            && slug.len() <= SLUG_MAXIMUM
            && slug.as_bytes()[0].is_ascii_lowercase()
            && slug
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        if !valid_slug {
            return Err(invalid_source());
        }
        if !valid_name(&company_name) || !valid_name(&branch_name) {
            return Err(invalid_source());
        }
        let state = CompanySlotState::parse(state).ok_or_else(invalid_source)?;
        Ok(Self {
            slug,
            company_name,
            branch_name,
            state,
        })
    }

    pub fn value(&self) -> Value {
        json!({
            "slug": self.slug,
            "company_name": self.company_name,
            "branch_name": self.branch_name,
            "state": self.state.as_str(),
        })
    }
}

fn valid_name(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= NAME_MAXIMUM
        && value.len() <= 4096
        && !value.chars().any(char::is_control)
}

fn invalid_source() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ONBOARDING_SOURCE_INVALID",
        "The reserved company slot is unavailable. Retry the current onboarding state.",
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnboardingView {
    /// Public registration remains a deployment configuration fact.
    pub enabled: bool,
    pub slot: Option<CompanySlot>,
}

impl OnboardingView {
    pub fn value(&self) -> Value {
        json!({
            "enabled": self.enabled,
            "slot": self.slot.as_ref().map(CompanySlot::value).unwrap_or(Value::Null),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_serializes_the_exact_owner_shape_without_holding_identifiers() {
        let slot = CompanySlot::new(
            "acme".into(),
            "Acme Company".into(),
            "Main".into(),
            "provisioning",
        )
        .unwrap();
        let view = OnboardingView {
            enabled: true,
            slot: Some(slot),
        };
        assert_eq!(
            view.value(),
            json!({
                "enabled": true,
                "slot": {
                    "slug": "acme",
                    "company_name": "Acme Company",
                    "branch_name": "Main",
                    "state": "provisioning",
                },
            })
        );
        assert_eq!(
            OnboardingView {
                enabled: false,
                slot: None,
            }
            .value(),
            json!({"enabled": false, "slot": null})
        );
    }

    #[test]
    fn malformed_slots_fail_closed_instead_of_defaulting() {
        for (slug, company, branch, state) in [
            ("", "Acme", "Main", "active"),
            ("Acme", "Acme", "Main", "active"),
            ("acme space", "Acme", "Main", "active"),
            ("acme", " ", "Main", "active"),
            ("acme", "Acme", "", "active"),
            ("acme", "Acme", "Main", "unknown"),
        ] {
            assert!(
                CompanySlot::new(slug.into(), company.into(), branch.into(), state).is_err(),
                "{slug:?}/{company:?}/{branch:?}/{state:?} must fail"
            );
        }
        assert!(CompanySlot::new("acme".into(), "a".repeat(201), "Main".into(), "active").is_err());
        assert!(CompanySlot::new("acme".into(), "ก".repeat(100), "สาขา".into(), "active").is_ok());
        assert!(CompanySlot::new("acme".into(), "Acme".into(), "Main".into(), "available").is_ok());
    }

    #[test]
    fn unverified_accounts_are_denied_before_any_owner_read() {
        let actor = OnboardingActor {
            user_id: UserId::new("owner").unwrap(),
            email_verified: false,
        };
        assert_eq!(actor.require_verified().unwrap_err().code, "VERIFY_EMAIL");
        assert!(
            OnboardingActor {
                user_id: UserId::new("owner").unwrap(),
                email_verified: true,
            }
            .require_verified()
            .is_ok()
        );
    }
}
