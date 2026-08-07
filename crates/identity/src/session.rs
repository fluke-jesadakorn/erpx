use std::fmt;

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::{
    AccessToken, AccessTokenLifetime, AccessTokenSigner, ActiveRoleClaim, IdentityError,
    SecretValue, TokenHash,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    id: Uuid,
    principal_ref: String,
    device_id: Uuid,
    refresh_token_hash: TokenHash,
    issued_at: DateTime<Utc>,
    refresh_expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    last_used_at: DateTime<Utc>,
    active_role_source: Option<String>,
    active_role_selection_id: Option<String>,
    authorization_context_version: u64,
}

impl SessionRecord {
    pub(crate) fn from_persisted(value: PersistedSession) -> Result<Self, IdentityError> {
        let principal_ref = value.principal_ref;
        if principal_ref.trim().is_empty() {
            return Err(IdentityError::InvalidIdentityReference);
        }
        if value.id.is_nil() || value.device_id.is_nil() {
            return Err(IdentityError::InvalidIdentityReference);
        }
        if value.refresh_expires_at <= value.issued_at {
            return Err(IdentityError::SessionInactive);
        }
        if value.last_used_at < value.issued_at
            || value.revoked_at.is_some_and(|revoked_at| {
                revoked_at < value.issued_at || revoked_at > value.last_used_at
            })
        {
            return Err(IdentityError::SessionInactive);
        }
        if value.authorization_context_version == 0
            || value.active_role_source.is_some() != value.active_role_selection_id.is_some()
            || value.active_role_source.as_ref().is_some_and(|source| {
                !matches!(source.as_str(), "local" | "directory")
                    || source.chars().any(char::is_control)
            })
            || value
                .active_role_selection_id
                .as_ref()
                .is_some_and(|selection| {
                    selection.trim().is_empty() || selection.chars().any(char::is_control)
                })
        {
            return Err(IdentityError::SessionInactive);
        }
        Ok(Self {
            id: value.id,
            principal_ref,
            device_id: value.device_id,
            refresh_token_hash: value.refresh_token_hash,
            issued_at: value.issued_at,
            refresh_expires_at: value.refresh_expires_at,
            revoked_at: value.revoked_at,
            last_used_at: value.last_used_at,
            active_role_source: value.active_role_source,
            active_role_selection_id: value.active_role_selection_id,
            authorization_context_version: value.authorization_context_version,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn principal_ref(&self) -> &str {
        &self.principal_ref
    }
    pub fn device_id(&self) -> Uuid {
        self.device_id
    }
    pub fn issued_at(&self) -> DateTime<Utc> {
        self.issued_at
    }
    pub fn refresh_expires_at(&self) -> DateTime<Utc> {
        self.refresh_expires_at
    }
    pub fn revoked_at(&self) -> Option<DateTime<Utc>> {
        self.revoked_at
    }
    pub fn last_used_at(&self) -> DateTime<Utc> {
        self.last_used_at
    }
    pub fn is_revoked_at(&self, at: DateTime<Utc>) -> bool {
        self.revoked_at.is_some_and(|revoked_at| at >= revoked_at)
    }
    pub fn is_active_at(&self, at: DateTime<Utc>) -> bool {
        at >= self.issued_at && !self.is_revoked_at(at) && at < self.refresh_expires_at
    }
    pub fn refresh_token_hash(&self) -> &TokenHash {
        &self.refresh_token_hash
    }
    pub fn active_role_source(&self) -> Option<&str> {
        self.active_role_source.as_deref()
    }
    pub fn active_role_selection_id(&self) -> Option<&str> {
        self.active_role_selection_id.as_deref()
    }
    pub fn authorization_context_version(&self) -> u64 {
        self.authorization_context_version
    }
    pub fn revoke_at(&self, at: DateTime<Utc>) -> Self {
        let mut next = self.clone();
        next.revoked_at = Some(self.revoked_at.map_or(at, |revoked_at| revoked_at.min(at)));
        next.last_used_at = self.last_used_at.max(at);
        next
    }
}

#[derive(Clone)]
pub(crate) struct PersistedSession {
    pub(crate) id: Uuid,
    pub(crate) principal_ref: String,
    pub(crate) device_id: Uuid,
    pub(crate) refresh_token_hash: TokenHash,
    pub(crate) issued_at: DateTime<Utc>,
    pub(crate) refresh_expires_at: DateTime<Utc>,
    pub(crate) revoked_at: Option<DateTime<Utc>>,
    pub(crate) last_used_at: DateTime<Utc>,
    pub(crate) active_role_source: Option<String>,
    pub(crate) active_role_selection_id: Option<String>,
    pub(crate) authorization_context_version: u64,
}

#[derive(Clone, PartialEq, Eq)]
pub struct IssuedSession {
    record: SessionRecord,
    access_token: AccessToken,
    refresh_token: SecretValue,
}

impl IssuedSession {
    pub fn record(&self) -> &SessionRecord {
        &self.record
    }
    pub fn access_token(&self) -> &AccessToken {
        &self.access_token
    }
    pub fn refresh_token(&self) -> &SecretValue {
        &self.refresh_token
    }
}

impl fmt::Debug for IssuedSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IssuedSession")
            .field("record", &self.record)
            .field("access_token", &self.access_token)
            .field("refresh_token", &"REDACTED")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct RotatedSession {
    record: SessionRecord,
    access_token: AccessToken,
    refresh_token: SecretValue,
}

impl RotatedSession {
    pub fn record(&self) -> &SessionRecord {
        &self.record
    }
    pub fn access_token(&self) -> &AccessToken {
        &self.access_token
    }
    pub fn refresh_token(&self) -> &SecretValue {
        &self.refresh_token
    }
}

impl fmt::Debug for RotatedSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RotatedSession")
            .field("record", &self.record)
            .field("access_token", &self.access_token)
            .field("refresh_token", &"REDACTED")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshRotationOutcome {
    Rotated(RotatedSession),
    ReuseDetected { revoked_session: SessionRecord },
}

#[derive(Debug, Clone)]
pub struct SessionService {
    access_tokens: AccessTokenSigner,
    refresh_lifetime: Duration,
}

impl SessionService {
    pub fn new(
        access_tokens: AccessTokenSigner,
        refresh_lifetime: Duration,
    ) -> Result<Self, IdentityError> {
        if refresh_lifetime <= Duration::zero() {
            return Err(IdentityError::InvalidAccessTokenLifetime);
        }
        Ok(Self {
            access_tokens,
            refresh_lifetime,
        })
    }

    pub fn issue(
        &self,
        principal_ref: impl Into<String>,
        device_id: Uuid,
        at: DateTime<Utc>,
        access_lifetime: AccessTokenLifetime,
    ) -> Result<IssuedSession, IdentityError> {
        let principal_ref = principal_ref.into();
        if principal_ref.trim().is_empty() {
            return Err(IdentityError::InvalidIdentityReference);
        }
        if device_id.is_nil() {
            return Err(IdentityError::InvalidIdentityReference);
        }
        let id = Uuid::new_v4();
        let refresh_token = SecretValue::random(32)?;
        let record = SessionRecord {
            id,
            principal_ref: principal_ref.clone(),
            device_id,
            refresh_token_hash: TokenHash::of(refresh_token.as_str()),
            issued_at: at,
            refresh_expires_at: at + self.refresh_lifetime,
            revoked_at: None,
            last_used_at: at,
            active_role_source: None,
            active_role_selection_id: None,
            authorization_context_version: 1,
        };
        let access_token = self.access_tokens.issue_with_context(
            principal_ref,
            id,
            None,
            Some(1),
            at,
            access_lifetime,
        )?;
        Ok(IssuedSession {
            record,
            access_token,
            refresh_token,
        })
    }

    pub fn rotate_refresh(
        &self,
        record: &SessionRecord,
        presented_refresh_token: &str,
        at: DateTime<Utc>,
    ) -> Result<RefreshRotationOutcome, IdentityError> {
        if !record.is_active_at(at) {
            return Err(IdentityError::SessionInactive);
        }
        let provided = TokenHash::of(presented_refresh_token);
        if !record.refresh_token_hash.equals_constant_time(&provided) {
            return Ok(RefreshRotationOutcome::ReuseDetected {
                revoked_session: record.revoke_at(at),
            });
        }
        let refresh_token = SecretValue::random(32)?;
        let next_record = SessionRecord {
            refresh_token_hash: TokenHash::of(refresh_token.as_str()),
            last_used_at: record.last_used_at.max(at),
            ..record.clone()
        };
        let active_role = record
            .active_role_source
            .as_ref()
            .zip(record.active_role_selection_id.as_ref())
            .map(|(source, selection_id)| ActiveRoleClaim {
                source: source.clone(),
                selection_id: selection_id.clone(),
            });
        let access_token = self.access_tokens.issue_with_context(
            record.principal_ref.clone(),
            record.id,
            active_role,
            Some(record.authorization_context_version),
            at,
            AccessTokenLifetime::five_minutes(),
        )?;
        Ok(RefreshRotationOutcome::Rotated(RotatedSession {
            record: next_record,
            access_token,
            refresh_token,
        }))
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};

    use super::*;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 27, 12, 0, 0)
            .single()
            .expect("fixed test time")
    }

    #[test]
    fn session_is_inactive_before_issuance_and_cannot_refresh_early() {
        let service = SessionService::new(
            AccessTokenSigner::new(b"01234567890123456789012345678901").expect("signer"),
            Duration::days(30),
        )
        .expect("service");
        let issued = service
            .issue(
                "principal-1",
                Uuid::new_v4(),
                at(),
                AccessTokenLifetime::five_minutes(),
            )
            .expect("session");

        assert!(
            !issued
                .record()
                .is_active_at(at() - Duration::nanoseconds(1))
        );
        assert!(matches!(
            service.rotate_refresh(
                issued.record(),
                issued.refresh_token().as_str(),
                at() - Duration::nanoseconds(1),
            ),
            Err(IdentityError::SessionInactive)
        ));
    }

    #[test]
    fn session_rejects_invalid_persisted_identity_and_time_ordering() {
        let issued_at = at();
        let base = PersistedSession {
            id: Uuid::new_v4(),
            principal_ref: "principal-1".into(),
            device_id: Uuid::new_v4(),
            refresh_token_hash: TokenHash::of("refresh"),
            issued_at,
            refresh_expires_at: issued_at + Duration::days(30),
            revoked_at: None,
            last_used_at: issued_at,
            active_role_source: None,
            active_role_selection_id: None,
            authorization_context_version: 1,
        };
        assert!(matches!(
            SessionRecord::from_persisted(PersistedSession {
                id: Uuid::nil(),
                ..base.clone()
            }),
            Err(IdentityError::InvalidIdentityReference)
        ));
        assert!(matches!(
            SessionRecord::from_persisted(PersistedSession {
                last_used_at: issued_at - Duration::seconds(1),
                ..base.clone()
            }),
            Err(IdentityError::SessionInactive)
        ));
        assert!(matches!(
            SessionRecord::from_persisted(PersistedSession {
                revoked_at: Some(issued_at - Duration::seconds(1)),
                ..base
            }),
            Err(IdentityError::SessionInactive)
        ));

        let base = PersistedSession {
            id: Uuid::new_v4(),
            principal_ref: "principal-1".into(),
            device_id: Uuid::new_v4(),
            refresh_token_hash: TokenHash::of("refresh"),
            issued_at,
            refresh_expires_at: issued_at + Duration::days(30),
            revoked_at: Some(issued_at + Duration::seconds(2)),
            last_used_at: issued_at + Duration::seconds(1),
            active_role_source: None,
            active_role_selection_id: None,
            authorization_context_version: 1,
        };
        assert!(matches!(
            SessionRecord::from_persisted(base),
            Err(IdentityError::SessionInactive)
        ));
    }

    #[test]
    fn revocation_and_last_used_timestamps_never_regress() {
        let service = SessionService::new(
            AccessTokenSigner::new(b"01234567890123456789012345678901").expect("signer"),
            Duration::days(30),
        )
        .expect("service");
        let issued = service
            .issue(
                "principal-1",
                Uuid::new_v4(),
                at(),
                AccessTokenLifetime::five_minutes(),
            )
            .expect("session");
        let revoked_later = issued.record().revoke_at(at() + Duration::seconds(5));
        let revoked_earlier = revoked_later.revoke_at(at() + Duration::seconds(2));
        assert_eq!(
            revoked_earlier.revoked_at(),
            Some(at() + Duration::seconds(2))
        );
        assert_eq!(revoked_earlier.last_used_at(), at() + Duration::seconds(5));

        assert!(matches!(
            service.rotate_refresh(
                issued.record(),
                issued.refresh_token().as_str(),
                at() - Duration::seconds(1),
            ),
            Err(IdentityError::SessionInactive)
        ));
    }

    #[test]
    fn refresh_preserves_exact_selected_role_and_context_version() {
        let service = SessionService::new(
            AccessTokenSigner::new(b"01234567890123456789012345678901").expect("signer"),
            Duration::days(30),
        )
        .expect("service");
        let selection_id = Uuid::new_v4().to_string();
        let record = SessionRecord::from_persisted(PersistedSession {
            id: Uuid::new_v4(),
            principal_ref: "principal-selected".into(),
            device_id: Uuid::new_v4(),
            refresh_token_hash: TokenHash::of("refresh-selected"),
            issued_at: at(),
            refresh_expires_at: at() + Duration::days(30),
            revoked_at: None,
            last_used_at: at(),
            active_role_source: Some("local".into()),
            active_role_selection_id: Some(selection_id.clone()),
            authorization_context_version: 9,
        })
        .expect("persisted selected-role session");

        let RefreshRotationOutcome::Rotated(rotated) = service
            .rotate_refresh(&record, "refresh-selected", at() + Duration::seconds(1))
            .expect("refresh rotation")
        else {
            panic!("unexpected reuse outcome");
        };
        assert_eq!(rotated.record().active_role_source(), Some("local"));
        assert_eq!(
            rotated.record().active_role_selection_id(),
            Some(selection_id.as_str())
        );
        assert_eq!(rotated.record().authorization_context_version(), 9);
        assert_eq!(
            rotated.access_token().claims().active_role,
            Some(ActiveRoleClaim {
                source: "local".into(),
                selection_id,
            })
        );
        assert_eq!(
            rotated
                .access_token()
                .claims()
                .authorization_context_version,
            Some(9)
        );
    }

    #[test]
    fn persisted_active_role_requires_a_complete_pair_and_positive_context() {
        let base = PersistedSession {
            id: Uuid::new_v4(),
            principal_ref: "principal-1".into(),
            device_id: Uuid::new_v4(),
            refresh_token_hash: TokenHash::of("refresh"),
            issued_at: at(),
            refresh_expires_at: at() + Duration::days(30),
            revoked_at: None,
            last_used_at: at(),
            active_role_source: Some("local".into()),
            active_role_selection_id: None,
            authorization_context_version: 1,
        };
        assert!(matches!(
            SessionRecord::from_persisted(base.clone()),
            Err(IdentityError::SessionInactive)
        ));
        assert!(matches!(
            SessionRecord::from_persisted(PersistedSession {
                active_role_source: None,
                authorization_context_version: 0,
                ..base
            }),
            Err(IdentityError::SessionInactive)
        ));
    }
}
