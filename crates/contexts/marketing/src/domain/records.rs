use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::MarketingDomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketingRecordKind {
    Campaign,
    Content,
    AudienceActivation,
    ConsentPurpose,
}

impl MarketingRecordKind {
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Campaign => "marketing.campaign",
            Self::Content => "marketing.content",
            Self::AudienceActivation => "marketing.audience-activation",
            Self::ConsentPurpose => "marketing.consent-purpose",
        }
    }

    pub const fn initial_status(self) -> MarketingRecordStatus {
        match self {
            Self::AudienceActivation => MarketingRecordStatus::Proposed,
            Self::Campaign | Self::Content | Self::ConsentPurpose => MarketingRecordStatus::Draft,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketingRecordStatus {
    Draft,
    Proposed,
    Approved,
    Active,
    Paused,
    Running,
    Published,
    Completed,
    Cancelled,
    Retired,
}

impl MarketingRecordStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Proposed => "proposed",
            Self::Approved => "approved",
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Running => "running",
            Self::Published => "published",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Retired => "retired",
        }
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Retired)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingRecord {
    id: Uuid,
    version: u64,
    kind: MarketingRecordKind,
    scope: TenantScope,
    title: String,
    status: MarketingRecordStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl MarketingRecord {
    pub fn create(
        id: Uuid,
        kind: MarketingRecordKind,
        scope: TenantScope,
        title: String,
        at: DateTime<Utc>,
    ) -> Result<Self, MarketingDomainError> {
        if id.is_nil() {
            return Err(MarketingDomainError::NilIdentifier {
                field: "marketing_record_id",
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
        expected_version: u64,
        next: MarketingRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), MarketingDomainError> {
        if self.version != expected_version {
            return Err(MarketingDomainError::RecordVersionConflict {
                expected: expected_version,
                actual: self.version,
            });
        }
        if at < self.updated_at || !allowed_transition(self.kind, self.status, next) {
            return Err(MarketingDomainError::InvalidRecordTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(MarketingDomainError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        Ok(())
    }

    pub fn validate_rehydrated(&self) -> Result<(), MarketingDomainError> {
        if self.id.is_nil() || self.version == 0 || self.updated_at < self.created_at {
            return Err(MarketingDomainError::InvalidRecordTransition);
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
    pub const fn kind(&self) -> MarketingRecordKind {
        self.kind
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn status(&self) -> MarketingRecordStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

const fn allowed_transition(
    kind: MarketingRecordKind,
    current: MarketingRecordStatus,
    next: MarketingRecordStatus,
) -> bool {
    use MarketingRecordKind as K;
    use MarketingRecordStatus as S;
    matches!(
        (kind, current, next),
        (K::Campaign, S::Draft, S::Active)
            | (K::Campaign, S::Active, S::Paused)
            | (K::Campaign, S::Paused, S::Active)
            | (
                K::Campaign,
                S::Active | S::Paused,
                S::Completed | S::Cancelled
            )
            | (K::Content, S::Draft, S::Approved)
            | (K::Content, S::Approved, S::Published)
            | (K::Content, S::Published, S::Retired)
            | (K::AudienceActivation, S::Proposed, S::Running)
            | (
                K::AudienceActivation,
                S::Running,
                S::Completed | S::Cancelled
            )
            | (K::ConsentPurpose, S::Draft, S::Active)
            | (K::ConsentPurpose, S::Active, S::Retired)
    )
}

fn validate_scope(scope: &TenantScope) -> Result<(), MarketingDomainError> {
    scope
        .validate()
        .map_err(|_| MarketingDomainError::InvalidScope)?;
    if !matches!(
        scope,
        TenantScope::Company { .. } | TenantScope::Branch { .. }
    ) {
        return Err(MarketingDomainError::InvalidScope);
    }
    Ok(())
}

fn validate_title(value: &str) -> Result<(), MarketingDomainError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(MarketingDomainError::Required {
            field: "marketing_record_title",
        });
    }
    if trimmed.len() > super::MAX_MARKETING_TEXT_BYTES {
        return Err(MarketingDomainError::TooLong {
            field: "marketing_record_title",
            maximum: super::MAX_MARKETING_TEXT_BYTES,
        });
    }
    if trimmed.chars().any(char::is_control) {
        return Err(MarketingDomainError::ControlCharacter {
            field: "marketing_record_title",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 16, hour, 0, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn every_record_kind_has_a_governed_distinct_lifecycle() {
        let scope = TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        let mut content = MarketingRecord::create(
            Uuid::from_u128(3),
            MarketingRecordKind::Content,
            scope,
            "Launch copy".into(),
            at(8),
        )
        .unwrap();
        assert_eq!(content.status(), MarketingRecordStatus::Draft);
        assert!(
            content
                .transition(1, MarketingRecordStatus::Published, at(9))
                .is_err()
        );
        assert_eq!(content.version(), 1);
        content
            .transition(1, MarketingRecordStatus::Approved, at(9))
            .unwrap();
        content
            .transition(2, MarketingRecordStatus::Published, at(10))
            .unwrap();
        content
            .transition(3, MarketingRecordStatus::Retired, at(11))
            .unwrap();
        assert!(content.status().is_terminal());
    }
}
