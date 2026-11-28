//! Portable Customer Authority customer identity-change contract.
pub use crate::adapters::identity_postgres::PgCustomerIdentity;
pub use crate::application::identity::{
    IdentityCommandRepository, IdentityFuture, IdentityReadRepository, change_customer_identity,
    identity_detail, identity_work, identity_workspace, request_customer_identity_change,
};
pub use crate::domain::identity::{
    ChangePlan, CustomerAddress, CustomerIdentity, CustomerSnapshot, IdentityChange,
    IdentityChangeAction, IdentityChangeRow, IdentityCustomer, IdentityEventRow, IdentityRequest,
    MAX_ADDRESS_LINE, MAX_CANDIDATES, MAX_CITY, MAX_EMAIL, MAX_EVIDENCE, MAX_HISTORY_ROWS,
    MAX_NAME, MAX_PHONE, MAX_POSTAL_CODE, MAX_REGION, MAX_TAX_REGISTRATION, MAX_WORKSPACE_ROWS,
    NotificationConsent, REQUEST_OPERATION, RequestPlan, ValidatedIdentityChange,
    ValidatedIdentityRequest, change_changed, change_identity, change_not_found, change_plan,
    customer_changed, customer_not_found, idempotency_conflict, independent_approver_required,
    readback_invalid, request_id, request_invalid, request_plan, resubmit_forbidden,
    reviewer_unavailable, unchanged,
};
