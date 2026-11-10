//! Company-onboarding command facts owned by Manage ERPX.
//!
//! The command is a pre-Company operation: the current verified Account claims
//! only its own reserved slot and derives the Company from that slot. The caller
//! cannot name a Holding, Company, assignment or role, so no cross-Company
//! authority is accepted or granted here.
use crate::domain::onboarding::{CompanySlotState, cp_len, js_trim};
use erpx_server_shared::public::{BusinessError, FailureKind, UserId};
use serde_json::{Value, json};

const NAME_MINIMUM: usize = 2;
const NAME_MAXIMUM: usize = 100;
const SLUG_TAIL_MINIMUM: usize = 2;
const SLUG_TAIL_MAXIMUM: usize = 39;
const BINDING_MINIMUM: usize = 3;
const BINDING_MAXIMUM: usize = 61;
const HOLDING_ID_MINIMUM: usize = 3;
const HOLDING_ID_MAXIMUM: usize = 24;
const OWNER_ASSIGNMENT_DAYS: i64 = 90;

fn validation_failed() -> BusinessError {
    BusinessError::invalid(
        "VALIDATION_FAILED",
        "Provide a company name, branch name and company URL.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANAGE_ERPX_CONFIGURATION_INVALID",
        "The Manage ERPX onboarding configuration is unavailable.",
    )
}

/// The exact retained POST body schema:
/// `companyName` and `branchName` are trimmed then bounded at 2..=100 Unicode
/// code points; `slug` matches `^[a-z][a-z0-9-]{2,39}$` without trimming.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnboardingCommandInput {
    pub company_name: String,
    pub branch_name: String,
    pub slug: String,
}

impl OnboardingCommandInput {
    pub fn parse(company_name: &str, branch_name: &str, slug: &str) -> Result<Self, BusinessError> {
        let company_name = valid_name(company_name).ok_or_else(validation_failed)?;
        let branch_name = valid_name(branch_name).ok_or_else(validation_failed)?;
        if !valid_slug(slug) {
            return Err(validation_failed());
        }
        Ok(Self {
            company_name,
            branch_name,
            slug: slug.to_owned(),
        })
    }

    /// Read the retained POST body. Missing or non-string fields fail with the
    /// same bounded validation error as the retired Zod schema; unknown fields
    /// are ignored, as Zod object parsing does.
    pub fn from_json(body: &Value) -> Result<Self, BusinessError> {
        let field = |name: &str| {
            body.get(name)
                .and_then(Value::as_str)
                .ok_or_else(validation_failed)
        };
        Self::parse(field("companyName")?, field("branchName")?, field("slug")?)
    }
}

fn valid_name(value: &str) -> Option<String> {
    let trimmed = js_trim(value);
    let length = cp_len(trimmed);
    // The retained Zod schema does not reject control characters, but the
    // strict onboarding read does. Reject them here so a command can never
    // persist a slot that the native reader must refuse.
    if length < NAME_MINIMUM || length > NAME_MAXIMUM || trimmed.chars().any(char::is_control) {
        return None;
    }
    if value.len() > 4096 {
        return None;
    }
    Some(trimmed.to_owned())
}

fn valid_slug(value: &str) -> bool {
    let bytes = value.as_bytes();
    let Some(first) = bytes.first() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    let tail = &bytes[1..];
    (SLUG_TAIL_MINIMUM..=SLUG_TAIL_MAXIMUM).contains(&tail.len())
        && tail
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

/// The current Account witness supplied by the composition after it verified
/// the request session. Manage ERPX rechecks verification but never mints an
/// Account, Company or Holding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnboardingCommandActor {
    pub user_id: UserId,
    pub display_name: String,
    pub email: String,
    pub email_verified: bool,
}

impl OnboardingCommandActor {
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

    /// Bounded authority evidence retained on the receipt, audit and outbox.
    /// It records the acting Account only; it is not a Company permission.
    pub fn authority_json(&self) -> String {
        json!({
            "userId": self.user_id.as_str(),
            "emailVerified": true,
            "scope": "platform_account",
        })
        .to_string()
    }
}

