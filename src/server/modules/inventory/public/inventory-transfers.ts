/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { transferSchema, transferActionSchema, canPrepareTransfers, canEditTransfer, canReviewTransfer, listTransfers, transfersPage, getTransfer, transferOptions, transferDetail, saveTransfer, transitionTransfer } from "@/server/modules/inventory/adapters/inventory-transfers";
export type { TransferRow } from "@/server/modules/inventory/domain/transfers";
