pub use crate::adapters::parts_wire::{NormalizedFieldServiceVisit, normalize_field_service_visit};
pub use crate::domain::visit::{VisitAction, VisitAuthorityFacts, require_visit_authority};

pub use crate::adapters::parts::visit::{PgVisitTechnician, PreparedFieldServiceVisit};

pub use crate::adapters::parts::visit::{CompletionStockGroup, PreparedCompletionReservations};
