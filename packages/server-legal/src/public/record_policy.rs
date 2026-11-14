pub use crate::adapters::postgres_record_policy::{
    LegalRecordAccess, LegalRecordWitness, PgLegalRecordExternalPolicy, PgLegalRecordPolicy,
    lock_legal_record_sources,
};
pub use crate::application::record_policy::authorize_legal_record;
pub use crate::domain::record_policy::{
    LegalCommercialMode, LegalPurchaseObservation, LegalPurchasePurpose, LegalPurchaseRequirement,
    LegalRecordActor, LegalRecordDocument, LegalRecordFacts, LegalRecordOperation,
    LegalRecordRequest, LegalRecordSummary, LegalReleaseObservation, legal_kind_capability,
};
