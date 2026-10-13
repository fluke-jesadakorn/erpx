/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { inventoryReorderPolicySchema, inventoryReorderPolicyActionSchema, listInventoryReorderPolicies, saveInventoryReorderPolicy, transitionInventoryReorderPolicy } from "@/server/modules/inventory/adapters/inventory-reorder-policies";
export type { InventoryReorderPolicyRow } from "@/server/modules/inventory/domain/reorder-policies";
