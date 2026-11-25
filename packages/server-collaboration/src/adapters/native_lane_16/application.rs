//! Owner query port and handler. This module never sees SQL or a connection.

use super::domain::{ReadKind, SentNoticeDetail, SentNoticeRow, TaskOrigin};
use erpx_server_shared::public::BusinessError;
use serde_json::json;
use std::{future::Future, pin::Pin};

pub type LaneReadFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "LANE_16_READ_REQUEST_INVALID",
        "Use one bounded Collaboration sent-notice read.",
    )
}

/// The collaboration owner owns the SQL and the projection behind this port.
pub trait SentNoticeRead: Send + Sync {
    fn notices<'a>(
        &'a self,
        company: &'a str,
        sender: &'a str,
    ) -> LaneReadFuture<'a, Vec<SentNoticeRow>>;
    fn notice<'a>(
        &'a self,
        company: &'a str,
        sender: &'a str,
        id: &'a str,
    ) -> LaneReadFuture<'a, Option<SentNoticeDetail>>;
    fn delivery_count<'a>(&'a self, company: &'a str, key: &'a str) -> LaneReadFuture<'a, i64>;
    fn task_origin<'a>(
        &'a self,
        company: &'a str,
        task: &'a str,
    ) -> LaneReadFuture<'a, Option<TaskOrigin>>;
}

impl SentNoticeRead for std::sync::Arc<dyn SentNoticeRead> {
    fn notices<'a>(
        &'a self,
        company: &'a str,
        sender: &'a str,
    ) -> LaneReadFuture<'a, Vec<SentNoticeRow>> {
        self.as_ref().notices(company, sender)
    }
    fn notice<'a>(
        &'a self,
        company: &'a str,
        sender: &'a str,
        id: &'a str,
    ) -> LaneReadFuture<'a, Option<SentNoticeDetail>> {
        self.as_ref().notice(company, sender, id)
    }
    fn delivery_count<'a>(&'a self, company: &'a str, key: &'a str) -> LaneReadFuture<'a, i64> {
        self.as_ref().delivery_count(company, key)
    }
    fn task_origin<'a>(
        &'a self,
        company: &'a str,
        task: &'a str,
    ) -> LaneReadFuture<'a, Option<TaskOrigin>> {
        self.as_ref().task_origin(company, task)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadRequest {
    pub company: String,
    pub sender: String,
    pub kind: ReadKind,
    pub notice_id: Option<String>,
    pub event_key: Option<String>,
    pub task_id: Option<String>,
}

impl ReadRequest {
    /// The HTTP layer has already validated the Company against the registered
    /// route. This check keeps a malformed combination from reaching the port.
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.company.is_empty()
            || self.company.len() > 200
            || !matches!(self.kind, ReadKind::TaskOrigin | ReadKind::DeliveryCount)
                && !super::domain::valid_identifier(&self.sender)
        {
            return Err(invalid());
        }
        let (has_notice, has_event, has_task) = self.kind.shape();
        if has_notice
            != self
                .notice_id
                .as_deref()
                .is_some_and(super::domain::valid_identifier)
            || has_event
                != self.event_key.as_deref().is_some_and(super::domain::valid_event_key)
            || has_task
                != self.task_id.as_deref().is_some_and(super::domain::valid_identifier)
        {
            return Err(invalid());
        }
        Ok(())
    }
}

pub struct SentNoticeService<R: SentNoticeRead> {
    read: R,
}

impl<R: SentNoticeRead> SentNoticeService<R> {
    pub fn new(read: R) -> Self {
        Self { read }
    }

    /// Produce the complete native response. No caller SQL or predicate can
    /// enter this projection; the Company is the routing target.
    pub async fn read(&self, request: ReadRequest) -> Result<serde_json::Value, BusinessError> {
        request.validate()?;
        let company = request.company.as_str();
        match request.kind {
            ReadKind::Notices => {
                let rows = self.read.notices(company, &request.sender).await?;
                Ok(json!({
                    "kind": request.kind.as_str(),
                    "rows": rows.into_iter().map(SentNoticeRow::into_value).collect::<Vec<_>>(),
                }))
            }
            ReadKind::Notice => {
                let row = self
                    .read
                    .notice(company, &request.sender, request.notice_id.as_deref().unwrap())
                    .await?;
                Ok(json!({"kind": request.kind.as_str(), "row": row}))
            }
            ReadKind::DeliveryCount => {
                let n = self
                    .read
                    .delivery_count(company, request.event_key.as_deref().unwrap())
                    .await?;
                Ok(json!({"kind": request.kind.as_str(), "n": n}))
            }
            ReadKind::TaskOrigin => {
                let row = self
                    .read
                    .task_origin(company, request.task_id.as_deref().unwrap())
                    .await?;
                Ok(json!({"kind": request.kind.as_str(), "row": row}))
            }
        }
    }
}

/// Owner port for the three push registration reads. No SQL reaches this layer.
pub trait PushSettingsRead: Send + Sync {
    fn preferences<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
    ) -> LaneReadFuture<'a, Option<serde_json::Value>>;
    fn session_count<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
        session: &'a str,
    ) -> LaneReadFuture<'a, Option<i64>>;
    fn endpoint_owner<'a>(
        &'a self,
        company: &'a str,
        hash: &'a str,
    ) -> LaneReadFuture<'a, Option<serde_json::Value>>;
}

