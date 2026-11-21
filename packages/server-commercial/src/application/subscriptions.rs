//! Native subscription and grace/recovery application reads.
//!
//! This is a read-only projection over the retained Company package grant,
//! history-right and grant-event sources. It never writes a subscription,
//! starts a timer, or converts a grace period into new-work entitlement.

use crate::domain::packages::{CompanyPackageGrant, CompanyPackageHistoryRight};
use crate::domain::subscriptions::{
    CompanySubscriptionRead, SubscriptionTransition, company_subscriptions,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use serde::{Deserialize, Serialize};
use std::future::Future;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubscriptionFacts {
    pub grants: Vec<CompanyPackageGrant>,
    pub history: Vec<CompanyPackageHistoryRight>,
    pub transitions: Vec<SubscriptionTransition>,
}

/// Read-only port. Implementations use the caller's existing owner connection
/// and must not open another transaction.
pub trait CompanySubscriptionReadPort {
    type Error: From<BusinessError> + Send;
    fn read(
        &mut self,
        company: &CompanyId,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<CompanySubscriptionRead, Self::Error>> + Send;
}

pub fn current_subscriptions(
    company: &CompanyId,
    facts: &SubscriptionFacts,
    now: TimestampMillis,
) -> Result<CompanySubscriptionRead, BusinessError> {
    company_subscriptions(
        company.as_str(),
        &facts.grants,
        &facts.history,
        &facts.transitions,
        now,
    )
}
