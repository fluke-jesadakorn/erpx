//! Target-neutral Operations Console vertical slice.

pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;

pub use gateway::{OperationsGateway, OperationsProtoTransport, present_mutation, present_query};
pub use models::{
    OperationsExceptionItem, OperationsExceptionItems, OperationsSourceCountItem,
    OperationsSummaryItem, WorkQueuePolicyItem, WorkQueuePolicyItems,
};
pub use presenter::{OperationsContent, OperationsPresenter};
pub use view::{OperationsRenderedState, render_operations_state};
erpx_ui_core::declare_feature_registration!(
    OperationsFeatureSurface,
    "operations",
    "operations.workspace",
    "/workspaces/operations",
    "operations",
    "erpx-feature-operations-v1",
    dioxus
);
