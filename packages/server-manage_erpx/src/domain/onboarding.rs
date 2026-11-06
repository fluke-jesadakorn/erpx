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

/// ECMAScript `String.prototype.trim` whitespace: WhiteSpace plus
/// LineTerminator. Rust's `str::trim` additionally strips U+0085 NEL
/// (Unicode White_Space, not JavaScript whitespace) and keeps U+FEFF,
/// so the exact set is spelled out to match the retained Zod `trim()`
/// boundary of the onboarding schema.
pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(|c: char| {
        matches!(
            c,
            '\u{0009}'..='\u{000d}'
                | '\u{0020}'
                | '\u{00a0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
        )
    })
}

/// Zod string limits count JavaScript UTF-16 code units (`String.length`),
/// not Unicode scalar values or UTF-8 bytes. Thai text (3 bytes, 1 unit)
/// and astral text (surrogate pairs: 4 bytes, 2 units) must match the
/// retained schema boundary.
pub(crate) fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

fn valid_name(value: &str) -> bool {
    let trimmed = js_trim(value);
    !trimmed.is_empty()
        && utf16_len(trimmed) <= NAME_MAXIMUM
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
    fn company_and_branch_names_count_js_utf16_units_after_ecma_trim() {
        // Astral code points count as two UTF-16 units, as in the retained
        // Zod schema: 100 emoji are 200 units and pass, 101 exceed the bound
        // where a scalar count would accept them.
        assert!(CompanySlot::new("acme".into(), "😀".repeat(100), "Main".into(), "active").is_ok());
        assert!(
            CompanySlot::new("acme".into(), "😀".repeat(101), "Main".into(), "active").is_err()
        );
        // Thai text counts one unit per code point regardless of UTF-8 width.
        assert!(CompanySlot::new("acme".into(), "ก".repeat(200), "Main".into(), "active").is_ok());
        assert!(CompanySlot::new("acme".into(), "ก".repeat(201), "Main".into(), "active").is_err());
        // ECMAScript trim strips U+FEFF and trims before measuring, so a
        // padded 200-unit name passes while a FEFF-only name is empty.
        assert!(
            CompanySlot::new(
                "acme".into(),
                format!("\u{feff}{}\u{feff}", "a".repeat(200)),
                "Main".into(),
                "active"
            )
            .is_ok()
        );
        assert!(
            CompanySlot::new("acme".into(), "\u{feff}".into(), "Main".into(), "active").is_err()
        );
        assert!(
            CompanySlot::new(
                "acme".into(),
                "\u{00a0}Acme\u{00a0}".into(),
                "Main".into(),
                "active"
            )
            .is_ok()
        );
        // JS trim does not strip NEL (U+0085): it remains counted and is a
        // control character, so Rust must reject rather than trim and accept.
        assert!(
            CompanySlot::new(
                "acme".into(),
                format!("\u{0085}{}", "a".repeat(199)),
                "Main".into(),
                "active"
            )
            .is_err()
        );
        // Interior controls remain rejected.
        assert!(
            CompanySlot::new(
                "acme".into(),
                "Acme\u{0007}Co".into(),
                "Main".into(),
                "active"
            )
            .is_err()
        );
        // Branch names share the same boundary.
        assert!(
            CompanySlot::new("acme".into(), "Acme".into(), "😀".repeat(101), "active").is_err()
        );
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
