//! Application policy selection for the Files Field intake port.
use crate::domain::field_visit_intake::{FieldSetRequest, classification};
use erpx_server_shared::public::BusinessError;
pub fn policy(input: &FieldSetRequest) -> Result<&'static str, BusinessError> {
    classification(
        &input.target_type,
        &input.component_kind,
        &input.purpose_key,
    )
}
