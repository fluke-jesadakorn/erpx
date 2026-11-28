//! Native Procurement supplier-bill detail read and its owner port.
pub use crate::adapters::supplier_bill_detail_read::PgSupplierBillDetailRead;
pub use crate::application::supplier_bill_detail::{
    SupplierBillDetailFuture, SupplierBillDetailRepository, supplier_bill_detail,
};
pub use crate::domain::supplier_bill_detail::{
    Row, SupplierBillDetail, SupplierBillDetailActor,
};
