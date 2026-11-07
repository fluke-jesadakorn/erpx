//! Original Field Service order binds the Collaboration writer; prepared data
//! and the actual sealed after-image use distinct source objects.
use super::{Error, changed, source::FieldServiceReferences};
use erpx_server_collaboration::public::field_service_notifications::{
    FieldServiceNotificationFacts, PgFieldServiceNotificationSource,
};
use erpx_server_field_service::public::parts_source::{
    CurrentFieldServiceOrder, PreparedFieldServiceOrderWrite, RetainedFieldServiceOrderParts,
};
use erpx_server_shared::public::TimestampMillis;
use sqlx::PgConnection;

pub(super) struct BeforeNotificationSource<'p, 'a> {
    source: &'p CurrentFieldServiceOrder<'a, FieldServiceReferences>,
    work: &'p PreparedFieldServiceOrderWrite,
    facts: FieldServiceNotificationFacts,
}
impl<'p, 'a> BeforeNotificationSource<'p, 'a> {
    pub(super) fn new(
        source: &'p CurrentFieldServiceOrder<'a, FieldServiceReferences>,
        work: &'p PreparedFieldServiceOrderWrite,
    ) -> Result<Self, Error> {
        if work.command().order_id() != &source.facts().id
            || work.technician_id() != source.facts().technician_id
        {
            return Err(changed().into());
        }
        Ok(Self {
            source,
            work,
            facts: FieldServiceNotificationFacts {
                owner_key: work.command().key().clone(),
                holding_id: source.facts().holding_id.clone(),
                company_id: source.facts().company_id.clone(),
                order_id: source.facts().id.as_str().to_owned(),
                technician_user_id: work.technician_id().to_owned(),
                order_version_after: work.version_after(),
                updated_at_after: work.application_at(),
            },
        })
    }
    pub(super) fn after<'s>(
        &'s self,
        receipt: &'s RetainedFieldServiceOrderParts,
    ) -> AfterNotificationSource<'s, 'p, 'a> {
        AfterNotificationSource {
            before: self,
            receipt,
        }
    }
}
impl PgFieldServiceNotificationSource for BeforeNotificationSource<'_, '_> {
    fn facts(&self) -> &FieldServiceNotificationFacts {
        &self.facts
    }
    async fn require_before(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.work
            .intent()
            .require_current_source(c, self.source, now)
            .await
    }
    async fn require_after(&self, _: &mut PgConnection, _: TimestampMillis) -> Result<(), Error> {
        Err(changed().into())
    }
}
pub(super) struct AfterNotificationSource<'s, 'p, 'a> {
    before: &'s BeforeNotificationSource<'p, 'a>,
    receipt: &'s RetainedFieldServiceOrderParts,
}
impl PgFieldServiceNotificationSource for AfterNotificationSource<'_, '_, '_> {
    fn facts(&self) -> &FieldServiceNotificationFacts {
        &self.before.facts
    }
    async fn require_before(&self, _: &mut PgConnection, _: TimestampMillis) -> Result<(), Error> {
        Err(changed().into())
    }
    async fn require_after(&self, c: &mut PgConnection, now: TimestampMillis) -> Result<(), Error> {
        self.receipt
            .require_plan(c, self.before.source, self.before.work, now)
            .await
    }
}
