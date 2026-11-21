use crate::domain::invitation_person_link::{
    InvitationPersonLinkRequest, InvitationPersonLinkWitness,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;

/// The caller owns authorization and the enclosing Holding transaction.
pub trait InvitationPersonLinkPort {
    type Error: From<BusinessError> + Send;
    fn link_invited_person(
        &mut self,
        request: &InvitationPersonLinkRequest,
    ) -> impl Future<Output = Result<InvitationPersonLinkWitness, Self::Error>> + Send;
}
pub async fn link_invited_person<P: InvitationPersonLinkPort>(
    port: &mut P,
    request: &InvitationPersonLinkRequest,
) -> Result<InvitationPersonLinkWitness, P::Error> {
    port.link_invited_person(request).await
}
