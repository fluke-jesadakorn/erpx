use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use uuid::Uuid;

use super::{
    DomainEvent, IdentityAccessDomainError, RiskTier, next_version, validate_scope, validate_text,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RoleId(Uuid);

impl RoleId {
    pub fn try_from_uuid(value: Uuid) -> Result<Self, IdentityAccessDomainError> {
        if value.is_nil() {
            return Err(IdentityAccessDomainError::NilIdentifier { field: "role_id" });
        }
        Ok(Self(value))
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl TryFrom<Uuid> for RoleId {
    type Error = IdentityAccessDomainError;

    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        Self::try_from_uuid(value)
    }
}

impl From<RoleId> for Uuid {
    fn from(value: RoleId) -> Self {
        value.0
    }
}

macro_rules! canonical_key {
    ($name:ident, $field:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn try_new(value: impl Into<String>) -> Result<Self, IdentityAccessDomainError> {
                let value = value.into();
                validate_text(&value, $field)?;
                if !value.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'_' | b'-' | b':')
                }) {
                    return Err(IdentityAccessDomainError::InvalidKey { field: $field });
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::try_new(value).map_err(de::Error::custom)
            }
        }
    };
}

canonical_key!(RoleCode, "role_code");
canonical_key!(PermissionKey, "permission");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessRole {
    id: RoleId,
    version: u64,
    scope: TenantScope,
    code: RoleCode,
    display_name: String,
    risk_tier: RiskTier,
    permissions: Vec<PermissionKey>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessRoleState {
    pub id: RoleId,
    pub version: u64,
    pub scope: TenantScope,
    pub code: RoleCode,
    pub display_name: String,
    pub risk_tier: RiskTier,
    pub permissions: Vec<PermissionKey>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AccessRole {
    pub fn try_rehydrate(state: AccessRoleState) -> Result<Self, IdentityAccessDomainError> {
        validate_scope(&state.scope)?;
        validate_text(&state.display_name, "display_name")?;
        if state.version == 0 || state.updated_at < state.created_at {
            return Err(IdentityAccessDomainError::InvalidTimestamp);
        }
        let value = Self {
            id: state.id,
            version: state.version,
            scope: state.scope,
            code: state.code,
            display_name: state.display_name,
            risk_tier: state.risk_tier,
            permissions: canonical_permissions(state.permissions)?,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        };
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn define(
        id: RoleId,
        scope: TenantScope,
        code: RoleCode,
        display_name: String,
        risk_tier: RiskTier,
        permissions: Vec<PermissionKey>,
        at: DateTime<Utc>,
    ) -> Result<Self, IdentityAccessDomainError> {
        validate_scope(&scope)?;
        validate_text(&display_name, "display_name")?;
        let permissions = canonical_permissions(permissions)?;
        let mut value = Self {
            id,
            version: 1,
            scope,
            code,
            display_name,
            risk_tier,
            permissions,
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.push_changed(at);
        Ok(value)
    }

    pub fn set_permissions(
        &mut self,
        permissions: Vec<PermissionKey>,
        at: DateTime<Utc>,
    ) -> Result<(), IdentityAccessDomainError> {
        if at < self.updated_at {
            return Err(IdentityAccessDomainError::InvalidTimestamp);
        }
        let permissions = canonical_permissions(permissions)?;
        let mut candidate = self.clone();
        candidate.version = next_version(candidate.version)?;
        candidate.permissions = permissions;
        candidate.updated_at = at;
        *self = candidate;
        self.push_changed(at);
        Ok(())
    }

    fn push_changed(&mut self, at: DateTime<Utc>) {
        self.uncommitted_events
            .push(DomainEvent::RoleDefinitionChanged {
                role_id: self.id,
                code: self.code.as_str().to_owned(),
                at,
            });
    }

    pub const fn id(&self) -> RoleId {
        self.id
    }

    pub const fn version(&self) -> u64 {
        self.version
    }

    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }

    pub const fn code(&self) -> &RoleCode {
        &self.code
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub const fn risk_tier(&self) -> RiskTier {
        self.risk_tier
    }

    pub fn permissions(&self) -> &[PermissionKey] {
        &self.permissions
    }

    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

fn canonical_permissions(
    mut permissions: Vec<PermissionKey>,
) -> Result<Vec<PermissionKey>, IdentityAccessDomainError> {
    if permissions.is_empty() {
        return Err(IdentityAccessDomainError::InvalidPermissionSet);
    }
    permissions.sort_unstable();
    let original_len = permissions.len();
    permissions.dedup();
    if permissions.len() != original_len {
        return Err(IdentityAccessDomainError::InvalidPermissionSet);
    }
    Ok(permissions)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 5, hour, 0, 0)
            .single()
            .unwrap()
    }

    fn permission(value: &str) -> PermissionKey {
        PermissionKey::try_new(value).unwrap()
    }

    #[test]
    fn role_definition_requires_canonical_unique_permissions() {
        let role = AccessRole::define(
            RoleId::try_from_uuid(id(1)).unwrap(),
            TenantScope::Platform,
            RoleCode::try_new("platform.admin").unwrap(),
            "Platform administrator".into(),
            RiskTier::Administrator,
            vec![permission("role.read"), permission("role.write")],
            at(9),
        )
        .unwrap();
        assert_eq!(role.version(), 1);
        assert_eq!(role.permissions().len(), 2);

        assert!(matches!(
            RoleCode::try_new("invalid role"),
            Err(IdentityAccessDomainError::InvalidKey { .. })
        ));
    }

    #[test]
    fn rejected_permission_update_preserves_role() {
        let mut role = AccessRole::define(
            RoleId::try_from_uuid(id(1)).unwrap(),
            TenantScope::Platform,
            RoleCode::try_new("security.reviewer").unwrap(),
            "Security reviewer".into(),
            RiskTier::High,
            vec![permission("role.read")],
            at(9),
        )
        .unwrap();
        let before = role.clone();
        assert_eq!(
            role.set_permissions(
                vec![permission("role.read"), permission("role.read")],
                at(10)
            ),
            Err(IdentityAccessDomainError::InvalidPermissionSet)
        );
        assert_eq!(role, before);
    }
}
