//! Bounded operational cleanup for one explicitly leased Field Company.
//! Every phase revalidates the current Holding binding and active Company.
//! Retained or bound sources and legal-hold destruction require a separate gate.
use erpx_server_files::public::field_visit_upload::{
    LocalFieldQuarantineWriter, expire_field_drafts, next_field_cleanup_candidate,
    reconcile_field_upload_object,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock};

pub struct FieldFilesCleanupWorker<C> {
    pub transactions: crate::files_system_scan::DevelopmentFilesScanTransactions<C>,
    pub writer: LocalFieldQuarantineWriter,
    pub clock: std::sync::Arc<C>,
    cursor: Option<String>,
}
impl<C: Clock + Send + Sync + 'static> FieldFilesCleanupWorker<C> {
    pub fn new(
        transactions: crate::files_system_scan::DevelopmentFilesScanTransactions<C>,
        writer: LocalFieldQuarantineWriter,
        clock: std::sync::Arc<C>,
    ) -> Self {
        Self {
            transactions,
            writer,
            clock,
            cursor: None,
        }
    }
    /// One turn touches at most 50 draft sets and one physical object. Failures
    /// advance the cursor; end-of-range wraps for retry on a later turn.
    pub async fn run_once(&mut self) -> Result<(), PgTransactionError<BusinessError>> {
        let clock = self.clock.clone();
        self.transactions
            .run_field_cleanup_phase(move |c, scope| {
                let clock = clock.clone();
                Box::pin(
                    async move { expire_field_drafts(c, &scope.company_id, clock.as_ref()).await },
                )
            })
            .await?;
        let after = self.cursor.clone();
        let now = self.clock.now().get();
        let candidate = self
            .transactions
            .run_field_cleanup_phase(move |c, scope| {
                let after = after.clone();
                Box::pin(async move {
                    next_field_cleanup_candidate(c, &scope.company_id, after.as_deref(), now).await
                })
            })
            .await?;
        if let Some(id) = candidate {
            self.cursor = Some(id.clone());
            reconcile_field_upload_object(
                &self.writer,
                &self.transactions,
                &id,
                self.clock.clone(),
            )
            .await?;
        } else {
            self.cursor = None;
        }
        Ok(())
    }
}
