//! Source change intent is not approval. Its required native policy is supplied
//! at the PostgreSQL composition boundary, never reconstructed from this DTO.
use crate::domain::directory_provenance::{Denial, SourceRevision, valid_key_id};
use erpx_server_shared::public::{BusinessError, RecordId};
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum DirectorySourceChangeKind {
    InitialSeed,
    Rotation,
}
pub struct DirectorySourceChangeRequest {
    pub(crate) connection_id: RecordId,
    pub(crate) key_id: String,
    pub(crate) expected_connection_version: i64,
    pub(crate) expected_source_revision: Option<SourceRevision>,
}
impl DirectorySourceChangeRequest {
    pub fn new(
        connection_id: RecordId,
        key_id: String,
        expected_connection_version: i64,
        expected_source_revision: Option<SourceRevision>,
    ) -> Result<Self, BusinessError> {
        if !valid_key_id(&key_id) || expected_connection_version <= 0 {
            return Err(Denial::ConfigurationNotSelected.business());
        }
        Ok(Self {
            connection_id,
            key_id,
            expected_connection_version,
            expected_source_revision,
        })
    }
    pub fn kind(&self) -> DirectorySourceChangeKind {
        if self.expected_source_revision.is_some() {
            DirectorySourceChangeKind::Rotation
        } else {
            DirectorySourceChangeKind::InitialSeed
        }
    }
    pub fn connection_id(&self) -> &RecordId {
        &self.connection_id
    }
    pub fn key_id(&self) -> &str {
        &self.key_id
    }
    pub fn expected_connection_version(&self) -> i64 {
        self.expected_connection_version
    }
    pub fn expected_source_revision(&self) -> Option<SourceRevision> {
        self.expected_source_revision
    }
}