/// A persisted `beta_slots` row. Malformed source rows fail closed rather than
/// defaulting to a company.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnboardingSlotFacts {
    pub holding_id: String,
    pub slug: String,
    pub company_name: String,
    pub branch_name: String,
    pub state: CompanySlotState,
}

impl OnboardingSlotFacts {
    pub fn from_row(
        holding_id: String,
        slug: String,
        company_name: String,
        branch_name: String,
        state: &str,
    ) -> Result<Self, BusinessError> {
        let state = CompanySlotState::parse(state).ok_or_else(source_invalid)?;
        if !valid_holding_id(&holding_id) {
            return Err(source_invalid());
        }
        Ok(Self {
            holding_id,
            slug,
            company_name,
            branch_name,
            state,
        })
    }

    pub fn matches(&self, input: &OnboardingCommandInput) -> bool {
        self.slug == input.slug
            && self.company_name == input.company_name
            && self.branch_name == input.branch_name
    }
}

fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ONBOARDING_SOURCE_INVALID",
        "The reserved company slot is unavailable. Retry the current onboarding state.",
    )
}

fn valid_holding_id(value: &str) -> bool {
    (HOLDING_ID_MINIMUM..=HOLDING_ID_MAXIMUM).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

/// A registered beta resource slot selected by Holding identifier. Bindings are
/// deployment facts validated here before any receipt is written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnboardingCommandResource {
    pub holding_id: String,
    pub write_binding: String,
    pub read_binding: String,
    pub files_binding: String,
    pub beta: bool,
}

impl OnboardingCommandResource {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_holding_id(&self.holding_id)
            || ![&self.write_binding, &self.read_binding, &self.files_binding]
                .into_iter()
                .all(|binding| valid_binding(binding))
        {
            return Err(configuration_invalid());
        }
        Ok(())
    }
}

fn valid_binding(value: &str) -> bool {
    let bytes = value.as_bytes();
    let Some(first) = bytes.first() else {
        return false;
    };
    first.is_ascii_uppercase()
        && (BINDING_MINIMUM - 1..=BINDING_MAXIMUM - 1).contains(&bytes[1..].len())
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || *byte == b'_')
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OnboardingCommandLimits {
    pub members: i64,
    pub branches: i64,
    pub ai: i64,
    pub storage: i64,
}

impl OnboardingCommandLimits {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if [self.members, self.branches, self.ai, self.storage]
            .into_iter()
            .all(|value| value > 0)
        {
            Ok(())
        } else {
            Err(configuration_invalid())
        }
    }
}

/// The bounded command result retained by the route and the bootstrap receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnboardingCommandState {
    Active,
    Waitlisted,
}

impl OnboardingCommandState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Waitlisted => "waitlisted",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnboardingCommandReceipt {
    pub state: OnboardingCommandState,
    pub href: Option<String>,
}

impl OnboardingCommandReceipt {
    pub fn active(slug: &str) -> Self {
        Self {
            state: OnboardingCommandState::Active,
            href: Some(format!("/holdings/{slug}/home")),
        }
    }

    pub const fn waitlisted() -> Self {
        Self {
            state: OnboardingCommandState::Waitlisted,
            href: None,
        }
    }

    pub fn value(&self) -> Value {
        json!({"state": self.state.as_str(), "href": self.href})
    }

    pub fn from_receipt_json(value: &Value, expected_href: &str) -> Option<Self> {
        if value.get("state")?.as_str()? != OnboardingCommandState::Active.as_str() {
            return None;
        }
        if value.get("href")?.as_str()? != expected_href {
            return None;
        }
        Some(Self::active_from_href(expected_href))
    }

    fn active_from_href(href: &str) -> Self {
        Self {
            state: OnboardingCommandState::Active,
            href: Some(href.to_owned()),
        }
    }
}

