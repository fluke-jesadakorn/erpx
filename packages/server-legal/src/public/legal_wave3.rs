//! Public re-exports for the native Legal wf3a13 write slice.
pub use crate::adapters::legal_wave3_http::{
    LegalWave3Backend, LegalWave3CommandBody, LegalWave3Future, LegalWave3Request,
    LegalWave3Target, legal_wave3_browser_router, legal_wave3_company_router,
    legal_wave3_holding_router,
};
pub use crate::adapters::legal_wave3_postgres::legal_wave3_statement;
pub use crate::application::legal_wave3::{LegalWave3Command, LegalWave3Port, plan_legal_wave3};
pub use crate::domain::legal_wave3::{
    AI_COMPLETE, AI_FAIL, AI_INTERRUPT, AI_QUEUE, ENROLLMENT_ATTEMPT, INVITE_CANCEL, INVITE_FAIL,
    INVITE_RECOVER, INVITE_SENT, KINDS, LegalWave3Statement, SOURCE_COMPLETE, SOURCE_FAIL,
    SOURCE_PRESERVE, SOURCE_RECOVER, plan, supported,
};
