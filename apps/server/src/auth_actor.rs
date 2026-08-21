use erpx_domain::contracts::ActorKind as LegacyActorKind;
use erpx_identity::{AccessTokenClaims, AuthenticatedPrincipalKind, AuthenticatedRealm};
use erpx_kernel::ActorKind;

/// Derive trusted actor attribution only from authenticated token claims.
/// Presented transport actor kinds are never authoritative at this boundary.
pub(crate) fn kernel_actor_kind(claims: &AccessTokenClaims) -> ActorKind {
    match claims.principal_kind {
        AuthenticatedPrincipalKind::Human => match claims.realm {
            AuthenticatedRealm::PlatformAdministration => ActorKind::PlatformUser,
            _ => ActorKind::HoldingUser,
        },
        AuthenticatedPrincipalKind::Agent => ActorKind::Agent,
        AuthenticatedPrincipalKind::Service => ActorKind::Service,
    }
}

/// The legacy command contract has no Agent variant, so authenticated agents
/// retain non-human attribution through its Service compatibility value.
pub(crate) fn legacy_actor_kind(claims: &AccessTokenClaims) -> LegacyActorKind {
    match claims.principal_kind {
        AuthenticatedPrincipalKind::Human => match claims.realm {
            AuthenticatedRealm::PlatformAdministration => LegacyActorKind::PlatformUser,
            _ => LegacyActorKind::HoldingUser,
        },
        AuthenticatedPrincipalKind::Agent | AuthenticatedPrincipalKind::Service => {
            LegacyActorKind::Service
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use uuid::Uuid;

    use super::*;

    fn claims(
        realm: AuthenticatedRealm,
        principal_kind: AuthenticatedPrincipalKind,
    ) -> AccessTokenClaims {
        let issued_at = Utc::now();
        AccessTokenClaims {
            principal_ref: Uuid::new_v4().to_string(),
            session_id: Uuid::new_v4(),
            active_role: None,
            authorization_context_version: Some(1),
            realm,
            principal_kind,
            issued_at,
            expires_at: issued_at + Duration::minutes(5),
        }
    }

    #[test]
    fn human_actor_kind_follows_the_authenticated_realm() {
        let workforce = claims(
            AuthenticatedRealm::InternalWorkforce,
            AuthenticatedPrincipalKind::Human,
        );
        assert_eq!(kernel_actor_kind(&workforce), ActorKind::HoldingUser);
        assert_eq!(legacy_actor_kind(&workforce), LegacyActorKind::HoldingUser);

        let platform = claims(
            AuthenticatedRealm::PlatformAdministration,
            AuthenticatedPrincipalKind::Human,
        );
        assert_eq!(kernel_actor_kind(&platform), ActorKind::PlatformUser);
        assert_eq!(legacy_actor_kind(&platform), LegacyActorKind::PlatformUser);
    }

    #[test]
    fn non_human_actor_kind_is_preserved_without_legacy_human_impersonation() {
        let agent = claims(
            AuthenticatedRealm::InternalWorkforce,
            AuthenticatedPrincipalKind::Agent,
        );
        assert_eq!(kernel_actor_kind(&agent), ActorKind::Agent);
        assert_eq!(legacy_actor_kind(&agent), LegacyActorKind::Service);

        let service = claims(
            AuthenticatedRealm::PlatformAdministration,
            AuthenticatedPrincipalKind::Service,
        );
        assert_eq!(kernel_actor_kind(&service), ActorKind::Service);
        assert_eq!(legacy_actor_kind(&service), LegacyActorKind::Service);
    }
}
