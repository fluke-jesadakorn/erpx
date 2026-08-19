//! SQLx record-policy authorization for Collaboration.
//!
//! Thread lookup reads only the opaque anchor index. The Item Directory grant
//! is then checked before audience, snapshot, comment, relation, or stream data
//! can be inspected.

use async_trait::async_trait;
use erpx_collaboration::{
    ActorContext, AnchorPolicy, Audience, AuthorizationPort, AuthorizedSearchAccess,
    AuthorizedThreadAccess, CollaborationAction, PortError, ReferenceRelation, ThreadId,
};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::{AuthorityScope, HoldingId, RecordId};
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::{DirectoryAccess, DirectoryLookup, ProjectionFreshness, SqlxItemDirectory};

#[derive(Debug, Clone)]
pub struct SqlxCollaborationAuthorization {
    write_pool: PgPool,
    directory: SqlxItemDirectory,
    allowed_actions: Vec<CollaborationAction>,
}

impl SqlxCollaborationAuthorization {
    /// `allowed_actions` is supplied by the server only after current role and
    /// realm authorization; the adapter narrows it further using record grants
    /// and thread audience policy.
    pub fn from_authorized_request(
        write_pool: PgPool,
        read_pool: PgPool,
        allowed_actions: impl IntoIterator<Item = CollaborationAction>,
    ) -> Self {
        Self {
            write_pool,
            directory: SqlxItemDirectory::new(read_pool),
            allowed_actions: allowed_actions.into_iter().collect(),
        }
    }

    fn permits(&self, action: CollaborationAction) -> Result<(), PortError> {
        self.allowed_actions
            .contains(&action)
            .then_some(())
            .ok_or(PortError::Denied)
    }

    fn directory_access(&self, actor: &ActorContext) -> Result<DirectoryAccess, PortError> {
        DirectoryAccess::try_from_authorization(
            actor.holding_id,
            actor.principal.id(),
            actor.authority_scope.clone(),
        )
        .map_err(|_| PortError::Denied)
    }

    async fn authorize_record(
        &self,
        actor: &ActorContext,
        record_id: RecordId,
    ) -> Result<(), PortError> {
        self.directory
            .authorize_record(&self.directory_access(actor)?, record_id)
            .await
            .map_err(|_| PortError::Unavailable)?
            .is_some()
            .then_some(())
            .ok_or(PortError::Denied)
    }

    async fn opaque_anchor(
        &self,
        actor: &ActorContext,
        thread_id: ThreadId,
    ) -> Result<RecordId, PortError> {
        let mut tx = self.write_pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, actor.holding_id).await?;
        let value = sqlx::query_scalar::<_, uuid::Uuid>(
            "SELECT anchor_record_id \
             FROM erpx_collaboration.thread_anchor_authority_index \
             WHERE holding_id=$1 AND thread_id=$2",
        )
        .bind(actor.holding_id.as_uuid())
        .bind(thread_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        value
            .and_then(|value| RecordId::try_from_uuid(value).ok())
            .ok_or(PortError::Denied)
    }

    async fn enforce_thread_audience(
        &self,
        actor: &ActorContext,
        thread_id: ThreadId,
        anchor_record_id: RecordId,
    ) -> Result<(), PortError> {
        let mut tx = self.write_pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, actor.holding_id).await?;
        let row = sqlx::query(
            "SELECT policy.policy_document->'visibility_scope' AS visibility_scope, \
                    audience.audience_document AS audience \
             FROM erpx_collaboration.threads thread \
             JOIN erpx_collaboration.thread_policies policy \
               ON policy.holding_id=thread.holding_id AND policy.thread_id=thread.thread_id \
             JOIN erpx_collaboration.thread_audiences audience \
               ON audience.holding_id=thread.holding_id AND audience.thread_id=thread.thread_id \
             WHERE thread.holding_id=$1 AND thread.thread_id=$2 AND thread.anchor_record_id=$3",
        )
        .bind(actor.holding_id.as_uuid())
        .bind(thread_id.as_uuid())
        .bind(anchor_record_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        let Some(row) = row else {
            return Err(PortError::Denied);
        };
        let visibility = serde_json::from_value::<AuthorityScope>(
            row.try_get("visibility_scope")
                .map_err(|_| PortError::Unavailable)?,
        )
        .map_err(|_| PortError::Unavailable)?;
        let audience = serde_json::from_value::<Audience>(
            row.try_get("audience")
                .map_err(|_| PortError::Unavailable)?,
        )
        .map_err(|_| PortError::Unavailable)?;
        if !actor.authority_scope.contains(&visibility) || !audience.includes(actor.principal) {
            return Err(PortError::Denied);
        }
        audience_realm_permits(&audience, actor.external_portal)
            .then_some(())
            .ok_or(PortError::Denied)
    }