/// Derived identifiers and literals for one company bootstrap. The application
/// derives every value from the validated input, the caller's own slot and the
/// deployment limits; the repository only persists this plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnboardingBootstrapPlan {
    pub command_id: String,
    pub bootstrap_key: String,
    pub holding_id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: String,
    pub person_id: String,
    pub employee_id: String,
    pub engagement_id: String,
    pub assignment_id: String,
    pub principal_id: String,
    pub member_level_id: String,
    pub organization_level_ids: [String; 6],
    pub ceo_position_id: String,
    pub started_event_id: String,
    pub completed_event_id: String,
    pub slug: String,
    pub company_name: String,
    pub branch_name: String,
    pub owner_user_id: UserId,
    pub owner_name: String,
    pub owner_email: String,
    pub limits: OnboardingCommandLimits,
    pub requested_at: i64,
    pub completed_at: i64,
    pub href: String,
    pub authority_json: String,
}

impl OnboardingBootstrapPlan {
    pub fn new(
        slot: &OnboardingSlotFacts,
        actor: &OnboardingCommandActor,
        limits: OnboardingCommandLimits,
        now: i64,
    ) -> Self {
        let holding_id = slot.holding_id.clone();
        let company_id = format!("company-{holding_id}");
        let branch_id = format!("branch-{holding_id}");
        Self {
            command_id: format!("onboarding-bootstrap-{holding_id}"),
            bootstrap_key: format!("bootstrap-{holding_id}"),
            department_id: format!("department-{branch_id}"),
            person_id: format!("person-{}", actor.user_id.as_str()),
            employee_id: format!("employee-{}", actor.user_id.as_str()),
            engagement_id: format!("engagement-{}", actor.user_id.as_str()),
            assignment_id: format!("beta-assignment-{}", actor.user_id.as_str()),
            principal_id: format!("principal-{}", actor.user_id.as_str()),
            member_level_id: format!("level-{company_id}"),
            organization_level_ids: std::array::from_fn(|index| {
                format!("org-level-{company_id}-{}", index + 1)
            }),
            ceo_position_id: format!("company-ceo-{company_id}"),
            started_event_id: format!("company-provisioning-started-{holding_id}"),
            completed_event_id: format!("company-provisioning-completed-{holding_id}"),
            slug: slot.slug.clone(),
            company_name: slot.company_name.clone(),
            branch_name: slot.branch_name.clone(),
            owner_user_id: actor.user_id.clone(),
            owner_name: actor.display_name.clone(),
            owner_email: actor.email.clone(),
            limits,
            requested_at: now,
            completed_at: now + 1,
            href: format!("/holdings/{}/home", slot.slug),
            authority_json: actor.authority_json(),
            company_id,
            branch_id,
            holding_id,
        }
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.holding_id.is_empty()
            || self.company_id.is_empty()
            || self.branch_id.is_empty()
            || self.slug.is_empty()
            || self.href.is_empty()
        {
            return Err(configuration_invalid());
        }
        self.limits.validate()
    }

    /// Canonical replay token for this bootstrap. It is stored on the command
    /// receipt so a replay of different details cannot reuse the receipt.
    pub fn request_hash(&self) -> String {
        json!({
            "holdingId": self.holding_id,
            "companyId": self.company_id,
            "branchId": self.branch_id,
            "slug": self.slug,
            "companyName": self.company_name,
            "branchName": self.branch_name,
            "owner": self.owner_user_id.as_str(),
            "limits": {
                "members": self.limits.members,
                "branches": self.limits.branches,
                "ai": self.limits.ai,
                "storage": self.limits.storage,
            },
        })
        .to_string()
    }

    /// The current assignment row is retained for the owner. `valid_until` is
    /// the retained 90-day owner window from the TypeScript bootstrap.
    pub fn assignment_valid_until(&self) -> i64 {
        self.requested_at + OWNER_ASSIGNMENT_DAYS * 86_400_000
    }
}

/// Control-plane activation written after the holding bootstrap receipt is
/// durable. It is idempotent and never invents bindings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnboardingActivationPlan {
    pub holding_id: String,
    pub slug: String,
    pub company_name: String,
    pub user_id: UserId,
    pub display_name: String,
    pub principal_id: String,
    pub write_binding: String,
    pub read_binding: String,
    pub files_binding: String,
    pub now: i64,
}

