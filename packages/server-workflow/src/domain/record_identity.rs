//! Logical command identity and the separately observed database transport value.
use super::command_json::CommandJson;
use erpx_server_shared::public::BusinessError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowRecordIdentity {
    original_utf16: Vec<u16>,
    physical: String,
}
impl WorkflowRecordIdentity {
    pub fn from_utf16(original_utf16: Vec<u16>) -> Self {
        let physical = String::from_utf16_lossy(&original_utf16);
        Self {
            original_utf16,
            physical,
        }
    }
    pub fn from_text(value: &str) -> Self {
        Self::from_utf16(value.encode_utf16().collect())
    }
    pub fn original_utf16(&self) -> &[u16] {
        &self.original_utf16
    }
    pub fn physical_text(&self) -> &str {
        &self.physical
    }
    pub fn logical_json(&self) -> Result<CommandJson, BusinessError> {
        CommandJson::from_utf16_string(&self.original_utf16)
    }
}
