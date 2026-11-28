//! Public re-exports for the native Customer Authority wf3a13 write slice.
pub use crate::adapters::customer_authority_wave3_http::{
    CustomerAuthorityWave3Backend, CustomerAuthorityWave3CommandBody, CustomerAuthorityWave3Future,
    CustomerAuthorityWave3Request, CustomerAuthorityWave3Target,
    customer_authority_wave3_browser_router, customer_authority_wave3_company_router,
};
pub use crate::adapters::customer_authority_wave3_postgres::customer_authority_wave3_statement;
pub use crate::application::customer_authority_wave3::{
    CustomerAuthorityWave3Command, CustomerAuthorityWave3Port, plan_customer_authority_wave3,
};
pub use crate::domain::customer_authority_wave3::{
    CustomerAuthorityWave3Statement, EVIDENCE_BINDING_VERSION, EVIDENCE_EVENT, KINDS, plan,
    supported,
};