    fn access(
        actor: &ActorContext,
        thread_id: ThreadId,
        anchor_record_id: RecordId,
        action: CollaborationAction,
    ) -> Result<AuthorizedThreadAccess, PortError> {
        AuthorizedThreadAccess::try_from_authorization(
            actor.holding_id,
            thread_id,
            anchor_record_id,
            actor.authority_scope.clone(),
            actor.principal,
            actor.external_portal,
            action,
        )
    }
}

fn audience_realm_permits(
    audience: &Audience,
    actor_portal: Option<erpx_collaboration::ExternalPortal>,
) -> bool {
    match (audience, actor_portal) {
        (Audience::Internal { .. }, None) => true,
        (Audience::ExternalPortal { portal, .. }, Some(actor_portal)) => *portal == actor_portal,
        _ => false,
    }
}

#[async_trait]
impl AuthorizationPort for SqlxCollaborationAuthorization {
    async fn authorize_new_thread(
        &self,
        actor: &ActorContext,
        thread_id: ThreadId,
        anchor: &AnchorPolicy,
        audience: &Audience,
    ) -> Result<AuthorizedThreadAccess, PortError> {
        self.permits(CollaborationAction::CreateThread)?;
        if anchor.holding_id() != actor.holding_id
            || !actor.authority_scope.contains(anchor.visibility_scope())
            || audience.validate_against(anchor).is_err()
            || !audience_realm_permits(audience, actor.external_portal)
        {
            return Err(PortError::Denied);
        }
        let lookup = self
            .directory
            .resolve(
                &self.directory_access(actor)?,
                anchor.record().id(),
                chrono::Utc::now(),
            )
            .await
            .map_err(|_| PortError::Unavailable)?;
        let DirectoryLookup::Visible {
            value: descriptor,
            freshness: ProjectionFreshness::Fresh,
        } = lookup
        else {
            return Err(PortError::Denied);
        };
        if descriptor.record() != anchor.record()
            || descriptor.authorization_policy() != anchor.authorization_policy()
            || !actor.authority_scope.contains(descriptor.scope())
            || anchor.classification() < descriptor.classification()
        {
            return Err(PortError::Denied);
        }
        Self::access(
            actor,
            thread_id,
            anchor.record().id(),
            CollaborationAction::CreateThread,
        )
    }

    async fn authorize_thread(
        &self,
        actor: &ActorContext,
        thread_id: ThreadId,
        action: CollaborationAction,
    ) -> Result<AuthorizedThreadAccess, PortError> {
        self.permits(action)?;
        let anchor_record_id = self.opaque_anchor(actor, thread_id).await?;
        self.authorize_record(actor, anchor_record_id).await?;
        self.enforce_thread_audience(actor, thread_id, anchor_record_id)
            .await?;
        Self::access(actor, thread_id, anchor_record_id, action)
    }

    async fn authorize_search(
        &self,
        actor: &ActorContext,
    ) -> Result<AuthorizedSearchAccess, PortError> {
        self.permits(CollaborationAction::SearchThreads)?;
        AuthorizedSearchAccess::try_from_authorization(
            actor.holding_id,
            actor.authority_scope.clone(),
            actor.principal,
            actor.external_portal,
        )
    }

