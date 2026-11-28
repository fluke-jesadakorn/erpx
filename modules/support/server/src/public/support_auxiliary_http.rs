//! Native Support auxiliary-read HTTP ingress contract.
//!
//! Composition supplies the Account proof-derived actor/connection, selected
//! Support scope and purchase gates through [`SupportAuxiliaryHttpBackend`].
//! This owner router only validates registered Host/Holding targets and bounded
//! identifiers.
pub const CONTRACT_VERSION: &str = "support.auxiliary-http.v1";

pub use crate::adapters::support_http::SupportHttpFuture;

pub use crate::adapters::support_auxiliary_http::{
    SupportAuxiliaryHttpBackend, SupportAuxiliaryHttpTarget, SupportAuxiliaryReadHttpRequest,
    SupportAuxiliarySourceKind, SupportAuxiliarySourceReply, SupportAuxiliaryWorkspaceHttpRequest,
    support_auxiliary_http_router,
};
