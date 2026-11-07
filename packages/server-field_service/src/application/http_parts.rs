//! Source data from the shared POST discriminator. This is not authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldServiceRawOrderTarget {
    original_utf16: Vec<u16>,
}
impl FieldServiceRawOrderTarget {
    pub fn from_utf16(original_utf16: Vec<u16>) -> Self {
        Self { original_utf16 }
    }
    pub fn original_utf16(&self) -> &[u16] {
        &self.original_utf16
    }
    /// Matches the observed pg string transport, preserving logical identity separately.
    pub fn storage_text(&self) -> String {
        String::from_utf16_lossy(&self.original_utf16)
    }
}

/// Physical text bound to owner rows. It never asserts current owner authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldServicePhysicalOrderId(String);
impl FieldServicePhysicalOrderId {
    pub fn from_storage(value: String) -> Self {
        Self(value)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl From<erpx_server_shared::public::RecordId> for FieldServicePhysicalOrderId {
    fn from(value: erpx_server_shared::public::RecordId) -> Self {
        Self(value.as_str().to_owned())
    }
}
/// A parsed request before late key admission. Its data cannot commit an effect.
#[derive(Clone)]
pub struct PendingFieldServiceParts {
    pub target: FieldServiceRawOrderTarget,
    pub key: String,
    pub normalized: super::parts::NormalizedFieldServiceParts,
}
