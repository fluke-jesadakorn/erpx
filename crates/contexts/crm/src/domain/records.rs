use chrono::{DateTime, Utc};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{CrmDomainError, MAX_CRM_TEXT_BYTES};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrmRecordKind {
    Activity,
    Forecast,
}

impl CrmRecordKind {
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Activity => "crm.activity",
            Self::Forecast => "crm.forecast",
        }
    }
    const fn initial_status(self) -> CrmRecordStatus {
        match self {
            Self::Activity => CrmRecordStatus::Planned,
            Self::Forecast => CrmRecordStatus::Draft,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrmRecordStatus {
    Draft,
    Planned,
    InProgress,
    Submitted,
    Approved,
    Completed,
    Cancelled,
    Superseded,
}

impl CrmRecordStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Planned => "planned",
            Self::InProgress => "in_progress",
            Self::Submitted => "submitted",
            Self::Approved => "approved",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Superseded => "superseded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrmRecord {
    id: Uuid,
    version: u64,
    kind: CrmRecordKind,
    scope: AuthorityScope,
    title: String,
    status: CrmRecordStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl CrmRecord {
    pub fn create(
        id: Uuid,
        kind: CrmRecordKind,
        scope: AuthorityScope,
        title: String,
        at: DateTime<Utc>,
    ) -> Result<Self, CrmDomainError> {
        if id.is_nil() {
            return Err(CrmDomainError::NilIdentifier {
                field: "crm_record_id",
            });
        }
        validate_scope(&scope)?;
        validate_title(&title)?;
        Ok(Self {
            id,
            version: 1,
            kind,
            scope,
            title,
            status: kind.initial_status(),
            created_at: at,
            updated_at: at,
        })
    }
    pub fn transition(
        &mut self,
        expected: u64,
        next: CrmRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), CrmDomainError> {
        if self.version != expected {
            return Err(CrmDomainError::RecordVersionConflict {
                expected,
                actual: self.version,
            });
        }
        if at < self.updated_at || !allowed(self.kind, self.status, next) {
            return Err(CrmDomainError::InvalidRecordTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(CrmDomainError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        Ok(())
    }
    pub fn validate_rehydrated(&self) -> Result<(), CrmDomainError> {
        if self.id.is_nil() || self.version == 0 || self.updated_at < self.created_at {
            return Err(CrmDomainError::InvalidRecordTransition);
        }
        validate_scope(&self.scope)?;
        validate_title(&self.title)
    }
    pub const fn id(&self) -> Uuid {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn kind(&self) -> CrmRecordKind {
        self.kind
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn status(&self) -> CrmRecordStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

const fn allowed(kind: CrmRecordKind, current: CrmRecordStatus, next: CrmRecordStatus) -> bool {
    use CrmRecordKind as K;
    use CrmRecordStatus as S;
    matches!(
        (kind, current, next),
        (K::Activity, S::Planned, S::InProgress | S::Cancelled)
            | (K::Activity, S::InProgress, S::Completed | S::Cancelled)
            | (K::Forecast, S::Draft, S::Submitted)
            | (K::Forecast, S::Submitted, S::Approved)
            | (K::Forecast, S::Approved, S::Superseded)
    )
}
fn validate_scope(scope: &AuthorityScope) -> Result<(), CrmDomainError> {
    scope.validate().map_err(|_| CrmDomainError::InvalidScope)?;
    if !matches!(
        scope,
        AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
    ) {
        Err(CrmDomainError::InvalidScope)
    } else {
        Ok(())
    }
}
fn validate_title(value: &str) -> Result<(), CrmDomainError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(CrmDomainError::Required {
            field: "crm_record_title",
        });
    }
    if value.len() > MAX_CRM_TEXT_BYTES {
        return Err(CrmDomainError::TooLong {
            field: "crm_record_title",
            maximum: MAX_CRM_TEXT_BYTES,
        });
    }
    if value.chars().any(char::is_control) {
        return Err(CrmDomainError::ControlCharacter {
            field: "crm_record_title",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn activity_and_forecast_lifecycles_are_distinct() {
        let scope = AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        let at = Utc.with_ymd_and_hms(2026, 8, 16, 8, 0, 0).single().unwrap();
        let mut forecast = CrmRecord::create(
            Uuid::from_u128(3),
            CrmRecordKind::Forecast,
            scope,
            "Q3 commit".into(),
            at,
        )
        .unwrap();
        assert!(
            forecast
                .transition(1, CrmRecordStatus::Completed, at)
                .is_err()
        );
        forecast
            .transition(1, CrmRecordStatus::Submitted, at)
            .unwrap();
        forecast
            .transition(2, CrmRecordStatus::Approved, at)
            .unwrap();
        assert_eq!(forecast.version(), 3);
    }
}
