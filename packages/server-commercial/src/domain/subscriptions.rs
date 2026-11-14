//! Retained Commercial subscription and grace/recovery reads.
//!
//! There is no separate subscription table in the Commercial owner. The
//! retained TypeScript derives subscription access from the Company package
//! grant: `usable` is `active` and not past `endsAt`; expiry behaves as
//! `draining` for new work while retaining settlement/history until an explicit
//! owner-reviewed inactivation. This module ports those exact predicates and
//! derives recovery only from the retained grant-event history.

use crate::domain::packages::{CompanyPackageGrant, CompanyPackageHistoryRight};
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use serde::{Deserialize, Serialize};

pub const MAX_SUBSCRIPTIONS: usize = 1024;
pub const MAX_SUBSCRIPTION_HISTORY: usize = 4096;
pub const MAX_SUBSCRIPTION_TRANSITIONS: usize = 4096;
const MAX_SAFE: i64 = 9_007_199_254_740_991;

/// Current Commercial subscription phase for one Company package grant.
/// `Grace` is the retained expiry-to-draining behaviour. `Recovered` is an
/// active period whose immediately preceding retained transition was draining
/// or inactive; it is still subject to the normal grant/dependency rules.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum SubscriptionState {
    Active,
    Grace,
    Expired,
    Recovered,
}

/// The only retained forward action. A grace/expired period can be restored by
/// a normal reviewed grant change; an active period needs no Commercial action.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum SubscriptionAction {
    Restore,
}

/// The retained owner defines no bounded grace end: expiry/draining retain
/// settlement and history until an explicit inactivation. `ends_at` and
/// `remaining_ms` therefore stay null rather than inventing a countdown.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct GraceWindow {
    pub starts_at: i64,
    pub ends_at: Option<i64>,
    pub remaining_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct CompanySubscription {
    pub package_id: String,
    pub package_version: i64,
    pub grant_state: String,
    pub state: SubscriptionState,
    pub starts_at: i64,
    pub ends_at: Option<i64>,
    /// The retained renewal boundary: the grant `endsAt`, or null when the
    /// period has no expiry.
    pub renewal_at: Option<i64>,
    pub version: i64,
    pub grace: Option<GraceWindow>,
    pub next_action: Option<SubscriptionAction>,
}

/// One retained `company_package_grant_events` row, reduced to the source
/// fields needed for subscription history/recovery observation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct SubscriptionTransition {
    pub package_id: String,
    pub package_version: i64,
    pub state: String,
    pub starts_at: i64,
    pub ends_at: Option<i64>,
    pub version: i64,
    pub recorded_at: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct CompanySubscriptionRead {
    pub company_id: String,
    /// The retained `packageGrantRevision`; the same optimistic Company
    /// revision used by package command CAS. It is not authority.
    pub revision: String,
    pub observed_at: i64,
    pub subscriptions: Vec<CompanySubscription>,
    /// Retained `company_package_history_rights` rows (the same rows exposed by
    /// the TS package reader).
    pub history: Vec<CompanyPackageHistoryRight>,
    /// Retained grant transitions used to derive recovery.
    pub transitions: Vec<SubscriptionTransition>,
}

/// Derive the current subscription/renewal/grace/recovery view from retained
/// facts. The caller supplies the already-read sources in the same owner unit.
pub fn company_subscriptions(
    company_id: &str,
    grants: &[CompanyPackageGrant],
    history: &[CompanyPackageHistoryRight],
    transitions: &[SubscriptionTransition],
    now: TimestampMillis,
) -> Result<CompanySubscriptionRead, BusinessError> {
    validate_sources(company_id, grants, history, transitions)?;
    let now = now.get();
    let collator = crate::domain::collation::package_id_collator()?;
    let mut subscriptions = grants
        .iter()
        .filter(|grant| grant.company_id == company_id)
        .map(|grant| subscription(grant, transitions, now))
        .collect::<Vec<_>>();
    subscriptions.sort_by(|left, right| collator.compare(&left.package_id, &right.package_id));
    let mut retained = history
        .iter()
        .filter(|right| right.company_id == company_id)
        .cloned()
        .collect::<Vec<_>>();
    retained.sort_by(|left, right| {
        collator
            .compare(&left.package_id, &right.package_id)
            .then(left.package_version.cmp(&right.package_version))
    });
    // Retained transitions stay visible even when the package is no longer in
    // the current grant set; only the adapter's Company filter scopes them.
    let mut transitions = transitions.to_vec();
    transitions.sort_by(|left, right| {
        collator
            .compare(&left.package_id, &right.package_id)
            .then(left.recorded_at.cmp(&right.recorded_at))
            .then(left.version.cmp(&right.version))
    });
    Ok(CompanySubscriptionRead {
        company_id: company_id.to_owned(),
        revision: crate::application::grant_batches::package_grant_revision(company_id, grants)?,
        observed_at: now,
        subscriptions,
        history: retained,
        transitions,
    })
}

