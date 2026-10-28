//! Parts request/policy/preparation boundary. No current actor/source factory.
pub use crate::adapters::parts_wire::{
    FieldServiceIssuePath, FieldServiceWireFailure, FieldServiceWireIssue,
    field_service_parts_details_json, field_service_parts_receipt_json,
    normalize_field_service_parts,
};
pub use crate::application::parts::{
    EncodedFieldServiceInput, FieldServiceLotSelection, FieldServicePartsCommand,
    FieldServicePartsDetails, FieldServicePartsInput, FieldServicePartsPreparationPorts,
    FieldServicePreparedConsumptionFacts, NormalizedFieldServiceParts, PreparedFieldServiceParts,
    prepare_new_field_service_parts,
};
pub use crate::domain::parts::{
    FieldServicePartsActionKind, FieldServicePartsAuthorityDecision,
    FieldServicePartsAuthorityFacts, parts_capabilities, require_future_reservation,
    require_parts_authority,
};

pub use crate::adapters::http_parts::field_service_first_wire_message;
pub use crate::application::http_parts::{
    FieldServicePhysicalOrderId, FieldServiceRawOrderTarget, PendingFieldServiceParts,
};
