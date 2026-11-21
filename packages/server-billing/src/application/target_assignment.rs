//! Billing target assignment/price read use case.
use crate::domain::target_assignment::{
    TargetAssignmentRead, TargetAssignmentRow, TargetPriceRow,
    read_target_assignment as map_assignment_read,
};
use erpx_server_shared::public::{BusinessError, HoldingId};
use std::future::Future;

pub trait TargetAssignmentRepository {
    type Error: From<BusinessError> + Send;

    fn assignment(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Option<TargetAssignmentRow>, Self::Error>> + Send;

    fn assigned_price(
        &mut self,
        price_id: &str,
        month: &str,
    ) -> impl Future<Output = Result<Option<TargetPriceRow>, Self::Error>> + Send;
}

pub async fn read_target_assignment<R: TargetAssignmentRepository>(
    repository: &mut R,
    holding: &HoldingId,
    month: &str,
) -> Result<TargetAssignmentRead, R::Error> {
    let assignment = repository.assignment(holding, month).await?;
    let price = match &assignment {
        Some(assignment) => {
            repository
                .assigned_price(&assignment.price_id, month)
                .await?
        }
        None => None,
    };
    map_assignment_read(holding, month, assignment, price).map_err(Into::into)
}
