//! Application policy selection for the Files Field intake port.
use crate::domain::field_visit_intake::{FieldSetRequest, IntakeOwner, owner_classification};
use erpx_server_shared::public::BusinessError;
pub fn policy(owner: IntakeOwner, input: &FieldSetRequest) -> Result<&'static str, BusinessError> {
    owner_classification(
        owner,
        &input.target_type,
        &input.component_kind,
        &input.purpose_key,
    )
}