    async fn authorize_relation_target(
        &self,
        actor: &ActorContext,
        relation: &ReferenceRelation,
    ) -> Result<(), PortError> {
        self.permits(CollaborationAction::RelateRecord)?;
        self.authorize_record(actor, relation.target.id()).await
    }

    async fn authorize_record_id(
        &self,
        actor: &ActorContext,
        record_id: RecordId,
        action: CollaborationAction,
    ) -> Result<(), PortError> {
        self.permits(action)?;
        self.authorize_record(actor, record_id).await
    }

    async fn authorize_record_ref(
        &self,
        actor: &ActorContext,
        record: &StableRecordRef,
        action: CollaborationAction,
    ) -> Result<(), PortError> {
        self.permits(action)?;
        let lookup = self
            .directory
            .resolve(
                &self.directory_access(actor)?,
                record.id(),
                chrono::Utc::now(),
            )
            .await
            .map_err(|_| PortError::Unavailable)?;
        let descriptor = match lookup {
            DirectoryLookup::Visible {
                value,
                freshness: ProjectionFreshness::Fresh,
            } => value,
            DirectoryLookup::Degraded(ProjectionFreshness::Stale) => {
                return Err(PortError::Stale);
            }
            DirectoryLookup::Degraded(ProjectionFreshness::Unavailable)
            | DirectoryLookup::Unavailable => return Err(PortError::Unavailable),
            DirectoryLookup::Degraded(ProjectionFreshness::Fresh) => {
                return Err(PortError::Unavailable);
            }
            DirectoryLookup::Neutral => return Err(PortError::Denied),
            DirectoryLookup::Visible { .. } => return Err(PortError::Unavailable),
        };
        (descriptor.record() == record)
            .then_some(())
            .ok_or(PortError::Denied)
    }

    async fn authorize_inbox(&self, _: &ActorContext) -> Result<(), PortError> {
        self.permits(CollaborationAction::ReadInbox)
    }
}

async fn set_holding(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
) -> Result<(), PortError> {
    let expected = holding_id.to_string();
    let bound = sqlx::query_scalar::<_, String>("SELECT set_config('app.holding_id', $1, true)")
        .bind(holding_id.to_string())
        .fetch_one(&mut **tx)
        .await
        .map_err(map_database_error)?;
    (bound == expected).then_some(()).ok_or(PortError::Denied)
}

fn map_database_error(_: sqlx::Error) -> PortError {
    PortError::Unavailable
}

#[cfg(test)]
mod tests {
    use erpx_collaboration::{Audience, ExternalPortal, PrincipalRef};
    use erpx_kernel::{AuthorityScope, PrincipalId, PrincipalKind};
    use uuid::Uuid;

    #[test]
    fn source_orders_opaque_anchor_record_grant_then_audience() {
        let source = include_str!("authorization.rs");
        let anchor = source
            .find("thread_anchor_authority_index")
            .expect("opaque anchor lookup");
        let grant = source
            .find("self.authorize_record(actor, anchor_record_id)")
            .expect("record grant");
        let audience = source
            .find("self.enforce_thread_audience(actor, thread_id, anchor_record_id)")
            .expect("audience check");
        assert!(anchor < grant && grant < audience);
    }

    #[test]
    fn external_portal_audience_is_exact_and_internal_notes_are_denied() {
        let principal = PrincipalRef::new(
            PrincipalId::try_from_uuid(Uuid::new_v4()).unwrap(),
            PrincipalKind::Human,
        );
        let external = Audience::ExternalPortal {
            portal: ExternalPortal::Customer,
            participants: vec![principal],
        };
        assert!(super::audience_realm_permits(
            &external,
            Some(ExternalPortal::Customer)
        ));
        assert!(!super::audience_realm_permits(
            &external,
            Some(ExternalPortal::Commerce)
        ));
        let internal = Audience::Internal {
            scope: AuthorityScope::Company {
                holding_id: Uuid::new_v4(),
                company_id: Uuid::new_v4(),
            },
            principals: vec![principal],
        };
        assert!(!super::audience_realm_permits(
            &internal,
            Some(ExternalPortal::Customer)
        ));
    }
}
