//! Input and planned Inventory-only effects for a Service Count revision.
//! The composition must authorize the selected actor and retained owners in the
//! same Holding transaction; these values are not authority.
use crate::application::service_initial_physical_count::InitialCountProductSnapshot;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialCountRevisionAllocation {
    pub lot_id: Option<String>,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub manufactured_date: Option<String>,
    pub expiry_date: Option<String>,
    pub quantity_milli: i64,
    pub unit_cost_minor: Option<i64>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialCountRevision {
    pub company_id: String,
    pub branch_id: String,
    pub count_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub expected_version: i64,
    pub count_date: String,
    pub reason: String,
    pub actor_id: String,
    pub now: i64,
    pub product: InitialCountProductSnapshot,
    pub quantity_milli: i64,
    pub unit_cost_minor: Option<i64>,
    pub allocations: Vec<InitialCountRevisionAllocation>,
}
