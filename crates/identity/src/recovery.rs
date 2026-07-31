use std::collections::HashSet;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    IdentityError, PasswordDigest, PasswordService, SecretValue, TokenHash,
    totp::validate_recovery_session_lifetime,
};

const MIN_RECOVERY_CODE_COUNT: usize = 1;
const MAX_RECOVERY_CODE_COUNT: usize = 32;
const RECOVERY_CODE_BYTES: usize = 16;

#[derive(Clone)]
pub struct RecoveryCodeRecord {
    id: Uuid,
    password_hash: PasswordDigest,
    consumed_at: Option<DateTime<Utc>>,
}

impl RecoveryCodeRecord {
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn hash(&self) -> &PasswordDigest {
        &self.password_hash
    }

    pub fn consumed_at(&self) -> Option<DateTime<Utc>> {
        self.consumed_at
    }
}

impl std::fmt::Debug for RecoveryCodeRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecoveryCodeRecord")
            .field("id", &self.id)
            .field("password_hash", &self.password_hash)
            .field("consumed_at", &self.consumed_at)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct RecoveryCodeSet {
    principal_ref: String,
    issued_at: DateTime<Utc>,
    records: Vec<RecoveryCodeRecord>,
}

impl RecoveryCodeSet {
    pub fn issue(
        principal_ref: impl Into<String>,
        count: usize,
        service: &PasswordService,
        issued_at: DateTime<Utc>,
    ) -> Result<RecoveryCodeBatch, IdentityError> {
        let principal_ref = principal_ref.into();
        if principal_ref.trim().is_empty() {
            return Err(IdentityError::InvalidIdentityReference);
        }
        if !(MIN_RECOVERY_CODE_COUNT..=MAX_RECOVERY_CODE_COUNT).contains(&count) {
            return Err(IdentityError::InvalidRecoveryCodeCount);
        }
        let mut records = Vec::with_capacity(count);
        let mut codes = Vec::with_capacity(count);
        for _ in 0..count {
            let secret = crate::SecretValue::random(RECOVERY_CODE_BYTES)?;
            records.push(RecoveryCodeRecord {
                id: Uuid::new_v4(),
                password_hash: service.hash(secret.as_str())?,
                consumed_at: None,
            });
            codes.push(secret);
        }
        Ok(RecoveryCodeBatch {
            set: Self {
                principal_ref,
                issued_at,
                records,
            },
            codes,
        })
    }

    pub fn from_records(
        principal_ref: impl Into<String>,
        issued_at: DateTime<Utc>,
        records: Vec<RecoveryCodeRecord>,
    ) -> Result<Self, IdentityError> {
        let principal_ref = principal_ref.into();
        if principal_ref.trim().is_empty() {
            return Err(IdentityError::InvalidIdentityReference);
        }
        if !(MIN_RECOVERY_CODE_COUNT..=MAX_RECOVERY_CODE_COUNT).contains(&records.len()) {
            return Err(IdentityError::InvalidRecoveryCodeCount);
        }
        let mut ids = HashSet::with_capacity(records.len());
        for record in &records {
            if record.id.is_nil() || !ids.insert(record.id) {
                return Err(IdentityError::InvalidIdentityReference);
            }
        }
        Ok(Self {
            principal_ref,
            issued_at,
            records,
        })
    }

    pub fn principal_ref(&self) -> &str {
        &self.principal_ref
    }

    pub fn issued_at(&self) -> DateTime<Utc> {
        self.issued_at
    }

    pub fn records(&self) -> &[RecoveryCodeRecord] {
        &self.records
    }

    /// Returns a new immutable set with the matching hash marked consumed.
    /// Every stored hash is checked before returning, avoiding an early exit
    /// based on the position of the matching code.
    pub fn consume(
        &self,
        presented_code: &str,
        consumed_at: DateTime<Utc>,
        service: &PasswordService,
    ) -> Result<(Self, Uuid), IdentityError> {
        if presented_code.trim().is_empty() {
            return Err(IdentityError::InvalidRecoveryCode);
        }
        let mut match_id = None;
        let mut match_consumed = false;
        for record in &self.records {
            if service
                .verify(presented_code, &record.password_hash)
                .is_ok()
                && match_id.is_none()
            {
                match_id = Some(record.id);
                match_consumed = record.consumed_at.is_some();
            }
        }
        let id = match_id.ok_or(IdentityError::InvalidRecoveryCode)?;
        if match_consumed {
            return Err(IdentityError::RecoveryCodeAlreadyConsumed);
        }
        let mut next = self.clone();
        if let Some(record) = next.records.iter_mut().find(|record| record.id == id) {
            record.consumed_at = Some(consumed_at);
        }
        Ok((next, id))
    }
}

pub struct RecoveryCodeBatch {
    set: RecoveryCodeSet,
    codes: Vec<crate::SecretValue>,
}

