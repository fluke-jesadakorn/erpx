/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { countSchema, countActionSchema, canReadCounts, canPrepareCounts, canApproveCounts, canEditCount, canReviewCount, listCounts, countsPage, getCount, countDetail, countOptions, saveCount, transitionCount } from "@/server/modules/inventory/adapters/inventory-counts";
export type { CountRow } from "@/server/modules/inventory/domain/counts";
