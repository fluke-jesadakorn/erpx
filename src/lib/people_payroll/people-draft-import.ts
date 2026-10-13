import type { DraftDefinition, DraftItem } from "@/lib/organization/organization-draft";
import type { MoveTrayItem } from "@/components/people_payroll/people-move-tray-state";

/** Preserve newer draft choices and leave overflow/conflicts in the legacy tray. */
export function mergeLegacyMoveTray(definition: DraftDefinition, tray: readonly MoveTrayItem[]) {
  const items = [...definition.items];
  const importedItems: MoveTrayItem[] = [];
  const remainingItems: MoveTrayItem[] = [];
  for (const legacy of tray) {
    const existing = items.find(item => item.kind === "person" && item.id === legacy.engagementId);
    const sameDestination = existing && (legacy.positionId === null
      ? existing.parked && !existing.positionId
      : !existing.parked && existing.positionId === legacy.positionId);
    if ((existing && !sameDestination) || (!existing && items.length >= 20)) {
      remainingItems.push({ ...legacy });
      continue;
    }
    if (!existing) {
      const item: DraftItem = { kind: "person", id: legacy.engagementId, parked: legacy.positionId === null };
      if (legacy.positionId !== null) item.positionId = legacy.positionId;
      items.push(item);
    }
    importedItems.push({ ...legacy });
  }
  return {
    definition: { ...definition, items },
    importedEngagementIds: [...new Set(importedItems.map(item => item.engagementId))],
    importedItems,
    remainingItems,
  };
}
