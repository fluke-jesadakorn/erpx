//! Native POS add-on/package obligation read kinds mounted in the owner
//! statement registry. The TypeScript obligation bridge sends these typed kinds
//! with positional Company/package values; the authoritative SQL is selected
//! here. These re-exports let the integration oracle assert byte equality with
//! the runtime registry. The settlement-source exports are owner-owned SQL text
//! whose Finance/Inventory/PMS callers remain in TypeScript.
pub use crate::adapters::obligation_reads::{
    settlement_sources, sql, supported, Fragments, SettlementSources, ADDON_HOTEL,
    ADDON_RESTAURANT, KINDS, PACKAGE, PMS_PACKAGE, POS_HOTEL_CHARGE, POS_RESTAURANT,
};
