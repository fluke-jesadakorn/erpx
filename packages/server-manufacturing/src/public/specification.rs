//! Native Manufacturing specification command contract.
//!
//! Create/review/retire over the certified BOM register. Wire bodies are the
//! extracted retained command bodies (without the product route's `entity`
//! discriminator):
//!
//! ```text
//! POST /api/v1/holdings/{holding_slug}/manufacturing/specifications
//! Idempotency-Key: <key>
//! {"name","mode","outputProductId","batchQuantity","expectedYieldBps",
//!  "lines":[{"productId","quantity","substitutionGroup"?}],
//!  "operations":[{"resourceId","name","setupMinutes","runMinutesPerBatch"}]}
//!
//! POST /api/v1/holdings/{holding_slug}/manufacturing/specifications/{id}
//! Idempotency-Key: <key>
//! {"action":"approve"|"reject"|"retire","expectedVersion","reason"}
//! ```
//!
//! Both replies are `{"data":{"id","status","version"},"context":{...}}`.
//! Draft creation writes version 1; approve/reject require a draft and an
//! independent `records.review` holder; retire requires `approved`. Every
//! command also requires `records.write`, the `manufacturing` module and a
//! current assignment, and writes the command receipt, audit and outbox rows in
//! the caller's serializable Holding transaction. The retained specification
//! commands do not accept attachment-set bindings.
//!
//! The HTTP router only resolves the registered Host/Holding target, the
//! exact browser origin, the bounded JSON body and the Idempotency-Key.
//! Composition owns Account session resolution, the selected Company/branch
//! scope, purchase admission and the owner-unit transaction.
pub use crate::adapters::specification::{
    PgSpecificationCommands, SpecificationHttpBackend, SpecificationHttpCommand,
    SpecificationHttpFuture, SpecificationHttpRequest, SpecificationHttpResponse,
    SpecificationHttpTarget, specification_router,
};
pub use crate::application::specification::{
    SpecificationCommandRepository, SpecificationFuture, create_result, create_specification,
    review_result, review_specification,
};
pub use crate::domain::specification::{
    APPROVE_OPERATION, CREATE_OPERATION, CommandResult, CreateLinePlan, CreateOperationPlan,
    CreatePlan, CreateRequest, MAX_LINES, MAX_OPERATIONS, MAX_REASON, MAX_SUBSTITUTION_GROUP,
    REJECT_OPERATION, RETIRE_OPERATION, ReceiptSnapshot, ReviewAction, ReviewPlan, ReviewRequest,
    SpecificationLineInput, SpecificationOperationInput, SpecificationRow, ValidatedCreate,
    ValidatedLine, ValidatedOperation, ValidatedReview, invalid_lines, parse_quantity_milli,
    require_reviewable, specification_not_found,
};
