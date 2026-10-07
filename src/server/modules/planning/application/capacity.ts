import { consolidateAllocations, defaultCapacity, nextCapacityDate, type CapacityOperation, type PlanningContext, type ResourceAllocation } from "../domain/capacity";

export type CapacityInput = { specificationId: string; branchId: string; factor: number; startDate: string; dueDate: string };
export type CapacityRepository = {
  operations(specificationId: string, companyId: string): Promise<CapacityOperation[]>;
  calendar(resourceId: string, date: string): Promise<{ available_minutes: number; version: number } | null>;
  allocatedMinutes(resourceId: string, date: string): Promise<number>;
};
export function createCapacityPlanner(ports: { repository: CapacityRepository; error(status: number, code: string, message: string): Error }) {
  return async (companyId: string, input: CapacityInput) => {
    const operations = await ports.repository.operations(input.specificationId, companyId);
    if (!operations.length) return null;
    let cursor = input.startDate, requiredMinutes = 0;
    const allocations: ResourceAllocation[] = [], sourceOperations: NonNullable<PlanningContext["operations"]> = [];
    for (const operation of operations) {
      if (operation.status !== "active" || operation.branch_id !== input.branchId)
        throw ports.error(409, "PLANNING_RESOURCE_CHANGED", "An approved routing resource is inactive or belongs to another branch.");
      let remaining = Number(operation.setup_minutes) + Math.ceil(Number(operation.run_minutes_per_batch) * input.factor);
      requiredMinutes += remaining;
      sourceOperations.push({ id: String(operation.id), name: String(operation.name), resourceId: String(operation.resource_id), requiredMinutes: remaining });
      const weekdays = JSON.parse(String(operation.work_weekdays_json)) as number[];
      for (let day = 0; remaining > 0; day++) {
        if (day > 366) throw ports.error(422, "PLANNING_CAPACITY_UNAVAILABLE", "The routing cannot be scheduled within a year using the configured resource calendar.");
        const resourceId = String(operation.resource_id), calendar = await ports.repository.calendar(resourceId, cursor), existing = await ports.repository.allocatedMinutes(resourceId, cursor),
          available = calendar ? Number(calendar.available_minutes) : defaultCapacity(cursor, weekdays, Number(operation.daily_minutes)),
          used = allocations.filter(x => x.resourceId === operation.resource_id && x.workDate === cursor).reduce((n, x) => n + x.minutes, 0),
          minutes = Math.min(remaining, Math.max(0, available - existing - used));
        if (minutes > 0) allocations.push({ resourceId, workDate: cursor, minutes, resourceVersion: Number(operation.resource_version), calendarVersion: Number(calendar?.version ?? 0), availableMinutes: available });
        remaining -= minutes;
        if (remaining > 0) cursor = nextCapacityDate(cursor);
      }
    }
    const consolidated = consolidateAllocations(allocations);
    return { requiredMinutes, feasibleDate: cursor, capacityMinutes: consolidated.filter(x => x.workDate <= input.dueDate).reduce((n, x) => n + x.minutes, 0), allocations: consolidated, operations: sourceOperations };
  };
}
