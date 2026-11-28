//! Files-owned preparation of a Field record bind.
use crate::domain::field_visit_bind::{FieldBindManifestItem, FieldBindSet, prepare_field_bind};
use erpx_platform_shared_server::public::BusinessError;
pub fn prepare(
    set: &FieldBindSet,
    expected: i64,
) -> Result<Vec<FieldBindManifestItem>, BusinessError> {
    prepare_field_bind(set, expected)
}
