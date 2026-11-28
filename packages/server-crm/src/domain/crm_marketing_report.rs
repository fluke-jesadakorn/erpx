//! Pure query vocabulary for the native CRM/marketing report ingress.
//!
//! The wire shape mirrors the retired shared Next route: one `kind` discriminator
//! plus the same bounded filter set as the native `sales-crm` report. This module
//! carries no transport, session or SQL; it only normalises and validates the
//! read request before the owner port resolves it.
use super::sales_report::{SalesReportQuery, request_invalid as sales_report_invalid};
use erpx_server_shared::public::BusinessError;
use serde::Deserialize;

/// The retired route's `kind` discriminator. Both branches are owner reads;
/// composition decides which owner query executes for each branch.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrmMarketingReportKind {
    Crm,
    Marketing,
}

/// Exact, deny-unknown query accepted by `GET /reports/crm-marketing`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrmMarketingReportQuery {
    #[serde(default)]
    pub kind: Option<CrmMarketingReportKind>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub branch_id: Option<String>,
    #[serde(default)]
    pub property_id: Option<String>,
    #[serde(default)]
    pub campaign_id: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
}

impl CrmMarketingReportQuery {
    /// `kind` defaults to the CRM report, matching the retired route.
    pub fn kind(&self) -> CrmMarketingReportKind {
        self.kind.unwrap_or(CrmMarketingReportKind::Crm)
    }

    /// Reuse the already-native `sales-crm` validation for every filter: dates
    /// must be ISO `YYYY-MM-DD` and identifiers must be bounded/control-free.
    pub fn validate(&self) -> Result<(), BusinessError> {
        self.sales_report_query().validate()
    }

    /// Drop the discriminator; the owner report port consumes the exact filter
    /// contract shared with the native `sales-crm` route.
    pub fn sales_report_query(&self) -> SalesReportQuery {
        SalesReportQuery {
            from: self.from.clone(),
            to: self.to.clone(),
            branch_id: self.branch_id.clone(),
            property_id: self.property_id.clone(),
            campaign_id: self.campaign_id.clone(),
            owner_id: self.owner_id.clone(),
        }
    }
}

pub fn request_invalid() -> BusinessError {
    sales_report_invalid()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_kind_defaults_to_crm() {
        let query = CrmMarketingReportQuery::default();
        assert_eq!(query.kind(), CrmMarketingReportKind::Crm);
    }

    #[test]
    fn filters_are_validated_with_the_native_sales_report_rules() {
        let mut query = CrmMarketingReportQuery::default();
        query.from = Some("2026-11-01".to_owned());
        query.to = Some("2026-11-28".to_owned());
        query.owner_id = Some("owner-1".to_owned());
        assert!(query.validate().is_ok());

        query.from = Some("28/11/2026".to_owned());
        assert_eq!(query.validate().unwrap_err().code, "CRM_SALES_REPORT_REQUEST_INVALID");
    }

    #[test]
    fn discriminator_is_dropped_from_the_owner_report_query() {
        let query = CrmMarketingReportQuery {
            kind: Some(CrmMarketingReportKind::Marketing),
            campaign_id: Some("campaign-1".to_owned()),
            ..CrmMarketingReportQuery::default()
        };
        let owner = query.sales_report_query();
        assert_eq!(owner.campaign_id.as_deref(), Some("campaign-1"));
        assert_eq!(owner.from, None);
    }
}
