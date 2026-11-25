//! Bind a registered Marketing read plan on the caller's own transaction.
//!
//! The adapter only maps caller scope facts onto the exact registered SQL and
//! its ordered bind positions. It never opens a connection, commits, locks, or
//! changes a live TypeScript reader; authority stays with the retained caller.
use crate::public::native_read::{self, BindField, ReadPlan};
use erpx_server_shared::public::BusinessError;

/// Scope facts the live caller already resolved from its WorkContext.
#[derive(Clone, Debug, Default)]
pub struct ReadScope<'a> {
    /// Public Marketing asset identifier (not a typed `RecordId`).
    pub asset_id: Option<&'a str>,
    pub company_id: Option<&'a str>,
    /// Active-form / form-content version, bound as a PG integer.
    pub version: Option<i64>,
    /// Public Marketing campaign identifier.
    pub campaign_id: Option<&'a str>,
    /// Purchase command identity and authority facts on the shared ledger.
    pub command_id: Option<&'a str>,
    pub operation: Option<&'a str>,
    pub record_id: Option<&'a str>,
    pub actor_id: Option<&'a str>,
}

/// A plan ready for the caller to run on its existing SQL snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundPlan {
    pub kind: &'static str,
    pub sql: &'static str,
    pub binds: Vec<String>,
}

fn scope_required(field: BindField) -> BusinessError {
    let message = match field {
        BindField::AssetId => "The Marketing read needs the asset id.",
        BindField::CompanyId => "The Marketing read needs the current Company.",
        BindField::Version => "The Marketing read needs the active form version.",
        BindField::CampaignId => "The Marketing read needs the campaign id.",
        BindField::CommandId => "The Marketing read needs the command id.",
        BindField::Operation => "The Marketing read needs the command operation.",
        BindField::RecordId => "The Marketing read needs the command record id.",
        BindField::ActorId => "The Marketing read needs the command actor id.",
    };
    BusinessError::invalid("MARKETING_READ_SCOPE_REQUIRED", message)
}

/// Bind `kind` using `scope`, preserving the exact registered SQL and order.
pub fn bind(kind: &str, scope: &ReadScope<'_>) -> Result<BoundPlan, BusinessError> {
    let plan: &'static ReadPlan = native_read::plan(kind).ok_or_else(native_read::invalid)?;
    let mut binds = Vec::with_capacity(plan.binds.len());
    for &field in plan.binds {
        let value = match field {
            BindField::AssetId => scope.asset_id.map(str::to_owned),
            BindField::CompanyId => scope.company_id.map(str::to_owned),
            BindField::Version => scope.version.map(|version| version.to_string()),
            BindField::CampaignId => scope.campaign_id.map(str::to_owned),
            BindField::CommandId => scope.command_id.map(str::to_owned),
            BindField::Operation => scope.operation.map(str::to_owned),
            BindField::RecordId => scope.record_id.map(str::to_owned),
            BindField::ActorId => scope.actor_id.map(str::to_owned),
        };
        let value = value.ok_or_else(|| scope_required(field))?;
        binds.push(value);
    }
    Ok(BoundPlan {
        kind: plan.kind,
        sql: plan.sql,
        binds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public::native_read as kinds;

    fn full_scope<'a>() -> ReadScope<'a> {
        ReadScope {
            asset_id: Some("asset-1"),
            company_id: Some("company-1"),
            version: Some(3),
            campaign_id: Some("campaign-1"),
            command_id: Some("command-1"),
            operation: Some("marketing.asset.activate"),
            record_id: Some("record-1"),
            actor_id: Some("actor-1"),
        }
    }

    #[test]
    fn every_registered_kind_binds_without_losing_its_plan() {
        for kind in kinds::kinds() {
            let bound = bind(kind, &full_scope()).expect("bindable plan");
            let plan = kinds::plan(kind).unwrap();
            assert_eq!(bound.kind, kind);
            assert_eq!(bound.sql, plan.sql);
            assert_eq!(bound.binds.len(), bound.sql.matches('?').count(), "{kind}");
        }
    }

    #[test]
    fn form_binds_preserve_asset_company_version_order() {
        let bound = bind(kinds::RD_FORM_CONTENT_ROW, &full_scope()).unwrap();
        assert_eq!(
            bound.binds,
            vec!["asset-1".to_owned(), "company-1".to_owned(), "3".to_owned()]
        );
        let public = bind(kinds::RD_FORM_PUBLIC, &full_scope()).unwrap();
        assert_eq!(public.binds, vec!["asset-1".to_owned()]);
        let campaign = bind(kinds::RD_CAMPAIGN_EXISTS, &full_scope()).unwrap();
        assert_eq!(
            campaign.binds,
            vec!["campaign-1".to_owned(), "company-1".to_owned()]
        );
    }

    #[test]
    fn receipt_guard_binds_command_operation_record_actor_company_in_order() {
        let bound = bind(kinds::RD_COMMAND_RECEIPT_PRIOR, &full_scope()).unwrap();
        assert_eq!(
            bound.binds,
            vec![
                "command-1".to_owned(),
                "marketing.asset.activate".to_owned(),
                "record-1".to_owned(),
                "actor-1".to_owned(),
                "company-1".to_owned(),
            ]
        );
        assert_eq!(bound.binds.len(), bound.sql.matches('?').count());
    }

    #[test]
    fn required_scope_and_unknown_kind_are_validation_errors() {
        assert_eq!(
            bind("bogus.read", &full_scope()).unwrap_err().code,
            "MARKETING_READ_PLAN_INVALID"
        );
        assert_eq!(
            bind(kinds::RD_ASSET_POINT, &ReadScope::default())
                .unwrap_err()
                .code,
            "MARKETING_READ_SCOPE_REQUIRED"
        );
        assert_eq!(
            bind(kinds::RD_FORM_SOURCE, &ReadScope::default())
                .unwrap_err()
                .code,
            "MARKETING_READ_SCOPE_REQUIRED"
        );
        assert_eq!(
            bind(kinds::RD_CAMPAIGN_EXISTS, &ReadScope::default())
                .unwrap_err()
                .code,
            "MARKETING_READ_SCOPE_REQUIRED"
        );
    }
}
