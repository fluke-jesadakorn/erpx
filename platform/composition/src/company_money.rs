//! Native Company Money read composition.
//!
//! Company-keyed reads carry no browser WorkContext, so composition resolves
//! the owner pool and delegates to the Finance owner's exported readers. The
//! SQL stays in the owner crate; this module only owns the pool choice.
use erpx_finance_server::public::company_money_http::{
    CompanyMoneyHttpBackend, CompanyMoneyRatesFuture, CompanyMoneyScaleFuture,
    company_money_read_router, read_company_currency_scale, read_company_fx_rates,
};
use erpx_platform_runtime_server::public::http_support::Router;
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMPANY_MONEY_READ_UNAVAILABLE",
        "The Company Money read is unavailable.",
    )
}

pub struct CompanyMoneyBackend {
    holding: DatabasePool,
    companies: BTreeMap<String, DatabasePool>,
}

impl CompanyMoneyBackend {
    pub fn new(holding: DatabasePool) -> Self {
        Self {
            holding,
            companies: BTreeMap::new(),
        }
    }

    /// Company-keyed native callers fall back to the owning Holding pool when
    /// the Company is not in the configured owner map.
    pub fn with_companies(mut self, companies: BTreeMap<String, DatabasePool>) -> Self {
        self.companies = companies;
        self
    }

    fn pool(&self, company: &str) -> &DatabasePool {
        self.companies.get(company).unwrap_or(&self.holding)
    }
}

impl CompanyMoneyHttpBackend for CompanyMoneyBackend {
    fn rates<'a>(&'a self, company_id: &'a str) -> CompanyMoneyRatesFuture<'a> {
        Box::pin(async move {
            let mut connection = self.pool(company_id).raw().acquire().await.map_err(|_| unavailable())?;
            read_company_fx_rates(&mut connection, company_id).await
        })
    }

    fn scale<'a>(
        &'a self,
        company_id: &'a str,
        currency: &'a str,
    ) -> CompanyMoneyScaleFuture<'a> {
        Box::pin(async move {
            let mut connection = self.pool(company_id).raw().acquire().await.map_err(|_| unavailable())?;
            read_company_currency_scale(&mut connection, company_id, currency).await
        })
    }
}

pub fn company_money_reads(
    holding: DatabasePool,
    companies: BTreeMap<String, DatabasePool>,
) -> Result<Router, BusinessError> {
    Ok(company_money_read_router(Arc::new(
        CompanyMoneyBackend::new(holding).with_companies(companies),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_finance_server::public::ManualRate;
    use erpx_platform_runtime_server::public::http_support::{body, Body, Request, StatusCode};
    use tower::ServiceExt;

    struct Fake;

    impl CompanyMoneyHttpBackend for Fake {
        fn rates<'a>(&'a self, _company_id: &'a str) -> CompanyMoneyRatesFuture<'a> {
            Box::pin(async move { Ok(Vec::<ManualRate>::new()) })
        }
        fn scale<'a>(
            &'a self,
            _company_id: &'a str,
            _currency: &'a str,
        ) -> CompanyMoneyScaleFuture<'a> {
            Box::pin(async move { Ok(None) })
        }
    }

    #[tokio::test]
    async fn company_money_route_matches() {
        let router = erpx_platform_runtime_server::public::foundation_router()
            .merge(company_money_read_router(Arc::new(Fake)));
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/companies/co-1/finance/company-money/rates")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
