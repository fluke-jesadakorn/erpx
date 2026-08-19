use chrono::{DateTime, Utc};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FinanceRecordError {
    #[error("Finance record identifier cannot be nil")]
    NilIdentifier,
    #[error("Finance record requires an exact company or branch scope")]
    InvalidScope,
    #[error("Finance record title is invalid")]
    InvalidTitle,
    #[error("Finance record transition is not allowed")]
    InvalidTransition,
    #[error("Finance record version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("Finance record version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceRecordKind {
    CreditNote,
    DebitNote,
    Payment,
    Reconciliation,
}
impl FinanceRecordKind {
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::CreditNote => "finance.credit-note",
            Self::DebitNote => "finance.debit-note",
            Self::Payment => "finance.payment",
            Self::Reconciliation => "finance.reconciliation",
        }
    }
    const fn initial(self) -> FinanceRecordStatus {
        match self {
            Self::CreditNote | Self::DebitNote => FinanceRecordStatus::Draft,
            Self::Payment => FinanceRecordStatus::Pending,
            Self::Reconciliation => FinanceRecordStatus::Open,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceRecordStatus {
    Draft,
    Pending,
    Open,
    Posted,
    Authorized,
    Matched,
    Applied,
    Settled,
    Closed,
    Failed,
    Cancelled,
    Void,
}
impl FinanceRecordStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Pending => "pending",
            Self::Open => "open",
            Self::Posted => "posted",
            Self::Authorized => "authorized",
            Self::Matched => "matched",
            Self::Applied => "applied",
            Self::Settled => "settled",
            Self::Closed => "closed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Void => "void",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceRecord {
    id: Uuid,
    version: u64,
    kind: FinanceRecordKind,
    scope: AuthorityScope,
    title: String,
    status: FinanceRecordStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl FinanceRecord {
    pub fn create(
        id: Uuid,
        kind: FinanceRecordKind,
        scope: AuthorityScope,
        title: String,
        at: DateTime<Utc>,
    ) -> Result<Self, FinanceRecordError> {
        if id.is_nil() {
            return Err(FinanceRecordError::NilIdentifier);
        }
        validate_scope(&scope)?;
        validate_title(&title)?;
        Ok(Self {
            id,
            version: 1,
            kind,
            scope,
            title,
            status: kind.initial(),
            created_at: at,
            updated_at: at,
        })
    }
    pub fn transition(
        &mut self,
        expected: u64,
        next: FinanceRecordStatus,
        at: DateTime<Utc>,
    ) -> Result<(), FinanceRecordError> {
        if self.version != expected {
            return Err(FinanceRecordError::VersionConflict {
                expected,
                actual: self.version,
            });
        }
        if at < self.updated_at || !allowed(self.kind, self.status, next) {
            return Err(FinanceRecordError::InvalidTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(FinanceRecordError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        Ok(())
    }
    pub fn validate_rehydrated(&self) -> Result<(), FinanceRecordError> {
        if self.id.is_nil() || self.version == 0 || self.updated_at < self.created_at {
            return Err(FinanceRecordError::InvalidTransition);
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
    pub const fn kind(&self) -> FinanceRecordKind {
        self.kind
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn status(&self) -> FinanceRecordStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}
const fn allowed(
    kind: FinanceRecordKind,
    current: FinanceRecordStatus,
    next: FinanceRecordStatus,
) -> bool {
    use FinanceRecordKind as K;
    use FinanceRecordStatus as S;
    matches!(
        (kind, current, next),
        (K::CreditNote | K::DebitNote, S::Draft, S::Posted | S::Void)
            | (
                K::CreditNote | K::DebitNote,
                S::Posted,
                S::Applied | S::Void
            )
            | (
                K::Payment,
                S::Pending,
                S::Authorized | S::Cancelled | S::Failed
            )
            | (
                K::Payment,
                S::Authorized,
                S::Settled | S::Failed | S::Cancelled
            )
            | (K::Reconciliation, S::Open, S::Matched)
            | (K::Reconciliation, S::Matched, S::Open | S::Closed)
    )
}
fn validate_scope(scope: &AuthorityScope) -> Result<(), FinanceRecordError> {
    scope
        .validate()
        .map_err(|_| FinanceRecordError::InvalidScope)?;
    if matches!(
        scope,
        AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(FinanceRecordError::InvalidScope)
    }
}
fn validate_title(value: &str) -> Result<(), FinanceRecordError> {
    let v = value.trim();
    if v.is_empty() || v.len() > 1024 || v.chars().any(char::is_control) {
        Err(FinanceRecordError::InvalidTitle)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn payment_and_reconciliation_are_governed_separately() {
        let scope = AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        let at = Utc.with_ymd_and_hms(2026, 8, 16, 8, 0, 0).single().unwrap();
        let mut payment = FinanceRecord::create(
            Uuid::from_u128(3),
            FinanceRecordKind::Payment,
            scope,
            "Settlement".into(),
            at,
        )
        .unwrap();
        assert!(
            payment
                .transition(1, FinanceRecordStatus::Closed, at)
                .is_err()
        );
        payment
            .transition(1, FinanceRecordStatus::Authorized, at)
            .unwrap();
        payment
            .transition(2, FinanceRecordStatus::Settled, at)
            .unwrap();
        assert_eq!(payment.version(), 3);
    }
}
