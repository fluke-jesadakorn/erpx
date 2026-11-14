//! Native released Benefits payroll-input read use case for People payroll.
//!
//! SQL stays on the caller's retained serializable Holding transaction. This
//! owner read never opens a transaction, never authorizes the People payroll
//! command and never writes a payroll row; it rechecks the current Company
//! scope, engagement/currency/period selection and every released-input shape.
use crate::application::workspace::BenefitsFuture;
use crate::domain::payroll_source::{
    BenefitPayrollInputRow, BenefitsPayrollQuery, BenefitsPayrollScope, PAYROLL_SOURCE_LIMIT,
    released_payroll_inputs,
};
use erpx_server_shared::public::BusinessError;

pub trait BenefitsPayrollSourceRepository: Send {
    type Error: From<BusinessError> + Send;

    fn released_inputs<'a>(
        &'a mut self,
        scope: &'a BenefitsPayrollScope,
        query: &'a BenefitsPayrollQuery,
        limit: usize,
    ) -> BenefitsFuture<'a, Vec<BenefitPayrollInputRow>, Self::Error>;
}

pub async fn payroll_benefit_inputs<R>(
    repository: &mut R,
    scope: &BenefitsPayrollScope,
    query: &BenefitsPayrollQuery,
) -> Result<Vec<BenefitPayrollInputRow>, R::Error>
where
    R: BenefitsPayrollSourceRepository,
{
    scope.validate()?;
    query.validate()?;
    let rows = repository
        .released_inputs(scope, query, PAYROLL_SOURCE_LIMIT + 1)
        .await?;
    released_payroll_inputs(scope, query, rows).map_err(R::Error::from)
}
