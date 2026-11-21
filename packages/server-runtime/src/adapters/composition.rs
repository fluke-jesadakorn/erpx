use crate::application::inspection::OwnerCompositionCoverage;
use axum::Router;
pub(crate) struct BusinessComposition {
    pub router: Router,
    pub coverage: Vec<OwnerCompositionCoverage>,
}
/// Coverage is assembled together with the actual business routes and workers.
/// No business owner is mounted by this foundation. Metadata cannot add one.
pub(crate) fn compose_business_owners() -> BusinessComposition {
    BusinessComposition {
        router: Router::new(),
        coverage: Vec::new(),
    }
}
