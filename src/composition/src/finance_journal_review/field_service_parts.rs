//! Minimum original financial lineage only. Finance reviewers borrow neither
//! current operational worker rights nor current Support/Agreement/Asset state.
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
use erpx_server_field_service::public::finance_lineage::{
    CurrentFieldServicePartsFinanceLineage, FieldServicePartsFinanceRequest,
    read_field_service_parts_finance_lineage,
};
use erpx_server_finance::public::journal_review::CurrentFieldServicePartsJournalTarget;
use erpx_server_inventory::public::field_service_consumption_finance::{
    CurrentFieldServiceConsumptionFinance, read_field_service_consumption_finance,
};
use erpx_server_workflow::public::record_origin::{
    PgRawRecordOrigin, RawRecordOriginRequest, read_raw_record_origin, verify_raw_record_origin,
};

pub(crate) struct FieldServicePartsSource {
    target: CurrentFieldServicePartsJournalTarget,
    consumption: CurrentFieldServiceConsumptionFinance,
    lineage: CurrentFieldServicePartsFinanceLineage,
    origin: PgRawRecordOrigin,
}
impl FieldServicePartsSource {
    #[cfg(feature = "existing-business")]
    pub(crate) fn journal_id(&self) -> &str {
        &self.target.facts().journal_id
    }
    pub(crate) async fn retain(
        c: &mut PgConnection,
        target: CurrentFieldServicePartsJournalTarget,
    ) -> Result<Self, Error> {
        let f = target.facts();
        let consumption = read_field_service_consumption_finance(
            c,
            &f.holding_id,
            &f.company_id,
            &f.consumption_id,
        )
        .await?
        .ok_or_else(unavailable)?;
        let i = consumption.facts();
        if i.journal_id != f.journal_id
            || i.currency != f.currency
            || i.value_minor.to_string() != f.debit_total
            || i.value_minor.to_string() != f.credit_total
        {
            return Err(unavailable());
        }
        let key = i
            .consumption_id
            .strip_prefix("operational-part-")
            .ok_or_else(unavailable)?
            .to_owned();
        let lineage = read_field_service_parts_finance_lineage(
            c,
            FieldServicePartsFinanceRequest {
                holding_id: i.holding_id.clone(),
                company_id: i.company_id.clone(),
                order_id: i.order_id.clone(),
                branch_id: i.branch_id.clone(),
                department_id: i.department_id.clone(),
                minimum_order_version: i.source_version,
                owner_key: key.clone(),
                consumption_id: i.consumption_id.clone(),
                journal_id: i.journal_id.clone(),
                warehouse_id: i.warehouse_id.clone(),
                product_id: i.product_id.clone(),
                quantity_milli: i.quantity_milli,
                value_minor: i.value_minor,
                creator_user_id: i.creator_user_id.clone(),
            },
        )
        .await?
        .ok_or_else(unavailable)?;
        let origin = read_raw_record_origin(
            c,
            RawRecordOriginRequest {
                command_key: key.clone(),
                operation: "field.order.issue-part".into(),
                record_id: i.order_id.as_str().into(),
                creator_user_id: i.creator_user_id.clone(),
                company_id: i.company_id.clone(),
            },
        )
        .await?
        .ok_or_else(unavailable)?;
        let source = Self {
            target,
            consumption,
            lineage,
            origin,
        };
        source.require_current(c).await?;
        Ok(source)
    }
    pub(crate) async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.target.require_current(c).await?;
        self.consumption.require_current(c).await?;
        self.lineage.require_current(c).await?;
        verify_raw_record_origin(c, &self.origin).await
    }
}
fn unavailable() -> Error {
    BusinessError::new(FailureKind::Forbidden,"FINANCE_FIELD_SERVICE_SOURCE_UNAVAILABLE","The actual immutable Service parts event, reservation, receipt, consumption or stock_cost is unavailable.").into()
}
