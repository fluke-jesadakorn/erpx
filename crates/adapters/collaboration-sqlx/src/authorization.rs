//! SQLx record-policy authorization for Collaboration.
//!
//! Thread lookup reads only the opaque anchor index. The Item Directory grant
//! is then checked before audience, snapshot, comment, relation, or stream data
//! can be inspected.

use async_trait::async_trait;
use erpx_collaboration::{
    ActorContext, AnchorPolicy, AttachmentAuthorizationPort, Audience, AuthorizationPort,
    AuthorizedAttachmentAccess, AuthorizedRecordThreadPolicy, AuthorizedRelationRecordAccess,
    AuthorizedSearchAccess, AuthorizedThreadAccess, CollaborationAction, PortError,
    RecordRelationAuthorizationPort, RecordThreadPolicyPort, ReferenceRelation, RetentionPolicy,
    ThreadId,
};
use erpx_integration_contracts::{StableRecordRef, record_collaboration_policy};
use erpx_kernel::{ActorKind, AuthorityScope, HoldingId, PrincipalKind, RecordId};
use sqlx::{PgPool, Postgres, Transaction};

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

    async fn scope_contains(
        &self,
        actor: &ActorContext,
        target: &AuthorityScope,
    ) -> Result<bool, PortError> {
        let mut tx = self
            .directory
            .pool()
            .begin()
            .await
            .map_err(map_database_error)?;
        set_holding(&mut tx, actor.holding_id).await?;
        let contains =
            sqlx::query_scalar::<_, bool>("SELECT erpx_collaboration_read.scope_contains($1, $2)")
                .bind(serde_json::to_value(&actor.authority_scope).map_err(|_| PortError::Denied)?)
                .bind(serde_json::to_value(target).map_err(|_| PortError::Denied)?)
                .fetch_one(&mut *tx)
                .await
                .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        Ok(contains)
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
    ) -> Result<AuthorityScope, PortError> {
        let mut tx = self.write_pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, actor.holding_id).await?;
        let visibility = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT policy.policy_document->'visibility_scope' \
             FROM erpx_collaboration.threads thread \
             JOIN erpx_collaboration.thread_policies policy \
               ON policy.holding_id=thread.holding_id AND policy.thread_id=thread.thread_id \
             WHERE thread.holding_id=$1 AND thread.thread_id=$2 AND thread.anchor_record_id=$3",
        )
        .bind(actor.holding_id.as_uuid())
        .bind(thread_id.as_uuid())
        .bind(anchor_record_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        let Some(visibility) = visibility else {
            return Err(PortError::Denied);
        };
        let visibility = serde_json::from_value::<AuthorityScope>(visibility)
            .map_err(|_| PortError::Unavailable)?;
        if !self.scope_contains(actor, &visibility).await? {
            return Err(PortError::Denied);
        }

        // The current Organization lineage is proven before the audience
        // document is selected. The exact visibility predicate also prevents
        // a policy change between the two databases from widening access.
        let mut tx = self.write_pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, actor.holding_id).await?;
        let audience = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT audience.audience_document \
             FROM erpx_collaboration.threads thread \
             JOIN erpx_collaboration.thread_policies policy \
               ON policy.holding_id=thread.holding_id AND policy.thread_id=thread.thread_id \
             JOIN erpx_collaboration.thread_audiences audience \
               ON audience.holding_id=thread.holding_id AND audience.thread_id=thread.thread_id \
             WHERE thread.holding_id=$1 AND thread.thread_id=$2 AND thread.anchor_record_id=$3 \
               AND policy.policy_document->'visibility_scope'=$4::JSONB",
        )
        .bind(actor.holding_id.as_uuid())
        .bind(thread_id.as_uuid())
        .bind(anchor_record_id.as_uuid())
        .bind(serde_json::to_value(&visibility).map_err(|_| PortError::Unavailable)?)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        let Some(audience) = audience else {
            return Err(PortError::Denied);
        };
        let audience =
            serde_json::from_value::<Audience>(audience).map_err(|_| PortError::Unavailable)?;
        if !audience.includes(actor.principal) {
            return Err(PortError::Denied);
        }
        audience_realm_permits(&audience, actor.external_portal)
            .then_some(visibility)
            .ok_or(PortError::Denied)
    }

    fn access(
        actor: &ActorContext,
        thread_id: ThreadId,
        anchor_record_id: RecordId,
        visibility_scope: AuthorityScope,
        action: CollaborationAction,
    ) -> Result<AuthorizedThreadAccess, PortError> {
        AuthorizedThreadAccess::try_from_authorization(
            actor.holding_id,
            thread_id,
            anchor_record_id,
            actor.authority_scope.clone(),
            visibility_scope,
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
            || !self
                .scope_contains(actor, anchor.visibility_scope())
                .await?
            || !self.scope_contains(actor, descriptor.scope()).await?
            || anchor.classification() < descriptor.classification()
        {
            return Err(PortError::Denied);
        }
        Self::access(
            actor,
            thread_id,
            anchor.record().id(),
            anchor.visibility_scope().clone(),
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
        let visibility_scope = self
            .enforce_thread_audience(actor, thread_id, anchor_record_id)
            .await?;
        Self::access(actor, thread_id, anchor_record_id, visibility_scope, action)
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

#[async_trait]
impl RecordRelationAuthorizationPort for SqlxCollaborationAuthorization {
    async fn authorize_relation_record(
        &self,
        actor: &ActorContext,
        record: &StableRecordRef,
        action: CollaborationAction,
    ) -> Result<AuthorizedRelationRecordAccess, PortError> {
        if !matches!(
            action,
            CollaborationAction::RelateRecord
                | CollaborationAction::RevokeRelation
                | CollaborationAction::ListRecordRelations
        ) {
            return Err(PortError::Denied);
        }
        self.authorize_record_ref(actor, record, action).await?;
        AuthorizedRelationRecordAccess::try_from_authorization(
            actor.holding_id,
            record.clone(),
            actor.principal,
            actor.attribution.clone(),
            actor.authority_scope.clone(),
            actor.external_portal,
            action,
        )
    }
}

#[async_trait]
impl AttachmentAuthorizationPort for SqlxCollaborationAuthorization {
    async fn authorize_attachment(
        &self,
        actor: &ActorContext,
        anchor: &StableRecordRef,
        thread_id: Option<ThreadId>,
        action: CollaborationAction,
    ) -> Result<AuthorizedAttachmentAccess, PortError> {
        if !matches!(
            action,
            CollaborationAction::PrepareAttachmentUpload
                | CollaborationAction::FinalizeAttachmentUpload
                | CollaborationAction::DownloadAttachment
        ) {
            return Err(PortError::Denied);
        }
        self.permits(action)?;
        if actor.principal.kind() != PrincipalKind::Human
            || !matches!(
                actor.attribution.kind,
                ActorKind::HoldingUser | ActorKind::PlatformUser
            )
            || actor.attribution.session_id.is_none()
            || actor.attribution.represented_principal_ref.is_some()
            || actor.attribution.support_capability_id.is_some()
            || actor.external_portal.is_some()
        {
            return Err(PortError::Denied);
        }
        let lookup = self
            .directory
            .resolve(
                &self.directory_access(actor)?,
                anchor.id(),
                chrono::Utc::now(),
            )
            .await
            .map_err(|_| PortError::Unavailable)?;
        let descriptor = match lookup {
            DirectoryLookup::Visible {
                value,
                freshness: ProjectionFreshness::Fresh,
            } if value.record() == anchor => value,
            DirectoryLookup::Degraded(ProjectionFreshness::Stale) => return Err(PortError::Stale),
            DirectoryLookup::Neutral => return Err(PortError::Denied),
            _ => return Err(PortError::Unavailable),
        };
        record_collaboration_policy(anchor.record_type().as_str())
            .filter(|policy| {
                policy.owner == anchor.owner_context()
                    && policy.attachments_enabled
                    && policy.human_internal_mutations_only
                    && policy.classification == descriptor.classification()
            })
            .ok_or(PortError::Denied)?;

        // Prepare may reserve a not-yet-created deterministic thread ID. Every
        // download, and any finalize against an existing thread, proves that
        // thread still belongs to this exact full anchor and audience.
        if action == CollaborationAction::DownloadAttachment {
            let thread_id = thread_id.ok_or(PortError::Denied)?;
            let record_id = self.opaque_anchor(actor, thread_id).await?;
            if record_id != anchor.id() {
                return Err(PortError::Denied);
            }
            self.enforce_thread_audience(actor, thread_id, record_id)
                .await?;
        } else if let Some(thread_id) = thread_id {
            match self.opaque_anchor(actor, thread_id).await {
                Ok(record_id) if record_id == anchor.id() => {
                    self.enforce_thread_audience(actor, thread_id, record_id)
                        .await?;
                }
                Ok(_) => return Err(PortError::Denied),
                Err(PortError::Denied)
                    if action == CollaborationAction::PrepareAttachmentUpload => {}
                Err(error) => return Err(error),
            }
        }

        AuthorizedAttachmentAccess::try_from_authorization(
            actor.holding_id,
            anchor.clone(),
            thread_id,
            actor.principal,
            actor.attribution.clone(),
            actor.authority_scope.clone(),
            descriptor.classification(),
            action,
        )
    }
}

#[async_trait]
impl RecordThreadPolicyPort for SqlxCollaborationAuthorization {
    async fn authorize_new_record_thread(
        &self,
        actor: &ActorContext,
        thread_id: ThreadId,
        record: &StableRecordRef,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<AuthorizedRecordThreadPolicy, PortError> {
        self.permits(CollaborationAction::CreateThread)?;
        let lookup = self
            .directory
            .resolve(&self.directory_access(actor)?, record.id(), at)
            .await
            .map_err(|_| PortError::Unavailable)?;
        let descriptor = match lookup {
            DirectoryLookup::Visible {
                value,
                freshness: ProjectionFreshness::Fresh,
            } if value.record() == record => value,
            DirectoryLookup::Degraded(ProjectionFreshness::Stale) => return Err(PortError::Stale),
            DirectoryLookup::Neutral => return Err(PortError::Denied),
            _ => return Err(PortError::Unavailable),
        };
        let owner = record_collaboration_policy(record.record_type().as_str())
            .filter(|policy| {
                policy.owner == record.owner_context()
                    && policy.discussion_threads_enabled
                    && policy.human_internal_mutations_only
                    && policy.classification == descriptor.classification()
            })
            .ok_or(PortError::Denied)?;
        let retain_until = at
            .checked_add_signed(chrono::Duration::days(i64::from(
                owner.retention.minimum_days,
            )))
            .ok_or(PortError::InvalidState)?;
        let policy = AnchorPolicy::try_new(
            actor.holding_id,
            record.clone(),
            descriptor.scope().clone(),
            None,
            descriptor.classification(),
            descriptor.authorization_policy(),
        )
        .map_err(|_| PortError::Denied)?;
        let audience = Audience::Internal {
            scope: descriptor.scope().clone(),
            principals: Vec::new(),
        };
        let retention = RetentionPolicy {
            parent_retain_until: retain_until,
            comment_retain_until: retain_until,
            jurisdiction_retain_until: retain_until,
            legal_holds: std::collections::BTreeSet::new(),
        };
        let access = self
            .authorize_new_thread(actor, thread_id, &policy, &audience)
            .await?;
        AuthorizedRecordThreadPolicy::try_from_owner_policy(access, policy, audience, retention, at)
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
    fn hierarchy_is_database_proven_and_exact_visibility_is_carried_forward() {
        let source = include_str!("authorization.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("SELECT erpx_collaboration_read.scope_contains($1, $2)"));
        assert!(!production.contains("actor.authority_scope.contains("));
        assert!(production.contains("anchor.visibility_scope().clone()"));
        assert!(production.contains("then_some(visibility)"));
        let audience_enforcement = production
            .split("async fn enforce_thread_audience")
            .nth(1)
            .expect("thread audience authorization")
            .split("fn access")
            .next()
            .unwrap();
        let hierarchy = audience_enforcement
            .find("self.scope_contains(actor, &visibility)")
            .expect("lineage proof");
        let audience = audience_enforcement
            .find("SELECT audience.audience_document")
            .expect("audience payload query");
        assert!(hierarchy < audience);

        let write = include_str!("write.rs");
        assert!(!write.contains("authority_scope()\n                .contains("));
        assert!(write.contains("thread.anchor_record_id=$3"));
        assert!(write.contains("policy.policy_document->'visibility_scope'=$4::JSONB"));
        assert!(
            write.matches("access.visibility_scope() !=").count() >= 2,
            "load and commit must both bind the authorization witness to exact persisted policy"
        );
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