impl PushSettingsRead for std::sync::Arc<dyn PushSettingsRead> {
    fn preferences<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
    ) -> LaneReadFuture<'a, Option<serde_json::Value>> {
        self.as_ref().preferences(company, user)
    }
    fn session_count<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
        session: &'a str,
    ) -> LaneReadFuture<'a, Option<i64>> {
        self.as_ref().session_count(company, user, session)
    }
    fn endpoint_owner<'a>(
        &'a self,
        company: &'a str,
        hash: &'a str,
    ) -> LaneReadFuture<'a, Option<serde_json::Value>> {
        self.as_ref().endpoint_owner(company, hash)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PushReadRequest {
    pub company: String,
    pub user: String,
    pub kind: super::domain::PushReadKind,
    pub session: Option<String>,
    pub endpoint_hash: Option<String>,
}

impl PushReadRequest {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !super::domain::valid_identifier(&self.company) {
            return Err(invalid());
        }
        if !matches!(self.kind, super::domain::PushReadKind::EndpointOwner)
            && !super::domain::valid_identifier(&self.user)
        {
            return Err(invalid());
        }
        match self.kind {
            super::domain::PushReadKind::Preferences => {
                if self.session.is_some() || self.endpoint_hash.is_some() {
                    return Err(invalid());
                }
            }
            super::domain::PushReadKind::SessionCount => {
                if self.session.as_deref().is_none_or(|v| !super::domain::valid_identifier(v))
                    || self.endpoint_hash.is_some()
                {
                    return Err(invalid());
                }
            }
            super::domain::PushReadKind::EndpointOwner => {
                if self.endpoint_hash.as_deref().is_none_or(|v| !super::domain::valid_identifier(v))
                    || self.session.is_some()
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}

pub struct PushSettingsService<R: PushSettingsRead> {
    read: R,
}

impl<R: PushSettingsRead> PushSettingsService<R> {
    pub fn new(read: R) -> Self {
        Self { read }
    }

    pub async fn read(&self, request: PushReadRequest) -> Result<serde_json::Value, BusinessError> {
        request.validate()?;
        let company = request.company.as_str();
        let user = request.user.as_str();
        let value = match request.kind {
            super::domain::PushReadKind::Preferences => json!({
                "kind": request.kind.as_str(),
                "row": self.read.preferences(company, user).await?,
            }),
            super::domain::PushReadKind::SessionCount => json!({
                "kind": request.kind.as_str(),
                "n": self
                    .read
                    .session_count(company, user, request.session.as_deref().unwrap())
                    .await?
                    .unwrap_or(0),
            }),
            super::domain::PushReadKind::EndpointOwner => json!({
                "kind": request.kind.as_str(),
                "row": self.read.endpoint_owner(company, request.endpoint_hash.as_deref().unwrap()).await?,
            }),
        };
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(kind: ReadKind) -> ReadRequest {
        ReadRequest {
            company: "company-1".into(),
            sender: "user-1".into(),
            kind,
            notice_id: matches!(kind, ReadKind::Notice).then(|| "notice-1".into()),
            event_key: matches!(kind, ReadKind::DeliveryCount).then(|| "notice:notice-1".into()),
            task_id: matches!(kind, ReadKind::TaskOrigin).then(|| "task-1".into()),
        }
    }

    #[test]
    fn all_registered_shapes_validate() {
        for kind in [
            ReadKind::Notices,
            ReadKind::Notice,
            ReadKind::DeliveryCount,
            ReadKind::TaskOrigin,
        ] {
            assert!(request(kind).validate().is_ok());
        }
    }

    #[test]
    fn push_read_shapes_validate_and_reject_cross_kind_fields() {
        use super::super::domain::PushReadKind;
        let base = || PushReadRequest {
            company: "company-1".into(),
            user: "user-1".into(),
            kind: PushReadKind::Preferences,
            session: None,
            endpoint_hash: None,
        };
        assert!(base().validate().is_ok());
        let mut bad = base();
        bad.session = Some("session-1".into());
        assert!(bad.validate().is_err());
        let mut bad = base();
        bad.kind = PushReadKind::SessionCount;
        bad.session = Some("session-1".into());
        assert!(bad.validate().is_ok());
        bad.endpoint_hash = Some("hash-1".into());
        assert!(bad.validate().is_err());
    }

    #[test]
    fn cross_kind_fields_and_unsafe_values_are_rejected() {
        let mut bad = request(ReadKind::Notice);
        bad.event_key = Some("notice:1".into());
        assert!(bad.validate().is_err());
        let mut bad = request(ReadKind::Notices);
        bad.notice_id = Some("notice-1".into());
        assert!(bad.validate().is_err());
        let mut bad = request(ReadKind::DeliveryCount);
        bad.event_key = Some("' OR 1=1".into());
        assert!(bad.validate().is_err());
        let mut bad = request(ReadKind::Notices);
        bad.sender = String::new();
        assert!(bad.validate().is_err());
    }
}
