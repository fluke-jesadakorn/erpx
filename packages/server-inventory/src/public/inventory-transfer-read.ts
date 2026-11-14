/**
 * Current warehouse-transfer read port.
 *
 * Commands are native (`packages/server-inventory/src/public/transfers.rs`).
 * This entry publishes only the current Transfer reads and pure predicates so
 * Workflow flow access and Work projections never import the retired command
 * barrel.
 */
export {transferSchema,transferActionSchema,canPrepareTransfers,canEditTransfer,canReviewTransfer,listTransfers,transfersPage,getTransfer,transferOptions,transferDetail} from "@erpx/server-inventory/adapters/inventory-transfers";
export type {TransferRow} from "@erpx/server-inventory/domain/transfers";
