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
erpx_ui_kit::declare_current_workspace_feature!(
    LegalFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::Legal,
    "legal",
    [("legal.workspace", "/legal", "governance")],
    "erpx-feature-legal-v2"
);
