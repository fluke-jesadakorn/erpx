import { describe, expect, it } from "vitest";
import { mergeLegacyMoveTray } from "../../../src/lib/people_payroll/people-draft-import";
import { emptyDraft } from "../../../src/lib/organization/organization-draft";

describe("Legacy People move tray import", () => {
  it("keeps undecided moves parked and preserves the original definition", () => {
    const definition = { ...emptyDraft(), reason: "Reorganize operations", managers: { existing: "manager" }, leave: { existing: true } };
    const result = mergeLegacyMoveTray(definition, [{ engagementId: "pending", positionId: null }, { engagementId: "chosen", positionId: "destination" }]);
    expect(result.definition.items).toEqual([
      { kind: "person", id: "pending", parked: true },
      { kind: "person", id: "chosen", parked: false, positionId: "destination" },
    ]);
    expect(result.definition.reason).toBe(definition.reason);
    expect(result.definition.managers).toEqual(definition.managers);
    expect(result.definition.leave).toEqual(definition.leave);
    expect(definition.items).toEqual([]);
    expect(result.importedEngagementIds).toEqual(["pending", "chosen"]);
    expect(result.remainingItems).toEqual([]);
  });

  it("never overwrites a draft choice and recognizes moves already saved", () => {
    const definition = { ...emptyDraft(), items: [
      { kind: "person" as const, id: "changed", parked: false, positionId: "newer" },
      { kind: "person" as const, id: "same", parked: false, positionId: "destination", managerPersonId: "manager" },
      { kind: "person" as const, id: "pending", parked: true },
    ] };
    const conflict = { engagementId: "changed", positionId: "older" };
    const result = mergeLegacyMoveTray(definition, [conflict, { engagementId: "same", positionId: "destination" }, { engagementId: "pending", positionId: null }]);
    expect(result.definition.items).toEqual(definition.items);
    expect(result.importedEngagementIds).toEqual(["same", "pending"]);
    expect(result.remainingItems).toEqual([conflict]);
    expect(result.importedItems).toEqual([{ engagementId: "same", positionId: "destination" }, { engagementId: "pending", positionId: null }]);
  });

  it("retains overflow for a later import and counts all kinds toward the limit", () => {
    const definition = { ...emptyDraft(), items: Array.from({ length: 19 }, (_, index) => ({ kind: "position" as const, id: `position-${index}`, parked: true })) };
    const result = mergeLegacyMoveTray(definition, [{ engagementId: "first", positionId: "destination" }, { engagementId: "overflow", positionId: null }]);
    expect(result.definition.items).toHaveLength(20);
    expect(result.importedEngagementIds).toEqual(["first"]);
    expect(result.remainingItems).toEqual([{ engagementId: "overflow", positionId: null }]);
    const retry = mergeLegacyMoveTray(result.definition, result.importedItems);
    expect(retry.definition).toEqual(result.definition);
    expect(retry.importedEngagementIds).toEqual(["first"]);
  });
});
