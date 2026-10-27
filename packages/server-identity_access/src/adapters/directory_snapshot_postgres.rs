//! A complete snapshot, its standing mapping effects and immutable audit are one unit.
use super::{directory_mapping_postgres as mappings, directory_postgres::*};
use crate::{
    application::{
        account_ports::AccountFuture,
        directory::{DirectorySynchronization, SynchronizationOutcome},
    },
    domain::directory::*,
};
use erpx_server_runtime::public::postgres::{PgRequestLease, PgUnitOfWork, TransactionOptions};
use sqlx::PgConnection;
use std::sync::Arc;
struct SnapshotWork {
    descriptor: DirectoryConnectionDescriptor,
    snapshot: CompleteDirectorySnapshot,
    identifiers: Vec<String>,
    audit_id: String,
}
impl PostgresDirectory {
    pub(super) async fn apply_with_retained_generation(
        &self,
        control: &mut PgConnection,
        descriptor: &DirectoryConnectionDescriptor,
        snapshot: CompleteDirectorySnapshot,
        retention: &dyn crate::application::directory_refresh::DirectoryObservationRetention,
    ) -> Tx<SynchronizationOutcome> {
        if retention.fingerprint() != descriptor.profile_fingerprint {
            return Err(unavailable().into());
        }
        snapshot.require_fresh(self.clock.now().get())?;
        Self::lock_routes(control).await?;
        let route = Arc::new(self.route(control, &descriptor.connection_id).await?);
        route.matches(descriptor)?;
        let work = Arc::new(SnapshotWork {
            descriptor: descriptor.clone(),
            snapshot,
            identifiers: (0..10000)
                .map(|_| self.secrets.identifier())
                .collect::<Result<Vec<_>, _>>()?,
            audit_id: self.secrets.identifier()?,
        });
        let outcome = PgUnitOfWork::new(route.pool.clone())
            .run(TransactionOptions::default(), |source| {
                let this = self.clone();
                let route = route.clone();
                let work = work.clone();
                Box::pin(async move { this.apply_snapshot_unit(source, &route, &work).await })
            })
            .await?;
        route.require_current(control).await?;
        Ok(outcome)
    }
    async fn apply_snapshot_unit(
        &self,
        source: &mut PgConnection,
        route: &DirectoryRoute,
        work: &SnapshotWork,
    ) -> Tx<SynchronizationOutcome> {
        let descriptor = &work.descriptor;
        let snapshot = &work.snapshot;
        limits(source).await?;
        mappings::lock_sources(source, descriptor.company_id == "platform").await?;
        matches_descriptor(
            &connection(source, &descriptor.connection_id, route).await?,
            descriptor,
        )?;
        let last: Option<i64> =
            sqlx::query_scalar("SELECT last_sync_at FROM iam_connections WHERE id=$1 FOR UPDATE")
                .bind(&descriptor.connection_id)
                .fetch_one(&mut *source)
                .await?;
        let observed = snapshot.observed_at();
        if last.is_some_and(|time| time > observed) {
            return Ok(SynchronizationOutcome::NewerCompleteObservationRetained);
        }
        snapshot.require_fresh(self.clock.now().get())?;
        let hierarchy = serde_json::to_value(snapshot.hierarchy()).map_err(|_| unavailable())?;
        sqlx::query(STORE_HIERARCHY)
            .bind(&descriptor.connection_id)
            .bind(hierarchy)
            .bind(observed)
            .execute(&mut *source)
            .await?;
        let groups = serde_json::to_value(snapshot.groups()).map_err(|_| unavailable())?;
        sqlx::query(OBSERVE_GROUPS)
            .bind(&descriptor.connection_id)
            .bind(observed)
            .bind(groups)
            .execute(&mut *source)
            .await?;
        let mut users = serde_json::to_value(snapshot.users()).map_err(|_| unavailable())?;
        for (user, id) in users
            .as_array_mut()
            .ok_or_else(unavailable)?
            .iter_mut()
            .zip(&work.identifiers)
        {
            user.as_object_mut()
                .ok_or_else(unavailable)?
                .insert("id".into(), serde_json::Value::String(id.clone()));
        }
        let count = sqlx::query(OBSERVE_IDENTITIES)
            .bind(&descriptor.company_id)
            .bind(&descriptor.connection_id)
            .bind(observed)
            .bind(users)
            .execute(&mut *source)
            .await?
            .rows_affected();
        if count != snapshot.users().len() as u64 {
            return Err(unavailable().into());
        }
        let user_ids: Vec<&str> = snapshot
            .users()
            .iter()
            .map(|u| u.external_id.as_str())
            .collect();
        let group_ids: Vec<&str> = snapshot
            .groups()
            .iter()
            .map(|g| g.external_id.as_str())
            .collect();
        sqlx::query(DISABLE_MISSING_IDENTITIES)
            .bind(observed)
            .bind(&descriptor.connection_id)
            .bind(&descriptor.company_id)
            .bind(&user_ids)
            .execute(&mut *source)
            .await?;
        sqlx::query("DELETE FROM iam_groups WHERE connection_id=$1 AND NOT(external_id=ANY($2))")
            .bind(&descriptor.connection_id)
            .bind(&group_ids)
            .execute(&mut *source)
            .await?;
        let now = self.clock.now().get();
        let retained =
            mappings::reconcile(source, descriptor, route, now, &work.identifiers).await?;
        sqlx::query("UPDATE iam_connections SET last_sync_at=$1,health='healthy',last_error=NULL WHERE id=$2").bind(observed).bind(&descriptor.connection_id).execute(&mut *source).await?;
        let evidence = serde_json::json!({"users":snapshot.users().len(),"groups":snapshot.groups().len(),"version":descriptor.version});
        sqlx::query(RECORD_SYNC_AUDIT)
            .bind(&work.audit_id)
            .bind(&descriptor.company_id)
            .bind(&descriptor.connection_id)
            .bind(evidence)
            .bind(observed)
            .execute(&mut *source)
            .await?;
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *source)
            .await?;
        let final_now = self.clock.now().get();
        snapshot.require_fresh(final_now)?;
        matches_descriptor(
            &connection(source, &descriptor.connection_id, route).await?,
            descriptor,
        )?;
        retained
            .require_current(source, descriptor, final_now)
            .await?;
        Ok(SynchronizationOutcome::Applied)
    }
    async fn failed_snapshot_unit(
        &self,
        source: &mut PgConnection,
        route: &DirectoryRoute,
        descriptor: &DirectoryConnectionDescriptor,
        started: i64,
        code: &str,
    ) -> Tx<()> {
        limits(source).await?;
        mappings::lock_sources(source, descriptor.company_id == "platform").await?;
        matches_descriptor(
            &connection(source, &descriptor.connection_id, route).await?,
            descriptor,
        )?;
        let changed = sqlx::query(RECORD_FAILED_SYNC)
            .bind(code)
            .bind(&descriptor.connection_id)
            .bind(descriptor.version)
            .bind(started)
            .execute(&mut *source)
            .await?
            .rows_affected();
        if changed == 1 {
            let cutoff = self
                .clock
                .now()
                .get()
                .checked_sub(DIRECTORY_FRESH_MS)
                .ok_or_else(unavailable)?;
            sqlx::query(INVALIDATE_FAILED_FRESHNESS)
                .bind(cutoff)
                .bind(&descriptor.connection_id)
                .bind(&descriptor.company_id)
                .execute(&mut *source)
                .await?;
        }
        Ok(())
    }
}
impl DirectorySynchronization for PostgresDirectory {
    fn apply<'a>(
        &'a self,
        descriptor: &'a DirectoryConnectionDescriptor,
        snapshot: CompleteDirectorySnapshot,
    ) -> AccountFuture<'a, SynchronizationOutcome> {
        Box::pin(async move {
            snapshot.require_fresh(self.clock.now().get())?;
            // Stable IDs are generated before the retryable Holding unit.
            let identifiers = (0..10000)
                .map(|_| self.secrets.identifier())
                .collect::<Result<Vec<_>, _>>()?;
            let work = Arc::new(SnapshotWork {
                descriptor: descriptor.clone(),
                snapshot,
                identifiers,
                audit_id: self.secrets.identifier()?,
            });
            let this = self.clone();
            let result = PgRequestLease::new(self.control.clone())
                .run(move |control| {
                    Box::pin(async move {
                        Self::lock_routes(control).await?;
                        let route =
                            Arc::new(this.route(control, &work.descriptor.connection_id).await?);
                        route.matches(&work.descriptor)?;
                        let profile = this
                            .profiles
                            .retain(
                                &work.descriptor.config.secret_profile,
                                &work.descriptor.connection_id,
                                Some(&work.descriptor.profile_fingerprint),
                            )
                            .await?;
                        let unit = PgUnitOfWork::new(route.pool.clone());
                        let outcome = unit
                            .run(TransactionOptions::default(), |source| {
                                let this = this.clone();
                                let route = route.clone();
                                let work = work.clone();
                                Box::pin(async move {
                                    this.apply_snapshot_unit(source, &route, &work).await
                                })
                            })
                            .await?;
                        route.require_current(control).await?;
                        if profile.fingerprint() != work.descriptor.profile_fingerprint {
                            return Err(unavailable().into());
                        }
                        Ok(outcome)
                    })
                })
                .await;
            finish(result)
        })
    }
    fn failed<'a>(
        &'a self,
        descriptor: &'a DirectoryConnectionDescriptor,
        started: i64,
        code: &'static str,
    ) -> AccountFuture<'a, ()> {
        Box::pin(async move {
            let this = self.clone();
            let descriptor = descriptor.clone();
            let result = PgRequestLease::new(self.control.clone())
                .run(move |control| {
                    Box::pin(async move {
                        Self::lock_routes(control).await?;
                        let route = Arc::new(this.route(control, &descriptor.connection_id).await?);
                        route.matches(&descriptor)?;
                        let profile = this
                            .profiles
                            .retain(
                                &descriptor.config.secret_profile,
                                &descriptor.connection_id,
                                Some(&descriptor.profile_fingerprint),
                            )
                            .await?;
                        PgUnitOfWork::new(route.pool.clone())
                            .run(TransactionOptions::default(), |source| {
                                let this = this.clone();
                                let route = route.clone();
                                let descriptor = descriptor.clone();
                                Box::pin(async move {
                                    this.failed_snapshot_unit(
                                        source,
                                        &route,
                                        &descriptor,
                                        started,
                                        code,
                                    )
                                    .await
                                })
                            })
                            .await?;
                        route.require_current(control).await?;
                        drop(profile);
                        Ok(())
                    })
                })
                .await;
            finish(result)
        })
    }
}
const STORE_HIERARCHY: &str = "INSERT INTO iam_directory_hierarchies(connection_id,hierarchy_json,observed_at) VALUES($1,$2,$3) ON CONFLICT(connection_id) DO UPDATE SET hierarchy_json=excluded.hierarchy_json,observed_at=excluded.observed_at";
const OBSERVE_GROUPS: &str = "INSERT INTO iam_groups(connection_id,external_id,name,observed_at) SELECT $1,x.\"externalId\",x.name,$2 FROM jsonb_to_recordset($3) AS x(\"externalId\" text,name text) ON CONFLICT(connection_id,external_id) DO UPDATE SET name=excluded.name,observed_at=excluded.observed_at";
const OBSERVE_IDENTITIES: &str = "INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,email,enabled,groups_json,observed_at) SELECT x.id,$1,$2,x.\"externalId\",x.username,x.name,x.email,x.enabled,x.\"groupIds\",$3 FROM jsonb_to_recordset($4) AS x(id text,\"externalId\" text,username text,name text,email text,enabled boolean,\"groupIds\" jsonb) ON CONFLICT(connection_id,external_id) DO UPDATE SET username=excluded.username,name=excluded.name,email=excluded.email,enabled=excluded.enabled,groups_json=excluded.groups_json,observed_at=excluded.observed_at WHERE iam_identities.company_id=excluded.company_id";
const DISABLE_MISSING_IDENTITIES: &str = "UPDATE iam_identities SET enabled=false,groups_json='[]',observed_at=$1 WHERE connection_id=$2 AND company_id=$3 AND NOT(external_id=ANY($4))";
const RECORD_SYNC_AUDIT: &str = "INSERT INTO iam_audit(id,company_id,actor_id,assignment_id,action,target_id,evidence,created_at) VALUES($1,$2,'directory-sync','system','directory.synchronized',$3,$4,$5)";
const RECORD_FAILED_SYNC: &str = "UPDATE iam_connections SET health='error',last_error=$1 WHERE id=$2 AND version=$3 AND COALESCE(last_sync_at,0)<=$4";
const INVALIDATE_FAILED_FRESHNESS: &str = "UPDATE iam_identities SET observed_at=LEAST(observed_at,$1) WHERE connection_id=$2 AND company_id=$3";
