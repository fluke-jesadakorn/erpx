use erpx_server_shared::public::{RecordId, UserId};
use serde::Serialize;

/// B0 PermissionActor.iam audit data, never a grant or a deserializable Actor.
/// Field order, explicit nulls and the omitted HR/queued fields match the actual
/// createIamAuthority result and queuedAuthority's appended assignment version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandIamProjection {
    pub(crate) person_user_ids: Vec<UserId>,
    pub(crate) emergency_until: Option<i64>,
    pub(crate) role_revision: i64,
    pub(crate) identity_version: i64,
    pub(crate) permissions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) hr_appointment_version: Option<i64>,
    pub(crate) grantable_permissions: Vec<String>,
    pub(crate) principal_id: String,
    pub(crate) person_id: Option<RecordId>,
    pub(crate) fresh_until: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) queued_assignment_version: Option<i64>,
}
