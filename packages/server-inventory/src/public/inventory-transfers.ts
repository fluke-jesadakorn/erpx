/** Compatibility entry; its legacy implementation is tracked in ownership.json. The list read is native-only. */
export { nativeTransferList as listTransfers } from "../adapters/native-transfer-list-read";
export { transferSchema, transferActionSchema, canPrepareTransfers, canEditTransfer, canReviewTransfer, transfersPage, getTransfer, transferOptions, transferDetail, saveTransfer, transitionTransfer } from "@erpx/server-inventory/adapters/inventory-transfers";
export type { TransferRow } from "@erpx/server-inventory/domain/transfers";
