use crate::ports::{
    CustomerAuthorityRecordAuthorizationPort, CustomerAuthorityRecordCommand,
    CustomerAuthorityRecordReceipt, CustomerAuthorityRecordUnitOfWork, PortError,
};

pub struct CustomerAuthorityRecordCommandHandler<U, A> {
    unit: U,
    authorization: A,
}
impl<U, A> CustomerAuthorityRecordCommandHandler<U, A> {
    pub const fn new(unit: U, authorization: A) -> Self {
        Self {
            unit,
            authorization,
        }
    }
}
impl<U: CustomerAuthorityRecordUnitOfWork, A: CustomerAuthorityRecordAuthorizationPort>
    CustomerAuthorityRecordCommandHandler<U, A>
{
    pub async fn execute(
        &self,
        command: CustomerAuthorityRecordCommand,
    ) -> Result<CustomerAuthorityRecordReceipt, PortError> {
        command.record.validate().map_err(|_| PortError::Internal)?;
        if !command.authority_scope.contains(command.record.scope()) {
            return Err(PortError::Denied);
        }
        self.authorization.authorize_record(&command).await?;
        self.unit.save_record(command).await
    }
}