impl RecoveryCodeBatch {
    pub fn set(&self) -> &RecoveryCodeSet {
        &self.set
    }

    pub fn code(&self, index: usize) -> Option<&str> {
        self.codes.get(index).map(crate::SecretValue::as_str)
    }

    pub fn codes(&self) -> impl Iterator<Item = &str> {
        self.codes.iter().map(crate::SecretValue::as_str)
    }

    pub fn into_parts(self) -> (RecoveryCodeSet, Vec<crate::SecretValue>) {
        (self.set, self.codes)
    }
}

impl std::fmt::Debug for RecoveryCodeBatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecoveryCodeBatch")
            .field("set", &self.set)
            .field("codes", &"REDACTED")
            .finish()
    }
}

/// A short-lived, one-time recovery handoff. Only the caller holding the
/// returned token can exchange it for a new device/session; PostgreSQL stores
/// only its SHA-256 digest.
pub struct RecoverySession {
    id: Uuid,
    principal_ref: String,
    token: SecretValue,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl RecoverySession {
    pub(crate) fn issue(
        principal_ref: impl Into<String>,
        issued_at: DateTime<Utc>,
        lifetime: chrono::Duration,
    ) -> Result<Self, IdentityError> {
        let principal_ref = principal_ref.into();
        if principal_ref.trim().is_empty() {
            return Err(IdentityError::InvalidIdentityReference);
        }
        validate_recovery_session_lifetime(lifetime)?;
        let expires_at = issued_at
            .checked_add_signed(lifetime)
            .ok_or(IdentityError::InvalidRecoverySessionLifetime)?;
        Ok(Self {
            id: Uuid::new_v4(),
            principal_ref,
            token: SecretValue::random(32)?,
            issued_at,
            expires_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn principal_ref(&self) -> &str {
        &self.principal_ref
    }

    pub fn token(&self) -> &SecretValue {
        &self.token
    }

    pub fn issued_at(&self) -> DateTime<Utc> {
        self.issued_at
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    pub(crate) fn token_hash(&self) -> TokenHash {
        TokenHash::of(self.token.as_str())
    }

    pub fn is_active_at(&self, at: DateTime<Utc>) -> bool {
        at >= self.issued_at && at < self.expires_at
    }
}

impl std::fmt::Debug for RecoverySession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecoverySession")
            .field("id", &self.id)
            .field("principal_ref", &self.principal_ref)
            .field("token", &"REDACTED")
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[cfg(test)]
mod session_tests {
    use crate::totp::MAX_RECOVERY_SESSION_LIFETIME;

    use super::*;

    #[test]
    fn recovery_sessions_are_bounded_and_debug_redacted() {
        let at = Utc::now();
        let session = RecoverySession::issue("principal-1", at, MAX_RECOVERY_SESSION_LIFETIME)
            .expect("session");
        assert!(session.is_active_at(at));
        assert!(!format!("{session:?}").contains(session.token().as_str()));
        assert!(matches!(
            RecoverySession::issue(
                "principal-1",
                at,
                MAX_RECOVERY_SESSION_LIFETIME + chrono::Duration::seconds(1),
            ),
            Err(IdentityError::InvalidRecoverySessionLifetime)
        ));
    }

    #[test]
    fn persisted_recovery_sets_reject_invalid_record_cardinality_and_identity() {
        let service = PasswordService::default();
        let issued_at = Utc::now();
        let digest = service.hash("recovery-code").expect("digest");
        let too_many = (0..=MAX_RECOVERY_CODE_COUNT)
            .map(|_| RecoveryCodeRecord {
                id: Uuid::new_v4(),
                password_hash: digest.clone(),
                consumed_at: None,
            })
            .collect();
        assert!(matches!(
            RecoveryCodeSet::from_records("principal-1", issued_at, too_many),
            Err(IdentityError::InvalidRecoveryCodeCount)
        ));

        let duplicate_id = Uuid::new_v4();
        let duplicate = vec![
            RecoveryCodeRecord {
                id: duplicate_id,
                password_hash: digest.clone(),
                consumed_at: None,
            },
            RecoveryCodeRecord {
                id: duplicate_id,
                password_hash: digest.clone(),
                consumed_at: None,
            },
        ];
        assert!(matches!(
            RecoveryCodeSet::from_records("principal-1", issued_at, duplicate),
            Err(IdentityError::InvalidIdentityReference)
        ));

        assert!(matches!(
            RecoveryCodeSet::from_records(
                "principal-1",
                issued_at,
                vec![RecoveryCodeRecord {
                    id: Uuid::nil(),
                    password_hash: digest,
                    consumed_at: None,
                }],
            ),
            Err(IdentityError::InvalidIdentityReference)
        ));
    }
}
