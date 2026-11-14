//! Native Manufacturing BOM/formula register read contract.
pub use crate::adapters::bom_read::PgBomRead;
pub use crate::application::bom::{BomReadFuture, BomReadRepository, bom_register};
pub use crate::domain::bom::{
    BomActor, BomLine, BomOperation, BomRegister, BomSpecification, MAX_LINES_PER_SPECIFICATION,
    MAX_OPERATIONS_PER_SPECIFICATION, MAX_SAFE_INTEGER, MAX_SPECIFICATIONS, MAX_TOTAL_LINES,
    MAX_TOTAL_OPERATIONS, validate_register,
};
