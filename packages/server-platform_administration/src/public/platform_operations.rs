//! Native Platform Administration control-plane reads and lifecycle commands.
pub use crate::adapters::platform_http::{
    PlatformCommandOperation, PlatformHttpBackend, PlatformHttpCommand, PlatformHttpCommandRequest,
    PlatformHttpFuture, PlatformHttpReadRequest, PlatformHttpResponse, PlatformHttpService,
    PlatformHttpTarget, platform_router,
};
pub use crate::adapters::platform_postgres::{
    PgPlatformAdministration, PlatformHoldingDatabase, platform_business_error,
};
pub use crate::application::platform_operations::{
    PlatformAdministrationStore, PlatformFuture, platform_operations_snapshot,
    platform_operations_summary, provision_holding, provisioning_snapshot, recover_dead_letter_job,
};
pub use crate::domain::platform_operations::{
    ControlServiceHealth, DEAD_LETTER_LIMIT, HOLDING_ID_MAX, HoldingJobStatistics, LAST_ERROR_MAX,
    PlatformActor, PlatformDeadLetter, PlatformHoldingIdentity, PlatformHoldingOperation,
    PlatformMigrationState, PlatformOperationTotals, PlatformOperationsSnapshot,
    PlatformOperationsSummary, PlatformProvisioningRow, PlatformProvisioningSnapshot,
    PlatformResourceSlot, ProvisionHoldingInput, ProvisionHoldingResult, ProvisioningPlan,
    ProvisioningReceipt, REASON_MAX, RecoverDeadLetterInput, RecoverDeadLetterResult, RecoveryPlan,
    RecoveryReceipt, SLUG_MAX, available_resource_slots, holding_operation_status,
    operation_totals, provisioning_digest, provisioning_result, recovery_digest,
    require_platform_admin, require_platform_read, sha256_hex, unavailable_holding_operation,
    validate_command_key, validate_display_name, validate_holding_id, validate_provisioning,
    validate_reason, validate_recovery, validate_slug,
};
