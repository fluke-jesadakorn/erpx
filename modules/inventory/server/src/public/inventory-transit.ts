/** Read/compatibility entry. Native transport commands mount through composition. */
export { getTransit, listTransit, transitDetail, prepareTransit, transitWork } from "@erpx/inventory-server/adapters/inventory-transit";
export { transitDispatchSchema, transitReceiveSchema, transitLossSchema, transitLossDecisionSchema, transitCancelSchema, transitWorkItem } from '@erpx/inventory-server/domain/transit';
