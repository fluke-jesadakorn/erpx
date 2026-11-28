//! Native Supply inventory package-obligation read. The caller owns the
//! serializable transaction and the Company/package authority; this port only
//! re-checks the owner scope and reads the current facts.
pub use crate::adapters::supply_package_obligations::{
    lock_supply_inventory_package_sources, read_supply_inventory_package_obligations,
    SupplyInventoryObligationReport,
};
pub use crate::domain::supply_package_obligations::{
    SupplyObligationFact, supply_obligation_outstanding, supply_obligation_revision,
    valid_package,
};
