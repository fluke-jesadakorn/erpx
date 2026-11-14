//! Native Handbook HTTP owner port. Composition supplies session and scope.
pub const CONTRACT_VERSION: &str = "collaboration.handbook.http.v1";

pub use crate::adapters::handbook_http::{
    HandbookDetailData, HandbookEditorsData, HandbookHttpBackend, HandbookHttpCommandRequest,
    HandbookHttpFuture, HandbookHttpReadReply, HandbookHttpReadRequest, HandbookHttpTarget,
    HandbookListData, HandbookReadKind, handbook_router,
};
