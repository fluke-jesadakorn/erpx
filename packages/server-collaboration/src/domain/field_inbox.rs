//! Field-owned Inbox candidate stream. Visibility is supplied by native Records
//! in the caller's transaction; a cursor is never an authorization snapshot.
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FieldInboxProjection {
    #[default]
    Inbox,
    StaffList,
    StaffPage,
}
impl FieldInboxProjection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::StaffList => "staff-list",
            Self::StaffPage => "staff-page",
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldInboxCursor {
    pub at: i64,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldInboxQuery {
    #[serde(default)]
    pub projection: FieldInboxProjection,
    #[serde(default)]
    pub reverse: bool,
    #[serde(default)]
    pub all: bool,
    pub category: Option<String>,
    #[serde(default)]
    pub search: String,
    pub before: Option<FieldInboxCursor>,
    pub limit: usize,
}
impl FieldInboxQuery {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if (self.reverse && self.projection != FieldInboxProjection::StaffPage)
            || !(1..=101).contains(&self.limit)
            || self.search.chars().count() > 100
            || self.category.as_deref().is_some_and(|v| {
                !matches!(v, "work" | "calendar" | "mention" | "notice" | "system")
            })
            || self.before.as_ref().is_some_and(|v| {
                v.at.unsigned_abs() > 9_007_199_254_740_991
                    || v.id.is_empty()
                    || v.id.len() > 512
                    || v.key.as_ref().is_some_and(|key| {
                        self.projection != FieldInboxProjection::StaffPage || key.len() > 4096
                    })
            })
        {
            return Err(BusinessError::invalid(
                "INVALID_INBOX_QUERY",
                "Restart the Inbox search.",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct FieldInboxCandidate {
    pub id: String,
    pub event_id: String,
    pub company_id: String,
    pub user_id: String,
    pub delivered_at: i64,
    pub read_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub source_type: String,
    pub source_id: String,
    pub source_version: i64,
    pub category: String,
    pub preview: String,
}
impl FieldInboxCandidate {
    pub fn cursor(&self) -> FieldInboxCursor {
        FieldInboxCursor {
            at: self.delivered_at,
            id: self.id.clone(),
            key: None,
        }
    }
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.id.len() > 512
            || self.event_id.len() > 1024
            || self.company_id.len() > 200
            || self.user_id.len() > 200
            || !matches!(
                self.source_type.as_str(),
                "field_service_order"
                    | "operational_exception"
                    | "collaboration_action"
                    | "support_ticket"
                    | "stock_count"
                    | "invoice"
                    | "journal"
            )
            || self.source_id.is_empty()
            || self.source_id.len() > 512
            || !(1..=9_007_199_254_740_991).contains(&self.source_version)
            || self.preview.len() > 65536
            || ![Some(self.delivered_at), self.read_at, self.archived_at]
                .into_iter()
                .flatten()
                .all(|v| (0..=9_007_199_254_740_991).contains(&v))
            || !matches!(
                self.category.as_str(),
                "work" | "calendar" | "mention" | "notice" | "system"
            )
        {
            return Err(BusinessError::invalid(
                "INBOX_SOURCE_INVALID",
                "The Inbox source is unavailable.",
            ));
        }
        Ok(())
    }
}

/// User acknowledgement state only. Delivered event metadata is immutable and
/// the current Records source remains a separate authorization witness.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldInboxPatch {
    pub read: Option<bool>,
    pub archived: Option<bool>,
}
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldInboxState {
    pub id: String,
    pub read_at: Option<i64>,
    pub archived_at: Option<i64>,
}
impl FieldInboxPatch {
    pub fn plan(
        &self,
        row: &FieldInboxCandidate,
        now: i64,
    ) -> Result<FieldInboxState, BusinessError> {
        if self.read.is_none() && self.archived.is_none() {
            return Err(BusinessError::invalid(
                "INVALID_INBOX_STATE",
                "Choose a read or archive state.",
            ));
        }
        row.validate()?;
        if !(0..=9_007_199_254_740_991).contains(&now) {
            return Err(BusinessError::invalid(
                "INVALID_INBOX_STATE",
                "Use a valid acknowledgement time.",
            ));
        }
        Ok(FieldInboxState {
            id: row.id.clone(),
            read_at: match self.read {
                None => row.read_at,
                Some(true) => Some(row.read_at.unwrap_or(now)),
                Some(false) => None,
            },
            archived_at: match self.archived {
                None => row.archived_at,
                Some(true) => Some(row.archived_at.unwrap_or(now)),
                Some(false) => None,
            },
        })
    }
}
