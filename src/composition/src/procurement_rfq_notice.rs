//! Real owner binding for the unmounted RFQ notification stage.
use erpx_server_collaboration::public::rfq_notifications::{
    RfqNoticeFacts, RfqNoticeSource, notify_rfq_staff,
};
use erpx_server_identity_access::public::staff_notifications::rfq_notification_recipients;
use erpx_server_procurement::public::rfq_notification_source::RfqNotificationSource;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;
struct Binding<'a> {
    source: &'a RfqNotificationSource,
    facts: RfqNoticeFacts,
}
impl RfqNoticeSource for Binding<'_> {
    fn facts(&self) -> &RfqNoticeFacts {
        &self.facts
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.source.require_current(c).await
    }
    async fn recipients(&self, c: &mut PgConnection) -> Result<Vec<String>, Error> {
        Ok(rfq_notification_recipients(
            c,
            &self.facts.company_id,
            &self.facts.branch_id,
            self.facts.department_id.as_deref(),
        )
        .await?)
    }
}
/// Caller must retain Identity, purchase and all owner source fences and must
/// revalidate them before committing the transaction.
pub(crate) async fn notify_rfq(
    c: &mut PgConnection,
    key: &str,
    source: &RfqNotificationSource,
) -> Result<(), Error> {
    let f = source.facts();
    let binding = Binding {
        source,
        facts: RfqNoticeFacts {
            id: f.id.clone(),
            company_id: f.company_id.clone(),
            branch_id: f.branch_id.clone(),
            department_id: f.department_id.clone(),
            version: f.version,
            updated_at: f.updated_at,
        },
    };
    notify_rfq_staff(c, key, &binding).await
}
