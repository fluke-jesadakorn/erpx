//! Native standalone supplier-bill (payable) workflow.
pub use crate::adapters::supplier_bill::{
    SupplierBillCreateInput, SupplierBillDetail, SupplierBillEditInput, SupplierBillLine,
    SupplierBillPostInput, SupplierBillPostResult, SupplierBillResult, create_supplier_bill,
    edit_supplier_bill, lock_supplier_bill_post_sources, lock_supplier_bill_read_sources,
    lock_supplier_bill_write_sources, post_supplier_bill, read_supplier_bill,
};
