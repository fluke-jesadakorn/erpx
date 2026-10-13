import { expect, it } from "vitest";
import { createCapacityPlanner, type CapacityRepository } from "../../../src/server/modules/planning/application/capacity";
import type { CapacityOperation } from "../../../src/server/modules/planning/domain/capacity";

const operation = (overrides: Partial<CapacityOperation> = {}): CapacityOperation => ({ id: "cut", name: "Cut", resource_id: "machine", status: "active", branch_id: "branch", setup_minutes: 15, run_minutes_per_batch: 100, work_weekdays_json: "[1,2,3,4,5]", daily_minutes: 120, resource_version: 7, ...overrides });
const input = { specificationId: "spec", branchId: "branch", factor: 1.5, startDate: "2026-10-02", dueDate: "2026-10-04" };
function planner(operations: CapacityOperation[], overrides: Partial<CapacityRepository> = {}) {
  const repository: CapacityRepository = { operations: async () => operations, calendar: async () => null, allocatedMinutes: async () => 0, ...overrides };
  return createCapacityPlanner({ repository, error: (_status, code) => new Error(code) });
}
it("honors existing allocations, skips ordinary weekends, and measures capacity before the due date", async () => {
  const result = await planner([operation()], { allocatedMinutes: async (_resource, date) => date === "2026-10-02" ? 20 : 0 })("company", input);
  expect(result).toMatchObject({ requiredMinutes: 165, feasibleDate: "2026-10-05", capacityMinutes: 100 });
  expect(result?.allocations).toEqual([
    { resourceId: "machine", workDate: "2026-10-02", minutes: 100, resourceVersion: 7, calendarVersion: 0, availableMinutes: 120 },
    { resourceId: "machine", workDate: "2026-10-05", minutes: 65, resourceVersion: 7, calendarVersion: 0, availableMinutes: 120 },
  ]);
});
it("uses reviewed calendar overrides and consolidates resources reused by consecutive operations", async () => {
  const result = await planner([operation({ setup_minutes: 0, run_minutes_per_batch: 20 }), operation({ id: "finish", setup_minutes: 0, run_minutes_per_batch: 30 })], {
    calendar: async () => ({ available_minutes: 100, version: 4 }),
  })("company", { ...input, factor: 1, startDate: "2026-10-03" });
  expect(result?.allocations).toEqual([{ resourceId: "machine", workDate: "2026-10-03", minutes: 50, resourceVersion: 7, calendarVersion: 4, availableMinutes: 100 }]);
});
it("rejects stale resource eligibility and unavailable capacity; an unconfigured routing returns null", async () => {
  await expect(planner([operation({ status: "archived" })])("company", input)).rejects.toThrow("PLANNING_RESOURCE_CHANGED");
  await expect(planner([operation({ branch_id: "foreign" })])("company", input)).rejects.toThrow("PLANNING_RESOURCE_CHANGED");
  await expect(planner([operation({ daily_minutes: 0 })])("company", input)).rejects.toThrow("PLANNING_CAPACITY_UNAVAILABLE");
  await expect(planner([])("company", input)).resolves.toBeNull();
});
