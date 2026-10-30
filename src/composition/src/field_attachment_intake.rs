//! Trusted Files intake port backed by current Field and Account owner proofs.
use crate::field_attachment::{AttachmentContext, Proof, proof};
use erpx_server_commercial::public::purchases::CommercialCatalogue;
use erpx_server_files::public::field_visit_intake::{FieldIntakeAuthority, FieldIntakeScope};
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, HoldingWorkSourcePreparation,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use sqlx::PgConnection;
use std::collections::BTreeMap;
use tokio::sync::Mutex;

type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_ATTACHMENT_CHANGED",
        "The original Service visit or selected role changed before its evidence committed.",
    )
}

/// Prepare locks before the Account coordinator opens its first source snapshot.
#[derive(Clone)]
pub struct FieldAttachmentIntakeSources;
impl HoldingWorkSourcePreparation for FieldAttachmentIntakeSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        crate::field_attachment::lock_attachment_sources(c, false)
    }
}

/// Compiled composition authority, never constructed from HTTP facts or plugins.
/// Files also binds its opaque guard to the current serializable database unit.
pub struct FieldAttachmentIntakeAuthority<'a, C> {
    context: AttachmentContext<'a, C>,
    proofs: Mutex<BTreeMap<(String, bool), Proof>>,
}
impl<'a, C: Clock + Sync> FieldAttachmentIntakeAuthority<'a, C> {
    pub fn new(
        actor: &'a AuthorizedWorkContext,
        holding: &'a HoldingId,
        company: &'a CompanyId,
        catalogue: &'a CommercialCatalogue,
        clock: &'a C,
    ) -> Self {
        Self {
            context: AttachmentContext {
                actor,
                holding,
                company,
                catalogue,
                clock,
            },
            proofs: Mutex::new(BTreeMap::new()),
        }
    }
    fn actor(&self, actor: &AuthorizedWorkContext, write: bool) -> Result<(), BusinessError> {
        if !std::ptr::eq(actor, self.context.actor)
            || actor.holding_id() != self.context.holding
            || &actor.selection().workspace().company_id != self.context.company
            || self.context.clock.now().get() >= actor.expires_at()
        {
            return Err(changed());
        }
        actor.require_permission("records.read")?;
        if write {
            actor.require_permission("records.write")?;
        }
        Ok(())
    }
}
impl<C: Clock + Sync> FieldIntakeAuthority for FieldAttachmentIntakeAuthority<'_, C> {
    fn authorize<'a>(
        &'a self,
        c: &'a mut PgConnection,
        actor: &'a AuthorizedWorkContext,
        target_id: &'a str,
        write: bool,
    ) -> PgWorkFuture<'a, FieldIntakeScope, BusinessError> {
        Box::pin(async move {
            self.actor(actor, write)?;
            let source = proof(c, &self.context, target_id, write).await?;
            source
                .current(
                    c,
                    actor,
                    self.context.holding,
                    self.context.company,
                    self.context.catalogue,
                    self.context.clock,
                )
                .await?;
            let facts = source.field.facts();
            let scope = FieldIntakeScope {
                company_id: self.context.company.clone(),
                branch_id: Some(facts.branch_id.clone()),
                department_id: facts.department_id.clone(),
                version: facts.version,
            };
            let mut proofs = self.proofs.lock().await;
            // An authority instance retains each original proof; re-preparing a
            // source must not silently replace a guard's captured witness.
            let key = (target_id.to_owned(), write);
            if proofs.contains_key(&key) {
                return Err(changed().into());
            }
            proofs.insert(key, source);
            Ok(scope)
        })
    }
    fn revalidate<'a>(
        &'a self,
        c: &'a mut PgConnection,
        actor: &'a AuthorizedWorkContext,
        target_id: &'a str,
        write: bool,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.actor(actor, write)?;
            let proofs = self.proofs.lock().await;
            let source = proofs
                .get(&(target_id.to_owned(), write))
                .ok_or_else(changed)?;
            source
                .current(
                    c,
                    actor,
                    self.context.holding,
                    self.context.company,
                    self.context.catalogue,
                    self.context.clock,
                )
                .await?;
            // Bind evidence intentionally permits a Field version increment;
            // intake itself cannot advance or ignore that original version.
            let fresh = erpx_server_field_service::public::file_source::read_field_file_source(
                c, actor, target_id,
            )
            .await?;
            if fresh.facts() != source.field.facts() {
                return Err(Error::Business(changed()));
            }
            Ok(())
        })
    }
}
