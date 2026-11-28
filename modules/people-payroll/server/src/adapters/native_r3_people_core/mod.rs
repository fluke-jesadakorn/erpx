//! Additive native R3 owner slices for the L02 People core reads.
//!
//! This module owns the SQL and the exact `{data: ...}` transport for the
//! HR settings and hire-handoff pay-group workspace reads. Cross-owner branches
//! (levels/templates/lifecycle, leave administration, purchase authority and
//! command replay) are explicitly reported as not ported rather than silently
//! falling back to TypeScript.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    CoreFuture, HrInput, PeopleCoreRead, hire_pay_group_workspace, hr_settings_data, now_ms,
};
pub use domain::{
    HR_SETTINGS_PATH, HR_TOPICS, PAY_GROUPS_PATH, command_key_required, configuration_invalid,
    forbidden, invalid, missing, origin_forbidden, unavailable, valid_id, valid_impact,
    valid_slug, valid_topic,
};
pub use http::{
    CONTEXT_COMPANY, CONTEXT_PERMISSIONS, CONTEXT_USER, PeopleCoreTarget, WORKSPACE_BRANCH,
    WORKSPACE_DEPARTMENT, router,
};
pub use postgres::{
    PgPeopleCoreRead, SQL_AFFECTED_PEOPLE, SQL_ASSIGNMENTS, SQL_CALENDARS,
    SQL_CURRENT_POLICIES, SQL_ENGAGEMENT, SQL_ENGAGEMENT_GROUPS, SQL_ELIGIBILITY,
    SQL_FUTURE_LEAVES, SQL_FUTURE_SHIFTS, SQL_HIRE_RECEIPT, SQL_HIRE_SOURCE,
    SQL_MATCHING_PEOPLE, SQL_OFFER_APPLICATION, SQL_PAY_GROUPS, SQL_POLICIES,
    SQL_SAME_CURRENCY_GROUPS, SQL_TODAY,
};

#[cfg(test)]
mod tests {
    use super::domain::{HR_SETTINGS_PATH, PAY_GROUPS_PATH};

    #[test]
    fn slices_are_named_for_the_shared_routes() {
        assert!(HR_SETTINGS_PATH.ends_with("/people/hr-settings"));
        assert!(PAY_GROUPS_PATH.ends_with("/pay-groups"));
    }
}