fn subscription(
    grant: &CompanyPackageGrant,
    transitions: &[SubscriptionTransition],
    now: i64,
) -> CompanySubscription {
    let expired = grant.ends_at.is_some_and(|end| end <= now);
    let previous = transitions
        .iter()
        .filter(|transition| {
            transition.package_id == grant.package_id && transition.version < grant.version
        })
        .max_by(|left, right| {
            left.version
                .cmp(&right.version)
                .then(left.recorded_at.cmp(&right.recorded_at))
        });
    let recovered = grant.state == "active"
        && !expired
        && previous
            .is_some_and(|transition| matches!(transition.state.as_str(), "draining" | "inactive"));
    let state = if recovered {
        SubscriptionState::Recovered
    } else if grant.state == "inactive" {
        SubscriptionState::Expired
    } else if grant.state == "draining" || expired {
        SubscriptionState::Grace
    } else {
        SubscriptionState::Active
    };
    let grace = match state {
        SubscriptionState::Grace => {
            let drained_at = transitions
                .iter()
                .filter(|transition| {
                    transition.package_id == grant.package_id && transition.state == "draining"
                })
                .max_by_key(|transition| transition.recorded_at)
                .map(|transition| transition.recorded_at);
            let starts_at = if grant.state == "draining" {
                drained_at.or(grant.ends_at).unwrap_or(grant.starts_at)
            } else {
                grant.ends_at.unwrap_or(grant.starts_at)
            };
            Some(GraceWindow {
                starts_at,
                ends_at: None,
                remaining_ms: None,
            })
        }
        _ => None,
    };
    let next_action = match state {
        SubscriptionState::Grace | SubscriptionState::Expired => Some(SubscriptionAction::Restore),
        SubscriptionState::Active | SubscriptionState::Recovered => None,
    };
    CompanySubscription {
        package_id: grant.package_id.clone(),
        package_version: grant.package_version,
        grant_state: grant.state.clone(),
        state,
        starts_at: grant.starts_at,
        ends_at: grant.ends_at,
        renewal_at: grant.ends_at,
        version: grant.version,
        grace,
        next_action,
    }
}

fn validate_sources(
    company_id: &str,
    grants: &[CompanyPackageGrant],
    history: &[CompanyPackageHistoryRight],
    transitions: &[SubscriptionTransition],
) -> Result<(), BusinessError> {
    if grants.len() > MAX_SUBSCRIPTIONS
        || history.len() > MAX_SUBSCRIPTION_HISTORY
        || transitions.len() > MAX_SUBSCRIPTION_TRANSITIONS
    {
        return Err(invalid());
    }
    let mut packages = std::collections::BTreeSet::new();
    for grant in grants {
        if grant.company_id != company_id
            || grant.package_id.is_empty()
            || grant.package_id.len() > 200
            || !(1..=MAX_SAFE).contains(&grant.package_version)
            || !(1..=MAX_SAFE).contains(&grant.version)
            || !matches!(grant.state.as_str(), "active" | "draining" | "inactive")
            || !(0..=MAX_SAFE).contains(&grant.starts_at)
            || grant
                .ends_at
                .is_some_and(|end| end <= grant.starts_at || end > MAX_SAFE)
            || !packages.insert(grant.package_id.as_str())
        {
            return Err(invalid());
        }
    }
    let mut retained = std::collections::BTreeSet::new();
    for right in history {
        if right.company_id != company_id
            || right.package_id.is_empty()
            || right.package_id.len() > 200
            || !(1..=MAX_SAFE).contains(&right.package_version)
            || !(1..=MAX_SAFE).contains(&right.source_grant_version)
            || right.source_event_id.is_empty()
            || right.source_event_id.len() > 200
            || !(0..=MAX_SAFE).contains(&right.retained_at)
            || !retained.insert((right.package_id.as_str(), right.package_version))
        {
            return Err(invalid());
        }
    }
    for transition in transitions {
        if transition.package_id.is_empty()
            || transition.package_id.len() > 200
            || !(1..=MAX_SAFE).contains(&transition.package_version)
            || !(1..=MAX_SAFE).contains(&transition.version)
            || !matches!(
                transition.state.as_str(),
                "active" | "draining" | "inactive"
            )
            || !(0..=MAX_SAFE).contains(&transition.starts_at)
            || transition
                .ends_at
                .is_some_and(|end| end <= transition.starts_at || end > MAX_SAFE)
            || !(0..=MAX_SAFE).contains(&transition.recorded_at)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCIAL_SUBSCRIPTION_FACTS_INVALID",
        "The current Commercial subscription facts are invalid.",
    )
}
