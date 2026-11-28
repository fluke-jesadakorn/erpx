//! Application query and owner port for the production accounting-profiles
//! read. SQL and connections stay in the Postgres adapter; this layer owns
//! validation and the read-only query boundary.

use crate::domain::production_profiles::{AccountingProfileRow, validate_company};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin, sync::Arc};

pub type AccountingProfilesReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner port injected by composition. The reader executes the single
/// Manufacturing-owned SELECT and returns the complete projection.
pub trait AccountingProfilesRead: Send + Sync {
    fn profiles<'a>(
        &'a self,
        company_id: &'a str,
    ) -> AccountingProfilesReadFuture<'a, Vec<AccountingProfileRow>>;
}

/// Read-only query. There is no command key or transaction boundary: the read
/// is a single snapshot SELECT on the owner pool.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountingProfilesQuery {
    pub company_id: String,
}

impl AccountingProfilesQuery {
    pub fn new(company_id: &str) -> Result<Self, BusinessError> {
        validate_company(company_id)?;
        Ok(Self {
            company_id: company_id.to_owned(),
        })
    }
}

#[derive(Clone)]
pub struct AccountingProfilesHandler {
    reader: Arc<dyn AccountingProfilesRead>,
}

impl AccountingProfilesHandler {
    pub fn new(reader: Arc<dyn AccountingProfilesRead>) -> Self {
        Self { reader }
    }

    pub fn handle<'a>(
        &'a self,
        query: AccountingProfilesQuery,
    ) -> AccountingProfilesReadFuture<'a, Vec<AccountingProfileRow>> {
        let reader = self.reader.clone();
        Box::pin(async move { reader.profiles(&query.company_id).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Probe;

    impl AccountingProfilesRead for Probe {
        fn profiles<'a>(
            &'a self,
            company_id: &'a str,
        ) -> AccountingProfilesReadFuture<'a, Vec<AccountingProfileRow>> {
            let row = AccountingProfileRow {
                id: format!("profile-{company_id}"),
                status: "approved".into(),
                version: 1,
                currency: "THB".into(),
                chart_id: "chart-1".into(),
                inventory_account_id: "inv".into(),
                wip_account_id: "wip".into(),
                labor_account_id: "labor".into(),
                overhead_account_id: "overhead".into(),
            };
            Box::pin(async move { Ok(vec![row]) })
        }
    }

    #[test]
    fn query_validates_before_reaching_the_owner() {
        assert!(AccountingProfilesQuery::new("company-1").is_ok());
        assert!(AccountingProfilesQuery::new("").is_err());
        assert!(AccountingProfilesQuery::new("bad id!").is_err());
    }

    #[tokio::test]
    async fn handler_returns_the_complete_projection() {
        let handler = AccountingProfilesHandler::new(Arc::new(Probe));
        let query = AccountingProfilesQuery::new("company-1").unwrap();
        let rows = handler.handle(query).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "profile-company-1");
        assert_eq!(rows[0].version, 1);
        assert_eq!(rows[0].currency, "THB");
        assert_eq!(rows[0].chart_id, "chart-1");
        assert_eq!(rows[0].inventory_account_id, "inv");
        assert_eq!(rows[0].wip_account_id, "wip");
        assert_eq!(rows[0].labor_account_id, "labor");
        assert_eq!(rows[0].overhead_account_id, "overhead");
    }
}
