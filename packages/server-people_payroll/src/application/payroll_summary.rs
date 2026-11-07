//! Native Payroll read use cases. Repository SQL stays on the caller's
//! retained serializable transaction; these functions enforce the Company
//! payroll scope and validate bounded owner rows before presentation.
use crate::domain::payroll_summary::{
    PayrollMonth, PayrollReadActor, PayrollRunRow, PayrollSummaryRow, validate_run_rows,
    validate_summary_rows,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use std::future::Future;

pub type PayrollReadFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait PayrollReadRepository: Send {
    type Error: From<BusinessError> + Send;
    fn summary<'a>(
        &'a mut self,
        company: &'a CompanyId,
        month: Option<&'a PayrollMonth>,
    ) -> PayrollReadFuture<'a, Vec<PayrollSummaryRow>, Self::Error>;
    fn runs<'a>(
        &'a mut self,
        company: &'a CompanyId,
        full: bool,
    ) -> PayrollReadFuture<'a, Vec<PayrollRunRow>, Self::Error>;
}

pub async fn payroll_summary<R: PayrollReadRepository>(
    actor: &PayrollReadActor,
    repository: &mut R,
    month: Option<&str>,
    now: TimestampMillis,
) -> Result<Vec<PayrollSummaryRow>, R::Error> {
    actor.require_summary(now)?;
    let month = month.map(PayrollMonth::parse).transpose()?;
    let rows = repository
        .summary(&actor.company_id, month.as_ref())
        .await?;
    validate_summary_rows(&actor.company_id, rows).map_err(Into::into)
}

pub async fn payroll_runs<R: PayrollReadRepository>(
    actor: &PayrollReadActor,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Vec<PayrollRunRow>, R::Error> {
    actor.require_runs(now)?;
    let rows = repository
        .runs(&actor.company_id, actor.can_read_payroll())
        .await?;
    validate_run_rows(&actor.company_id, rows).map_err(Into::into)
}
