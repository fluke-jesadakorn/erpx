export type ResourceAllocation = { resourceId: string; workDate: string; minutes: number; resourceVersion: number; calendarVersion: number; availableMinutes: number };
export type PlanningContext = {
  demandSource?: { id: string; version: number; quantityMilli: number };
  incomingSupply?: Array<{ id: string; version: number; productId: string; quantityMilli: number; dueDate: string }>;
  stock?: Array<{ productId: string; version: number }>;
  allocations?: ResourceAllocation[];
  operations?: Array<{ id: string; name: string; resourceId: string; requiredMinutes: number }>;
  startDate?: string; capacityBasis?: string; comparisonScenarioId?: string | null;
  comparison?: { scenarioId: string; sourceVersion: number; shortageDeltaMilli: number; requiredMinutesDelta: number; feasibleDate: string; currentFeasibleDate: string };
};
export type CapacityOperation = {
  id: string; name: string; resource_id: string; status: string; branch_id: string;
  setup_minutes: number; run_minutes_per_batch: number; work_weekdays_json: string;
  daily_minutes: number; resource_version: number;
};
export function nextCapacityDate(date: string) { return new Date(Date.parse(`${date}T00:00:00Z`) + 86400000).toISOString().slice(0, 10); }
export function defaultCapacity(date: string, weekdays: number[], dailyMinutes: number) {
  return weekdays.includes(new Date(`${date}T00:00:00Z`).getUTCDay()) ? dailyMinutes : 0;
}
export function consolidateAllocations(allocations: ResourceAllocation[]) {
  const grouped = new Map<string, ResourceAllocation>();
  for (const allocation of allocations) {
    const key = `${allocation.resourceId}:${allocation.workDate}`, found = grouped.get(key);
    if (found) found.minutes += allocation.minutes; else grouped.set(key, { ...allocation });
  }
  return [...grouped.values()];
}
