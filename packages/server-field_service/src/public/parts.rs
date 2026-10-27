//! Parts request/policy/preparation boundary. No current actor/source factory.
pub use crate::adapters::parts_wire::{
    FieldServiceIssuePath, FieldServiceWireFailure, FieldServiceWireIssue,
    normalize_field_service_parts,
};
pub use crate::application::parts::{
    FieldServiceLotSelection, FieldServicePartsCommand, FieldServicePartsInput,
    FieldServicePartsPreparationPorts, FieldServicePreparedConsumptionFacts,
    NormalizedFieldServiceParts, PreparedFieldServiceParts, prepare_new_field_service_parts,
};
pub use crate::domain::parts::{
    FieldServicePartsActionKind, FieldServicePartsAuthorityDecision,
    FieldServicePartsAuthorityFacts, parts_capabilities, require_future_reservation,
    require_parts_authority,
};
