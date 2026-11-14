//! Deterministic fakes and request facts for the portal-session unit rules.
use erpx_server_customer_authority::public::portal_session::{
    PortalCustomerProof, PortalCustomerRequest, PortalEnrollmentSource, PortalSessionEvidence,
    PortalSessionFuture, PortalSessionVerifier,
};
use erpx_server_shared::public::BusinessError;

#[derive(Default)]
pub struct Session {
    pub value: Option<PortalSessionEvidence>,
    pub error: Option<BusinessError>,
    pub calls: usize,
}

impl PortalSessionVerifier for Session {
    type Error = BusinessError;

    fn verify_session<'a>(
        &'a mut self,
        _: &'a PortalCustomerRequest<'_>,
        _: i64,
    ) -> PortalSessionFuture<'a, Option<PortalSessionEvidence>, Self::Error> {
        Box::pin(async move {
            self.calls += 1;
            match self.error.take() {
                Some(error) => Err(error),
                None => Ok(self.value.clone()),
            }
        })
    }
}

#[derive(Default)]
pub struct Enrollment {
    pub value: Option<PortalCustomerProof>,
    pub error: Option<BusinessError>,
    pub calls: usize,
}

impl PortalEnrollmentSource for Enrollment {
    type Error = BusinessError;

    fn enrolled_customer<'a>(
        &'a mut self,
        _: &'a PortalSessionEvidence,
        _: &'a PortalCustomerRequest<'_>,
        _: i64,
    ) -> PortalSessionFuture<'a, Option<PortalCustomerProof>, Self::Error> {
        Box::pin(async move {
            self.calls += 1;
            match self.error.take() {
                Some(error) => Err(error),
                None => Ok(self.value.clone()),
            }
        })
    }
}

pub fn request() -> PortalCustomerRequest<'static> {
    PortalCustomerRequest {
        token: "opaque-session-token",
        audience: Some("portal"),
        environment: "development",
        holding_id: "h001",
        holding_slug: "acme",
    }
}

pub fn proof(user: &str, email: &str) -> PortalCustomerProof {
    proof_with_holding(user, email, "h001")
}

pub fn proof_with_holding(user: &str, email: &str, holding: &str) -> PortalCustomerProof {
    PortalCustomerProof {
        user_id: user.into(),
        email: email.into(),
        holding_id: Some(holding.into()),
    }
}
