//! Native Company Money read HTTP ingress (composition mounts).
pub use crate::adapters::company_money_read_http::{
    CompanyMoneyHttpBackend, CompanyMoneyRatesFuture, CompanyMoneyRatesResponse,
    CompanyMoneyScaleFuture, CompanyMoneyScaleQuery, CompanyMoneyScaleResponse,
    company_money_read_router, read_company_currency_scale, read_company_fx_rates,
};
