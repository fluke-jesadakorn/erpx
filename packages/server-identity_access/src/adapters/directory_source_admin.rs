//! Explicit source mutation. No startup approval, implicit key selection,
//! actor-DTO authority, database commit in a borrowed port or permissive policy.
use super::{
    directory_postgres::{
        DirectoryRoute, PostgresDirectory, Tx, connection_without_source, finish, limits,
        matches_descriptor,
    },
    directory_profiles::DirectoryProfileLease,
    directory_source_postgres as sources,
};
use crate::{
    application::{account_ports::AccountFuture, directory_source_admin::*},
    domain::{directory::DirectoryConnectionDescriptor, directory_provenance::*},
};
use erpx_server_runtime::public::postgres::{
    PgRequestLease, PgUnitOfWork, PgWorkFuture, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use sqlx::{PgConnection, Row};
use std::sync::Arc;

/// Sealed actual candidate; copied source data is never administrative approval.
pub struct PreparedDirectorySourceChange {
    descriptor: DirectoryConnectionDescriptor,
    previous: Option<SelectedSource>,
    next: SelectedSource,
    profile: DirectoryProfileLease,
    kind: DirectorySourceChangeKind,
    audit_id: String,
}
impl PreparedDirectorySourceChange {
    pub fn connection_id(&self) -> &str {
        &self.descriptor.connection_id
    }
    pub fn company_id(&self) -> &str {
        &self.descriptor.company_id
    }
    pub fn holding_id(&self) -> Option<&str> {
        self.descriptor.holding_id.as_deref()
    }
    pub fn kind(&self) -> DirectorySourceChangeKind {
        self.kind
    }
    pub fn expected_revision(&self) -> Option<SourceRevision> {
        self.previous.as_ref().map(|p| p.revision)
    }
    pub fn selected_revision(&self) -> SourceRevision {
        self.next.revision
    }
    /// Exact proposed key/tag/configuration data for a retained native review.
    /// Copying this binding cannot approve or perform a source change.
    pub fn proposed_source(&self) -> &SelectedSource {
        &self.next
    }
}
/// Native composition must supply session-first Control sources, current
/// reviewed admin/fresh-MFA policy, or an explicit guarded initial-seed policy.
/// No default implementation, boolean approval or body-supplied Actor exists.
pub trait PgDirectorySourceChangePolicy: Send + Sync {
    fn prepare_control<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError>;
    fn retain_control<'c>(
        &'c self,
        c: &'c mut PgConnection,
        change: &'c PreparedDirectorySourceChange,
        now: i64,
    ) -> PgWorkFuture<'c, Arc<dyn PgDirectorySourceControlPolicy>, BusinessError>;
}
pub trait PgDirectorySourceControlPolicy: Send + Sync {
    /// Utility locks only, before the first Holding data snapshot.
    fn prepare_holding<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError>;
    fn authorize_holding<'c>(
        &'c self,
        c: &'c mut PgConnection,
        change: &'c PreparedDirectorySourceChange,
        now: i64,
    ) -> PgWorkFuture<'c, Box<dyn PgDirectorySourceHoldingPolicy>, BusinessError>;
    /// Retained exact Control proof deadline; every policy must declare it.
    fn deadline(&self) -> i64;
    fn require_current<'c>(
        &'c self,
        c: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError>;
}
pub trait PgDirectorySourceHoldingPolicy: Send + Sync {
    fn audit_actor(&self) -> &str;
    fn audit_assignment(&self) -> &str;
    fn audit_reason(&self) -> &str;
    fn require_current<'c>(
        &'c self,
        c: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError>;
}
impl PostgresDirectory {
    /// Prepare actual current Connection data, then release every database lease
    /// before waiting for the private profile. This performs no source mutation.
    pub async fn prepare_source_change(
        &self,
        request: DirectorySourceChangeRequest,
    ) -> Result<Arc<PreparedDirectorySourceChange>, BusinessError> {
        let this = self.clone();
        let id = request.connection_id.as_str().to_owned();
        let (descriptor, previous) = finish(
            PgRequestLease::new(self.control.clone())
                .run(move |control| {
                    Box::pin(async move {
                        Self::lock_routes(control).await?;
                        let route = this.route(control, &id).await?;
                        if route.holding_id.is_none() {
                            return Err(Denial::SourceNotApproved.business().into());
                        }
                        let mut c = route
                            .pool
                            .raw()
                            .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
                            .await?;
                        limits(&mut c).await?;
                        super::directory_postgres::lock_identity_sources(&mut c).await?;
                        let d = connection_without_source(&mut c, &id, &route).await?;
                        let previous =
                            sources::existing_selection(&mut c, &id, &route.company_id).await?;
                        c.rollback().await?;
                        Ok((d, previous))
                    })
                })
                .await,
        )?;
        if descriptor.version != request.expected_connection_version
            || previous.as_ref().map(|p| p.revision) != request.expected_source_revision
        {
            return Err(Denial::RevisionConflict.business());
        }
        let profile = self
            .profiles
            .retain(
                &descriptor.config.secret_profile,
                &descriptor.connection_id,
                None,
            )
            .await?;
        let local = CurrentLocalSource {
            key_id: request.key_id.clone(),
            profile_tag: self
                .profiles
                .selected_tag(&descriptor, &request.key_id, &profile)?,
            connection_version: descriptor.version,
            config_fingerprint: sources::config_bytes(&descriptor)?,
        };
        let next = if let Some(previous) = &previous {
            rotate(
                previous,
                request
                    .expected_source_revision
                    .ok_or_else(|| Denial::RevisionConflict.business())?,
                local,
            )
            .map_err(Denial::business)?
        } else {
            SelectedSource {
                revision: SourceRevision::checked(1).map_err(Denial::business)?,
                key_id: local.key_id,
                profile_tag: local.profile_tag,
                connection_version: local.connection_version,
                config_fingerprint: local.config_fingerprint,
            }
        };
        Ok(Arc::new(PreparedDirectorySourceChange {
            descriptor,
            previous,
            next,
            profile,
            kind: request.kind(),
            audit_id: self.secrets.identifier()?,
        }))
    }
    /// Required policy surrounds the real route and inner Holding unit. Control
    /// is never automatically retried around an independently committed change.
    pub fn change_source<'a>(
        &'a self,
        change: Arc<PreparedDirectorySourceChange>,
        policy: Arc<dyn PgDirectorySourceChangePolicy>,
    ) -> AccountFuture<'a, SourceRevision> {
        Box::pin(async move {
            let this = self.clone();
            finish(
                PgRequestLease::new(self.control.clone())
                    .run(move |control| {
                        Box::pin(async move {
                            policy.prepare_control(control).await?;
                            Self::lock_routes(control).await?;
                            let proof = policy
                                .retain_control(control, &change, this.clock.now().get())
                                .await?;
                            if proof.deadline() <= this.clock.now().get() {
                                return Err(Denial::RevisionConflict.business().into());
                            }
                            let route = Arc::new(
                                this.route(control, &change.descriptor.connection_id)
                                    .await?,
                            );
                            route.matches(&change.descriptor)?;
                            if route.holding_id.is_none() {
                                return Err(Denial::SourceNotApproved.business().into());
                            }
                            let revision = PgUnitOfWork::new(route.pool.clone())
                                .run(TransactionOptions::default(), |c| {
                                    let this = this.clone();
                                    let route = route.clone();
                                    let change = change.clone();
                                    let proof = proof.clone();
                                    Box::pin(async move {
                                        this.change_source_unit(c, &route, &change, proof.as_ref())
                                            .await
                                    })
                                })
                                .await?;
                            proof
                                .require_current(control, this.clock.now().get())
                                .await?;
                            route.require_current(control).await?;
                            Ok(revision)
                        })
                    })
                    .await,
            )
        })
    }
    async fn change_source_unit(
        &self,
        c: &mut PgConnection,
        route: &DirectoryRoute,
        change: &PreparedDirectorySourceChange,
        proof: &dyn PgDirectorySourceControlPolicy,
    ) -> Tx<SourceRevision> {
        limits(c).await?;
        proof.prepare_holding(c).await?;
        // No iam_connections write is performed here, so no late upgrade of its
        // policy-source SHARE lock is required. Configuration edits need their
        // own writer utility fences before this source-only adapter is entered.
        super::directory_postgres::lock_identity_sources(c).await?;
        let actual = connection_without_source(c, &change.descriptor.connection_id, route).await?;
        matches_descriptor(&actual, &change.descriptor)?;
        if sources::existing_selection(c, &actual.connection_id, &actual.company_id).await?
            != change.previous
        {
            return Err(Denial::RevisionConflict.business().into());
        }
        self.profiles
            .selected_tag(&actual, &change.next.key_id, &change.profile)
            .and_then(|tag| {
                if tag == change.next.profile_tag {
                    Ok(())
                } else {
                    Err(Denial::LocalSourceMismatch.business())
                }
            })?;
        let retained = proof
            .authorize_holding(c, change, self.clock.now().get())
            .await?;
        self.write_source_change(c, &actual, change, retained.as_ref(), proof.deadline())
            .await
    }
    /// Borrowed production administration seam. The sealed review is retained
    /// from the existing normal IAM coordinator; it cannot be deserialized from
    /// stored review data. No transaction, Control checkout or retry starts here.
    pub async fn apply_reviewed_source(
        &self,
        c: &mut PgConnection,
        change: &PreparedDirectorySourceChange,
        approval: &super::directory_source_review::CurrentDirectorySourceApproval<'_>,
    ) -> Tx<SourceRevision> {
        approval.require_candidate(change)?;
        let actual = self.require_source_candidate(c, change).await?;
        self.write_source_change(c, &actual, change, approval, approval.deadline())
            .await
    }
    pub async fn require_source_candidate(
        &self,
        c: &mut PgConnection,
        change: &PreparedDirectorySourceChange,
    ) -> Tx<DirectoryConnectionDescriptor> {
        let holding = erpx_server_shared::public::HoldingId::new(
            change
                .holding_id()
                .ok_or_else(|| Denial::SourceNotApproved.business())?,
        )?;
        let configured = self
            .holdings
            .get(&holding)
            .ok_or_else(|| Denial::SourceNotApproved.business())?;
        let route = DirectoryRoute {
            company_id: change.company_id().into(),
            holding_id: Some(holding),
            binding: None,
            pool: configured.pool.clone(),
        };
        let actual = connection_without_source(c, change.connection_id(), &route).await?;
        matches_descriptor(&actual, &change.descriptor)?;
        if sources::existing_selection(c, &actual.connection_id, &actual.company_id).await?
            != change.previous
        {
            return Err(Denial::RevisionConflict.business().into());
        }
        if self
            .profiles
            .selected_tag(&actual, &change.next.key_id, &change.profile)?
            != change.next.profile_tag
        {
            return Err(Denial::LocalSourceMismatch.business().into());
        }
        Ok(actual)
    }
    async fn write_source_change(
        &self,
        c: &mut PgConnection,
        actual: &DirectoryConnectionDescriptor,
        change: &PreparedDirectorySourceChange,
        retained: &dyn PgDirectorySourceHoldingPolicy,
        deadline: i64,
    ) -> Tx<SourceRevision> {
        let now = self.clock.now().get();
        let next = &change.next;
        let count = if let Some(previous) = &change.previous {
            sqlx::query("UPDATE iam_directory_source_selections SET revision=$1,format_version=1,key_id=$2,profile_tag=$3,connection_version=$4,config_fingerprint=$5,selected_at=$6 WHERE connection_id=$7 AND company_id=$8 AND revision=$9")
                .bind(next.revision.get()).bind(&next.key_id).bind(next.profile_tag.database_bytes().as_slice()).bind(i32::try_from(next.connection_version).map_err(|_|Denial::InvalidRevision.business())?).bind(next.config_fingerprint.as_slice()).bind(now).bind(&actual.connection_id).bind(&actual.company_id).bind(previous.revision.get()).execute(&mut*c).await?.rows_affected()
        } else {
            sqlx::query("INSERT INTO iam_directory_source_selections(connection_id,company_id,revision,format_version,key_id,profile_tag,connection_version,config_fingerprint,selected_at) VALUES($1,$2,1,1,$3,$4,$5,$6,$7) ON CONFLICT(connection_id) DO NOTHING")
                .bind(&actual.connection_id).bind(&actual.company_id).bind(&next.key_id).bind(next.profile_tag.database_bytes().as_slice()).bind(i32::try_from(next.connection_version).map_err(|_|Denial::InvalidRevision.business())?).bind(next.config_fingerprint.as_slice()).bind(now).execute(&mut*c).await?.rows_affected()
        };
        if count != 1 {
            return Err(Denial::RevisionConflict.business().into());
        }
        let actor = retained.audit_actor();
        let assignment = retained.audit_assignment();
        let reason = retained.audit_reason();
        if actor.is_empty()
            || actor.len() > 200
            || assignment.is_empty()
            || assignment.len() > 200
            || reason.trim().is_empty()
            || reason.len() > 4096
        {
            return Err(Denial::InconsistentObservation.business().into());
        }
        let evidence = serde_json::json!({"sourceRevision":next.revision.get(),"connectionVersion":next.connection_version,"keyId":next.key_id,"formatVersion":1,"reason":reason});
        sqlx::query("INSERT INTO iam_audit(id,company_id,actor_id,assignment_id,action,target_id,evidence,created_at) VALUES($1,$2,$3,$4,'directory.source-selected',$5,$6,$7)")
            .bind(&change.audit_id).bind(&actual.company_id).bind(actor).bind(assignment).bind(&actual.connection_id).bind(&evidence).bind(now).execute(&mut*c).await?;
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *c)
            .await?;
        retained.require_current(c, self.clock.now().get()).await?;
        let final_now = self.clock.now().get();
        if final_now >= deadline {
            return Err(Denial::RevisionConflict.business().into());
        }
        if sources::current_selection(c, &actual.connection_id, &actual.company_id).await?
            != change.next
        {
            return Err(Denial::InconsistentObservation.business().into());
        }
        let selected_at:i64=sqlx::query_scalar("SELECT selected_at FROM iam_directory_source_selections WHERE connection_id=$1 AND company_id=$2 FOR SHARE").bind(&actual.connection_id).bind(&actual.company_id).fetch_one(&mut*c).await?;
        let audit=sqlx::query("SELECT CASE WHEN octet_length(actor_id)<=200 THEN actor_id END actor_id,CASE WHEN octet_length(assignment_id)<=200 THEN assignment_id END assignment_id,CASE WHEN octet_length(action)<=80 THEN action END action,CASE WHEN octet_length(target_id)<=200 THEN target_id END target_id,CASE WHEN octet_length(evidence::text)<=8192 THEN evidence END evidence,created_at FROM iam_audit WHERE id=$1 AND company_id=$2 FOR SHARE").bind(&change.audit_id).bind(&actual.company_id).fetch_optional(&mut*c).await?.ok_or_else(||Denial::InconsistentObservation.business())?;
        if selected_at != now
            || audit.try_get::<Option<String>, _>("actor_id")?.as_deref() != Some(actor)
            || audit
                .try_get::<Option<String>, _>("assignment_id")?
                .as_deref()
                != Some(assignment)
            || audit.try_get::<Option<String>, _>("action")?.as_deref()
                != Some("directory.source-selected")
            || audit.try_get::<Option<String>, _>("target_id")?.as_deref()
                != Some(actual.connection_id.as_str())
            || audit
                .try_get::<Option<serde_json::Value>, _>("evidence")?
                .as_ref()
                != Some(&evidence)
            || audit.try_get::<i64, _>("created_at")? != now
        {
            return Err(Denial::InconsistentObservation.business().into());
        }
        Ok(next.revision)
    }
}
