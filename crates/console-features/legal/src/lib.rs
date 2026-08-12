//! Target-neutral Legal Console vertical slice.
pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;
pub use gateway::{LegalGateway, LegalProtoTransport, present_mutation, present_query};
pub use models::{
    AccessEntryItem, AccessEntryItems, ContractItem, ContractItems, ContractPartyItem,
    DocumentItem, DocumentItems, DocumentVersionItem,
};
pub use presenter::{LegalContent, LegalPresenter};
pub use view::{LegalRenderedState, render_legal_state};
erpx_ui_core::declare_feature_registration!(
    LegalFeatureSurface,
    "legal",
    "legal.workspace",
    "/workspaces/legal",
    "governance",
    "erpx-feature-legal-v1",
    dioxus
);
