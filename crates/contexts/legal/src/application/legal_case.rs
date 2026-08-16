use crate::ports::{
    LegalCaseAuthorizationPort, LegalCaseCommand, LegalCaseReceipt, LegalCaseUnitOfWork, PortError,
};
pub struct LegalCaseCommandHandler<U, A> {
    unit: U,
    authorization: A,
}
impl<U, A> LegalCaseCommandHandler<U, A> {
    pub const fn new(unit: U, authorization: A) -> Self {
        Self {
            unit,
            authorization,
        }
    }
}
impl<U: LegalCaseUnitOfWork, A: LegalCaseAuthorizationPort> LegalCaseCommandHandler<U, A> {
    pub async fn execute(&self, command: LegalCaseCommand) -> Result<LegalCaseReceipt, PortError> {
        command.record.validate().map_err(|_| PortError::Internal)?;
        if !command.authority_scope.contains(command.record.scope()) {
            return Err(PortError::Denied);
        }
        self.authorization.authorize_legal_case(&command).await?;
        self.unit.save_legal_case(command).await
    }
}