impl OnboardingActivationPlan {
    pub fn new(
        slot: &OnboardingSlotFacts,
        actor: &OnboardingCommandActor,
        resource: &OnboardingCommandResource,
        now: i64,
    ) -> Self {
        Self {
            holding_id: slot.holding_id.clone(),
            slug: slot.slug.clone(),
            company_name: slot.company_name.clone(),
            user_id: actor.user_id.clone(),
            display_name: actor.display_name.clone(),
            principal_id: format!("principal-{}", actor.user_id.as_str()),
            write_binding: resource.write_binding.clone(),
            read_binding: resource.read_binding.clone(),
            files_binding: resource.files_binding.clone(),
            now,
        }
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.holding_id.is_empty()
            || self.slug.is_empty()
            || self.company_name.is_empty()
            || self.display_name.is_empty()
            || !valid_binding(&self.write_binding)
            || !valid_binding(&self.read_binding)
            || !valid_binding(&self.files_binding)
        {
            return Err(configuration_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OnboardingBootstrapOutcome {
    pub completed: bool,
    pub replayed: bool,
}

/// Result of the reservation compare-and-set. `Conflict` means the row changed
/// under a concurrent claim and the caller must recover the owner's slot;
/// `Unavailable` means no reserved slot is currently free.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OnboardingClaimOutcome {
    Claimed(OnboardingSlotFacts),
    Unavailable,
    Conflict,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_schema_matches_the_retained_trim_and_codepoint_boundaries() {
        let input =
            OnboardingCommandInput::parse("  Acme Company  ", "\u{00a0}Main\u{00a0}", "acme-1")
                .unwrap();
        assert_eq!(input.company_name, "Acme Company");
        assert_eq!(input.branch_name, "Main");
        assert_eq!(input.slug, "acme-1");

        for (company, branch, slug) in [
            ("A", "Main", "acme"),
            ("Acme", "M", "acme"),
            ("Acme", "Main", "Ac"),
            ("Acme", "Main", "1acme"),
            ("Acme", "Main", "Acme"),
            ("Acme", "Main", "acme_1"),
            ("Acme", "Main", &"a".repeat(41)),
            ("Acme", "Main", " acme"),
            ("Acme", "Main", "acme "),
            ("Acme\u{0007}Co", "Main", "acme"),
            ("Acme", "Main\u{0085}", "acme"),
            ("", "Main", "acme"),
            ("Acme", " ", "acme"),
        ] {
            assert!(
                OnboardingCommandInput::parse(company, branch, slug).is_err(),
                "{company:?}/{branch:?}/{slug:?} must fail"
            );
        }
        assert!(OnboardingCommandInput::parse(&"ก".repeat(100), &"😀".repeat(100), "a12").is_ok());
        assert!(OnboardingCommandInput::parse(&"a".repeat(101), "Main", "a12").is_err());
        assert!(
            OnboardingCommandInput::parse("Acme", "Main", &format!("a{}", "b".repeat(39))).is_ok()
        );
        assert!(
            OnboardingCommandInput::parse("Acme", "Main", &format!("a{}", "b".repeat(40))).is_err()
        );
    }

    #[test]
    fn exact_input_types_reject_whitespace_only_and_untrimmed_slugs() {
        assert_eq!(
            OnboardingCommandInput::parse(" Acme ", " Main ", "acme").unwrap(),
            OnboardingCommandInput {
                company_name: "Acme".into(),
                branch_name: "Main".into(),
                slug: "acme".into(),
            }
        );
        // The slug is never trimmed; a padded slug is rejected as-is.
        assert!(OnboardingCommandInput::parse("Acme", "Main", " acme").is_err());
        assert!(OnboardingCommandInput::parse("Acme", "Main", "acme ").is_err());
        let error = OnboardingCommandInput::parse("Acme", "Main", "Acme").unwrap_err();
        assert_eq!(error.code, "VALIDATION_FAILED");
        assert_eq!(error.kind, FailureKind::InvalidInput);
        assert!(OnboardingCommandInput::parse("Acme", "Main", "acme").is_ok());
    }

    #[test]
    fn json_body_extracts_only_the_three_retained_fields() {
        let value = json!({
            "companyName": " Acme Company ",
            "branchName": "Main",
            "slug": "acme",
            "extra": "ignored",
        });
        let input = OnboardingCommandInput::from_json(&value).unwrap();
        assert_eq!(input.company_name, "Acme Company");
        assert_eq!(input.branch_name, "Main");
        assert_eq!(input.slug, "acme");
        for body in [
            json!({}),
            json!({"companyName": 1, "branchName": "Main", "slug": "acme"}),
            json!({"companyName": "Acme", "branchName": null, "slug": "acme"}),
            json!({"companyName": "Acme", "branchName": "Main"}),
        ] {
            assert_eq!(
                OnboardingCommandInput::from_json(&body).unwrap_err().code,
                "VALIDATION_FAILED"
            );
        }
    }

    #[test]
    fn unverified_commands_are_denied_before_deriving_a_plan() {
        let actor = OnboardingCommandActor {
            user_id: UserId::new("owner").unwrap(),
            display_name: "Owner".into(),
            email: "owner@example.test".into(),
            email_verified: false,
        };
        assert_eq!(actor.require_verified().unwrap_err().code, "VERIFY_EMAIL");
    }

    #[test]
    fn plans_derive_only_the_owners_holding_company_and_slot() {
        let slot = OnboardingSlotFacts::from_row(
            "h001".into(),
            "acme".into(),
            "Acme Company".into(),
            "Main".into(),
            "provisioning",
        )
        .unwrap();
        let actor = OnboardingCommandActor {
            user_id: UserId::new("owner").unwrap(),
            display_name: "Owner".into(),
            email: "owner@example.test".into(),
            email_verified: true,
        };
        let limits = OnboardingCommandLimits {
            members: 20,
            branches: 5,
            ai: 100,
            storage: 1 << 30,
        };
        let plan = OnboardingBootstrapPlan::new(&slot, &actor, limits, 1_700_000_000_000);
        assert_eq!(plan.command_id, "onboarding-bootstrap-h001");
        assert_eq!(plan.company_id, "company-h001");
        assert_eq!(plan.branch_id, "branch-h001");
        assert_eq!(plan.department_id, "department-branch-h001");
        assert_eq!(plan.assignment_id, "beta-assignment-owner");
        assert_eq!(plan.organization_level_ids[0], "org-level-company-h001-1");
        assert_eq!(plan.organization_level_ids[5], "org-level-company-h001-6");
        assert_eq!(plan.completed_at, plan.requested_at + 1);
        assert_eq!(
            plan.assignment_valid_until(),
            plan.requested_at + 90 * 86_400_000
        );
        assert_eq!(plan.href, "/holdings/acme/home");
        assert!(plan.validate().is_ok());

        let resource = OnboardingCommandResource {
            holding_id: "h001".into(),
            write_binding: "H001_DB".into(),
            read_binding: "H001_READ_DB".into(),
            files_binding: "H001_FILES".into(),
            beta: true,
        };
        resource.validate().unwrap();
        let activation = OnboardingActivationPlan::new(&slot, &actor, &resource, 0);
        assert_eq!(activation.principal_id, "principal-owner");
        activation.validate().unwrap();
    }

    #[test]
    fn malformed_slots_and_configuration_fail_closed() {
        for (holding, slug, state) in [
            ("", "acme", "active"),
            ("H001", "acme", "active"),
            ("h001", "acme", "unknown"),
        ] {
            assert!(
                OnboardingSlotFacts::from_row(
                    holding.into(),
                    slug.into(),
                    "Acme".into(),
                    "Main".into(),
                    state
                )
                .is_err()
            );
        }
        assert!(
            OnboardingCommandResource {
                holding_id: "h001".into(),
                write_binding: "h001_db".into(),
                read_binding: "H001_READ_DB".into(),
                files_binding: "H001_FILES".into(),
                beta: true,
            }
            .validate()
            .is_err()
        );
        assert!(
            OnboardingCommandLimits {
                members: 0,
                branches: 1,
                ai: 1,
                storage: 1,
            }
            .validate()
            .is_err()
        );
    }
}
