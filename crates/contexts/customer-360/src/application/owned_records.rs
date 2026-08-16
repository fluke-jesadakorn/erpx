use crate::ports::{
    Customer360RecordAuthorizationPort, Customer360RecordCommand, Customer360RecordReceipt,
    Customer360RecordUnitOfWork, PortError,
};
pub struct Customer360RecordCommandHandler<U, A> {
    unit: U,
    authorization: A,
}
impl<U, A> Customer360RecordCommandHandler<U, A> {
    pub const fn new(unit: U, authorization: A) -> Self {
        Self {
            unit,
            authorization,
        }
    }
}
impl<U: Customer360RecordUnitOfWork, A: Customer360RecordAuthorizationPort>
    Customer360RecordCommandHandler<U, A>
{
    pub async fn execute(
        &self,
        command: Customer360RecordCommand,
    ) -> Result<Customer360RecordReceipt, PortError> {
        command.record.validate().map_err(|_| PortError::Internal)?;
        if !command.authority_scope.contains(command.record.scope()) {
            return Err(PortError::Denied);
        }
        self.authorization.authorize_record(&command).await?;
        self.unit.save_record(command).await
    }
}
