//! Query port and handler for the native lane-32 Benefits payroll-input read.
//! No SQL is assembled here; the owner postgres adapter owns the statement.

use super::domain::{PayrollInputsRequest, PayrollInputsResponse};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type PayrollInputReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner query port. Composition injects the Pg reader; the handler only owns
/// validation, the Company-safe scope and the response projection.
pub trait PayrollInputRead: Send + Sync {
    fn released<'a>(
        &'a self,
        request: &'a PayrollInputsRequest,
    ) -> PayrollInputReadFuture<'a, Vec<super::domain::PayrollInputFact>>;
}

/// A shared owner reader can itself be injected behind the port. This lets a
/// router hold one independently backed reader per Holding without weakening
/// the trait boundary.
impl PayrollInputRead for std::sync::Arc<dyn PayrollInputRead> {
    fn released<'a>(
        &'a self,
        request: &'a PayrollInputsRequest,
    ) -> PayrollInputReadFuture<'a, Vec<super::domain::PayrollInputFact>> {
        (**self).released(request)
    }
}

/// Bounded query handler. Every read is validated before it reaches SQL, and
/// no caller can supply a statement or widen the Company scope.
pub struct ReadPayrollInputs<R: PayrollInputRead>(R);

impl<R: PayrollInputRead> ReadPayrollInputs<R> {
    pub fn new(reader: R) -> Self {
        Self(reader)
    }

    /// The injected reader, exposed so router tests can prove distinct targets
    /// hold distinct readers (and therefore distinct pools).
    pub fn reader(&self) -> &R {
        &self.0
    }

    pub async fn execute(
        &self,
        request: PayrollInputsRequest,
    ) -> Result<PayrollInputsResponse, BusinessError> {
        request.validate()?;
        let facts = self.0.released(&request).await?;
        Ok(PayrollInputsResponse::from(facts))
    }
}
