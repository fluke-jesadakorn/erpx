use crate::ports::{
    PlatformCaseAuthorizationPort, PlatformCaseCommand, PlatformCaseReceipt,
    PlatformCaseUnitOfWork, PortError,
};
pub struct PlatformCaseCommandHandler<U, A> {
    unit: U,
    authorization: A,
}
impl<U, A> PlatformCaseCommandHandler<U, A> {
    pub const fn new(unit: U, authorization: A) -> Self {
        Self {
            unit,
            authorization,
        }
    }
}
impl<U: PlatformCaseUnitOfWork, A: PlatformCaseAuthorizationPort> PlatformCaseCommandHandler<U, A> {
    pub async fn execute(
        &self,
        command: PlatformCaseCommand,
    ) -> Result<PlatformCaseReceipt, PortError> {
        command.record.validate().map_err(|_| PortError::Internal)?;
        if !command.authority_scope.contains(command.record.scope()) {
            return Err(PortError::Denied);
        }
        self.authorization.authorize_platform_case(&command).await?;
        self.unit.save_platform_case(command).await
    }
}
