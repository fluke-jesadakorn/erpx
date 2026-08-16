use crate::ports::{
    PortError, TimeRecordAuthorizationPort, TimeRecordCommand, TimeRecordReceipt,
    TimeRecordUnitOfWork,
};
pub struct TimeRecordCommandHandler<U, A> {
    unit: U,
    authorization: A,
}
impl<U, A> TimeRecordCommandHandler<U, A> {
    pub const fn new(unit: U, authorization: A) -> Self {
        Self {
            unit,
            authorization,
        }
    }
}
impl<U: TimeRecordUnitOfWork, A: TimeRecordAuthorizationPort> TimeRecordCommandHandler<U, A> {
    pub async fn execute(
        &self,
        command: TimeRecordCommand,
    ) -> Result<TimeRecordReceipt, PortError> {
        command.record.validate().map_err(|_| PortError::Internal)?;
        if !command.authority_scope.contains(command.record.scope()) {
            return Err(PortError::Denied);
        }
        self.authorization.authorize_time_record(&command).await?;
        self.unit.save_time_record(command).await
    }
}
