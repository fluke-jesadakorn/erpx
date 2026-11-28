//! Public surface for the native R3 supplier-bill workspace read.
//!
//! Composition mounts the router and supplies the backend that resolves the
//! Account session, Company scope, purchased-package admission and the
//! caller-owned Holding unit. Consumers never import adapter internals.
pub use crate::adapters::native_r3_supplier_bill_workspace_http::{
    supplier_bill_workspace_router, SupplierBillWorkspaceHttpBackend,
    SupplierBillWorkspaceHttpFuture, SupplierBillWorkspaceHttpRequest,
    SupplierBillWorkspaceHttpResponse, SupplierBillWorkspaceHttpTarget,
};
pub use crate::adapters::native_r3_supplier_bill_workspace_read::{
    supplier_bill_workspace_in_unit, PgSupplierBillWorkspaceRead,
};
pub use crate::application::native_r3_supplier_bill_workspace::{
    supplier_bill_workspace, WorkspaceFuture, WorkspaceRepository,
};
pub use crate::domain::native_r3_supplier_bill_workspace::{
    validate_rows, Row, WorkspaceActor,
};
